use super::*;
use crate::native_core::common::CorePaths;
fn context(path: &std::path::Path) -> CoreContext {
    CoreContext::headless(CorePaths {
        data_dir: path.into(),
        config_dir: path.join("config"),
        resource_dir: path.join("resources"),
        temp_dir: path.join("tmp"),
        home_dir: path.into(),
    })
}
#[tokio::test]
async fn real_pipes_reverse_rpc_and_disposal() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    let calls = Arc::new(Mutex::new(Vec::new()));
    let captured = calls.clone();
    ctx.set_router(Arc::new(move |channel, args| {
        captured.lock().unwrap().push((channel, args));
        Box::pin(async { Ok(json!(true)) })
    }))
    .unwrap();
    process_install::test_install(&ctx, "reverse");
    let sessions = NativeSessions::default();
    let files = Arc::new(Mutex::new(()));
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        sessions.handle(
            &ctx,
            &files,
            "extension:run-command",
            &[json!({"extensionId":"com.tezbar.native.perplexity","commandName":"ask-perplexity"})],
        ),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    assert_eq!(result["title"], "Reverse completed");
    assert_eq!(result["refreshMs"], Value::Null);
    assert_eq!(calls.lock().unwrap()[0].0, "shell:open");
    let id = result["sessionId"].as_str().unwrap();
    assert_eq!(
        sessions
            .handle(
                &ctx,
                &files,
                "extension:dispose-session",
                &[json!({"sessionId":id})]
            )
            .await
            .unwrap()
            .unwrap(),
        true
    );
    assert!(sessions.0.lock().unwrap().sessions.is_empty());
    sessions.clear().await;
}
#[tokio::test]
async fn no_child_on_listing_and_canonical_damage_never_falls_back() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    let sessions = NativeSessions::default();
    let files = Arc::new(Mutex::new(()));
    super::super::v2_catalog::cli_list(&ctx).unwrap();
    registry::handle(&ctx, "extension:list", &[]).unwrap();
    assert!(sessions.0.lock().unwrap().slots.is_empty());
    assert!(sessions
        .handle(
            &ctx,
            &files,
            "extension:run-command",
            &[json!({"extensionId":"raycast.unrelated","commandName":"run"})]
        )
        .await
        .is_none());
    let result = sessions
        .handle(
            &ctx,
            &files,
            "extension:run-command",
            &[json!({"extensionId":"com.tezbar.native.perplexity","commandName":"ask-perplexity"})],
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result["ok"], false);
    assert!(sessions.0.lock().unwrap().sessions.is_empty());
}
#[tokio::test]
async fn exe_named_fixture_keeps_open_and_call_crashes_distinct() {
    use tokio::io::AsyncWriteExt;
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    process_install::test_install(&ctx, "normal");
    let package = process_install::resolve(&ctx, "com.tezbar.native.perplexity").unwrap();
    for (mode, code) in [("crash", 17), ("crashcall", 19)] {
        // Exercise Windows spelling with the real executable on every host.
        let executable = temp.path().join(format!("fixture-{mode}.exe"));
        std::fs::copy(&package.executable, &executable).unwrap();
        let mut child = tokio::process::Command::new(executable)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let packets = [
            json!({"type":"request","id":"init","method":"initialize","params":{"dataDir":temp.path()}}),
            json!({"type":"request","id":"open","method":"session.open"}),
            json!({"type":"request","id":"call","method":"session.call"}),
        ];
        let mut input = child.stdin.take().unwrap();
        let payload: String = packets
            .into_iter()
            .map(|packet| format!("{packet}\n"))
            .collect();
        input.write_all(payload.as_bytes()).await.unwrap();
        drop(input);
        let output = tokio::time::timeout(Duration::from_secs(10), child.wait_with_output())
            .await
            .unwrap()
            .unwrap();
        let frames: Vec<Value> = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(output.status.code(), Some(code), "{mode}: {frames:?}");
        assert_eq!(
            frames
                .iter()
                .any(|frame| frame["id"] == "open" && frame["result"]["ok"] == true),
            mode == "crashcall",
            "{mode}: {frames:?}"
        );
    }
}
#[tokio::test]
async fn crash_malformed_and_bad_identity_drain_pending_launches() {
    for mode in ["crash", "malformed", "badhello"] {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        process_install::test_install(&ctx, mode);
        let sessions = NativeSessions::default();
        let files = Arc::new(Mutex::new(()));
        let result=tokio::time::timeout(Duration::from_secs(10),sessions.handle(&ctx,&files,"extension:run-command",&[json!({"extensionId":"com.tezbar.native.perplexity","commandName":"ask-perplexity"})])).await.unwrap().unwrap().unwrap();
        assert_eq!(result["ok"], false, "{mode}: {result}");
        assert!(sessions.0.lock().unwrap().sessions.is_empty());
        sessions.clear().await;
    }
}
#[tokio::test]
async fn deadline_sends_real_cancel_packet_and_does_not_replay() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    process_install::test_install(&ctx, "delay");
    let package = process_install::resolve(&ctx, "com.tezbar.native.perplexity").unwrap();
    let p = Process::spawn(&ctx, package, "test").await.unwrap();
    let error = p
        .call(
            "session.open",
            json!({"sessionId":"test","commandName":"ask-perplexity"}),
            Duration::from_millis(40),
        )
        .await
        .unwrap_err();
    assert!(error.contains("deadline"));
    let cancellation = p.host.paths().unwrap().0.join("cancelled");
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if cancellation.is_file() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(p.pending_count_for_test() == 0);
    p.stop_and_wait().await;
}
#[tokio::test]
async fn frame_limits_truncation_and_unicode() {
    let mut r = tokio::io::BufReader::new(&b"{\"text\":\"\xe2\x80\xa8\"}\n{}\n"[..]);
    assert!(super::super::process::read_frame(&mut r)
        .await
        .unwrap()
        .is_some());
    assert_eq!(
        super::super::process::read_frame(&mut r)
            .await
            .unwrap()
            .unwrap(),
        b"{}"
    );
    assert!(
        super::super::process::read_frame(&mut tokio::io::BufReader::new(&b"{partial"[..]))
            .await
            .is_err()
    );
    assert!(
        super::super::process::read_frame(&mut tokio::io::BufReader::new(&b"{}\r\n"[..]))
            .await
            .is_err()
    );
    let bytes = vec![b'x'; super::super::process::MAX_FRAME + 1];
    assert!(
        super::super::process::read_frame(&mut tokio::io::BufReader::new(bytes.as_slice()))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn background_event_survives_interactive_work_but_has_a_bounded_deadline() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    process_install::test_install(&ctx, "event-delay");
    let package = process_install::resolve(&ctx, "com.tezbar.native.perplexity").unwrap();
    let p = Process::spawn(&ctx, package, "test").await.unwrap();
    let child = p.clone();
    let mut callback = tokio::spawn(async move {
        child
            .background_event(json!({"resourceId":"tray","actionId":"interactive"}))
            .await
    });
    let marker = p.host.paths().unwrap().0.join("background-event");
    tokio::select! {
        result = &mut callback => panic!("Callback ended before dispatch: {result:?}"),
        _ = wait_file(&marker) => {}
    }
    let request: Value = serde_json::from_slice(&std::fs::read(marker).unwrap()).unwrap();
    assert_eq!(request["method"], "background.event");
    assert_eq!(request["params"]["actionId"], "interactive");
    // Only advance host time after the real child has received the callback.
    // No GUI/hardware access or 120-second wall-clock sleep is needed.
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(31)).await;
    tokio::task::yield_now().await;
    assert!(
        !callback.is_finished(),
        "Interactive background callback expired at 30 seconds"
    );
    assert!(p.alive());
    assert_eq!(p.pending_count_for_test(), 1);
    tokio::time::advance(Duration::from_secs(90)).await;
    let error = callback.await.unwrap().unwrap_err();
    assert!(error.contains("deadline"), "{error}");
    assert!(!p.alive(), "Expired callback must stop its resource owner");
    assert_eq!(p.pending_count_for_test(), 0);
    tokio::time::resume();
    p.stop_and_wait().await;
}

#[tokio::test]
async fn rejected_background_lease_does_not_persist_an_unowned_activation() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    process_install::test_install(&ctx, "normal");
    let package = process_install::resolve(&ctx, "com.tezbar.native.perplexity").unwrap();
    let p = Process::spawn(&ctx, package, "test").await.unwrap();
    for key in 0..128 {
        p.host
            .request(
                &p,
                "host.background.retain",
                json!({"key":format!("lease-{key}"),"durable":false}),
            )
            .await
            .unwrap();
    }
    let error = p
        .host
        .request(
            &p,
            "host.background.retain",
            json!({"key":"overflow","durable":true,"data":{"running":true}}),
        )
        .await
        .unwrap_err();
    assert!(error.contains("Too many background leases"), "{error}");
    assert_eq!(
        super::super::process_host::background_records(&ctx).unwrap(),
        json!({}),
        "Rejected retain must not restore an activation on the next startup"
    );
    // Existing keys can still become durable at capacity, and release still works.
    p.host
        .request(
            &p,
            "host.background.retain",
            json!({"key":"lease-0","durable":true,"data":{"running":true}}),
        )
        .await
        .unwrap();
    assert_eq!(
        super::super::process_host::background_records(&ctx).unwrap()
            ["com.tezbar.native.perplexity"]["lease-0"]["running"],
        true
    );
    p.host
        .request(&p, "host.background.release", json!({"key":"lease-0"}))
        .await
        .unwrap();
    assert_eq!(
        super::super::process_host::background_records(&ctx).unwrap(),
        json!({})
    );
    p.stop_and_wait().await;
}

#[tokio::test]
async fn reverse_services_wait_for_initialization_acknowledgement() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    let calls = Arc::new(Mutex::new(Vec::new()));
    let captured = calls.clone();
    ctx.set_router(Arc::new(move |channel, args| {
        captured.lock().unwrap().push((channel, args));
        Box::pin(async { Ok(json!(true)) })
    }))
    .unwrap();
    process_install::test_install(&ctx, "initservice");
    let package = process_install::resolve(&ctx, "com.tezbar.native.perplexity").unwrap();
    let p = Process::spawn(&ctx, package, "test").await.unwrap();
    let reply: Value = serde_json::from_slice(
        &std::fs::read(p.host.paths().unwrap().0.join("before-ready-response")).unwrap(),
    )
    .unwrap();
    assert!(
        reply["error"]["message"]
            .as_str()
            .is_some_and(|s| s.contains("before initialization completes")),
        "{reply}"
    );
    assert!(
        calls.lock().unwrap().is_empty(),
        "No host side effect before ready acknowledgement"
    );
    // A rejected early RPC does not poison the acknowledged process.
    assert!(p.ready());
    p.host
        .request(
            &p,
            "host.shell.open",
            json!({"target":"https://example.com/ready"}),
        )
        .await
        .unwrap();
    assert_eq!(calls.lock().unwrap().len(), 1);
    p.stop_and_wait().await;
}

#[tokio::test]
async fn service_ports_keep_exact_migration_files_and_oauth_budget() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    std::fs::create_dir_all(temp.path().join("native-google-oauth")).unwrap();
    for name in ["native-google-oauth/tokens.json", "named-ports.json"] {
        std::fs::write(temp.path().join(name), b"{}").unwrap();
    }
    let manifest = process_install::test_install_with(&ctx, "normal", |m| {
        m["migrationPaths"] = json!({"nativeGoogleOAuth":"native-google-oauth/tokens.json","namedPorts":"named-ports.json"});
        m["commands"][0]["timeoutMs"] = json!(240000);
    });
    let s = NativeSessions::default();
    let view = launch(&s, &ctx).await;
    let entry = s.0.lock().unwrap().sessions[view["sessionId"].as_str().unwrap()].clone();
    assert_eq!(deadline(&entry), Duration::from_secs(240));
    assert_eq!(
        entry.process.host.migration_paths(),
        json!({
            "nativeGoogleOAuth":temp.path().join("native-google-oauth/tokens.json"),
            "namedPorts":temp.path().join("named-ports.json")
        })
    );
    // Source manifests must use the frozen string map, not structured descriptors.
    for descriptor in [
        json!({"description":"tokens"}),
        json!({"source":"named-ports.json","kind":"file"}),
    ] {
        let mut invalid = manifest.clone();
        invalid["migrationPaths"]["namedPorts"] = descriptor;
        assert!(process_install::validate(&invalid).is_err());
    }
    std::fs::remove_file(temp.path().join("named-ports.json")).unwrap();
    std::fs::create_dir(temp.path().join("named-ports.json")).unwrap();
    assert!(entry
        .process
        .host
        .migration_paths()
        .get("namedPorts")
        .is_none());
    #[cfg(unix)]
    {
        std::fs::remove_file(temp.path().join("native-google-oauth/tokens.json")).unwrap();
        std::os::unix::fs::symlink(
            temp.path().join("named-ports.json"),
            temp.path().join("native-google-oauth/tokens.json"),
        )
        .unwrap();
        assert!(entry
            .process
            .host
            .migration_paths()
            .get("nativeGoogleOAuth")
            .is_none());
    }
    s.clear().await;
}

#[tokio::test]
async fn service_safety_shapes_reach_real_authorization_and_audit_without_os_actions() {
    use crate::native_core::system::SystemCore;
    let temp = tempfile::tempdir().unwrap();
    let safety_ctx = context(temp.path());
    let response = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let selected = response.clone();
    safety_ctx
        .set_router(Arc::new(move |channel, _| {
            assert_eq!(channel, "core:platform:confirm");
            let response = selected.load(Ordering::Acquire);
            Box::pin(async move { Ok(json!({"response":response})) })
        }))
        .unwrap();
    let safety = Arc::new(SystemCore::new(safety_ctx));
    let service = safety.clone();
    let ctx = context(temp.path());
    ctx.set_router(Arc::new(move |channel, args| {
        let service = service.clone();
        Box::pin(async move { service.handle(&channel, &args).await.unwrap() })
    }))
    .unwrap();
    process_install::test_install_with(&ctx, "normal", |m| m["capabilities"] = json!(["safety"]));
    let package = process_install::resolve(&ctx, "com.tezbar.native.perplexity").unwrap();
    let p = Process::spawn(&ctx, package, "test").await.unwrap();
    let input = json!({"operation":"port.kill","context":{"port":43210,"pid":12345,"process":"fixture","signal":"15","identity":"fixture-birth","extensionId":"spoofed"}});
    assert_eq!(
        p.host
            .request(&p, "host.safety.authorize", input.clone())
            .await
            .unwrap(),
        json!({"accepted":false,"dryRun":false})
    );
    response.store(1, Ordering::Release);
    safety
        .handle("safety:dry-run:set", &[json!(true)])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        p.host
            .request(&p, "host.safety.authorize", input.clone())
            .await
            .unwrap(),
        json!({"accepted":true,"dryRun":true})
    );
    safety
        .handle("safety:dry-run:set", &[json!(false)])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        p.host
            .request(&p, "host.safety.authorize", input.clone())
            .await
            .unwrap(),
        json!({"accepted":true,"dryRun":false})
    );
    let mut record = input;
    record["result"] = json!({"ok":true,"message":"Fixture result; no signal sent"});
    assert_eq!(
        p.host
            .request(&p, "host.safety.record", record.clone())
            .await
            .unwrap(),
        Value::Null
    );
    let log = safety.handle("safety:log", &[]).await.unwrap().unwrap();
    assert_eq!(log.as_array().unwrap().len(), 3);
    assert_eq!(log[0]["action"], "port.kill");
    assert_eq!(
        log[0]["context"]["extensionId"],
        "com.tezbar.native.perplexity"
    );
    for key in ["port", "pid", "process", "signal", "identity"] {
        assert_eq!(log[0]["context"][key], record["context"][key]);
    }
    std::fs::write(temp.path().join("safety/action-log.json"), b"broken").unwrap();
    assert!(p
        .host
        .request(&p, "host.safety.record", record)
        .await
        .is_err());
    p.stop_and_wait().await;
}

async fn run(s: &NativeSessions, ctx: &CoreContext, channel: &str, body: Value) -> Value {
    s.handle(ctx, &Arc::new(Mutex::new(())), channel, &[body])
        .await
        .expect("native request handled")
        .unwrap()
}
async fn launch(s: &NativeSessions, ctx: &CoreContext) -> Value {
    run(
        s,
        ctx,
        "extension:run-command",
        json!({"extensionId":"com.tezbar.native.perplexity","commandName":"ask-perplexity"}),
    )
    .await
}
async fn wait_file(path: &std::path::Path) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !path.is_file() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn lazy_shared_process_capacity_and_stale_ids_never_worker() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    process_install::test_install(&ctx, "normal");
    let s = NativeSessions::default();
    s.start(&ctx).unwrap();
    assert!(s.0.lock().unwrap().slots.is_empty());
    let (a, b) = tokio::join!(launch(&s, &ctx), launch(&s, &ctx));
    assert_eq!(a["ok"], true);
    assert_eq!(b["ok"], true);
    assert_ne!(a["sessionId"], b["sessionId"]);
    let entries =
        s.0.lock()
            .unwrap()
            .sessions
            .values()
            .cloned()
            .collect::<Vec<_>>();
    assert!(Arc::ptr_eq(&entries[0].process, &entries[1].process));
    let starts = entries[0].process.host.paths().unwrap().0.join("starts");
    assert_eq!(std::fs::read_to_string(starts).unwrap().lines().count(), 1);
    for _ in 2..128 {
        assert_eq!(launch(&s, &ctx).await["ok"], true);
    }
    assert_eq!(launch(&s, &ctx).await["ok"], false);
    assert_eq!(s.0.lock().unwrap().sessions.len(), 128);
    s.clear().await;
    let stale=run(&s,&ctx,"extension:run-command",json!({"sessionId":a["sessionId"],"extensionId":"raycast.perplexity","commandName":"ask-perplexity"})).await;
    assert_eq!(stale["ok"], false);
    assert!(s.0.lock().unwrap().sessions.is_empty());
}
#[tokio::test]
async fn search_supersession_busy_refresh_dispose_and_generation_fences() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    process_install::test_install_with(&ctx, "busy", |m| {
        m["commands"][0]["concurrency"] = json!("serialize")
    });
    let s = NativeSessions::default();
    let opened = launch(&s, &ctx).await;
    let id = opened["sessionId"].as_str().unwrap().to_string();
    let p = s.0.lock().unwrap().sessions[&id].process.clone();
    let old = {
        let s = s.clone();
        let ctx = ctx.clone();
        let id = id.clone();
        tokio::spawn(async move {
            run(
                &s,
                &ctx,
                "extension:search-text-changed",
                json!({"sessionId":id,"searchText":"old"}),
            )
            .await
        })
    };
    tokio::time::timeout(Duration::from_secs(2), async {
        while p.pending_count_for_test() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let refresh = run(
        &s,
        &ctx,
        "extension:refresh-session",
        json!({"sessionId":id}),
    )
    .await;
    assert_eq!(refresh["mode"], "unchanged");
    let latest = run(
        &s,
        &ctx,
        "extension:search-text-changed",
        json!({"sessionId":id,"searchText":"latest"}),
    )
    .await;
    assert_eq!(latest["title"], "Latest 世界");
    assert_eq!(old.await.unwrap()["ok"], false);
    wait_file(&p.host.paths().unwrap().0.join("cancelled")).await;
    let action = {
        let s = s.clone();
        let ctx = ctx.clone();
        let id = id.clone();
        tokio::spawn(async move {
            run(
                &s,
                &ctx,
                "extension:invoke-action",
                json!({"sessionId":id,"actionId":"blocked"}),
            )
            .await
        })
    };
    tokio::time::timeout(Duration::from_secs(2), async {
        while p.pending_count_for_test() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        run(
            &s,
            &ctx,
            "extension:dispose-session",
            json!({"sessionId":id})
        )
        .await,
        true
    );
    assert_eq!(action.await.unwrap()["ok"], false);
    assert!(s.0.lock().unwrap().sessions.is_empty());
    s.clear().await;
    assert!(!p.alive());
}
#[tokio::test]
async fn clear_other_extension_preserves_active_process_and_blur_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let captured = calls.clone();
    let ctx = context(temp.path());
    ctx.set_router(Arc::new(move |channel, args| {
        captured.lock().unwrap().push((channel, args));
        Box::pin(async { Ok(json!(true)) })
    }))
    .unwrap();
    process_install::test_install(&ctx, "normal");
    let s = NativeSessions::default();
    let opened = launch(&s, &ctx).await;
    let id = opened["sessionId"].as_str().unwrap();
    let p = s.0.lock().unwrap().sessions[id].process.clone();
    p.host
        .request(&p, "host.window.suppressBlur", json!({"suppress":true}))
        .await
        .unwrap();
    s.clear_slug("unrelated").await;
    assert!(p.alive());
    assert_eq!(
        run(
            &s,
            &ctx,
            "extension:refresh-session",
            json!({"sessionId":id})
        )
        .await["mode"],
        "unchanged"
    );
    s.clear().await;
    assert!(!p.alive());
    let calls = calls.lock().unwrap();
    assert_eq!(calls.last().unwrap().1[0], false);
    assert!(p
        .host
        .request(&p, "host.window.visibility", json!({"visible":true}))
        .await
        .is_err());
}
#[tokio::test]
async fn real_pipe_failures_stderr_and_handshake_fields() {
    for mode in [
        "badversion",
        "badid",
        "eofhello",
        "eofopen",
        "truncated",
        "oversize",
        "crashcall",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        process_install::test_install(&ctx, mode);
        let s = NativeSessions::default();
        let result = tokio::time::timeout(Duration::from_secs(10), launch(&s, &ctx))
            .await
            .unwrap();
        if mode == "crashcall" {
            assert_eq!(result["ok"], true);
            let r = run(
                &s,
                &ctx,
                "extension:refresh-session",
                json!({"sessionId":result["sessionId"]}),
            )
            .await;
            assert_eq!(r["ok"], false);
        } else {
            assert_eq!(result["ok"], false, "{mode}: {result}");
        }
        s.clear().await;
    }
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    process_install::test_install(&ctx, "stderr");
    let s = NativeSessions::default();
    assert_eq!(launch(&s, &ctx).await["ok"], true);
    s.clear().await;
}
#[tokio::test]
async fn typed_grants_durable_ack_and_startup_probe_are_generic() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    ctx.set_router(Arc::new(|_, _| Box::pin(async { Ok(json!(true)) })))
        .unwrap();
    process_install::test_install_with(&ctx, "normal", |m| {
        m["capabilities"] = json!(["shell.open", "background", "window"]);
        m["migrationPaths"] = json!({"old":"legacy/state.json"});
        m["startupProbe"] = json!({"key":"migration","migrationPaths":["old"]});
    });
    assert!(super::super::process_host::startup_probes(&ctx)
        .unwrap()
        .is_empty());
    std::fs::create_dir_all(temp.path().join("legacy")).unwrap();
    std::fs::write(temp.path().join("legacy/state.json"), b"{}").unwrap();
    let probes = super::super::process_host::startup_probes(&ctx).unwrap();
    assert_eq!(probes.len(), 1);
    let s = NativeSessions::default();
    let v = launch(&s, &ctx).await;
    let p = s.0.lock().unwrap().sessions[v["sessionId"].as_str().unwrap()]
        .process
        .clone();
    assert!(p
        .host
        .request(
            &p,
            "host.shell.open",
            json!({"target":"https://example.com"})
        )
        .await
        .is_ok());
    assert!(p
        .host
        .request(&p, "host.shell.reveal", json!({"path":"/tmp"}))
        .await
        .is_err());
    assert!(p
        .host
        .request(&p, "core:ports:named-add", json!({}))
        .await
        .is_err());
    assert!(p
        .host
        .request(
            &p,
            "host.background.retain",
            json!({"key":"work","durable":true,"data":{"n":1}})
        )
        .await
        .is_ok());
    assert_eq!(
        super::super::process_host::background_records(&ctx).unwrap()
            ["com.tezbar.native.perplexity"]["work"]["n"],
        1
    );
    s.clear().await;
    assert_eq!(
        super::super::process_host::background_records(&ctx).unwrap()
            ["com.tezbar.native.perplexity"]["work"]["n"],
        1
    );
    super::super::process_host::acknowledge_probe(&ctx, &probes[0].0, &probes[0].2).unwrap();
    assert!(super::super::process_host::startup_probes(&ctx)
        .unwrap()
        .is_empty());
    let paths = p.host.migration_paths();
    assert!(paths["old"]
        .as_str()
        .unwrap()
        .ends_with("legacy/state.json"));
    assert_eq!(paths.as_object().unwrap().len(), 1);
}
#[cfg(unix)]
#[tokio::test]
async fn shutdown_kills_actual_helper_process_group() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    process_install::test_install(&ctx, "tree-hangshutdown");
    let s = NativeSessions::default();
    let v = launch(&s, &ctx).await;
    let p = s.0.lock().unwrap().sessions[v["sessionId"].as_str().unwrap()]
        .process
        .clone();
    let file = p.host.paths().unwrap().0.join("helper-pid");
    wait_file(&file).await;
    let pid: i32 = std::fs::read_to_string(file).unwrap().parse().unwrap();
    assert_eq!(unsafe { libc::kill(pid, 0) }, 0);
    s.clear().await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while unsafe { libc::kill(pid, 0) } == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

/// The default fixture always exercises the full process/host path. Packaging
/// validation can supply a real independently built protocol-1 release instead.
#[tokio::test]
async fn installed_package_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    let calls = Arc::new(Mutex::new(Vec::new()));
    let captured = calls.clone();
    ctx.set_router(Arc::new(move |channel, args| {
        captured.lock().unwrap().push((channel, args));
        Box::pin(async { Ok(json!({"ok":true})) })
    }))
    .unwrap();
    let real = std::env::var_os("TEZBAR_HOST_TEST_MANIFEST");
    let mut extension_id = "com.tezbar.native.perplexity".to_string();
    let command = std::env::var("TEZBAR_HOST_TEST_COMMAND").unwrap_or("ask-perplexity".into());
    if let Some(path) = real.as_ref() {
        let manifest = crate::native_core::common::read_json(std::path::Path::new(path)).unwrap();
        extension_id = manifest["id"].as_str().unwrap().to_string();
        if extension_id != "com.tezbar.native.perplexity" {
            assert_eq!(extension_id, "com.tezbar.native.amphetamine");
            assert_eq!(
                command, "amp-duration",
                "Only the inert form may be used for headless acceptance"
            );
        }
        let zip =
            std::env::var_os("TEZBAR_HOST_TEST_ZIP").expect("ZIP required with release manifest");
        process_install::install(&ctx, manifest.clone(), Some(zip.into()))
            .await
            .unwrap();
        if let Some(windows) = std::env::var_os("TEZBAR_HOST_TEST_WINDOWS_ZIP") {
            let other = tempfile::tempdir().unwrap();
            let other_ctx = context(other.path());
            let installed = process_install::test_import_target(
                &other_ctx,
                &manifest,
                "x86_64-pc-windows-msvc",
                std::path::Path::new(&windows),
            )
            .unwrap();
            assert!(!installed["nativeV2"]["files"]
                .as_object()
                .unwrap()
                .is_empty());
            if cfg!(target_os = "macos") {
                assert!(
                    process_install::resolve(&other_ctx, &extension_id).is_err(),
                    "Windows package must not launch as a Mac artifact"
                );
            }
            eprintln!("Host acceptance: published Windows ZIP inventory/executable verified (not executed)");
        }
        eprintln!(
            "Host vertical slice: real packaged SDK executable from {}",
            std::path::Path::new(path).display()
        );
    } else {
        process_install::test_install(&ctx, "reverse");
    }
    let s = NativeSessions::default();
    let opened = run(
        &s,
        &ctx,
        "extension:run-command",
        json!({"extensionId":extension_id,"commandName":command}),
    )
    .await;
    assert_eq!(opened["ok"], true);
    assert_eq!(opened["extensionId"], extension_id);
    assert_eq!(opened["refreshMs"], Value::Null);
    if real.is_some() && extension_id == "com.tezbar.native.perplexity" {
        assert_eq!(opened["root"]["type"], "Form");
        let id = opened["sessionId"].clone();
        let invalid = run(
            &s,
            &ctx,
            "extension:invoke-action",
            json!({"sessionId":id,"actionId":"perplexity:submit","formValues":{"query":""}}),
        )
        .await;
        assert_eq!(invalid["ok"], false);
        assert!(calls.lock().unwrap().is_empty());
        let submitted=run(&s,&ctx,"extension:invoke-action",json!({"sessionId":id,"actionId":"perplexity:submit","formValues":{"query":" Привет 世界 & "}})).await;
        assert_eq!(submitted["mode"], "no-view");
        assert_eq!(calls.lock().unwrap()[0].1[0],"https://www.perplexity.ai/search?q=+%D0%9F%D1%80%D0%B8%D0%B2%D0%B5%D1%82+%E4%B8%96%E7%95%8C+%26+");
    }
    if real.is_some() && extension_id != "com.tezbar.native.perplexity" {
        assert_eq!(opened["root"]["type"], "Form");
        assert!(calls.lock().unwrap().is_empty());
        eprintln!("Host acceptance: published Amphetamine form only; no awake assertion or hardware action");
    } else {
        assert_eq!(calls.lock().unwrap().len(), 1);
        assert_eq!(calls.lock().unwrap()[0].0, "shell:open");
    }
    run(
        &s,
        &ctx,
        "extension:dispose-session",
        json!({"sessionId":opened["sessionId"]}),
    )
    .await;
    assert!(s.0.lock().unwrap().sessions.is_empty());
    s.clear().await;
}

#[tokio::test]
async fn actual_startup_probe_once_and_shutdown_cancels_pending_restore() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    process_install::test_install_with(&ctx, "normal", |m| {
        m["migrationPaths"] = json!({"old":"legacy/state.json"});
        m["startupProbe"] =
            json!({"key":"migrate","migrationPaths":["old"],"data":{"restore":true}});
    });
    std::fs::create_dir_all(temp.path().join("legacy")).unwrap();
    std::fs::write(temp.path().join("legacy/state.json"), b"{}").unwrap();
    let s = NativeSessions::default();
    s.start(&ctx).unwrap();
    let data = temp
        .path()
        .join("external-extensions/com.tezbar.native.perplexity");
    wait_file(&data.join("activated")).await;
    wait_file(&temp.path().join("extensions/startup-probes.json")).await;
    let activation: Value =
        serde_json::from_slice(&std::fs::read(data.join("activated")).unwrap()).unwrap();
    assert_eq!(activation["params"]["key"], "migrate");
    assert_eq!(activation["params"]["data"]["restore"], true);
    s.clear().await;
    let next = NativeSessions::default();
    next.start(&ctx).unwrap();
    tokio::task::yield_now().await;
    assert!(next.0.lock().unwrap().slots.is_empty());
    assert_eq!(
        std::fs::read_to_string(data.join("starts"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    // Explicit durable activation survives process shutdown and restores lazily.
    let s = NativeSessions::default();
    let v = launch(&s, &ctx).await;
    let p = s.0.lock().unwrap().sessions[v["sessionId"].as_str().unwrap()]
        .process
        .clone();
    p.host
        .request(
            &p,
            "host.background.retain",
            json!({"key":"work","durable":true,"data":{"running":true}}),
        )
        .await
        .unwrap();
    s.clear().await;
    std::fs::remove_file(data.join("activated")).unwrap();
    let next = NativeSessions::default();
    next.start(&ctx).unwrap();
    wait_file(&data.join("activated")).await;
    next.clear().await;
    let activation: Value =
        serde_json::from_slice(&std::fs::read(data.join("activated")).unwrap()).unwrap();
    assert_eq!(activation["params"]["key"], "work");
    process_install::test_install(&ctx, "background-delay");
    std::fs::remove_file(data.join("activated")).unwrap();
    let s = NativeSessions::default();
    s.start(&ctx).unwrap();
    wait_file(&data.join("activated")).await;
    tokio::time::timeout(Duration::from_secs(8), s.clear())
        .await
        .unwrap();
    assert!(s
        .0
        .lock()
        .unwrap()
        .slots
        .values()
        .all(|slot| slot.process.try_lock().is_ok_and(|p| p.is_none())));
}
#[tokio::test]
async fn canonical_scalar_metadata_and_unknown_install_do_not_reach_worker() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    let root = temp
        .path()
        .join("extensions/packages/com.tezbar.native.perplexity");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("package.json"), b"\"not an object\"").unwrap();
    let list = registry::handle(&ctx, "extension:list", &[]).unwrap();
    assert_eq!(list[0]["id"], "com.tezbar.native.perplexity");
    assert_eq!(list[0]["commands"], json!([]));
    let core = super::super::ExtensionsCore::new(ctx);
    let result = core
        .handle("extension:install", &[json!("com.tezbar.native.unknown")])
        .await
        .unwrap();
    assert!(result.is_err());
    assert!(!core.worker_running().await);
}

#[tokio::test]
async fn owned_revision_events_and_unknown_response_fence() {
    let temp = tempfile::tempdir().unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = events.clone();
    let ctx = context(temp.path()).with_event_sink(Arc::new(move |channel, payload| {
        if channel == "extension:session-invalidated" {
            captured.lock().unwrap().push(payload);
        }
    }));
    process_install::test_install(&ctx, "events");
    let s = NativeSessions::default();
    let v = launch(&s, &ctx).await;
    let id = v["sessionId"].as_str().unwrap();
    assert_eq!(
        run(
            &s,
            &ctx,
            "extension:refresh-session",
            json!({"sessionId":id})
        )
        .await["mode"],
        "unchanged"
    );
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["revision"], 1);
    assert_eq!(events[1]["revision"], 2);
    assert_eq!(events[0]["generation"], v["generation"]);
    drop(events);
    s.clear().await;
    process_install::test_install(&ctx, "unknownresponse");
    let v = launch(&s, &ctx).await;
    assert_eq!(
        run(
            &s,
            &ctx,
            "extension:refresh-session",
            json!({"sessionId":v["sessionId"]})
        )
        .await["ok"],
        false
    );
    s.clear().await;
}
#[tokio::test]
async fn shutdown_aborts_reverse_host_future_without_deadlock() {
    struct Dropped(Arc<AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    let entered = Arc::new(tokio::sync::Notify::new());
    let notified = entered.clone();
    let dropped = Arc::new(AtomicBool::new(false));
    let capture = dropped.clone();
    ctx.set_router(Arc::new(move |_, _| {
        let entered = notified.clone();
        let dropped = capture.clone();
        Box::pin(async move {
            let _guard = Dropped(dropped);
            entered.notify_one();
            std::future::pending().await
        })
    }))
    .unwrap();
    process_install::test_install(&ctx, "reverse");
    let s = NativeSessions::default();
    let pending = {
        let s = s.clone();
        let ctx = ctx.clone();
        tokio::spawn(async move { launch(&s, &ctx).await })
    };
    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(8), s.clear())
        .await
        .unwrap();
    assert_eq!(pending.await.unwrap()["ok"], false);
    assert!(dropped.load(Ordering::Acquire));
}

#[tokio::test]
async fn unused_process_stops_after_idle_grace_with_graceful_shutdown() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    process_install::test_install(&ctx, "normal");
    let p = Process::spawn(
        &ctx,
        process_install::resolve(&ctx, "com.tezbar.native.perplexity").unwrap(),
        "idle-test",
    )
    .await
    .unwrap();
    let shutdown = p.host.paths().unwrap().0.join("shutdown");
    let started = std::time::Instant::now();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(p.alive());
    tokio::time::timeout(Duration::from_secs(21), async {
        while p.alive() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    assert!(started.elapsed() >= Duration::from_secs(14));
    wait_file(&shutdown).await;
    p.stop_and_wait().await;
}

#[tokio::test]
async fn typed_services_preserve_core_argument_and_image_shapes() {
    let temp = tempfile::tempdir().unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let captured = calls.clone();
    let ctx = context(temp.path());
    ctx.set_router(Arc::new(move |channel, args| {
        captured.lock().unwrap().push((channel.clone(), args));
        Box::pin(async move {
            if channel == "core:platform:screenshot" {
                Ok(json!({"data":"iVBORw0KGgo=","mimeType":"image/png"}))
            } else {
                Ok(json!({"ok":true}))
            }
        })
    }))
    .unwrap();
    process_install::test_install_with(&ctx, "normal", |m| {
        m["capabilities"] = json!(["ai", "safety", "screen", "clipboard", "settings"])
    });
    let s = NativeSessions::default();
    let v = launch(&s, &ctx).await;
    let p = s.0.lock().unwrap().sessions[v["sessionId"].as_str().unwrap()]
        .process
        .clone();
    p.host
        .request(
            &p,
            "host.ai.ask",
            json!({"prompt":"colors","options":{"stream":false}}),
        )
        .await
        .unwrap();
    p.host
        .request(
            &p,
            "host.safety.authorize",
            json!({"operation":"terminate-process","context":{"pid":123,"extensionId":"spoofed"}}),
        )
        .await
        .unwrap();
    p.host
        .request(
            &p,
            "host.safety.record",
            json!({"operation":"terminate-process","context":{"pid":123},"result":{"dryRun":true}}),
        )
        .await
        .unwrap();
    let capture = p
        .host
        .request(&p, "host.screen.capture", json!({"maxWidth":4096}))
        .await
        .unwrap();
    assert_eq!(capture["mimeType"], "image/png");
    assert!(capture["data"].is_string());
    p.host
        .request(
            &p,
            "host.clipboard.writeImage",
            json!({"file":"/tmp/capture.png"}),
        )
        .await
        .unwrap();
    p.host
        .request(
            &p,
            "host.settings.open",
            json!({"surface":"settings-extensions"}),
        )
        .await
        .unwrap();
    assert!(p
        .host
        .request(&p, "host.clipboard.read", json!({"format":"unsupported"}))
        .await
        .is_err());
    let calls = calls.lock().unwrap();
    assert_eq!(
        calls[0],
        (
            "core:ai:extension-ask".into(),
            vec![json!("colors"), json!({"stream":false})]
        )
    );
    assert_eq!(calls[1].1[0], "terminate-process");
    assert_eq!(calls[1].1[1]["extensionId"], "com.tezbar.native.perplexity");
    assert_eq!(calls[2].1[2]["dryRun"], true);
    assert_eq!(calls[3].1[0]["maxWidth"], 4096);
    assert_eq!(calls[4].1[0]["file"], "/tmp/capture.png");
    assert_eq!(calls[5].1[0], "settings-extensions");
    drop(calls);
    s.clear().await;
}

#[tokio::test]
async fn legacy_uninstall_does_not_stop_canonical_sessions() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    process_install::test_install(&ctx, "normal");
    let core = super::super::ExtensionsCore::new(ctx.clone());
    let view = launch(&core.native, &ctx).await;
    let id = view["sessionId"].as_str().unwrap();
    let process = core.native.0.lock().unwrap().sessions[id].process.clone();
    let legacy = temp.path().join("extensions/packages/raycast.perplexity");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(legacy.join("package.json"), b"{}").unwrap();
    assert_eq!(
        core.handle("extension:uninstall", &[json!("raycast.perplexity")])
            .await
            .unwrap()
            .unwrap(),
        true
    );
    assert!(process.alive());
    assert!(core.native.0.lock().unwrap().sessions.contains_key(id));
    assert_eq!(
        run(
            &core.native,
            &ctx,
            "extension:refresh-session",
            json!({"sessionId":id})
        )
        .await["mode"],
        "unchanged"
    );
    core.shutdown().await;
}
