//! Real dispatch against loopback protocol fixtures. No credentialed requests.
use super::tests::{fixture, next_event};
use super::*;
use runtime::transcript::Protocol;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::mpsc,
};
pub(super) struct Fake {
    pub url: String,
    pub requests: mpsc::UnboundedReceiver<Value>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Fake {
    fn drop(&mut self) {
        self.task.abort()
    }
}
pub(super) async fn server(replies: Vec<(String, String)>) -> Fake {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        for (kind, body) in replies {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![];
            let end;
            loop {
                let mut b = [0; 4096];
                let n = socket.read(&mut b).await.unwrap();
                if n == 0 {
                    return;
                }
                bytes.extend_from_slice(&b[..n]);
                if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    end = i + 4;
                    break;
                }
            }
            let headers = String::from_utf8_lossy(&bytes[..end]).to_string();
            let length = headers
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|v| v.trim().parse::<usize>().ok())
                })
                .unwrap();
            while bytes.len() < end + length {
                let mut b = [0; 8192];
                let n = socket.read(&mut b).await.unwrap();
                if n == 0 {
                    return;
                }
                bytes.extend_from_slice(&b[..n]);
            }
            let value: Value = serde_json::from_slice(&bytes[end..end + length]).unwrap();
            let _=tx.send(json!({"headers":headers,"body":value,"bytes":String::from_utf8_lossy(&bytes[end..end+length])}));
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).await.unwrap();
            for chunk in body.as_bytes().chunks(3) {
                if socket.write_all(chunk).await.is_err() {
                    break;
                }
            }
            let _ = socket.shutdown().await;
        }
    });
    Fake {
        url,
        requests: rx,
        task,
    }
}
fn sse(v: Value) -> String {
    format!("data: {v}\r\n\r\n")
}
fn final_reply(protocol: Protocol) -> (String, String) {
    let body = match protocol {
        Protocol::OpenaiChat => {
            sse(json!({"choices":[{"delta":{"content":"fixture 🦀"},"finish_reason":"stop"}]}))
        }
        Protocol::OpenaiResponses => {
            sse(json!({"type":"response.output_text.delta","delta":"fixture 🦀"}))
                + &sse(
                    json!({"type":"response.completed","response":{"output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"fixture 🦀"}]}]}}),
                )
        }
        Protocol::Anthropic => {
            sse(
                json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":"fixture 🦀"}}),
            ) + &sse(json!({"type":"content_block_stop","index":0}))
                + &sse(json!({"type":"message_stop"}))
        }
        Protocol::Gemini => sse(
            json!({"candidates":[{"content":{"parts":[{"text":"fixture 🦀","thoughtSignature":"fixture-signature"}]},"finishReason":"STOP"}]}),
        ),
        Protocol::Ollama => {
            json!({"message":{"content":"fixture 🦀","thinking":"private"},"done":true}).to_string()
                + "\n"
        }
    };
    (
        if protocol == Protocol::Ollama {
            "application/x-ndjson"
        } else {
            "text/event-stream"
        }
        .into(),
        body,
    )
}
#[tokio::test]
async fn every_provider_and_protocol_executes_through_agent_dispatch() {
    for provider in config::PROVIDERS.iter().copied().chain(["custom:fixture"]) {
        let protocols = match provider {
            "anthropic" => vec![Protocol::Anthropic],
            "ollama" => vec![Protocol::Ollama],
            "gemini" | "antigravity" => vec![Protocol::Gemini],
            "copilot" | "opencode" | "custom:fixture" => vec![
                Protocol::OpenaiChat,
                Protocol::OpenaiResponses,
                Protocol::Anthropic,
            ],
            "openai" => vec![Protocol::OpenaiResponses],
            _ => vec![Protocol::OpenaiChat],
        };
        for protocol in protocols {
            let (_dir, core, mut events) = fixture();
            let png="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a0l8AAAAASUVORK5CYII=";
            core.inner
                .ctx
                .set_router(Arc::new(move |channel, _| {
                    Box::pin(async move {
                        assert_eq!(channel, "core:platform:screenshot");
                        Ok(json!({"mimeType":"image/png","data":png}))
                    })
                }))
                .unwrap();
            std::fs::write(
                core.inner.ctx.paths.home_dir.join("evidence.txt"),
                "parallel evidence 🦀",
            )
            .unwrap();
            let mut fake = server(vec![
                parallel_tool_reply(protocol.clone()),
                final_reply(protocol.clone()),
                final_reply(protocol.clone()),
            ])
            .await;
            core.inner
                .endpoints
                .lock()
                .unwrap()
                .insert(provider.into(), fake.url.clone());
            if provider == "antigravity" {
                let dir = core.pi_dir();
                std::fs::create_dir_all(&dir).unwrap();
                common::write_json_atomic(&dir.join("antigravity-accounts.json"),&json!({"version":1,"activeAccountId":"fixture","accounts":{"fixture":{"accountId":"fixture","access":"fixture-access","refresh":"fixture-refresh","expires":now_ms()+3600000,"projectId":"fixture-project"}}})).unwrap();
            }
            let name = serde_json::to_value(&protocol).unwrap();
            core.patch_config(json!({"provider":provider,"model":"fixture-model","customProviders":[{"id":"custom:fixture","title":"Fixture"}],"providerConfigs":{provider:{"baseURL":fake.url,"apiKey":"fixture-api-key","copilotGithubToken":"fixture-github","protocol":name}},"providerModels":{provider:[{"id":"fixture-model","protocol":name,"capabilities":["tools","vision"]}]},"agentRuntime":"pi","agentApprovalMode":"turbo"})).await.unwrap();
            let run = "agent-protocol";
            assert_eq!(
                core.handle(
                    "agent:run",
                    &[json!({"runId":run,"task":"fixture","cwd":core.inner.ctx.paths.home_dir,"sessionId":"all-providers","userTurnId":"user-one","assistantTurnId":"answer-one","images":[{"mimeType":"image/png","data":"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a0l8AAAAASUVORK5CYII="}]})]
                )
                .await
                .unwrap()
                .unwrap()["ok"],
                true
            );
            assert_eq!(
                next_event(&mut events, "answer").await["text"],
                "fixture 🦀",
                "{provider} {protocol:?}"
            );
            next_event(&mut events, "done").await;
            let request = fake.requests.recv().await.unwrap();
            let headers = s(&request["headers"]);
            let body = &request["body"];
            assert!(!body.to_string().contains("fixture-api-key"));
            if protocol == Protocol::OpenaiResponses {
                assert_eq!(body["store"], false)
            }
            if provider == "antigravity" {
                assert_eq!(body["project"], "fixture-project");
                assert!(body["request"]["contents"].is_array());
                assert!(headers.to_lowercase().contains("antigravity/cli/1.4.0"));
            }
            if provider == "copilot" {
                assert!(headers.to_lowercase().contains("x-initiator: user"));
            }
            if provider == "opencode" {
                assert!(headers.contains("/openai/v1/") || headers.contains("/anthropic/v1/"));
            }
            let follow = fake.requests.recv().await.unwrap();
            assert!(
                follow.to_string().matches("parallel evidence").count() >= 1
                    && follow.to_string().matches(png).count() >= 2,
                "{provider} {protocol:?}: {follow}"
            );
            if provider == "copilot" {
                assert!(s(&follow["headers"])
                    .to_lowercase()
                    .contains("x-initiator: agent"));
                assert!(s(&follow["headers"])
                    .to_lowercase()
                    .contains("copilot-vision-request: true"));
            }
            core.handle("agent:run",&[json!({"runId":"agent-all-resume","task":"continue","sessionId":"all-providers","priorTurnIds":["user-one","answer-one"]})]).await.unwrap().unwrap();
            next_event(&mut events, "answer").await;
            next_event(&mut events, "done").await;
            let resumed = fake.requests.recv().await.unwrap();
            assert!(resumed.to_string().contains("parallel evidence"));
            assert!(resumed
                .to_string()
                .contains(if protocol == Protocol::Ollama {
                    "private-thought"
                } else {
                    "private-signature"
                }));
            core.shutdown().await;
        }
    }
}
#[tokio::test]
async fn canonical_store_roundtrips_images_tools_and_opaque_without_renderer_exposure() {
    use runtime::transcript::*;
    let (_dir, core, _) = fixture();
    let data = core.inner.ctx.paths.data_dir.clone();
    let origin = Origin {
        protocol: Protocol::Anthropic,
        provider: "fixture".into(),
        model: "model".into(),
    };
    let message = Message {
        role: "assistant".into(),
        blocks: vec![Block::ToolCall {
            id: "canonical".into(),
            wire_id: "wire".into(),
            name: "read".into(),
            arguments: "{ \"path\": \"a\" }".into(),
        }],
        opaque: Some(Opaque {
            origin,
            value: json!([{"type":"thinking","thinking":"private","signature":"signed"}]),
        }),
    };
    runtime::store::save(&data, "session", "turn", &[message.clone()]).unwrap();
    let restored = runtime::store::load(&data, "session", "turn")
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_vec(&restored).unwrap(),
        serde_json::to_vec(&[message]).unwrap()
    );
    runtime::store::delete(&data, Some("session"), Some("turn")).unwrap();
    assert!(runtime::store::load(&data, "session", "turn")
        .unwrap()
        .is_none());
    core.shutdown().await;
}

fn tool_reply(protocol: Protocol) -> (String, String) {
    let call = json!({"path":"evidence.txt"}).to_string();
    let body=match protocol{
 Protocol::OpenaiChat=>sse(json!({"choices":[{"delta":{"reasoning_content":"private-thought","tool_calls":[{"index":0,"id":"wire:0","extra_content":{"signature":"private-signature"},"function":{"name":"read","arguments":call}}]},"finish_reason":"stop"}]})),
 Protocol::OpenaiResponses=>sse(json!({"type":"response.completed","response":{"output":[{"type":"reasoning","id":"private-reasoning","encrypted_content":"private-signature","summary":[]},{"type":"function_call","id":"fc-0","call_id":"wire:0","name":"read","arguments":call}]}})),
 Protocol::Anthropic=>sse(json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"private-thought","signature":"private-signature"}}))+&sse(json!({"type":"content_block_stop","index":0}))+&sse(json!({"type":"content_block_start","index":1,"content_block":{"type":"redacted_thinking","data":"private-redacted"}}))+&sse(json!({"type":"content_block_stop","index":1}))+&sse(json!({"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"wire:0","name":"read","input":{"path":"evidence.txt"}}}))+&sse(json!({"type":"content_block_stop","index":2}))+&sse(json!({"type":"message_stop"})),
 Protocol::Gemini=>sse(json!({"candidates":[{"content":{"parts":[{"thought":true,"text":"private-thought","thoughtSignature":"private-signature"},{"functionCall":{"id":"wire:0","name":"read","args":{"path":"evidence.txt"}},"thoughtSignature":"private-signature-call"}]},"finishReason":"STOP"}]})),
 Protocol::Ollama=>json!({"message":{"thinking":"private-thought","tool_calls":[{"function":{"name":"read","arguments":{"path":"evidence.txt"}}}]},"done":true}).to_string()+"\n"};
    (
        if protocol == Protocol::Ollama {
            "application/x-ndjson"
        } else {
            "text/event-stream"
        }
        .into(),
        body,
    )
}
#[tokio::test]
async fn durable_signed_history_resumes_by_id_and_switches_models_without_foreign_opaque() {
    for protocol in [
        Protocol::OpenaiChat,
        Protocol::OpenaiResponses,
        Protocol::Anthropic,
        Protocol::Gemini,
        Protocol::Ollama,
    ] {
        let (_dir, core, mut events) = fixture();
        std::fs::write(
            core.inner.ctx.paths.home_dir.join("evidence.txt"),
            "durable evidence",
        )
        .unwrap();
        let mut fake = server(vec![
            tool_reply(protocol.clone()),
            final_reply(protocol.clone()),
            final_reply(protocol.clone()),
            final_reply(protocol.clone()),
        ])
        .await;
        let protocol_value = serde_json::to_value(&protocol).unwrap();
        core.patch_config(json!({"provider":"custom:history","model":"model-a","customProviders":[{"id":"custom:history","title":"Fixture"}],"providerConfigs":{"custom:history":{"baseURL":fake.url,"protocol":protocol_value}},"providerModels":{"custom:history":[{"id":"model-a","capabilities":["tools","vision"]}]}})).await.unwrap();
        let png="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a0l8AAAAASUVORK5CYII=";
        core.handle("agent:run",&[json!({"runId":"agent-history-a","task":"read evidence","sessionId":"history","userTurnId":"user-a","assistantTurnId":"answer-a","images":[{"mimeType":"image/png","data":png}]})]).await.unwrap().unwrap();
        next_event(&mut events, "answer").await;
        next_event(&mut events, "done").await;
        let first = fake.requests.recv().await.unwrap();
        assert!(first.to_string().contains(png));
        let follow = fake.requests.recv().await.unwrap();
        assert!(follow.to_string().contains("durable evidence"));
        core.handle("agent:run",&[json!({"runId":"agent-history-b","task":"continue","sessionId":"history","userTurnId":"user-b","assistantTurnId":"answer-b","priorTurnIds":["user-a","answer-a"]})]).await.unwrap().unwrap();
        next_event(&mut events, "answer").await;
        next_event(&mut events, "done").await;
        let same = fake.requests.recv().await.unwrap();
        assert!(same.to_string().contains("durable evidence"));
        assert!(
            same.to_string().contains(if protocol == Protocol::Ollama {
                "private-thought"
            } else {
                "private-signature"
            }),
            "{protocol:?} {same}"
        );
        core.patch_config(json!({"model":"model-b"})).await.unwrap();
        core.handle("agent:run",&[json!({"runId":"agent-history-c","task":"another model","sessionId":"history","priorTurnIds":["user-a","answer-a"]})]).await.unwrap().unwrap();
        next_event(&mut events, "answer").await;
        next_event(&mut events, "done").await;
        let foreign = fake.requests.recv().await.unwrap();
        assert!(!foreign.to_string().contains("private-signature"));
        assert!(!foreign.to_string().contains("private-thought"));
        assert!(!foreign.to_string().contains("private-redacted"));
        assert!(foreign.to_string().contains("durable evidence"));
        core.shutdown().await;
    }
}
#[tokio::test]
async fn a_crashed_saved_call_gets_an_unknown_result_and_is_never_reexecuted() {
    use runtime::transcript::*;
    let (_dir, core, mut events) = fixture();
    let mut fake = server(vec![final_reply(Protocol::OpenaiChat)]).await;
    let message = Message {
        role: "assistant".into(),
        opaque: None,
        blocks: vec![Block::ToolCall {
            id: "crashed-call".into(),
            wire_id: "wire-0".into(),
            name: "write".into(),
            arguments: json!({"path":"must-not-exist","content":"effect"}).to_string(),
        }],
    };
    runtime::store::save(
        &core.inner.ctx.paths.data_dir,
        "crashed",
        "answer",
        &[message],
    )
    .unwrap();
    core.patch_config(json!({"provider":"openai-compatible","model":"fixture","providerConfigs":{"openai-compatible":{"baseURL":fake.url}}})).await.unwrap();
    core.handle("agent:run",&[json!({"runId":"agent-crash-resume","task":"continue","sessionId":"crashed","priorTurnIds":["answer"]})]).await.unwrap().unwrap();
    next_event(&mut events, "answer").await;
    next_event(&mut events, "done").await;
    let request = fake.requests.recv().await.unwrap();
    assert!(request
        .to_string()
        .contains("The outcome of this call is unknown"));
    assert!(!core
        .inner
        .ctx
        .paths
        .home_dir
        .join("must-not-exist")
        .exists());
    core.shutdown().await;
}

fn parallel_tool_reply(protocol: Protocol) -> (String, String) {
    let (kind, body) = tool_reply(protocol.clone());
    let mut events = if protocol == Protocol::Ollama {
        vec![serde_json::from_str::<Value>(body.trim()).unwrap()]
    } else {
        body.split("\r\n\r\n")
            .filter_map(|s| s.strip_prefix("data: "))
            .map(|s| serde_json::from_str::<Value>(s).unwrap())
            .collect::<Vec<_>>()
    };
    match protocol {
        Protocol::OpenaiChat => {
            let mut call = events[0]["choices"][0]["delta"]["tool_calls"][0].clone();
            call["index"] = json!(1);
            call["id"] = json!("wire:1");
            call["function"] = json!({"name":"screen_capture","arguments":"{}"});
            events[0]["choices"][0]["delta"]["tool_calls"]
                .as_array_mut()
                .unwrap()
                .push(call);
        }
        Protocol::OpenaiResponses => {
            let mut call = events[0]["response"]["output"][1].clone();
            call["id"] = json!("fc-1");
            call["call_id"] = json!("wire:1");
            call["name"] = json!("screen_capture");
            call["arguments"] = json!("{}");
            events[0]["response"]["output"]
                .as_array_mut()
                .unwrap()
                .push(call);
        }
        Protocol::Anthropic => {
            let terminal = events.pop().unwrap();
            events.push(json!({"type":"content_block_start","index":3,"content_block":{"type":"tool_use","id":"wire:1","name":"screen_capture","input":{}}}));
            events.push(json!({"type":"content_block_stop","index":3}));
            events.push(terminal);
        }
        Protocol::Gemini => {
            let mut call = events[0]["candidates"][0]["content"]["parts"][1].clone();
            call["functionCall"] = json!({"id":"wire:1","name":"screen_capture","args":{}});
            events[0]["candidates"][0]["content"]["parts"]
                .as_array_mut()
                .unwrap()
                .push(call);
        }
        Protocol::Ollama => {
            let call = json!({"function":{"name":"screen_capture","arguments":{}}});
            events[0]["message"]["tool_calls"]
                .as_array_mut()
                .unwrap()
                .push(call);
        }
    }
    (
        kind,
        events
            .iter()
            .map(|v| {
                if protocol == Protocol::Ollama {
                    format!("{v}\n")
                } else {
                    sse(v.clone())
                }
            })
            .collect(),
    )
}

#[tokio::test]
async fn retry_reuses_original_image_and_preserves_completed_effect_history() {
    use runtime::transcript::*;
    let (_dir,core,mut events)=fixture();
    let png="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a0l8AAAAASUVORK5CYII=";
    let mut user=Message::text("user","original");
    user.blocks.push(Block::Image{mime:"image/png".into(),data:png.into()});
    runtime::store::save(&core.inner.ctx.paths.data_dir,"retry","user",&[user]).unwrap();
    let completed=Message { role:"assistant".into(),opaque:None,blocks:vec![Block::ToolCall{id:"effect".into(),wire_id:"effect".into(),name:"write".into(),arguments:json!({"path":"effect-marker","content":"once"}).to_string()}] };
    let result=Message{role:"tool".into(),opaque:None,blocks:vec![Block::ToolResult{call:"effect".into(),wire_id:"effect".into(),name:"write".into(),blocks:vec![Block::Text{text:"completed effect marker once".into()}],is_error:false}]};
    runtime::store::save(&core.inner.ctx.paths.data_dir,"retry","answer",&[completed,result]).unwrap();
    let mut fake=server(vec![final_reply(Protocol::OpenaiChat),final_reply(Protocol::OpenaiChat),final_reply(Protocol::OpenaiChat)]).await;
    core.patch_config(json!({"provider":"openai-compatible","model":"vision","providerConfigs":{"openai-compatible":{"baseURL":fake.url}},"providerModels":{"openai-compatible":[{"id":"vision","capabilities":["tools","vision"]}]},"agentApprovalMode":"turbo"})).await.unwrap();
    core.handle("agent:run",&[json!({"runId":"agent-reopened-retry","task":"original","sessionId":"retry","userTurnId":"user","assistantTurnId":"answer","retry":true})]).await.unwrap().unwrap();
    next_event(&mut events,"answer").await; next_event(&mut events,"done").await;
    let wire=fake.requests.recv().await.unwrap().to_string();
    assert!(wire.contains(png)); assert!(wire.contains("completed effect marker once"));
    assert!(!core.inner.ctx.paths.home_dir.join("effect-marker").exists());
    core.patch_config(json!({"model":"text"})).await.unwrap();
    core.handle("agent:run",&[json!({"runId":"agent-history-nonvision","task":"continue","sessionId":"retry","priorTurnIds":["user","answer"]})]).await.unwrap().unwrap();
    next_event(&mut events,"answer").await; next_event(&mut events,"done").await;
    let wire=fake.requests.recv().await.unwrap().to_string(); assert!(!wire.contains(png)); assert!(wire.contains("Earlier image omitted"));
    assert!(serde_json::to_string(&runtime::store::load(&core.inner.ctx.paths.data_dir,"retry","user").unwrap()).unwrap().contains(png));
    core.patch_config(json!({"model":"vision"})).await.unwrap();
    core.handle("agent:run",&[json!({"runId":"agent-history-vision-again","task":"continue","sessionId":"retry","priorTurnIds":["user","answer"]})]).await.unwrap().unwrap();
    next_event(&mut events,"answer").await; next_event(&mut events,"done").await;
    assert!(fake.requests.recv().await.unwrap().to_string().contains(png));
    core.shutdown().await;
}
