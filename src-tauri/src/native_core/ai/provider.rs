use super::*;
impl AiCore {
    pub(super) async fn cfg(&self, task: Option<&str>, provider: Option<&str>) -> CoreResult {
        let raw = self.read_config().await?;
        let raw = self.extension_config(raw, None).await?;
        config::validate_selection(&raw, task, provider)?;
        Ok(config::selected(&raw, task, provider))
    }
    pub(super) async fn stream(
        &self,
        cfg: &Value,
        messages: Vec<Value>,
        cancel: &Cancel,
        mut on_delta: impl FnMut(Value) + Send,
    ) -> Result<String, String> {
        let messages = messages
            .iter()
            .map(runtime::transcript::Message::legacy)
            .collect::<Vec<_>>();
        let turn = self
            .canonical_turn(cfg, &messages, &[], 0, cancel, |text| {
                on_delta(json!({"text":text}))
            })
            .await?;
        if !turn.calls.is_empty() {
            return Err("Native protocol: model requested tools in a tool-free completion".into());
        }
        Ok(turn.text)
    }
    async fn custom_catalog(&self, cfg: &Value) -> CoreResult {
        use runtime::transcript::Protocol;
        let mut cfg = cfg.clone();
        if s(&cfg["model"]).is_empty() {
            cfg["model"] = json!("catalog");
        }
        let route = self.native_route(&cfg, &[]).await?;
        let path = route.url.path();
        let path = match route.origin.protocol {
            Protocol::Anthropic => format!("{}/models", path.trim_end_matches("/messages")),
            Protocol::Gemini => format!("{}/models", path.split("/models/").next().unwrap_or(path)),
            Protocol::Ollama => format!("{}/api/tags", path.trim_end_matches("/api/chat")),
            Protocol::OpenaiResponses => format!("{}/models", path.trim_end_matches("/responses")),
            Protocol::OpenaiChat => {
                format!("{}/models", path.trim_end_matches("/chat/completions"))
            }
        };
        let mut url = route.url;
        url.set_path(&path);
        url.set_query(None);
        let response = self
            .inner
            .http
            .get(url)
            .headers(route.headers)
            .timeout(Duration::from_secs(4))
            .send()
            .await
            .map_err(|_| "Custom provider catalog: network failed")?;
        if !response.status().is_success() {
            return Err(format!(
                "Custom provider catalog: HTTP {}",
                response.status().as_u16()
            ));
        }
        oauth::bounded_json(response).await
    }
    pub(super) async fn available(&self, cfg: &Value) -> bool {
        let p = s(&cfg["provider"]);
        if p.starts_with("custom:") {
            return self.custom_catalog(cfg).await.is_ok();
        }
        if p == "antigravity" {
            return self
                .antigravity_accounts()
                .await
                .ok()
                .and_then(|v| v.as_array().map(|a| !a.is_empty()))
                .unwrap_or(false);
        }
        if p == "opencode" {
            return self
                .opencode_key(s(&cfg["apiKey"]), false)
                .await
                .is_ok_and(|key| !key.is_empty())
                || self
                    .opencode_key(s(&cfg["apiKey"]), true)
                    .await
                    .is_ok_and(|key| !key.is_empty());
        }
        if p == "copilot" {
            return self
                .copilot_models()
                .await
                .is_ok_and(|m| m.as_array().is_some_and(|m| !m.is_empty()));
        }
        let base = cfg["openaiCompatibleBaseURL"]
            .as_str()
            .filter(|b| !b.is_empty())
            .unwrap_or(s(&cfg["baseURL"]))
            .trim_end_matches('/')
            .trim_end_matches("/chat/completions")
            .replace("://0.0.0.0", "://127.0.0.1");
        let key = if p == "copilot" {
            s(&cfg["copilotGithubToken"])
        } else if p == "gemini" {
            cfg["geminiApiKey"].as_str().unwrap_or(s(&cfg["apiKey"]))
        } else {
            s(&cfg["apiKey"])
        };
        if !matches!(p, "ollama" | "openai-compatible")
            && !p.starts_with("custom:")
            && key.trim().is_empty()
        {
            return false;
        }
        if p != "copilot" && base.trim().is_empty() {
            return false;
        }
        let url = match p {
            "copilot" => "https://api.githubcopilot.com/models".to_string(),
            "ollama" => format!("{}/api/tags", base.trim_end_matches("/v1")),
            "anthropic" => format!(
                "{}/models",
                if base.ends_with("/v1") {
                    base.clone()
                } else {
                    format!("{base}/v1")
                }
            ),
            "gemini"
                if base == "https://generativelanguage.googleapis.com/v1beta/openai"
                    || base == "https://generativelanguage.googleapis.com/v1beta" =>
            {
                "https://generativelanguage.googleapis.com/v1beta/models".into()
            }
            _ => format!("{base}/models"),
        };
        let url = self.endpoint(&format!("{p}-models"), &url);
        let mut request = self.inner.http.get(url).timeout(Duration::from_secs(4));
        if p == "anthropic" {
            request = request
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01");
        } else if p == "gemini"
            && (base == "https://generativelanguage.googleapis.com/v1beta/openai"
                || base == "https://generativelanguage.googleapis.com/v1beta")
        {
            request = request.header("x-goog-api-key", key);
        } else if !key.trim().is_empty() {
            request = request.bearer_auth(key.trim());
        }
        if p == "copilot" {
            request = request
                .header("Editor-Version", "Tezbar/0.1.0")
                .header("Copilot-Integration-Id", "vscode-chat");
        }
        match request.send().await {
            Ok(response) if matches!(p, "anthropic" | "ollama") => response.status().is_success(),
            Ok(response) if p == "copilot" => response.status().is_success(),
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }
    pub(super) async fn models(
        &self,
        id: &str,
        base_override: Option<&str>,
        key_override: Option<&str>,
    ) -> CoreResult {
        let mut cfg = self.cfg(None, Some(id)).await?;
        if let Some(base) = base_override.filter(|b| !b.trim().is_empty()) {
            cfg["baseURL"] = json!(base);
            cfg["openaiCompatibleBaseURL"] = json!(base);
        }
        if let Some(key) = key_override {
            cfg["apiKey"] = json!(key);
        }
        if id.starts_with("custom:") {
            let v = self.custom_catalog(&cfg).await?;
            let mut ids = v["data"]
                .as_array()
                .or(v["models"].as_array())
                .into_iter()
                .flatten()
                .filter_map(|m| {
                    m["id"]
                        .as_str()
                        .or(m["name"].as_str())
                        .or(m["model"].as_str())
                })
                .map(|id| id.strip_prefix("models/").unwrap_or(id).to_string())
                .collect::<Vec<_>>();
            ids.sort();
            ids.dedup();
            return Ok(json!(ids));
        }
        if id == "opencode" {
            let catalog: Value =
                serde_json::from_str(include_str!("providers/opencode_catalog.json"))
                    .expect("static catalog");
            let mut models = Vec::<String>::new();
            for go in [false, true] {
                let family = if go { "opencode-go/" } else { "opencode/" };
                let key = self.opencode_key(s(&cfg["apiKey"]), go).await?;
                let mut found = vec![];
                let base = cfg["baseURL"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("https://opencode.ai/inference");
                let base = if go && !base.ends_with("/go") {
                    format!("{}/go", base.trim_end_matches('/'))
                } else {
                    base.to_string()
                };
                let name = if go {
                    "opencode-go-models"
                } else {
                    "opencode-models"
                };
                let url = self.endpoint(name, &format!("{}/v1/models", base.trim_end_matches('/')));
                #[cfg(test)]
                let probe = self.inner.endpoints.lock().unwrap().contains_key(name);
                #[cfg(not(test))]
                let probe = true;
                if probe && !key.is_empty() {
                    if let Ok(response) = self
                        .inner
                        .http
                        .get(url)
                        .bearer_auth(key)
                        .timeout(Duration::from_secs(8))
                        .send()
                        .await
                    {
                        if response.status().is_success() {
                            if let Ok(v) = oauth::bounded_json(response).await {
                                for model in v["data"].as_array().into_iter().flatten().take(512) {
                                    if let Some(id) = model["id"].as_str() {
                                        let id = id.strip_prefix(family).unwrap_or(id);
                                        if !id.contains('/')
                                            && !id.starts_with("jev-")
                                            && !id.ends_with("-free")
                                            && id != "big-pickle"
                                        {
                                            found.push(format!("{family}{id}"));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if found.is_empty() {
                    found.extend(
                        catalog
                            .as_object()
                            .unwrap()
                            .keys()
                            .filter(|id| {
                                id.starts_with(family)
                                    && !id.contains("/jev-")
                                    && !id.ends_with("-free")
                                    && !id.ends_with("/big-pickle")
                            })
                            .cloned(),
                    );
                }
                models.extend(found);
            }
            models.sort();
            models.dedup();
            return Ok(json!(models));
        }
        if id == "antigravity" {
            return self.antigravity_catalog().await;
        }
        if id == "copilot" {
            return self.copilot_models().await.map(|models| {
                json!(models
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|m| m["id"].as_str())
                    .collect::<Vec<_>>())
            });
        }
        let base = cfg["openaiCompatibleBaseURL"]
            .as_str()
            .filter(|b| !b.is_empty())
            .unwrap_or(s(&cfg["baseURL"]))
            .trim_end_matches('/')
            .trim_end_matches("/chat/completions")
            .replace("://0.0.0.0", "://127.0.0.1");
        let key = if id == "copilot" {
            self.copilot_token().await?
        } else if id == "gemini" {
            cfg["geminiApiKey"]
                .as_str()
                .unwrap_or(s(&cfg["apiKey"]))
                .to_string()
        } else {
            s(&cfg["apiKey"]).to_string()
        };
        let native_gemini = id == "gemini"
            && (base == "https://generativelanguage.googleapis.com/v1beta/openai"
                || base == "https://generativelanguage.googleapis.com/v1beta");
        let urls = if native_gemini {
            vec!["https://generativelanguage.googleapis.com/v1beta/models".into()]
        } else if id == "ollama" {
            vec![format!("{}/api/tags", base.trim_end_matches("/v1"))]
        } else if id == "copilot" {
            vec!["https://api.githubcopilot.com/models".into()]
        } else if id == "anthropic" {
            vec![format!(
                "{}/models",
                if base.ends_with("/v1") {
                    base.clone()
                } else {
                    format!("{base}/v1")
                }
            )]
        } else {
            vec![
                format!("{base}/models"),
                if base.ends_with("/v1") {
                    format!("{}/models", base.trim_end_matches("/v1"))
                } else {
                    format!("{base}/v1/models")
                },
            ]
        };
        for url in urls {
            let url = self.endpoint(&format!("{id}-models"), &url);
            let mut req = self.inner.http.get(url).timeout(Duration::from_secs(12));
            if id == "anthropic" {
                req = req
                    .header("x-api-key", &key)
                    .header("anthropic-version", "2023-06-01");
            } else if native_gemini {
                req = req.header("x-goog-api-key", &key);
            } else if !key.is_empty() {
                req = req.bearer_auth(&key);
            }
            if id == "copilot" {
                req = req
                    .header("Editor-Version", "Tezbar/0.1.0")
                    .header("Copilot-Integration-Id", "vscode-chat");
            }
            if let Ok(response) = req.send().await {
                if response.status().is_success() {
                    if let Ok(v) = oauth::bounded_json(response).await {
                        let mut ids = Vec::new();
                        if let Some(list) = v["data"].as_array().or_else(|| v["models"].as_array())
                        {
                            for model in list {
                                let id_value = model["id"]
                                    .as_str()
                                    .or_else(|| model["name"].as_str())
                                    .or_else(|| model["model"].as_str())
                                    .unwrap_or("");
                                if id_value.is_empty() {
                                    continue;
                                }
                                if id == "copilot" {
                                    let caps = &model["capabilities"];
                                    if caps["type"].as_str().is_some_and(|t| t != "chat")
                                        || matches!(
                                            s(&caps["family"]),
                                            "search-agent" | "exec-agent" | "trajectory-compaction"
                                        )
                                        || regex::Regex::new(
                                            "(?i)(^|[-_/])(exec-)?agent[-_]?[a-z0-9]*$",
                                        )
                                        .unwrap()
                                        .is_match(id_value)
                                    {
                                        continue;
                                    }
                                }
                                if native_gemini
                                    && model["supportedGenerationMethods"]
                                        .as_array()
                                        .is_some_and(|a| !a.iter().any(|m| m == "generateContent"))
                                {
                                    continue;
                                }
                                ids.push(
                                    if native_gemini {
                                        id_value.strip_prefix("models/").unwrap_or(id_value)
                                    } else {
                                        id_value
                                    }
                                    .to_string(),
                                );
                            }
                        }
                        ids.sort();
                        ids.dedup();
                        if !ids.is_empty() {
                            return Ok(json!(ids));
                        }
                    }
                }
            }
        }
        Ok(match id {
            "gemini" => json!(["gemini-3.5-flash", "gemini-2.5-flash", "gemini-2.5-pro"]),
            "deepseek" => json!(["deepseek-v4-flash", "deepseek-v4-pro", "deepseek-reasoner"]),
            "tokenrouter" => json!(["moonshotai/kimi-k3-free"]),
            _ => json!([]),
        })
    }
}
pub(super) fn format_error(raw: &str, label: &str) -> String {
    let detail = raw
        .find('{')
        .and_then(|at| serde_json::from_str::<Value>(&raw[at..]).ok())
        .and_then(|v| {
            v["error"]["message"]
                .as_str()
                .map(str::trim)
                .map(str::to_string)
        });
    let message = detail.as_deref().unwrap_or(raw).trim();
    if let Some(caps) =
        regex::Regex::new("(?i)supported API model names are (.+?), but you passed (.+?)(?:\\.$|$)")
            .unwrap()
            .captures(message)
    {
        return format!(
            "Model \"{}\" is not supported by this provider. Choose {} and try again.",
            &caps[2], &caps[1]
        );
    }
    if let Some(caps) = regex::Regex::new(r#"(?i)Model\s+["']([^"']+)["']\s+not found"#)
        .unwrap()
        .captures(message)
    {
        return format!("The selected model \"{}\" is unavailable. Choose another model in AI settings and try again.",&caps[1]);
    }
    if detail.is_some() {
        let status = regex::Regex::new(r"(?i)\berror\s+(\d{3})\b")
            .unwrap()
            .captures(raw)
            .map(|c| format!(" ({})", &c[1]))
            .unwrap_or_default();
        format!("{label} request failed{status}: {message}")
    } else {
        raw.trim().into()
    }
}
#[cfg(test)]
mod error_tests {
    use super::*;
    #[test]
    fn provider_errors_keep_actionable_model_messages() {
        assert_eq!(format_error(r#"OpenAI error 400: {"error":{"message":"Model 'missing' not found"}}"#,"OpenAI"),"The selected model \"missing\" is unavailable. Choose another model in AI settings and try again.");
        assert_eq!(
            format_error(
                r#"OpenAI error 429: {"error":{"message":"Try later"}}"#,
                "OpenAI"
            ),
            "OpenAI request failed (429): Try later"
        );
    }
}

impl AiCore {
    pub(in crate::native_core::ai) async fn opencode_key(
        &self,
        configured: &str,
        go: bool,
    ) -> Result<String, String> {
        if !configured.is_empty() {
            return Ok(configured.into());
        }
        #[cfg(not(test))]
        if let Ok(key) = std::env::var("OPENCODE_API_KEY") {
            if !key.is_empty() {
                return Ok(key);
            }
        }
        let path = self
            .inner
            .ctx
            .paths
            .home_dir
            .join(".local/share/opencode/auth.json");
        tokio::task::spawn_blocking(move || {
            if !path.exists() {
                return Ok(String::new());
            }
            let v = common::read_json(&path)?;
            Ok(v[if go { "opencode-go" } else { "opencode" }]["key"]
                .as_str()
                .unwrap_or("")
                .to_string())
        })
        .await
        .map_err(err)?
    }
}
