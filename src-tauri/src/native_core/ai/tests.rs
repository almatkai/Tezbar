use super::*;
use common::CorePaths;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn fixture() -> (
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
    core.inner
        .login_path
        .set(std::env::var("PATH").unwrap_or_default())
        .unwrap();
    (dir, core, rx)
}
async fn next_event(
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
            .approval("agent-test", "Allow?", "rg needle file | head -5", &cancel)
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
            .approval("agent-new", "Allow?", "rm file", &cancel)
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
#[cfg(unix)]
fn mock_pi(core: &AiCore, scenario: &str) {
    use std::os::unix::fs::PermissionsExt;
    let path = core.inner.ctx.paths.resource_dir.join("pi-mock");
    let script=r#"#!/usr/bin/env python3
import json,sys,pathlib,time
base=pathlib.Path(__file__).parent
scenario=SCENARIO
count=1
# Only retry tests need a counter; observer RPCs intentionally run concurrently.
if scenario in ('retry','partial'):
    counter=base/'launches'
    count=int(counter.read_text())+1 if counter.exists() else 1
    counter.write_text(str(count))
if scenario=='retry' and count==1:
    sys.stderr.write('startup failed\n')
    sys.exit(3)
def send(v,tail=False):
    data=json.dumps(v,ensure_ascii=False).encode()+ (b'' if tail else b'\n')
    for byte in data:
        sys.stdout.buffer.write(bytes([byte]))
        sys.stdout.buffer.flush()
def finish(text):
    message={'role':'assistant','stopReason':'stop','content':[{'type':'text','text':text}]}
    send({'type':'message_end','message':message})
    send({'type':'agent_end','willRetry':False,'messages':[message]})
    send({'type':'agent_settled'},True)
for line in sys.stdin:
    request=json.loads(line)
    if request['type']=='set_model':
        send({'type':'response','id':'wrong-id','success':False,'error':'ignore stale'})
        send({'type':'response','id':request['id'],'success':True})
    elif request['type']=='get_state':
        send({'type':'response','id':request['id'],'success':True,'data':{'model':{'provider':'mock','id':'fixture'},'messageCount':3,'isStreaming':False}})
        break
    elif request['type']=='get_session_stats':
        send({'type':'response','id':request['id'],'success':True,'data':{'toolCalls':2,'contextUsage':{'tokens':10,'contextWindow':100,'percent':10}}})
        break
    elif request['type']=='prompt':
        send({'type':'response','id':request['id'],'success':True})
        send({'type':'agent_start'})
        if scenario=='partial':
            send({'type':'message_update','assistantMessageEvent':{'type':'text_delta','delta':'partial 🦀'}})
            sys.stderr.write('transport lost\n')
            sys.exit(2)
        if scenario=='approval':
            send({'type':'extension_ui_request','id':'ui-1','method':'confirm','title':'Allow command?','message':'echo exact'})
        else:
            finish('complete 🦀\u2028\u2029 end')
            break
    elif request['type']=='extension_ui_response':
        finish('approved' if request.get('confirmed') else 'denied')
        break
"#.replace("SCENARIO",&format!("{scenario:?}"));
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let policy = core
        .inner
        .ctx
        .paths
        .resource_dir
        .join("agent/raymes-pi-policy.ts");
    std::fs::create_dir_all(policy.parent().unwrap()).unwrap();
    std::fs::write(policy, "// Fixture: mock CLI never executes this file\n").unwrap();
    *core.inner.pi_override.lock().unwrap() = Some(path);
}
#[tokio::test]
#[cfg(unix)]
async fn subprocess_retry_is_once_and_partial_output_never_reexecutes() {
    for (scenario, expected) in [("retry", "2"), ("partial", "1")] {
        let (_dir, core, mut events) = fixture();
        mock_pi(&core, scenario);
        let result = core
            .handle(
                "agent:run",
                &[json!({"task":"test","runId":"agent-fixture"})],
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result["ok"], true);
        let terminal = if scenario == "retry" {
            let answer = next_event(&mut events, "answer").await;
            assert_eq!(answer["text"], "complete 🦀\u{2028}\u{2029} end");
            next_event(&mut events, "done").await
        } else {
            let delta = next_event(&mut events, "message").await;
            assert_eq!(delta["delta"], "partial 🦀");
            let error = next_event(&mut events, "error").await;
            assert!(s(&error["message"]).contains("transport lost"));
            next_event(&mut events, "done").await
        };
        assert_eq!(terminal["runId"], "agent-fixture");
        assert_eq!(
            tokio::fs::read_to_string(core.inner.ctx.paths.resource_dir.join("launches"))
                .await
                .unwrap(),
            expected
        );
        core.shutdown().await;
    }
}
#[tokio::test]
#[cfg(unix)]
async fn subprocess_approval_rpc_response_is_correlated() {
    let (_dir, core, mut events) = fixture();
    mock_pi(&core, "approval");
    // Exercise the actual Tauri resource mapping as well as the flat resource contract.
    let resources = &core.inner.ctx.paths.resource_dir;
    std::fs::create_dir_all(resources.join("dist-backend")).unwrap();
    std::fs::rename(
        resources.join("agent"),
        resources.join("dist-backend/agent"),
    )
    .unwrap();
    core.handle(
        "agent:run",
        &[json!({"task":"test","runId":"agent-approval"})],
    )
    .await
    .unwrap()
    .unwrap();
    let event = next_event(&mut events, "approval").await;
    core.handle(
        "agent:approve",
        &[json!({"runId":"agent-approval","approvalId":event["approvalId"],"decision":"deny"})],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(next_event(&mut events, "answer").await["text"], "denied");
    next_event(&mut events, "done").await;
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
            "data: {\"choices\":[{\"delta\":{\"content\":\" native 🦀 \"}}]}\n\ndata: [DONE]\n\n";
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
async fn gateway_auth_limits_routing_and_launcher_filters() {
    let (_dir, core, _events) = fixture();
    core.inner.ctx.set_router(Arc::new(|channel,args|Box::pin(async move{match channel.as_str(){"core:knowledge:search"=>{assert_eq!(args,json!(["needle",20]).as_array().unwrap().clone());Ok(json!([{"id":"hit"}]))},"core:knowledge:read"=>Ok(json!({"id":args[0],"text":"content"})),"search:all"=>Ok(json!([{"id":"deep-search:hidden"},{"id":"note-add:hidden"},{"id":"file:a","title":"A","action":{"type":"open-file","path":"/fixture/a"}}])),_=>Err("Unexpected channel".into())}}))).unwrap();
    let (endpoint, token) = core.ensure_gateway().await.unwrap();
    let base = endpoint.trim_end_matches("/search");
    let http = reqwest::Client::new();
    assert_eq!(
        http.post(&endpoint)
            .json(&json!({"query":"needle"}))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let response = http
        .post(&endpoint)
        .bearer_auth(&token)
        .json(&json!({"query":"needle","limit":100}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(response, json!({"hits":[{"id":"hit"}]}));
    let response = http
        .post(format!("{base}/launcher-search"))
        .bearer_auth(&token)
        .json(&json!({"query":"a"}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(response["results"].as_array().unwrap().len(), 1);
    assert_eq!(response["results"][0]["target"], "/fixture/a");
    assert_eq!(
        http.post(&endpoint)
            .bearer_auth(&token)
            .body("x".repeat(65537))
            .send()
            .await
            .unwrap()
            .status(),
        413
    );
    core.shutdown().await;
    assert!(http.post(&endpoint).send().await.is_err());
}

#[tokio::test]
#[cfg(unix)]
async fn observer_queries_return_native_pi_state_and_usage() {
    let (_dir, core, _events) = fixture();
    mock_pi(&core, "observer");
    let observed = core
        .handle("core:ai:pi-observe", &[])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(observed["model"], "mock/fixture");
    assert_eq!(observed["messageCount"], 3);
    assert_eq!(observed["toolCalls"], 2);
    assert_eq!(observed["contextUsage"]["tokens"], 10);
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
