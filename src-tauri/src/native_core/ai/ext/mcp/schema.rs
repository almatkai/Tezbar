//! Bounded schemas with local references only. This module never fetches URLs.
use serde_json::Value;
fn inspect(v: &Value, depth: usize, count: &mut usize) -> Result<(), String> {
    *count += 1;
    if depth > 32 || *count > 2000 {
        return Err("Tool definition exceeds schema limits".into());
    }
    match v {
        Value::Object(m) => {
            if let Some(r) = m.get("$ref") {
                if !r.as_str().is_some_and(|s| s.starts_with('#')) {
                    return Err("Remote schema references are not supported".into());
                }
            }
            if let Some(d) = m.get("$schema").and_then(Value::as_str) {
                if !matches!(
                    d,
                    "https://json-schema.org/draft/2020-12/schema"
                        | "http://json-schema.org/draft-07/schema#"
                        | "https://json-schema.org/draft-07/schema"
                ) {
                    return Err("Unsupported schema dialect".into());
                }
            }
            for v in m.values() {
                inspect(v, depth + 1, count)?
            }
        }
        Value::Array(a) => {
            for v in a {
                inspect(v, depth + 1, count)?
            }
        }
        _ => {}
    }
    Ok(())
}
pub fn compile(schema: &Value) -> Result<jsonschema::Validator, String> {
    if schema.to_string().len() > 65536 || schema["type"] != "object" {
        return Err("A tool needs an object schema under 64 KiB".into());
    }
    inspect(schema, 0, &mut 0)?;
    jsonschema::validator_for(schema).map_err(|_| "Tool schema did not compile".into())
}
pub async fn validate(schema: Value, args: Value) -> Result<(), String> {
    tokio::time::timeout(
        std::time::Duration::from_millis(100),
        tokio::task::spawn_blocking(move || {
            let validator = compile(&schema)?;
            if validator.is_valid(&args) {
                Ok(())
            } else {
                Err("Arguments do not match the tool definition".into())
            }
        }),
    )
    .await
    .map_err(|_| "Schema validation exceeded 100 ms")?
    .map_err(|_| "Schema validation failed")?
}
pub fn for_wire(schema: &Value) -> Result<Value, String> {
    inspect(schema, 0, &mut 0)?;
    fn walk(root: &Value, v: &Value, depth: usize) -> Result<Value, String> {
        if depth > 32 {
            return Err("Recursive schema cannot be sent to this provider".into());
        }
        if let Some(r) = v.get("$ref").and_then(Value::as_str) {
            let p = r.strip_prefix('#').ok_or("Remote schema reference")?;
            let target = root.pointer(p).ok_or("Unresolved schema reference")?;
            return walk(root, target, depth + 1);
        }
        match v {
            Value::Object(o) => {
                let mut result = serde_json::Map::new();
                for (k, v) in o {
                    if !matches!(
                        k.as_str(),
                        "$schema" | "$id" | "$defs" | "definitions" | "x-mcp-header"
                    ) {
                        result.insert(k.clone(), walk(root, v, depth + 1)?);
                    }
                }
                Ok(Value::Object(result))
            }
            Value::Array(a) => a
                .iter()
                .map(|v| walk(root, v, depth + 1))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array),
            _ => Ok(v.clone()),
        }
    }
    walk(schema, schema, 0)
}
