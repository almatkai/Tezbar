//! Bounded schemas with local references only. This module never fetches URLs.
use serde_json::Value;
const MAP_KEYWORDS: &[&str] = &[
    "properties",
    "patternProperties",
    "$defs",
    "definitions",
    "dependentSchemas",
];
const VALUE_KEYWORDS: &[&str] = &[
    "items",
    "additionalItems",
    "additionalProperties",
    "unevaluatedItems",
    "unevaluatedProperties",
    "contains",
    "propertyNames",
    "not",
    "if",
    "then",
    "else",
    "contentSchema",
];
const ARRAY_KEYWORDS: &[&str] = &["allOf", "anyOf", "oneOf", "prefixItems"];
fn inspect(v: &Value, depth: usize, count: &mut usize) -> Result<(), String> {
    *count += 1;
    if depth > 32 || *count > 2000 {
        return Err("Tool definition exceeds schema limits".into());
    }
    if let Value::Object(map) = v {
        if map
            .get("$ref")
            .is_some_and(|v| !v.as_str().is_some_and(|s| s.starts_with('#')))
        {
            return Err("Remote schema references are not supported".into());
        }
        if let Some(draft) = map.get("$schema").and_then(Value::as_str) {
            if !matches!(
                draft,
                "https://json-schema.org/draft/2020-12/schema"
                    | "http://json-schema.org/draft-07/schema#"
                    | "https://json-schema.org/draft-07/schema"
            ) {
                return Err("Unsupported schema dialect".into());
            }
        }
        for (key, value) in map {
            if MAP_KEYWORDS.contains(&key.as_str()) {
                for child in value.as_object().into_iter().flatten().map(|(_, v)| v) {
                    inspect(child, depth + 1, count)?
                }
            } else if ARRAY_KEYWORDS.contains(&key.as_str()) || key == "items" && value.is_array() {
                for child in value.as_array().into_iter().flatten() {
                    inspect(child, depth + 1, count)?
                }
            } else if VALUE_KEYWORDS.contains(&key.as_str()) {
                inspect(value, depth + 1, count)?
            }
        }
    }
    Ok(())
}
pub fn compile(schema: &Value) -> Result<jsonschema::Validator, String> {
    if schema.to_string().len() > 65536 || schema["type"] != "object" {
        return Err("A tool needs an object schema under 64 KiB".into());
    }
    inspect(schema, 0, &mut 0)?;
    let draft = if schema["$schema"]
        .as_str()
        .is_some_and(|s| s.contains("draft-07"))
    {
        jsonschema::Draft::Draft7
    } else {
        jsonschema::Draft::Draft202012
    };
    jsonschema::options()
        .with_draft(draft)
        .build(schema)
        .map_err(|_| "Tool schema did not compile".into())
}
static VALIDATORS: std::sync::LazyLock<std::sync::Arc<tokio::sync::Semaphore>> =
    std::sync::LazyLock::new(|| std::sync::Arc::new(tokio::sync::Semaphore::new(2)));
pub async fn definition(schema: Value) -> Result<(), String> {
    check(schema, None).await
}
pub async fn validate(schema: Value, args: Value) -> Result<(), String> {
    check(schema, Some(args)).await
}
async fn check(schema: Value, args: Option<Value>) -> Result<(), String> {
    tokio::time::timeout(std::time::Duration::from_millis(100), async move {
        let permit = VALIDATORS
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| "Schema validation unavailable")?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let validator = compile(&schema)?;
            if args.as_ref().is_none_or(|args| validator.is_valid(args)) {
                Ok(())
            } else {
                Err("Arguments do not match the tool definition".into())
            }
        })
        .await
        .map_err(|_| "Schema validation failed")?
    })
    .await
    .map_err(|_| "Schema validation exceeded 100 ms")?
}
pub fn for_wire(schema: &Value) -> Result<Value, String> {
    inspect(schema, 0, &mut 0)?;
    fn anchor<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
        if value["$anchor"] == name {
            return Some(value);
        }
        match value {
            Value::Object(map) => map.values().find_map(|v| anchor(v, name)),
            Value::Array(array) => array.iter().find_map(|v| anchor(v, name)),
            _ => None,
        }
    }
    fn walk(root: &Value, value: &Value, depth: usize) -> Result<Value, String> {
        if depth > 32 {
            return Err("Recursive schema cannot be sent to this provider".into());
        }
        if let Some(reference) = value["$ref"].as_str() {
            let fragment = reference
                .strip_prefix('#')
                .ok_or("Remote schema reference")?;
            let target = if fragment.is_empty() || fragment.starts_with('/') {
                root.pointer(fragment)
            } else {
                anchor(root, fragment)
            }
            .ok_or("Unresolved schema reference")?;
            let mut resolved = walk(root, target, depth + 1)?;
            if let Some(siblings) = value.as_object().filter(|o| o.len() > 1) {
                let mut siblings = siblings.clone();
                siblings.remove("$ref");
                let siblings = walk(root, &Value::Object(siblings), depth + 1)?;
                if let (Some(to), Some(from)) = (resolved.as_object_mut(), siblings.as_object()) {
                    for (key, value) in from {
                        to.insert(key.clone(), value.clone());
                    }
                }
            }
            return Ok(resolved);
        }
        let Value::Object(map) = value else {
            return Ok(value.clone());
        };
        let mut result = serde_json::Map::new();
        for (key, value) in map {
            if matches!(
                key.as_str(),
                "$schema" | "$id" | "$anchor" | "$defs" | "definitions" | "x-mcp-header"
            ) {
                continue;
            }
            let value = if MAP_KEYWORDS.contains(&key.as_str()) {
                let mut properties = serde_json::Map::new();
                for (name, child) in value.as_object().into_iter().flatten() {
                    properties.insert(name.clone(), walk(root, child, depth + 1)?);
                }
                Value::Object(properties)
            } else if ARRAY_KEYWORDS.contains(&key.as_str()) || key == "items" && value.is_array() {
                Value::Array(
                    value
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|v| walk(root, v, depth + 1))
                        .collect::<Result<Vec<_>, _>>()?,
                )
            } else if VALUE_KEYWORDS.contains(&key.as_str()) {
                walk(root, value, depth + 1)?
            } else {
                value.clone()
            };
            result.insert(key.clone(), value);
        }
        Ok(Value::Object(result))
    }
    walk(schema, schema, 0)
}
/// Header mirrors may be reached only through `properties`, never through
/// references, arrays or composition branches (MCP 2026-07-28).
pub fn mirrored_headers(
    schema: &Value,
    args: Option<&Value>,
) -> Result<Vec<(String, String)>, String> {
    fn walk(
        schema: &Value,
        args: Option<&Value>,
        reachable: bool,
        is_property: bool,
        depth: usize,
        used: &mut std::collections::HashSet<String>,
        out: &mut Vec<(String, String)>,
    ) -> Result<(), String> {
        if depth > 32 {
            return Err("Header parameter schema is too deep".into());
        }
        if let Some(name) = schema.get("x-mcp-header") {
            let name = name.as_str().ok_or("Invalid header parameter annotation")?;
            if !reachable
                || !is_property
                || name.is_empty()
                || name.len() > 64
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
                || !matches!(
                    schema["type"].as_str(),
                    Some("string" | "integer" | "boolean")
                )
                || !used.insert(name.to_ascii_lowercase())
            {
                return Err("Invalid or duplicate header parameter annotation".into());
            }
            if let Some(value) = args.filter(|v| !v.is_null()) {
                let text = match schema["type"].as_str() {
                    Some("string") => value
                        .as_str()
                        .ok_or("Header parameter must be text")?
                        .to_string(),
                    Some("boolean") => value
                        .as_bool()
                        .ok_or("Header parameter must be boolean")?
                        .to_string(),
                    Some("integer") => {
                        let n = value
                            .as_i64()
                            .filter(|n| n.unsigned_abs() <= 9_007_199_254_740_991)
                            .ok_or("Header integer is outside the safe range")?;
                        n.to_string()
                    }
                    _ => unreachable!(),
                };
                out.push((format!("Mcp-Param-{name}"), header_value(&text)));
            }
        }
        match schema {
            Value::Object(object) => {
                for (key, value) in object {
                    if key == "properties" {
                        for (name, child) in value.as_object().into_iter().flatten() {
                            walk(
                                child,
                                args.and_then(|v| v.get(name)),
                                reachable,
                                true,
                                depth + 1,
                                used,
                                out,
                            )?
                        }
                    } else if !matches!(key.as_str(), "enum" | "const" | "default" | "examples")
                        && (value.is_object() || value.is_array())
                    {
                        walk(value, None, false, false, depth + 1, used, out)?
                    }
                }
            }
            Value::Array(array) => {
                for child in array {
                    walk(child, None, false, false, depth + 1, used, out)?
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut out = vec![];
    walk(
        schema,
        args,
        true,
        false,
        0,
        &mut std::collections::HashSet::new(),
        &mut out,
    )?;
    Ok(out)
}
pub fn header_value(text: &str) -> String {
    use base64::Engine;
    if !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
        && !text.starts_with("=?base64?")
    {
        text.into()
    } else {
        format!(
            "=?base64?{}?=",
            base64::engine::general_purpose::STANDARD.encode(text)
        )
    }
}
