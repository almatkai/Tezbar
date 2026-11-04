use super::*;
pub(super) fn encode(o: &Origin, ms: &[Message], tools: &[Value]) -> Result<Value, String> {
    let mut input = vec![];
    for m in ms {
        if let Some(raw) = m.raw_for(o) {
            input.extend(
                raw.as_array()
                    .ok_or_else(|| failure("invalid saved Responses items"))?
                    .iter()
                    .cloned(),
            );
            continue;
        }
        let mut content = vec![];
        for b in &m.blocks {
            match b{
            Block::Text{text}=>content.push(json!({"type":if m.role=="assistant"{"output_text"}else{"input_text"},"text":text})),
            Block::Image{mime,data}=>content.push(json!({"type":"input_image","image_url":image_url(mime,data)})),
            Block::ToolCall{wire_id:id,name,arguments,..}=>input.push(json!({"type":"function_call","call_id":id_for(o,ms,id),"name":name,"arguments":arguments})),
            Block::ToolResult{wire_id:id,name,blocks,..}=>{input.push(json!({"type":"function_call_output","call_id":id_for(o,ms,id),"output":text_blocks(blocks)}));let mut images=vec![];for b in blocks{if let Block::Image{mime,data}=b{images.push(json!({"type":"input_image","image_url":image_url(mime,data)}));}}if !images.is_empty(){images.insert(0,json!({"type":"input_text","text":format!("Image from tool {name}")}));input.push(json!({"role":"user","content":images}));}},_=>{}
        }
        }
        if !content.is_empty() {
            input.push(json!({"role":m.role,"content":content}));
        }
    }
    let mut v = json!({"input":input,"stream":true,"store":false,"include":["reasoning.encrypted_content"]});
    if !tools.is_empty() {
        v["tools"] = json!(tools
            .iter()
            .map(|t| {
                let mut f = t["function"].clone();
                f["type"] = json!("function");
                f
            })
            .collect::<Vec<_>>());
    }
    Ok(v)
}
pub(super) fn decode(d: &mut Decoder, v: Value, on: &mut impl FnMut(&str)) -> Result<(), String> {
    let i = v["output_index"].as_u64().unwrap_or(0);
    match s(&v["type"]) {
        "response.output_text.delta" => d.delta(s(&v["delta"]), on),
        "response.output_item.added" => {
            d.blocks.insert(i, v["item"].clone());
        }
        "response.function_call_arguments.delta" => {
            d.args.entry(i).or_default().push_str(s(&v["delta"]));
        }
        "response.function_call_arguments.done" => {
            d.args.insert(i, s(&v["arguments"]).into());
        }
        "response.output_item.done" => {
            d.blocks.insert(i, v["item"].clone());
        }
        "response.completed" => {
            if let Some(items) = v["response"]["output"].as_array() {
                d.blocks = items
                    .iter()
                    .enumerate()
                    .map(|(i, b)| (i as u64, b.clone()))
                    .collect();
            }
            for (i, a) in &d.args {
                if let Some(b) = d.blocks.get_mut(i) {
                    if b["type"] == "function_call" && b["arguments"].is_null() {
                        b["arguments"] = json!(a)
                    }
                }
            }
            if d.text.is_empty() {
                let text = d
                    .blocks
                    .values()
                    .flat_map(|b| b["content"].as_array().into_iter().flatten())
                    .filter_map(|c| c["text"].as_str())
                    .collect::<String>();
                d.delta(&text, on)
            }
            d.terminal = true;
        }
        "response.failed" | "response.incomplete" | "error" => {
            return Err(failure("Responses turn failed or incomplete"))
        }
        _ => {}
    }
    Ok(())
}
