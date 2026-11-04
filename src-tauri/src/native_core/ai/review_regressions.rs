use super::fixture_http::{server, Reply};
use super::runtime::transcript::{Block, Message};
use super::tests::fixture;
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
fn sse(events: Vec<Value>) -> Reply {
    let mut r = Reply::json(json!({}));
    r.content_type = "text/event-stream".into();
    r.body = events
        .iter()
        .map(|e| format!("data: {e}\n\n"))
        .collect::<String>();
    r
}
fn user() -> Vec<Message> {
    vec![Message::text("user", "fixture")]
}
fn tool(call: &str, wire: &str, name: &str) -> Message {
    Message {
        role: "tool".into(),
        opaque: None,
        blocks: vec![Block::ToolResult {
            call: call.into(),
            wire_id: wire.into(),
            name: name.into(),
            blocks: vec![Block::Text { text: "ok".into() }],
            is_error: false,
        }],
    }
}

#[tokio::test]
async fn regression_w_t02_responses_reasoning_without_encrypted_replay() {
    let (_d, core, _e) = fixture();
    let bodies = Arc::new(Mutex::new(Vec::<Value>::new()));
    let b = bodies.clone();
    let http = server(move |r| { b.lock().unwrap().push(r["body"].clone()); sse(vec![
        json!({"type":"response.completed","response":{"output":[{"type":"reasoning","id":"rs_1","summary":[]},{"type":"function_call","id":"fc_1","call_id":"call_1","name":"read","arguments":"{}"}]}}),
    ])}).await;
    for (label, raw) in [
        (
            "openai discovered gpt-6-sol",
            json!({"provider":"openai","protocol":"openai-responses","model":"gpt-6-sol","providerConfigs":{"openai":{"apiKey":"fixture-k","baseURL":http.url,"protocol":"openai-responses"}},"providerModels":{"openai":[{"id":"gpt-6-sol"}]}}),
        ),
        (
            "opencode gpt-6-sol default list",
            json!({"provider":"opencode","model":"opencode/gpt-6-sol","providerConfigs":{"opencode":{"apiKey":"fixture-k","baseURL":http.url}}}),
        ),
    ] {
        let cfg = config::selected(&raw, None, None);
        bodies.lock().unwrap().clear();
        let t = core
            .canonical_turn(&cfg, &user(), &[], 0, &Cancel::new(), |_| {})
            .await
            .unwrap();
        let call = t.calls[0].clone();
        let mut ms = user();
        ms.push(t.message.clone());
        ms.push(tool(s(&call["callId"]), s(&call["id"]), "read"));
        let _ = core
            .canonical_turn(&cfg, &ms, &[], 1, &Cancel::new(), |_| {})
            .await;
        let got = bodies.lock().unwrap().clone();
        assert_eq!(
            got[0]["include"],
            json!(["reasoning.encrypted_content"]),
            "{label}"
        );
        assert_eq!(got[0]["store"], false);
        assert!(!got[1]["input"].to_string().contains("rs_1"));
        assert!(!got[1]["input"].to_string().contains("fc_1"));
        assert!(got[1]["input"].to_string().contains("call_1"));
    }
    core.shutdown().await;
}

#[tokio::test]
async fn regression_w_t03_thinking_checkbox_routes() {
    let (_d, core, _e) = fixture();
    let bodies = Arc::new(Mutex::new(Vec::<Value>::new()));
    let b = bodies.clone();
    let http = server(move |r| {
        b.lock().unwrap().push(r["body"].clone());
        Reply::json(json!({"error":"\"llama3.2:latest\" does not support thinking"})).status(400)
    })
    .await;
    let raw = json!({"provider":"ollama","thinking":true,"providerConfigs":{"ollama":{"baseURL":http.url}}});
    let cfg = config::selected(&raw, None, None);
    let e = core
        .canonical_turn(&cfg, &user(), &[], 0, &Cancel::new(), |_| {})
        .await
        .err();
    assert_eq!(cfg["model"], "llama3.2");
    assert!(bodies.lock().unwrap()[0]["think"].is_null());
    assert!(e.is_some());
    bodies.lock().unwrap().clear();
    let raw = json!({"provider":"anthropic","thinking":true,"providerConfigs":{"anthropic":{"baseURL":http.url,"apiKey":"fixture-k"}}});
    let cfg = config::selected(&raw, None, None);
    let _ = core
        .canonical_turn(&cfg, &user(), &[], 0, &Cancel::new(), |_| {})
        .await;
    assert_eq!(bodies.lock().unwrap()[0]["thinking"]["type"], "enabled");
    assert!(
        bodies.lock().unwrap()[0]["thinking"]["budget_tokens"]
            .as_u64()
            .unwrap()
            >= 1024
    );
    core.shutdown().await;
}

#[tokio::test]
async fn regression_w_t04_opencode_go_only_key() {
    let (_d, core, _e) = fixture();
    let dir = core.inner.ctx.paths.home_dir.join(".local/share/opencode");
    std::fs::create_dir_all(&dir).unwrap();
    common::write_json_atomic(
        &dir.join("auth.json"),
        &json!({"opencode-go":{"key":"fixture-go"}}),
    )
    .unwrap();
    core.patch_config(json!({"provider":"opencode","model":"opencode-go/glm-5.3"}))
        .await
        .unwrap();
    let st = core
        .handle("llm-provider-statuses", &[])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        core.handle("llm-config-get", &[]).await.unwrap().unwrap()["nativeProviderCredentials"]
            ["opencode"]["hasValue"],
        true
    );
    let raw = core.read_config().await.unwrap();
    let cfg = config::selected(&raw, None, None);
    let mut c2 = cfg.clone();
    c2["providerModels"] =
        json!({"opencode":[{"id":"opencode-go/glm-5.3","protocol":"openai-chat"}]});
    let r = core.native_route(&c2, &[]).await;
    assert_eq!(st["opencode"], true);
    assert!(r
        .unwrap()
        .url
        .as_str()
        .contains("/inference/go/openai/v1/chat/completions"));
    let m = core
        .handle("llm-list-models", &[json!("opencode")])
        .await
        .unwrap()
        .unwrap();
    assert!(m
        .as_array()
        .unwrap()
        .iter()
        .any(|v| s(v).starts_with("opencode-go/")));
    // Zen + Go key: the live Zen list replaces the static catalog
    common::write_json_atomic(
        &dir.join("auth.json"),
        &json!({"opencode":{"key":"fixture-zen"},"opencode-go":{"key":"fixture-go"}}),
    )
    .unwrap();
    let mut go_live = server(|r| {
        assert_eq!(r["headers"]["authorization"], "Bearer fixture-go");
        Reply::json(json!({"data":[{"id":"glm-5.3"}]}))
    })
    .await;
    core.inner
        .endpoints
        .lock()
        .unwrap()
        .insert("opencode-go-models".into(), go_live.url.clone());
    let mut live = server(|_| Reply::json(json!({"data":[{"id":"gpt-6-sol"}]}))).await;
    core.inner
        .endpoints
        .lock()
        .unwrap()
        .insert("opencode-models".into(), live.url.clone());
    let m = core
        .handle("llm-list-models", &[json!("opencode")])
        .await
        .unwrap()
        .unwrap();
    assert!(m
        .as_array()
        .unwrap()
        .iter()
        .any(|v| s(v).starts_with("opencode-go/")));
    assert!(m
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "opencode/gpt-6-sol"));
    assert!(m
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "opencode-go/glm-5.3"));
    assert!(go_live.requests.try_recv().is_ok());
    assert_eq!(
        live.requests.try_recv().unwrap()["headers"]["authorization"],
        "Bearer fixture-zen"
    );
    core.shutdown().await;
}

#[test]
fn regression_w_t05_anthropic_raw_empty_text_and_responses_shapes() {
    use super::wire::{encode, Decoder};
    use runtime::transcript::{Origin, Protocol};
    let o = |p: Protocol| Origin {
        protocol: p,
        provider: "custom:x".into(),
        model: "m".into(),
    };
    let run = |p: Protocol, ev: Vec<Value>| {
        let mut d = Decoder::new(o(p), 0);
        let mut bytes = String::new();
        for e in ev {
            bytes.push_str(&format!("data: {e}\n\n"));
        }
        d.feed(bytes.as_bytes(), true, &mut |_| {}).unwrap();
        d.finish()
    };
    let t = run(Protocol::Anthropic, vec![
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"a","name":"read","input":{}}}),
        json!({"type":"content_block_stop","index":1}),
        json!({"type":"message_delta","delta":{"stop_reason":"tool_use"}}),
        json!({"type":"message_stop"}),
    ]).unwrap();
    let body = encode(
        &o(Protocol::Anthropic),
        &[Message::text("user", "u"), t.message],
        &[],
        10,
        false,
    )
    .unwrap();
    assert_eq!(body["messages"][1]["content"].as_array().unwrap().len(), 1);
    assert_eq!(body["messages"][1]["content"][0]["type"], "tool_use");
    let t = run(Protocol::OpenaiResponses, vec![
        json!({"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","call_id":"c","name":"write","arguments":""}}),
        json!({"type":"response.function_call_arguments.delta","output_index":0,"delta":"{\"path\":\"a\"}"}),
        json!({"type":"response.completed","response":{"output":[]}}),
    ]).unwrap();
    assert_eq!(t.calls[0]["function"]["arguments"], "{\"path\":\"a\"}");
    let t = run(
        Protocol::OpenaiResponses,
        vec![
            json!({"type":"response.output_text.delta","delta":"hello"}),
            json!({"type":"response.completed","response":{"output":[]}}),
        ],
    )
    .unwrap();
    assert_eq!(t.text, "hello");
    assert_eq!(t.message.blocks.len(), 1);
    assert!(t
        .message
        .opaque
        .unwrap()
        .value
        .to_string()
        .contains("hello"));
    let t = run(Protocol::OpenaiChat, vec![json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c","function":{"name":"write","arguments":{"path":"a"}}}]},"finish_reason":"tool_calls"}]})]).unwrap();
    assert_eq!(t.calls[0]["function"]["arguments"], "{\"path\":\"a\"}");
    let t = run(Protocol::OpenaiResponses, vec![json!({"type":"response.completed","response":{"output":[{"type":"function_call","call_id":"c","name":"write","arguments":{"path":"a"}}]}})]).unwrap();
    assert_eq!(t.calls[0]["function"]["arguments"], "{\"path\":\"a\"}");
}

#[tokio::test]
async fn regression_w_t06_routes_and_json_errors() {
    let (_d, core, _e) = fixture();
    let n = Arc::new(AtomicUsize::new(0));
    let c = n.clone();
    let http = server(move |_| {
        c.fetch_add(1, Ordering::SeqCst);
        Reply::json(json!({"error":{"message":"fixture model is loading"}}))
    })
    .await;
    let cfg = json!({"provider":"openai-compatible","model":"m","baseURL":http.url});
    let e = core
        .canonical_turn(&cfg, &user(), &[], 0, &Cancel::new(), |_| {})
        .await
        .err();
    assert!(e.unwrap().contains("fixture model is loading"));
    assert_eq!(n.load(Ordering::SeqCst), 1);
    let raw = json!({"provider":"gemini","providerConfigs":{"gemini":{"geminiApiKey":"fixture-k","baseURL":"https://generativelanguage.googleapis.com/v1beta"}}});
    let r = core
        .native_route(&config::selected(&raw, None, None), &[])
        .await
        .unwrap();
    assert_eq!(r.origin.protocol, runtime::transcript::Protocol::Gemini);
    assert!(r.headers.contains_key("x-goog-api-key"));
    assert!(r.url.as_str().contains(":streamGenerateContent"));
    let raw = json!({"provider":"custom:g","customProviders":[{"id":"custom:g","title":"G","protocol":"gemini"}],"providerConfigs":{"custom:g":{"apiKey":"fixture-k","baseURL":"https://generativelanguage.googleapis.com/v1beta","openaiCompatibleBaseURL":"https://generativelanguage.googleapis.com/v1beta"}},"model":"gemini-3.5-flash"});
    let r = core
        .native_route(&config::selected(&raw, None, None), &[])
        .await
        .unwrap();
    assert!(!r.headers.contains_key("authorization"));
    assert!(r.headers.contains_key("x-goog-api-key"));
    let raw = json!({"provider":"opencode","providerConfigs":{"opencode":{"apiKey":"fixture-k"}}});
    let cfg = config::selected(&raw, None, None);
    let r = core.native_route(&cfg, &[]).await.unwrap();
    assert_eq!(cfg["model"], "opencode/gpt-5.4-mini");
    assert!(r.url.as_str().ends_with("/openai/v1/responses"));
    core.shutdown().await;
}

#[tokio::test]
async fn regression_w_t07_placeholder_key_and_override_headers() {
    let (_d, core, _e) = fixture();
    for key in ["none", "lm-studio", "EMPTY"] {
        let http = server(move |_| sse(vec![json!({"choices":[{"delta":{"content":"none of the lm-studio EMPTY values changed"},"finish_reason":"stop"}]})])).await;
        let cfg =
            json!({"provider":"openai-compatible","model":"m","baseURL":http.url,"apiKey":key});
        assert_eq!(
            core.canonical_turn(&cfg, &user(), &[], 0, &Cancel::new(), |_| {})
                .await
                .unwrap()
                .text,
            "none of the lm-studio EMPTY values changed"
        );
    }
    core.shutdown().await;
}

#[tokio::test]
async fn coding_output_thinking_arguments_and_placeholder_keys_survive_all_codecs() {
    let (_dir, core, _) = fixture();
    let code = "interface Login { token: string; password: string } const apiKey = process.env.KEY; the secret: rotate it weekly; none lm-studio EMPTY";
    for protocol in [
        "openai-chat",
        "openai-responses",
        "anthropic",
        "gemini",
        "ollama",
    ] {
        let args = json!({"content":code});
        let reply = match protocol {
            "openai-chat" => {
                json!({"choices":[{"message":{"content":code,"reasoning_content":code,"tool_calls":[{"id":"call","function":{"name":"write","arguments":args.to_string()}}]},"finish_reason":"tool_calls"}]})
            }
            "openai-responses" => {
                json!({"output":[{"type":"message","content":[{"type":"output_text","text":code}]},{"type":"reasoning","encrypted_content":"opaque-fixture","summary":[{"text":code}]},{"type":"function_call","call_id":"call","name":"write","arguments":args}]})
            }
            "anthropic" => {
                json!({"content":[{"type":"text","text":code},{"type":"thinking","thinking":code,"signature":"signed-fixture"},{"type":"tool_use","id":"call","name":"write","input":args}],"stop_reason":"tool_use"})
            }
            "gemini" => {
                json!({"candidates":[{"content":{"parts":[{"text":code},{"text":code,"thought":true},{"functionCall":{"name":"write","args":args},"thoughtSignature":"signed-fixture"}]},"finishReason":"STOP"}]})
            }
            _ => {
                json!({"message":{"content":code,"thinking":code,"tool_calls":[{"function":{"name":"write","arguments":args}}]},"done":true})
            }
        };
        let secret_reply = reply.to_string().replace(code, "fixture-real-private-key");
        let http = server(move |_| Reply::json(reply.clone())).await;
        for key in ["fixture-real-private-key", "none", "EMPTY", "lm-studio"] {
            let cfg = json!({"provider":"custom:fixture","model":"fixture","protocol":protocol,"baseURL":http.url,"apiKey":key});
            let mut shown = String::new();
            let turn = core
                .canonical_turn(&cfg, &user(), &[], 0, &Cancel::new(), |t| shown.push_str(t))
                .await
                .unwrap();
            assert_eq!(turn.text, code, "{protocol}/{key}");
            assert_eq!(shown, code);
            let a: Value =
                serde_json::from_str(s(&turn.calls[0]["function"]["arguments"])).unwrap();
            assert_eq!(a["content"], code);
            assert!(turn
                .message
                .opaque
                .unwrap()
                .value
                .to_string()
                .contains("token: string"));
        }
        let secret_reply: Value = serde_json::from_str(&secret_reply).unwrap();
        let secret_http = server(move |_| Reply::json(secret_reply.clone())).await;
        let cfg = json!({"provider":"custom:fixture","model":"fixture","protocol":protocol,"baseURL":secret_http.url,"apiKey":"fixture-real-private-key"});
        let error = core
            .canonical_turn(&cfg, &user(), &[], 0, &Cancel::new(), |_| {})
            .await
            .err()
            .unwrap();
        assert!(error.contains("credential"), "{protocol}: {error}");
        assert!(!error.contains("fixture-real-private-key"));
    }
    core.shutdown().await;
}

#[tokio::test]
async fn unsupported_responses_include_falls_back_once_before_output_and_keeps_encrypted_replay() {
    let (_dir, core, _) = fixture();
    let count = Arc::new(AtomicUsize::new(0));
    let n = count.clone();
    let http=server(move|r|{
        if n.fetch_add(1,Ordering::SeqCst)==0 {assert!(r["body"]["include"].is_array()); return Reply::json(json!({"error":{"message":"include is unsupported"}})).status(400);}
        assert!(r["body"]["include"].is_null());
        Reply::json(json!({"output":[{"type":"reasoning","encrypted_content":"cipher","id":"rs"},{"type":"message","content":[{"type":"output_text","text":"ok"}]}]}))
    }).await;
    let cfg = json!({"provider":"custom:x","model":"unknown","protocol":"openai-responses","baseURL":http.url});
    let t = core
        .canonical_turn(&cfg, &user(), &[], 0, &Cancel::new(), |_| {})
        .await
        .unwrap();
    assert_eq!(t.text, "ok");
    assert_eq!(count.load(Ordering::SeqCst), 2);
    let body = wire::encode(
        &t.message.opaque.as_ref().unwrap().origin,
        &[t.message.clone()],
        &[],
        1024,
        false,
    )
    .unwrap();
    assert_eq!(body["input"][0]["encrypted_content"], "cipher");
    core.shutdown().await;
}

#[tokio::test]
async fn host_derived_deepseek_replay_and_first_party_anthropic_budget() {
    let (_dir, core, _) = fixture();
    let mut http = server(|r| {
        assert_eq!(r["body"]["messages"][1]["reasoning_content"], "");
        Reply::json(json!({"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}]}))
    })
    .await;
    core.inner
        .endpoints
        .lock()
        .unwrap()
        .insert("openai-compatible".into(), http.url.clone());
    let cfg = json!({"provider":"openai-compatible","model":"deepseek-v4-flash","baseURL":"https://api.deepseek.com"});
    core.canonical_turn(
        &cfg,
        &[
            Message::text("user", "hi"),
            Message::text("assistant", "foreign history"),
            Message::text("user", "continue"),
        ],
        &[],
        0,
        &Cancel::new(),
        |_| {},
    )
    .await
    .unwrap();
    assert!(http.requests.try_recv().is_ok());
    for (host, budget) in [
        ("https://api.anthropic.com", 128_000_000),
        ("https://gateway.example", 32_000_000),
    ] {
        let r=core.native_route(&json!({"provider":"anthropic","model":"claude-haiku-4-5-20251001","apiKey":"fixture-key","baseURL":host}),&[]).await.unwrap();
        assert_eq!(r.request_budget, budget);
        let mut view = vec![Message {
            role: "user".into(),
            opaque: None,
            blocks: (0..4)
                .map(|_| Block::Image {
                    mime: "image/png".into(),
                    data: "a".repeat(8_000_000),
                })
                .collect(),
        }];
        assert_eq!(
            runtime::limits::project_images_budget(&r.origin.protocol, &mut view, r.request_budget),
            budget == 32_000_000
        );
    }
    core.shutdown().await;
}

#[tokio::test]
async fn json_provider_errors_preserve_message_and_typed_retry_policy_on_every_codec() {
    let (_dir, core, _) = fixture();
    for protocol in [
        "openai-chat",
        "openai-responses",
        "anthropic",
        "gemini",
        "ollama",
    ] {
        let n = Arc::new(AtomicUsize::new(0));
        let count = n.clone();
        let http = server(move |_| {
            count.fetch_add(1, Ordering::SeqCst);
            Reply::json(json!({"error":{"message":"fixture model is loading"}}))
        })
        .await;
        let cfg = json!({"provider":"custom:x","model":"m","protocol":protocol,"baseURL":http.url});
        let e = core
            .canonical_turn(&cfg, &user(), &[], 0, &Cancel::new(), |_| {})
            .await
            .err()
            .unwrap();
        assert!(e.contains("fixture model is loading"), "{protocol}: {e}");
        assert_eq!(n.load(Ordering::SeqCst), 1);
    }
    core.shutdown().await;
}

#[test]
fn repeated_signature_keeps_first_nonempty_and_invalid_argument_types_fail_closed() {
    use runtime::transcript::{Origin, Protocol};
    for args in [json!(true), json!(42), json!(["bad"])] {
        for protocol in [Protocol::OpenaiChat, Protocol::OpenaiResponses] {
            let mut d = wire::Decoder::new(
                Origin {
                    provider: "fixture".into(),
                    model: "fixture".into(),
                    protocol: protocol.clone(),
                },
                0,
            );
            let body = if protocol == Protocol::OpenaiChat {
                json!({"choices":[{"message":{"tool_calls":[{"id":"c","function":{"name":"write","arguments":args}}]},"finish_reason":"tool_calls"}]})
            } else {
                json!({"output":[{"type":"function_call","call_id":"c","name":"write","arguments":args}]})
            };
            let result = d.json(body, &mut |_| {}).and_then(|_| d.finish());
            assert!(result.is_err());
        }
    }
    let origin = Origin {
        provider: "fixture".into(),
        model: "fixture".into(),
        protocol: Protocol::OpenaiChat,
    };
    let mut d = wire::Decoder::new(origin.clone(), 0);
    let events = [
        json!({"choices":[{"delta":{"reasoning_details":[{"index":0,"type":"reasoning.text","text":"first","signature":""}]}}]}),
        json!({"choices":[{"delta":{"reasoning_details":[{"index":0,"type":"reasoning.text","text":"second","signature":"signature"}]}}]}),
        json!({"choices":[{"delta":{"content":"ok","reasoning_details":[{"index":0,"type":"reasoning.text","signature":"signature"}]},"finish_reason":"stop"}]}),
    ];
    for e in events {
        d.feed(format!("data: {e}\n\n").as_bytes(), false, &mut |_| {})
            .unwrap();
    }
    let t = d.finish().unwrap();
    let v = wire::encode(&origin, &[t.message], &[], 100, false).unwrap();
    assert_eq!(
        v["messages"][0]["reasoning_details"][0]["signature"],
        "signature"
    );
    assert_eq!(
        v["messages"][0]["reasoning_details"][0]["text"],
        "firstsecond"
    );
}

#[test]
fn anthropic_thinking_shape_matches_model_generation_and_gpt6_infers_vision_and_reasoning() {
    use runtime::transcript::{Origin, Protocol};
    for (model, shape) in [
        ("claude-haiku-4-5-20251001", "enabled"),
        ("claude-sonnet-4-5", "enabled"),
        ("claude-opus-4-5", "enabled"),
        ("claude-opus-4-6", "adaptive"),
        ("claude-sonnet-4-6", "adaptive"),
        ("claude-opus-5-5", "adaptive"),
        ("claude-3-5-haiku", ""),
    ] {
        let v = wire::encode(
            &Origin {
                provider: "anthropic".into(),
                model: model.into(),
                protocol: Protocol::Anthropic,
            },
            &user(),
            &[],
            8192,
            true,
        )
        .unwrap();
        assert_eq!(s(&v["thinking"]["type"]), shape, "{model}");
    }
    let cfg = config::selected(
        &json!({"provider":"openai","providerModels":{"openai":[{"id":"gpt-6-sol"}]}}),
        None,
        None,
    );
    let caps = cfg["providerModels"]["openai"][0]["capabilities"]
        .as_array()
        .unwrap();
    assert!(caps.contains(&json!("thinking")));
    assert!(caps.contains(&json!("vision")));
}

#[tokio::test]
async fn streamed_empty_argument_fragments_never_replace_accumulated_arguments() {
    let (_dir, core, _) = fixture();
    for protocol in ["openai-chat", "openai-responses"] {
        let http=server(move|_|sse(if protocol=="openai-chat" {vec![
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c","function":{"name":"write","arguments":""}}]}}]}),
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\":\"a\"}"}}]},"finish_reason":"tool_calls"}]})
        ]} else {vec![
            json!({"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","call_id":"c","name":"write","arguments":""}}),
            json!({"type":"response.function_call_arguments.delta","output_index":0,"delta":"{\"path\":\"a\"}"}),
            json!({"type":"response.function_call_arguments.done","output_index":0,"arguments":""}),
            json!({"type":"response.completed","response":{"output":[]}})
        ]})).await;
        let cfg = json!({"provider":"custom:x","model":"m","protocol":protocol,"baseURL":http.url});
        let t = core
            .canonical_turn(&cfg, &user(), &[], 0, &Cancel::new(), |_| {})
            .await
            .unwrap();
        assert_eq!(t.calls[0]["function"]["arguments"], "{\"path\":\"a\"}");
    }
    core.shutdown().await;
}
