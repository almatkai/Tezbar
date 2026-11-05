//! Resources are read through their connection, never fetched by URI. Template
//! matching only authorizes a shape; it cannot select another connection.
use super::*;
pub(super) fn catalog_hash(c: &Value) -> String {
    manifest::hash(&json!([
        c["catalog"]["resources"],
        c["catalog"]["templates"]
    ]))
}
fn template_matches(template: &str, uri: &str) -> bool {
    if template.len() > 8192 || uri.len() > 8192 {
        return false;
    }
    let mut pattern = String::from("^");
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        pattern.push_str(&regex::escape(&rest[..i]));
        let Some(end) = rest[i..].find('}') else {
            return false;
        };
        let variable = &rest[i + 1..i + end];
        if variable.starts_with('+') {
            pattern.push_str(".+")
        } else if variable.starts_with('?') || variable.starts_with('&') {
            pattern.push_str("(?:[?&][^#]+)?")
        } else {
            pattern.push_str("[^/?#]+")
        };
        rest = &rest[i + end + 1..];
    }
    pattern.push_str(&regex::escape(rest));
    pattern.push('$');
    regex::Regex::new(&pattern).is_ok_and(|r| r.is_match(uri))
}
pub(super) fn validate_prompt_arguments(prompt: &Value, args: &Value) -> Result<(), String> {
    let object = args
        .as_object()
        .ok_or("Prompt arguments must be an object")?;
    for argument in prompt["arguments"].as_array().into_iter().flatten() {
        let name = s(&argument["name"]);
        if argument["required"] == true
            && object
                .get(name)
                .and_then(Value::as_str)
                .is_none_or(|s| s.trim().is_empty())
        {
            return Err(format!("Enter {name}"));
        }
    }
    if object
        .values()
        .any(|v| v.as_str().is_none_or(|s| s.len() > 65536))
    {
        return Err("Prompt arguments must be text, up to 64 KiB each".into());
    }
    Ok(())
}
impl AiCore {
    pub(super) async fn extension_resource(
        &self,
        client: &Client,
        c: &Value,
        operation: ResourceOp,
        args: &Value,
        cancel: &Cancel,
    ) -> CoreResult {
        if matches!(operation, ResourceOp::List) {
            return Ok(
                json!({"content":[{"type":"text","text":json!({"resources":c["catalog"]["resources"],"templates":c["catalog"]["templates"]}).to_string()}]}),
            );
        }
        let uri = s(&args["uri"]);
        if uri.is_empty()
            || uri.len() > 8192
            || !(c["catalog"]["resources"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|r| r["uri"] == uri)
                || c["catalog"]["templates"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|t| template_matches(s(&t["uriTemplate"]), uri)))
        {
            return Err(
                "Choose a resource listed by this connection or matching one of its templates"
                    .into(),
            );
        }
        let response = client
            .call(
                "resources/read",
                json!({"uri":uri}),
                cancel,
                Duration::from_secs(10),
            )
            .await?;
        let contents = response["contents"]
            .as_array()
            .ok_or("Invalid resource response")?;
        let content = contents
            .iter()
            .map(|r| {
                if let Some(text) = r["text"].as_str() {
                    json!({"type":"text","text":format!("Resource {}:\n{text}",s(&r["uri"]))})
                } else if r["mimeType"]
                    .as_str()
                    .is_some_and(|m| m.starts_with("image/"))
                {
                    json!({"type":"image","mimeType":r["mimeType"],"data":r["blob"]})
                } else {
                    json!({"type":"text","text":"[Binary resource omitted]"})
                }
            })
            .collect::<Vec<_>>();
        Ok(json!({"content":content}))
    }
}
pub(super) fn draft(client: &Client, result: &Value, prompt: bool) -> CoreResult {
    let mut text = String::new();
    let mut images = vec![];
    let contents = if prompt {
        result["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|m| (Some(s(&m["role"])), m["content"].clone()))
            .collect::<Vec<_>>()
    } else {
        result["content"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| (None, c.clone()))
            .collect()
    };
    for (role, c) in contents {
        match s(&c["type"]) {
            "text" => {
                if let Some(role) = role {
                    text.push_str(&format!("{role}: "));
                }
                text.push_str(s(&c["text"]));
                text.push_str("\n\n");
            }
            "image" => {
                if images.len() >= 4 {
                    return Err("A draft can include at most four images".into());
                }
                images.extend(legacy::normalize_images(&[
                    json!({"mimeType":c["mimeType"],"data":c["data"]}),
                ])?);
            }
            "resource" => text.push_str(
                c["resource"]["text"]
                    .as_str()
                    .unwrap_or("[Binary resource omitted]"),
            ),
            "resource_link" => text.push_str(&format!("{}: {}", s(&c["name"]), s(&c["uri"]))),
            _ => text.push_str("[Unsupported content omitted]"),
        }
        if text.len() > 65536 {
            return Err("Draft exceeds 64 KiB".into());
        }
    }
    Ok(json!({"text":client.scrub(text.trim()),"images":images}))
}
