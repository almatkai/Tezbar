use super::super::tests::fixture;
use super::*;
fn program(dir: &std::path::Path) -> std::path::PathBuf {
    let source = dir.join("mcp_fixture.rs");
    std::fs::write(&source, include_str!("mcp/fixture.rs.txt")).unwrap();
    let binary = dir.join(if cfg!(windows) {
        "mcp-fixture.exe"
    } else {
        "mcp-fixture"
    });
    let result = std::process::Command::new("rustc")
        .arg("--edition=2021")
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    binary
}
#[tokio::test]
async fn stdio_modern_and_legacy_test_enable_snapshot_call_and_encrypted_import() {
    for legacy in [false, true] {
        let (dir, core, _events) = fixture();
        let binary = program(dir.path());
        let marker = dir.path().join("effect");
        let config=json!({"mcpServers":{"Fixture":{"command":binary,"args":if legacy{vec!["legacy"]}else{vec![]},"env":{"FIXTURE_MARKER":marker,"FIXTURE_TOKEN":"fixture-secret-private"}}}}).to_string();
        let preview = core
            .handle("ai-extensions:parse-import", &[json!(config)])
            .await
            .unwrap()
            .unwrap();
        assert!(!preview.to_string().contains("fixture-secret-private"));
        assert!(!marker.exists());
        let record = core
            .handle(
                "ai-extensions:add",
                &[json!({"previewId":preview["previewId"]})],
            )
            .await
            .unwrap()
            .unwrap();
        let id = s(&record["id"]);
        assert_eq!(record["enabled"], false);
        assert!(!marker.exists());
        assert!(!std::fs::read_to_string(
            core.inner.ctx.paths.config_dir.join("ai-extensions.json")
        )
        .unwrap()
        .contains("fixture-secret-private"));
        assert!(!String::from_utf8_lossy(
            &std::fs::read(
                core.inner
                    .ctx
                    .paths
                    .config_dir
                    .join("ai-extension-secrets.bin")
            )
            .unwrap()
        )
        .contains("fixture-secret-private"));
        let tested = core
            .handle("ai-extensions:test", &[json!(id)])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(tested["ok"], true, "{tested}");
        assert!(!marker.exists());
        core.handle("ai-extensions:set-enabled", &[json!(id), json!(true)])
            .await
            .unwrap()
            .unwrap();
        let snapshot = core
            .extension_snapshot(&core.inner.ctx.paths.home_dir)
            .await
            .unwrap();
        assert_eq!(snapshot.len(), 1);
        let (name, entry) = snapshot.iter().next().unwrap();
        assert!(name.len() <= 64);
        assert!(name.contains("__"));
        assert!(validate_call(entry, json!({"text":5})).await.is_err());
        assert!(!marker.exists());
        let result = entry
            .client
            .call(
                "tools/call",
                json!({"name":"echo","arguments":{"text":"fixture"}}),
                &Cancel::new(),
                Duration::from_secs(5),
            )
            .await
            .unwrap();
        let scrubbed = entry.client.scrub(&result.to_string());
        assert!(scrubbed.contains("🦀"));
        assert!(!scrubbed.contains("fixture-secret-private"));
        assert!(marker.exists());
        let prompts = core
            .handle("ai-extensions:composer-items", &[])
            .await
            .unwrap()
            .unwrap();
        assert!(prompts
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "hello"));
        core.handle("ai-extensions:set-enabled", &[json!(id), json!(false)])
            .await
            .unwrap()
            .unwrap();
        assert!(core
            .extension_snapshot(&core.inner.ctx.paths.home_dir)
            .await
            .unwrap()
            .is_empty());
        core.shutdown().await;
    }
}
#[tokio::test]
async fn instructions_prompts_and_profile_are_data_and_resolve_to_drafts() {
    let (_dir, core, _) = fixture();
    let manifest = json!({"manifestVersion":1,"id":"notes","name":"My prompts","connections":[],"instructions":"extension instructions","prompts":[{"name":"triage","template":"Triage {{issue}}.","arguments":[{"name":"issue","required":true}]}],"agents":[{"id":"reviewer","instructions":"Review only","tools":["read"]}]});
    core.handle("ai-extensions:add", &[manifest])
        .await
        .unwrap()
        .unwrap();
    core.handle("ai-extensions:set-enabled", &[json!("notes"), json!(true)])
        .await
        .unwrap()
        .unwrap();
    core.handle(
        "ai-extensions:set-instructions",
        &[json!("global instructions"), json!(false)],
    )
    .await
    .unwrap()
    .unwrap();
    let result = core
        .handle(
            "ai-extensions:prompt-resolve",
            &[
                json!({"extensionId":"notes","name":"triage"}),
                json!({"issue":"42"}),
            ],
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result["text"], "Triage 42.");
    let (instructions, tools) = core
        .extension_instructions(
            &core.inner.ctx.paths.home_dir,
            &json!({"agentProfileId":"reviewer"}),
        )
        .await
        .unwrap();
    assert!(
        instructions.find("global instructions").unwrap()
            < instructions.find("extension instructions").unwrap()
    );
    assert!(tools.unwrap().contains("read"));
    core.shutdown().await;
}
#[test]
fn schemas_refuse_remote_refs_dialects_depth_and_validate_real_constraints() {
    for schema in [
        json!({"type":"object","$ref":"https://example.com/schema"}),
        json!({"type":"object","$schema":"https://unknown"}),
    ] {
        assert!(mcp::schema::compile(&schema).is_err());
    }
    let schema = json!({"type":"object","$defs":{"value":{"type":"integer","minimum":2}},"properties":{"value":{"$ref":"#/$defs/value"}},"required":["value"],"additionalProperties":false});
    let validator = mcp::schema::compile(&schema).unwrap();
    assert!(validator.is_valid(&json!({"value":2})));
    assert!(!validator.is_valid(&json!({"value":1})));
    assert!(!mcp::schema::for_wire(&schema)
        .unwrap()
        .to_string()
        .contains("$ref"));
}

#[tokio::test]
async fn modern_and_legacy_http_tools_execute_through_real_agent_dispatch() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    for legacy in [false, true] {
        let (_dir, core, mut events) = fixture();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        let effects = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = effects.clone();
        let task = tokio::spawn(async move {
            loop {
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
                    if let Some(i) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        end = i + 4;
                        break;
                    }
                }
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let length = headers
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap();
                while bytes.len() < end + length {
                    let mut b = [0; 4096];
                    let n = socket.read(&mut b).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&b[..n]);
                }
                let v: Value = serde_json::from_slice(&bytes[end..end + length]).unwrap();
                let method = s(&v["method"]);
                let mut reply = json!({"jsonrpc":"2.0","id":v["id"]});
                reply["result"] = match method {
                    "server/discover" if !legacy => {
                        assert_eq!(
                            v["params"]["_meta"]["io.modelcontextprotocol/clientCapabilities"],
                            json!({})
                        );
                        json!({"capabilities":{"tools":{}}})
                    }
                    "server/discover" => {
                        reply["error"] = json!({"code":-32601,"message":"Method not found"});
                        Value::Null
                    }
                    "initialize" => {
                        json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{}}})
                    }
                    "tools/list" => {
                        json!({"tools":[{"name":"echo","inputSchema":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"]},"annotations":{"readOnlyHint":true}}]})
                    }
                    "tools/call" => {
                        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        json!({"content":[{"type":"text","text":"native web result 🦀"}]})
                    }
                    _ => json!({}),
                };
                let notification = v["id"].is_null();
                let body = if notification {
                    String::new()
                } else {
                    reply.to_string()
                };
                let status = if notification { 202 } else { 200 };
                socket.write_all(format!("HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nMCP-Session-Id: fixture-session\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
        });
        let manifest = json!({"manifestVersion":1,"id":"web","name":"Web fixture","connections":[{"id":"web","name":"Web","transport":"http","url":url,"toolPolicy":"allowReadOnly"}]});
        core.handle("ai-extensions:add", &[manifest])
            .await
            .unwrap()
            .unwrap();
        let tested = core
            .handle("ai-extensions:test", &[json!("web")])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(tested["ok"], true, "{tested}");
        core.handle("ai-extensions:set-enabled", &[json!("web"), json!(true)])
            .await
            .unwrap()
            .unwrap();
        let snapshot = core
            .extension_snapshot(&core.inner.ctx.paths.home_dir)
            .await
            .unwrap();
        let name = snapshot.keys().next().unwrap();
        let first = format!(
            "data: {}\n\n",
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"web_call","function":{"name":name,"arguments":"{\"text\":\"fixture\"}"}}]},"finish_reason":"tool_calls"}]})
        );
        let final_reply = format!(
            "data: {}\n\n",
            json!({"choices":[{"delta":{"content":"done"},"finish_reason":"stop"}]})
        );
        let mut provider = super::super::canonical_tests::server(vec![
            ("text/event-stream".into(), first),
            ("text/event-stream".into(), final_reply),
        ])
        .await;
        core.patch_config(json!({"provider":"openai-compatible","model":"fixture","providerConfigs":{"openai-compatible":{"baseURL":provider.url}}})).await.unwrap();
        core.handle(
            "agent:run",
            &[json!({"runId":"agent-web","task":"use fixture"})],
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            super::super::tests::next_event(&mut events, "answer").await["text"],
            "done"
        );
        super::super::tests::next_event(&mut events, "done").await;
        assert_eq!(effects.load(std::sync::atomic::Ordering::SeqCst), 1);
        provider.requests.recv().await.unwrap();
        let resumed = provider.requests.recv().await.unwrap();
        assert!(resumed.to_string().contains("native web result 🦀"));
        assert_eq!(resumed["body"]["messages"][3]["tool_call_id"], "web_call");
        core.shutdown().await;
        task.abort();
    }
}
