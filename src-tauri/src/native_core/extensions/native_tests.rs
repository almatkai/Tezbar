//! Public ExtensionsCore dispatch tests. Only temporary profiles and recording routers.
use super::*;
use crate::native_core::{
    common::{write_json_atomic, CorePaths},
    extensions::ExtensionsCore,
};
use std::sync::atomic::AtomicUsize;
struct Fixture {
    _dir: tempfile::TempDir,
    core: ExtensionsCore,
    package: std::path::PathBuf,
    opens: Arc<Mutex<Vec<Vec<Value>>>>,
    fallback: Arc<Mutex<Vec<(String, Vec<Value>)>>>,
    fail: Arc<AtomicBool>,
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
        let package = ctx.paths.data_dir.join("extensions/perplexity");
        write_json_atomic(&package.join("package.json"), &manifest()).unwrap();
        let opens = Arc::new(Mutex::new(Vec::new()));
        let record = opens.clone();
        let fail = Arc::new(AtomicBool::new(false));
        let fail_open = fail.clone();
        ctx.set_router(Arc::new(move |channel, args| {
            let record = record.clone();
            let fail = fail_open.clone();
            Box::pin(async move {
                if channel == "shell:open" {
                    record.lock().unwrap().push(args);
                    if fail.load(Ordering::Acquire) {
                        return Err("fake open failed".into());
                    }
                } else {
                    assert_eq!(channel, "core:launcher:reindex-extensions");
                }
                Ok(Value::Null)
            })
        }))
        .unwrap();
        let fallback = Arc::new(Mutex::new(Vec::new()));
        let worker_record = fallback.clone();
        let mut core = ExtensionsCore::new(ctx);
        core.worker_override = Some(Arc::new(move |channel, args| {
            worker_record.lock().unwrap().push((channel, args));
            Box::pin(async { Ok(json!({"worker":true})) })
        }));
        Self {
            _dir: dir,
            core,
            package,
            opens,
            fallback,
            fail,
        }
    }
    async fn call(&self, channel: &str, body: Value) -> Value {
        self.core.handle(channel, &[body]).await.unwrap().unwrap()
    }
    async fn launch(&self, arguments: Option<Value>) -> Value {
        let mut body =
            json!({"extensionId":perplexity::EXTENSION,"commandName":perplexity::COMMAND});
        if let Some(arguments) = arguments {
            body["argumentValues"] = arguments;
        }
        self.call("extension:run-command", body).await
    }
    async fn form(&self) -> Value {
        self.launch(None).await
    }
    async fn action(&self, id: &Value, query: Value) -> Value {
        self.call(
            "extension:invoke-action",
            json!({"sessionId":id,"actionId":perplexity::SUBMIT,"formValues":{"query":query}}),
        )
        .await
    }
    fn assert_no_worker(&self) {
        assert!(self.fallback.lock().unwrap().is_empty());
    }
}
fn manifest() -> Value {
    json!({"name":"perplexity","title":"Perplexity","platforms":["macOS","Windows","Linux"],
        "commands":[{"name":"ask-perplexity","mode":"view","title":"Ask Perplexity",
            "arguments":[{"name":"query","type":"text"}],
            "preferences":[{"name":"perplexityApp","type":"checkbox","default":false}]}]})
}
#[tokio::test]
async fn native_perplexity_form_validation_and_malformed_requests() {
    let f = Fixture::new();
    assert!(!f.core.worker_running().await);
    for arguments in [None, Some(json!({})), Some(json!({"query":""}))] {
        let form = f.launch(arguments).await;
        assert_eq!(form["mode"], "view");
        assert_eq!(form["title"], "Ask Perplexity");
        assert_eq!(form["root"]["type"], "Form");
        assert_eq!(form["root"]["children"].as_array().unwrap().len(), 1);
        assert_eq!(
            form["root"]["children"][0],
            json!({"type":"Form.TextArea","props":{"id":"query","title":"Ask Anything"}})
        );
        assert_eq!(form["actions"][0]["kind"], "submit-form");
        let id = &form["sessionId"];
        // ok:false prevents the renderer from showing a green 'Done' notice.
        assert_eq!(
            f.action(id, json!("")).await,
            json!({"ok":false,"message":"Query cannot be empty"})
        );
        for query in [Value::Null, json!(10), json!([]), json!({})] {
            assert_eq!(f.action(id, query).await["ok"], false);
        }
        for body in [
            json!({"sessionId":id}),
            json!({"sessionId":id,"actionId":"wrong"}),
            json!({"sessionId":id,"actionId":perplexity::SUBMIT,"formValues":[]}),
            json!({"sessionId":id,"actionId":perplexity::SUBMIT}),
        ] {
            assert_eq!(f.call("extension:invoke-action", body).await["ok"], false);
        }
        for channel in [
            "extension:search-text-changed",
            "extension:load-more",
            "extension:unknown",
        ] {
            assert_eq!(f.call(channel, json!({"sessionId":id})).await["ok"], false);
        }
    }
    for args in [
        Value::Null,
        json!([]),
        json!("query"),
        json!({"query":false}),
        json!({"query":1}),
        json!({"query":null}),
    ] {
        assert_eq!(f.launch(Some(args)).await["ok"], false);
    }
    assert!(f.opens.lock().unwrap().is_empty());
    f.assert_no_worker();
    assert!(!f.core.worker_running().await);
}
#[tokio::test]
async fn native_perplexity_urls_scoped_preferences_exact_encoding_and_open_failure() {
    let f = Fixture::new();
    let query = "Қазақ 日本語 😀\n +?&%~!*'()\r\n";
    let encoded = "%D2%9A%D0%B0%D0%B7%D0%B0%D2%9B+%E6%97%A5%E6%9C%AC%E8%AA%9E+%F0%9F%98%80%0A+%2B%3F%26%25%7E%21*%27%28%29%0D%0A";
    write_json_atomic(
        &f.package.join("preferences.json"),
        &json!({"commands":{"other":{"perplexityApp":true}}}),
    )
    .unwrap();
    let result = f.launch(Some(json!({"query":query}))).await;
    assert_eq!(result["mode"], "no-view");
    let web = format!("https://www.perplexity.ai/search?q={encoded}");
    assert_eq!(result["effects"][0]["value"], web);
    assert_eq!(*f.opens.lock().unwrap(), vec![vec![json!(web)]]);
    assert!(f.core.native.0.lock().unwrap().sessions.is_empty());
    let form = f.form().await;
    // Existing sessions retain the command's preference snapshot.
    f.call("preferences:set", json!({"extensionId":perplexity::EXTENSION,"commandName":perplexity::COMMAND,"values":{"perplexityApp":true}})).await;
    assert_eq!(
        f.action(&form["sessionId"], json!(" ")).await["mode"],
        "no-view"
    );
    assert_eq!(
        f.opens.lock().unwrap().last().unwrap(),
        &vec![json!("https://www.perplexity.ai/search?q=+")]
    );
    let app = f.form().await;
    assert_eq!(
        f.action(&app["sessionId"], json!(query)).await["effects"][0]["value"],
        format!("perplexity-app://search?q={encoded}")
    );
    assert_eq!(
        f.launch(Some(json!({"query":"a+b c"}))).await["effects"][0]["value"],
        "perplexity-app://search?q=a%2Bb+c"
    );
    f.fail.store(true, Ordering::Release);
    let before = f.opens.lock().unwrap().len();
    for result in [
        f.action(&app["sessionId"], json!("fail")).await,
        f.launch(Some(json!({"query":"fail"}))).await,
    ] {
        assert_eq!(result, json!({"ok":false,"message":"fake open failed"}));
        assert!(result.get("effects").is_none());
    }
    assert_eq!(f.opens.lock().unwrap().len(), before + 2);
    f.fail.store(false, Ordering::Release);
    assert_eq!(
        f.action(&app["sessionId"], json!("retry")).await["ok"],
        true
    );
    f.assert_no_worker();
    assert!(!f.core.worker_running().await);
}
#[tokio::test]
async fn native_perplexity_refresh_dispose_restart_shutdown_and_uninstall() {
    let f = Fixture::new();
    let a = f.form().await;
    let b = f.form().await;
    assert_ne!(a["sessionId"], b["sessionId"]);
    for _ in 0..3 {
        assert_eq!(
            f.call(
                "extension:refresh-session",
                json!({"sessionId":a["sessionId"]})
            )
            .await,
            json!({"ok":true,"mode":"unchanged"})
        );
    }
    for expected in [true, false] {
        assert_eq!(
            f.call(
                "extension:dispose-session",
                json!({"sessionId":a["sessionId"]})
            )
            .await,
            expected
        );
    }
    assert_eq!(f.action(&a["sessionId"], json!("stale")).await["ok"], false);
    for _ in 0..2 {
        assert_eq!(f.action(&b["sessionId"], json!("again")).await["ok"], true);
    }
    f.core.restart_worker().await;
    assert_eq!(f.action(&b["sessionId"], json!("stale")).await["ok"], false);
    let c = f.form().await;
    f.core.shutdown().await;
    assert_eq!(
        f.call(
            "extension:refresh-session",
            json!({"sessionId":c["sessionId"]})
        )
        .await["ok"],
        false
    );
    let d = f.form().await;
    assert_eq!(
        f.call("extension:uninstall", json!(" raycast.perplexity "))
            .await,
        true
    );
    assert_eq!(f.action(&d["sessionId"], json!("stale")).await["ok"], false);
    assert_eq!(
        f.call(
            "extension:dispose-session",
            json!({"sessionId":d["sessionId"]})
        )
        .await,
        false
    );
    assert_eq!(f.opens.lock().unwrap().len(), 2);
    f.assert_no_worker();
}
#[tokio::test]
async fn native_perplexity_manifest_validation_fallback_and_cheap_heartbeat() {
    let f = Fixture::new();
    let form = f.form().await;
    std::fs::remove_file(f.package.join("package.json")).unwrap();
    assert_eq!(
        f.call(
            "extension:refresh-session",
            json!({"sessionId":form["sessionId"]})
        )
        .await,
        json!({"ok":true,"mode":"unchanged"})
    );
    assert_eq!(
        f.action(&form["sessionId"], json!("removed")).await["ok"],
        false
    );
    assert_eq!(f.launch(None).await, json!({"worker":true}));
    let mut package = manifest();
    package["commands"] = json!([{"name":"another"}]);
    write_json_atomic(&f.package.join("package.json"), &package).unwrap();
    assert_eq!(
        f.launch(Some(json!({"query":"missing command"}))).await,
        json!({"worker":true})
    );
    for (extension, command) in [
        ("raycast.other", perplexity::COMMAND),
        (perplexity::EXTENSION, "other"),
    ] {
        assert_eq!(
            f.call(
                "extension:run-command",
                json!({"extensionId":extension,"commandName":command})
            )
            .await,
            json!({"worker":true})
        );
    }
    assert_eq!(f.fallback.lock().unwrap().len(), 4);
    assert!(f.opens.lock().unwrap().is_empty());
    write_json_atomic(&f.package.join("package.json"), &manifest()).unwrap();
    std::fs::write(f.package.join("preferences.json"), "{broken").unwrap();
    assert_eq!(f.launch(None).await["ok"], false);
    assert_eq!(f.fallback.lock().unwrap().len(), 4);
}
#[tokio::test]
async fn native_perplexity_capacity_rejects_without_evicting_active_sessions() {
    let f = Fixture::new();
    let first = f.form().await;
    for _ in 1..MAX_SESSIONS {
        assert_eq!(f.form().await["mode"], "view");
    }
    assert_eq!(f.form().await["ok"], false);
    assert_eq!(
        f.launch(Some(json!({"query":"immediate"}))).await["ok"],
        true
    );
    assert_eq!(
        f.action(&first["sessionId"], json!("still alive")).await["ok"],
        true
    );
    f.call(
        "extension:dispose-session",
        json!({"sessionId":first["sessionId"]}),
    )
    .await;
    assert_eq!(f.form().await["mode"], "view");
    assert_eq!(
        f.action(&first["sessionId"], json!("stale")).await["ok"],
        false
    );
    f.assert_no_worker();
}
#[tokio::test]
async fn native_perplexity_concurrent_submission_and_cancelled_immediate_open() {
    let mut f = Fixture::new();
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let count = Arc::new(AtomicUsize::new(0));
    let ctx = CoreContext::headless(f.core.ctx.paths.clone());
    let (started, released, opens) = (entered.clone(), release.clone(), count.clone());
    ctx.set_router(Arc::new(move |channel, _args| {
        assert_eq!(channel, "shell:open");
        let (started, released, opens) = (started.clone(), released.clone(), opens.clone());
        Box::pin(async move {
            opens.fetch_add(1, Ordering::SeqCst);
            started.notify_one();
            released.notified().await;
            Ok(Value::Null)
        })
    }))
    .unwrap();
    f.core.ctx = ctx;
    let form = f.form().await;
    let core = f.core.clone();
    let body = json!({"sessionId":form["sessionId"],"actionId":perplexity::SUBMIT,"formValues":{"query":"first"}});
    let pending =
        tokio::spawn(async move { core.handle("extension:invoke-action", &[body]).await });
    entered.notified().await;
    assert_eq!(
        f.action(&form["sessionId"], json!("duplicate")).await["ok"],
        false
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    pending.abort();
    let _ = pending.await;
    let core = f.core.clone();
    let body = json!({"sessionId":form["sessionId"],"actionId":perplexity::SUBMIT,"formValues":{"query":"retry"}});
    let pending =
        tokio::spawn(async move { core.handle("extension:invoke-action", &[body]).await });
    entered.notified().await;
    release.notify_one();
    assert_eq!(pending.await.unwrap().unwrap().unwrap()["ok"], true);
    let visible = f.core.native.0.lock().unwrap().sessions.len();
    let core = f.core.clone();
    let pending = tokio::spawn(async move {
        core.handle("extension:run-command", &[json!({"extensionId":perplexity::EXTENSION,"commandName":perplexity::COMMAND,"argumentValues":{"query":"immediate"}})]).await
    });
    entered.notified().await;
    pending.abort();
    let _ = pending.await;
    assert_eq!(f.core.native.0.lock().unwrap().sessions.len(), visible);
    f.core.shutdown().await;
    assert_eq!(
        f.action(&form["sessionId"], json!("stale")).await["ok"],
        false
    );
    f.assert_no_worker();
}
