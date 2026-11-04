//! Gemini signature-group validation follows MIT pi-antigravity 0.11.0,
//! © Rahul Arya. See licenses/pi-antigravity-MIT.txt.
use super::*;
fn image(mime: &str, data: &str) -> Value {
    json!({"inlineData":{"mimeType":mime,"data":data}})
}
pub(super) fn encode(o: &Origin, ms: &[Message], tools: &[Value]) -> Result<Value, String> {
    let mut contents: Vec<Value> = vec![];
    let mut system = vec![];
    let requires_ids = o.model.starts_with("claude-") || o.model.starts_with("gpt-oss-");
    // This is the documented signature capability of Gemini generations, not
    // protocol routing. Never fabricate a signature for older/foreign history.
    let requires_signature = o.model.strip_prefix("gemini-").is_some_and(|model| {
        let major = model
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>();
        major.parse::<u32>().map_or(true, |major| major >= 3)
    });
    fn signed_group(raw: &Value) -> bool {
        let calls = raw
            .as_array()
            .into_iter()
            .flatten()
            .filter(|p| p.get("functionCall").is_some())
            .collect::<Vec<_>>();
        fn valid(value: &Value) -> bool {
            use base64::Engine;
            value.as_str().is_some_and(|s| {
                !s.is_empty()
                    && s.len() % 4 == 0
                    && base64::engine::general_purpose::STANDARD.decode(s).is_ok()
            })
        }
        calls.first().is_some_and(|p| valid(&p["thoughtSignature"]))
            && calls
                .iter()
                .all(|p| p.get("thoughtSignature").is_none() || valid(&p["thoughtSignature"]))
    }
    let foreign = ms
        .iter()
        .filter(|m| {
            m.role == "assistant"
                && m.raw_for(o)
                    .is_none_or(|raw| requires_signature && !signed_group(raw))
        })
        .flat_map(|m| m.calls())
        .map(|c| s(&c["callId"]).to_string())
        .collect::<HashSet<_>>();
    for m in ms {
        if m.role == "system" {
            system.push(m.text_content());
            continue;
        }
        let mut call_index = 0;
        let parts = if let Some(raw) = m.raw_for(o) {
            raw.as_array()
                .ok_or_else(|| failure("invalid saved Gemini parts"))?
                .iter()
                .filter(|part| {
                    !requires_signature || signed_group(raw) || part.get("functionCall").is_none()
                })
                .cloned()
                .map(|mut part| {
                    if requires_ids && part.get("functionCall").is_some() {
                        let calls = m.calls();
                        if let Some(call) = calls.get(call_index) {
                            part["functionCall"]["id"] = json!(wire_id(s(&call["id"])));
                        }
                        call_index += 1;
                    }
                    part
                })
                .collect()
        } else {
            let mut parts = vec![];
            for b in &m.blocks {
                match b {
                    Block::Text { text } => parts.push(json!({"text":text})),
                    Block::Image { mime, data } => parts.push(image(mime, data)),
                    Block::ToolCall { .. } => {}
                    Block::ToolResult {
                        call: canonical,
                        wire_id: id,
                        name,
                        blocks,
                        is_error,
                    } => {
                        if foreign.contains(canonical) {
                            parts.push(json!({"text":format!("[Observation from {name}]\n{}",text_blocks(blocks))}));
                        } else {
                            let text = text_blocks(blocks);
                            let mut response = json!({"functionResponse":{"name":name,"response":if *is_error { json!({"error":if text.trim().is_empty(){"Tool failed"}else{&text}}) } else { json!({"output":text}) }}});
                            let supplied = ms
                                .iter()
                                .filter(|m| m.calls().iter().any(|c| c["callId"] == *canonical))
                                .filter_map(|m| m.raw_for(o))
                                .flat_map(|v| v.as_array().into_iter().flatten())
                                .any(|p| p["functionCall"]["id"].as_str() == Some(id));
                            if requires_ids || supplied {
                                response["functionResponse"]["id"] = json!(if requires_ids {
                                    wire_id(id)
                                } else {
                                    id_for(o, ms, canonical, id)
                                });
                            }
                            parts.push(response);
                        }
                        for b in blocks {
                            if let Block::Image { mime, data } = b {
                                parts.push(image(mime, data));
                            }
                        }
                    }
                    _ => {}
                }
            }
            parts
        };
        if parts.is_empty() {
            continue;
        }
        let role = if m.role == "assistant" {
            "model"
        } else {
            "user"
        };
        if role == "model"
            && (contents.is_empty()
                || parts.iter().any(|p| p.get("functionCall").is_some())
                    && contents.last().is_some_and(|v| v["role"] == "model"))
        {
            contents.push(json!({"role":"user","parts":[{"text":"Continue from the recorded conversation."}]}));
        }
        let mut parts = parts;
        if role == "user" && parts.iter().all(|p| p.get("inlineData").is_some()) {
            parts.insert(0, json!({"text":"See the attached image."}));
        }
        if let Some(last) = contents.last_mut().filter(|v| v["role"] == role) {
            last["parts"].as_array_mut().unwrap().extend(parts)
        } else {
            contents.push(json!({"role":role,"parts":parts}));
        }
    }
    let mut v =
        json!({"contents":contents,"systemInstruction":{"parts":[{"text":system.join("\n\n")}]}});
    if !tools.is_empty() {
        v["tools"] = json!([{"functionDeclarations":tools.iter().map(|t|json!({"name":t["function"]["name"],"description":t["function"]["description"],"parametersJsonSchema":t["function"]["parameters"]})).collect::<Vec<_>>()}]);
    }
    Ok(v)
}
pub(super) fn decode(d: &mut Decoder, v: Value, on: &mut impl FnMut(&str)) -> Result<(), String> {
    let v = v.get("response").unwrap_or(&v);
    let c = &v["candidates"][0];
    for b in c["content"]["parts"].as_array().into_iter().flatten() {
        if b["thought"] != true {
            if let Some(t) = b["text"].as_str() {
                d.delta(t, on)
            }
        }
        let i = d.blocks.len() as u64;
        d.blocks.insert(i, b.clone());
    }
    if let Some(r) = c["finishReason"].as_str() {
        d.stop_reason = Some(r.into());
        d.terminal = true;
    }
    Ok(())
}
