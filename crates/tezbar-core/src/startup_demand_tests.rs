//! Full routed core, default startup options, isolated profile and explicit TestHost.
//! Catalog data is fresh/on disk so these tests never need a network request.
use crate::{
    common::{now_ms, write_json_atomic, CoreContext, CorePaths},
    extension_worker::WorkerLaunch,
    host::TestHost,
    NativeCore,
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

type Events = Arc<Mutex<Vec<(String, Value)>>>;
const EXTENSION: &str = "raycast.demand-fixture";

fn fixture() -> (tempfile::TempDir, CoreContext, Events) {
    let dir = tempfile::tempdir().unwrap();
    let paths = CorePaths {
        data_dir: dir.path().join("data"),
        config_dir: dir.path().join("config"),
        resource_dir: dir.path().join("resources"),
        temp_dir: dir.path().join("temp"),
        home_dir: dir.path().join("home"),
    };
    write_json_atomic(
        &paths.data_dir.join("extension-catalog.json"),
        &json!({"version":6,"fetchedAt":now_ms(),"entries":[{
            "name":"demand-fixture","title":"Demand Fixture","commands":[]
        }]}),
    )
    .unwrap();
    let extension = paths.data_dir.join("extensions/demand-fixture");
    write_json_atomic(
        &extension.join("package.json"),
        &json!({
            "name":"demand-fixture","title":"Demand Fixture",
            "platforms":["macOS","Windows","Linux"],
            "preferences":[{"name":"style","type":"textfield","required":false,"default":"native"}],
            "commands":[{"name":"view","title":"View","mode":"view"}]
        }),
    )
    .unwrap();
    std::fs::create_dir_all(extension.join(".sc-build")).unwrap();
    std::fs::write(extension.join(".sc-build/view.js"), r#"
        const React = require('react');
        const { Detail } = require('@raycast/api');
        exports.default = () => React.createElement(Detail, { markdown: 'worker pid:' + process.pid });
    "#).unwrap();
    let events: Events = Arc::default();
    let log = events.clone();
    let host = Arc::new(TestHost {
        allow_interaction: true,
        ..Default::default()
    });
    let ctx =
        CoreContext::new(paths, Some(host)).with_event_sink(Arc::new(move |channel, value| {
            log.lock().unwrap().push((channel.to_owned(), value));
        }));
    (dir, ctx, events)
}

// This is a launch-attempt spy, not an is_running assertion: launch() emits
// starting BEFORE runtime discovery or spawn, including failed launch attempts.
fn attempts(events: &Events) -> usize {
    events
        .lock()
        .unwrap()
        .iter()
        .filter(|(channel, value)| {
            channel == "extension-worker-status" && value["phase"] == "starting"
        })
        .count()
}

async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(30), future)
        .await
        .expect("startup demand test timed out")
}

async fn common_requests(core: &NativeCore, events: &Events) {
    for (channel, args) in [
        ("health:ping", vec![]),
        ("get-settings", vec![]),
        ("search:all", vec![json!("demand")]),
        ("search:candidates", vec![]),
        ("extension:list", vec![]),
        ("get-extensions", vec![]),
        ("extensions:listInstalled", vec![]),
        ("get-installed-extensions-settings-schema", vec![]),
        ("extensions:integrity", vec![json!(EXTENSION)]),
        ("extensions:install-error", vec![json!(EXTENSION)]),
        ("extension:search-store", vec![json!({"query":"demand"})]),
        ("extensions:searchStore", vec![json!("demand")]),
        (
            "preferences:get",
            vec![json!({"extensionId":EXTENSION,"commandName":"view"})],
        ),
        (
            "preferences:setup",
            vec![json!({"extensionId":EXTENSION,"commandName":"view"})],
        ),
        (
            "preferences:set",
            vec![json!({"extensionId":EXTENSION,"values":{"style":"native"}})],
        ),
        ("clipboard:list", vec![]),
        ("notes:list", vec![]),
        ("snippets:list", vec![]),
        ("terminal:list", vec![]),
        ("native-commands:list", vec![]),
        ("safety:descriptors", vec![]),
        ("background-tasks:list", vec![]),
        ("knowledge:snapshot", vec![]),
    ] {
        let result = bounded(core.request(channel, &args)).await;
        assert_eq!(
            attempts(events),
            0,
            "{channel} attempted a JS runtime launch: {result:?}"
        );
        assert!(result.is_ok(), "{channel}: {result:?}");
    }
    // Built-in shortcut/surface opening still resolves extension metadata in Rust.
    for command in ["open-settings", "open-extensions", "open-notes"] {
        bounded(core.execute_command_shortcut(command))
            .await
            .unwrap();
        assert_eq!(attempts(events), 0, "built-in {command} activated JS");
    }
}

async fn stale_controls(core: &NativeCore, events: &Events, expected_attempts: usize) {
    for session in ["stale-js-session", "native-extension:stale", ""] {
        for channel in [
            "extension:refresh-session",
            "extension:invoke-action",
            "extension:search-text-changed",
            "extension:load-more",
            "extension:dispose-session",
        ] {
            let result = bounded(core.request(
                channel,
                &[json!({
                    "sessionId":session,"actionId":"stale","searchText":"ignored"
                })],
            ))
            .await;
            assert_eq!(
                attempts(events),
                expected_attempts,
                "stale {channel}: {result:?}"
            );
            if channel == "extension:dispose-session" {
                assert_eq!(result.unwrap(), false);
            } else {
                assert!(result.is_err() || result.unwrap()["ok"] == false);
            }
        }
    }
    // Status refresh uses an internal route, not renderer IPC.
    assert!(bounded(core.ctx.request(
        "core:extension-status-refresh",
        vec![json!({"sessionId":"stale-js-session"})]
    ))
    .await
    .is_err());
    assert_eq!(attempts(events), expected_attempts);
}

#[tokio::test]
async fn normal_startup_and_builtin_requests_never_attempt_js_launch() {
    let (dir, ctx, events) = fixture();
    // Explicit non-executable fixture prevents runtime discovery/download even
    // on regression. The event spy detects an attempt even when spawn fails.
    let bundle = dir.path().join("fake-worker");
    std::fs::create_dir(&bundle).unwrap();
    std::fs::write(bundle.join("extension-worker.js"), b"not a real bundle").unwrap();
    let executable = dir.path().join("not-an-executable");
    std::fs::write(&executable, b"launch must not be attempted").unwrap();
    let launch = WorkerLaunch::new(executable, bundle).unwrap();
    let core = NativeCore::new_with_extension_worker(ctx, launch).unwrap();
    // Deliberately NOT ISOLATED_PREVIEW: restore Rust background owners and start
    // Knowledge/shortcuts normally; only desktop side effects are mocked.
    bounded(core.start()).await.unwrap();
    assert_eq!(attempts(&events), 0, "startup attempted a JS launch");
    common_requests(&core, &events).await;
    stale_controls(&core, &events, 0).await;
    bounded(core.shutdown()).await;
    assert_eq!(attempts(&events), 0, "shutdown attempted a JS launch");
}

#[tokio::test]
#[ignore = "requires existing Bun and built dist-backend; no downloads; TEZBAR_TEST_BUN/TEZBAR_TEST_RESOURCES"]
async fn first_js_use_starts_one_worker_and_live_sessions_reuse_it() {
    let (_dir, ctx, events) = fixture();
    let bun = std::env::var_os("TEZBAR_TEST_BUN")
        .map(PathBuf::from)
        .or_else(|| {
            let name = if cfg!(windows) { "bun.exe" } else { "bun" };
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .map(|path| path.join(name))
                .find(|path| path.is_file())
        })
        .expect("existing Bun required; no download attempted");
    let resources = std::env::var_os("TEZBAR_TEST_RESOURCES")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let launch = WorkerLaunch::new(bun, resources.join("dist-backend")).unwrap();
    let core = NativeCore::new_with_extension_worker(ctx, launch).unwrap();
    bounded(core.start()).await.unwrap();
    common_requests(&core, &events).await;
    stale_controls(&core, &events, 0).await;
    let run = [json!({"extensionId":EXTENSION,"commandName":"view"})];
    let reply = bounded(core.request("extension:run-command", &run))
        .await
        .unwrap();
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["mode"], "view", "{reply}");
    assert!(reply["root"].to_string().contains("worker pid:"), "{reply}");
    let first_root = reply["root"].clone();
    let session = reply["sessionId"].as_str().unwrap();
    assert_eq!(attempts(&events), 1);
    assert!(core.extensions.worker_running().await);
    for channel in [
        "extension:refresh-session",
        "extension:search-text-changed",
        "extension:load-more",
    ] {
        let reply = bounded(core.request(
            channel,
            &[json!({"sessionId":session,"searchText":"native"})],
        ))
        .await
        .unwrap();
        assert_eq!(reply["ok"], true, "{channel}: {reply}");
        assert_eq!(attempts(&events), 1);
    }
    stale_controls(&core, &events, 1).await;
    assert_eq!(
        bounded(core.request("extension:dispose-session", &[json!({"sessionId":session})]))
            .await
            .unwrap(),
        true
    );
    assert!(
        bounded(core.request("extension:refresh-session", &[json!({"sessionId":session})]))
            .await
            .is_err()
    );
    // No idle shutdown policy: explicit future runs reuse the same live worker.
    let reply = bounded(core.request("extension:run-command", &run))
        .await
        .unwrap();
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(
        reply["root"], first_root,
        "reused worker must retain its PID"
    );
    assert_eq!(attempts(&events), 1);
    bounded(core.shutdown()).await;
    assert!(!core.extensions.worker_running().await);
}
