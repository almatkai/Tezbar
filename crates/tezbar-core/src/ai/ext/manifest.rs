use super::super::*;
use sha2::{Digest, Sha256};
pub fn hash(v: &Value) -> String {
    // GPUI enables serde_json/preserve_order. Trust must depend on JSON values,
    // not insertion order or swap-removal of transient discovery metadata.
    let mut canonical = v.clone();
    canonical.sort_all_objects();
    format!("{:x}", Sha256::digest(canonical.to_string().as_bytes()))
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
    if s(&v["id"]).is_empty()
        || s(&v["id"]).len() > 160
        || s(&v["name"]).trim().is_empty()
        || s(&v["name"]).len() > 2000
    {
        return Err("Choose a name and an ID under 160 characters".into());
    }
    for key in ["instructions"] {
        if v[key].as_str().is_some_and(|s| s.len() > 128 * 1024) {
            return Err("Extension instructions exceed 128 KiB".into());
        }
    }
    if v.get("connections").is_none() {
        v["connections"] = json!([])
    }
    for kind in ["connections", "prompts", "commands", "agents", "providers"] {
        if v[kind].is_null() {
            v[kind] = json!([]);
        }
        if v.get(kind).is_some_and(|value| !value.is_array()) {
            return Err(format!("{kind} must be an array"));
        }
    }
    let mut ids = std::collections::HashSet::new();
    for c in v["connections"].as_array().into_iter().flatten() {
        validate_connection(c)?;
        if !ids.insert(s(&c["id"]).to_string()) {
            return Err("Connection names must be unique".into());
        }
    }
    for kind in ["prompts", "commands", "agents"] {
        let mut names = std::collections::HashSet::new();
        for item in v[kind].as_array().into_iter().flatten() {
            let name = s(&item[if kind == "agents" { "id" } else { "name" }]);
            if name.is_empty() || name.len() > 128 || !names.insert(name.to_string()) {
                return Err(format!("{kind} need unique names under 128 characters"));
            }
            if item["template"].as_str().is_some_and(|s| s.len() > 65536)
                || item["instructions"]
                    .as_str()
                    .is_some_and(|s| s.len() > 128 * 1024)
            {
                return Err("Prompt or profile text exceeds its limit".into());
            }
            if kind == "commands"
                && !v["prompts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|p| p["name"] == item["prompt"])
            {
                return Err("Each command must refer to a prompt in this extension".into());
            }
        }
    }
    let mut providers = std::collections::HashSet::new();
    for provider in v["providers"].as_array().into_iter().flatten() {
        if !providers.insert(s(&provider["id"]).to_string()) {
            return Err("Provider IDs must be unique".into());
        }
        if !s(&provider["id"]).starts_with("custom:") || s(&provider["title"]).trim().is_empty() {
            return Err("Custom providers need an ID starting with custom: and a title".into());
        }
        if let Some(protocol) = provider["protocol"].as_str() {
            runtime::transcript::Protocol::parse(protocol)?;
        }
        let url = reqwest::Url::parse(s(&provider["baseURL"]))
            .map_err(|_| "A custom provider needs a valid web address")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.password().is_some()
            || !url.username().is_empty()
        {
            return Err("Provider addresses must use http or https without credentials".into());
        }
        if provider["models"]
            .as_array()
            .is_some_and(|models| models.len() > 512)
        {
            return Err("A provider may list at most 512 models".into());
        }
    }
    let known = [
        "manifestVersion",
        "id",
        "name",
        "version",
        "description",
        "connections",
        "instructions",
        "prompts",
        "commands",
        "agents",
        "providers",
        "importWarnings",
    ];
    let unknown = v
        .as_object()
        .unwrap()
        .keys()
        .filter(|key| !known.contains(&key.as_str()))
        .map(|k| format!("Ignored unknown field: {k}"))
        .collect::<Vec<_>>();
    let keys = v
        .as_object()
        .unwrap()
        .keys()
        .filter(|key| !known.contains(&key.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    for key in keys {
        v.as_object_mut().unwrap().remove(&key);
    }
    v["importWarnings"] = json!(unknown
        .into_iter()
        .take(32)
        .map(|s| cut(&s, 500))
        .collect::<Vec<_>>());
    for kind in ["connections", "prompts", "commands", "agents", "providers"] {
        if v.get(kind).is_some_and(|v| !v.is_array()) {
            return Err(format!("{kind} must be a list"));
        }
    }
    Ok(v)
}
pub fn validate_connection(c: &Value) -> Result<(), String> {
    if c.get("timeoutSeconds")
        .is_some_and(|v| !v.is_null() && !v.as_u64().is_some_and(|n| (1..=1800).contains(&n)))
    {
        return Err("Time limit must be between 1 and 1800 seconds.".into());
    }

    if s(&c["id"]).is_empty() || s(&c["name"]).trim().is_empty() {
        return Err("Name is required".into());
    }
    if c["cwd"]
        .as_str()
        .is_some_and(|cwd| !cwd.is_empty() && !std::path::Path::new(cwd).is_absolute())
    {
        return Err("Working folder must be an absolute path.".into());
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
            let url = reqwest::Url::parse(s(&c["url"]).trim())
                .map_err(|_| "Enter a valid web address")?;
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
        c["auth"]["type"].as_str(),
        None | Some("none" | "token" | "oauth")
    ) {
        return Err("Choose no sign-in, an access token or browser sign-in".into());
    }
    for field in ["token", "accessToken", "clientSecret", "password"] {
        if c["auth"][field].is_string() {
            return Err(
                "Put credentials in declared secret references or the access-token field".into(),
            );
        }
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
    regex::Regex::new("(?i)token|secret|password|api.?key|authorization|credential|cookie|(?:^|[-_])key(?:$|[-_])")
        .unwrap()
        .is_match(key)
        || redact_text(value) != value
}
