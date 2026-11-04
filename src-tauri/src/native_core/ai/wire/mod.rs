//! Pure wire adapters. Encoding and terminal validation are independent of I/O.
mod anthropic;
mod chat;
mod gemini;
mod ollama;
mod redact;
mod responses;
pub(super) mod transport;
use super::{
    err,
    process::Lines,
    runtime::{limits, transcript::*},
    s,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
pub struct Turn {
    pub message: Message,
    pub text: String,
    pub calls: Vec<Value>,
}
pub fn encode(
    origin: &Origin,
    messages: &[Message],
    tools: &[Value],
    max_tokens: u64,
    thinking: bool,
) -> Result<Value, String> {
    let mut v = match origin.protocol {
        Protocol::OpenaiChat => chat::encode(origin, messages, tools),
        Protocol::OpenaiResponses => responses::encode(origin, messages, tools),
        Protocol::Anthropic => anthropic::encode(origin, messages, tools),
        Protocol::Gemini => gemini::encode(origin, messages, tools),
        Protocol::Ollama => ollama::encode(origin, messages, tools),
    }?;
    v["model"] = json!(origin.model);
    match origin.protocol {
        Protocol::OpenaiChat => {
            v["max_tokens"] = json!(max_tokens);
        }
        Protocol::OpenaiResponses => {
            if thinking {
                v["include"] = json!(["reasoning.encrypted_content"]);
            }
            v["max_output_tokens"] = json!(max_tokens);
        }
        Protocol::Anthropic => {
            v["max_tokens"] = json!(max_tokens);
            if thinking {
                v["thinking"] = json!({"type":"adaptive"});
            }
        }
        Protocol::Ollama => {
            if thinking {
                v["think"] = json!(true);
            }
        }
        Protocol::Gemini => {
            v.as_object_mut().unwrap().remove("model");
            v["generationConfig"]["maxOutputTokens"] = json!(max_tokens);
        }
    }
    Ok(v)
}
fn text_blocks(blocks: &[Block]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b {
            Block::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn id_for(origin: &Origin, messages: &[Message], canonical: &str, id: &str) -> String {
    if messages
        .iter()
        .any(|m| m.raw_for(origin).is_some() && m.calls().iter().any(|c| c["callId"] == canonical))
    {
        id.into()
    } else {
        wire_id(canonical)
    }
}
fn image_url(mime: &str, data: &str) -> String {
    format!("data:{mime};base64,{data}")
}
fn object_args(args: &str) -> Result<Value, String> {
    let v: Value = serde_json::from_str(if args.trim().is_empty() { "{}" } else { args })
        .map_err(|_| "Native protocol: invalid completed tool JSON")?;
    if !v.is_object() {
        return Err("Native protocol: tool arguments must be an object".into());
    }
    Ok(v)
}
fn provider_stream_error(v: &Value) -> String {
    let error = if !v["response"]["error"].is_null() {
        &v["response"]["error"]
    } else {
        &v["error"]
    };
    let detail = error["message"]
        .as_str()
        .or(error.as_str())
        .or(v["message"].as_str())
        .unwrap_or("Provider returned a stream error");
    let retryable = matches!(
        error["type"].as_str(),
        Some("overloaded_error" | "server_error" | "api_error" | "rate_limit_error")
    ) || matches!(
        error["code"].as_str(),
        Some("server_error" | "overloaded_error" | "rate_limit_exceeded")
    ) || error["code"].as_u64().is_some_and(|s| s >= 500);
    format!(
        "Native model{}: {}",
        if retryable {
            " temporarily unavailable"
        } else {
            ""
        },
        super::cut(&super::redact_text(detail), 500)
    )
}
fn failure(s: &str) -> String {
    format!("Native protocol: {s}")
}
/// A decoder buffers opaque and tool blocks; only text crosses the progress seam.
pub struct Decoder {
    origin: Origin,
    lines: Lines,
    data: Vec<String>,
    bytes: usize,
    terminal: bool,
    text: String,
    raw: Value,
    closed_blocks: HashSet<u64>,
    blocks: BTreeMap<u64, Value>,
    args: BTreeMap<u64, String>,
    calls: BTreeMap<u64, Value>,
    turn: String,
    stop_reason: Option<String>,
}
impl Decoder {
    pub fn new(origin: Origin, turn: impl ToString) -> Self {
        Self {
            origin,
            lines: Lines::default(),
            data: vec![],
            bytes: 0,
            terminal: false,
            text: String::new(),
            raw: json!({}),
            blocks: BTreeMap::new(),
            closed_blocks: HashSet::new(),
            args: BTreeMap::new(),
            calls: BTreeMap::new(),
            turn: turn.to_string(),
            stop_reason: None,
        }
    }
    fn has_thinking(&self) -> bool {
        [
            "reasoning_content",
            "reasoning",
            "reasoning_details",
            "thinking",
        ]
        .iter()
        .any(|k| self.raw.get(k).is_some_and(|v| !v.is_null()))
            || self.blocks.values().any(|b| {
                matches!(
                    s(&b["type"]),
                    "thinking" | "redacted_thinking" | "reasoning"
                ) || b["thought"] == true
            })
    }
    pub fn feed(
        &mut self,
        bytes: &[u8],
        eof: bool,
        on: &mut impl FnMut(&str),
    ) -> Result<(), String> {
        self.bytes += bytes.len();
        if self.bytes > limits::STREAM_BYTES {
            return Err(failure("turn exceeds 8 MiB"));
        }
        for line in self.lines.feed(bytes, eof)? {
            if self.origin.protocol == Protocol::Ollama {
                if !line.trim().is_empty() {
                    self.payload(serde_json::from_str(&line).map_err(err)?, on)?;
                }
            } else if line.is_empty() {
                self.flush(on)?;
            } else if let Some(data) = line.strip_prefix("data:") {
                self.data.push(data.trim_start_matches(' ').into());
            }
        }
        if eof {
            self.flush(on)?;
        }
        Ok(())
    }
    pub fn json(&mut self, v: Value, on: &mut impl FnMut(&str)) -> Result<(), String> {
        match self.origin.protocol {
            Protocol::OpenaiChat => {
                let c = &v["choices"][0];
                let mut c = c.clone();
                c["delta"] = c["message"].clone();
                self.payload(json!({"choices":[c]}), on)
            }
            Protocol::OpenaiResponses => {
                self.payload(json!({"type":"response.completed","response":v}), on)
            }
            Protocol::Anthropic => {
                self.blocks = v["content"]
                    .as_array()
                    .ok_or_else(|| failure("missing content"))?
                    .iter()
                    .enumerate()
                    .map(|(i, b)| (i as u64, b.clone()))
                    .collect();
                self.closed_blocks.extend(self.blocks.keys().copied());
                self.terminal = true;
                self.stop_reason = v["stop_reason"].as_str().map(str::to_string);
                for b in self.blocks.values() {
                    if b["type"] == "text" {
                        let t = s(&b["text"]);
                        self.text.push_str(t);
                        on(t);
                    }
                }
                Ok(())
            }
            _ => self.payload(v, on),
        }
    }
    fn flush(&mut self, on: &mut impl FnMut(&str)) -> Result<(), String> {
        if self.data.is_empty() {
            return Ok(());
        }
        let data = std::mem::take(&mut self.data).join("\n");
        if data == "[DONE]" {
            if self.origin.protocol == Protocol::OpenaiChat && self.stop_reason.is_none() {
                return Err(failure("terminal marker without finish_reason"));
            }
            return Ok(());
        }
        self.payload(
            serde_json::from_str(&data).map_err(|_| failure("invalid stream JSON"))?,
            on,
        )
    }
    fn payload(&mut self, v: Value, on: &mut impl FnMut(&str)) -> Result<(), String> {
        if v.get("error").is_some_and(|e| !e.is_null()) {
            return Err(provider_stream_error(&v));
        }
        match self.origin.protocol.clone() {
            Protocol::OpenaiChat => chat::decode(self, v, on),
            Protocol::OpenaiResponses => responses::decode(self, v, on),
            Protocol::Anthropic => anthropic::decode(self, v, on),
            Protocol::Gemini => gemini::decode(self, v, on),
            Protocol::Ollama => ollama::decode(self, v, on),
        }
    }
    fn delta(&mut self, t: &str, on: &mut impl FnMut(&str)) {
        self.text.push_str(t);
        if !t.is_empty() {
            on(t)
        }
    }
    pub fn finish(mut self) -> Result<Turn, String> {
        if self.origin.protocol == Protocol::Anthropic
            && self.blocks.keys().any(|i| !self.closed_blocks.contains(i))
        {
            return Err(failure(
                "Anthropic message stopped with unfinished content blocks",
            ));
        }
        if !self.terminal {
            return Err(failure(
                "stream ended before terminal signal; no tools executed",
            ));
        }
        if self
            .stop_reason
            .as_deref()
            .is_some_and(|s| matches!(s, "length" | "max_tokens" | "MAX_TOKENS" | "content_filter"))
        {
            return Err(failure("provider returned incomplete turn"));
        }
        let mut canonical = vec![];
        match self.origin.protocol {
            Protocol::OpenaiChat | Protocol::Ollama => {
                if !self.text.is_empty() {
                    canonical.push(Block::Text {
                        text: self.text.clone(),
                    })
                }
                for (i, c) in &self.calls {
                    let args = if self.origin.protocol == Protocol::Ollama {
                        c["function"]["arguments"].to_string()
                    } else {
                        self.args
                            .get(i)
                            .cloned()
                            .unwrap_or_else(|| s(&c["function"]["arguments"]).into())
                    };
                    let args = if args.trim().is_empty() {
                        "{}".into()
                    } else {
                        args
                    };
                    object_args(&args)?;
                    canonical.push(Block::ToolCall {
                        id: format!("c{}_{}", self.turn, i),
                        wire_id: c["id"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                            .unwrap_or_else(|| format!("tz_{}_{}", self.turn, i)),
                        name: s(&c["function"]["name"]).into(),
                        arguments: args,
                    });
                }
                self.raw["role"] = json!("assistant");
                self.raw["content"] = json!(self.text);
                if !self.calls.is_empty() {
                    let mut wire_calls = vec![];
                    for b in &canonical {
                        if let Block::ToolCall {
                            wire_id,
                            name,
                            arguments,
                            ..
                        } = b
                        {
                            let args = if self.origin.protocol == Protocol::Ollama {
                                object_args(arguments)?
                            } else {
                                json!(arguments)
                            };
                            wire_calls.push(if self.origin.protocol==Protocol::Ollama{json!({"function":{"name":name,"arguments":args}})}else{json!({"id":wire_id,"type":"function","function":{"name":name,"arguments":args}})});
                        }
                    }
                    self.raw["tool_calls"] = json!(wire_calls);
                    // Retain unknown call-level fields from the source.
                    for ((_, source), dest) in self
                        .calls
                        .iter()
                        .zip(self.raw["tool_calls"].as_array_mut().unwrap())
                    {
                        for (k, v) in source.as_object().ok_or_else(|| failure("invalid call"))? {
                            if k != "index" && k != "function" && k != "id" {
                                dest[k] = v.clone();
                            }
                        }
                    }
                }
            }
            _ => {
                for (i, b) in &self.blocks {
                    match (self.origin.protocol.clone(), s(&b["type"])) {
                        (Protocol::Anthropic, "text") => canonical.push(Block::Text {
                            text: s(&b["text"]).into(),
                        }),
                        (Protocol::Anthropic, "thinking") => canonical.push(Block::Thinking {
                            text: s(&b["thinking"]).into(),
                        }),
                        (Protocol::Anthropic, "redacted_thinking") => {}
                        (Protocol::Anthropic, "tool_use") => {
                            let a = if let Some(a) = self.args.get(i) {
                                object_args(a)?
                            } else {
                                b["input"].clone()
                            };
                            if !a.is_object() {
                                return Err(failure("invalid tool input"));
                            }
                            canonical.push(Block::ToolCall {
                                id: format!("c{}_{}", self.turn, i),
                                wire_id: s(&b["id"]).into(),
                                name: s(&b["name"]).into(),
                                arguments: self
                                    .args
                                    .get(i)
                                    .cloned()
                                    .filter(|s| !s.trim().is_empty())
                                    .unwrap_or_else(|| a.to_string()),
                            });
                        }
                        (Protocol::OpenaiResponses, "function_call") => {
                            let a = if s(&b["arguments"]).trim().is_empty() {
                                "{}"
                            } else {
                                s(&b["arguments"])
                            };
                            object_args(a)?;
                            canonical.push(Block::ToolCall {
                                id: format!("c{}_{}", self.turn, i),
                                wire_id: s(&b["call_id"]).into(),
                                name: s(&b["name"]).into(),
                                arguments: a.into(),
                            });
                        }
                        (Protocol::OpenaiResponses, "message") => {
                            for c in b["content"].as_array().into_iter().flatten() {
                                if let Some(t) = c["text"].as_str() {
                                    canonical.push(Block::Text { text: t.into() })
                                }
                            }
                        }
                        (Protocol::OpenaiResponses, "reasoning") => {}
                        (Protocol::Gemini, _) => {
                            if let Some(t) = b["text"].as_str() {
                                canonical.push(if b["thought"] == true {
                                    Block::Thinking { text: t.into() }
                                } else {
                                    Block::Text { text: t.into() }
                                })
                            }
                            if let Some(f) = b.get("functionCall") {
                                if !f["args"].is_null() && !f["args"].is_object() {
                                    return Err(failure("invalid functionCall args"));
                                }
                                canonical.push(Block::ToolCall {
                                    id: format!("c{}_{}", self.turn, i),
                                    wire_id: f["id"]
                                        .as_str()
                                        .unwrap_or(&format!("tz_{}_{}", self.turn, i))
                                        .into(),
                                    name: s(&f["name"]).into(),
                                    arguments: if f["args"].is_null() {
                                        "{}".into()
                                    } else {
                                        f["args"].to_string()
                                    },
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        if self.origin.protocol == Protocol::Anthropic {
            for (i, a) in self.args {
                self.blocks
                    .get_mut(&i)
                    .ok_or_else(|| failure("orphan tool delta"))?["input"] = object_args(&a)?;
            }
        }
        if !matches!(
            self.origin.protocol,
            Protocol::OpenaiChat | Protocol::Ollama
        ) {
            self.raw = json!(self.blocks.into_values().collect::<Vec<_>>());
        }
        let message = Message {
            role: "assistant".into(),
            blocks: canonical,
            opaque: Some(Opaque {
                origin: self.origin,
                value: self.raw,
            }),
        };
        let calls = message.calls();
        if calls.len() > limits::CALLS_PER_TURN {
            return Err(failure("more than 32 calls"));
        }
        let mut ids = HashSet::new();
        for c in &calls {
            if s(&c["function"]["name"]).is_empty()
                || !ids.insert(s(&c["id"]).to_string())
                || s(&c["function"]["arguments"]).len() > limits::ARGUMENT_BYTES
            {
                return Err(failure("missing name, duplicate ID or oversized arguments"));
            }
        }
        if calls.is_empty() && self.text.trim().is_empty() {
            return Err("Native model: empty final answer".into());
        }
        Ok(Turn {
            text: self.text,
            calls,
            message,
        })
    }
}

#[cfg(test)]
mod tests;
