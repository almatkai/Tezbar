//! Upstream adapter regressions adapted to the real beta15 host (no SDK/registry stubs).
//! Temporary profiles and recording routers only; never touches the OS clipboard.
use super::v2_latex_math_symbols::{acknowledged, handle};
use super::{v2_catalog, v2_runtime, ExtensionsCore};
use crate::native_core::common::{write_json_atomic, CoreContext, CorePaths};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

type Calls = Arc<Mutex<Vec<(String, Vec<Value>)>>>;

struct Fixture {
    dir: tempfile::TempDir,
    ctx: CoreContext,
    core: ExtensionsCore,
    calls: Calls,
    reply: Arc<Mutex<Result<Value, String>>>,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let ctx = CoreContext::headless(CorePaths {
            data_dir: dir.path().join("data"),
            config_dir: dir.path().join("config"),
            resource_dir: dir.path().join("resources"),
            temp_dir: dir.path().join("tmp"),
            home_dir: dir.path().into(),
        });
        v2_catalog::install(&ctx, "com.tezbar.native.latex-math-symbols")
            .unwrap()
            .unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let reply = Arc::new(Mutex::new(Ok(json!({"ok":true}))));
        let record = calls.clone();
        let response = reply.clone();
        ctx.set_router(Arc::new(move |ch, args| {
            let record = record.clone();
            let response = response.clone();
            Box::pin(async move {
                if ch == "core:launcher:reindex-extensions" {
                    assert!(args.is_empty());
                    return Ok(Value::Null);
                }
                assert_eq!(
                    ch, "clipboard:write",
                    "No real effects or alternative route allowed"
                );
                record.lock().unwrap().push((ch, args));
                response.lock().unwrap().clone()
            })
        }))
        .unwrap();
        let mut core = ExtensionsCore::new(ctx.clone());
        core.worker_override = Some(Arc::new(|_, _| {
            Box::pin(async { Ok(json!({"fallback":true})) })
        }));
        Self {
            dir,
            ctx,
            calls,
            reply,
            core,
        }
    }
    async fn call(&self, op: &str, body: Value) -> Value {
        self.core
            .handle(&format!("extension:{op}"), &[body])
            .await
            .unwrap()
            .unwrap()
    }
    async fn launch(&self) -> Value {
        self.call(
            "run-command",
            json!({"extensionId":"com.tezbar.native.latex-math-symbols","commandName":"index"}),
        )
        .await
    }
    async fn action(&self, view: &Value, id: Value) -> Value {
        self.call(
            "invoke-action",
            json!({"sessionId":view["sessionId"],"actionId":id}),
        )
        .await
    }
    fn count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}
fn first(view: &Value) -> Value {
    view["actions"][0]["id"].clone()
}
async fn direct(f: &Fixture, op: &str, body: Value, state: &mut Value) -> Result<Value, String> {
    handle(&f.ctx, "index", &format!("extension:{op}"), &body, state).await
}
#[test]
fn strict_ack_matrix() {
    for good in [
        json!({"ok":true}),
        json!({"ok":true,"error":null}),
        json!({"ok":true,"success":true,"message":"Copied"}),
    ] {
        assert!(acknowledged(&good));
    }
    for bad in [
        Value::Null,
        json!(true),
        json!(false),
        json!([]),
        json!("ok"),
        json!({}),
        json!({"success":true}),
        json!({"ok":false}),
        json!({"ok":1}),
        json!({"ok":true,"error":"private"}),
        json!({"ok":true,"error":false}),
        json!({"ok":true,"success":false}),
        json!({"ok":true,"success":null}),
        json!({"ok":true,"message":{}}),
    ] {
        assert!(!acknowledged(&bad), "{bad}");
    }
}
#[test]
fn registry_dispatch_catalog_are_consistent() {
    let manifest: Value = serde_json::from_str(include_str!(
        "../../../native-extensions/latex-math-symbols/manifest.json"
    ))
    .unwrap();
    for id in [
        "latex-math-symbols",
        "raycast.latex-math-symbols",
        "com.tezbar.native.latex-math-symbols",
    ] {
        assert_eq!(v2_runtime::slug(id), Some("latex-math-symbols"));
        assert!(v2_runtime::supports(id, "index"));
        assert!(!v2_runtime::supports(id, "other"));
    }
    assert_eq!(
        v2_runtime::canonical_id("latex-math-symbols"),
        manifest["id"]
    );
    assert_eq!(
        manifest["commands"].as_array().unwrap().len(),
        v2_runtime::commands("latex-math-symbols").len()
    );
    assert!(!v2_runtime::supports("com.tezbar.native.other", "index"));
    let bundled: Vec<Value> = serde_json::from_str(include_str!("v2_catalog.json")).unwrap();
    let entries: Vec<_> = bundled
        .iter()
        .filter(|v| v["slug"] == "latex-math-symbols")
        .collect();
    assert_eq!(entries, vec![&manifest]);
    let compiled = v2_catalog::manifest("latex-math-symbols").unwrap();
    assert_eq!(compiled["status"], "complete");
    assert_eq!(compiled["totalCommands"], 1);
    assert_eq!(compiled["commands"][0]["runtime"], "rust");
    assert_eq!(compiled["replaces"], json!(["raycast.latex-math-symbols"]));
    assert!(compiled["verification"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["state"] == "not-run"));
    let renderer_icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../src/renderer/public")
        .join(manifest["icon"].as_str().unwrap().trim_start_matches('/'));
    let icon = std::fs::read(renderer_icon).unwrap();
    assert_eq!(icon.len(), 7516);
    assert_eq!(
        format!("{:x}", Sha256::digest(&icon)),
        "e245364b6c13c1c70abf5a6e9255442477639e18526f857d086b5d611ca24191"
    );
    assert_eq!(
        icon,
        include_bytes!("../../../native-extensions/latex-math-symbols/assets/icon.png")
    );
    // Public assets ship via Vite, so compiled distributions carry every notice
    // without changing installer configuration or requiring a network request.
    let notices: [(&str, &[u8]); 4] = [
        (
            "NOTICE.txt",
            include_bytes!("../../../native-extensions/latex-math-symbols/NOTICE"),
        ),
        (
            "LICENSE-MIT-UPSTREAM.txt",
            include_bytes!("../../../native-extensions/latex-math-symbols/LICENSE-MIT-UPSTREAM"),
        ),
        (
            "LICENSE-APACHE-2.0.txt",
            include_bytes!("../../../native-extensions/latex-math-symbols/LICENSE-APACHE-2.0"),
        ),
        (
            "LICENSE-MIT-FORK.txt",
            include_bytes!("../../../native-extensions/latex-math-symbols/fixtures/own/LICENSE"),
        ),
    ];
    let public = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../src/renderer/public/extensions/licenses/latex-math-symbols");
    for (name, bytes) in notices {
        assert_eq!(std::fs::read(public.join(name)).unwrap(), bytes, "{name}");
    }
}
#[tokio::test]
async fn actual_native_sessions_search_copy_refresh_and_dispose() {
    let f = Fixture::new();
    let view = f.launch().await;
    assert_eq!(view["root"]["children"].as_array().unwrap().len(), 131);
    assert_eq!(view["root"]["props"]["__hasServerSearch"], true);
    let searched = f
        .call(
            "search-text-changed",
            json!({"sessionId":view["sessionId"],"searchText":"fraction"}),
        )
        .await;
    assert_eq!(
        searched["root"]["children"][0]["props"]["title"],
        "\\frac{}{}"
    );
    assert_eq!(f.action(&searched, first(&view)).await["ok"], false);
    assert_eq!(f.count(), 0);
    let copy = f.action(&searched, first(&searched)).await;
    assert_eq!(copy["message"], "Copied to Clipboard");
    assert_eq!(
        f.calls.lock().unwrap()[0],
        ("clipboard:write".into(), vec![json!("\\frac{}{}")])
    );
    assert_eq!(f.action(&copy, first(&searched)).await["ok"], false);
    for op in ["refresh-session", "load-more"] {
        assert_eq!(
            f.call(op, json!({"sessionId":copy["sessionId"]})).await,
            json!({"ok":true,"mode":"unchanged"})
        );
    }
    assert_eq!(
        f.call("dispose-session", json!({"sessionId":copy["sessionId"]}))
            .await,
        true
    );
    assert_eq!(f.action(&copy, first(&copy)).await["ok"], false);
    assert_eq!(f.count(), 1);
}
#[tokio::test]
async fn malformed_forged_and_cross_session_calls_do_not_consume_valid_bindings() {
    let f = Fixture::new();
    let a = f.launch().await;
    let b = f.launch().await;
    for id in [
        Value::Null,
        json!(17),
        json!("latex:forged:copy:0"),
        first(&b),
    ] {
        assert_eq!(f.action(&a, id).await["ok"], false);
    }
    for field in [json!([]), json!(null), json!(42)] {
        assert_eq!(
            f.call(
                "invoke-action",
                json!({"sessionId":a["sessionId"],"actionId":first(&a),"formValues":field})
            )
            .await["ok"],
            false
        );
    }
    let copied=f.call("invoke-action",json!({"sessionId":a["sessionId"],"actionId":first(&a),"content":"EVIL","url":"https://evil","formValues":{"title":"EVIL"}})).await;
    assert_eq!(copied["message"], "Copied to Clipboard");
    assert_eq!(f.calls.lock().unwrap()[0].1, vec![json!("\\to")]);
    assert_eq!(f.count(), 1);
}
#[tokio::test]
async fn failures_are_sanitized_never_success_and_retry_requires_visible_deliberate_action() {
    for reply in [
        Ok(Value::Null),
        Ok(json!(true)),
        Ok(json!({"ok":true,"success":false})),
        Ok(json!({"ok":true,"error":"/private/SECRET"})),
        Err("/private/SECRET".into()),
    ] {
        let f = Fixture::new();
        let view = f.launch().await;
        *f.reply.lock().unwrap() = reply;
        let failure = f.action(&view, first(&view)).await;
        assert_eq!(failure["ok"], false);
        assert!(!failure.to_string().contains("SECRET"));
        assert!(!failure.to_string().contains("Copied to Clipboard"));
        assert_eq!(f.count(), 1);
        let recovery = f.action(&view, first(&view)).await;
        assert_eq!(recovery["actions"][0]["title"], "Retry Copy to Clipboard");
        assert_eq!(f.count(), 1);
        *f.reply.lock().unwrap() = Ok(json!({"ok":true,"error":null}));
        let retry = f.action(&recovery, first(&recovery)).await;
        assert_eq!(retry["message"], "Copied to Clipboard");
        assert_eq!(f.count(), 2);
    }
}
#[tokio::test]
async fn staged_clone_ids_never_revive_and_refresh_does_not_mutate() {
    let f = Fixture::new();
    let mut state = json!({});
    let a = direct(&f, "run-command", json!({}), &mut state)
        .await
        .unwrap();
    let mut discarded = state.clone();
    let staged = direct(
        &f,
        "search-text-changed",
        json!({"searchText":"root"}),
        &mut discarded,
    )
    .await
    .unwrap();
    let current = direct(
        &f,
        "search-text-changed",
        json!({"searchText":"root"}),
        &mut state,
    )
    .await
    .unwrap();
    assert_ne!(first(&staged), first(&current));
    let before = state.clone();
    for op in ["refresh-session", "load-more"] {
        assert_eq!(
            direct(&f, op, json!({}), &mut state.clone()).await.unwrap()["mode"],
            "unchanged"
        );
        assert_eq!(state, before);
    }
    assert!(direct(
        &f,
        "invoke-action",
        json!({"actionId":first(&staged)}),
        &mut state
    )
    .await
    .is_err());
    assert!(direct(
        &f,
        "invoke-action",
        json!({"actionId":first(&a)}),
        &mut state
    )
    .await
    .is_err());
    direct(
        &f,
        "invoke-action",
        json!({"actionId":first(&current)}),
        &mut state,
    )
    .await
    .unwrap();
    assert_eq!(state["effect"]["stage"], "Acknowledged");
    assert_eq!(f.count(), 1);
}
#[tokio::test]
async fn input_errors_leave_state_and_source_has_no_storage_or_drafts() {
    let f = Fixture::new();
    let mut state = json!({});
    direct(
        &f,
        "run-command",
        json!({"preferences":{"unknown":"preserved externally"},"argumentValues":{}}),
        &mut state,
    )
    .await
    .unwrap();
    let before = state.clone();
    for payload in [
        json!({}),
        json!({"searchText":null}),
        json!({"searchText":42}),
        json!({"searchText":"x".repeat(4097)}),
        json!({"searchText":"q","preferences":[]}),
    ] {
        assert!(direct(&f, "search-text-changed", payload, &mut state)
            .await
            .is_err());
        assert_eq!(state, before);
    }
    for bad in [
        json!(null),
        json!({"preferences":null}),
        json!({"argumentValues":{"value":"injected"}}),
        json!({"argumentValues":[]}),
    ] {
        assert!(direct(&f, "run-command", bad, &mut state).await.is_err());
        assert_eq!(state, before);
    }
    assert!(!f.ctx.paths.data_dir.join("native-extensions-v2").exists());
    assert!(!f.ctx.paths.config_dir.exists());
    assert_eq!(std::fs::read_dir(f.dir.path()).unwrap().count(), 1);
}
#[tokio::test]
async fn cancellation_followed_by_recovery_does_not_automatically_copy_again() {
    let f = Fixture::new();
    let mut state = json!({});
    let view = direct(&f, "run-command", json!({}), &mut state)
        .await
        .unwrap();
    let ctx = CoreContext::headless(f.ctx.paths.clone());
    ctx.set_router(Arc::new(|_, _| Box::pin(std::future::pending())))
        .unwrap();
    let body = json!({"actionId":first(&view)});
    {
        let future = handle(&ctx, "index", "extension:invoke-action", &body, &mut state);
        tokio::pin!(future);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), &mut future)
                .await
                .is_err()
        );
    }
    assert_eq!(state["effect"]["stage"], "Pending");
    let recover = direct(&f, "invoke-action", body, &mut state).await.unwrap();
    assert_eq!(recover["actions"][0]["title"], "Retry Copy to Clipboard");
    assert_eq!(f.count(), 0);
    direct(
        &f,
        "invoke-action",
        json!({"actionId":first(&recover)}),
        &mut state,
    )
    .await
    .unwrap();
    assert_eq!(state["effect"]["stage"], "Acknowledged");
    assert_eq!(f.count(), 1);
}
#[tokio::test]
async fn native_sessions_serializes_concurrent_actions_and_clears_on_uninstall() {
    let f = Fixture::new();
    let view = f.launch().await;
    let (a, b) = tokio::join!(f.action(&view, first(&view)), f.action(&view, first(&view)));
    assert_ne!(a["ok"], b["ok"]);
    assert_eq!(f.count(), 1);
    assert_eq!(
        f.core
            .handle(
                "extension:uninstall",
                &[json!("com.tezbar.native.latex-math-symbols")]
            )
            .await
            .unwrap()
            .unwrap(),
        true
    );
    let good = if a["ok"] == true { a } else { b };
    assert_eq!(f.action(&good, first(&good)).await["ok"], false);
    assert_eq!(f.count(), 1);
    f.core.native.clear().await;
    f.core.native.clear_slug("latex-math-symbols").await;
}
#[tokio::test]
async fn stale_native_ids_capacity_and_unknown_commands_never_fall_through() {
    let f = Fixture::new();
    assert_eq!(
        f.call(
            "run-command",
            json!({"extensionId":"com.tezbar.native.latex-math-symbols","commandName":"bad"})
        )
        .await["ok"],
        false
    );
    assert_eq!(f.call("run-command",json!({"extensionId":"other","commandName":"index","sessionId":"native-extension:stale"})).await["ok"],false);
    assert_eq!(
        f.call(
            "run-command",
            json!({"extensionId":"raycast.unrelated","commandName":"index"})
        )
        .await["fallback"],
        true
    );
    let mut views = Vec::new();
    for _ in 0..128 {
        views.push(f.launch().await);
    }
    assert!(views.iter().all(|v| v["ok"] == true));
    assert_eq!(f.launch().await["ok"], false);
    assert_eq!(
        f.action(&views[0], first(&views[0])).await["message"],
        "Copied to Clipboard"
    );
    f.core.native.clear().await;
    assert_eq!(f.action(&views[1], first(&views[1])).await["ok"], false);
}

#[tokio::test]
async fn every_canonical_record_copies_exactly_through_the_production_adapter() {
    let f = Fixture::new();
    let mut state = json!({});
    let mut view = direct(&f, "run-command", json!({}), &mut state)
        .await
        .unwrap();
    for record in 0..131 {
        let id = view["actions"][record]["id"].clone();
        view = direct(
            &f,
            "invoke-action",
            json!({"actionId":id,"formValues":{"content":"forged"}}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(
            f.calls.lock().unwrap()[record].1,
            vec![json!(
                latex_math_symbols_native::symbols().unwrap()[record].command
            )]
        );
    }
    assert_eq!(f.count(), 131);
}

#[tokio::test]
async fn actual_sessions_refresh_during_an_effect_and_disposal_cancel_without_repeat() {
    let f = Fixture::new();
    let ctx = CoreContext::headless(f.ctx.paths.clone());
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = calls.clone();
    let started = Arc::new(tokio::sync::Notify::new());
    let signal = started.clone();
    ctx.set_router(Arc::new(move |channel, args| {
        assert_eq!(channel, "clipboard:write");
        assert_eq!(args, vec![json!("\\to")]);
        let counted = counted.clone();
        let signal = signal.clone();
        Box::pin(async move {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            signal.notify_one();
            std::future::pending().await
        })
    }))
    .unwrap();
    let sessions = ExtensionsCore::new(ctx);
    let view = sessions
        .handle(
            "extension:run-command",
            &[json!({"extensionId":"com.tezbar.native.latex-math-symbols","commandName":"index"})],
        )
        .await
        .unwrap()
        .unwrap();
    let body = json!({"sessionId":view["sessionId"],"actionId":first(&view)});
    let work = sessions.clone();
    let action = tokio::spawn(async move {
        work.handle("extension:invoke-action", &[body])
            .await
            .unwrap()
            .unwrap()
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), started.notified())
        .await
        .expect("recording clipboard route was not reached");
    assert_eq!(
        sessions
            .handle(
                "extension:refresh-session",
                &[json!({"sessionId":view["sessionId"]})]
            )
            .await
            .unwrap()
            .unwrap()["mode"],
        "unchanged"
    );
    assert_eq!(
        sessions
            .handle(
                "extension:dispose-session",
                &[json!({"sessionId":view["sessionId"]})]
            )
            .await
            .unwrap()
            .unwrap(),
        true
    );
    assert_eq!(action.await.unwrap()["ok"], false);
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn actual_sessions_concurrent_searches_commit_only_latest_visible_capabilities() {
    let f = Fixture::new();
    let view = f.launch().await;
    let a = json!({"sessionId":view["sessionId"],"searchText":"sqrt"});
    let b = json!({"sessionId":view["sessionId"],"searchText":"fraction"});
    let (older, latest) = tokio::join!(
        f.call("search-text-changed", a),
        f.call("search-text-changed", b)
    );
    assert_eq!(latest["root"]["props"]["searchText"], "fraction");
    if older["ok"] == true {
        assert_eq!(f.action(&latest, first(&older)).await["ok"], false);
    }
    assert_eq!(
        f.action(&latest, first(&latest)).await["message"],
        "Copied to Clipboard"
    );
    assert_eq!(f.calls.lock().unwrap()[0].1, vec![json!("\\frac{}{}")]);
}

#[tokio::test]
async fn latex_cli_install_aliases_and_v1_package_preservation() {
    let f = Fixture::new();
    let legacy = f.ctx.paths.data_dir.join("extensions/latex-math-symbols");
    let legacy_manifest = json!({"name":"latex-math-symbols","platforms":["macOS","Windows"],
        "commands":[{"name":"index","title":"Search LateX Symbols","mode":"view"},
                    {"name":"legacy-only","mode":"view"}]});
    write_json_atomic(&legacy.join("package.json"), &legacy_manifest).unwrap();
    std::fs::write(legacy.join("index.js"), b"// unchanged V1 source").unwrap();
    let before = std::fs::read(legacy.join("package.json")).unwrap();
    let cli = super::v2_cli_list(&f.ctx).unwrap();
    let item = cli
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["slug"] == "latex-math-symbols")
        .unwrap();
    assert_eq!(item["installed"], true);
    assert_eq!(item["commands"][0]["runtime"], "rust");
    // Real catalog installer is idempotent and never overwrites the legacy package.
    super::v2_cli_install(&f.ctx, "com.tezbar.native.latex-math-symbols").unwrap();
    for id in [
        "com.tezbar.native.latex-math-symbols",
        "raycast.latex-math-symbols",
        "latex-math-symbols",
    ] {
        let view = f
            .call(
                "run-command",
                json!({"extensionId":id,"commandName":"index"}),
            )
            .await;
        assert_eq!(view["mode"], "view", "{id}: {view}");
        assert!(view["sessionId"]
            .as_str()
            .unwrap()
            .starts_with("native-extension:"));
        f.call("dispose-session", json!({"sessionId":view["sessionId"]}))
            .await;
    }
    // Unsupported V1 commands still reach the compatibility worker.
    assert_eq!(
        f.call(
            "run-command",
            json!({"extensionId":"raycast.latex-math-symbols","commandName":"legacy-only"})
        )
        .await["fallback"],
        true
    );
    assert_eq!(
        f.core
            .handle(
                "extension:uninstall",
                &[json!("com.tezbar.native.latex-math-symbols")]
            )
            .await
            .unwrap()
            .unwrap(),
        true
    );
    assert_eq!(std::fs::read(legacy.join("package.json")).unwrap(), before);
    assert_eq!(
        std::fs::read(legacy.join("index.js")).unwrap(),
        b"// unchanged V1 source"
    );
    // Without an installed command the Raycast alias falls through, canonical IDs fail closed.
    std::fs::remove_dir_all(&legacy).unwrap();
    assert_eq!(
        f.call(
            "run-command",
            json!({"extensionId":"raycast.latex-math-symbols","commandName":"index"})
        )
        .await["fallback"],
        true
    );
    assert_eq!(f.launch().await["ok"], false);
    assert!(!f.core.worker_running().await);
    assert_eq!(f.count(), 0);
}
