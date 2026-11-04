use super::*;
use common::CorePaths;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
pub(super) fn fixture() -> (
    tempfile::TempDir,
    AiCore,
    tokio::sync::mpsc::UnboundedReceiver<(String, Value)>,
) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let paths = CorePaths {
        data_dir: root.join("data"),
        config_dir: root.join("config"),
        resource_dir: root.join("resources"),
        temp_dir: root.join("tmp"),
        home_dir: root.join("home"),
    };
    for p in [
        &paths.data_dir,
        &paths.config_dir,
        &paths.resource_dir,
        &paths.temp_dir,
        &paths.home_dir,
    ] {
        std::fs::create_dir_all(p).unwrap();
    }
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let ctx = CoreContext::headless(paths).with_event_sink(Arc::new(move |channel, value| {
        let _ = tx.send((channel.to_string(), value));
    }));
    let core = AiCore::new(ctx);
    let bin = root.join("no-runtime-bin");
    std::fs::create_dir_all(&bin).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for name in ["pi", "node", "npm", "pnpm", "bun", "opencode"] {
            let path = bin.join(name);
            std::fs::write(&path,"#!/bin/sh\nif [ \"$(basename \"$0\")\" = opencode ] && [ \"$1\" = models ]; then printf 'opencode/fixture\\n'; exit 0; fi\nprintf forbidden > \"$(dirname \"$0\")/forbidden\"\nexit 91\n").unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    #[cfg(windows)]
    {
        let executable = no_runtime_marker();
        for name in ["pi", "node", "npm", "pnpm", "bun", "opencode"] {
            std::fs::hard_link(&executable, bin.join(format!("{name}.exe"))).unwrap();
        }
    }
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap()
    .to_string_lossy()
    .into_owned();
    core.inner.login_path.set(path).unwrap();
    (dir, core, rx)
}
pub(super) async fn next_event(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<(String, Value)>,
    kind: &str,
) -> Value {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let (channel, event) = rx.recv().await.unwrap();
            if channel == "agent:event" && event["type"] == kind {
                return event;
            }
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
async fn approval_matching_cancellation_and_exact_persistence() {
    let (_dir, core, mut events) = fixture();
    let cancel = Cancel::new();
    core.inner.state.lock().unwrap().agent = Some(Run {
        id: "agent-test".into(),
        cancel: cancel.clone(),
        emitted: false,
    });
    let owner = core.clone();
    let pending = tokio::spawn(async move {
        owner
            .approval_prompt(
                "agent-test",
                "Allow?",
                "rg needle file | head -5",
                &cancel,
                true,
            )
            .await
    });
    let event = next_event(&mut events, "approval").await;
    assert_eq!(event["suggestedRule"], "rg");
    assert_eq!(
        core.approve(
            &json!({"runId":"agent-other","approvalId":event["approvalId"],"decision":"always"})
        )
        .await
        .unwrap()["ok"],
        false
    );
    assert_eq!(
        core.approve(
            &json!({"runId":"agent-test","approvalId":event["approvalId"],"decision":"always"})
        )
        .await
        .unwrap()["ok"],
        true
    );
    assert!(pending.await.unwrap());
    let raw = core.read_config().await.unwrap();
    assert_eq!(
        raw["agentAlwaysAllowedExactCommands"],
        json!(["rg needle file | head -5"])
    );
    assert!(raw.get("agentAlwaysAllowedCommands").is_none());
    let cancel = Cancel::new();
    core.inner.state.lock().unwrap().agent = Some(Run {
        id: "agent-new".into(),
        cancel: cancel.clone(),
        emitted: false,
    });
    let owner = core.clone();
    let pending = tokio::spawn(async move {
        owner
            .approval_prompt("agent-new", "Allow?", "rm file", &cancel, true)
            .await
    });
    let event = next_event(&mut events, "approval").await;
    core.cancel_run(Some("agent-old"), false);
    assert!(!core
        .inner
        .state
        .lock()
        .unwrap()
        .agent
        .as_ref()
        .unwrap()
        .cancel
        .is_cancelled());
    core.cancel_run(Some("agent-new"), false);
    assert!(!pending.await.unwrap());
    assert_eq!(
        core.approve(
            &json!({"runId":"agent-new","approvalId":event["approvalId"],"decision":"once"})
        )
        .await
        .unwrap()["ok"],
        false
    );
    core.shutdown().await;
}
#[tokio::test]
async fn chat_metadata_handles_multibyte_answer_boundaries_without_panicking() {
    let (_dir, core, _events) = fixture();
    // Missing credentials intentionally exercise the local fallback, without a provider request.
    core.patch_config(json!({"provider":"openai","providerConfigs":{"openai":{"apiKey":""}}}))
        .await
        .unwrap();
    for answer in [
        format!("A{}", "я".repeat(200)),
        format!("A{}", "🦀".repeat(100)),
        "x".repeat(400),
    ] {
        let meta = core
            .handle(
                "chat:generate-meta",
                &[json!({"id":"unicode-chat","prompt":"Unicode response","answer":answer})],
            )
            .await
            .unwrap()
            .unwrap();
        assert!(!s(&meta["title"]).is_empty());
        assert!(!s(&meta["icon"]).is_empty());
    }
    core.shutdown().await;
}
#[tokio::test]
async fn extension_ask_uses_native_http_and_action_provider() {
    let (_dir, core, _events) = fixture();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    core.patch_config(json!({"provider":"ollama","taskProviderOverrides":{"action":"custom:local"},"customProviders":[{"id":"custom:local","title":"Local fixture"}],"providerConfigs":{"custom:local":{"baseURL":base,"apiKey":"fixture"}},"providerSelectedModels":{"custom:local":"mock-model"}})).await.unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        loop {
            let mut chunk = [0; 4096];
            let n = socket.read(&mut chunk).await.unwrap();
            request.extend_from_slice(&chunk[..n]);
            if let Some(i) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&request[..i]).to_lowercase();
                let length = header
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .unwrap()
                    .parse::<usize>()
                    .unwrap();
                if request.len() >= i + 4 + length {
                    let body: Value =
                        serde_json::from_slice(&request[i + 4..i + 4 + length]).unwrap();
                    assert_eq!(body["model"], "mock-model");
                    assert_eq!(body["messages"][1]["content"], "test prompt");
                    break;
                }
            }
        }
        let data =
            "data: {\"choices\":[{\"delta\":{\"content\":\" native 🦀 \"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",data.len()).as_bytes()).await.unwrap();
        for byte in data.as_bytes() {
            socket.write_all(&[*byte]).await.unwrap();
        }
        socket.shutdown().await.unwrap();
    });
    let result = core
        .handle("core:ai:extension-ask", &[json!("test prompt"), json!({})])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result, "native 🦀");
    server.await.unwrap();
    core.shutdown().await;
}
#[tokio::test]
async fn legacy_aliases_report_native_state_without_a_runtime() {
    let (_dir, core, _events) = fixture();
    assert_eq!(
        core.handle("pi:check", &[]).await.unwrap().unwrap(),
        json!({"installed":true,"version":"native"})
    );
    assert_eq!(
        core.handle("pi:install", &[]).await.unwrap().unwrap(),
        json!({"ok":true,"method":"native","version":"native"})
    );
    assert_eq!(
        core.handle("core:ai:pi-observe", &[])
            .await
            .unwrap()
            .unwrap()["isStreaming"],
        false
    );
    assert!(core
        .handle("core:ai:pi-query", &[json!({"type":"prompt"})])
        .await
        .unwrap()
        .is_err());
    core.shutdown().await;
}

#[test]
fn dispatcher_future_can_be_installed_in_send_router() {
    let (_dir, core, _events) = fixture();
    fn send<T: Send>(_: T) {}
    send(core.handle("llm-config-get", &[]));
}

#[tokio::test]
async fn hotkey_patch_preserves_platform_normalization_and_rejects_atomically() {
    let (_dir, core, _events) = fixture();
    let config = core.inner.ctx.config.clone();
    core.inner
        .ctx
        .set_router(Arc::new(move |channel, args| {
            let config = config.clone();
            Box::pin(async move {
                assert_eq!(channel, "core:platform:update-launcher-hotkey");
                if args[0] == "invalid" {
                    return Ok(json!({"ok":false,"error":"Invalid shortcut"}));
                }
                assert_eq!(args[0], "ctrl+space");
                config.patch(&json!({"raymesHotkey":"Control+Space"}))?;
                Ok(json!({"ok":true,"accelerator":"Control+Space"}))
            })
        }))
        .unwrap();
    let result = core
        .handle(
            "llm-config-set",
            &[json!({"raymesHotkey":"ctrl+space","memoryEnabled":true})],
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result["accelerator"], "Control+Space");
    let raw = core.read_config().await.unwrap();
    assert_eq!(raw["raymesHotkey"], "Control+Space");
    assert_eq!(raw["memoryEnabled"], true);
    assert_eq!(
        core.handle(
            "llm-config-set",
            &[json!({"raymesHotkey":"invalid","memoryEnabled":false})]
        )
        .await
        .unwrap()
        .unwrap()["ok"],
        false
    );
    assert_eq!(core.read_config().await.unwrap(), raw);
    core.shutdown().await;
}

#[tokio::test]
async fn superseded_answer_cannot_complete_replacement_stream() {
    let (_dir, core, mut events) = fixture();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    core.patch_config(
        json!({"provider":"ollama","baseURL":format!("http://{}", listener.local_addr().unwrap())}),
    )
    .await
    .unwrap();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        // The old request holds an incomplete body until it is cancelled.
        let (mut old, _) = listener.accept().await.unwrap();
        let mut request = [0; 8192];
        old.read(&mut request).await.unwrap();
        old.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10000\r\n\r\n")
            .await
            .unwrap();
        ready_tx.send(()).unwrap();
        let (mut new, _) = listener.accept().await.unwrap();
        new.read(&mut request).await.unwrap();
        release_rx.await.unwrap();
        let body = "{\"message\":{\"content\":\"replacement\"},\"done\":true}\n";
        new.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    });
    core.answer("old request".into());
    ready_rx.await.unwrap();
    core.answer("new request".into());
    // The cancelled request must settle while the replacement is still waiting.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        events.try_recv().is_err(),
        "Stale completion leaked into the replacement stream"
    );
    release_tx.send(()).unwrap();
    let (channel, value) = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (channel.as_str(), value),
        ("stream-token", json!("replacement"))
    );
    assert_eq!(events.recv().await.unwrap().0, "stream-done");
    server.await.unwrap();
    core.shutdown().await;
}

#[cfg(windows)]
fn no_runtime_marker() -> std::path::PathBuf {
    static FIXTURE: std::sync::OnceLock<(tempfile::TempDir, std::path::PathBuf)> =
        std::sync::OnceLock::new();
    FIXTURE.get_or_init(||{let dir=tempfile::tempdir().unwrap();let source=dir.path().join("marker.rs");let exe=dir.path().join("marker.exe");std::fs::write(&source,r#"fn main(){let exe=std::env::current_exe().unwrap();if exe.file_stem().unwrap()=="opencode"&&std::env::args().nth(1).as_deref()==Some("models"){println!("opencode/fixture");return}std::fs::write(exe.parent().unwrap().join("forbidden"),b"forbidden").unwrap();std::process::exit(91)}"#).unwrap();let result=std::process::Command::new("rustc").arg("--edition=2021").arg(&source).arg("-o").arg(&exe).output().unwrap();assert!(result.status.success(),"{}",String::from_utf8_lossy(&result.stderr));(dir,exe)}).1.clone()
}
#[test]
fn native_source_has_only_reviewed_process_seams_and_never_stages_pi_policy() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let ai = root.join("src/native_core/ai");
    for file in walkdir::WalkDir::new(&ai)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|s| s == "rs"))
    {
        let relative = file
            .path()
            .strip_prefix(&ai)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if relative.ends_with("tests.rs") || relative == "fixture_http.rs" {
            continue;
        }
        let source = std::fs::read_to_string(file.path()).unwrap();
        for forbidden in [
            "pi_binary",
            "pi_run(",
            "opencode run",
            ".command(\"pi\")",
            ".command(\"npm\")",
            ".command(\"node\")",
        ] {
            assert!(!source.contains(forbidden), "{relative}: {forbidden}")
        }
        let production = source.split("#[cfg(test)]").next().unwrap();
        if production.contains("process::command(")
            || production.contains(".command(")
            || production.contains("Command::new(")
        {
            assert!(
                [
                    "mod.rs",
                    "process.rs",
                    "legacy.rs",
                    "native_tools.rs",
                    "provider.rs",
                    "voice.rs",
                    "ext/mcp/client.rs"
                ]
                .contains(&relative.as_str()),
                "Unreviewed process seam: {relative}"
            )
        }
    }
    for removed in [
        "pi.rs",
        "pi_loop.rs",
        "native.rs",
        "native_protocol.rs",
        "gateway.rs",
    ] {
        assert!(!ai.join(removed).exists())
    }
    assert!(!std::fs::read_to_string(root.join("tauri.conf.json"))
        .unwrap()
        .contains("raymes-pi-policy.ts"));
    let provider = std::fs::read_to_string(ai.join("provider.rs")).unwrap();
    assert!(!provider.contains("self.command(\"opencode\")"));
}
