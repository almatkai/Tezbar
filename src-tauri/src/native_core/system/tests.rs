use super::super::common::{CorePaths, CoreRouter};
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
fn context(root: &std::path::Path) -> CoreContext {
    CoreContext::headless(CorePaths {
        data_dir: root.join("data"),
        config_dir: root.join("config"),
        resource_dir: root.join("resources"),
        temp_dir: root.join("tmp"),
        home_dir: root.join("home"),
    })
}
#[test]
fn catalogs_match_all_ids() {
    let list = commands::descriptors();
    let ids: std::collections::HashSet<_> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 43);
    for id in [
        "brew-update",
        "toggle-bluetooth",
        "generate-password",
        "show-system-monitor",
    ] {
        assert!(ids.contains(id));
    }
}
#[test]
fn currency_validates_both_schemas() {
    assert_eq!(
        currency_payload(
            "USD",
            &json!({"result":"success","time_last_update_utc":"date","rates":{"EUR":0.9}}),
            true
        )
        .unwrap()["date"],
        "date"
    );
    assert_eq!(
        currency_payload("EUR", &json!({"date":"today","rates":{"USD":1.1}}), false).unwrap()
            ["base"],
        "EUR"
    );
    assert!(currency_payload("USD", &json!({"result":"error","rates":{"EUR":1}}), true).is_err());
    assert!(currency_payload("USD", &json!({"rates":{"EUR":0}}), false).is_err());
}
#[test]
fn timer_ignores_paused_and_invalid() {
    let file = "2026-10-21T12__00__00.000Z---60.timer";
    assert!(background::timer(file, r#"{"name":"Paused"}"#, 0).is_none());
    let task = background::timer(
        file,
        r#"{"name":" Tea ","pid":123,"pauseElapsed":10}"#,
        1792584000000,
    )
    .unwrap();
    assert_eq!(task["title"], "Tea");
    assert_eq!(task["remainingSeconds"], 70);
    assert!(background::timer("bad---60.timer", r#"{"pid":1}"#, 0).is_none());
}
#[test]
fn unavailable_stats_are_null() {
    let top = stats::parse_top("");
    assert!(top["cpu"]["usagePercent"].is_null());
    assert!(top["memory"]["freeBytes"].is_null());
    assert!(stats::cpu_between([1., 2., 3.], [1., 2., 3.]).is_none());
    assert!(stats::cpu_between([2., 3., 4.], [1., 4., 5.]).is_none());
    assert_eq!(
        stats::cpu_between([0., 0., 0.], [20., 10., 70.]),
        Some([20., 10., 70.])
    );
}
#[test]
fn top_parses_last_delta_sample() {
    let v=stats::parse_top("CPU usage: 10.0% user, 20.0% sys, 70.0% idle\nCPU usage: 5.0% user, 3.0% sys, 92.0% idle\nLoad Avg: 1.2, 2.3, 3.4\nProcesses: 600 total, 3 running\nPhysMem: 12G used (1G wired), 400M unused");
    assert_eq!(v["cpu"]["usagePercent"], 8.0);
    assert_eq!(v["processes"]["total"], 600.0);
    assert_eq!(v["memory"]["freeBytes"], 419430400.0);
}
#[test]
fn audit_redacts_before_truncation() {
    // Generate a synthetic credential in memory; never store key-shaped values.
    let fake_api_key = format!("sk-{}", "x".repeat(24));
    let s = safety::redact(&format!(
        "password=abc {fake_api_key} ghp_abcdefghijklmnop Bearer abcdefghijklmnop1234"
    ));
    assert!(!s.contains(&fake_api_key));
    assert!(!s.contains("abcdefghijklmnop"));
    assert!(!s.contains("abc "));
}

#[tokio::test]
async fn cancellation_and_dry_run_do_not_execute_os_effects() {
    let root = tempfile::tempdir().unwrap();
    let ctx = context(root.path());
    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();
    let router: CoreRouter = Arc::new(move |channel, _| {
        let c = c.clone();
        Box::pin(async move {
            assert_eq!(channel, "core:platform:confirm");
            c.fetch_add(1, Ordering::Relaxed);
            Ok(json!({"response":0}))
        })
    });
    ctx.set_router(router).unwrap();
    let core = SystemCore::new(ctx.clone());
    let r = core
        .handle("core:native:execute", &[json!({"commandId":"empty-trash"})])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r["message"], "Cancelled");
    assert_eq!(count.load(Ordering::Relaxed), 1);
    assert_eq!(
        core.audit_log(false)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    ctx.config
        .patch(&json!({"futureKey":42,"safetyDryRun":true}))
        .unwrap();
    assert_eq!(core.dry_run().await.unwrap(), true);
    assert_eq!(ctx.config.read().unwrap()["futureKey"], 42);
}
#[tokio::test]
async fn durable_audit_bounded_and_password_redacted() {
    let root = tempfile::tempdir().unwrap();
    let ctx = context(root.path());
    let core = SystemCore::new(ctx.clone());
    for _ in 0..205 {
        core.record(
            "native.command",
            json!({"commandId":"generate-password","apiKey":"plainSecret"}),
            result(true, "UNRECOGNIZABLE_PASSWORD"),
        )
        .await
        .unwrap();
    }
    let log = core.audit_log(false).await.unwrap();
    assert_eq!(log.as_array().unwrap().len(), 200);
    assert!(!log.to_string().contains("UNRECOGNIZABLE_PASSWORD"));
    assert!(!log.to_string().contains("plainSecret"));
    let reopened = SystemCore::new(ctx);
    assert_eq!(reopened.audit_log(false).await.unwrap(), log);
}
#[tokio::test]
async fn malformed_audit_is_preserved() {
    let root = tempfile::tempdir().unwrap();
    let ctx = context(root.path());
    let p = ctx.paths.data_dir.join("safety/action-log.json");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, "{broken").unwrap();
    let core = SystemCore::new(ctx);
    assert!(core
        .record("native.command", json!({}), result(true, "test"))
        .await
        .is_err());
    assert_eq!(std::fs::read_to_string(p).unwrap(), "{broken");
}
#[tokio::test]
async fn accepted_preview_never_dispatches_effect_and_preserves_config() {
    let root = tempfile::tempdir().unwrap();
    let ctx = context(root.path());
    ctx.config
        .patch(&json!({"safetyDryRun":true,"futureKey":{"preserve":1}}))
        .unwrap();
    ctx.set_router(Arc::new(|channel, _| {
        Box::pin(async move {
            assert_eq!(channel, "core:platform:confirm");
            Ok(json!({"response":1}))
        })
    }))
    .unwrap();
    let core = SystemCore::new(ctx.clone());
    let r = core
        .handle(
            "core:native:execute",
            &[json!({"type":"run-shell","command":"definitely-no-such-command"})],
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r["ok"], true);
    assert!(r["message"].as_str().unwrap().starts_with("Dry run:"));
    core.handle("safety:dry-run:set", &[json!(false)])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ctx.config.read().unwrap()["futureKey"]["preserve"], 1);
    let log = core.audit_log(false).await.unwrap();
    assert_eq!(log[0]["context"]["dryRun"], true);
}
#[tokio::test]
async fn invalid_shell_and_unknown_commands_never_dispatch_effects() {
    let root = tempfile::tempdir().unwrap();
    let core = SystemCore::new(context(root.path()));
    let r = core
        .execute_native(&json!({"commandId":"nonexistent"}))
        .await
        .unwrap();
    assert_eq!(r["ok"], false);
    assert!(core.handle("unowned:channel", &[]).await.is_none());
    assert!(core
        .handle("currency:frankfurter-latest", &[json!("$(bad)")])
        .await
        .unwrap()
        .is_err());
    assert_eq!(
        core.execute_builtin("unknown", &Value::Null).await.unwrap()["ok"],
        false
    );
}
#[tokio::test]
async fn background_poll_only_uses_cached_knowledge_and_timer_files() {
    let root = tempfile::tempdir().unwrap();
    let ctx = context(root.path());
    ctx.set_router(Arc::new(|channel,args|Box::pin(async move{
        assert_eq!(channel,"core:knowledge:background");assert!(args.is_empty());
        Ok(json!({"status":{"state":"indexing","jobId":"job","progress":1.5},"settings":{},"semanticSearch":null}))
    }))).unwrap();
    let core = SystemCore::new(ctx);
    let tasks = core.background_tasks().await.unwrap();
    assert_eq!(tasks[0]["id"], "indexing:job");
    assert_eq!(tasks[0]["progress"], 1.0);
}
#[test]
fn generated_password_has_each_required_class() {
    for _ in 0..100 {
        let password = commands::password();
        assert_eq!(password.len(), 20);
        assert!(password.bytes().any(|b| b.is_ascii_uppercase()));
        assert!(password.bytes().any(|b| b.is_ascii_lowercase()));
        assert!(password.bytes().any(|b| b.is_ascii_digit()));
        assert!(password.bytes().any(|b| !b.is_ascii_alphanumeric()));
        assert!(
            !password.contains('0')
                && !password.contains('1')
                && !password.contains('I')
                && !password.contains('O')
                && !password.contains('l')
        );
    }
}

#[test]
fn missing_low_power_state_remains_unknown() {
    let p = json!({"SPPowerDataType":[{"_name":"sppower_information"}]});
    assert!(stats::mac_fixed(&p)["batteryHealth"]["lowPowerMode"].is_null());
    let p = json!({"SPPowerDataType":[{"_name":"sppower_information","Battery Power":{"LowPowerMode":"No"}}]});
    assert_eq!(stats::mac_fixed(&p)["batteryHealth"]["lowPowerMode"], false);
}
#[tokio::test]
async fn audit_redacts_nested_credentials_before_serializing_context() {
    let root = tempfile::tempdir().unwrap();
    let core = SystemCore::new(context(root.path()));
    core.record("native.command", json!({"details":{"account":"fixture","password":"short-private-value","children":[{"authorization":"private-authorization"}],"safe":true}}), result(true,"Completed")).await.unwrap();
    let entries = core.audit_log(false).await.unwrap();
    let stored = entries.to_string();
    assert!(!stored.contains("short-private-value"));
    assert!(!stored.contains("private-authorization"));
    assert!(stored.contains("fixture"));
    assert!(stored.contains("REDACTED"));
}
