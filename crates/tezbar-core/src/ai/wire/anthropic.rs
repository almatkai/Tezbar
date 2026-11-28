use super::*;
fn image(mime: &str, data: &str) -> Value {
    json!({"type":"image","source":{"type":"base64","media_type":mime,"data":data}})
}
pub(super) fn encode(o: &Origin, ms: &[Message], tools: &[Value]) -> Result<Value, String> {
    let mut messages: Vec<Value> = vec![];
    let mut system = vec![];
    for m in ms {
        if m.role == "system" {
            system.push(m.text_content());
            continue;
        }
        let content = if let Some(raw) = m.raw_for(o) {
            raw.as_array()
                .ok_or_else(|| failure("invalid saved Anthropic blocks"))?
                .iter()
                .filter(|b| b["type"] != "text" || !s(&b["text"]).trim().is_empty())
                .cloned()
                .collect()
        } else {
            let mut content = vec![];
            for b in &m.blocks {
                match b{
            Block::Text{text} if !text.trim().is_empty()=>content.push(json!({"type":"text","text":text})),Block::Image{mime,data}=>content.push(image(mime,data)),
            Block::ToolCall{id:canonical,wire_id:id,name,arguments,..}=>content.push(json!({"type":"tool_use","id":id_for(o,ms,canonical,id),"name":name,"input":object_args(arguments)?})),
            Block::ToolResult{call:canonical,wire_id:id,blocks,is_error,..}=>{let mut c=vec![];for b in blocks{match b{Block::Text{text} if !text.trim().is_empty()=>c.push(json!({"type":"text","text":text})),Block::Image{mime,data}=>c.push(image(mime,data)),_=>{}}}content.push(json!({"type":"tool_result","tool_use_id":id_for(o,ms,canonical,id),"is_error":is_error,"content":c}));},_=>{}
        }
            }
            content
        };
        if content.is_empty() {
            continue;
        }
        let role = if m.role == "assistant" {
            "assistant"
        } else {
            "user"
        };
        if let Some(last) = messages.last_mut().filter(|last| last["role"] == role) {
            last["content"].as_array_mut().unwrap().extend(content)
        } else {
            messages.push(json!({"role":role,"content":content}))
        }
    }
    if messages.first().is_some_and(|m| m["role"] == "assistant") {
        messages.insert(0,json!({"role":"user","content":[{"type":"text","text":"[Earlier user message unavailable. Continue from the recorded conversation.]"}]}));
    }
    let tools=tools.iter().map(|t|json!({"name":t["function"]["name"],"description":t["function"]["description"],"input_schema":t["function"]["parameters"]})).collect::<Vec<_>>();
    let mut v = json!({"messages":messages,"system":system.join("\n\n"),"stream":true});
    if !tools.is_empty() {
        v["tools"] = json!(tools)
    }
    Ok(v)
}
pub(super) fn decode(d: &mut Decoder, v: Value, on: &mut impl FnMut(&str)) -> Result<(), String> {
    let i = v["index"].as_u64().unwrap_or(0);
    match s(&v["type"]) {
        "content_block_start" => {
            if d.blocks.contains_key(&i) {
                return Err(failure("duplicate Anthropic block index"));
            }
            if i > 4096 {
                return Err(failure("too many Anthropic content blocks"));
            }
            let b = v["content_block"].clone();
            if b["type"] == "text" {
                d.delta(s(&b["text"]), on)
            }
            d.blocks.insert(i, b);
        }
        "content_block_delta" => {
            if d.closed_blocks.contains(&i) {
                return Err(failure("delta after content_block_stop"));
            }
            let delta = &v["delta"];
            if delta["type"] == "text_delta" {
                d.delta(s(&delta["text"]), on)
            }
            let b = d
                .blocks
                .get_mut(&i)
                .ok_or_else(|| failure("delta without block"))?;
            let key = match s(&delta["type"]) {
                "text_delta" => "text",
                "thinking_delta" => "thinking",
                "signature_delta" => "signature",
                "input_json_delta" => {
                    d.args
                        .entry(i)
                        .or_default()
                        .push_str(s(&delta["partial_json"]));
                    return Ok(());
                }
                _ => return Ok(()),
            };
            b[key] = json!(format!("{}{}", s(&b[key]), s(&delta[key])));
        }
        "content_block_stop" => {
            if !d.blocks.contains_key(&i) || !d.closed_blocks.insert(i) {
                return Err(failure("invalid Anthropic block stop"));
            }
        }
        "message_delta" => {
            d.stop_reason = v["delta"]["stop_reason"].as_str().map(str::to_string);
        }
        "message_stop" => d.terminal = true,
        "error" => return Err(failure("Anthropic stream error")),
        _ => {}
    }
    Ok(())
}
