use super::super::*;
use sha2::{Digest, Sha256};
pub fn hash(v: &Value) -> String {
    format!("{:x}", Sha256::digest(v.to_string().as_bytes()))
}
pub fn parse(text: &str) -> CoreResult {
    if text.len() > 256 * 1024 {
        return Err("Configuration exceeds 256 KiB".into());
    }
    let mut v: Value = serde_json::from_str(text).map_err(|_| "Configuration isn't valid JSON")?;
    if let Some(servers) = v["mcpServers"].as_object() {
        if servers.len() > 32 {
            return Err("At most 32 connections may be imported".into());
        }
        let connections = servers
            .iter()
            .map(|(name, server)| {
                let mut c = server.clone();
                c["id"] = json!(uuid::Uuid::new_v4().to_string());
                c["name"] = json!(name);
                c["transport"] = json!(if c["url"].is_string() {
                    "http"
                } else {
                    "stdio"
                });
                c["toolPolicy"] = json!("ask");
                c
            })
            .collect::<Vec<_>>();
        v = json!({"manifestVersion":1,"id":format!("import-{}",uuid::Uuid::new_v4()),"name":"Imported connections","connections":connections});
    }
    if v["manifestVersion"] != 1 || !v["id"].is_string() || !v["name"].is_string() {
        return Err(
            "Choose a version 1 AI extension manifest or a mcpServers configuration".into(),
        );
    }
    if v["connections"].as_array().is_some_and(|a| a.len() > 32) {
        return Err("At most 32 connections may be imported".into());
    }
    let mut ids = std::collections::HashSet::new();
    for c in v["connections"].as_array().into_iter().flatten() {
        validate_connection(c)?;
        if !ids.insert(s(&c["id"]).to_string()) {
            return Err("Connection names must be unique".into());
        }
    }
    Ok(v)
}
pub fn validate_connection(c: &Value) -> Result<(), String> {
    if s(&c["id"]).is_empty() || s(&c["name"]).trim().is_empty() {
        return Err("Name is required".into());
    }
    match s(&c["transport"]) {
        "stdio" => {
            if s(&c["command"]).trim().is_empty()
                || c["command"].as_str().is_some_and(|s| s.contains('\0'))
            {
                return Err("Command is required".into());
            }
            if c["args"]
                .as_array()
                .is_some_and(|a| a.len() > 128 || a.iter().any(|v| !v.is_string()))
            {
                return Err("Use one argument per line, at most 128".into());
            }
        }
        "http" => {
            let url = reqwest::Url::parse(s(&c["url"])).map_err(|_| "Enter a valid web address")?;
            if !(url.scheme() == "https"
                || url.scheme() == "http"
                    && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
            {
                return Err("Use https, or http on localhost".into());
            }
            if !url.username().is_empty() || url.password().is_some() {
                return Err("Store credentials in the token field".into());
            }
        }
        _ => return Err("Choose a program or web connection".into()),
    }
    if !matches!(
        c["toolPolicy"].as_str(),
        None | Some("ask" | "allowReadOnly" | "allow")
    ) {
        return Err("Invalid tool policy".into());
    }
    Ok(())
}
pub fn secret_key(key: &str, value: &str) -> bool {
    regex::Regex::new("(?i)token|secret|password|api.?key|authorization|credential")
        .unwrap()
        .is_match(key)
        || redact_text(value) != value
}
