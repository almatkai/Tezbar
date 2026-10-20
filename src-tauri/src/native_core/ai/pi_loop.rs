use super::*;
use std::collections::{BTreeMap, HashMap};
#[derive(Default)]
pub struct Loop {
    stages: HashMap<String, Value>,
    blocks: BTreeMap<u64, String>,
    previous: Vec<String>,
    current: String,
    current_failed: bool,
    separator: bool,
    modern: bool,
    pub finished: bool,
    needs_answer: bool,
    error: Option<String>,
    next: u64,
    pub answer: String,
}
fn content(v: &Value) -> String {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n\n")
        })
        .unwrap_or_default()
}
fn compact(v: &str, n: usize) -> String {
    let clean = v.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() > n {
        format!("{}…", cut(&clean, n - 1))
    } else {
        clean
    }
}
fn label(name: &str, a: &Value) -> String {
    let value = |key: &str, fallback: &str| compact(a[key].as_str().unwrap_or(fallback), 60);
    match name {
        "bash" => format!("bash: {}", value("command", "<cmd>")),
        "read" | "write" => format!("{name} {}", value("path", "<path>")),
        "edit" => {
            let n = a["edits"].as_array().map_or(0, Vec::len);
            format!(
                "edit {} ({n} change{})",
                value("path", "<path>"),
                if n == 1 { "" } else { "s" }
            )
        }
        "grep" | "find" => format!("{name} {}", value("pattern", "<pattern>")),
        "ls" => format!("ls {}", value("path", ".")),
        "launcher_search" => format!("search Tezbar: {}", value("query", "<query>")),
        "pc_search" | "search_knowledge" => format!("deep search: {}", value("query", "<query>")),
        "pc_read" => format!("read deep-search result: {}", value("resultId", "<result>")),
        "generate_image" => format!("generate image: {}", value("prompt", "<prompt>")),
        "workflow_create" => {
            let title = a["name"].as_str().unwrap_or(s(&a["label"]));
            if title.is_empty() {
                "multi-agent workflow".into()
            } else {
                format!("workflow: {}", compact(title, 60))
            }
        }
        "workflow_list" => "list workflows".into(),
        "workflow_stop" => "stop workflow".into(),
        "workflow_inspect" | "workflow_result" => {
            let prefix = if name == "workflow_inspect" {
                "inspect workflow"
            } else {
                "workflow result"
            };
            if s(&a["run"]).is_empty() {
                prefix.into()
            } else {
                format!("{prefix}: {}", compact(s(&a["run"]), 24))
            }
        }
        _ => name.into(),
    }
}
impl Loop {
    fn update(&mut self, key: &str, patch: Value, out: &mut Vec<Value>) {
        if let Some(v) = self.stages.get_mut(key) {
            for (k, vv) in patch.as_object().unwrap() {
                v[k] = vv.clone();
            }
            out.push(json!({"type":"stage","stage":v}));
        }
    }
    fn start(&mut self, key: &str, label: String, extra: Value, out: &mut Vec<Value>) {
        let mut v = json!({"index":self.next,"label":label,"status":"running"});
        self.next += 1;
        for (k, vv) in extra.as_object().unwrap() {
            v[k] = vv.clone();
        }
        self.stages.insert(key.into(), v.clone());
        out.push(json!({"type":"stage","stage":v}));
    }
    fn thinking_done(&mut self, out: &mut Vec<Value>) {
        if self
            .stages
            .get("thinking")
            .is_some_and(|v| v["status"] == "running")
        {
            self.update("thinking", json!({"status":"done"}), out);
        }
    }
    fn snapshot(&mut self, m: &Value) {
        if m["role"] != "assistant" {
            return;
        }
        let reason = s(&m["stopReason"]);
        let text = content(&m["content"]);
        if reason == "toolUse" {
            self.needs_answer = true;
        } else if !text.trim().is_empty() && !matches!(reason, "error" | "aborted") {
            self.needs_answer = false;
        }
        if matches!(reason, "error" | "aborted") || !s(&m["errorMessage"]).is_empty() {
            self.error = Some(
                m["errorMessage"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(if reason == "aborted" {
                        "Agent run aborted"
                    } else {
                        "The model provider encountered an error."
                    })
                    .into(),
            );
            self.current_failed = true;
        } else if !reason.is_empty() && reason != "pending" {
            self.error = None;
            self.current_failed = false;
        }
        if let Some(blocks) = m["content"].as_array() {
            self.current = text;
            self.blocks.clear();
            for (i, b) in blocks.iter().enumerate() {
                if b["type"] == "text" {
                    self.blocks.insert(i as u64, s(&b["text"]).into());
                }
            }
        }
    }
    fn finish(&mut self, out: &mut Vec<Value>) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.thinking_done(out);
        let mut texts = self.previous.clone();
        if !self.current_failed && !self.current.trim().is_empty() {
            texts.push(self.current.trim().into());
        }
        self.answer = texts.join("\n\n");
        let running = self
            .stages
            .iter()
            .filter(|(_, v)| v["status"] == "running")
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>();
        for key in running {
            self.update(&key,json!({"status":"failed","detail":self.error.as_deref().unwrap_or("Run ended before this step finished.")}),out);
        }
        if !self.answer.is_empty() {
            out.push(json!({"type":"answer","text":self.answer}));
        }
        if let Some(error) = &self.error {
            out.push(json!({"type":"error","message":error}));
        } else if self.answer.is_empty() || self.needs_answer {
            self.error = Some("Agent finished without a response.".into());
            out.push(json!({"type":"error","message":self.error}));
        }
    }
    pub fn result(&self) -> Result<String, String> {
        if let Some(error) = &self.error {
            Err(error.clone())
        } else {
            Ok(self.answer.clone())
        }
    }
    pub fn event(&mut self, e: &Value) -> Vec<Value> {
        let mut out = Vec::new();
        if self.finished {
            return out;
        }
        match s(&e["type"]) {
            "turn_start" => {
                if !self.current.trim().is_empty() && !self.current_failed {
                    self.previous.push(self.current.trim().into());
                }
                self.current.clear();
                self.blocks.clear();
                self.current_failed = false;
                self.separator = !self.previous.is_empty();
            }
            "message_end" | "turn_end" => self.snapshot(&e["message"]),
            "message_update" => {
                let d = &e["assistantMessageEvent"];
                match s(&d["type"]) {
                    "thinking_delta" => {
                        if !self
                            .stages
                            .get("thinking")
                            .is_some_and(|v| v["status"] == "running")
                        {
                            self.start("thinking", "Thinking…".into(), json!({}), &mut out)
                        }
                    }
                    "thinking_end" => self.thinking_done(&mut out),
                    "text_delta" | "text_end" => {
                        self.thinking_done(&mut out);
                        let index = d["contentIndex"].as_u64().unwrap_or(0);
                        if d["type"] == "text_delta" {
                            let delta = s(&d["delta"]);
                            if !delta.trim().is_empty() {
                                self.needs_answer = false;
                            }
                            if self.separator && !delta.is_empty() {
                                out.push(json!({"type":"message","delta":"\n\n"}));
                                self.separator = false;
                            }
                            self.blocks.entry(index).or_default().push_str(delta);
                            if !delta.is_empty() {
                                out.push(json!({"type":"message","delta":delta}));
                            }
                        } else {
                            let text = s(&d["content"]);
                            if !text.trim().is_empty() {
                                self.needs_answer = false;
                            }
                            self.blocks.insert(index, text.into());
                        }
                        self.current = self
                            .blocks
                            .values()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join("\n\n");
                    }
                    _ => {}
                }
            }
            "tool_execution_start" => {
                self.needs_answer = true;
                self.thinking_done(&mut out);
                let id = s(&e["toolCallId"]);
                if !id.is_empty() {
                    let name = e["toolName"].as_str().unwrap_or("tool");
                    self.start(
                        id,
                        label(name, &e["args"]),
                        json!({"toolName":name}),
                        &mut out,
                    );
                }
            }
            "tool_execution_update" | "tool_execution_end" => {
                let end = e["type"] == "tool_execution_end";
                let text = content(&e[if end { "result" } else { "partialResult" }]["content"]);
                let ansi = regex::Regex::new("\u{1b}\\[[0-9;]*[a-zA-Z]").unwrap();
                let text = ansi.replace_all(&text, "");
                let mut patch = json!({});
                if !text.is_empty() {
                    patch["output"] = json!(cut(&text, 8000));
                    patch["outputTruncated"] = json!(text.chars().count() > 8000);
                }
                if end {
                    let failed = e["isError"] == true;
                    patch["status"] = json!(if failed { "failed" } else { "done" });
                    if failed {
                        patch["detail"] = json!(compact(
                            if text.is_empty() {
                                "Tool failed"
                            } else {
                                &text
                            },
                            160
                        ));
                    }
                    if e["durationMs"].as_f64().is_some_and(|n| n >= 0.0) {
                        patch["durationMs"] = e["durationMs"].clone();
                    }
                }
                self.update(s(&e["toolCallId"]), patch, &mut out);
            }
            "auto_retry_start" => {
                self.thinking_done(&mut out);
                if self
                    .stages
                    .get("retry")
                    .is_some_and(|v| v["status"] == "running")
                {
                    self.update(
                        "retry",
                        json!({"status":"failed","detail":"Attempt failed; trying again."}),
                        &mut out,
                    );
                }
                self.start(
                    "retry",
                    format!(
                        "Retrying ({}/{})",
                        e["attempt"].as_u64().unwrap_or(1),
                        e["maxAttempts"].as_u64().unwrap_or(3)
                    ),
                    json!({"detail":cut(s(&e["errorMessage"]),160)}),
                    &mut out,
                );
            }
            "auto_retry_end" => {
                let ok = e["success"] == true;
                if ok {
                    self.error = None
                } else {
                    self.error = Some(
                        e["finalError"]
                            .as_str()
                            .unwrap_or(
                                self.error
                                    .as_deref()
                                    .unwrap_or("The provider could not complete the request."),
                            )
                            .into(),
                    );
                }
                self.update(
                    "retry",
                    json!({"status":if ok{"done"}else{"failed"},"detail":self.error}),
                    &mut out,
                );
            }
            "agent_end" => {
                if e["willRetry"].is_boolean() {
                    self.modern = true;
                }
                if let Some(messages) = e["messages"].as_array() {
                    if let Some(last) = messages.iter().rev().find(|m| m["role"] == "assistant") {
                        self.snapshot(last);
                    }
                }
                if e["willRetry"] != true && !self.modern {
                    self.finish(&mut out);
                }
            }
            "agent_settled" => {
                if e["aborted"] == true {
                    self.error = Some("Agent run aborted".into());
                }
                self.finish(&mut out);
            }
            _ => {}
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modern_end_waits_for_retry_and_authoritative_snapshot() {
        let mut d = Loop::default();
        d.event(&json!({"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"partial"}}));
        d.event(&json!({"type":"agent_end","willRetry":true,"messages":[{"role":"assistant","stopReason":"error","errorMessage":"retry","content":[{"type":"text","text":"failed"}]}]}));
        assert!(!d.finished);
        d.event(&json!({"type":"turn_start"}));
        d.event(&json!({"type":"message_end","message":{"role":"assistant","stopReason":"stop","content":[{"type":"text","text":"recovered"}]}}));
        d.event(&json!({"type":"agent_end","willRetry":false}));
        assert!(!d.finished);
        d.event(&json!({"type":"agent_settled"}));
        assert_eq!(d.result().unwrap(), "recovered");
        assert!(d.event(&json!({"type":"agent_settled"})).is_empty());
    }
    #[test]
    fn tool_without_final_answer_errors() {
        let mut d = Loop::default();
        let stages=d.event(&json!({"type":"tool_execution_start","toolCallId":"a","toolName":"bash","args":{"command":"ls"}}));
        assert_eq!(stages[0]["stage"]["label"], "bash: ls");
        d.event(&json!({"type":"agent_settled"}));
        assert!(d.result().is_err());
    }
}
