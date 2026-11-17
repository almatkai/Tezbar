//! Single adapter/core/provider fixture, local HTTP only. No startup services or auth store.
use super::*;
use tezbar_core::{
    ai::AiCore,
    common::{CoreContext, CorePaths},
    host::TestHost,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
struct Loopback(AiCore);
impl ChatTransport for Loopback {
    fn send(&self, request: ChatRequest) -> ChatFuture {
        let core = self.0.clone();
        let (channel, args) = request.wire();
        Box::pin(async move {
            core.handle(channel, &args)
                .await
                .ok_or("Unknown fixture channel")?
        })
    }
}
#[tokio::test]
async fn finite_adapter_runs_loopback_and_persists_metadata_without_desktop_services() {
    let temp = tempfile::tempdir().unwrap();
    let paths = CorePaths {
        data_dir: temp.path().join("data"),
        config_dir: temp.path().join("config"),
        home_dir: temp.path().join("home"),
        temp_dir: temp.path().join("temp"),
        resource_dir: temp.path().join("resources"),
    };
    std::fs::create_dir_all(&paths.home_dir).unwrap();
    let host = Arc::new(TestHost::default());
    let ctx = CoreContext::new(paths.clone(), Some(host.clone()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut bytes = vec![];
        let end = loop {
            let mut chunk = [0u8; 4096];
            let n = socket.read(&mut chunk).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            assert!(bytes.len() < 2 * 1024 * 1024);
            if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let header = String::from_utf8_lossy(&bytes[..end]);
        let length = header
            .lines()
            .find_map(|l| {
                l.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .and_then(|s| s.trim().parse::<usize>().ok())
            })
            .unwrap();
        while bytes.len() < end + length {
            let mut b = [0u8; 4096];
            let n = socket.read(&mut b).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
        }
        let request: Value = serde_json::from_slice(&bytes[end..end + length]).unwrap();
        assert_eq!(request["model"], "fixture");
        assert!(request.to_string().contains("Native adapter question"));
        let body=json!({"model":"fixture","message":{"role":"assistant","content":"Native adapter answer 🦀"},"done":true}).to_string()+"\n";
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
        request
    });
    ctx.config.patch(&json!({"provider":"ollama","model":"fixture","providerConfigs":{"ollama":{"baseURL":url}},"providerModels":{"ollama":[{"id":"fixture","capabilities":["tools","vision"]}]},"agentApprovalMode":"review"})).unwrap();
    let transport = Loopback(AiCore::new(ctx));
    let mut model = ChatController::default();
    model
        .boot(
            Boot::NewChat {
                session_id: "native-wire".into(),
                cwd: Some(paths.home_dir.to_string_lossy().into_owned()),
            },
            Some("native-nonce".into()),
        )
        .unwrap();
    model.edit_draft("native-wire", "Native adapter question".into());
    model
        .attach_references(
            "native-wire",
            vec![Reference::Image {
                blob_id: "fixture-input".into(),
                mime_type: "image/png".into(),
                name: "Fixture".into(),
                width: Some(1),
                height: Some(1),
            }],
        )
        .unwrap();
    let Effect::AppendUser {
        token,
        session_id,
        turn,
    } = model
        .send(
            "native-wire",
            "agent-native-fixture".into(),
            "user".into(),
            "assistant".into(),
            1,
        )
        .unwrap()
    else {
        panic!()
    };
    let selection = Selection {
        provider: Some("ollama".into()),
        provider_title: Some("Ollama".into()),
        model: Some("fixture".into()),
        cwd: Some(paths.home_dir.to_string_lossy().into_owned()),
        ..Default::default()
    };
    transport
        .send(ChatRequest::Append {
            session: session_id.clone(),
            title: "Fixture".into(),
            selection: selection.clone(),
            turn: turn.clone(),
        })
        .await
        .unwrap();
    let mut effects =
        model.prepared_with_selection(&session_id, token, Ok(turn), selection.clone(), 1);
    let Effect::Start(request) = effects.pop().unwrap() else {
        panic!()
    };
    let reply = transport
        .send(ChatRequest::Run {
            request,
            images: vec![image()],
        })
        .await
        .unwrap();
    assert_eq!(reply["runId"], "agent-native-fixture");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if host
                .events
                .lock()
                .unwrap()
                .iter()
                .any(|(c, v)| c == "agent:event" && v["type"] == "done")
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let events = host.events.lock().unwrap().clone();
    let mut persisted = 0;
    for (channel, event) in events {
        if channel == "agent:event" {
            if let Ok(event) = serde_json::from_value::<chat::Event>(event) {
                for effect in model.event(event) {
                    if let Effect::PersistTurn {
                        token,
                        session_id,
                        turn,
                    } = effect
                    {
                        transport
                            .send(ChatRequest::Append {
                                session: session_id.clone(),
                                title: "Fixture".into(),
                                selection: selection.clone(),
                                turn,
                            })
                            .await
                            .unwrap();
                        model.persisted(&session_id, token, true);
                        persisted += 1;
                    }
                }
            }
        }
    }
    assert_eq!(persisted, 1);
    let saved = transport
        .send(ChatRequest::Get("native-wire".into()))
        .await
        .unwrap();
    let assistant = saved["turns"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["role"] == "assistant")
        .unwrap();
    assert_eq!(assistant["text"], "Native adapter answer 🦀");
    assert_eq!(assistant["responseMeta"]["provider"], "ollama");
    assert_eq!(assistant["responseMeta"]["model"], "fixture");
    let canonical = transport
        .send(ChatRequest::HistoryImage {
            session: "native-wire".into(),
            turn: "user".into(),
            index: 0,
        })
        .await
        .unwrap();
    assert_eq!(canonical["mimeType"], "image/png");
    assert_eq!(canonical["data"], image()["data"]);
    assert!(
        transport
            .send(ChatRequest::HistoryImage {
                session: "other-session".into(),
                turn: "user".into(),
                index: 0
            })
            .await
            .unwrap()
            .is_null()
    );
    assert!(
        transport
            .send(ChatRequest::HistoryImage {
                session: "native-wire".into(),
                turn: "assistant".into(),
                index: 0
            })
            .await
            .unwrap()
            .is_null()
    );
    assert!(
        transport
            .send(ChatRequest::HistoryImage {
                session: "native-wire".into(),
                turn: "user".into(),
                index: 8
            })
            .await
            .is_err()
    );
    transport
        .send(ChatRequest::Delete("native-wire".into()))
        .await
        .unwrap();
    assert!(
        transport
            .send(ChatRequest::HistoryImage {
                session: "native-wire".into(),
                turn: "user".into(),
                index: 0
            })
            .await
            .unwrap()
            .is_null()
    );
    assert!(host.calls.lock().unwrap().is_empty());
    server.await.unwrap();
    transport.0.shutdown().await;
}

#[tokio::test]
async fn real_core_clear_covers_over_200_rows_and_tombstones_preparing_owner() {
    let temp = tempfile::tempdir().unwrap();
    let paths = CorePaths {
        data_dir: temp.path().join("data"),
        config_dir: temp.path().join("config"),
        home_dir: temp.path().join("home"),
        temp_dir: temp.path().join("temp"),
        resource_dir: temp.path().join("resources"),
    };
    std::fs::create_dir_all(&paths.home_dir).unwrap();
    let host = Arc::new(TestHost::default());
    let transport = Loopback(AiCore::new(CoreContext::new(paths, Some(host.clone()))));
    let turn = Arc::new(Turn {
        id: "first-turn".into(),
        role: chat::Role::User,
        text: "Stored".into(),
        error: None,
        response_meta: None,
        references: vec![],
        stages: vec![],
        created_at: 1,
        timeline: vec![],
    });
    for n in 0..240 {
        let mut turn = (*turn).clone();
        turn.id = format!("turn-{n}");
        transport
            .send(ChatRequest::Append {
                session: format!("saved-{n}"),
                title: "Fixture".into(),
                selection: Selection::default(),
                turn: Arc::new(turn),
            })
            .await
            .unwrap();
    }
    assert_eq!(
        transport
            .send(ChatRequest::List)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        200
    );
    transport
        .send(ChatRequest::Clear(vec!["not-yet-appended".into()]))
        .await
        .unwrap();
    assert!(
        transport
            .send(ChatRequest::List)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        transport
            .send(ChatRequest::Get("saved-239".into()))
            .await
            .unwrap()
            .is_null()
    );
    assert!(!ok(&transport
        .send(ChatRequest::Append {
            session: "not-yet-appended".into(),
            title: "Late preparation".into(),
            selection: Selection::default(),
            turn
        })
        .await));
    assert!(host.calls.lock().unwrap().is_empty());
    transport.0.shutdown().await;
}
