use super::*;
fn origin(protocol: Protocol) -> Origin {
    Origin {
        protocol,
        provider: "fixture".into(),
        model: "fixture-model".into(),
    }
}
fn stream(protocol: Protocol, events: Vec<Value>) -> Turn {
    let mut d = Decoder::new(origin(protocol.clone()), 7);
    let mut visible = String::new();
    let data = events
        .iter()
        .map(|v| {
            if protocol == Protocol::Ollama {
                format!("{v}\n")
            } else {
                format!("data: {v}\r\n\r\n")
            }
        })
        .collect::<String>();
    for byte in data.as_bytes() {
        d.feed(&[*byte], false, &mut |t| visible.push_str(t))
            .unwrap()
    }
    d.feed(&[], true, &mut |t| visible.push_str(t)).unwrap();
    let t = d.finish().unwrap();
    assert_eq!(visible, t.text);
    t
}
#[test]
fn chat_preserves_reasoning_extra_content_and_missing_ids() {
    let t = stream(
        Protocol::OpenaiChat,
        vec![
            json!({"choices":[{"delta":{"content":"🦀","reasoning_content":"private","tool_calls":[{"index":0,"function":{"name":"read","arguments":"{\"path\":\"a\"}"},"extra_content":{"signature":"fixture"}}]},"finish_reason":"stop"}]}),
        ],
    );
    assert_eq!(t.calls[0]["id"], "tz_7_0");
    let body = encode(
        &origin(Protocol::OpenaiChat),
        &[t.message.clone()],
        &[],
        42,
        false,
    )
    .unwrap();
    assert_eq!(body["messages"][0]["reasoning_content"], "private");
    assert_eq!(
        body["messages"][0]["tool_calls"][0]["extra_content"]["signature"],
        "fixture"
    );
    let mut other = origin(Protocol::OpenaiChat);
    other.model = "other".into();
    let body = encode(&other, &[t.message], &[], 42, false).unwrap();
    assert!(body["messages"][0].get("reasoning_content").is_none());
    assert!(body["messages"][0]["tool_calls"][0]
        .get("extra_content")
        .is_none());
}
#[test]
fn anthropic_signed_blocks_are_replayed_in_original_order() {
    let t = stream(
        Protocol::Anthropic,
        vec![
            json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"private"}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"signed"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"redacted_thinking","data":"redacted"}}),
            json!({"type":"content_block_stop","index":1}),
            json!({"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"abc","name":"read","input":{}}}),
            json!({"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"path\":\"a\"}"}}),
            json!({"type":"content_block_stop","index":2}),
            json!({"type":"message_stop"}),
        ],
    );
    let body = encode(&origin(Protocol::Anthropic), &[t.message], &[], 42, false).unwrap();
    assert_eq!(
        body["messages"][0]["content"],
        json!([{"type":"thinking","thinking":"private","signature":"signed"},{"type":"redacted_thinking","data":"redacted"},{"type":"tool_use","id":"abc","name":"read","input":{"path":"a"}}])
    );
    assert!(body.get("thinking").is_none());
}
#[test]
fn responses_keeps_encrypted_reasoning_and_call_identity() {
    let raw = json!([{"type":"reasoning","id":"rs_1","encrypted_content":"fixture-cipher"},{"type":"function_call","id":"fc_1","call_id":"call_1","name":"read","arguments":"{}"}]);
    let t = stream(
        Protocol::OpenaiResponses,
        vec![json!({"type":"response.completed","response":{"output":raw}})],
    );
    assert_eq!(t.calls[0]["id"], "call_1");
    let body = encode(
        &origin(Protocol::OpenaiResponses),
        &[t.message],
        &[],
        42,
        false,
    )
    .unwrap();
    assert_eq!(body["store"], false);
    assert_eq!(body["input"], raw);
}
#[test]
fn gemini_keeps_signed_part_boundaries_and_foreign_observations() {
    let parts = json!([{"text":"🦀","thoughtSignature":"text-sig"},{"functionCall":{"name":"read","args":{},"id":"a"},"thoughtSignature":"call-sig"},{"functionCall":{"name":"read","args":{},"id":"b"}}]);
    let t = stream(
        Protocol::Gemini,
        vec![
            json!({"response":{"candidates":[{"content":{"parts":parts},"finishReason":"STOP"}]}}),
        ],
    );
    let body = encode(
        &origin(Protocol::Gemini),
        &[t.message.clone()],
        &[],
        42,
        false,
    )
    .unwrap();
    assert_eq!(body["contents"][0]["parts"], parts);
    let result = Message {
        role: "tool".into(),
        blocks: vec![Block::ToolResult {
            call: "c7_1".into(),
            wire_id: "a".into(),
            name: "read".into(),
            blocks: vec![Block::Text {
                text: "observed".into(),
            }],
            is_error: false,
        }],
        opaque: None,
    };
    let mut other = origin(Protocol::Gemini);
    other.model = "other".into();
    let body = encode(&other, &[t.message, result], &[], 42, false).unwrap();
    assert!(!body.to_string().contains("thoughtSignature"));
    assert!(!body.to_string().contains("functionCall"));
    assert!(body.to_string().contains("Observation from read"));
}
#[test]
fn ollama_native_thinking_tool_arguments_and_done() {
    let t = stream(
        Protocol::Ollama,
        vec![
            json!({"message":{"thinking":"private","tool_calls":[{"function":{"name":"read","arguments":{"path":"a"}}}]},"done":true}),
        ],
    );
    let body = encode(&origin(Protocol::Ollama), &[t.message], &[], 42, false).unwrap();
    assert_eq!(body["messages"][0]["thinking"], "private");
    assert_eq!(
        body["messages"][0]["tool_calls"][0]["function"]["arguments"],
        json!({"path":"a"})
    );
    assert!(body.get("think").is_none());
}
#[test]
fn every_protocol_requires_terminal_signal() {
    for p in [
        Protocol::OpenaiChat,
        Protocol::OpenaiResponses,
        Protocol::Anthropic,
        Protocol::Gemini,
        Protocol::Ollama,
    ] {
        assert!(Decoder::new(origin(p), 0)
            .finish()
            .err()
            .unwrap()
            .contains("terminal"));
    }
}
#[test]
fn duplicate_ids_and_non_object_arguments_fail_before_execution() {
    for args in ["[]", "{bad"] {
        let mut d = Decoder::new(origin(Protocol::OpenaiChat), 0);
        d.json(json!({"choices":[{"message":{"tool_calls":[{"id":"same","function":{"name":"read","arguments":args}}]},"finish_reason":"tool_calls"}]}),&mut |_|{}).unwrap();
        assert!(d.finish().is_err());
    }
}
#[test]
fn every_encoder_places_user_and_tool_images_without_flattening() {
    let image = Block::Image {
        mime: "image/png".into(),
        data: "Zml4dHVyZQ==".into(),
    };
    let user = Message {
        role: "user".into(),
        blocks: vec![image.clone()],
        opaque: None,
    };
    let result = Message {
        role: "tool".into(),
        blocks: vec![Block::ToolResult {
            call: "c".into(),
            wire_id: "wire".into(),
            name: "capture".into(),
            blocks: vec![
                Block::Text {
                    text: "image".into(),
                },
                image,
            ],
            is_error: false,
        }],
        opaque: None,
    };
    for p in [
        Protocol::OpenaiChat,
        Protocol::OpenaiResponses,
        Protocol::Anthropic,
        Protocol::Gemini,
        Protocol::Ollama,
    ] {
        let body = encode(
            &origin(p.clone()),
            &[user.clone(), result.clone()],
            &[],
            42,
            false,
        )
        .unwrap();
        assert_eq!(body.to_string().matches("Zml4dHVyZQ==").count(), 2, "{p:?}");
    }
}
