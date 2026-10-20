use super::*;
use futures_util::StreamExt;
use process::{Cancel, Lines};
use std::collections::BTreeMap;
#[derive(Default)]
pub struct StreamParser {
    lines: Lines,
    tools: BTreeMap<u64, (String, String)>,
    anthropic: BTreeMap<u64, (String, String)>,
    sse_data: Vec<String>,
}
impl StreamParser {
    pub fn feed(&mut self, bytes: &[u8], eof: bool, kind: &str) -> Result<Vec<Value>, String> {
        let mut out = Vec::new();
        for line in self.lines.feed(bytes, eof)? {
            if kind == "ollama" {
                if !line.trim().is_empty() {
                    self.payload(serde_json::from_str(&line).map_err(err)?, kind, &mut out)?;
                }
                continue;
            }
            if line.is_empty() {
                self.flush(kind, &mut out)?;
            } else if let Some(data) = line.strip_prefix("data:") {
                let data = data.strip_prefix(' ').unwrap_or(data);
                self.sse_data.push(data.into());
            }
        }
        if eof {
            self.flush(kind, &mut out)?;
            self.flush_tools(&mut out);
        }
        Ok(out)
    }
    fn flush(&mut self, kind: &str, out: &mut Vec<Value>) -> Result<(), String> {
        if self.sse_data.is_empty() {
            return Ok(());
        }
        let data = std::mem::take(&mut self.sse_data).join("\n");
        if data.trim() == "[DONE]" {
            self.flush_tools(out);
            return Ok(());
        }
        let value = serde_json::from_str(&data)
            .map_err(|e| format!("Invalid provider stream JSON: {e}"))?;
        self.payload(value, kind, out)
    }
    fn flush_tools(&mut self, out: &mut Vec<Value>) {
        for (_, (name, args)) in std::mem::take(&mut self.tools) {
            if !name.is_empty() {
                out.push(json!({"toolCall":{"name":name,"args":serde_json::from_str::<Value>(&args).unwrap_or_else(|_|if args.trim().is_empty(){json!({})}else{json!(args)})}}));
            }
        }
    }
    fn payload(&mut self, v: Value, kind: &str, out: &mut Vec<Value>) -> Result<(), String> {
        if let Some(error) = v.get("error") {
            return Err(if error.is_string() {
                s(error).into()
            } else {
                error.to_string()
            });
        }
        if kind == "ollama" {
            if let Some(text) = v["message"]["content"].as_str() {
                if !text.is_empty() {
                    out.push(json!({"text":text}));
                }
            }
            if let Some(tools) = v["message"]["tool_calls"].as_array() {
                for tool in tools {
                    out.push(json!({"toolCall":{"name":tool["function"]["name"],"args":tool["function"]["arguments"]}}));
                }
            }
            return Ok(());
        }
        if kind == "anthropic" {
            let index = v["index"].as_u64().unwrap_or(0);
            match s(&v["type"]) {
                "content_block_start" => {
                    let b = &v["content_block"];
                    if b["type"] == "tool_use" {
                        self.anthropic
                            .insert(index, (s(&b["name"]).into(), String::new()));
                    } else if let Some(t) = b["text"].as_str() {
                        if !t.is_empty() {
                            out.push(json!({"text":t}));
                        }
                    }
                }
                "content_block_delta" => {
                    let d = &v["delta"];
                    if let Some(text) = d["text"].as_str() {
                        out.push(json!({"text":text}));
                    }
                    if let Some(part) = d["partial_json"].as_str() {
                        self.anthropic.entry(index).or_default().1.push_str(part);
                    }
                }
                "content_block_stop" => {
                    if let Some((name, args)) = self.anthropic.remove(&index) {
                        out.push(json!({"toolCall":{"name":name,"args":serde_json::from_str::<Value>(&args).unwrap_or(json!({}))}}));
                    }
                }
                _ => {}
            }
            return Ok(());
        }
        let choice = &v["choices"][0];
        let d = &choice["delta"];
        if let Some(text) = d["content"].as_str() {
            if !text.is_empty() {
                out.push(json!({"text":text}));
            }
        }
        if let Some(tools) = d["tool_calls"].as_array() {
            for tool in tools {
                let part = self
                    .tools
                    .entry(tool["index"].as_u64().unwrap_or(0))
                    .or_default();
                if let Some(name) = tool["function"]["name"].as_str() {
                    part.0 = name.into();
                }
                if let Some(args) = tool["function"]["arguments"].as_str() {
                    part.1.push_str(args);
                }
            }
        }
        if choice["finish_reason"] == "tool_calls" {
            self.flush_tools(out);
        }
        Ok(())
    }
}
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
        let p = s(&cfg["provider"]);
        let mut answer = String::new();
        if p == "opencode" {
            let prompt=std::iter::once("Continue the conversation below. Follow the system instructions and respond to the final user message.".to_string()).chain(messages.iter().map(|m|format!("{}:\n{}",s(&m["role"]).to_uppercase(),s(&m["content"])))).collect::<Vec<_>>().join("\n\n");
            let result = process::output(
                self.command("opencode")
                    .await
                    .args(["run", "--model", s(&cfg["model"]), "--", &prompt])
                    .env("TERM", "dumb")
                    .env("CI", "true"),
                Duration::from_secs(600),
                cancel,
            )
            .await?;
            let result = result.trim().to_string();
            on_delta(json!({"text":result}));
            return Ok(result);
        }
        if p == "antigravity" {
            let prompt = messages
                .iter()
                .map(|m| format!("{}: {}", s(&m["role"]).to_uppercase(), s(&m["content"])))
                .collect::<Vec<_>>()
                .join("\n\n");
            let result = self.pi_run(&prompt, &[], None, cfg, cancel, None).await?;
            on_delta(json!({"text":result}));
            return Ok(result);
        }
        let base = cfg["openaiCompatibleBaseURL"]
            .as_str()
            .filter(|b| !b.is_empty())
            .unwrap_or(s(&cfg["baseURL"]))
            .trim_end_matches('/')
            .replace("://0.0.0.0", "://127.0.0.1");
        let kind = if p == "anthropic" {
            "anthropic"
        } else if p == "ollama" {
            "ollama"
        } else {
            "openai"
        };
        let key = if p == "copilot" {
            self.copilot_token().await?
        } else if p == "gemini" {
            cfg["geminiApiKey"]
                .as_str()
                .unwrap_or(s(&cfg["apiKey"]))
                .to_string()
        } else {
            s(&cfg["apiKey"]).to_string()
        };
        if !matches!(p, "ollama" | "openai-compatible")
            && !p.starts_with("custom:")
            && key.trim().is_empty()
        {
            return Err(format!("{p}: missing API key or access token"));
        }
        let url = if p == "copilot" {
            "https://api.githubcopilot.com/chat/completions".to_string()
        } else if kind == "anthropic" {
            format!(
                "{}/messages",
                if base.ends_with("/v1") {
                    base.clone()
                } else {
                    format!("{base}/v1")
                }
            )
        } else if kind == "ollama" {
            format!("{}/api/chat", base.trim_end_matches("/v1"))
        } else if base.ends_with("/chat/completions") {
            base.clone()
        } else {
            format!("{base}/chat/completions")
        };
        let mut body = json!({"model":cfg["model"],"messages":messages,"stream":true});
        if kind == "openai" && p != "copilot" {
            body["max_tokens"] = json!(8192);
        }
        if matches!(kind, "anthropic" | "ollama") {
            body["messages"] = json!(messages
                .iter()
                .map(|m| if m["role"] == "tool" {
                    json!({"role":"user","content":format!("[tool]\n{}",s(&m["content"]))})
                } else {
                    m.clone()
                })
                .collect::<Vec<_>>());
        }
        if kind == "anthropic" {
            let system = messages
                .iter()
                .filter(|m| m["role"] == "system")
                .map(|m| s(&m["content"]))
                .collect::<Vec<_>>()
                .join("\n\n");
            body["system"] = json!(system);
            body["messages"] = json!(body["messages"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|m| m["role"] != "system")
                .cloned()
                .collect::<Vec<_>>());
            body["max_tokens"] = json!(4096);
        }
        let mut request = self.inner.http.post(url).json(&body);
        if kind == "anthropic" {
            request = request
                .header("x-api-key", &key)
                .header("anthropic-version", "2023-06-01");
        } else if !key.is_empty() {
            request = request.bearer_auth(&key);
        }
        if p == "copilot" {
            request = request
                .header("Editor-Version", "Tezbar/0.1.0")
                .header("Copilot-Integration-Id", "vscode-chat");
        }
        let result = async {
            let response = request.send().await.map_err(err)?;
            let status = response.status();
            if !status.is_success() {
                let raw = format!(
                    "{p} error {}: {}",
                    status.as_u16(),
                    cut(&response.text().await.unwrap_or_default(), 500)
                );
                return Err(if kind == "openai" && p != "copilot" {
                    format_error(&raw, provider_label(cfg))
                } else {
                    raw
                });
            }
            let mut stream = response.bytes_stream();
            let mut parser = StreamParser::default();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(err)?;
                for delta in parser.feed(&chunk, false, kind)? {
                    if let Some(text) = delta["text"].as_str() {
                        answer.push_str(text);
                    }
                    on_delta(delta);
                }
            }
            for delta in parser.feed(&[], true, kind)? {
                if let Some(text) = delta["text"].as_str() {
                    answer.push_str(text);
                }
                on_delta(delta);
            }
            Ok(answer)
        };
        tokio::select! {_=cancel.cancelled()=>Err("Operation cancelled".into()),result=tokio::time::timeout(Duration::from_secs(600),result)=>result.map_err(|_|"Provider request timed out".to_string())?}
    }
    pub(super) async fn available(&self, cfg: &Value) -> bool {
        let p = s(&cfg["provider"]);
        if p == "antigravity" {
            return true;
        }
        if p == "opencode" {
            return process::output(
                self.command(if cfg!(windows) { "where.exe" } else { "which" })
                    .await
                    .arg("opencode"),
                Duration::from_secs(4),
                &self.inner.stop,
            )
            .await
            .is_ok();
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
            Ok(response) if p == "copilot" => response.status().is_success() || key.len() > 20,
            Ok(response) => response.status().as_u16() < 500,
            Err(_) => p == "copilot" && key.len() > 20,
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
mod tests {
    use super::*;
    #[test]
    fn stream_chunk_boundaries_and_tool_assembly() {
        let input=concat!("data: {\"choices\":[{\"delta\":{\"content\":\"🦀\"}}]}\r\n\r\n","data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"name\":\"read\",\"arguments\":\"{\\\"path\\\":\"}}]}}]}\n\n","data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"a\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]");
        let mut parser = StreamParser::default();
        let mut got = Vec::new();
        for byte in input.as_bytes() {
            got.extend(parser.feed(&[*byte], false, "openai").unwrap());
        }
        got.extend(parser.feed(&[], true, "openai").unwrap());
        assert_eq!(
            got,
            json!([{"text":"🦀"},{"toolCall":{"name":"read","args":{"path":"a"}}}])
                .as_array()
                .unwrap()
                .clone()
        );
    }
    #[test]
    fn ndjson_tail_is_not_lost() {
        let mut p = StreamParser::default();
        assert_eq!(
            p.feed(br#"{"message":{"content":"last"}}"#, true, "ollama")
                .unwrap()[0]["text"],
            "last"
        );
    }
}

#[cfg(test)]
mod anthropic_tests {
    use super::*;
    #[test]
    fn anthropic_text_and_incremental_tool_json_are_preserved() {
        let mut parser = StreamParser::default();
        let events = [
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hello 🦀"}}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","name":"read","input":{}}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"\"fixture\"}"}}),
            json!({"type":"content_block_stop","index":1}),
        ];
        let mut out = Vec::new();
        for event in events {
            let data = format!("event: message\ndata: {event}\n\n");
            for chunk in data.as_bytes().chunks(3) {
                out.extend(parser.feed(chunk, false, "anthropic").unwrap());
            }
        }
        out.extend(parser.feed(&[], true, "anthropic").unwrap());
        assert_eq!(
            out,
            json!([{"text":"hello 🦀"},{"toolCall":{"name":"read","args":{"path":"fixture"}}}])
                .as_array()
                .unwrap()
                .clone()
        );
    }
}

fn provider_label(cfg: &Value) -> &str {
    match s(&cfg["provider"]) {
        "openai" => "OpenAI",
        "openai-compatible" => "OpenAI-compatible provider",
        "tokenrouter" => "TokenRouter",
        "deepseek" => "DeepSeek",
        "gemini" => "Gemini",
        custom => cfg["customProviders"]
            .as_array()
            .and_then(|providers| providers.iter().find(|provider| provider["id"] == custom))
            .and_then(|provider| provider["title"].as_str())
            .unwrap_or("Custom provider"),
    }
}
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
