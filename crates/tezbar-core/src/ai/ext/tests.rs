use super::super::tests::fixture;
use super::*;
#[tokio::test]
async fn agent_and_provider_updates_validate_and_preserve_unedited_opaque_refs() {
    let (_dir, core, _events) = fixture();
    let record = json!({"manifestVersion":1,"id":"m7-update","name":"M7","agents":[{"id":"a","instructions":"old"}],"providers":[{"id":"custom:m7","title":"M7 provider","baseURL":"https://old.example","headers":{"Authorization":{"secret":"opaque-ref","label":"Key"}}}]});
    core.handle("ai-extensions:add", &[record])
        .await
        .unwrap()
        .unwrap();
    core.handle(
        "ai-extensions:set-secret",
        &[
            json!("m7-update"),
            json!("opaque-ref"),
            json!("fixture-original-key"),
            json!({"providerId":"custom:m7"}),
        ],
    )
    .await
    .unwrap()
    .unwrap();
    let edited = core
        .handle(
            "ai-extensions:update",
            &[
                json!("m7-update"),
                json!({"agents":[{"id":"a","instructions":"new"}]}),
            ],
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(edited["agents"][0]["instructions"], "new");
    assert_eq!(
        edited["providers"][0]["headers"]["Authorization"]["secret"],
        "opaque-ref"
    );
    let mut providers = edited["providers"].clone();
    providers[0]["baseURL"] = json!("https://new.example");
    providers[0]["headers"]["X-Api-Key"] = json!("fixture-new-key");
    let edited = core
        .handle(
            "ai-extensions:update",
            &[json!("m7-update"), json!({"providers":providers})],
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(edited["providers"][0]["baseURL"], "https://new.example");
    assert_eq!(
        edited["providers"][0]["headers"]["Authorization"]["secret"],
        "opaque-ref"
    );
    assert_eq!(edited["agents"][0]["instructions"], "new");
    assert_eq!(
        edited["providers"][0]["headers"]["X-Api-Key"]["secret"],
        "X-Api-Key"
    );
    let secrets = core
        .inner
        .extensions
        .secrets
        .values(&provider_key("m7-update", "custom:m7"))
        .unwrap();
    assert_eq!(
        secrets.get("opaque-ref").map(String::as_str),
        Some("fixture-original-key")
    );
    assert_eq!(
        secrets.get("X-Api-Key").map(String::as_str),
        Some("fixture-new-key")
    );
    let durable =
        std::fs::read_to_string(core.inner.ctx.paths.config_dir.join("ai-extensions.json"))
            .unwrap();
    assert!(!durable.contains("fixture-new-key") && !durable.contains("fixture-original-key"));
    let mut bad = edited["providers"].clone();
    bad[0]["baseURL"] = json!("file:///unsafe");
    assert!(core
        .handle(
            "ai-extensions:update",
            &[json!("m7-update"), json!({"providers":bad})]
        )
        .await
        .unwrap()
        .is_err());
    let data = core.ext_read().await.unwrap();
    assert_eq!(
        data["extensions"][0]["providers"][0]["baseURL"],
        "https://new.example"
    );
    core.shutdown().await;
}
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
        assert!(snapshot
            .values()
            .any(|entry| entry.descriptor["name"] == "echo"));
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
                if headers.starts_with("GET ") {
                    socket.write_all(b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
                    continue;
                }
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

#[tokio::test]
async fn project_manifest_never_starts_before_review_and_byte_changes_revoke_trust() {
    let (dir, core, _events) = fixture();
    let project = dir.path().join("project");
    std::fs::create_dir_all(project.join(".tezbar")).unwrap();
    let bin = program(dir.path());
    let marker = dir.path().join("project-start");
    let manifest = json!({"manifestVersion":1,"id":"project-fixture","name":"Project fixture","connections":[{"id":"fixture","name":"Fixture","transport":"stdio","command":bin,"env":{"FIXTURE_START_MARKER":marker}}]});
    let file = project.join(".tezbar/ai-extension.json");
    std::fs::write(&file, manifest.to_string()).unwrap();
    let notice = core.project_status(&project).await.unwrap();
    assert!(notice["entryHash"].is_string());
    assert!(!marker.exists());
    assert!(core.extension_snapshot(&project).await.unwrap().is_empty());
    assert!(!marker.exists());
    let record = core
        .handle(
            "ai-extensions:trust-project",
            &[json!(project), notice["entryHash"].clone()],
        )
        .await
        .unwrap()
        .unwrap();
    let id = s(&record["id"]);
    assert_eq!(record["enabled"], false);
    assert!(!marker.exists());
    assert_eq!(
        core.handle("ai-extensions:test", &[json!(id)])
            .await
            .unwrap()
            .unwrap()["ok"],
        true
    );
    assert!(marker.exists());
    core.handle("ai-extensions:set-enabled", &[json!(id), json!(true)])
        .await
        .unwrap()
        .unwrap();
    assert!(core
        .extension_snapshot(&project)
        .await
        .unwrap()
        .values()
        .any(|entry| entry.descriptor["name"] == "echo"));
    assert!(core
        .extension_snapshot(dir.path())
        .await
        .unwrap()
        .is_empty());
    std::fs::remove_file(&marker).unwrap();
    std::fs::write(&file, manifest.to_string() + "\n").unwrap();
    assert!(core.extension_snapshot(&project).await.unwrap().is_empty());
    assert!(!marker.exists());
    let data = core
        .handle("ai-extensions:list", &[json!(project)])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(data["extensions"][0]["enabled"], false);
    assert!(data["extensions"][0]["needsProjectReview"] == true);
    let latest = core.project_status(&project).await.unwrap();
    let retrusted = core
        .trust_project(project.to_str().unwrap(), s(&latest["entryHash"]))
        .await
        .unwrap();
    assert_eq!(retrusted["needsProjectReview"], false);
    assert_eq!(
        core.ext_read().await.unwrap()["extensions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(!marker.exists(), "re-trusting must not start a connection");
    core.shutdown().await;
}
#[tokio::test]
#[cfg(unix)]
async fn agents_md_cannot_smuggle_a_sensitive_symlink_into_the_system_prompt() {
    let (dir, core, _events) = fixture();
    let project = dir.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    let ssh = core.inner.ctx.paths.home_dir.join(".ssh");
    std::fs::create_dir_all(&ssh).unwrap();
    let sensitive = ssh.join("fixture-key");
    std::fs::write(&sensitive, "fixture-sensitive-key").unwrap();
    std::os::unix::fs::symlink(&sensitive, project.join("AGENTS.md")).unwrap();
    let (text, _) = core
        .extension_instructions(&project, &Value::Null)
        .await
        .unwrap();
    assert!(!text.contains("fixture-sensitive-key"));
    core.shutdown().await;
}

#[tokio::test]
async fn http_modern_error_negotiates_legacy_and_expired_sessions_restart_only_discovery() {
    use super::super::fixture_http::{server, Reply};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (_dir, core, _) = fixture();
    let initializations = Arc::new(AtomicUsize::new(0));
    let init_count = initializations.clone();
    let lists = Arc::new(AtomicUsize::new(0));
    let list_count = lists.clone();
    let tools = Arc::new(AtomicUsize::new(0));
    let tool_count = tools.clone();
    let http=server(move|r|{let v=&r["body"];let mut reply=Reply::json(json!({"jsonrpc":"2.0","id":v["id"],"result":{}}));match s(&v["method"]){
        "server/discover"=>return Reply::json(json!({"jsonrpc":"2.0","id":v["id"],"error":{"code":-32022,"message":"Unsupported version","data":{"supported":["2025-06-18"]}}})).status(400),
        "initialize"=>{let n=init_count.fetch_add(1,Ordering::SeqCst)+1;reply=Reply::json(json!({"jsonrpc":"2.0","id":v["id"],"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}}}}));reply.headers.push(("MCP-Session-Id".into(),format!("session-{n}")));},
        "tools/list"=>{if list_count.fetch_add(1,Ordering::SeqCst)==0{return reply.status(404)}reply=Reply::json(json!({"jsonrpc":"2.0","id":v["id"],"result":{"tools":[{"name":"effect","inputSchema":{"type":"object","properties":{}},"annotations":{"readOnlyHint":true}}]}}));},
        "tools/call"=>{if tool_count.fetch_add(1,Ordering::SeqCst)==0{return reply.status(404)}reply=Reply::json(json!({"jsonrpc":"2.0","id":v["id"],"result":{"content":[{"type":"text","text":"new model decision"}]}}));},_=>{}
    }reply}).await;
    core.ext_handle("ai-extensions:add",&[json!({"manifestVersion":1,"id":"session","name":"Session fixture","connections":[{"id":"web","name":"Web","transport":"http","url":http.url,"toolPolicy":"allowReadOnly"}]})]).await.unwrap();
    let tested = core
        .ext_handle("ai-extensions:test", &[json!("session")])
        .await
        .unwrap();
    assert_eq!(tested["ok"], true, "{tested}");
    assert_eq!(initializations.load(Ordering::SeqCst), 2);
    core.ext_handle(
        "ai-extensions:set-enabled",
        &[json!("session"), json!(true)],
    )
    .await
    .unwrap();
    let snapshot = core
        .extension_snapshot(&core.inner.ctx.paths.home_dir)
        .await
        .unwrap();
    let entry = snapshot.values().next().unwrap();
    let ready_initializations = initializations.load(Ordering::SeqCst);
    let error = core
        .extension_call(entry, json!({}), "run", &Cancel::new(), None)
        .await
        .unwrap_err();
    assert!(error.contains("not replayed"));
    assert_eq!(tools.load(Ordering::SeqCst), 1);
    let result = core
        .extension_call(entry, json!({}), "run", &Cancel::new(), None)
        .await
        .unwrap();
    assert!(!result.1);
    assert_eq!(
        initializations.load(Ordering::SeqCst),
        ready_initializations + 1
    );
    assert_eq!(tools.load(Ordering::SeqCst), 2);
    core.shutdown().await;
}
#[tokio::test]
async fn http_oauth_discovery_dcr_pkce_resource_refresh_and_secret_persistence() {
    use super::super::fixture_http::{server, Reply};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (_dir, core, _) = fixture();
    let origin = Arc::new(Mutex::new(String::new()));
    let base = origin.clone();
    let exchanges = Arc::new(AtomicUsize::new(0));
    let count = exchanges.clone();
    let registrations = Arc::new(AtomicUsize::new(0));
    let registrations_count = registrations.clone();
    let expected_challenge = Arc::new(Mutex::new(String::new()));
    let challenge = expected_challenge.clone();
    let http=server(move|r|{let base=base.lock().unwrap().clone();let resource=format!("{base}/mcp");let line=s(&r["line"]);if line.starts_with("GET /mcp "){let mut reply=Reply::json(json!({})).status(401);reply.headers.push(("WWW-Authenticate".into(),format!("Bearer resource_metadata=\"{base}/resource-metadata\"")));return reply}
        if line.contains("/resource-metadata "){return Reply::json(json!({"resource":resource,"authorization_servers":[format!("{base}/issuer")]}))}
        if line.contains("/.well-known/oauth-authorization-server/issuer "){return Reply::json(json!({"issuer":format!("{base}/issuer"),"authorization_endpoint":format!("{base}/authorize"),"token_endpoint":format!("{base}/token"),"registration_endpoint":format!("{base}/register"),"code_challenge_methods_supported":["S256"]}))}
        if line.contains("/register "){registrations_count.fetch_add(1,Ordering::SeqCst);assert_eq!(r["body"]["application_type"],"native");return Reply::json(json!({"client_id":"fixture-registered-client"}))}
        if line.contains("/token "){let form=reqwest::Url::parse(&format!("http://fixture/?{}",s(&r["raw"]))).unwrap().query_pairs().map(|(k,v)|(k.into_owned(),v.into_owned())).collect::<HashMap<_,_>>();assert_eq!(form["resource"],resource);let n=count.fetch_add(1,Ordering::SeqCst);if n==0{use base64::Engine;use sha2::Digest;let actual=base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(form["code_verifier"].as_bytes()));assert_eq!(actual,*challenge.lock().unwrap());assert_eq!(form["grant_type"],"authorization_code")}else{assert_eq!(form["grant_type"],"refresh_token");assert_eq!(form["refresh_token"],"fixture-oauth-refresh")};return Reply::json(json!({"access_token":if n==0{"fixture-oauth-access"}else{"fixture-oauth-new"},"refresh_token":"fixture-oauth-refresh","token_type":"Bearer","expires_in":3600}))}
        let v=&r["body"];let result=match s(&v["method"]){"server/discover"=>json!({"capabilities":{"tools":{}}}),"tools/list"=>json!({"tools":[{"name":"echo","inputSchema":{"type":"object","properties":{}},"annotations":{"readOnlyHint":true}}]}),"tools/call"=>{if r["headers"]["authorization"]=="Bearer fixture-oauth-access"{return Reply::json(json!({})).status(401)}assert_eq!(r["headers"]["authorization"],"Bearer fixture-oauth-new");json!({"content":[{"type":"text","text":"result fixture-oauth-new"}]})},_=>json!({})};Reply::json(json!({"jsonrpc":"2.0","id":v["id"],"result":result}))
    }).await;
    *origin.lock().unwrap() = http.url.clone();
    let port = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    core.ext_handle("ai-extensions:add",&[json!({"manifestVersion":1,"id":"oauth","name":"OAuth fixture","connections":[{"id":"web","name":"Web","transport":"http","url":format!("{}/mcp",http.url),"auth":{"type":"oauth","callbackPort":port},"toolPolicy":"allowReadOnly"}]})]).await.unwrap();
    let started = core
        .ext_handle("ai-extensions:oauth-start", &[json!("oauth"), json!("web")])
        .await
        .unwrap();
    let authorize = reqwest::Url::parse(s(&started["url"])).unwrap();
    let params = authorize.query_pairs().collect::<HashMap<_, _>>();
    assert_eq!(params["resource"], format!("{}/mcp", http.url));
    assert_eq!(params["code_challenge_method"], "S256");
    *expected_challenge.lock().unwrap() = params["code_challenge"].to_string();
    let mut callback = reqwest::Url::parse(&params["redirect_uri"]).unwrap();
    callback
        .query_pairs_mut()
        .append_pair("code", "fixture-code")
        .append_pair("state", &params["state"])
        .append_pair("iss", &format!("{}/issuer", http.url));
    reqwest::get(callback).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let status = core
                .ext_handle(
                    "ai-extensions:oauth-status",
                    &[json!("oauth"), json!("web")],
                )
                .await
                .unwrap();
            if status["status"] != "pending" {
                assert_eq!(status["status"], "success", "{status}");
                break;
            }
            tokio::task::yield_now().await
        }
    })
    .await
    .unwrap();
    assert_eq!(registrations.load(Ordering::SeqCst), 1);
    assert_eq!(
        core.ext_handle("ai-extensions:test", &[json!("oauth")])
            .await
            .unwrap()["ok"],
        true
    );
    core.ext_handle("ai-extensions:set-enabled", &[json!("oauth"), json!(true)])
        .await
        .unwrap();
    let snapshot = core
        .extension_snapshot(&core.inner.ctx.paths.home_dir)
        .await
        .unwrap();
    let result = core
        .extension_call(
            snapshot.values().next().unwrap(),
            json!({}),
            "oauth-run",
            &Cancel::new(),
            None,
        )
        .await
        .unwrap();
    assert!(!format!("{:?}", result.0).contains("fixture-oauth-new"));
    assert_eq!(exchanges.load(Ordering::SeqCst), 2);
    let settings = core
        .ext_handle("ai-extensions:list", &[])
        .await
        .unwrap()
        .to_string();
    assert!(!settings.contains("fixture-oauth-access"));
    assert!(!settings.contains("fixture-oauth-refresh"));
    assert!(!std::fs::read_to_string(
        core.inner
            .ctx
            .paths
            .config_dir
            .join("ai-extension-secrets.bin")
    )
    .unwrap()
    .contains("fixture-oauth-refresh"));
    core.shutdown().await;
}
#[test]
fn mirrored_headers_are_statically_reachable_unique_safe_and_encoded() {
    let schema = json!({"type":"object","properties":{"region":{"type":"string","x-mcp-header":"Region"},"options":{"type":"object","properties":{"retry":{"type":"integer","x-mcp-header":"Retry"}}}}});
    let headers = mcp::schema::mirrored_headers(
        &schema,
        Some(&json!({"region":"Алматы","options":{"retry":2}})),
    )
    .unwrap();
    assert!(headers.contains(&("Mcp-Param-Retry".into(), "2".into())));
    assert!(headers
        .iter()
        .any(|(k, v)| k == "Mcp-Param-Region" && v.starts_with("=?base64?")));
    for invalid in [
        json!({"type":"object","items":{"type":"string","x-mcp-header":"bad"}}),
        json!({"type":"object","anyOf":[{"type":"string","x-mcp-header":"bad"}]}),
        json!({"type":"object","properties":{"a":{"type":"number","x-mcp-header":"bad"}}}),
        json!({"type":"object","properties":{"a":{"type":"string","x-mcp-header":"X"},"b":{"type":"boolean","x-mcp-header":"x"}}}),
    ] {
        assert!(mcp::schema::mirrored_headers(&invalid, None).is_err())
    }
}
#[tokio::test]
async fn resources_templates_prompt_images_and_invalid_descriptors_use_the_same_connection() {
    use super::super::fixture_http::{server, Reply};
    let (_dir, core, mut events) = fixture();
    let png="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a0l8AAAAASUVORK5CYII=";
    let http=server(move|r|{let v=&r["body"];let result=match s(&v["method"]){"server/discover"=>json!({"capabilities":{"tools":{},"prompts":{},"resources":{}}}),"tools/list"=>json!({"tools":[{"name":"good","inputSchema":{"type":"object","properties":{}},"outputSchema":{"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"]},"annotations":{"readOnlyHint":true}},{"name":"remote","inputSchema":{"type":"object","$ref":"https://must-not-fetch.invalid/schema"}},{"name":"dup","inputSchema":{"type":"object"}},{"name":"dup","inputSchema":{"type":"object"}}]}),"prompts/list"=>json!({"prompts":[{"name":"hello","arguments":[{"name":"topic","required":true}]}]}),"resources/list"=>json!({"resources":[{"name":"note","uri":"fixture:///note"}]}),"resources/templates/list"=>json!({"resourceTemplates":[{"name":"notes","uriTemplate":"fixture:///notes/{id}"}]}),"resources/read"=>{assert!(s(&v["params"]["uri"]).starts_with("fixture:///"));json!({"contents":[{"uri":v["params"]["uri"],"text":"resource evidence"},{"uri":v["params"]["uri"],"mimeType":"image/png","blob":png}]})},"prompts/get"=>json!({"messages":[{"role":"user","content":{"type":"text","text":"User draft"}},{"role":"assistant","content":{"type":"text","text":"Assistant context"}},{"role":"user","content":{"type":"image","mimeType":"image/png","data":png}}]}),"tools/call"=>json!({"content":[{"type":"text","text":"only the person can see this","annotations":{"audience":["user"]}},{"type":"text","text":"public evidence"},{"type":"audio","data":"omitted"},{"type":"image","mimeType":"image/png","data":"aW52YWxpZA=="}],"structuredContent":{"ok":"invalid"},"isError":true}),_=>json!({})};Reply::json(json!({"jsonrpc":"2.0","id":v["id"],"result":result}))}).await;
    core.ext_handle("ai-extensions:add",&[json!({"manifestVersion":1,"id":"rich","name":"Rich fixture","connections":[{"id":"rich","name":"Rich","transport":"http","url":http.url,"toolPolicy":"allowReadOnly"}]})]).await.unwrap();
    assert_eq!(
        core.ext_handle("ai-extensions:test", &[json!("rich")])
            .await
            .unwrap()["ok"],
        true
    );
    core.ext_handle("ai-extensions:set-enabled", &[json!("rich"), json!(true)])
        .await
        .unwrap();
    let snapshot = core
        .extension_snapshot(&core.inner.ctx.paths.home_dir)
        .await
        .unwrap();
    assert!(snapshot
        .values()
        .all(|e| e.descriptor["name"] != "remote" && e.descriptor["name"] != "dup"));
    let good = snapshot
        .values()
        .find(|e| e.descriptor["name"] == "good")
        .unwrap();
    let (blocks, failed) = core
        .extension_call(good, json!({}), "rich-run", &Cancel::new(), None)
        .await
        .unwrap();
    assert!(failed);
    let text = format!("{blocks:?}");
    assert!(text.contains("public evidence"));
    assert!(text.contains("Structured output"));
    assert!(text.contains("Image omitted"));
    assert!(!text.contains("only the person"));
    let user_stage = super::super::tests::next_event(&mut events, "stage").await;
    assert!(user_stage.to_string().contains("only the person"));
    let read = snapshot
        .values()
        .find(|e| matches!(e.resource, Some(ResourceOp::Read)))
        .unwrap();
    assert!(core
        .extension_call(
            read,
            json!({"uri":"https://must-not-fetch.invalid"}),
            "rich-run",
            &Cancel::new(),
            None
        )
        .await
        .is_err());
    let resource = core
        .extension_call(
            read,
            json!({"uri":"fixture:///notes/42"}),
            "rich-run",
            &Cancel::new(),
            None,
        )
        .await
        .unwrap();
    assert!(resource
        .0
        .iter()
        .any(|b| matches!(b, runtime::transcript::Block::Image { .. })));
    let reference = json!({"extensionId":"rich","connectionId":"rich","name":"hello"});
    assert!(core
        .ext_handle(
            "ai-extensions:prompt-resolve",
            &[reference.clone(), json!({})]
        )
        .await
        .is_err());
    let draft = core
        .ext_handle(
            "ai-extensions:prompt-resolve",
            &[reference, json!({"topic":"fixture"})],
        )
        .await
        .unwrap();
    assert!(s(&draft["text"]).contains("user: User draft"));
    assert!(s(&draft["text"]).contains("assistant: Assistant context"));
    assert_eq!(draft["images"][0]["data"], png);
    core.shutdown().await;
}
#[tokio::test]
async fn descriptor_review_revokes_allowances_and_concurrent_cancels_are_isolated() {
    use super::super::fixture_http::{server, Reply};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (_dir, core, _) = fixture();
    let revision = Arc::new(AtomicUsize::new(0));
    let rev = revision.clone();
    let effects = Arc::new(AtomicUsize::new(0));
    let effect = effects.clone();
    let mut http=server(move|r|{let v=&r["body"];let result=match s(&v["method"]){"server/discover"=>json!({"capabilities":{"tools":{}}}),"tools/list"=>json!({"tools":[{"name":"echo","description":if rev.load(Ordering::SeqCst)==0{"original"}else{"changed"},"inputSchema":{"type":"object","properties":{"slow":{"type":"boolean"}},"additionalProperties":false},"annotations":{"readOnlyHint":true}}]}),"tools/call"=>{effect.fetch_add(1,Ordering::SeqCst);json!({"content":[{"type":"text","text":if v["params"]["arguments"]["slow"]==true{"slow"}else{"fast"}}]})},_=>json!({})};let mut reply=Reply::json(json!({"jsonrpc":"2.0","id":v["id"],"result":result}));if v["params"]["arguments"]["slow"]==true{reply.delay=Duration::from_millis(300)}reply}).await;
    core.ext_handle("ai-extensions:add",&[json!({"manifestVersion":1,"id":"change","name":"Change","connections":[{"id":"web","name":"Web","transport":"http","url":http.url,"toolPolicy":"allowReadOnly"}]})]).await.unwrap();
    assert_eq!(
        core.ext_handle("ai-extensions:test", &[json!("change")])
            .await
            .unwrap()["ok"],
        true
    );
    core.ext_handle("ai-extensions:set-enabled", &[json!("change"), json!(true)])
        .await
        .unwrap();
    let old = core
        .extension_snapshot(&core.inner.ctx.paths.home_dir)
        .await
        .unwrap();
    let old_entry = old.values().next().unwrap();
    core.remember_extension_tool(
        "change".into(),
        "web".into(),
        "echo".into(),
        old_entry.hash.clone(),
    )
    .await
    .unwrap();
    revision.store(1, Ordering::SeqCst);
    core.ext_discover("change").await.unwrap();
    assert_eq!(old_entry.descriptor["description"], "original");
    assert!(core
        .extension_call(old_entry, json!({}), "change-run", &Cancel::new(), None)
        .await
        .is_err());
    assert_eq!(effects.load(Ordering::SeqCst), 0);
    let pending = core.ext_read().await.unwrap();
    assert_eq!(
        pending["extensions"][0]["connections"][0]["needsReview"],
        true
    );
    assert_eq!(
        pending["extensions"][0]["connections"][0]["allowances"],
        json!({})
    );
    assert!(core
        .extension_snapshot(&core.inner.ctx.paths.home_dir)
        .await
        .unwrap()
        .is_empty());
    core.ext_handle(
        "ai-extensions:accept-review",
        &[json!("change"), review_token(&core, "change").await],
    )
    .await
    .unwrap();
    let current = core
        .extension_snapshot(&core.inner.ctx.paths.home_dir)
        .await
        .unwrap();
    let entry = current.values().next().unwrap();
    assert_eq!(entry.descriptor["description"], "changed");
    while http.requests.try_recv().is_ok() {}
    let cancel = Cancel::new();
    let owner = cancel.clone();
    let client = entry.client.clone();
    let definition = entry.descriptor.clone();
    let slow = tokio::spawn(async move {
        client
            .call_tool(
                "echo",
                json!({"slow":true}),
                &definition,
                &owner,
                Duration::from_secs(3),
            )
            .await
    });
    loop {
        let r = http.requests.recv().await.unwrap();
        if r["body"]["method"] == "tools/call" {
            break;
        }
    }
    let fast_cancel = Cancel::new();
    let fast = entry.client.call_tool(
        "echo",
        json!({"slow":false}),
        &entry.descriptor,
        &fast_cancel,
        Duration::from_secs(3),
    );
    let stop = async {
        cancel.cancel();
        slow.await.unwrap()
    };
    let (fast, slow) = tokio::join!(fast, stop);
    assert_eq!(fast.unwrap()["content"][0]["text"], "fast");
    assert!(slow.unwrap_err().contains("cancel"));
    assert_eq!(effects.load(Ordering::SeqCst), 2);
    let client = entry.client.clone();
    let descriptor = entry.descriptor.clone();
    let inflight = tokio::spawn(async move {
        client
            .call_tool(
                "echo",
                json!({"slow":true}),
                &descriptor,
                &Cancel::new(),
                Duration::from_secs(3),
            )
            .await
    });
    loop {
        let r = http.requests.recv().await.unwrap();
        if r["body"]["method"] == "tools/call" && r["body"]["params"]["arguments"]["slow"] == true {
            break;
        }
    }
    core.ext_handle(
        "ai-extensions:set-enabled",
        &[json!("change"), json!(false)],
    )
    .await
    .unwrap();
    assert!(inflight.await.unwrap().is_err());
    assert!(core
        .extension_call(entry, json!({}), "change-run", &Cancel::new(), None)
        .await
        .is_err());
    assert_eq!(effects.load(Ordering::SeqCst), 3);
    core.shutdown().await;
}
#[test]
fn connection_identity_is_unambiguous_even_when_ids_contain_separators() {
    assert_ne!(key("a:b", "c"), key("a", "b:c"));
    assert!(!key("a:b", "c").starts_with(&prefix("a")));
}
#[tokio::test]
async fn declarative_provider_is_native_scoped_and_its_header_is_never_returned() {
    use super::super::fixture_http::{server, Reply};
    let (_dir, core, mut events) = fixture();
    let http=server(|request|{assert!(s(&request["line"]).contains("/v1/messages"));assert_eq!(request["headers"]["x-team-key"],"fixture-custom-private");Reply::json(json!({"content":[{"type":"text","text":"native contributed provider"}],"stop_reason":"end_turn"}))}).await;
    let manifest = json!({"manifestVersion":1,"id":"provider","name":"Provider","connections":[],"providers":[{"id":"custom:declared","title":"Declared","baseURL":http.url,"protocol":"anthropic","models":[{"id":"declared-model","capabilities":["tools"]}],"headers":{"X-Team-Key":"fixture-custom-private"}}]});
    let preview = core
        .ext_handle("ai-extensions:parse-import", &[json!(manifest.to_string())])
        .await
        .unwrap();
    assert!(!preview.to_string().contains("fixture-custom-private"));
    core.ext_handle(
        "ai-extensions:add",
        &[json!({"previewId":preview["previewId"]})],
    )
    .await
    .unwrap();
    core.ext_handle(
        "ai-extensions:set-enabled",
        &[json!("provider"), json!(true)],
    )
    .await
    .unwrap();
    let config = core.handle("llm-config-get", &[]).await.unwrap().unwrap();
    assert!(config["customProviders"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["id"] == "custom:declared"));
    assert!(!config.to_string().contains("fixture-custom-private"));
    core.handle("llm-config-set", &[json!({"customProviders":config["customProviders"],"providerModels":config["providerModels"],"providerConfigs":{"custom:declared":{"apiKey":"fixture-should-not-persist"}}})]).await.unwrap().unwrap();
    let saved = core.read_config().await.unwrap();
    assert!(saved["customProviders"].as_array().unwrap().is_empty());
    assert!(saved["providerModels"].get("custom:declared").is_none());
    assert!(saved["providerConfigs"].get("custom:declared").is_none());
    core.patch_config(json!({"provider":"custom:declared","model":"declared-model"}))
        .await
        .unwrap();
    core.handle(
        "agent:run",
        &[json!({"runId":"agent-declared-provider","task":"fixture"})],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        super::super::tests::next_event(&mut events, "answer").await["text"],
        "native contributed provider"
    );
    super::super::tests::next_event(&mut events, "done").await;
    core.ext_handle(
        "ai-extensions:set-enabled",
        &[json!("provider"), json!(false)],
    )
    .await
    .unwrap();
    assert!(core
        .cfg(Some("chat"), None)
        .await
        .unwrap_err()
        .contains("disabled"));
    core.shutdown().await;
}
#[test]
fn schema_keywords_in_property_names_and_literal_values_are_preserved() {
    let schema = json!({"type":"object","$defs":{"label":{"type":"string","$anchor":"label"}},"properties":{"$ref":{"type":"string"},"$schema":{"$ref":"#label"},"x-mcp-header":{"type":"object","enum":[{"$ref":"https://must-not-fetch.invalid/data"}]}},"required":["$ref","$schema","x-mcp-header"]});
    let validator = mcp::schema::compile(&schema).unwrap();
    assert!(validator.is_valid(&json!({"$ref":"literal","$schema":"text","x-mcp-header":{"$ref":"https://must-not-fetch.invalid/data"}})));
    let wire = mcp::schema::for_wire(&schema).unwrap();
    assert_eq!(wire["properties"]["$ref"]["type"], "string");
    assert_eq!(wire["properties"]["$schema"]["type"], "string");
    assert_eq!(
        wire["properties"]["x-mcp-header"]["enum"][0]["$ref"],
        "https://must-not-fetch.invalid/data"
    );
    let schema = json!({"type":"object","properties":{"tuple":{"type":"array","prefixItems":[{"type":"string"}],"items":false}}});
    assert!(!mcp::schema::compile(&schema)
        .unwrap()
        .is_valid(&json!({"tuple":["a",2]})));
}

#[tokio::test]
async fn catalog_notification_keeps_run_snapshot_immutable_and_requires_changed_tool_review() {
    for legacy in [false, true] {
        let (dir, core, _) = fixture();
        let binary = program(dir.path());
        let args = if legacy {
            vec!["legacy", "notify"]
        } else {
            vec!["notify"]
        };
        core.handle("ai-extensions:add", &[json!({"manifestVersion":1,"id":"notify","name":"Notification fixture","connections":[{"id":"server","name":"Server","transport":"stdio","command":binary,"args":args,"toolPolicy":"allowReadOnly"}]})]).await.unwrap().unwrap();
        let tested = core
            .handle("ai-extensions:test", &[json!("notify")])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(tested["ok"], true, "{tested}");
        core.handle("ai-extensions:set-enabled", &[json!("notify"), json!(true)])
            .await
            .unwrap()
            .unwrap();
        let snapshot = core
            .extension_snapshot(&core.inner.ctx.paths.home_dir)
            .await
            .unwrap();
        let (name, entry) = snapshot
            .iter()
            .find(|(_, entry)| entry.descriptor["name"] == "echo")
            .unwrap();
        let name = name.clone();
        core.extension_call(
            entry,
            json!({"text":"trigger"}),
            "agent-notification",
            &Cancel::new(),
            Some("auto"),
        )
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while !entry
                .client
                .changed
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(snapshot[&name].descriptor["description"], "Echo fixture");
        assert!(core
            .extension_call(
                entry,
                json!({"text":"current snapshot"}),
                "agent-stale",
                &Cancel::new(),
                Some("auto")
            )
            .await
            .is_ok());
        let fresh = core
            .extension_snapshot(&core.inner.ctx.paths.home_dir)
            .await
            .unwrap();
        assert!(!fresh.contains_key(&name));
        let data = core.ext_read().await.unwrap();
        assert_eq!(data["extensions"][0]["connections"][0]["needsReview"], true);
        core.handle(
            "ai-extensions:accept-review",
            &[json!("notify"), review_token(&core, "notify").await],
        )
        .await
        .unwrap()
        .unwrap();
        let accepted = core
            .extension_snapshot(&core.inner.ctx.paths.home_dir)
            .await
            .unwrap();
        assert_eq!(accepted[&name].descriptor["description"], "Revised fixture");
        core.shutdown().await;
    }
}

#[tokio::test]
async fn provider_secret_references_can_be_filled_without_a_connection_and_do_not_alias_connection_ids(
) {
    let (_dir, core, _) = fixture();
    let manifest = json!({"manifestVersion":1,"id":"provider-secrets","name":"Provider fixture","connections":[],"providers":[{"id":"custom:fixture","title":"Fixture","baseURL":"https://example.invalid/v1","protocol":"openai-chat","headers":{"Authorization":{"secret":"MODEL_AUTH"}},"models":[{"id":"fixture"}]}]});
    core.handle("ai-extensions:add", &[manifest])
        .await
        .unwrap()
        .unwrap();
    assert_ne!(
        provider_key("provider-secrets", "custom:fixture"),
        key("provider-secrets", "provider:custom:fixture")
    );
    core.handle(
        "ai-extensions:set-secret",
        &[
            json!("provider-secrets"),
            json!("MODEL_AUTH"),
            json!("fixture-secret-header"),
            json!({"providerId":"custom:fixture"}),
        ],
    )
    .await
    .unwrap()
    .unwrap();
    assert!(core
        .handle(
            "ai-extensions:set-secret",
            &[
                json!("provider-secrets"),
                json!("UNDECLARED"),
                json!("fixture"),
                json!({"providerId":"custom:fixture"})
            ]
        )
        .await
        .unwrap()
        .is_err());
    core.handle(
        "ai-extensions:set-enabled",
        &[json!("provider-secrets"), json!(true)],
    )
    .await
    .unwrap()
    .unwrap();
    let config = core.extension_config(json!({}), None).await.unwrap();
    let provider = &config["customProviders"][0];
    assert_eq!(
        core.extension_provider_headers(provider).await.unwrap()["Authorization"],
        "fixture-secret-header"
    );
    assert!(!core
        .handle("ai-extensions:list", &[])
        .await
        .unwrap()
        .unwrap()
        .to_string()
        .contains("fixture-secret-header"));
    for field in ["connections", "prompts", "commands", "agents", "providers"] {
        let mut malformed = json!({"manifestVersion":1,"id":"bad","name":"Bad"});
        malformed[field] = json!("wrong type");
        assert!(manifest::parse(&malformed.to_string()).is_err());
    }
    core.shutdown().await;
}

#[tokio::test]
async fn stdio_crash_and_stdout_flood_report_unknown_without_replaying_effects() {
    for legacy in [false, true] {
        for failure in ["crash", "flood"] {
            let (dir, core, _) = fixture();
            let binary = program(dir.path());
            let marker = dir.path().join("effects");
            let args = if legacy {
                vec!["legacy", failure]
            } else {
                vec![failure]
            };
            core.handle("ai-extensions:add",&[json!({"manifestVersion":1,"id":"failure","name":"Failure fixture","connections":[{"id":"stdio","name":"Server","transport":"stdio","command":binary,"args":args,"env":{"FIXTURE_MARKER":marker},"toolPolicy":"allowReadOnly"}]})]).await.unwrap().unwrap();
            assert_eq!(
                core.handle("ai-extensions:test", &[json!("failure")])
                    .await
                    .unwrap()
                    .unwrap()["ok"],
                true
            );
            core.handle(
                "ai-extensions:set-enabled",
                &[json!("failure"), json!(true)],
            )
            .await
            .unwrap()
            .unwrap();
            let snapshot = core
                .extension_snapshot(&core.inner.ctx.paths.home_dir)
                .await
                .unwrap();
            let entry = snapshot
                .values()
                .find(|entry| entry.descriptor["name"] == "echo")
                .unwrap();
            let error = tokio::time::timeout(
                Duration::from_secs(5),
                core.extension_call(
                    entry,
                    json!({"text":"effect"}),
                    "agent-failure",
                    &Cancel::new(),
                    Some("auto"),
                ),
            )
            .await
            .unwrap()
            .unwrap_err();
            assert!(error.contains("outcome of this call is unknown"), "{error}");
            assert_eq!(std::fs::read_to_string(&marker).unwrap().lines().count(), 1);
            core.shutdown().await;
        }
    }
}

#[tokio::test]
async fn project_review_handoff_survives_an_independent_settings_request() {
    let (_dir, core, _events) = fixture();
    let folder = core.inner.ctx.paths.home_dir.join("handoff");
    std::fs::create_dir_all(folder.join(".tezbar")).unwrap();
    std::fs::write(
        folder.join(".tezbar/ai-extension.json"),
        json!({"manifestVersion":1,"id":"handoff","name":"Handoff","connections":[]}).to_string(),
    )
    .unwrap();
    // Launcher webview queues through native IPC; a settings webview has no cwd or browser storage.
    core.handle("ai-extensions:review-project", &[json!(folder)])
        .await
        .unwrap()
        .unwrap();
    let independent = core
        .handle("ai-extensions:list", &[])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        independent["project"]["folder"],
        json!(folder.canonicalize().unwrap())
    );
    assert_eq!(independent["project"]["preview"]["name"], "Handoff");
    let nonce = independent["projectReviewId"].clone();
    assert!(nonce.is_string());
    for _ in 0..3 {
        assert_eq!(
            core.handle("ai-extensions:list", &[])
                .await
                .unwrap()
                .unwrap()["projectReviewId"],
            nonce
        );
    }
    core.handle("ai-extensions:dismiss-project-review", &[nonce.clone()])
        .await
        .unwrap()
        .unwrap();
    assert!(core
        .handle("ai-extensions:list", &[])
        .await
        .unwrap()
        .unwrap()["project"]
        .is_null());
    core.handle("ai-extensions:review-project", &[json!(folder)])
        .await
        .unwrap()
        .unwrap();
    let again = core
        .handle("ai-extensions:list", &[])
        .await
        .unwrap()
        .unwrap();
    assert_ne!(again["projectReviewId"], nonce);
    // An old settings reply cannot dismiss a newer request for the same folder.
    core.handle("ai-extensions:dismiss-project-review", &[nonce])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        core.handle("ai-extensions:list", &[])
            .await
            .unwrap()
            .unwrap()["projectReviewId"],
        again["projectReviewId"]
    );
    core.handle(
        "ai-extensions:trust-project",
        &[
            json!(folder),
            again["project"]["entryHash"].clone(),
            again["projectReviewId"].clone(),
        ],
    )
    .await
    .unwrap()
    .unwrap();
    assert!(core
        .handle("ai-extensions:list", &[])
        .await
        .unwrap()
        .unwrap()["project"]
        .is_null());
    // Reusing an already open settings window receives the newly queued folder.
    let other = core.inner.ctx.paths.home_dir.join("other");
    std::fs::create_dir_all(other.join(".tezbar")).unwrap();
    std::fs::write(
        other.join(".tezbar/ai-extension.json"),
        json!({"manifestVersion":1,"id":"other","name":"Other","connections":[]}).to_string(),
    )
    .unwrap();
    core.handle("ai-extensions:review-project", &[json!(other)])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        core.handle("ai-extensions:list", &[])
            .await
            .unwrap()
            .unwrap()["project"]["folder"],
        json!(other.canonicalize().unwrap())
    );
    core.shutdown().await;
}

async fn review_token(core: &AiCore, id: &str) -> Value {
    let data = core.ext_read().await.unwrap();
    let record = data["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap();
    json!(record["connections"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["needsReview"] == true)
        .map(|c| (
            s(&c["id"]).to_string(),
            json!({"pins":c["pendingPins"],"trustPin":c["trustPin"]})
        ))
        .collect::<serde_json::Map<_, _>>())
}

#[tokio::test]
async fn pending_catalog_survives_refresh_edit_and_rejects_stale_review() {
    use super::super::fixture_http::{server, Reply};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (_dir, core, _events) = fixture();
    let revision = Arc::new(AtomicUsize::new(1));
    let rev = revision.clone();
    let http=server(move|r| {let result=match s(&r["body"]["method"]) {"server/discover"=>json!({"capabilities":{"tools":{}}}),"tools/list"=>json!({"tools":[{"name":"effect","description":format!("version {}",rev.load(Ordering::SeqCst)),"inputSchema":{"type":"object","properties":{}}}]}),_=>json!({})}; Reply::json(json!({"jsonrpc":"2.0","id":r["body"]["id"],"result":result}))}).await;
    core.ext_handle("ai-extensions:add",&[json!({"manifestVersion":1,"id":"pins","name":"Pins","connections":[{"id":"web","name":"Web","transport":"http","url":http.url}]})]).await.unwrap();
    assert_eq!(
        core.ext_handle("ai-extensions:test", &[json!("pins")])
            .await
            .unwrap()["ok"],
        true
    );
    revision.store(2, Ordering::SeqCst);
    core.ext_handle("ai-extensions:test", &[json!("pins")])
        .await
        .unwrap();
    let reviewed = review_token(&core, "pins").await;
    core.ext_handle("ai-extensions:test", &[json!("pins")])
        .await
        .unwrap();
    let data = core.ext_read().await.unwrap();
    let connection = data["extensions"][0]["connections"][0].clone();
    assert_eq!(
        connection["previousCatalog"]["tools"][0]["description"],
        "version 1"
    );
    core.ext_handle(
        "ai-extensions:update",
        &[
            json!("pins"),
            json!({"connections":[connection],"name":"Pins edited"}),
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        core.ext_read().await.unwrap()["extensions"][0]["connections"][0]["needsReview"],
        true
    );
    revision.store(3, Ordering::SeqCst);
    core.ext_handle("ai-extensions:test", &[json!("pins")])
        .await
        .unwrap();
    assert!(core
        .ext_handle("ai-extensions:accept-review", &[json!("pins"), reviewed])
        .await
        .unwrap_err()
        .contains("changed since"));
    core.ext_handle(
        "ai-extensions:accept-review",
        &[json!("pins"), review_token(&core, "pins").await],
    )
    .await
    .unwrap();
    assert_eq!(
        core.ext_read().await.unwrap()["extensions"][0]["connections"][0]["needsReview"],
        false
    );
    core.shutdown().await;
}

#[tokio::test]
async fn missing_secret_key_does_not_block_secretless_test_or_remove() {
    use super::super::fixture_http::{server, Reply};
    let (_dir, core, _events) = fixture();
    core.inner
        .extensions
        .secrets
        .put("other", "token", Some("fixture-unrelated-secret"))
        .unwrap();
    let ciphertext = std::fs::read(
        core.inner
            .ctx
            .paths
            .config_dir
            .join("ai-extension-secrets.bin"),
    )
    .unwrap();
    core.inner.extensions.secrets.forget_fixture_key();
    let http=server(|r|Reply::json(json!({"jsonrpc":"2.0","id":r["body"]["id"],"result":if r["body"]["method"]=="server/discover" {json!({"capabilities":{}})}else{json!({})}}))).await;
    core.ext_handle("ai-extensions:add",&[json!({"manifestVersion":1,"id":"secretless","name":"Secretless","connections":[{"id":"web","name":"Web","transport":"http","url":http.url}]})]).await.unwrap();
    assert_eq!(
        core.ext_handle("ai-extensions:test", &[json!("secretless")])
            .await
            .unwrap()["ok"],
        true
    );
    core.ext_handle("ai-extensions:remove", &[json!("secretless")])
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(
            core.inner
                .ctx
                .paths
                .config_dir
                .join("ai-extension-secrets.bin")
        )
        .unwrap(),
        ciphertext
    );
    core.shutdown().await;
}

#[tokio::test]
async fn changed_project_revocation_is_idempotent_and_has_one_event() {
    let (_dir, core, mut events) = fixture();
    let folder = core.inner.ctx.paths.home_dir.canonicalize().unwrap();
    std::fs::create_dir_all(folder.join(".tezbar")).unwrap();
    let file = folder.join(".tezbar/ai-extension.json");
    let text =
        json!({"manifestVersion":1,"id":"project-event","name":"Project event","connections":[]})
            .to_string();
    std::fs::write(&file, &text).unwrap();
    let project = core.project_status(&folder).await.unwrap();
    core.trust_project(folder.to_str().unwrap(), s(&project["entryHash"]))
        .await
        .unwrap();
    while events.try_recv().is_ok() {}
    std::fs::write(&file, format!("{text}\n")).unwrap();
    for _ in 0..5 {
        core.revoke_changed_projects(&folder).await.unwrap();
    }
    let mut changes = 0;
    while let Ok((channel, _)) = events.try_recv() {
        if channel == "ai-extensions-changed" {
            changes += 1;
        }
    }
    assert_eq!(changes, 1);
    core.shutdown().await;
}

#[tokio::test]
async fn silent_legacy_era_is_cached_by_trusted_connection_for_cold_snapshot() {
    let (dir, core, _events) = fixture();
    let binary = program(dir.path());
    core.ext_handle("ai-extensions:add",&[json!({"manifestVersion":1,"id":"silent","name":"Silent","connections":[{"id":"stdio","name":"Silent","transport":"stdio","command":binary,"args":["silent"]}]})]).await.unwrap();
    assert_eq!(
        core.ext_handle("ai-extensions:test", &[json!("silent")])
            .await
            .unwrap()["ok"],
        true
    );
    assert_eq!(
        core.ext_read().await.unwrap()["extensions"][0]["connections"][0]["cachedEra"],
        "2025-11-25"
    );
    core.ext_handle(
        "ai-extensions:set-enabled",
        &[json!("silent"), json!(false)],
    )
    .await
    .unwrap();
    core.ext_handle("ai-extensions:set-enabled", &[json!("silent"), json!(true)])
        .await
        .unwrap();
    let snapshot = tokio::time::timeout(
        Duration::from_secs(4),
        core.extension_snapshot(&core.inner.ctx.paths.home_dir),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!snapshot.is_empty());
    core.shutdown().await;
}
#[tokio::test]
async fn failed_start_preserves_reason_and_log_without_call_replay() {
    let (dir, core, _events) = fixture();
    let binary = program(dir.path());
    core.ext_handle("ai-extensions:add",&[json!({"manifestVersion":1,"id":"crashed-start","name":"Crash","connections":[{"id":"stdio","name":"Crash","transport":"stdio","command":binary,"args":["startup-error"],"env":{"FIXTURE_TOKEN":"fixture-startup-private-token"}}]})]).await.unwrap();
    let tested = core
        .ext_handle("ai-extensions:test", &[json!("crashed-start")])
        .await
        .unwrap();
    assert_eq!(tested["ok"], false);
    assert!(
        tested.to_string().contains("exited during startup"),
        "{tested}"
    );
    let logs = core
        .ext_handle("ai-extensions:logs", &[json!("crashed-start")])
        .await
        .unwrap();
    assert!(
        logs.to_string().contains("fixture module missing"),
        "{logs}"
    );
    assert!(logs.as_array().unwrap().len() <= 20);
    assert!(!logs.to_string().contains("fixture-startup-private-token"));
    assert!(s(&tested["error"]).len() < 160);
    core.shutdown().await;
}

#[tokio::test]
async fn insufficient_scope_is_recorded_once_and_successful_discovery_resets_status() {
    use super::super::fixture_http::{server, Reply};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (_dir, core, _events) = fixture();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let http=server(move|r|{let result=match s(&r["body"]["method"]) {"server/discover"=>json!({"capabilities":{"tools":{}}}),"tools/list"=>json!({"tools":[{"name":"stepup","inputSchema":{"type":"object","properties":{}},"annotations":{"readOnlyHint":true}}]}),"tools/call"=>{if count.fetch_add(1,Ordering::SeqCst)==0 {let mut reply=Reply::json(json!({})).status(403);reply.headers.push(("WWW-Authenticate".into(),"Bearer error=\"insufficient_scope\", scope=\"issues:write\"".into()));return reply;}json!({"content":[{"type":"text","text":"success"}]})},_=>json!({})};Reply::json(json!({"jsonrpc":"2.0","id":r["body"]["id"],"result":result}))}).await;
    core.ext_handle("ai-extensions:add",&[json!({"manifestVersion":1,"id":"stepup","name":"Stepup","connections":[{"id":"web","name":"Web","transport":"http","url":http.url,"toolPolicy":"allowReadOnly"}]})]).await.unwrap();
    core.ext_handle("ai-extensions:test", &[json!("stepup")])
        .await
        .unwrap();
    core.ext_handle("ai-extensions:set-enabled", &[json!("stepup"), json!(true)])
        .await
        .unwrap();
    let snapshot = core
        .extension_snapshot(&core.inner.ctx.paths.home_dir)
        .await
        .unwrap();
    let entry = snapshot.values().next().unwrap();
    assert!(definitions(&snapshot).unwrap()[0]["function"]["description"].is_string());
    assert!(core
        .extension_call(entry, json!({}), "stepup-run", &Cancel::new(), None)
        .await
        .unwrap_err()
        .contains("Access denied"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let data = core.ext_read().await.unwrap();
    assert_eq!(
        data["extensions"][0]["connections"][0]["requestedScopes"],
        "issues:write"
    );
    assert_eq!(data["extensions"][0]["connections"][0]["needsAuth"], true);
    let c = &data["extensions"][0]["connections"][0];
    assert_eq!(c["trustPin"], command_pin(c));
    assert!(c["auth"]["scopes"].is_null());
    assert_eq!(
        core.ext_handle("ai-extensions:test", &[json!("stepup")])
            .await
            .unwrap()["ok"],
        true
    );
    let data = core.ext_read().await.unwrap();
    let connection = &data["extensions"][0]["connections"][0];
    core.ext_edit(|data| {
        data["extensions"][0]["connections"][0]["needsReview"] = json!(true);
        Ok(Value::Null)
    })
    .await
    .unwrap();
    let pending = core
        .extension_snapshot(&core.inner.ctx.paths.home_dir)
        .await
        .unwrap();
    assert!(
        pending
            .warnings
            .iter()
            .any(|w| w.contains("Tools changed in Web. Review them in AI settings.")),
        "{:?}",
        pending.warnings
    );
    assert!(connection["needsAuth"].is_null());
    assert!(connection["error"].is_null());
    core.shutdown().await;
}

#[tokio::test]
async fn continuations_preserve_mirrors_and_sse_eof_reports_unknown_outcome() {
    use super::super::fixture_http::{server, Reply};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (_dir, core, _events) = fixture();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let http=server(move|r|{let result=match s(&r["body"]["method"]) {"server/discover"=>json!({"capabilities":{"tools":{}}}),"tools/list"=>json!({"ttlMs":2000,"tools":[{"name":"continue","inputSchema":{"type":"object","properties":{"region":{"type":"string","x-mcp-header":"Region"}}},"annotations":{"readOnlyHint":true}}]}),"tools/call"=>{assert_eq!(r["headers"]["mcp-param-region"],"fixture-region");match count.fetch_add(1,Ordering::SeqCst) {0=>json!({"resultType":"input_required","requestState":"fixture-state"}),1=>{assert_eq!(r["body"]["params"]["requestState"],"fixture-state");json!({"content":[{"type":"text","text":"continued"}]})},_=>{let mut reply=Reply::json(json!({}));reply.content_type="text/event-stream".into();reply.body="event: ping\ndata: {}\n\n".into();return reply;}}},_=>json!({})};Reply::json(json!({"jsonrpc":"2.0","id":r["body"]["id"],"result":result}))}).await;
    core.ext_handle("ai-extensions:add",&[json!({"manifestVersion":1,"id":"continuation","name":"Continuation","connections":[{"id":"web","name":"Web","transport":"http","url":http.url,"timeoutSeconds":17,"toolPolicy":"allowReadOnly"}]})]).await.unwrap();
    core.ext_handle("ai-extensions:test", &[json!("continuation")])
        .await
        .unwrap();
    core.ext_handle(
        "ai-extensions:set-enabled",
        &[json!("continuation"), json!(true)],
    )
    .await
    .unwrap();
    assert_eq!(
        core.ext_read().await.unwrap()["extensions"][0]["connections"][0]["ttlMs"],
        2000
    );
    let snapshot = core
        .extension_snapshot(&core.inner.ctx.paths.home_dir)
        .await
        .unwrap();
    let entry = snapshot.values().next().unwrap();
    assert!(format!(
        "{:?}",
        core.extension_call(
            entry,
            json!({"region":"fixture-region"}),
            "continuation-run",
            &Cancel::new(),
            None
        )
        .await
        .unwrap()
    )
    .contains("continued"));
    assert!(core
        .extension_call(
            entry,
            json!({"region":"fixture-region"}),
            "continuation-run",
            &Cancel::new(),
            None
        )
        .await
        .unwrap_err()
        .contains("outcome of this call is unknown"));
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    core.shutdown().await;
}

#[tokio::test]
async fn unrelated_edit_does_not_cancel_a_disabled_connection_test_and_success_resets_backoff() {
    use super::super::fixture_http::{server, Reply};
    let (_dir, core, _events) = fixture();
    let mut http=server(|r|{let mut reply=Reply::json(json!({"jsonrpc":"2.0","id":r["body"]["id"],"result":if r["body"]["method"]=="server/discover" {json!({"capabilities":{}})}else{json!({})}}));if r["body"]["method"]=="server/discover"{reply.delay=Duration::from_millis(100);}reply}).await;
    core.ext_handle("ai-extensions:add",&[json!({"manifestVersion":1,"id":"independent","name":"Independent","connections":[{"id":"web","name":"Web","transport":"http","url":http.url}]})]).await.unwrap();
    let owner = core.clone();
    let testing = tokio::spawn(async move {
        owner
            .ext_handle("ai-extensions:test", &[json!("independent")])
            .await
    });
    http.requests.recv().await.unwrap();
    core.ext_handle(
        "ai-extensions:set-instructions",
        &[json!("unrelated edit"), json!(false)],
    )
    .await
    .unwrap();
    assert_eq!(testing.await.unwrap().unwrap()["ok"], true);
    core.ext_handle(
        "ai-extensions:set-enabled",
        &[json!("independent"), json!(false)],
    )
    .await
    .unwrap();
    core.inner.extensions.failures.lock().unwrap().insert(
        key("independent", "web"),
        (3, 0, "fixture old failure".into()),
    );
    let data = core.ext_read().await.unwrap();
    let c = &data["extensions"][0]["connections"][0];
    core.ext_client_mode("independent", c, true).await.unwrap();
    assert!(!core
        .inner
        .extensions
        .failures
        .lock()
        .unwrap()
        .contains_key(&key("independent", "web")));
    core.shutdown().await;
}

#[tokio::test]
async fn invalid_add_has_no_orphan_secrets_and_timeouts_are_positive_bounded() {
    let (_dir, core, _events) = fixture();
    let invalid = json!({"manifestVersion":1,"id":"invalid","name":"Invalid","connections":[{"id":"one","name":"One","transport":"stdio","command":"must-not-run","env":{"API_KEY":"fixture-must-not-persist"}},{"id":"two","name":"Two","transport":"stdio","command":"must-not-run","toolPolicy":"allow"}]});
    assert!(core
        .ext_handle("ai-extensions:add", &[invalid])
        .await
        .is_err());
    assert!(!core
        .inner
        .ctx
        .paths
        .config_dir
        .join("ai-extension-secrets.bin")
        .exists());
    for timeout in [json!(0), json!(-1), json!(1801), json!(""), json!(1.5)] {
        assert!(core.ext_handle("ai-extensions:add",&[json!({"manifestVersion":1,"id":"bad-timeout","name":"Timeout","connections":[{"id":"web","name":"Web","transport":"http","url":"https://fixture.invalid/mcp","timeoutSeconds":timeout}]})]).await.unwrap_err().contains("between 1 and 1800"));
    }
    core.shutdown().await;
}

#[tokio::test]
async fn combined_instructions_are_bounded_and_name_the_source_that_was_truncated() {
    let (_dir, core, _events) = fixture();
    core.ext_handle(
        "ai-extensions:set-instructions",
        &[json!("global ".repeat(16000)), json!(false)],
    )
    .await
    .unwrap();
    core.ext_handle("ai-extensions:add",&[json!({"manifestVersion":1,"id":"large","name":"Large source","connections":[],"instructions":"extension ".repeat(12000)})]).await.unwrap();
    core.ext_handle("ai-extensions:set-enabled", &[json!("large"), json!(true)])
        .await
        .unwrap();
    let (text, _) = core
        .extension_instructions(&core.inner.ctx.paths.home_dir, &json!({}))
        .await
        .unwrap();
    assert!(text.len() <= 128 * 1024);
    assert!(text.contains("Instructions from Extension Large source truncated"));
    core.shutdown().await;
}

#[tokio::test]
async fn contributed_provider_cannot_borrow_legacy_keys_or_erase_a_user_owned_collision() {
    let (_dir, core, _events) = fixture();
    core.patch_config(json!({"provider":"custom:declared","apiKey":"fixture-unrelated-legacy"}))
        .await
        .unwrap();
    core.ext_handle("ai-extensions:add",&[json!({"manifestVersion":1,"id":"ownership","name":"Ownership","connections":[],"providers":[{"id":"custom:declared","title":"Declared","baseURL":"https://declared.invalid/v1","protocol":"openai-chat","models":[{"id":"fixture"}]}]})]).await.unwrap();
    core.ext_handle(
        "ai-extensions:set-enabled",
        &[json!("ownership"), json!(true)],
    )
    .await
    .unwrap();
    let cfg = core.cfg(None, None).await.unwrap();
    assert!(s(&cfg["apiKey"]).is_empty());
    core.patch_config(json!({"customProviders":[{"id":"custom:declared","title":"User owned","baseURL":"https://user.invalid/v1"}],"providerConfigs":{"custom:declared":{"apiKey":"fixture-user-owned"}}})).await.unwrap();
    core.handle("llm-config-set",&[json!({"customProviders":[{"id":"custom:declared","title":"User owned","baseURL":"https://user.invalid/v1"}],"providerConfigs":{"custom:declared":{"apiKey":"fixture-user-owned"}}})]).await.unwrap().unwrap();
    let cfg = core.cfg(None, None).await.unwrap();
    assert_eq!(cfg["customProviders"][0]["title"], "User owned");
    assert_eq!(cfg["apiKey"], "fixture-user-owned");
    core.shutdown().await;
}

#[tokio::test]
async fn reset_missing_connection_key_archives_ciphertext_and_allows_new_secrets_only_in_fixture() {
    let (_dir, core, _) = fixture();
    core.ext_handle("ai-extensions:add", &[json!({"manifestVersion":1,"id":"reset-fixture","name":"Reset fixture","providers":[{"id":"custom:reset","title":"Reset provider","baseURL":"https://fixture.invalid","protocol":"openai-chat","models":[{"id":"m","capabilities":["tools"]}],"headers":{"Authorization":{"secret":"provider-token","label":"Provider key"}}}],"connections":[{"id":"secret","name":"Secret","transport":"http","url":"https://fixture.invalid/mcp","auth":{"type":"token"}},{"id":"plain","name":"Plain","transport":"http","url":"https://fixture.invalid/plain"}]})]).await.unwrap();
    core.inner
        .extensions
        .secrets
        .put(
            "reset-fixture:secret",
            "access-token",
            Some("fixture-reset-private"),
        )
        .unwrap();
    core.inner.extensions.secrets.forget_fixture_key();
    let path = core
        .inner
        .ctx
        .paths
        .config_dir
        .join("ai-extension-secrets.bin");
    let old = std::fs::read(&path).unwrap();
    assert_eq!(
        core.ext_handle("ai-extensions:list", &[]).await.unwrap()["canResetSecrets"],
        true
    );
    let before = core.ext_read().await.unwrap()["extensions"][0]["connections"][1].clone();
    core.ext_handle("ai-extensions:reset-secrets", &[])
        .await
        .unwrap();
    let data = core.ext_read().await.unwrap();
    assert_eq!(data["extensions"][0]["connections"][0]["tested"], false);
    assert_eq!(data["extensions"][0]["connections"][0]["needsAuth"], false);
    assert!(s(&data["extensions"][0]["connections"][0]["error"]).contains("Enter the secret again"));
    let listed = core.ext_handle("ai-extensions:list", &[]).await.unwrap();
    assert_eq!(
        listed["extensions"][0]["providers"][0]["missingCredentials"],
        true
    );
    assert_eq!(data["extensions"][0]["connections"][1], before);
    assert!(std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .any(|f| f.file_name().to_string_lossy().contains(".unreadable-")
            && std::fs::read(f.path()).unwrap() == old));
    core.ext_handle("ai-extensions:add", &[json!({"manifestVersion":1,"id":"after-reset","name":"After reset","connections":[{"id":"web","name":"Web","transport":"http","url":"https://fixture.invalid/mcp","auth":{"type":"token"}}],"token":"fixture-new-token"})]).await.unwrap();
    core.ext_handle(
        "ai-extensions:set-secret",
        &[
            json!("after-reset"),
            json!("access-token"),
            json!("fixture-new-token"),
            json!("web"),
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        core.inner
            .extensions
            .secrets
            .values(&key("after-reset", "web"))
            .unwrap()["access-token"],
        "fixture-new-token"
    );
    assert!(!std::fs::read_to_string(path)
        .unwrap()
        .contains("fixture-new-token"));
    core.shutdown().await;
}

#[tokio::test]
async fn cached_era_startup_crash_retains_stderr_and_explicit_test_reprobes() {
    let (dir, core, _) = fixture();
    let binary = program(dir.path());
    core.ext_handle("ai-extensions:add",&[json!({"manifestVersion":1,"id":"cached-crash","name":"Crash","connections":[{"id":"stdio","name":"Crash","transport":"stdio","command":binary,"args":["startup-error"]}]})]).await.unwrap();
    core.ext_edit(|data| {
        data["extensions"][0]["enabled"] = json!(true);
        data["extensions"][0]["connections"][0]["cachedEra"] = json!("2025-11-25");
        Ok(Value::Null)
    })
    .await
    .unwrap();
    let data = core.ext_read().await.unwrap();
    let c = &data["extensions"][0]["connections"][0];
    let error = core.ext_client("cached-crash", c).await.err().unwrap();
    assert!(error.contains("exited during startup"), "{error}");
    assert!(error.len() < 160, "{error}");
    let logs = core
        .ext_handle("ai-extensions:logs", &[json!("cached-crash")])
        .await
        .unwrap();
    assert!(
        logs.to_string().contains("fixture module missing"),
        "{logs}"
    );
    // Switch fixture mode without updating the cached era: Test must detect modern.
    core.ext_edit(|data| {
        let c = &mut data["extensions"][0]["connections"][0];
        c["args"] = json!([]);
        Ok(Value::Null)
    })
    .await
    .unwrap();
    assert_eq!(
        core.ext_handle("ai-extensions:test", &[json!("cached-crash")])
            .await
            .unwrap()["ok"],
        true
    );
    assert_eq!(
        core.ext_read().await.unwrap()["extensions"][0]["connections"][0]["version"],
        "2026-07-28"
    );
    core.shutdown().await;
}

#[tokio::test]
async fn closure_first_test_after_post_start_crash_reconnects() {
    let (dir, core, _) = fixture();
    let binary = program(dir.path());
    core.ext_handle("ai-extensions:add", &[json!({"manifestVersion":1,"id":"post-crash","name":"Crash","connections":[{"id":"stdio","name":"Crash","transport":"stdio","command":binary,"args":["crash"]}]})]).await.unwrap();
    assert_eq!(
        core.ext_handle("ai-extensions:test", &[json!("post-crash")])
            .await
            .unwrap()["ok"],
        true
    );
    let old = core.inner.extensions.clients.lock().unwrap()[&key("post-crash", "stdio")].clone();
    assert!(old
        .call(
            "tools/call",
            json!({"name":"echo","arguments":{"text":"once"}}),
            &Cancel::new(),
            Duration::from_secs(2)
        )
        .await
        .is_err());
    assert!(old.crashed());
    assert_eq!(
        core.ext_handle("ai-extensions:test", &[json!("post-crash")])
            .await
            .unwrap()["ok"],
        true
    );
    core.shutdown().await;
}
#[tokio::test]
async fn closure_test_retires_client_after_long_call_and_disable_stops_retired() {
    let (dir, core, _) = fixture();
    let binary = program(dir.path());
    let marker = dir.path().join("calls");
    let release = dir.path().join("release");
    core.ext_handle("ai-extensions:add", &[json!({"manifestVersion":1,"id":"retire","name":"Retire","connections":[{"id":"stdio","name":"Slow","transport":"stdio","command":binary,"args":["slow"],"env":{"FIXTURE_MARKER":marker,"FIXTURE_RELEASE":release}}]})]).await.unwrap();
    assert_eq!(
        core.ext_handle("ai-extensions:test", &[json!("retire")])
            .await
            .unwrap()["ok"],
        true
    );
    core.ext_handle("ai-extensions:set-enabled", &[json!("retire"), json!(true)])
        .await
        .unwrap();
    for disable in [false, true] {
        let _ = std::fs::remove_file(&release);
        let previous_calls = std::fs::read_to_string(&marker)
            .unwrap_or_default()
            .lines()
            .count();
        let old = core.inner.extensions.clients.lock().unwrap()[&key("retire", "stdio")].clone();
        let client = old.clone();
        let call = tokio::spawn(async move {
            client
                .call(
                    "tools/call",
                    json!({"name":"echo","arguments":{"text":"once"}}),
                    &Cancel::new(),
                    Duration::from_secs(20),
                )
                .await
        });
        tokio::time::timeout(Duration::from_secs(10), async {
            while std::fs::read_to_string(&marker)
                .unwrap_or_default()
                .lines()
                .count()
                == previous_calls
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            core.ext_handle("ai-extensions:test", &[json!("retire")])
                .await
                .unwrap()["ok"],
            true
        );
        assert!(!old.is_closed());
        if disable {
            core.ext_handle(
                "ai-extensions:set-enabled",
                &[json!("retire"), json!(false)],
            )
            .await
            .unwrap();
            assert!(old.is_closed());
            assert!(call.await.unwrap().is_err());
        } else {
            std::fs::write(&release, "release").unwrap();
            assert!(call.await.unwrap().unwrap().to_string().contains("fixture"));
            assert!(old.is_closed());
        }
    }
    assert_eq!(std::fs::read_to_string(marker).unwrap().lines().count(), 2);
    core.shutdown().await;
}
#[tokio::test]
async fn closure_test_does_not_join_normal_cached_era_start() {
    use super::super::fixture_http::{server, Reply};
    let (_dir, core, _) = fixture();
    let init = Arc::new(tokio::sync::Notify::new());
    let seen = init.clone();
    let http = server(move |r| {
        let method = s(&r["body"]["method"]);
        let value = if method == "initialize" {
            seen.notify_one();
            json!({"protocolVersion":"2025-11-25","capabilities":{}})
        } else {
            json!({"capabilities":{}})
        };
        let mut reply = Reply::json(json!({"jsonrpc":"2.0","id":r["body"]["id"],"result":value}));
        if method == "initialize" {
            reply.delay = Duration::from_millis(200);
        }
        reply
    })
    .await;
    core.ext_handle("ai-extensions:add", &[json!({"manifestVersion":1,"id":"race","name":"Race","connections":[{"id":"web","name":"Web","transport":"http","url":http.url}]})]).await.unwrap();
    core.ext_edit(|data| {
        data["extensions"][0]["enabled"] = json!(true);
        data["extensions"][0]["connections"][0]["cachedEra"] = json!("2025-11-25");
        Ok(Value::Null)
    })
    .await
    .unwrap();
    let c = core.ext_read().await.unwrap()["extensions"][0]["connections"][0].clone();
    let worker = core.clone();
    let normal = tokio::spawn(async move { worker.ext_client("race", &c).await });
    tokio::time::timeout(Duration::from_secs(3), init.notified())
        .await
        .unwrap();
    assert_eq!(
        core.ext_handle("ai-extensions:test", &[json!("race")])
            .await
            .unwrap()["ok"],
        true
    );
    assert!(normal.await.unwrap().is_ok());
    assert_eq!(
        core.ext_read().await.unwrap()["extensions"][0]["connections"][0]["version"],
        "2026-07-28"
    );
    core.shutdown().await;
}
#[tokio::test]
async fn closure_changed_server_clears_requested_scopes_only() {
    let (_dir, core, _) = fixture();
    core.ext_handle("ai-extensions:add", &[json!({"manifestVersion":1,"id":"scope-edit","name":"Scope","connections":[{"id":"web","name":"Web","transport":"http","url":"https://fixture.invalid/a","auth":{"type":"oauth","scopes":"intentional"}}]})]).await.unwrap();
    core.ext_edit(|data| {
        data["extensions"][0]["connections"][0]["requestedScopes"] = json!("server-a");
        Ok(Value::Null)
    })
    .await
    .unwrap();
    let mut record = core.ext_read().await.unwrap()["extensions"][0].clone();
    record["name"] = json!("renamed");
    core.ext_handle(
        "ai-extensions:update",
        &[json!("scope-edit"), record.clone()],
    )
    .await
    .unwrap();
    assert_eq!(
        core.ext_read().await.unwrap()["extensions"][0]["connections"][0]["requestedScopes"],
        "server-a"
    );
    record["connections"][0]["url"] = json!("https://fixture.invalid/b");
    core.ext_handle("ai-extensions:update", &[json!("scope-edit"), record])
        .await
        .unwrap();
    let data = core.ext_read().await.unwrap();
    let c = &data["extensions"][0]["connections"][0];
    assert!(c["requestedScopes"].is_null());
    assert_eq!(c["auth"]["scopes"], "intentional");
    core.shutdown().await;
}

#[cfg(unix)]
#[tokio::test]
async fn closure_escaped_stderr_pipe_does_not_hold_shutdown_tasks() {
    struct OwnedPid(i32);
    impl Drop for OwnedPid {
        fn drop(&mut self) {
            unsafe {
                libc::kill(self.0, libc::SIGKILL);
            }
        }
    }
    let (dir, core, _) = fixture();
    let binary = program(dir.path());
    let marker = dir.path().join("escaped.pid");
    core.ext_handle("ai-extensions:add", &[json!({"manifestVersion":1,"id":"escape","name":"Escape","connections":[{"id":"stdio","name":"Escape","transport":"stdio","command":binary,"args":["escape-stderr"],"env":{"ESCAPED_PID":marker}}]})]).await.unwrap();
    assert_eq!(
        core.ext_handle("ai-extensions:test", &[json!("escape")])
            .await
            .unwrap()["ok"],
        false
    );
    let owned = OwnedPid(std::fs::read_to_string(marker).unwrap().parse().unwrap());
    assert_eq!(
        unsafe { libc::kill(owned.0, 0) },
        0,
        "fixture must still hold stderr open"
    );
    let start = std::time::Instant::now();
    core.shutdown().await;
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "shutdown {:?}",
        start.elapsed()
    );
    assert!(core.inner.tasks.lock().unwrap().is_empty());
}

#[tokio::test]
async fn removing_an_extension_stops_retired_clients_and_their_pending_call() {
    let (dir, core, _) = fixture();
    let binary = program(dir.path());
    let marker = dir.path().join("calls");
    let release = dir.path().join("release");
    core.ext_handle("ai-extensions:add", &[json!({"manifestVersion":1,"id":"retire","name":"Retire","connections":[{"id":"stdio","name":"Slow","transport":"stdio","command":binary,"args":["slow"],"env":{"FIXTURE_MARKER":marker,"FIXTURE_RELEASE":release}}]})]).await.unwrap();
    assert_eq!(
        core.ext_handle("ai-extensions:test", &[json!("retire")])
            .await
            .unwrap()["ok"],
        true
    );
    core.ext_handle("ai-extensions:set-enabled", &[json!("retire"), json!(true)])
        .await
        .unwrap();
    let old = core.inner.extensions.clients.lock().unwrap()[&key("retire", "stdio")].clone();
    let client = old.clone();
    let call = tokio::spawn(async move {
        client
            .call(
                "tools/call",
                json!({"name":"echo","arguments":{"text":"once"}}),
                &Cancel::new(),
                Duration::from_secs(20),
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        while std::fs::read_to_string(&marker)
            .unwrap_or_default()
            .lines()
            .count()
            == 0
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        core.ext_handle("ai-extensions:test", &[json!("retire")])
            .await
            .unwrap()["ok"],
        true
    );
    assert!(!old.is_closed());
    core.ext_handle("ai-extensions:remove", &[json!("retire")])
        .await
        .unwrap();
    assert!(core.ext_read().await.unwrap()["extensions"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(old.is_closed());
    assert!(core.inner.extensions.clients.lock().unwrap().is_empty());
    assert!(tokio::time::timeout(Duration::from_secs(1), call)
        .await
        .unwrap()
        .unwrap()
        .is_err());
    assert_eq!(std::fs::read_to_string(&marker).unwrap().lines().count(), 1);
    core.shutdown().await;
}

#[tokio::test]
async fn stopping_during_a_test_catalog_cancels_the_probe_and_prevents_adoption() {
    use super::super::fixture_http::{server, Reply};
    for shutdown in [false, true] {
        let (_dir, core, _) = fixture();
        let mut http = server(move |r| {
            let method = s(&r["body"]["method"]);
            let result = match method {
                "initialize" => json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{}}}),
                "tools/list" => json!({"tools":[]}),
                _ => json!({"capabilities":{"tools":{}}}),
            };
            let mut reply =
                Reply::json(json!({"jsonrpc":"2.0","id":r["body"]["id"],"result":result}));
            if method == "tools/list" {
                reply.delay = Duration::from_secs(30);
            }
            reply
        })
        .await;
        core.ext_handle("ai-extensions:add", &[json!({"manifestVersion":1,"id":"quit","name":"Quit","connections":[{"id":"web","name":"Web","transport":"http","url":http.url}]})]).await.unwrap();
        let worker = core.clone();
        let test = tokio::spawn(async move {
            worker
                .ext_handle("ai-extensions:test", &[json!("quit")])
                .await
        });
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let request = http.requests.recv().await.unwrap();
                if request["body"]["method"] == "tools/list" {
                    break;
                }
            }
        })
        .await
        .unwrap();
        assert!(core
            .inner
            .extensions
            .clients
            .lock()
            .unwrap()
            .keys()
            .any(|k| k.contains(":probe:")));
        let start = std::time::Instant::now();
        if shutdown {
            core.shutdown().await;
        } else {
            core.ext_handle("ai-extensions:remove", &[json!("quit")])
                .await
                .unwrap();
        }
        assert!(start.elapsed() < Duration::from_secs(1));
        let outcome = tokio::time::timeout(Duration::from_secs(1), test)
            .await
            .unwrap()
            .unwrap();
        assert!(!outcome.is_ok_and(|v| v["ok"] == true));
        assert!(core.inner.extensions.clients.lock().unwrap().is_empty());
        assert!(core.inner.extensions.starting.lock().unwrap().is_empty());
        let data = core.ext_read().await.unwrap();
        if shutdown {
            assert_ne!(data["extensions"][0]["connections"][0]["tested"], true);
        } else {
            assert!(data["extensions"].as_array().unwrap().is_empty());
            core.shutdown().await;
        }
    }
}
