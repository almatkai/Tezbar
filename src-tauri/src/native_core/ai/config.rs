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
    if !cfg["baseURL"].is_string() {
        cfg["baseURL"] = data["DEFAULT_BASE_URL"][provider].clone();
    }
    if provider == "tokenrouter" && s(&cfg["apiKey"]).trim().is_empty() {
        cfg["apiKey"] = json!(std::env::var("TOKENROUTER_API_KEY").unwrap_or_default());
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

pub fn pi_bridge(cfg: &Value) -> (String, Option<String>, bool) {
    let p = s(&cfg["provider"]);
    let model = s(&cfg["model"]);
    let id = model.trim_start_matches(&format!("{p}/"));
    if p == "antigravity" {
        return (format!("antigravity/{id}"), None, true);
    }
    if p == "opencode" {
        return (
            if model.starts_with("opencode/opencode/") {
                model.to_string()
            } else if model.starts_with("opencode/") {
                format!("opencode/{model}")
            } else {
                format!("opencode/opencode/{model}")
            },
            None,
            false,
        );
    }
    let catalogs = defaults();
    let list = cfg["providerModels"][p]
        .as_array()
        .or_else(|| catalogs["DEFAULT_PROVIDER_MODELS"][p].as_array());
    let vision = list
        .and_then(|list| {
            list.iter()
                .find(|m| s(&m["id"]).trim_start_matches(&format!("{p}/")) == id)
        })
        .and_then(|m| m["capabilities"].as_array())
        .is_some_and(|c| c.iter().any(|c| c == "vision"));
    let mut base = cfg["openaiCompatibleBaseURL"]
        .as_str()
        .filter(|b| !b.is_empty())
        .unwrap_or(s(&cfg["baseURL"]))
        .trim_end_matches('/')
        .to_string();
    if p == "ollama" && !base.ends_with("/v1") {
        base.push_str("/v1");
    }
    let deepseek = p == "deepseek"
        && (base == "https://api.deepseek.com" || base == "https://api.deepseek.com/v1");
    if deepseek {
        base = "https://api.deepseek.com/anthropic".into();
    }
    let key = if p == "ollama" {
        "ollama"
    } else if p == "gemini" {
        cfg["geminiApiKey"].as_str().unwrap_or(s(&cfg["apiKey"]))
    } else if (p.starts_with("custom:") || p == "openai-compatible") && s(&cfg["apiKey"]).is_empty()
    {
        "tezbar-local"
    } else {
        s(&cfg["apiKey"])
    };
    if base.is_empty() || key.is_empty() {
        let pattern =
            if model.starts_with(&format!("{p}/")) || model.contains('/') && p != "tokenrouter" {
                model.to_string()
            } else {
                format!("{p}/{model}")
            };
        return (pattern, None, vision);
    }
    let mut m = json!({"id":id,"name":format!("Tezbar {p} {id}"),"reasoning":regex::Regex::new("(?i)reason|think|r1|o[0-9]|gpt-5|claude|deepseek").unwrap().is_match(id),"input":if vision{json!(["text","image"])}else{json!(["text"])},"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":128000,"maxTokens":8192});
    if deepseek {
        m["compat"] = json!({"supportsEagerToolInputStreaming":false,"supportsLongCacheRetention":false,"supportsCacheControlOnTools":false,"allowEmptySignature":true});
    } else if p == "deepseek" {
        m["compat"] = json!({"supportsStore":false,"supportsDeveloperRole":false,"requiresReasoningContentOnAssistantMessages":true,"thinkingFormat":"deepseek"});
    } else if p == "tokenrouter" || p == "openai-compatible" || p.starts_with("custom:") {
        m["compat"] = json!({"supportsStore":false,"supportsDeveloperRole":false,"supportsReasoningEffort":false,"maxTokensField":"max_tokens"});
    }
    (format!("tezbar/{id}"),Some(json!({"baseUrl":base,"apiKey":key,"api":if p=="anthropic"||deepseek{"anthropic-messages"}else{"openai-completions"},"authHeader":true,"models":[m]}).to_string()),vision)
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
    #[test]
    fn legacy_defaults_and_tokenrouter_slashes() {
        assert_eq!(selected(&json!({}), None, None)["model"], "llama3.2");
        let cfg = selected(
            &json!({"provider":"tokenrouter","apiKey":"key"}),
            None,
            None,
        );
        assert_eq!(pi_bridge(&cfg).0, "tezbar/moonshotai/kimi-k3-free");
    }
}
