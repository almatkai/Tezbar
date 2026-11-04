//! Strict OpenAI chat/completions turns. Buffered calls are never executable until
//! both a matching finish_reason and [DONE] have been received.
use super::*;
use futures_util::StreamExt;
use std::collections::BTreeMap;

pub(super) const CONTEXT_LIMIT: usize = 1024 * 1024;
const TURN_LIMIT: usize = 256 * 1024;
#[derive(Default)]
struct Call {
    id: String,
    name: String,
    arguments: String,
}
pub(super) struct Turn {
    pub text: String,
    pub calls: Vec<Value>,
}
#[derive(Default)]
struct Parser {
    lines: process::Lines,
    data: Vec<String>,
    calls: BTreeMap<u64, Call>,
    text: String,
    finish: Option<String>,
    done: bool,
    bytes: usize,
}
fn protocol(message: &str) -> String {
    format!("Native protocol: {message}")
}
impl Parser {
    fn feed(
        &mut self,
        bytes: &[u8],
        eof: bool,
        delta: &mut impl FnMut(&str),
    ) -> Result<(), String> {
        self.bytes += bytes.len();
        if self.bytes > TURN_LIMIT {
            return Err(protocol("turn exceeds 256 KiB"));
        }
        for line in self.lines.feed(bytes, eof)? {
            if line.is_empty() {
                self.flush(delta)?;
            } else if let Some(data) = line.strip_prefix("data:") {
                self.data.push(data.trim_start_matches(' ').into());
            }
        }
        if eof {
            self.flush(delta)?;
        }
        Ok(())
    }
    fn flush(&mut self, delta: &mut impl FnMut(&str)) -> Result<(), String> {
        if self.data.is_empty() {
            return Ok(());
        }
        let data = std::mem::take(&mut self.data).join("\n");
        if self.done {
            return Err(protocol("data after terminal marker"));
        }
        if data == "[DONE]" {
            if self.finish.is_none() {
                return Err(protocol("terminal marker without finish_reason"));
            }
            self.done = true;
            return Ok(());
        }
        let v: Value = serde_json::from_str(&data).map_err(|_| protocol("invalid SSE JSON"))?;
        if v.get("error").is_some() {
            return Err("Native model: provider returned a stream error".into());
        }
        let choices = v["choices"]
            .as_array()
            .ok_or_else(|| protocol("missing choices"))?;
        if choices.is_empty() {
            return Ok(());
        } // usage-only chunk
        if choices.len() != 1 || choices[0]["index"].as_u64().unwrap_or(0) != 0 {
            return Err(protocol("expected one choice"));
        }
        let c = &choices[0];
        if self.finish.is_some() {
            return Err(protocol("choice after finish_reason"));
        }
        let d = &c["delta"];
        if d.get("function_call").is_some() {
            return Err(protocol("legacy function_call is unsupported"));
        }
        if let Some(content) = d.get("content").filter(|v| !v.is_null()) {
            let text = content
                .as_str()
                .ok_or_else(|| protocol("invalid text delta"))?;
            self.text.push_str(text);
            if !text.is_empty() {
                delta(text);
            }
        }
        if let Some(tools) = d.get("tool_calls") {
            for t in tools
                .as_array()
                .ok_or_else(|| protocol("invalid tool_calls"))?
            {
                let i = t["index"]
                    .as_u64()
                    .filter(|i| *i < 16)
                    .ok_or_else(|| protocol("invalid tool index / more than 16 calls"))?;
                if t.get("type").is_some_and(|v| v != "function") {
                    return Err(protocol("unsupported tool type"));
                }
                let call = self.calls.entry(i).or_default();
                // IDs/names are metadata, not argument fragments. Repeating them
                // ambiguously is rejected rather than guessing an executable name.
                for (key, target) in [("id", &mut call.id), ("name", &mut call.name)] {
                    let source = if key == "id" {
                        &t[key]
                    } else {
                        &t["function"][key]
                    };
                    if !source.is_null() {
                        let v = source
                            .as_str()
                            .ok_or_else(|| protocol("invalid call metadata"))?;
                        if !target.is_empty() || v.is_empty() || v.len() > 128 {
                            return Err(protocol("duplicate/oversized call metadata"));
                        }
                        *target = v.into();
                    }
                }
                if let Some(a) = t["function"].get("arguments") {
                    call.arguments.push_str(
                        a.as_str()
                            .ok_or_else(|| protocol("invalid arguments delta"))?,
                    );
                    if call.arguments.len() > 65536 {
                        return Err(protocol("tool arguments exceed 64 KiB"));
                    }
                }
            }
        }
        if let Some(reason) = c["finish_reason"].as_str() {
            if !matches!(reason, "stop" | "tool_calls") {
                return Err(format!("Native model: incomplete turn ({reason})"));
            }
            self.finish = Some(reason.into());
        }
        Ok(())
    }
    fn complete(self) -> Result<Turn, String> {
        if !self.done {
            return Err(protocol(
                "stream ended without terminal marker; no tools executed",
            ));
        }
        if (self.finish.as_deref() == Some("tool_calls")) != !self.calls.is_empty() {
            return Err(protocol("finish_reason does not match tool calls"));
        }
        let mut ids = std::collections::HashSet::new();
        let mut calls = Vec::new();
        for (expected, (index, c)) in self.calls.into_iter().enumerate() {
            if index != expected as u64
                || c.id.is_empty()
                || c.name.is_empty()
                || !ids.insert(c.id.clone())
            {
                return Err(protocol("missing/duplicate call ID or noncontiguous index"));
            }
            if !c
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            {
                return Err(protocol("invalid call ID"));
            }
            let a: Value = serde_json::from_str(&c.arguments)
                .map_err(|_| protocol("invalid completed tool JSON"))?;
            if !a.is_object() {
                return Err(protocol("tool arguments must be an object"));
            }
            calls.push(json!({"id":c.id,"type":"function","function":{"name":c.name,"arguments":c.arguments}}));
        }
        if calls.is_empty() && self.text.trim().is_empty() {
            return Err("Native model: empty final answer".into());
        }
        Ok(Turn {
            text: self.text,
            calls,
        })
    }
}
impl AiCore {
    pub(super) async fn native_turn(
        &self,
        cfg: &Value,
        messages: &[Value],
        tools: &[Value],
        cancel: &Cancel,
        mut delta: impl FnMut(&str) + Send,
    ) -> Result<Turn, String> {
        let base = cfg["openaiCompatibleBaseURL"]
            .as_str()
            .filter(|b| !b.is_empty())
            .unwrap_or(s(&cfg["baseURL"]))
            .trim_end_matches('/');
        let url = if base.ends_with("/chat/completions") {
            base.to_string()
        } else {
            format!("{base}/chat/completions")
        };
        let url = reqwest::Url::parse(&url)
            .map_err(|_| "Native configuration: invalid provider base URL".to_string())?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err("Native configuration: provider URL must use http or https".into());
        }
        if s(&cfg["model"]).trim().is_empty() {
            return Err("Native configuration: model is required".into());
        }
        let mut request = self.inner.http.post(url).json(&json!({"model":cfg["model"],"messages":messages,"tools":tools,"tool_choice":"auto","stream":true}));
        if !s(&cfg["apiKey"]).is_empty() {
            request = request.bearer_auth(s(&cfg["apiKey"]));
        }
        if cfg["provider"] == "openai" && s(&cfg["apiKey"]).is_empty() {
            return Err("Native configuration: OpenAI requires an API key".into());
        }
        let response = tokio::select! {biased; _=cancel.cancelled()=>return Err("Operation cancelled".into()), r=tokio::time::timeout(Duration::from_secs(30),request.send())=>r.map_err(|_|"Native network: response headers timed out after 30s".to_string())?.map_err(|e|format!("Native network: request failed ({})",if e.is_connect(){"connection"}else{"transport"}))?};
        if !response.status().is_success() {
            // Never concatenate local stderr or echo an untrusted, possibly huge HTTP body.
            let retry = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .map(|n| format!("; Retry-After {n}s (no automatic replay)"))
                .unwrap_or_default();
            return Err(format!(
                "Native HTTP: status {}{retry}",
                response.status().as_u16()
            ));
        }
        let mut stream = response.bytes_stream();
        let mut parser = Parser::default();
        let work = async {
            loop {
                let chunk = tokio::time::timeout(Duration::from_secs(60), stream.next())
                    .await
                    .map_err(|_| "Native network: stream silent for 60s".to_string())?;
                match chunk {
                    Some(chunk) => parser.feed(
                        &chunk
                            .map_err(|_| "Native network: stream transport failed".to_string())?,
                        false,
                        &mut delta,
                    )?,
                    None => {
                        parser.feed(&[], true, &mut delta)?;
                        break;
                    }
                }
                if parser.done {
                    break;
                }
            }
            parser.complete()
        };
        tokio::select! {biased; _=cancel.cancelled()=>Err("Operation cancelled".into()), r=tokio::time::timeout(Duration::from_secs(600),work)=>r.map_err(|_|"Native network: turn exceeded 600s".to_string())?}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fragmented_utf8_and_arguments_preserve_exact_tool_identity() {
        let chunks = [
            json!({"choices":[{"delta":{"content":"🦀","tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"read","arguments":"{\"path\":"}}]}}]}),
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"🦀.txt\"}"}}]},"finish_reason":"tool_calls"}]}),
        ];
        let input = format!(
            "{}data: [DONE]\n\n",
            chunks
                .iter()
                .map(|v| format!("data: {v}\r\n\r\n"))
                .collect::<String>()
        );
        let mut parser = Parser::default();
        let mut text = String::new();
        for b in input.as_bytes() {
            parser
                .feed(&[*b], false, &mut |t| text.push_str(t))
                .unwrap();
        }
        let turn = parser.complete().unwrap();
        assert_eq!(text, "🦀");
        assert_eq!(turn.calls[0]["id"], "call_1");
        assert_eq!(
            turn.calls[0]["function"]["arguments"],
            "{\"path\":\"🦀.txt\"}"
        );
    }
}
