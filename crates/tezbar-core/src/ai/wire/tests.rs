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
        body["messages"][1]["content"],
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
    assert_eq!(body["contents"][1]["parts"], parts);
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

#[test]
fn gemini_three_unsigned_same_origin_history_becomes_observation_without_losing_images() {
    let origin = Origin {
        protocol: Protocol::Gemini,
        provider: "gemini".into(),
        model: "gemini-3.5-flash".into(),
    };
    let mut decoder = Decoder::new(origin.clone(), "unsigned");
    let data = format!(
        "data: {}\n\n",
        json!({"candidates":[{"content":{"parts":[{"functionCall":{"name":"read","id":"reused","args":{}}}]},"finishReason":"STOP"}]})
    );
    decoder.feed(data.as_bytes(), true, &mut |_| {}).unwrap();
    let turn = decoder.finish().unwrap();
    let result = Message {
        role: "tool".into(),
        blocks: vec![Block::ToolResult {
            call: s(&turn.calls[0]["callId"]).into(),
            wire_id: "reused".into(),
            name: "read".into(),
            blocks: vec![
                Block::Text {
                    text: "seen".into(),
                },
                Block::Image {
                    mime: "image/png".into(),
                    data: "fixture".into(),
                },
            ],
            is_error: false,
        }],
        opaque: None,
    };
    let body = encode(&origin, &[turn.message, result], &[], 42, false).unwrap();
    assert!(!body.to_string().contains("functionCall"));
    assert!(!body.to_string().contains("functionResponse"));
    assert!(body.to_string().contains("Observation from read"));
    assert!(body.to_string().contains("inlineData"));
}

#[test]
fn anthropic_terminal_does_not_complete_an_unclosed_tool_block_but_json_is_complete() {
    let mut decoder = Decoder::new(origin(Protocol::Anthropic), "incomplete");
    for event in [
        json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call","name":"write","input":{}}}),
        json!({"type":"message_stop"}),
    ] {
        decoder
            .feed(format!("data: {event}\n\n").as_bytes(), false, &mut |_| {})
            .unwrap();
    }
    assert!(decoder
        .finish()
        .err()
        .unwrap()
        .contains("unfinished content blocks"));
    let mut decoder = Decoder::new(origin(Protocol::Anthropic), "complete-json");
    decoder.json(json!({"content":[{"type":"tool_use","id":"call","name":"read","input":{}}],"stop_reason":"tool_use"}),&mut |_|{}).unwrap();
    assert_eq!(decoder.finish().unwrap().calls.len(), 1);
}

#[test]
fn empty_tool_json_null_reasoning_and_completed_empty_responses_are_valid() {
    let chat = stream(
        Protocol::OpenaiChat,
        vec![
            json!({"choices":[{"delta":{"reasoning_content":"one","reasoning_details":[{"type":"reasoning.text","index":0,"text":"one"}]}}]}),
            json!({"choices":[{"delta":{"reasoning_content":null,"reasoning_details":[{"type":"reasoning.text","index":0,"text":"two"}],"tool_calls":[{"index":0,"id":"empty","function":{"name":"screen_capture","arguments":""}}]},"finish_reason":"tool_calls"}]}),
        ],
    );
    assert_eq!(chat.calls[0]["function"]["arguments"], "{}");
    let raw = chat.message.opaque.as_ref().unwrap();
    assert_eq!(raw.value["reasoning_content"], "one");
    assert_eq!(raw.value["reasoning_details"][0]["text"], "onetwo");
    let mut deep = origin(Protocol::OpenaiChat);
    deep.provider = "deepseek".into();
    let mut msg = chat.message.clone();
    msg.opaque.as_mut().unwrap().origin = deep.clone();
    msg.opaque.as_mut().unwrap().value["reasoning_content"] = Value::Null;
    assert_eq!(
        encode(&deep, &[msg], &[], 10, false).unwrap()["messages"][0]["reasoning_content"],
        ""
    );
    let anthropic = stream(
        Protocol::Anthropic,
        vec![
            json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"empty","name":"screen_capture","input":{}}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":""}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"message_stop"}),
        ],
    );
    assert_eq!(anthropic.calls[0]["function"]["arguments"], "{}");
    let responses = stream(
        Protocol::OpenaiResponses,
        vec![
            json!({"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","call_id":"empty","name":"screen_capture","arguments":""}}),
            json!({"type":"response.completed","response":{"output":[]}}),
        ],
    );
    assert_eq!(responses.calls.len(), 1);
    assert_eq!(responses.calls[0]["function"]["arguments"], "{}");
    let gemini = stream(
        Protocol::Gemini,
        vec![
            json!({"error":null,"candidates":[{"content":{"parts":[{"functionCall":{"name":"screen_capture"}}]},"finishReason":"STOP"}]}),
        ],
    );
    assert_eq!(gemini.calls[0]["function"]["arguments"], "{}");
}
#[test]
fn chat_tool_results_stay_contiguous_before_image_followups_across_messages() {
    let assistant = stream(
        Protocol::OpenaiChat,
        vec![
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"a","function":{"name":"screen_capture","arguments":"{}"}},{"index":1,"id":"b","function":{"name":"read","arguments":"{}"}}]},"finish_reason":"tool_calls"}]}),
        ],
    );
    let mut messages = vec![Message::text("user", "inspect"), assistant.message];
    for (i, c) in assistant.calls.iter().enumerate() {
        messages.push(Message {
            role: "tool".into(),
            opaque: None,
            blocks: vec![Block::ToolResult {
                call: s(&c["callId"]).into(),
                wire_id: s(&c["id"]).into(),
                name: s(&c["function"]["name"]).into(),
                blocks: if i == 0 {
                    vec![Block::Image {
                        mime: "image/png".into(),
                        data: "fixture".into(),
                    }]
                } else {
                    vec![Block::Text {
                        text: "result".into(),
                    }]
                },
                is_error: false,
            }],
        });
    }
    let encoded = encode(&origin(Protocol::OpenaiChat), &messages, &[], 10, false).unwrap();
    assert_eq!(
        encoded["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| s(&m["role"]))
            .collect::<Vec<_>>(),
        vec!["user", "assistant", "tool", "tool", "user"]
    );
}
#[test]
fn incomplete_tool_json_is_labelled_before_parsing() {
    let mut d = Decoder::new(origin(Protocol::OpenaiChat), 0);
    d.json(json!({"choices":[{"message":{"tool_calls":[{"id":"a","function":{"name":"read","arguments":"{"}}]},"finish_reason":"length"}]}),&mut |_|{}).unwrap();
    assert!(d.finish().err().unwrap().contains("incomplete turn"));
}

#[test]
fn gemini_family_ids_and_error_results_are_correlated_without_synthesized_gemini_ids() {
    for model in ["claude-fixture", "gpt-oss-fixture", "gemini-2.5-flash"] {
        let mut o = origin(Protocol::Gemini);
        o.model = model.into();
        let mut d = Decoder::new(o.clone(), 1);
        d.json(json!({"candidates":[{"content":{"parts":[{"functionCall":{"name":"read","args":{}}},{"functionCall":{"name":"read","args":{}}}]},"finishReason":"STOP"}]}),&mut |_|{}).unwrap();
        let turn = d.finish().unwrap();
        let mut messages = vec![Message::text("user", "inspect"), turn.message];
        for call in turn.calls {
            messages.push(Message {
                role: "tool".into(),
                opaque: None,
                blocks: vec![Block::ToolResult {
                    call: s(&call["callId"]).into(),
                    wire_id: s(&call["id"]).into(),
                    name: "read".into(),
                    blocks: vec![],
                    is_error: true,
                }],
            });
        }
        let body = encode(&o, &messages, &[], 10, false).unwrap();
        let calls = body["contents"][1]["parts"].as_array().unwrap();
        let results = body["contents"][2]["parts"].as_array().unwrap();
        for i in 0..2 {
            assert_eq!(
                results[i]["functionResponse"]["response"]["error"],
                "Tool failed"
            );
            if model.starts_with("gemini") {
                assert!(calls[i]["functionCall"].get("id").is_none());
                assert!(results[i]["functionResponse"].get("id").is_none());
            } else {
                assert_eq!(
                    calls[i]["functionCall"]["id"],
                    results[i]["functionResponse"]["id"]
                );
                assert_ne!(
                    calls[0]["functionCall"]["id"],
                    calls[1]["functionCall"]["id"]
                );
            }
        }
    }
}
#[test]
fn anthropic_saved_empty_tool_text_is_omitted_and_base64_image_limit_is_current() {
    let message = Message {
        role: "tool".into(),
        opaque: None,
        blocks: vec![Block::ToolResult {
            call: "c".into(),
            wire_id: "c".into(),
            name: "empty".into(),
            blocks: vec![Block::Text { text: "".into() }],
            is_error: false,
        }],
    };
    let body = encode(&origin(Protocol::Anthropic), &[message], &[], 10, false).unwrap();
    assert_eq!(body["messages"][0]["content"][0]["content"], json!([]));
    let image = Message {
        role: "user".into(),
        opaque: None,
        blocks: vec![Block::Image {
            mime: "image/png".into(),
            data: "a".repeat(10_000_001),
        }],
    };
    assert!(
        limits::validate_images(&Protocol::Anthropic, &[image.clone()])
            .unwrap_err()
            .contains("10 MB base64")
    );
    assert!(limits::validate_images(&Protocol::Gemini, &[image]).is_ok());
}
