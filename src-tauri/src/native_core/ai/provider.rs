use super::*;
impl AiCore {
    pub(super) async fn cfg(&self, task: Option<&str>, provider: Option<&str>) -> CoreResult {
        let raw = self.read_config().await?;
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
        Ok(turn.text)
    }
    pub(super) async fn available(&self, cfg: &Value) -> bool {
        let p = s(&cfg["provider"]);
        if p == "antigravity" {
            return self
                .antigravity_accounts()
                .await
                .ok()
                .and_then(|v| v.as_array().map(|a| !a.is_empty()))
                .unwrap_or(false);
        }
        if p == "opencode" {
            if !s(&cfg["apiKey"]).is_empty() {
                return true;
            }
            #[cfg(not(test))]
            if std::env::var("OPENCODE_API_KEY").is_ok_and(|s| !s.is_empty()) {
                return true;
            }
            let path = self
                .inner
                .ctx
                .paths
                .home_dir
                .join(".local/share/opencode/auth.json");
            return tokio::task::spawn_blocking(move || {
                common::read_json(&path)
                    .ok()
                    .is_some_and(|v| !s(&v["opencode"]["key"]).is_empty())
            })
            .await
            .unwrap_or(false);
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
        if matches!(p, "anthropic" | "copilot") && key.trim().is_empty() {
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
            _ => format!("{base}/models"),
        };
        let mut request = self.inner.http.get(url).timeout(Duration::from_secs(4));
        if p == "anthropic" {
            request = request
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01");
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
        if id == "opencode" {
            let output = process::output(
                self.command("opencode").await.arg("models"),
                Duration::from_secs(12),
                &Cancel::new(),
            )
            .await
            .unwrap_or_default();
            let ansi = regex::Regex::new("\u{1b}\\[[0-9;]*m").unwrap();
            let clean = ansi.replace_all(&output, "");
            let mut models: Vec<String> = clean
                .lines()
                .map(str::trim)
                .filter(|m| m.starts_with("opencode/") || m.starts_with("opencode-go/"))
                .map(str::to_string)
                .collect();
            if models.is_empty() {
                models.push("opencode/big-pickle".into());
            }
            return Ok(json!(models));
        }
        if id == "antigravity" {
            let path = self.pi_dir().join("models-store.json");
            let raw = tokio::task::spawn_blocking(move || common::read_json(&path))
                .await
                .map_err(err)?
                .unwrap_or(Value::Null);
            if let Some(models) = raw["antigravity"]["models"].as_array() {
                let ids: Vec<_> = models.iter().filter_map(|m| m["id"].as_str()).collect();
                if !ids.is_empty() {
                    return Ok(json!(ids));
                }
            }
            return Ok(json!([
                "gemini-3.8-flash",
                "gemini-3.7-flash",
                "gemini-3.6-flash",
                "gemini-3.5-flash",
                "gemini-3.5-flash-lite",
                "gemini-3.1-flash-lite",
                "gemini-3-flash",
                "gemini-2.5-flash",
                "gemini-2.5-flash-lite",
                "gemini-3.1-pro",
                "gemini-2.5-pro",
                "claude-opus-4-6",
                "claude-sonnet-4-6",
                "gpt-oss-120b"
            ]));
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
        let urls = if id == "ollama" {
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
            let mut req = self.inner.http.get(url).timeout(Duration::from_secs(12));
            if id == "anthropic" {
                req = req
                    .header("x-api-key", &key)
                    .header("anthropic-version", "2023-06-01");
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
                    if let Ok(v) = response.json::<Value>().await {
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
                                ids.push(id_value.to_string());
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
            "gemini" => json!(["gemini-2.0-flash", "gemini-2.5-flash", "gemini-2.5-pro"]),
            "deepseek" => json!(["deepseek-v4-flash", "deepseek-v4-pro", "deepseek-reasoner"]),
            "tokenrouter" => json!(["moonshotai/kimi-k3-free"]),
            _ => json!([]),
        })
    }
}
#[cfg(test)]
fn format_error(raw: &str, label: &str) -> String {
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
