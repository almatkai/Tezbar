//! Endpoint/protocol selection. Explicit model/provider settings take precedence.
//! OpenCode/Copilot routing metadata follows MIT pi-ai (© Mario Zechner);
//! see licenses/pi-ai-MIT.txt. Antigravity routing is attributed in its module.
use super::super::*;
use runtime::transcript::{Origin, Protocol};
pub struct Route {
    pub origin: Origin,
    pub url: reqwest::Url,
    pub headers: reqwest::header::HeaderMap,
    pub project: Option<String>,
    pub max_tokens: u64,
    pub thinking: bool,
    pub secrets: Vec<String>,
}
fn endpoint(base: &str, suffix: &str) -> String {
    if base.ends_with(suffix) {
        base.into()
    } else {
        format!("{}{}", base.trim_end_matches('/'), suffix)
    }
}
impl AiCore {
    pub(in crate::native_core::ai) async fn native_route(
        &self,
        cfg: &Value,
        messages: &[runtime::transcript::Message],
    ) -> Result<Route, String> {
        let p = s(&cfg["provider"]);
        let model = s(&cfg["model"]);
        if model.is_empty() {
            return Err("Native configuration: model is required".into());
        }
        let custom = cfg["customProviders"]
            .as_array()
            .and_then(|ps| ps.iter().find(|c| c["id"] == p));
        let catalog = config::defaults();
        let metadata = cfg["providerModels"][p]
            .as_array()
            .or_else(|| catalog["DEFAULT_PROVIDER_MODELS"][p].as_array())
            .and_then(|ms| ms.iter().find(|m| m["id"] == model));
        let mut base = cfg["openaiCompatibleBaseURL"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| cfg["baseURL"].as_str())
            .or_else(|| custom.and_then(|c| c["baseURL"].as_str()))
            .unwrap_or("")
            .trim_end_matches('/')
            .replace("://0.0.0.0", "://127.0.0.1");
        let explicit = metadata
            .and_then(|m| m["protocol"].as_str())
            .or_else(|| cfg["protocol"].as_str())
            .or_else(|| custom.and_then(|c| c["protocol"].as_str()));
        let mut project = None;
        let mut protocol = match p {
            "anthropic" => Protocol::Anthropic,
            "ollama" => Protocol::Ollama,
            "openai" if base == "https://api.openai.com/v1" => Protocol::OpenaiResponses,
            "gemini"
                if base.is_empty()
                    || base == "https://generativelanguage.googleapis.com/v1beta/openai" =>
            {
                base = "https://generativelanguage.googleapis.com/v1beta".into();
                Protocol::Gemini
            }
            "copilot" => {
                if cfg["_explicitBaseURL"] != true {
                    base = "https://api.individual.githubcopilot.com".into();
                }
                Protocol::OpenaiChat
            }
            "antigravity" => {
                base = self
                    .antigravity_bases(cfg)?
                    .into_iter()
                    .next()
                    .ok_or("No Antigravity endpoint")?;
                Protocol::Gemini
            }
            _ => Protocol::OpenaiChat,
        };
        if let Some(explicit) = explicit {
            protocol = Protocol::parse(explicit)?
        }
        let mut key = if p == "copilot" {
            self.copilot_token().await?
        } else if p == "gemini" {
            cfg["geminiApiKey"]
                .as_str()
                .unwrap_or(s(&cfg["apiKey"]))
                .into()
        } else {
            s(&cfg["apiKey"]).into()
        };
        if p == "copilot" {
            if let Some(session) = self.cached_copilot_session(&key).await {
                key = session.token;
                if cfg["_explicitBaseURL"] != true {
                    base = session.base;
                }
            }
        }
        let mut model = model.to_string();
        if p == "antigravity" {
            model = super::antigravity::runtime_model(
                &model,
                cfg["reasoningEffort"].as_str().unwrap_or("off"),
            );
            let a = self.antigravity_account().await?;
            key = s(&a["access"]).into();
            project = Some(s(&a["projectId"]).into());
        }
        #[cfg(test)]
        if let Some(url) = self.inner.endpoints.lock().unwrap().get(p) {
            base = url.clone();
        }

        if p == "gemini" && protocol == Protocol::Gemini {
            model = model.strip_prefix("models/").unwrap_or(&model).to_string();
        }
        let mut opencode_route = false;
        if p == "copilot" && explicit.is_none() {
            let models = self.copilot_models().await.unwrap_or(json!([]));
            let authoritative = models
                .as_array()
                .into_iter()
                .flatten()
                .find(|m| m["id"] == model)
                .and_then(super::copilot::model_protocol);
            protocol = authoritative.unwrap_or_else(|| {
                if model.starts_with("claude-") {
                    Protocol::Anthropic
                } else if model.starts_with("gpt-") {
                    Protocol::OpenaiResponses
                } else {
                    Protocol::OpenaiChat
                }
            });
        }
        if p == "opencode" {
            let catalog: Value = serde_json::from_str(include_str!("opencode_catalog.json"))
                .expect("static OpenCode catalog");
            let catalog_model =
                if model.starts_with("opencode/") || model.starts_with("opencode-go/") {
                    model.clone()
                } else {
                    format!("opencode/{model}")
                };
            if explicit.is_none() {
                protocol=match catalog[&catalog_model]["api"].as_str(){Some("anthropic-messages")=>Protocol::Anthropic,Some("openai-responses")=>Protocol::OpenaiResponses,Some("google-generative-ai")=>Protocol::Gemini,Some("openai-completions")=>Protocol::OpenaiChat,_=>return Err("Native configuration: choose a protocol for this model; it is absent from the OpenCode catalog".into())};
            }
            if base.is_empty() {
                base = if model.starts_with("opencode-go/") {
                    "https://opencode.ai/zen/go"
                } else {
                    "https://opencode.ai/inference"
                }
                .into();
            }
            opencode_route = true;

            let opencode_go = model.starts_with("opencode-go/");
            model = model
                .strip_prefix("opencode/")
                .or_else(|| model.strip_prefix("opencode-go/"))
                .unwrap_or(&model)
                .to_string();
            key = self.opencode_key(&key, opencode_go).await?;
            if base.is_empty() {
                base = "https://opencode.ai/inference".into()
            }
            if key.is_empty() {
                return Err("Native auth: Add an OpenCode API key in AI settings. Free-tier chat requires OpenCode itself.".into());
            }
        }
        if !matches!(p, "ollama" | "openai-compatible")
            && !p.starts_with("custom:")
            && key.is_empty()
        {
            return Err(format!("Native auth: {p} requires an API key or sign-in"));
        }
        let url = if opencode_route {
            match protocol {
                Protocol::Anthropic => endpoint(&base, "/anthropic/v1/messages"),
                Protocol::OpenaiResponses => endpoint(&base, "/openai/v1/responses"),
                Protocol::Gemini => format!(
                    "{base}/google/v1beta/models/{}:streamGenerateContent?alt=sse",
                    auth::url_encode(&model)
                ),
                Protocol::OpenaiChat => endpoint(&base, "/openai/v1/chat/completions"),
                Protocol::Ollama => {
                    return Err(
                        "Native configuration: OpenCode does not offer Ollama protocol".into(),
                    )
                }
            }
        } else if p == "antigravity" {
            format!("{base}/v1internal:streamGenerateContent?alt=sse")
        } else {
            match protocol {
                Protocol::OpenaiChat => endpoint(&base, "/chat/completions"),
                Protocol::OpenaiResponses => endpoint(&base, "/responses"),
                Protocol::Anthropic => endpoint(
                    &base,
                    if base.ends_with("/v1") {
                        "/messages"
                    } else {
                        "/v1/messages"
                    },
                ),
                Protocol::Ollama => endpoint(base.trim_end_matches("/v1"), "/api/chat"),
                Protocol::Gemini => format!(
                    "{}/models/{}:streamGenerateContent?alt=sse",
                    base,
                    auth::url_encode(&model)
                ),
            }
        };
        let url =
            reqwest::Url::parse(&url).map_err(|_| "Native configuration: invalid provider URL")?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err("Native configuration: provider URL must use http or https".into());
        }
        let mut secrets = if key.is_empty() {
            vec![]
        } else {
            vec![key.clone()]
        };
        let mut headers = reqwest::header::HeaderMap::new();
        let mut header = |name: &str, value: &str| -> Result<(), String> {
            headers.insert(
                reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(err)?,
                reqwest::header::HeaderValue::from_str(value)
                    .map_err(|_| "Native configuration: invalid header".to_string())?,
            );
            Ok(())
        };
        if protocol == Protocol::Anthropic {
            if p == "copilot" {
                header("authorization", &format!("Bearer {key}"))?;
            } else {
                header("x-api-key", &key)?;
            }
            header(
                "anthropic-version",
                if url.host_str() == Some("api.anthropic.com") {
                    "2026-10-21"
                } else {
                    "2023-06-01"
                },
            )?;
        } else if p == "gemini" && protocol == Protocol::Gemini {
            header("x-goog-api-key", &key)?;
        } else if !key.is_empty() {
            header("authorization", &format!("Bearer {key}"))?;
        }
        if p == "copilot" {
            header("user-agent", "GitHubCopilotChat/0.35.0")?;
            header("editor-version", "vscode/1.107.0")?;
            header("editor-plugin-version", "copilot-chat/0.35.0")?;
            header("copilot-integration-id", "vscode-chat")?;
            header("openai-intent", "conversation-edits")?;
            header(
                "x-initiator",
                if messages.last().is_some_and(|m| m.role == "user") {
                    "user"
                } else {
                    "agent"
                },
            )?;
            if messages
                .iter()
                .any(|m| runtime::transcript::contains_images(&m.blocks))
            {
                header("copilot-vision-request", "true")?;
            }
        }
        if p == "opencode" {
            if let Some(session) = cfg["sessionId"].as_str() {
                header("x-opencode-session", session)?;
            }
        }
        if p == "antigravity" {
            header("user-agent", super::antigravity::USER_AGENT)?;
        }
        if let Some(custom) = custom {
            for (k, v) in self.extension_provider_headers(custom).await? {
                if custom["headers"][&k].is_object()
                    || regex::Regex::new(
                        "(?i)token|secret|password|api.?key|authorization|credential",
                    )
                    .unwrap()
                    .is_match(&k)
                {
                    secrets.push(s(&v).strip_prefix("Bearer ").unwrap_or(s(&v)).into());
                }
                header(&k, s(&v))?;
            }
        }
        Ok(Route {
            origin: Origin {
                provider: p.into(),
                model,
                protocol,
            },
            url,
            headers,
            project,
            max_tokens: metadata.and_then(|m| m["maxTokens"].as_u64()).unwrap_or(
                if matches!(p, "anthropic" | "openai") {
                    16000
                } else {
                    8192
                },
            ),
            thinking: cfg["thinking"] == true,
            secrets,
        })
    }
}
