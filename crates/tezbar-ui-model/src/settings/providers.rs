//! Presentation policy over the single core provider catalog; no copied catalog data.
use super::provider_catalog;
use serde_json::{json, Value};
use std::collections::BTreeSet;
pub fn infer_capabilities(id: &str) -> Vec<&'static str> {
    let id = id.to_lowercase();
    let mut caps = vec![];
    static PATTERNS: std::sync::OnceLock<[regex::Regex; 3]> = std::sync::OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        [
            "vision|vl|llava|gpt-4o|gpt-[56]|gemini|claude|fable|opus|sonnet|haiku",
            r"reason|think|r1|o\d|gpt-[56]|codex|sonnet|opus|fable|haiku|pro|v4|claude|deepseek",
            "embed|embedding|whisper|tts|audio|speech",
        ]
        .map(|p| regex::Regex::new(p).expect("static pattern"))
    });
    for (i, cap) in ["vision", "thinking"].into_iter().enumerate() {
        if patterns[i].is_match(&id) {
            caps.push(cap);
        }
    }
    if !patterns[2].is_match(&id) {
        caps.push("tools");
    }
    caps
}
pub fn normalize_models(models: &[Value]) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    models
        .iter()
        .filter_map(|model| {
            let id = model["id"].as_str().unwrap_or("").trim();
            if id.is_empty() || !seen.insert(id.to_owned()) {
                return None;
            }
            let caps = model["capabilities"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .filter(|s| matches!(*s, "vision" | "thinking" | "tools"))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| infer_capabilities(id));
            let mut next = json!({"id":id,"capabilities":caps});
            for field in ["protocol", "maxTokens"] {
                if let Some(v) = model.get(field) {
                    if !v.is_null() && v != false && v != 0 && v != "" {
                        next[field] = v.clone();
                    }
                }
            }
            if let Some(n) = model["contextWindow"].as_f64().filter(|n| n.is_finite()) {
                next["contextWindow"] = json!(n.round().max(0.0));
            }
            if model["hiddenFromPicker"] == true {
                next["hiddenFromPicker"] = json!(true);
            }
            if model["discovered"] == false {
                next["discovered"] = json!(false);
            }
            Some(next)
        })
        .collect()
}
pub fn configured(config: &Value, provider: &str) -> bool {
    let defaults = provider_catalog();
    let models = config["providerModels"][provider]
        .as_array()
        .or_else(|| defaults["DEFAULT_PROVIDER_MODELS"][provider].as_array());
    if provider == "ollama" {
        return models.is_some_and(|a| !a.is_empty());
    }
    let active = config["provider"] == provider;
    let record = &config["providerConfigs"][provider];
    let field = |keys: &[&str]| {
        keys.iter()
            .find_map(|k| record.get(*k).and_then(Value::as_str))
            .or_else(|| {
                if active {
                    keys.iter()
                        .find_map(|k| config.get(*k).and_then(Value::as_str))
                } else {
                    None
                }
            })
            .is_some_and(|s| !s.trim().is_empty())
    };
    match provider {
        "opencode" => {
            config["nativeProviderCredentials"][provider]["hasValue"] == true || field(&["apiKey"])
        }
        "antigravity" => config["nativeProviderCredentials"][provider]["hasValue"] == true,
        "gemini" => field(&["geminiApiKey", "apiKey"]),
        "copilot" => field(&["copilotGithubToken"]),
        p if p.starts_with("custom:") || p == "openai-compatible" => {
            field(&["openaiCompatibleBaseURL", "baseURL"])
                || field(&["apiKey"])
                || models.is_some_and(|a| !a.is_empty())
        }
        _ => field(&["apiKey"]),
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelOption {
    pub provider: String,
    pub model: String,
    pub title: String,
    pub configured: bool,
}
pub fn model_options(config: &Value, query: &str) -> Vec<ModelOption> {
    let catalog = provider_catalog();
    let mut ids = catalog["RECOMMENDED_AI_MODEL"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(id, _)| id.clone())
        .collect::<BTreeSet<_>>();
    for p in config["customProviders"].as_array().into_iter().flatten() {
        if let Some(id) = p["id"].as_str() {
            ids.insert(id.into());
        }
    }
    let q = query.trim().to_lowercase();
    let mut rows = vec![];
    for provider in ids {
        let models = config["providerModels"][&provider]
            .as_array()
            .or_else(|| catalog["DEFAULT_PROVIDER_MODELS"][&provider].as_array());
        for m in models.into_iter().flatten() {
            if m["hiddenFromPicker"] == true {
                continue;
            }
            let Some(model) = m["id"].as_str() else {
                continue;
            };
            let title = m["title"].as_str().unwrap_or(model);
            if format!("{provider} {model} {title}")
                .to_lowercase()
                .contains(&q)
            {
                rows.push(ModelOption {
                    provider: provider.clone(),
                    model: model.into(),
                    title: title.into(),
                    configured: configured(config, &provider),
                });
            }
        }
    }
    rows
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_base_url_without_key_is_configured() {
        assert!(configured(
            &json!({"providerConfigs":{"custom:model":{"openaiCompatibleBaseURL":"http://127.0.0.1:8080/v1","apiKey":""}}}),
            "custom:model"
        ));
    }
    #[test]
    fn compatible_base_url_without_key_is_configured() {
        assert!(configured(
            &json!({"providerConfigs":{"openai-compatible":{"openaiCompatibleBaseURL":"http://localhost:8080/v1","apiKey":""}}}),
            "openai-compatible"
        ));
    }
    #[test]
    fn cloud_requires_key() {
        assert!(!configured(
            &json!({"providerConfigs":{"openai":{"apiKey":""}}}),
            "openai"
        ));
    }
    #[test]
    fn native_accounts_not_inferred_from_models() {
        let mut c = json!({"providerModels":{"antigravity":[{"id":"fixture"}],"opencode":[{"id":"fixture"}]}});
        for p in ["antigravity", "opencode"] {
            assert!(!configured(&c, p));
            c["nativeProviderCredentials"][p] = json!({"hasValue":true});
            assert!(configured(&c, p));
        }
    }
    #[test]
    fn removes_legacy_context_capability_preserving_window() {
        let models = normalize_models(&[
            json!({"id":"deepseek-v4-flash","capabilities":["tools","context"],"contextWindow":128000}),
        ]);
        assert_eq!(models[0]["capabilities"], json!(["tools"]));
        assert_eq!(models[0]["contextWindow"].as_f64(), Some(128000.0));
    }
    #[test]
    fn hand_added_sibling_model_ids_are_kept() {
        let ids = ["gpt-5-mini", "auto", "claude-opus-4.5", "gpt-4.1"];
        let models = normalize_models(&ids.iter().map(|id| json!({"id":id})).collect::<Vec<_>>());
        assert_eq!(
            models
                .iter()
                .map(|v| v["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ids
        );
    }
    #[test]
    fn empty_list_does_not_resurrect_recommended_model() {
        assert!(normalize_models(&[]).is_empty());
    }
    #[test]
    fn model_ids_are_trimmed_deduped_and_blanks_dropped() {
        assert_eq!(
            normalize_models(&[
                json!({"id":"gpt-4o"}),
                json!({"id":"gpt-4o"}),
                json!({"id":"   "})
            ])
            .len(),
            1
        );
    }
    #[test]
    fn gpt_six_capabilities() {
        for id in ["gpt-6-sol", "opencode/gpt-6.1-sol"] {
            assert_eq!(infer_capabilities(id), vec!["vision", "thinking", "tools"]);
        }
    }
}
