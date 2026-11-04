use super::*;
pub(super) fn encode(o: &Origin, ms: &[Message], tools: &[Value]) -> Result<Value, String> {
    let mut messages = vec![];
    let mut followups = vec![];
    for m in ms {
        if m.role != "tool" {
            messages.append(&mut followups);
        }
        if let Some(raw) = m.raw_for(o) {
            let mut raw = raw.clone();
            if o.provider == "deepseek"
                && m.role == "assistant"
                && raw["reasoning_content"].is_null()
            {
                raw["reasoning_content"] = json!("");
            }
            messages.push(raw);
            continue;
        }
        let mut content = vec![];
        let mut calls = vec![];
        let mut results = vec![];
        for b in &m.blocks {
            match b {
            Block::Text{text}=>content.push(json!({"type":"text","text":text})),
            Block::Image{mime,data}=>content.push(json!({"type":"image_url","image_url":{"url":image_url(mime,data)}})),
            Block::ToolCall{id:canonical,wire_id:id,name,arguments,..}=>calls.push(json!({"id":id_for(o,ms,canonical,id),"type":"function","function":{"name":name,"arguments":arguments}})),
            Block::ToolResult{call:canonical,wire_id:id,name,blocks,..}=>{
                results.push(json!({"role":"tool","tool_call_id":id_for(o,ms,canonical,id),"content":text_blocks(blocks)}));
                let mut images=vec![];for b in blocks {if let Block::Image{mime,data}=b{images.push(json!({"type":"image_url","image_url":{"url":image_url(mime,data)}}));}}
                if !images.is_empty(){images.insert(0,json!({"type":"text","text":format!("Image from tool {name}")}));followups.push(json!({"role":"user","content":images}));}
            }, _=>{}
        }
        }
        if !content.is_empty() || !calls.is_empty() {
            let mut v = json!({"role":m.role,"content":if content.iter().all(|c|c["type"]=="text"){json!(content.iter().filter_map(|c|c["text"].as_str()).collect::<Vec<_>>().join("\n"))}else{json!(content)}});
            if !calls.is_empty() {
                v["tool_calls"] = json!(calls)
            }
            if o.provider == "deepseek" && m.role == "assistant" {
                v["reasoning_content"] = json!("");
            }
            messages.push(v)
        }
        messages.extend(results);
    }
    messages.append(&mut followups);
    let mut v = json!({"messages":messages,"stream":true});
    if !tools.is_empty() {
        v["tools"] = json!(tools);
        v["tool_choice"] = json!("auto");
    }
    Ok(v)
}
pub(super) fn decode(d: &mut Decoder, v: Value, on: &mut impl FnMut(&str)) -> Result<(), String> {
    let c = &v["choices"][0];
    if c.is_null() {
        return Ok(());
    }
    let delta = &c["delta"];
    if delta.get("function_call").is_some() {
        return Err(failure("legacy function_call is unsupported"));
    }
    if let Some(t) = delta["content"].as_str() {
        d.delta(t, on)
    }
    for key in ["reasoning_content", "reasoning", "reasoning_details"] {
        if let Some(v) = delta.get(key).filter(|v| !v.is_null()) {
            if let Some(t) = v.as_str() {
                let old = d.raw[key].as_str().unwrap_or("").to_string();
                d.raw[key] = json!(old + t)
            } else if key == "reasoning_details" && v.is_array() {
                let mut details = d.raw[key].as_array().cloned().unwrap_or_default();
                for entry in v.as_array().unwrap() {
                    if let Some(previous) = details.iter_mut().find(|p| {
                        (!entry["index"].is_null() || !entry["id"].is_null())
                            && p["index"] == entry["index"]
                            && p["type"] == entry["type"]
                            && (p["id"] == entry["id"] || entry["id"].is_null())
                    }) {
                        for (k, value) in entry
                            .as_object()
                            .ok_or_else(|| failure("invalid reasoning detail"))?
                        {
                            if matches!(k.as_str(), "text" | "summary" | "data" | "signature")
                                && value.is_string()
                            {
                                previous[k] = json!(format!("{}{}", s(&previous[k]), s(value)));
                            } else if !value.is_null() {
                                previous[k] = value.clone();
                            }
                        }
                    } else {
                        details.push(entry.clone());
                    }
                }
                d.raw[key] = json!(details);
            } else {
                d.raw[key] = v.clone()
            }
        }
    }
    for (index, t) in delta["tool_calls"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let i = t["index"].as_u64().unwrap_or(index as u64);
        if i >= limits::CALLS_PER_TURN as u64 {
            return Err(failure("invalid tool index"));
        }
        let call = d
            .calls
            .entry(i)
            .or_insert_with(|| json!({"type":"function","function":{}}));
        for (k, v) in t.as_object().ok_or_else(|| failure("invalid call"))? {
            if k != "function" && k != "index" {
                if k == "id" && call.get(k).is_some_and(|old| old != v) {
                    return Err(failure("changed call ID"));
                }
                call[k] = v.clone()
            }
        }
        if let Some(name) = t["function"]["name"].as_str() {
            if call["function"].get("name").is_some_and(|n| n != name) {
                return Err(failure("changed tool name"));
            }
            call["function"]["name"] = json!(name)
        }
        if let Some(a) = t["function"]["arguments"].as_str() {
            let buf = d.args.entry(i).or_default();
            if buf.len() + a.len() > limits::ARGUMENT_BYTES {
                return Err(failure("arguments exceed 1 MiB"));
            }
            buf.push_str(a)
        }
    }
    if let Some(r) = c["finish_reason"].as_str() {
        d.stop_reason = Some(r.into());
        d.terminal = true
    }
    Ok(())
}
