use super::*;
use common::{write_json_atomic, CorePaths};
use std::collections::HashSet;

#[test]
fn every_pre_migration_ipc_channel_has_a_native_owner() {
    // Frozen inventory of ipc.ts at the beta migration baseline, not generated
    // from the new dispatcher. Includes legacy consumers outside the renderer.
    let legacy: Vec<String> =
        serde_json::from_str(include_str!("fixtures/legacy_channels.json")).unwrap();
    // Four former Port Manager channels moved to the external package.
    assert_eq!(legacy.len(), 151);
    let routes: HashSet<&str> = [
        ai::CHANNELS,
        knowledge::CHANNELS,
        launcher::CHANNELS,
        system::CHANNELS,
        platform::CHANNELS,
        terminal_bridge::CHANNELS,
        extensions::CHANNELS,
        extension_worker::CHANNELS,
    ]
    .into_iter()
    .flatten()
    .copied()
    .collect();
    let missing: Vec<_> = legacy
        .iter()
        .filter(|channel| !routes.contains(channel.as_str()))
        .collect();
    assert!(missing.is_empty(), "Missing legacy routes: {missing:?}");
    assert!(extension_worker::CHANNELS
        .iter()
        .all(|channel| channel.starts_with("extension:") || channel.starts_with("extensions:")));
}

#[tokio::test]
async fn native_profile_survives_restart_without_an_extension_runtime() {
    let temp = tempfile::tempdir().unwrap();
    let paths = CorePaths {
        data_dir: temp.path().join("data"),
        config_dir: temp.path().join("config"),
        // No JS bundles or runtime are present anywhere in this test profile.
        resource_dir: temp.path().join("no-runtime"),
        temp_dir: temp.path().join("temp"),
        home_dir: temp.path().join("home"),
    };
    write_json_atomic(
        &paths.config_dir.join("config.json"),
        &json!({
            "futureOption":{"keep":"Қазақша"}, "clipboardWatchEnabled":false
        }),
    )
    .unwrap();
    let extension = paths.data_dir.join("extensions/native-fixture");
    write_json_atomic(&extension.join("package.json"), &json!({
        "name":"native-fixture", "title":"Native fixture", "platforms":["macOS","Windows","Linux"],
        "commands":[{"name":"run","title":"Run","mode":"no-view"}],
        "preferences":[{"name":"locale","type":"textfield","default":"kk"}]
    })).unwrap();
    let core = NativeCore::new(CoreContext::headless(paths.clone())).unwrap();
    // Model a large/locked legacy Knowledge store: launcher readiness must not
    // depend on opening or rebuilding this independent database.
    std::fs::create_dir_all(paths.data_dir.join("knowledge")).unwrap();
    let held =
        rusqlite::Connection::open(paths.data_dir.join("knowledge/knowledge.sqlite3")).unwrap();
    held.execute_batch("BEGIN EXCLUSIVE;").unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), core.start())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        core.request("health:ping", &[]).await.unwrap()["runtime"],
        "rust"
    );
    assert!(core.request("core:apps:list", &[]).await.is_err());
    assert!(core.request("unknown-service", &[]).await.is_err());
    let note = core
        .request("notes:append", &[json!("Migration note 日本語🐎")])
        .await
        .unwrap();
    let listed = core.request("notes:list", &[]).await.unwrap();
    assert_eq!(listed[0]["createdAt"], note["createdAt"]);
    held.execute_batch("ROLLBACK;").unwrap();
    drop(held);
    let found = core
        .request("search:all", &[json!("Migration note")])
        .await
        .unwrap();
    assert!(
        found
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["category"] == "quick-notes"),
        "{found}"
    );
    core.request(
        "save-settings",
        &[json!({"commandAliases":{"native:test":"saved alias"}})],
    )
    .await
    .unwrap();
    assert_eq!(
        core.request("get-settings", &[]).await.unwrap()["commandAliases"]["native:test"],
        "saved alias"
    );
    assert_eq!(
        core.request("extension:list", &[]).await.unwrap()[0]["id"],
        "raycast.native-fixture"
    );
    let pref = json!({"extensionId":"raycast.native-fixture"});
    assert_eq!(
        core.request("preferences:get", &[pref.clone()])
            .await
            .unwrap()["locale"],
        "kk"
    );
    core.request(
        "preferences:set",
        &[json!({"extensionId":"raycast.native-fixture","values":{"locale":"日本語"}})],
    )
    .await
    .unwrap();
    assert!(core
        .request("knowledge:snapshot", &[])
        .await
        .unwrap()
        .is_object());
    assert!(core
        .request("background-tasks:list", &[])
        .await
        .unwrap()
        .is_array());
    assert!(core.request("chat:list", &[]).await.unwrap().is_array());
    let appended = core.request("chat:append", &[json!({
        "session":{"id":"fixture-chat","title":"Migration","createdAt":1,"updatedAt":1},
        "turn":{"id":"fixture-turn","role":"assistant","text":"Қазақша 日本語🐎","createdAt":2,
            "timeline":[{"type":"text","text":"Complete"}],
            "responseMeta":{"provider":"custom:test","providerTitle":"Fixture","model":"local"}}
    })]).await.unwrap();
    assert_eq!(appended["ok"], true, "{appended}");
    assert!(!core.extensions.worker_running().await);
    core.shutdown().await;
    assert!(core.request("notes:list", &[]).await.is_err());
    drop(core);

    let reopened = NativeCore::new(CoreContext::headless(paths)).unwrap();
    reopened.start().await.unwrap();
    assert_eq!(
        reopened.request("notes:list", &[]).await.unwrap()[0]["text"],
        note["text"]
    );
    assert_eq!(
        reopened.request("preferences:get", &[pref]).await.unwrap()["locale"],
        "日本語"
    );
    assert_eq!(
        reopened.ctx.config.read().unwrap()["futureOption"]["keep"],
        "Қазақша"
    );
    let chat = reopened
        .request("chat:get", &[json!("fixture-chat")])
        .await
        .unwrap();
    assert_eq!(chat["turns"][0]["text"], "Қазақша 日本語🐎");
    assert_eq!(chat["turns"][0]["responseMeta"]["model"], "local");
    assert!(!reopened.extensions.worker_running().await);
    reopened.shutdown().await;
}
