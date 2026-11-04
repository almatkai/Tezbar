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
            let mut fake = server(vec![final_reply(protocol.clone())]).await;
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
            core.patch_config(json!({"provider":provider,"model":"fixture-model","customProviders":[{"id":"custom:fixture","title":"Fixture"}],"providerConfigs":{provider:{"baseURL":fake.url,"apiKey":"fixture-api-key","copilotGithubToken":"fixture-github","protocol":name}},"providerModels":{provider:[{"id":"fixture-model","protocol":name,"capabilities":["tools","vision"]}]},"agentRuntime":"pi"})).await.unwrap();
            let run = "agent-protocol";
            assert_eq!(
                core.handle(
                    "agent:run",
                    &[json!({"runId":run,"task":"fixture","cwd":core.inner.ctx.paths.home_dir})]
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
