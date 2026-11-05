use super::*;
pub(super) fn encode(o: &Origin, ms: &[Message], tools: &[Value]) -> Result<Value, String> {
    let mut messages = vec![];
    for m in ms {
        if let Some(raw) = m.raw_for(o) {
            messages.push(raw.clone());
            continue;
        }
        let mut v = json!({"role":m.role,"content":m.text_content()});
        let mut images = vec![];
        let mut calls = vec![];
        for b in &m.blocks {
            match b {
                Block::Image { data, .. } => images.push(json!(data)),
                Block::ToolCall {
                    name, arguments, ..
                } => calls
                    .push(json!({"function":{"name":name,"arguments":object_args(arguments)?}})),
                Block::ToolResult { name, blocks, .. } => {
                    messages.push(
                        json!({"role":"tool","tool_name":name,"content":text_blocks(blocks)}),
                    );
                    let imgs = blocks
                        .iter()
                        .filter_map(|b| {
                            if let Block::Image { data, .. } = b {
                                Some(json!(data))
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    if !imgs.is_empty() {
                        messages.push(json!({"role":"user","content":format!("Image from tool {name}"),"images":imgs}));
                    }
                }
                _ => {}
            }
        }
        if !images.is_empty() {
            v["images"] = json!(images)
        }
        if !calls.is_empty() {
            v["tool_calls"] = json!(calls)
        }
        if !m
            .blocks
            .iter()
            .all(|b| matches!(b, Block::ToolResult { .. }))
        {
            messages.push(v)
        }
    }
    let mut v = json!({"messages":messages,"stream":true});
    if !tools.is_empty() {
        v["tools"] = json!(tools)
    }
    Ok(v)
}
pub(super) fn decode(d: &mut Decoder, v: Value, on: &mut impl FnMut(&str)) -> Result<(), String> {
    let m = &v["message"];
    if let Some(t) = m["content"].as_str() {
        d.delta(t, on)
    }
    if let Some(t) = m["thinking"].as_str() {
        d.raw["thinking"] = json!(format!("{}{}", s(&d.raw["thinking"]), t))
    }
    for c in m["tool_calls"].as_array().into_iter().flatten() {
        let i = d.calls.len() as u64;
        d.calls.insert(i, c.clone());
    }
    if v["done"] == true {
        d.terminal = true;
        d.stop_reason = v["done_reason"].as_str().map(str::to_string)
    }
    Ok(())
}
