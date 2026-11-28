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
            // Early native Settings builds saved `name` instead of React's `title`.
            if id.starts_with("custom:")
                && !(s(&p["title"]).trim().is_empty() && s(&p["name"]).trim().is_empty())
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
/// A run may freeze routing, while credentials and provider configuration remain
/// owned by the canonical configuration. No other configuration is accepted here.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RunSelection {
    provider: String,
    model: String,
}
impl RunSelection {
    pub(super) fn parse(value: Option<&Value>) -> Result<Option<Self>, String> {
        let Some(value) = value else { return Ok(None) };
        let selection: Self = serde_json::from_value(value.clone()).map_err(|_| {
            "Invalid agent runSelection: provider and model are required".to_string()
        })?;
        if [&selection.provider, &selection.model].iter().any(|s| {
            s.trim().is_empty()
                || s.len() > 512
                || s.trim() != s.as_str()
                || s.chars().any(char::is_control)
        }) {
            return Err(
                "Invalid agent runSelection: provider and model must be nonempty identifiers"
                    .into(),
            );
        }
        Ok(Some(selection))
    }
}
pub(super) fn for_run(raw: &Value, selection: Option<&RunSelection>) -> Result<Value, String> {
    if let Some(selection) = selection {
        if !ids(raw).contains(&selection.provider) {
            return Err("Native configuration: the selected run provider is disabled or unavailable in this folder. Choose a provider in AI settings.".into());
        }
        validate_selection(raw, Some("chat"), Some(&selection.provider))?;
        let mut cfg = selected(raw, Some("chat"), Some(&selection.provider));
        cfg["model"] = json!(selection.model);
        Ok(cfg)
    } else {
        validate_selection(raw, Some("chat"), None)?;
        Ok(selected(raw, Some("chat"), None))
    }
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
    let contributed = raw["customProviders"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|p| p["id"] == provider && p.get("_extensionId").is_some());
    let legacy = !contributed
        && records.map_or(true, |records| {
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
        // ...and kept `baseURL`/`protocol` on the definition instead of `providerConfigs`.
        let value = value.or_else(|| {
            matches!(key, "baseURL" | "openaiCompatibleBaseURL" | "protocol")
                .then(|| {
                    raw["customProviders"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|p| p["id"] == provider && p.get("_extensionId").is_none())
                        .and_then(|p| {
                            p[if key == "protocol" { "protocol" } else { "baseURL" }].as_str()
                        })
                })
                .flatten()
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
        "gemini" => s(&data["RECOMMENDED_AI_MODEL"]["gemini"]),
        "deepseek" => "deepseek-v4-flash",
        "tokenrouter" => "moonshotai/kimi-k3-free",
        _ => s(&data["RECOMMENDED_AI_MODEL"][provider]),
    };
    cfg["model"] = json!(model.unwrap_or(default_model).trim());
    normalize_models(&mut cfg);
    cfg["_explicitBaseURL"] = json!(cfg["baseURL"].as_str().is_some_and(|v| !v.is_empty()
        && !(provider == "antigravity"
            && v.trim_end_matches('/')
                == s(&data["DEFAULT_BASE_URL"][provider]).trim_end_matches('/'))));
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
                for (pattern, cap) in [
                    (
                        "(?i)vision|vl|llava|gpt-4o|gpt-[56]|gemini|claude|fable|opus|sonnet|haiku",
                        "vision",
                    ),
                    (
                        "(?i)reason|think|r1|o[0-9]|gpt-[56]|codex|sonnet|opus|fable|haiku|pro|v4|claude|deepseek",
                        "thinking",
                    ),
                ] {
                    if regex::Regex::new(pattern).unwrap().is_match(id) {
                        caps.push(json!(cap));
                    }
                }
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
    #[test]
    fn custom_providers_resolve_in_react_and_early_native_shapes() {
        let raw = json!({
            "provider": "custom:react",
            "customProviders": [
                {"id": "custom:react", "title": "React", "subtitle": "Custom OpenAI-compatible endpoint"},
                {"id": "custom:early", "name": "Early", "baseURL": "http://127.0.0.1:1234/v1", "protocol": "openai-chat"}
            ],
            "providerConfigs": {"custom:react": {"baseURL": "http://127.0.0.1:9/v1"}}
        });
        let all = ids(&raw);
        assert!(all.iter().any(|id| id == "custom:react"));
        assert!(all.iter().any(|id| id == "custom:early"));
        assert!(validate_selection(&raw, None, Some("custom:early")).is_ok());
        assert_eq!(selected(&raw, None, Some("custom:react"))["baseURL"], "http://127.0.0.1:9/v1");
        let early = selected(&raw, None, Some("custom:early"));
        assert_eq!(early["baseURL"], "http://127.0.0.1:1234/v1");
        assert_eq!(early["openaiCompatibleBaseURL"], "http://127.0.0.1:1234/v1");
        assert_eq!(early["protocol"], "openai-chat");
    }

    use super::*;
    #[test]
    fn credentials_are_not_leaked_to_other_providers() {
        let raw = json!({"provider":"openai","apiKey":"legacy","providerConfigs":{"anthropic":{"apiKey":"claude"}},"taskProviderOverrides":{"chat":"anthropic"},"providerSelectedModels":{"anthropic":"custom-model"}});
        let cfg = selected(&raw, Some("chat"), None);
        assert_eq!(cfg["apiKey"], "claude");
        assert_eq!(cfg["model"], "custom-model");
        assert!(selected(&raw, None, None).get("apiKey").is_none());
    }
    #[test]
    fn run_selection_keeps_routing_and_uses_only_its_provider_credentials() {
        let mut raw = json!({"provider":"openai","model":"new-global","apiKey":"unrelated",
            "providerConfigs":{"openai":{"apiKey":"openai-key"},"anthropic":{"apiKey":"anthropic-key","baseURL":"https://api.anthropic.com"}},
            "taskProviderOverrides":{"chat":"openai"},"taskModelOverrides":{"chat":"new-global"}});
        let selection = RunSelection::parse(Some(
            &json!({"provider":"anthropic","model":"frozen-model"}),
        ))
        .unwrap();
        let frozen = for_run(&raw, selection.as_ref()).unwrap();
        assert_eq!(frozen["provider"], "anthropic");
        assert_eq!(frozen["model"], "frozen-model");
        assert_eq!(frozen["apiKey"], "anthropic-key");
        assert_eq!(raw["taskProviderOverrides"]["chat"], "openai");
        raw["customProviders"] = json!([]);
        let unavailable =
            RunSelection::parse(Some(&json!({"provider":"custom:removed","model":"x"}))).unwrap();
        assert!(for_run(&raw, unavailable.as_ref()).is_err());
        assert_eq!(for_run(&raw, None).unwrap()["model"], "new-global");
    }
    #[test]
    fn run_selection_rejects_partial_or_arbitrary_configuration() {
        assert!(RunSelection::parse(None).unwrap().is_none());
        for bad in [
            json!(null),
            json!({}),
            json!({"provider":"ollama"}),
            json!({"provider":"ollama","model":""}),
            json!({"provider":"ollama","model":" x"}),
            json!({"provider":"ollama","model":"x","apiKey":"injected"}),
        ] {
            assert!(RunSelection::parse(Some(&bad)).is_err());
        }
    }
}
