use super::*;
use ::core::prelude::v1::test;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tezbar_core::host::Host;
#[allow(dead_code)]
#[path = "../../../../tests/fixtures/extensions/native_harness.rs"]
mod native_harness;
const EXTENSION: &str = "raycast.m09-native-fixtures";

#[test]
fn sdk_and_css_tints_preserve_rgba_and_unknown_tints_keep_the_icon() {
    for (name, expected) in [
        ("Red", [255, 0, 0, 255]),
        ("Orange", [255, 165, 0, 255]),
        ("Yellow", [255, 255, 0, 255]),
        ("Green", [0, 128, 0, 255]),
        ("Blue", [0, 0, 255, 255]),
        ("Purple", [128, 0, 128, 255]),
        ("Magenta", [255, 0, 255, 255]),
        ("PrimaryText", [128, 128, 128, 255]),
        ("SecondaryText", [128, 128, 128, 255]),
        ("#f00", [255, 0, 0, 255]),
        ("#f008", [255, 0, 0, 136]),
        ("#ff000080", [255, 0, 0, 128]),
        ("rgb(255, 0, 0)", [255, 0, 0, 255]),
        ("pink", [255, 192, 203, 255]),
        ("rgba(255, 0, 0, .5)", [255, 0, 0, 128]),
    ] {
        assert_eq!(tint(Some(&json!(name))), expected, "{name}");
    }
    assert_eq!(
        tint(Some(&json!({"dark":"Magenta","light":"Blue"}))),
        [255, 0, 255, 255]
    );
    for invalid in [
        json!("future-color"),
        json!({"other":"Blue"}),
        json!(42),
        json!("x".repeat(1000)),
    ] {
        assert_eq!(tint(Some(&invalid)), [128, 128, 128, 255]);
    }
}

#[tokio::test]
async fn bare_sdk_assets_and_magenta_cross_the_real_scope_and_decoder() {
    let f = fixture().await;
    let resolve = |icon| f.host.resolve_sdk_status_icon(EXTENSION.into(), icon);
    for icon in [
        json!("pixel.png"),
        json!({"source":"pixel.png"}),
        json!("./assets/pixel.png"),
    ] {
        assert_eq!(resolve(icon).await.unwrap(), (vec![16, 32, 48, 255], 1, 1));
    }
    let magenta = resolve(json!({"source":"Checkmark","tintColor":"Magenta"}))
        .await
        .unwrap();
    assert_eq!((magenta.1, magenta.2), (24, 24));
    assert!(magenta.0.chunks_exact(4).any(|p| p == [255, 0, 255, 255]));
    let alpha = resolve(json!({"source":"Gear","tintColor":"#ff000080"}))
        .await
        .unwrap();
    assert!(alpha.0.chunks_exact(4).any(|p| p == [255, 0, 0, 128]));
    let fallback = resolve(json!({"source":"Checkmark","tintColor":"future-color"}))
        .await
        .unwrap();
    assert_eq!((fallback.1, fallback.2), (24, 24));
    assert!(
        fallback
            .0
            .chunks_exact(4)
            .any(|p| p == [128, 128, 128, 255])
    );
    f.transport.shutdown().await;
}

async fn fixture() -> native_harness::Fixture {
    let prepared =
        PathBuf::from(std::env::var_os("TEZBAR_M09_PREPARED").expect("explicit prepared SDK seed"));
    let bun = PathBuf::from(std::env::var_os("TEZBAR_M09_BUN").expect("explicit Bun path"));
    native_harness::Fixture::isolated(&prepared, bun, tokio::runtime::Handle::current())
        .await
        .unwrap()
}
async fn run(f: &native_harness::Fixture, icon: &str) -> Value {
    f.transport.core.request("extension:run-command", &[json!({"extensionId":EXTENSION,"commandName":"probe-status-icons","argumentValues":{"icon":icon}})]).await.unwrap()
}
async fn action(f: &native_harness::Fixture, reply: &Value, title: &str) -> Value {
    let id = reply["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["title"] == title)
        .unwrap()["id"]
        .clone();
    f.transport
        .core
        .request(
            "extension:invoke-action",
            &[json!({"sessionId":reply["sessionId"],"actionId":id})],
        )
        .await
        .unwrap()
}
fn spec(f: &native_harness::Fixture) -> tezbar_core::host::StatusItem {
    let specs = f.host.status_specs.lock().unwrap();
    assert_eq!(specs.len(), 1);
    specs.values().next().unwrap().clone()
}
async fn dispose(f: &native_harness::Fixture, reply: &Value) {
    f.transport
        .core
        .request(
            "extension:dispose-session",
            &[json!({"sessionId":reply["sessionId"]})],
        )
        .await
        .unwrap();
}
#[tokio::test]
async fn status_icons_real_bun_declared_builtin_and_custom_pixels_cross_core_host() {
    let f = fixture().await;
    let check_reply = run(&f, "Checkmark").await;
    let check = spec(&f);
    assert!(check.title.is_empty(), "icon-only root duplicates text");
    let check = check.icon.expect("declared Checkmark bitmap");
    assert_eq!((check.1, check.2), (24, 24));
    assert!(check.0.chunks_exact(4).any(|p| p[3] > 128));
    assert!(check.0.chunks_exact(4).any(|p| p[3] == 0));
    let gear_reply = action(&f, &check_reply, "Gear").await;
    let gear = spec(&f).icon.unwrap();
    assert_eq!((gear.1, gear.2), (16, 16));
    assert_ne!(
        check, gear,
        "F4: a declared non-gear became the generic gear"
    );
    let png_reply = action(&f, &gear_reply, "Local PNG").await;
    let png = spec(&f).icon.unwrap();
    assert_eq!(
        png,
        (vec![16, 32, 48, 255], 1, 1),
        "straight RGBA, not GPUI BGRA"
    );
    let svg_reply = action(&f, &png_reply, "Local SVG").await;
    let svg = spec(&f).icon.unwrap();
    assert_eq!((svg.1, svg.2), (4, 4));
    assert!(svg.0.chunks_exact(4).all(|p| p == [224, 32, 64, 255]));
    let unsupported = action(&f, &svg_reply, "Unsupported").await;
    let fallback = spec(&f);
    assert_eq!(fallback.icon.is_some(), cfg!(target_os = "windows"));
    if let Some((pixels, w, h)) = &fallback.icon {
        assert_eq!((*w, *h), (16, 16));
        assert!(pixels.chunks_exact(4).any(|p| p[3] != 0));
        assert_ne!(fallback.icon.as_ref().unwrap(), &gear);
    }
    assert_eq!(fallback.title, "probe status icons");
    assert!(fallback.tooltip.ends_with("(icon unavailable)"));
    dispose(&f, &unsupported).await;
    assert!(f.host.status_specs.lock().unwrap().is_empty());
    f.transport.shutdown().await;
}
#[tokio::test]
async fn status_icons_data_adaptive_and_refused_paths_use_canonical_registry() {
    let f = fixture().await;
    let resolve = |icon| f.host.resolve_sdk_status_icon(EXTENSION.into(), icon);
    let data = format!("data:image/svg+xml;base64,{}", STANDARD.encode(br##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><path fill="#123456" d="M0 0h2v2H0z"/></svg>"##));
    let data_reply = run(&f, &data).await;
    assert_eq!(spec(&f).icon.unwrap(), ([18, 52, 86, 255].repeat(4), 2, 2));
    dispose(&f, &data_reply).await;
    let (bytes, w, h) = resolve(json!({"source":{"dark":data,"light":"../refused"}}))
        .await
        .unwrap();
    assert_eq!((w, h), (2, 2));
    assert_eq!(bytes, [18, 52, 86, 255].repeat(4));
    let tinted = resolve(json!({"source":"Checkmark","tintColor":"#123456"}))
        .await
        .unwrap();
    assert!(tinted.0.chunks_exact(4).any(|p| p == [18, 52, 86, 255]));
    for icon in [
        json!("../outside.png"),
        json!("/tmp/unowned.png"),
        json!("file:///tmp/unowned.png"),
        json!("http://127.0.0.1/icon.png"),
        json!("https://user:secret@127.0.0.1/icon.png"),
        json!("blob:unowned"),
        json!("⚙"),
        json!("UnknownBuiltin"),
        json!("raycast-NotAReal-Gear"),
        json!({"fileIcon":"/Applications/unowned.app"}),
        json!({"source":"Gear","mask":"circle"}),
    ] {
        assert!(resolve(icon).await.is_err());
    }
    assert!(
        f.host
            .resolve_sdk_status_icon("raycast.not-installed".into(), json!("Gear"))
            .await
            .is_err()
    );
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("pixel.png"), [0; 4]).unwrap();
        let scope = installed_scope(f.transport.core.clone(), EXTENSION.into())
            .await
            .unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("pixel.png"),
            scope.extension_root.join("assets/escape.png"),
        )
        .unwrap();
        assert!(resolve(json!("assets/escape.png")).await.is_err());
        assert!(resolve(json!("escape.png")).await.is_err());
    }
    f.transport.shutdown().await;
}
#[tokio::test]
async fn status_icons_share_all_four_core_slots_and_recheck_scope_after_wait() {
    let f = fixture().await;
    let core = f.transport.core.clone();
    let mut permits = Vec::new();
    for _ in 0..4 {
        permits.push(core.assets.decode_permit().await.unwrap());
    }
    let mut pending = tokio::spawn(resolve_icon(
        Arc::downgrade(&core),
        EXTENSION.into(),
        json!("Gear"),
    ));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(40), &mut pending)
            .await
            .is_err()
    );
    assert_eq!(core.assets.preview_admission().available_permits(), 0);
    drop(permits.pop());
    assert_eq!(pending.await.unwrap().unwrap().1, 16);
    assert_eq!(core.assets.preview_admission().available_permits(), 1);
    permits.push(core.assets.decode_permit().await.unwrap());
    let mut pending = tokio::spawn(resolve_icon(
        Arc::downgrade(&core),
        EXTENSION.into(),
        json!("assets/status.svg"),
    ));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(40), &mut pending)
            .await
            .is_err()
    );
    let scope = installed_scope(core.clone(), EXTENSION.into())
        .await
        .unwrap();
    // Only this test's private fixture manifest; never an installed user package.
    std::fs::remove_file(scope.extension_root.join("package.json")).unwrap();
    drop(permits.pop());
    assert!(
        pending.await.unwrap().is_err(),
        "a revoked registry identity survived async decode"
    );
    drop(permits);
    assert_eq!(core.assets.preview_admission().available_permits(), 4);
    f.transport.shutdown().await;
}
#[tokio::test]
async fn status_icons_real_bun_invalidation_during_pending_load_keeps_latest_declared_pixels() {
    let f = fixture().await;
    let first = run(&f, "Checkmark").await;
    let original = f.host.status_icon_resolver.lock().unwrap().clone().unwrap();
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let (started, mut receive) = tokio::sync::mpsc::unbounded_channel();
    let (latest, mut receive_latest) = tokio::sync::mpsc::unbounded_channel();
    *f.host.status_icon_resolver.lock().unwrap() = Some(Arc::new({
        let gate = gate.clone();
        move |extension, icon| {
            let gear = icon == "Gear";
            let svg = icon == "assets/status.svg";
            let future = original(extension, icon);
            let gate = gate.clone();
            let started = started.clone();
            let latest = latest.clone();
            Box::pin(async move {
                if gear {
                    started.send(()).unwrap();
                    let _ = gate.acquire().await.unwrap();
                }
                let result = future.await;
                if svg {
                    latest.send(()).unwrap();
                }
                result
            })
        }
    }));
    let core = f.transport.core.clone();
    let id = first["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["title"] == "Gear then SVG")
        .unwrap()["id"]
        .clone();
    let session = first["sessionId"].clone();
    let pending = tokio::spawn(async move {
        core.request(
            "extension:invoke-action",
            &[json!({"sessionId":session,"actionId":id})],
        )
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), receive.recv())
        .await
        .unwrap()
        .unwrap();
    // The owned worker timer changes the declaration while both native refresh
    // and direct invoke are allowed to await their shared asset capability.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    gate.add_permits(4);
    pending.await.unwrap().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), receive_latest.recv())
        .await
        .unwrap()
        .unwrap();
    let refreshed = f
        .transport
        .core
        .request(
            "extension:refresh-session",
            &[json!({"sessionId":first["sessionId"],"viewRevision":0})],
        )
        .await
        .unwrap();
    assert_eq!(refreshed["root"]["props"]["icon"], "assets/status.svg");
    assert_eq!(
        spec(&f).icon.unwrap(),
        ([224, 32, 64, 255].repeat(16), 4, 4)
    );
    dispose(&f, &first).await;
    f.transport.shutdown().await;
}
#[tokio::test]
async fn status_icons_real_bun_pending_dispose_and_replacement_cannot_resurrect() {
    let f = fixture().await;
    let first = run(&f, "Checkmark").await;
    let original = f.host.status_icon_resolver.lock().unwrap().clone().unwrap();
    let (started, mut receive) = tokio::sync::mpsc::unbounded_channel();
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    *f.host.status_icon_resolver.lock().unwrap() = Some(Arc::new({
        let gate = gate.clone();
        let calls = calls.clone();
        move |extension, icon| {
            let future = original(extension, icon);
            let gate = gate.clone();
            let pause = calls.fetch_add(1, Ordering::SeqCst) == 0;
            let started = started.clone();
            Box::pin(async move {
                if pause {
                    started.send(()).unwrap();
                    let _ = gate.acquire().await.unwrap();
                }
                future.await
            })
        }
    }));
    let core = f.transport.core.clone();
    let id = first["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["title"] == "Gear")
        .unwrap()["id"]
        .clone();
    let session = first["sessionId"].clone();
    let pending = tokio::spawn(async move {
        core.request(
            "extension:invoke-action",
            &[json!({"sessionId":session,"actionId":id})],
        )
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), receive.recv())
        .await
        .unwrap()
        .unwrap();
    dispose(&f, &first).await;
    assert!(f.host.status_specs.lock().unwrap().is_empty());
    let second = run(&f, "assets/status.svg").await;
    let replacement = spec(&f).icon.unwrap();
    gate.add_permits(1);
    pending.await.unwrap().unwrap();
    assert_eq!(
        spec(&f).icon.unwrap(),
        replacement,
        "late old-session bitmap replaced the new tray"
    );
    assert_eq!(f.host.status_specs.lock().unwrap().len(), 1);
    dispose(&f, &second).await;
    assert!(f.host.status_specs.lock().unwrap().is_empty());
    f.transport.shutdown().await;
}
