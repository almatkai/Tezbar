//! Background update discovery for installed extensions.
//!
//! JavaScript extensions install from the catalog's pre-built bundle, so a bundle
//! rebuilt after the extension was installed is an update. Native Rust packages are
//! pinned in the bundled release catalog, so a newer pinned version is an update.
//! Nothing is installed here: the Store offers the update and reuses the install path.
use super::{acquisition, catalog::Catalog, process_install, registry, v2_catalog};
use crate::common::{now_ms, read_json, write_json_atomic, CoreContext, CoreResult};
use serde_json::{json, Map, Value};
use std::{collections::HashMap, path::PathBuf, time::Duration};

/// How often installed extensions are checked for updates.
pub(super) const INTERVAL_MS: u64 = 12 * 60 * 60 * 1000;
/// Startup work (launcher index, restored extensions) goes first.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(90);
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(60 * 60);
/// Clock skew allowance between the catalog server and this machine.
const GRACE_MS: u64 = 5 * 60 * 1000;
pub(super) const CHANGED_EVENT: &str = "extension-updates-changed";

fn path(ctx: &CoreContext) -> PathBuf {
    ctx.paths.data_dir.join("extension-updates.json")
}

fn stored(ctx: &CoreContext) -> Value {
    read_json(&path(ctx))
        .ok()
        .filter(|v| v["updates"].is_object())
        .unwrap_or_else(|| json!({"checkedAt":0,"updates":{}}))
}

fn version_newer(latest: &str, installed: &str) -> bool {
    match (semver::Version::parse(latest), semver::Version::parse(installed)) {
        (Ok(latest), Ok(installed)) => latest > installed,
        _ => false,
    }
}

/// A recorded update stops applying once the extension is reinstalled or upgraded.
fn applies(installed: &Value, update: &Value) -> bool {
    if let Some(latest) = update["latestVersion"].as_str() {
        return version_newer(latest, installed["version"].as_str().unwrap_or(""));
    }
    let installed_at = installed["installedAt"].as_u64().unwrap_or(u64::MAX);
    update["latestAt"]
        .as_u64()
        .is_some_and(|latest| latest > installed_at.saturating_add(GRACE_MS))
}

/// Attach `update` to installed extensions that still have one.
pub(super) fn annotate(ctx: &CoreContext, installed: &mut [Value]) {
    let stored = stored(ctx);
    for value in installed {
        let Some(update) = value["id"].as_str().and_then(|id| stored["updates"].get(id)) else {
            continue;
        };
        if applies(value, update) {
            value["update"] = update.clone();
        }
    }
}

/// Pure detection over the installed list, catalog bundle times and pinned native releases.
pub(super) fn detect(
    installed: &[Value],
    bundles: &HashMap<String, u64>,
    natives: &[Value],
) -> Map<String, Value> {
    let mut updates = Map::new();
    for value in installed {
        let Some(id) = value["id"].as_str() else { continue };
        let update = if id.starts_with("com.tezbar.native.") {
            natives
                .iter()
                .find(|native| native["id"] == id)
                .filter(|native| process_install::artifact(native).is_ok())
                .filter(|_| acquisition::require_remote(acquisition::for_id(id)).is_ok())
                .and_then(|native| native["version"].as_str())
                .map(|latest| json!({"kind":"version","latestVersion":latest}))
        } else {
            value["slug"]
                .as_str()
                .or_else(|| id.strip_prefix("raycast."))
                .and_then(|name| bundles.get(name))
                .map(|latest| json!({"kind":"bundle","latestAt":latest}))
        };
        if let Some(update) = update.filter(|update| applies(value, update)) {
            updates.insert(id.to_owned(), update);
        }
    }
    updates
}

/// Check now, persist the result and tell open surfaces.
pub(super) async fn check(ctx: &CoreContext, catalog: &Catalog) -> CoreResult {
    let bundles = catalog.bundle_times().await?;
    let worker_ctx = ctx.clone();
    let saved = tokio::task::spawn_blocking(move || {
        let installed = registry::list(&worker_ctx);
        let updates = detect(
            installed.as_array().map(Vec::as_slice).unwrap_or_default(),
            &bundles,
            &v2_catalog::source(),
        );
        let saved = json!({"checkedAt":now_ms(),"updates":updates});
        write_json_atomic(&path(&worker_ctx), &saved)?;
        Ok::<_, String>(saved)
    })
    .await
    .map_err(|e| e.to_string())??;
    let count = saved["updates"].as_object().map_or(0, Map::len);
    ctx.emit(CHANGED_EVENT, json!({"count":count}));
    Ok(summary(ctx, &saved))
}

/// The persisted check, limited to updates that still apply.
pub(super) fn current(ctx: &CoreContext) -> CoreResult {
    Ok(summary(ctx, &stored(ctx)))
}

fn summary(ctx: &CoreContext, saved: &Value) -> Value {
    // The installed list already carries the updates that still apply.
    let installed = registry::list(ctx).as_array().cloned().unwrap_or_default();
    let updates: Vec<Value> = installed
        .into_iter()
        .filter(|v| v["update"].is_object())
        .map(|v| json!({"id":v["id"],"name":v["name"],"version":v["version"],"update":v["update"]}))
        .collect();
    json!({"checkedAt":saved["checkedAt"],"intervalMs":INTERVAL_MS,"updates":updates})
}

/// Check every 12 hours, measured from the last persisted check so restarts do not
/// repeat it. Sleeps in short steps so a machine waking from sleep catches up.
pub(super) fn spawn(ctx: CoreContext, catalog: Catalog) -> Option<tokio::task::JoinHandle<()>> {
    let runtime = tokio::runtime::Handle::try_current().ok()?;
    Some(runtime.spawn(async move {
        tokio::time::sleep(FIRST_CHECK_DELAY).await;
        loop {
            let due = stored(&ctx)["checkedAt"]
                .as_u64()
                .unwrap_or(0)
                .saturating_add(INTERVAL_MS);
            let now = now_ms();
            if now < due {
                tokio::time::sleep(Duration::from_millis((due - now).min(15 * 60 * 1000))).await;
                continue;
            }
            if let Err(error) = check(&ctx, &catalog).await {
                log::warn!("Extension update check failed: {error}");
                tokio::time::sleep(RETRY_AFTER_FAILURE).await;
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn native(version: &str, pinned: bool) -> Value {
        let mut m = json!({"schemaVersion":3,"runtime":"rust","execution":"process","protocolVersion":1,
            "id":"com.tezbar.native.synthetic","slug":"synthetic","name":"Synthetic","version":version,
            "capabilities":[],"commands":[{"name":"run","runtime":"rust","platforms":["macOS","Windows"]}],
            "replaces":["raycast.synthetic"],"platforms":["macOS","Windows"],"artifacts":{}});
        if pinned {
            m["artifacts"][process_install::target()] = json!({"sha256":"0".repeat(64),
                "executable":"bin/synthetic","executableSha256":"1".repeat(64)});
        }
        m
    }

    #[test]
    fn rebuilt_bundles_and_newer_pinned_releases_are_updates() {
        let installed = vec![
            json!({"id":"raycast.base64","slug":"base64","installedAt":1_000_000_000}),
            json!({"id":"raycast.fresh","slug":"fresh","installedAt":2_000_000_000}),
            json!({"id":"raycast.unknown","slug":"unknown","installedAt":1}),
            json!({"id":"com.tezbar.native.synthetic","slug":"synthetic","version":"1.0.0","installedAt":1}),
        ];
        let bundles = HashMap::from([
            ("base64".to_owned(), 1_500_000_000),
            // Rebuilt before installation (within the clock-skew allowance) is current.
            ("fresh".to_owned(), 2_000_000_000 + GRACE_MS - 1),
        ]);
        let found = detect(&installed, &bundles, &[native("1.2.0", true)]);
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(found["raycast.base64"], json!({"kind":"bundle","latestAt":1_500_000_000u64}));
        assert_eq!(
            found["com.tezbar.native.synthetic"],
            json!({"kind":"version","latestVersion":"1.2.0"})
        );
        // Same version, or a release without a verified artifact, is not offered.
        assert!(detect(&installed[3..], &bundles, &[native("1.0.0", true)]).is_empty());
        assert!(detect(&installed[3..], &bundles, &[native("2.0.0", false)]).is_empty());
    }

    #[test]
    fn recorded_updates_stop_applying_after_reinstall() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = CoreContext::headless(crate::common::CorePaths {
            data_dir: dir.path().into(),
            config_dir: dir.path().join("config"),
            resource_dir: dir.path().join("resources"),
            temp_dir: dir.path().join("tmp"),
            home_dir: dir.path().into(),
        });
        write_json_atomic(
            &path(&ctx),
            &json!({"checkedAt":1,"updates":{
                "raycast.base64":{"kind":"bundle","latestAt":1_500_000_000u64},
                "com.tezbar.native.synthetic":{"kind":"version","latestVersion":"1.2.0"}}}),
        )
        .unwrap();
        let mut values = vec![
            json!({"id":"raycast.base64","installedAt":1_000_000_000u64}),
            json!({"id":"com.tezbar.native.synthetic","version":"1.0.0"}),
        ];
        annotate(&ctx, &mut values);
        assert_eq!(values[0]["update"]["latestAt"], 1_500_000_000u64);
        assert_eq!(values[1]["update"]["latestVersion"], "1.2.0");
        let mut reinstalled = vec![
            json!({"id":"raycast.base64","installedAt":1_600_000_000u64}),
            json!({"id":"com.tezbar.native.synthetic","version":"1.2.0"}),
        ];
        annotate(&ctx, &mut reinstalled);
        assert!(reinstalled.iter().all(|v| v.get("update").is_none()));
    }
}
