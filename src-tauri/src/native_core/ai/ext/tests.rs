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
    core.ext_handle("ai-extensions:accept-review", &[json!("change")])
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
                json!({"text":"stale"}),
                "agent-stale",
                &Cancel::new(),
                Some("auto")
            )
            .await
            .unwrap_err()
            .contains("catalog changed"));
        let fresh = core
            .extension_snapshot(&core.inner.ctx.paths.home_dir)
            .await
            .unwrap();
        assert!(!fresh.contains_key(&name));
        let data = core.ext_read().await.unwrap();
        assert_eq!(data["extensions"][0]["connections"][0]["needsReview"], true);
        core.handle("ai-extensions:accept-review", &[json!("notify")])
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
