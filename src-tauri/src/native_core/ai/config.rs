use super::*;
pub const PROVIDERS: &[&str] = &[
    "openai",
    "openai-compatible",
    "tokenrouter",
    "anthropic",
    "ollama",
    "copilot",
    "gemini",
    "opencode",
    "deepseek",
    "antigravity",
];
pub fn defaults() -> Value {
    serde_json::from_str(include_str!("provider_defaults.json")).expect("static provider catalog")
}
pub fn ids(raw: &Value) -> Vec<String> {
    let mut ids: Vec<String> = PROVIDERS.iter().map(|s| s.to_string()).collect();
    if let Some(custom) = raw["customProviders"].as_array() {
        for p in custom {
            let id = s(&p["id"]).trim();
            if id.starts_with("custom:")
                && !s(&p["title"]).trim().is_empty()
                && !ids.contains(&id.to_string())
            {
                ids.push(id.to_string());
            }
        }
    }
    ids
}
pub fn validate_selection(
    raw: &Value,
    task: Option<&str>,
    explicit: Option<&str>,
) -> Result<(), String> {
    let selected = explicit
        .or_else(|| task.and_then(|t| raw["taskProviderOverrides"][t].as_str()))
        .unwrap_or(s(&raw["provider"]));
    if selected.starts_with("custom:") && !ids(raw).iter().any(|id| id == selected) {
        return Err("Native configuration: the selected custom provider is disabled or unavailable in this folder. Choose a provider in AI settings.".into());
    }
    Ok(())
}
pub fn selected(raw: &Value, task: Option<&str>, explicit: Option<&str>) -> Value {
    let all = ids(raw);
    let active = raw["provider"]
        .as_str()
        .filter(|p| all.iter().any(|id| id == p))
        .unwrap_or_else(|| {
            if !s(&raw["copilotGithubToken"]).is_empty() {
                "copilot"
            } else {
                "ollama"
            }
        });
    let provider = explicit
        .or_else(|| task.and_then(|t| raw["taskProviderOverrides"][t].as_str()))
        .unwrap_or(active);
    let mut cfg = raw.clone();
    if !cfg.is_object() {
        cfg = json!({});
    }
    cfg["provider"] = json!(provider);
    let records = raw["providerConfigs"].as_object();
    let legacy = records.map_or(true, |records| {
        !all.iter()
            .any(|id| records.get(id).is_some_and(Value::is_object))
    });
    for key in [
        "apiKey",
        "baseURL",
        "openaiCompatibleBaseURL",
        "geminiApiKey",
        "copilotGithubToken",
        "githubOAuthClientId",
        "protocol",
    ] {
        let value = raw["providerConfigs"][provider][key].as_str().or_else(|| {
            if legacy && provider == active {
                raw[key].as_str()
            } else {
                None
            }
        });
        cfg.as_object_mut().unwrap().remove(key);
        if let Some(value) = value {
            cfg[key] = json!(value);
        }
    }
    let data = defaults();
    let model = task
        .and_then(|t| raw["taskModelOverrides"][t].as_str())
        .or_else(|| raw["providerSelectedModels"][provider].as_str())
        .filter(|m| !m.trim().is_empty())
        .or_else(|| {
            if provider == active {
                raw["model"].as_str()
            } else {
                None
            }
        });
    let default_model = match provider {
        "ollama" => "llama3.2",
        "gemini" => "gemini-2.0-flash",
        "deepseek" => "deepseek-v4-flash",
        "tokenrouter" => "moonshotai/kimi-k3-free",
        _ => s(&data["RECOMMENDED_AI_MODEL"][provider]),
    };
    cfg["model"] = json!(model.unwrap_or(default_model).trim());
    normalize_models(&mut cfg);
    cfg["_explicitBaseURL"] = json!(cfg["baseURL"].as_str().is_some_and(|v| !v.is_empty()));
    if !cfg["baseURL"].is_string() {
        cfg["baseURL"] = data["DEFAULT_BASE_URL"][provider].clone();
    }
    if provider == "tokenrouter" && s(&cfg["apiKey"]).trim().is_empty() {
        #[cfg(not(test))]
        {
            cfg["apiKey"] = json!(std::env::var("TOKENROUTER_API_KEY").unwrap_or_default());
        }
    }
    cfg
}

fn normalize_models(cfg: &mut Value) {
    let Some(raw) = cfg["providerModels"].as_object().cloned() else {
        return;
    };
    let mut normalized = serde_json::Map::new();
    for provider in ids(cfg) {
        let Some(models) = raw.get(&provider).and_then(Value::as_array) else {
            continue;
        };
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for model in models {
            let id = s(&model["id"]).trim();
            if id.is_empty() || !seen.insert(id.to_string()) {
                continue;
            }
            let caps = if let Some(caps) = model["capabilities"].as_array() {
                caps.iter()
                    .filter(|c| matches!(s(c), "vision" | "thinking" | "tools"))
                    .cloned()
                    .collect::<Vec<_>>()
            } else {
                let mut caps = Vec::new();
                for(pattern,cap)in [("(?i)vision|vl|llava|gpt-4o|gpt-5|gemini|claude|fable|opus|sonnet|haiku","vision"),("(?i)reason|think|r1|o[0-9]|gpt-5|codex|sonnet|opus|fable|haiku|pro|v4|claude|deepseek","thinking")]{if regex::Regex::new(pattern).unwrap().is_match(id){caps.push(json!(cap));}}
                if !regex::Regex::new("(?i)embed|embedding|whisper|tts|audio|speech")
                    .unwrap()
                    .is_match(id)
                {
                    caps.push(json!("tools"));
                }
                caps
            };
            let mut next = json!({"id":id,"capabilities":caps});
            for key in ["protocol", "maxTokens", "supported_endpoints"] {
                if let Some(v) = model.get(key) {
                    next[key] = v.clone();
                }
            }
            if let Some(n) = model["contextWindow"].as_f64() {
                next["contextWindow"] = json!(n.max(0.0).round());
            }
            if model["hiddenFromPicker"] == true {
                next["hiddenFromPicker"] = json!(true);
            }
            if model["discovered"] == false {
                next["discovered"] = json!(false);
            }
            out.push(next);
        }
        normalized.insert(provider, json!(out));
    }
    cfg["providerModels"] = json!(normalized);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credentials_are_not_leaked_to_other_providers() {
        let raw = json!({"provider":"openai","apiKey":"legacy","providerConfigs":{"anthropic":{"apiKey":"claude"}},"taskProviderOverrides":{"chat":"anthropic"},"providerSelectedModels":{"anthropic":"custom-model"}});
        let cfg = selected(&raw, Some("chat"), None);
        assert_eq!(cfg["apiKey"], "claude");
        assert_eq!(cfg["model"], "custom-model");
        assert!(selected(&raw, None, None).get("apiKey").is_none());
    }
}
