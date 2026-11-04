use super::super::fixture_http::{server, Reply};
use super::super::tests::{fixture, next_event};
use super::super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
#[tokio::test]
async fn copilot_refresh_is_serialized_and_catalog_routes_unusual_names() {
    let (_dir, core, mut events) = fixture();
    let refresh = Arc::new(AtomicUsize::new(0));
    let calls = refresh.clone();
    let mut http=server(move|request|{let line=s(&request["line"]);if line.contains("/oauth"){calls.fetch_add(1,Ordering::SeqCst);assert!(s(&request["raw"]).contains("grant_type=refresh_token"));Reply::json(json!({"access_token":"fixture-github-new","refresh_token":"fixture-refresh-new","expires_in":3600}))}else if line.contains("/models"){Reply::json(json!({"data":[{"id":"unusual-name","capabilities":{"type":"chat"},"supported_endpoints":["/responses"]},{"id":"non-chat","capabilities":{"type":"embeddings"}}]}))}else{assert!(line.contains("/responses"));Reply::json(json!({"output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"routed"}]}]}))}}).await;
    core.inner.endpoints.lock().unwrap().extend([
        ("github-oauth".into(), format!("{}/oauth", http.url)),
        ("copilot-models".into(), format!("{}/models", http.url)),
        ("copilot".into(), http.url.clone()),
    ]);
    core.patch_config(json!({"provider":"copilot","model":"unusual-name","providerConfigs":{"copilot":{"copilotGithubToken":"fixture-old","copilotRefreshToken":"fixture-refresh","copilotExpiresAt":1}}})).await.unwrap();
    let (a, b) = tokio::join!(core.copilot_token(), core.copilot_token());
    assert_eq!(a.unwrap(), "fixture-github-new");
    assert_eq!(b.unwrap(), "fixture-github-new");
    assert_eq!(refresh.load(Ordering::SeqCst), 1);
    core.handle(
        "agent:run",
        &[json!({"runId":"agent-copilot-catalog","task":"fixture"})],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(next_event(&mut events, "answer").await["text"], "routed");
    next_event(&mut events, "done").await;
    let models = core
        .handle("llm-list-models", &[json!("copilot")])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(models, json!(["unusual-name"]));
    while let Ok(req) = http.requests.try_recv() {
        assert!(!req["body"].to_string().contains("fixture-github-new"))
    }
    core.shutdown().await;
}
#[tokio::test]
async fn copilot_rejected_direct_token_exchanges_once_and_never_persists_the_session() {
    let (_dir, core, mut events) = fixture();
    let exchange = Arc::new(AtomicUsize::new(0));
    let counter = exchange.clone();
    let http=server(move|request|{if s(&request["line"]).contains("/exchange"){counter.fetch_add(1,Ordering::SeqCst);Reply::json(json!({"token":"fixture-session;proxy-ep=proxy.individual.githubcopilot.com;","expires_at":now_ms()/1000+3600}))}else if request["headers"]["authorization"]=="Bearer fixture-github"{Reply::json(json!({"error":{"message":"Unauthorized"}})).status(401)}else{assert!(s(&request["headers"]["authorization"]).contains("fixture-session"));Reply::json(json!({"choices":[{"message":{"content":"session accepted"},"finish_reason":"stop"}]}))}}).await;
    core.inner.endpoints.lock().unwrap().extend([
        ("copilot-session".into(), format!("{}/exchange", http.url)),
        ("copilot".into(), http.url.clone()),
        ("copilot-proxy".into(), http.url.clone()),
    ]);
    core.patch_config(json!({"provider":"copilot","model":"fixture","providerConfigs":{"copilot":{"copilotGithubToken":"fixture-github","protocol":"openai-chat"}}})).await.unwrap();
    for run in ["agent-session-one", "agent-session-two"] {
        core.handle("agent:run", &[json!({"runId":run,"task":"fixture"})])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            next_event(&mut events, "answer").await["text"],
            "session accepted"
        );
        next_event(&mut events, "done").await;
    }
    assert_eq!(exchange.load(Ordering::SeqCst), 1);
    assert!(!core
        .read_config()
        .await
        .unwrap()
        .to_string()
        .contains("fixture-session"));
    core.shutdown().await;
}
#[tokio::test]
async fn antigravity_refresh_preserves_unknown_account_fields_and_corrupt_auth_is_never_overwritten(
) {
    let (_dir, core, _events) = fixture();
    let dir = core.pi_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let auth = json!({"other":{"secret":"other-fixture","unknown":[1,2]},"antigravity":{"access":"old","refresh":"fixture-refresh","expires":1,"email":"fixture@example.invalid"}});
    let store = json!({"version":1,"unknown":true,"activeAccountId":"fixture@example.invalid","accounts":{"fixture@example.invalid":{"email":"fixture@example.invalid","refresh":"fixture-refresh","access":"old","expires":1,"projectId":"fixture-project","unknownAccount":42}}});
    common::write_json_atomic(&dir.join("auth.json"), &auth).unwrap();
    common::write_json_atomic(&dir.join("antigravity-accounts.json"), &store).unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let counter = count.clone();
    let http = server(move |r| {
        counter.fetch_add(1, Ordering::SeqCst);
        assert!(s(&r["raw"]).contains("refresh_token=fixture-refresh"));
        Reply::json(json!({"access_token":"fixture-google-new","expires_in":3600}))
    })
    .await;
    core.inner
        .endpoints
        .lock()
        .unwrap()
        .insert("google-token".into(), http.url.clone());
    let (a, b) = tokio::join!(core.antigravity_account(), core.antigravity_account());
    assert_eq!(a.unwrap()["access"], "fixture-google-new");
    assert_eq!(b.unwrap()["access"], "fixture-google-new");
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let next = common::read_json(&dir.join("auth.json")).unwrap();
    assert_eq!(next["other"], auth["other"]);
    let next = common::read_json(&dir.join("antigravity-accounts.json")).unwrap();
    assert_eq!(
        next["accounts"]["fixture@example.invalid"]["unknownAccount"],
        42
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(dir.join("auth.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        )
    }
    std::fs::write(dir.join("auth.json"), "{broken fixture}").unwrap();
    assert!(core.antigravity_account().await.is_err());
    assert_eq!(
        std::fs::read_to_string(dir.join("auth.json")).unwrap(),
        "{broken fixture}"
    );
    core.shutdown().await;
}
#[tokio::test]
async fn antigravity_quota_rotates_accounts_and_transport_errors_walk_endpoints() {
    let (_dir, core, mut events) = fixture();
    let dir = core.pi_dir();
    std::fs::create_dir_all(&dir).unwrap();
    common::write_json_atomic(&dir.join("antigravity-accounts.json"),&json!({"version":1,"activeAccountId":"a","accounts":{"a":{"accountId":"a","access":"fixture-a","refresh":"refresh-a","expires":now_ms()+3600000,"projectId":"project-a"},"b":{"accountId":"b","access":"fixture-b","refresh":"refresh-b","expires":now_ms()+3600000,"projectId":"project-b"}}})).unwrap();
    let http=server(move|request|{if request["headers"]["authorization"]=="Bearer fixture-a"{Reply::json(json!({"error":{"message":"Individual quota reached. Resets in 1h"}})).status(429)}else{assert_eq!(request["body"]["project"],"project-b");Reply::json(json!({"candidates":[{"content":{"parts":[{"text":"rotated"}]},"finishReason":"STOP"}]}))}}).await;
    core.inner
        .endpoints
        .lock()
        .unwrap()
        .insert("antigravity".into(), http.url.clone());
    core.patch_config(json!({"provider":"antigravity","model":"fixture"}))
        .await
        .unwrap();
    core.handle(
        "agent:run",
        &[json!({"runId":"agent-quota","task":"fixture"})],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(next_event(&mut events, "answer").await["text"], "rotated");
    next_event(&mut events, "done").await;
    assert!(core
        .antigravity_accounts()
        .await
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["accountId"] == "b" && a["active"] == true));
    let unavailable =
        server(|_| Reply::json(json!({"error":{"message":"Unavailable"}})).status(503)).await;
    core.inner
        .endpoints
        .lock()
        .unwrap()
        .insert("antigravity".into(), unavailable.url.clone());
    core.inner
        .endpoints
        .lock()
        .unwrap()
        .insert("antigravity-fallback-1".into(), http.url.clone());
    core.handle(
        "agent:run",
        &[json!({"runId":"agent-endpoint","task":"fixture"})],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(next_event(&mut events, "answer").await["text"], "rotated");
    next_event(&mut events, "done").await;
    core.shutdown().await;
}
#[tokio::test]
async fn opencode_auth_file_key_and_native_catalog_do_not_require_execution() {
    let (_dir, core, _events) = fixture();
    let auth = core
        .inner
        .ctx
        .paths
        .home_dir
        .join(".local/share/opencode/auth.json");
    std::fs::create_dir_all(auth.parent().unwrap()).unwrap();
    common::write_json_atomic(&auth,&json!({"opencode":{"type":"api","key":"fixture-disk-key"},"opencode-go":{"type":"api","key":"fixture-go-key"}})).unwrap();
    assert_eq!(
        core.opencode_key("configured", false).await.unwrap(),
        "configured"
    );
    assert_eq!(
        core.opencode_key("", false).await.unwrap(),
        "fixture-disk-key"
    );
    assert_eq!(core.opencode_key("", true).await.unwrap(), "fixture-go-key");
    let http = server(|request| {
        assert_eq!(
            request["headers"]["authorization"],
            "Bearer fixture-disk-key"
        );
        Reply::json(json!({"data":[{"id":"native-discovered"},{"id":"jev-non-chat"}]}))
    })
    .await;
    core.inner
        .endpoints
        .lock()
        .unwrap()
        .insert("opencode-models".into(), http.url.clone());
    let models = core
        .handle("llm-list-models", &[json!("opencode")])
        .await
        .unwrap()
        .unwrap();
    assert!(models
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m == "opencode/native-discovered"));
    assert!(!models.to_string().contains("jev-non-chat"));
    core.shutdown().await;
}
#[tokio::test]
async fn native_antigravity_search_and_image_tools_use_cloud_code_and_write_only_after_approval() {
    let (_dir, core, mut events) = fixture();
    let dir = core.pi_dir();
    std::fs::create_dir_all(&dir).unwrap();
    common::write_json_atomic(&dir.join("antigravity-accounts.json"),&json!({"version":1,"activeAccountId":"fixture","accounts":{"fixture":{"accountId":"fixture","access":"fixture-tool-token","refresh":"fixture-tool-refresh","expires":now_ms()+3600000,"projectId":"fixture-project"}}})).unwrap();
    let png="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a0l8AAAAASUVORK5CYII=";
    let http=server(move|r|{assert_eq!(r["body"]["project"],"fixture-project");assert!(s(&r["headers"]["authorization"]).contains("fixture-tool-token"));let content=if r["body"]["requestType"]=="image_gen"{json!({"parts":[{"inlineData":{"mimeType":"image/png","data":png}}]})}else{assert!(r["body"]["request"]["tools"].as_array().unwrap().iter().any(|t|t.get("googleSearch").is_some()));json!({"parts":[{"text":"grounded evidence"}]})};Reply::json(json!({"response":{"candidates":[{"content":content,"groundingMetadata":{"groundingChunks":[{"web":{"title":"Official fixture","uri":"https://example.invalid/primary"}}]}}]}}))}).await;
    core.inner
        .endpoints
        .lock()
        .unwrap()
        .insert("antigravity".into(), http.url.clone());
    core.patch_config(json!({"agentApprovalMode":"turbo"}))
        .await
        .unwrap();
    let cancel = Cancel::new();
    let result=core.native_antigravity_tool("google_search",json!({"query":"fixture","instruction":"Check primary sources","urls":["https://example.invalid/primary"]}),&core.inner.ctx.paths.home_dir,None,"agent-search",&cancel).await.unwrap();
    assert!(format!("{:?}", result.0).contains("grounded evidence"));
    assert!(format!("{:?}", result.0).contains("https://example.invalid/primary"));
    let generated = core
        .native_antigravity_tool(
            "generate_image",
            json!({"prompt":"fixture","path":"fixture.png"}),
            &core.inner.ctx.paths.home_dir,
            None,
            "agent-image",
            &cancel,
        )
        .await
        .unwrap();
    assert!(generated
        .0
        .iter()
        .any(|b| matches!(b, runtime::transcript::Block::Image { .. })));
    assert!(core
        .inner
        .ctx
        .paths
        .home_dir
        .join("generated-images/fixture.png")
        .is_file());
    core.inner.state.lock().unwrap().agents.insert(
        "agent-denied-image".into(),
        Run {
            id: "agent-denied-image".into(),
            cancel: Cancel::new(),
            emitted: false,
        },
    );
    let owner = core.clone();
    let path = core.inner.ctx.paths.home_dir.clone();
    let task = tokio::spawn(async move {
        owner
            .native_antigravity_tool(
                "generate_image",
                json!({"prompt":"fixture","path":"denied.png"}),
                &path,
                Some("review"),
                "agent-denied-image",
                &Cancel::new(),
            )
            .await
    });
    let approval = next_event(&mut events, "approval").await;
    core.handle("agent:approve",&[json!({"runId":"agent-denied-image","approvalId":approval["approvalId"],"decision":"deny"})]).await.unwrap().unwrap();
    assert!(task.await.unwrap().is_err());
    assert!(!core
        .inner
        .ctx
        .paths
        .home_dir
        .join("generated-images/denied.png")
        .exists());
    core.shutdown().await;
}
#[tokio::test]
async fn device_flow_cancellation_rejects_stale_success_and_success_returns_no_tokens() {
    let (_dir, core, _) = fixture();
    let mode = Arc::new(AtomicUsize::new(0));
    let state = mode.clone();
    let mut http=server(move|r|{if s(&r["raw"]).contains("grant_type="){let mut result=Reply::json(json!({"access_token":"fixture-device-token","refresh_token":"fixture-device-refresh","expires_in":3600}));if state.load(Ordering::SeqCst)==0{result.delay=Duration::from_millis(100)}result}else{assert!(s(&r["raw"]).contains("scope=read%3Auser"));Reply::json(json!({"device_code":"fixture-device-code","user_code":"VISIBLE","verification_uri":"https://github.com/login/device","interval":1,"expires_in":300}))}}).await;
    core.inner
        .endpoints
        .lock()
        .unwrap()
        .insert("github-oauth".into(), http.url.clone());
    let started = core
        .handle("github-device-start", &[])
        .await
        .unwrap()
        .unwrap();
    assert!(!started.to_string().contains("fixture-device-code"));
    let owner = core.clone();
    let polling =
        tokio::spawn(async move { owner.handle("github-device-poll", &[]).await.unwrap() });
    loop {
        let r = http.requests.recv().await.unwrap();
        if s(&r["raw"]).contains("grant_type=") {
            break;
        }
    }
    core.handle("github-device-cancel", &[])
        .await
        .unwrap()
        .unwrap();
    let stale = polling.await.unwrap().unwrap();
    assert_eq!(stale["status"], "error");
    assert!(!core
        .read_config()
        .await
        .unwrap()
        .to_string()
        .contains("fixture-device-token"));
    mode.store(1, Ordering::SeqCst);
    core.handle("github-device-start", &[])
        .await
        .unwrap()
        .unwrap();
    let success = core
        .handle("github-device-poll", &[])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(success["status"], "success");
    assert!(!success.to_string().contains("fixture-device-token"));
    assert_eq!(
        core.read_config().await.unwrap()["providerConfigs"]["copilot"]["copilotGithubToken"],
        "fixture-device-token"
    );
    core.shutdown().await;
}

#[tokio::test]
async fn copilot_catalog_exchanges_rejected_token_once_and_cache_is_credential_scoped() {
    let (_dir, core, _) = fixture();
    let exchanges = Arc::new(AtomicUsize::new(0));
    let count = exchanges.clone();
    let catalog = Arc::new(AtomicUsize::new(0));
    let lists = catalog.clone();
    let http=server(move|r| {
        if s(&r["line"]).contains("/exchange") {count.fetch_add(1, Ordering::SeqCst);return Reply::json(json!({"token":"fixture-session;proxy-ep=proxy.individual.githubcopilot.com;","expires_at":now_ms()/1000+3600}));}
        lists.fetch_add(1, Ordering::SeqCst);
        if r["headers"]["authorization"]=="Bearer fixture-first" {Reply::json(json!({})).status(401)} else {Reply::json(json!({"data":[{"id":if r["headers"]["authorization"]=="Bearer fixture-second"{"second"}else{"first"},"supported_endpoints":["/responses"]}]}))}
    }).await;
    core.inner.endpoints.lock().unwrap().extend([
        ("copilot-models".into(), format!("{}/models", http.url)),
        ("copilot-session".into(), format!("{}/exchange", http.url)),
        ("copilot-proxy".into(), http.url.clone()),
    ]);
    core.patch_config(
        json!({"providerConfigs":{"copilot":{"copilotGithubToken":"fixture-first"}}}),
    )
    .await
    .unwrap();
    assert_eq!(core.copilot_models().await.unwrap()[0]["id"], "first");
    assert_eq!(core.copilot_models().await.unwrap()[0]["id"], "first");
    assert_eq!(exchanges.load(Ordering::SeqCst), 1);
    assert_eq!(catalog.load(Ordering::SeqCst), 2);
    core.patch_config(
        json!({"providerConfigs":{"copilot":{"copilotGithubToken":"fixture-second"}}}),
    )
    .await
    .unwrap();
    assert_eq!(core.copilot_models().await.unwrap()[0]["id"], "second");
    assert_eq!(catalog.load(Ordering::SeqCst), 3);
    core.shutdown().await;
}

#[tokio::test]
async fn unsigned_copilot_never_borrows_generic_credentials_or_makes_requests() {
    let (_dir, core, mut events) = fixture();
    let mut http = server(|_| Reply::json(json!({"error":"fixture rejected"})).status(401)).await;
    core.inner.endpoints.lock().unwrap().extend([
        ("copilot-models".into(), http.url.clone()),
        ("copilot-session".into(), http.url.clone()),
        ("copilot".into(), http.url.clone()),
    ]);
    let catalog = server(|_| Reply::json(json!({"data":[]}))).await;
    for id in config::PROVIDERS.iter().filter(|id| **id != "copilot") {
        core.inner
            .endpoints
            .lock()
            .unwrap()
            .insert(format!("{id}-models"), catalog.url.clone());
    }
    core.patch_config(json!({"provider":"openai","apiKey":"fixture-other-provider-key"}))
        .await
        .unwrap();
    let statuses = core
        .handle("llm-provider-statuses", &[])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(statuses["copilot"], false);
    assert!(
        http.requests.try_recv().is_err(),
        "status leaked another provider's credential"
    );
    core.patch_config(json!({"provider":"copilot"}))
        .await
        .unwrap();
    core.handle(
        "agent:run",
        &[json!({"runId":"agent-unsigned-copilot","task":"fixture"})],
    )
    .await
    .unwrap()
    .unwrap();
    assert!(next_event(&mut events, "error")
        .await
        .to_string()
        .contains("Sign in to Copilot"));
    next_event(&mut events, "done").await;
    assert!(
        http.requests.try_recv().is_err(),
        "unsigned inference contacted Copilot"
    );
    core.shutdown().await;
}

#[tokio::test]
async fn credentialed_catalog_and_inference_refuse_redirects() {
    let (_dir, core, _events) = fixture();
    let mut target = server(|_| Reply::json(json!({"data":[]}))).await;
    let location = target.url.clone();
    let source = server(move |_| {
        let mut r = Reply::json(json!({})).status(307);
        r.headers.push(("Location".into(), location.clone()));
        r
    })
    .await;
    core.inner
        .endpoints
        .lock()
        .unwrap()
        .insert("anthropic-models".into(), source.url.clone());
    let cfg = json!({"provider":"anthropic","baseURL":source.url,"apiKey":"fixture-private"});
    assert!(!core.available(&cfg).await);
    assert!(target.requests.try_recv().is_err());
    let custom = json!({"provider":"custom:redirect","model":"fixture","customProviders":[{"id":"custom:redirect","title":"fixture","baseURL":source.url,"protocol":"anthropic","headers":{"X-Private":"fixture-private"}}]});
    let result = core
        .canonical_turn(
            &custom,
            &[runtime::transcript::Message::text("user", "hello")],
            &[],
            0,
            &Cancel::new(),
            |_| {},
        )
        .await;
    assert!(result.is_err());
    assert!(target.requests.try_recv().is_err());
    core.shutdown().await;
}

#[tokio::test]
async fn custom_catalog_status_uses_protocol_and_headers() {
    for (protocol, suffix) in [
        ("anthropic", "/v1/models"),
        ("gemini", "/models"),
        ("ollama", "/api/tags"),
        ("openai-responses", "/models"),
    ] {
        let (_dir, core, _events) = fixture();
        let expected = suffix.to_string();
        let http = server(move |r| {
            assert!(s(&r["line"]).contains(&expected), "{r}");
            assert_eq!(r["headers"]["x-catalog-key"], "fixture-catalog-private");
            Reply::json(json!({"data":[{"id":"fixture-model"}]}))
        })
        .await;
        core.patch_config(json!({"provider":"custom:catalog","model":"fixture-model","customProviders":[{"id":"custom:catalog","title":"Catalog","baseURL":http.url,"protocol":protocol,"headers":{"X-Catalog-Key":"fixture-catalog-private"}}]})).await.unwrap();
        let cfg = core.cfg(None, None).await.unwrap();
        assert!(core.available(&cfg).await);
        assert_eq!(
            core.models("custom:catalog", None, None).await.unwrap(),
            json!(["fixture-model"])
        );
        core.shutdown().await;
    }
}

#[tokio::test]
async fn antigravity_empty_reply_retries_and_model_fallback_has_its_own_budget() {
    let (_dir, core, _events) = fixture();
    let pi = core.pi_dir();
    std::fs::create_dir_all(&pi).unwrap();
    common::write_json_atomic(&pi.join("auth.json"),&json!({"antigravity":{"access":"fixture-ag","refresh":"fixture-ag-refresh","expires":now_ms()+3600000,"email":"fixture@example.invalid","projectId":"fixture-project"}})).unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let counter = count.clone();
    let http=server(move |r| {let n=counter.fetch_add(1,Ordering::SeqCst);if n<2 {Reply::json(json!({"response":{"candidates":[{"content":{"parts":[]},"finishReason":"STOP"}]}}))}else{assert!(r["body"]["project"].as_str().is_some_and(|p|!p.is_empty()));Reply::json(json!({"response":{"candidates":[{"content":{"parts":[{"text":"recovered"}]},"finishReason":"STOP"}]}}))}}).await;
    core.inner
        .endpoints
        .lock()
        .unwrap()
        .insert("antigravity".into(), http.url.clone());
    let cfg = config::selected(
        &json!({"provider":"antigravity","model":"gemini-3.8-flash"}),
        None,
        None,
    );
    let turn = core
        .canonical_turn(
            &cfg,
            &[runtime::transcript::Message::text("user", "fixture")],
            &[],
            0,
            &Cancel::new(),
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(turn.text, "recovered");
    assert_eq!(count.load(Ordering::SeqCst), 3);
    core.shutdown().await;
}

#[tokio::test]
async fn provider_errors_keep_messages_retry_before_text_and_never_persist_opaque_secrets() {
    let (_dir, core, _events) = fixture();
    let count = Arc::new(AtomicUsize::new(0));
    let counter = count.clone();
    let http=server(move|_| {let n=counter.fetch_add(1,Ordering::SeqCst);let mut reply=Reply::json(json!({}));reply.content_type="text/event-stream".into();reply.body=if n==0 {format!("data: {}\n\n",json!({"error":{"type":"overloaded_error","message":"fixture temporarily overloaded"}}))}else{format!("data: {}\n\n",json!({"choices":[{"delta":{"content":"recovered"},"finish_reason":"stop"}]}))};reply}).await;
    let cfg = json!({"provider":"openai-compatible","model":"fixture","baseURL":http.url,"apiKey":"fixture-wire-private"});
    let turn = core
        .canonical_turn(
            &cfg,
            &[runtime::transcript::Message::text("user", "fixture")],
            &[],
            0,
            &Cancel::new(),
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(turn.text, "recovered");
    assert_eq!(count.load(Ordering::SeqCst), 2);
    let http =
        server(|_| Reply::json(json!({"error":"model does not support tools"})).status(400)).await;
    let mut cfg = cfg;
    cfg["baseURL"] = json!(http.url);
    assert!(core
        .canonical_turn(&cfg, &[], &[], 0, &Cancel::new(), |_| {})
        .await
        .err()
        .unwrap()
        .contains("model does not support tools"));
    let http=server(|_|Reply::json(json!({"choices":[{"message":{"reasoning_content":"fixture-wire-private","content":"answer"},"finish_reason":"stop"}]}))).await;
    cfg["baseURL"] = json!(http.url);
    let error = core
        .canonical_turn(&cfg, &[], &[], 0, &Cancel::new(), |_| {})
        .await
        .err()
        .unwrap();
    assert!(error.contains("opaque history"));
    assert!(!error.contains("fixture-wire-private"));
    core.shutdown().await;
}

#[tokio::test]
async fn model_switch_migrates_legacy_credentials_to_their_original_owner() {
    let (_dir, core, _events) = fixture();
    core.patch_config(json!({"provider":"openai","apiKey":"fixture-original-openai","baseURL":"https://original.example/v1"})).await.unwrap();
    core.handle("llm-config-set",&[json!({"provider":"opencode","model":"opencode/fixture","providerModels":{"opencode":[{"id":"opencode/fixture"}]}})]).await.unwrap().unwrap();
    let raw = core.read_config().await.unwrap();
    assert_eq!(
        raw["providerConfigs"]["openai"]["apiKey"],
        "fixture-original-openai"
    );
    let selected = config::selected(&raw, None, Some("opencode"));
    assert!(s(&selected["apiKey"]).is_empty());
    assert_ne!(selected["baseURL"], "https://original.example/v1");
    core.shutdown().await;
}

#[tokio::test]
async fn antigravity_model_fallback_is_reached_with_all_three_endpoints_configured() {
    let (_dir, core, _events) = fixture();
    let pi = core.pi_dir();
    std::fs::create_dir_all(&pi).unwrap();
    common::write_json_atomic(&pi.join("auth.json"),&json!({"antigravity":{"access":"fixture-ag-fallback","refresh":"fixture-ag-refresh","expires":now_ms()+3600000,"email":"fallback@example.invalid"}})).unwrap();
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let models = seen.clone();
    let http=server(move|r| {let model=s(&r["body"]["model"]);models.lock().unwrap().push(model.into());assert!(r["body"]["project"].as_str().is_some_and(|s|!s.is_empty()));if model.starts_with("gemini-3.8") {Reply::json(json!({"error":{"message":"Model not found"}})).status(404)}else{Reply::json(json!({"response":{"candidates":[{"content":{"parts":[{"text":"model fallback"}]},"finishReason":"STOP"}]}}))}}).await;
    let fallback1 = server(|_| {
        panic!("recognized missing-model errors must try model fallback before endpoint walking")
    })
    .await;
    let fallback2 = server(|_| panic!("fallback budget was exhausted by endpoint walking")).await;
    core.inner.endpoints.lock().unwrap().extend([
        ("antigravity".into(), http.url.clone()),
        ("antigravity-fallback-1".into(), fallback1.url.clone()),
        ("antigravity-fallback-2".into(), fallback2.url.clone()),
    ]);
    let implicit = config::selected(
        &json!({"provider":"antigravity","baseURL":"https://cloudcode-pa.googleapis.com"}),
        None,
        None,
    );
    assert_eq!(implicit["_explicitBaseURL"], false);
    let cfg = config::selected(
        &json!({"provider":"antigravity","model":"gemini-3.8-flash"}),
        None,
        None,
    );
    assert_eq!(core.antigravity_bases(&cfg).unwrap().len(), 3);
    let result = core
        .canonical_turn(
            &cfg,
            &[runtime::transcript::Message::text("user", "fixture")],
            &[],
            0,
            &Cancel::new(),
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(result.text, "model fallback");
    assert_eq!(seen.lock().unwrap().len(), 2);
    assert!(seen.lock().unwrap()[1].starts_with("gemini-3.7"));
    core.shutdown().await;
}

#[tokio::test]
async fn opencode_go_default_and_explicit_uncatalogued_protocol_are_native_routes() {
    let (_dir, core, _events) = fixture();
    let catalog: Value = serde_json::from_str(include_str!("opencode_catalog.json")).unwrap();
    let model = catalog
        .as_object()
        .unwrap()
        .keys()
        .find(|id| id.starts_with("opencode-go/"))
        .unwrap();
    let cfg = config::selected(
        &json!({"provider":"opencode","model":model,"apiKey":"fixture-go-key"}),
        None,
        None,
    );
    let route = core.native_route(&cfg, &[]).await.unwrap();
    assert!(
        route
            .url
            .as_str()
            .starts_with("https://opencode.ai/inference/go/"),
        "{}",
        route.url
    );
    let cfg = config::selected(
        &json!({"provider":"opencode","model":"opencode/uncatalogued","apiKey":"fixture-go-key","providerModels":{"opencode":[{"id":"opencode/uncatalogued","protocol":"gemini"}]}}),
        None,
        None,
    );
    let route = core.native_route(&cfg, &[]).await.unwrap();
    assert_eq!(route.origin.protocol, runtime::transcript::Protocol::Gemini);
    assert!(route.headers.contains_key("authorization"));
    core.shutdown().await;
}

#[tokio::test]
async fn revoked_unexpired_copilot_session_is_exchanged_again_once() {
    use std::sync::atomic::AtomicBool;
    let (_dir, core, _events) = fixture();
    let exchanges = Arc::new(AtomicUsize::new(0));
    let count = exchanges.clone();
    let revoked = Arc::new(AtomicBool::new(false));
    let reject = revoked.clone();
    let http=server(move|r|{if s(&r["line"]).contains("/exchange") {let n=count.fetch_add(1,Ordering::SeqCst)+1;return Reply::json(json!({"token":format!("fixture-session-{n};proxy-ep=proxy.individual.githubcopilot.com;"),"expires_at":now_ms()/1000+3600}));}let auth=s(&r["headers"]["authorization"]);if auth=="Bearer fixture-github" || reject.load(Ordering::SeqCst) && auth.contains("fixture-session-1;"){Reply::json(json!({"error":{"message":"revoked"}})).status(401)}else{Reply::json(json!({"choices":[{"message":{"content":"signed in"},"finish_reason":"stop"}]}))}}).await;
    core.inner.endpoints.lock().unwrap().extend([
        ("copilot".into(), http.url.clone()),
        ("copilot-proxy".into(), http.url.clone()),
        ("copilot-session".into(), format!("{}/exchange", http.url)),
    ]);
    core.patch_config(json!({"provider":"copilot","model":"fixture","providerConfigs":{"copilot":{"copilotGithubToken":"fixture-github","protocol":"openai-chat"}}})).await.unwrap();
    let cfg = core.cfg(None, None).await.unwrap();
    for _ in 0..2 {
        assert_eq!(
            core.canonical_turn(
                &cfg,
                &[runtime::transcript::Message::text("user", "fixture")],
                &[],
                0,
                &Cancel::new(),
                |_| {}
            )
            .await
            .unwrap()
            .text,
            "signed in"
        );
        revoked.store(true, Ordering::SeqCst);
    }
    assert_eq!(exchanges.load(Ordering::SeqCst), 2);
    core.shutdown().await;
}

#[tokio::test]
async fn oversized_historical_tool_image_is_projected_without_poisoning_or_mutating_history() {
    use runtime::transcript::{Block, Message};
    let (_dir, core, _events) = fixture();
    let image = "A".repeat(10_000_004);
    let messages = vec![
        Message::text("user", "continue"),
        Message {
            role: "tool".into(),
            opaque: None,
            blocks: vec![Block::ToolResult {
                call: "c".into(),
                wire_id: "c".into(),
                name: "screen_capture".into(),
                blocks: vec![Block::Image {
                    mime: "image/png".into(),
                    data: image.clone(),
                }],
                is_error: false,
            }],
        },
    ];
    let http = server(|r| {
        assert!(!r["body"].to_string().contains(&"A".repeat(100)));
        assert!(r["body"].to_string().contains("Earlier image omitted"));
        Reply::json(
            json!({"content":[{"type":"text","text":"continued"}],"stop_reason":"end_turn"}),
        )
    })
    .await;
    let cfg = json!({"provider":"anthropic","model":"fixture","baseURL":http.url,"apiKey":"fixture-image-key"});
    assert_eq!(
        core.canonical_turn(&cfg, &messages, &[], 0, &Cancel::new(), |_| {})
            .await
            .unwrap()
            .text,
        "continued"
    );
    if let Block::ToolResult { blocks, .. } = &messages[1].blocks[0] {
        if let Block::Image { data, .. } = &blocks[0] {
            assert_eq!(data, &image)
        } else {
            panic!("canonical image mutated")
        }
    }
    core.shutdown().await;
}

#[tokio::test]
async fn official_openai_chat_uses_completion_token_limit_through_real_transport() {
    let (_dir, core, _events) = fixture();
    let http=server(|r|{assert!(r["body"].get("max_tokens").is_none());assert!(r["body"]["max_completion_tokens"].as_u64().is_some());Reply::json(json!({"choices":[{"message":{"content":"official compatibility"},"finish_reason":"stop"}]}))}).await;
    core.inner
        .endpoints
        .lock()
        .unwrap()
        .insert("openai".into(), http.url.clone());
    let cfg = json!({"provider":"openai","model":"gpt-5.5","baseURL":"https://api.openai.com/v1","protocol":"openai-chat","apiKey":"fixture-openai-compatible"});
    assert_eq!(
        core.canonical_turn(
            &cfg,
            &[runtime::transcript::Message::text("user", "fixture")],
            &[],
            0,
            &Cancel::new(),
            |_| {}
        )
        .await
        .unwrap()
        .text,
        "official compatibility"
    );
    core.shutdown().await;
}
