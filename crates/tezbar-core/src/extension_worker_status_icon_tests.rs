//! Narrow fake-status boundary only; no worker, desktop or user profile.
use super::*;
use crate::{common::CorePaths, host::TestHost};
fn fixture() -> (tempfile::TempDir, ExtensionWorker, Arc<TestHost>) {
    let dir = tempfile::tempdir().unwrap();
    let host = Arc::new(TestHost {
        allow_interaction: true,
        ..Default::default()
    });
    let paths = CorePaths {
        data_dir: dir.path().join("data"),
        config_dir: dir.path().join("config"),
        resource_dir: dir.path().join("resources"),
        temp_dir: dir.path().join("tmp"),
        home_dir: dir.path().join("home"),
    };
    let worker = ExtensionWorker::new(CoreContext::new(paths, Some(host.clone())));
    worker.sessions.lock().unwrap().insert(
        "s".into(),
        ("raycast.fixture".into(), "command".into(), "scope".into()),
    );
    (dir, worker, host)
}
fn reply(icon: &str) -> Value {
    json!({"sessionId":"s","root":{"type":"MenuBarExtra","props":{"icon":icon},"children":[]}})
}
#[tokio::test]
async fn sdk_status_icon_same_session_a_b_a_rejects_first_late_a() {
    let (_dir, worker, host) = fixture();
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (started, mut receive) = tokio::sync::mpsc::unbounded_channel();
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    *host.status_icon_resolver.lock().unwrap() = Some(Arc::new({
        let gate = gate.clone();
        move |_, _| {
            let call = count.fetch_add(1, Ordering::SeqCst);
            let gate = gate.clone();
            let started = started.clone();
            Box::pin(async move {
                if call == 0 {
                    started.send(()).unwrap();
                    let _ = gate.acquire().await.unwrap();
                }
                Ok((vec![call as u8, 0, 0, 255], 1, 1))
            })
        }
    }));
    let pending = tokio::spawn({
        let worker = worker.clone();
        async move { worker.sync_sdk_status(&reply("A")).await }
    });
    receive.recv().await.unwrap();
    worker.sync_sdk_status(&reply("B")).await.unwrap();
    worker.sync_sdk_status(&reply("A")).await.unwrap();
    assert_eq!(
        host.status_specs.lock().unwrap()["scope"]
            .icon
            .as_ref()
            .unwrap()
            .0,
        [2, 0, 0, 255]
    );
    gate.add_permits(1);
    pending.await.unwrap().unwrap();
    assert_eq!(
        host.status_specs.lock().unwrap()["scope"]
            .icon
            .as_ref()
            .unwrap()
            .0,
        [2, 0, 0, 255]
    );
}
#[tokio::test]
async fn sdk_status_icon_pending_empty_root_and_disposal_do_not_resurrect() {
    for dispose in [false, true] {
        let (_dir, worker, host) = fixture();
        let (started, mut receive) = tokio::sync::mpsc::unbounded_channel();
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        *host.status_icon_resolver.lock().unwrap() = Some(Arc::new({
            let gate = gate.clone();
            move |_, _| {
                let gate = gate.clone();
                let started = started.clone();
                Box::pin(async move {
                    started.send(()).unwrap();
                    let _ = gate.acquire().await.unwrap();
                    Ok((vec![0, 0, 0, 255], 1, 1))
                })
            }
        }));
        let pending = tokio::spawn({
            let worker = worker.clone();
            async move { worker.sync_sdk_status(&reply("Gear")).await }
        });
        receive.recv().await.unwrap();
        if dispose {
            worker.remove_sdk_session("s");
        } else {
            worker
                .sync_sdk_status(&json!({"sessionId":"s","root":null}))
                .await
                .unwrap();
        }
        gate.add_permits(1);
        pending.await.unwrap().unwrap();
        assert!(host.status_specs.lock().unwrap().is_empty());
    }
}
#[tokio::test]
async fn sdk_status_icon_title_only_and_declared_pixels_cross_actual_test_host_policy() {
    let (_dir, worker, host) = fixture();
    worker.sync_sdk_status(&json!({"sessionId":"s","root":{"type":"MenuBarExtra","props":{"title":"Named status","tooltip":"Useful status"},"children":[]}})).await.unwrap();
    {
        let specs = host.status_specs.lock().unwrap();
        let spec = &specs["scope"];
        assert_eq!(spec.title, "Named status");
        assert_eq!(
            spec.icon,
            crate::extensions::process_host::sdk_status_icon_fallback(
                cfg!(target_os = "windows"),
                crate::extensions::process_host::SdkStatusIconFallback::Absent,
            )
        );
        assert_eq!(
            spec.tooltip,
            if cfg!(target_os = "windows") {
                "Useful status (no icon declared)"
            } else {
                "Useful status"
            }
        );
    }
    *host.status_icon_resolver.lock().unwrap() = Some(Arc::new(|_, _| {
        Box::pin(async { Ok((vec![18, 52, 86, 255], 1, 1)) })
    }));
    worker.sync_sdk_status(&reply("Checkmark")).await.unwrap();
    let specs = host.status_specs.lock().unwrap();
    let spec = &specs["scope"];
    assert_eq!(
        spec.icon,
        Some((vec![18, 52, 86, 255], 1, 1)),
        "a good declaration must never be replaced by a platform fallback"
    );
    assert!(
        spec.title.is_empty(),
        "icon-only roots must not duplicate their glyph as text"
    );
    assert_eq!(spec.tooltip, "command");
}
#[tokio::test]
async fn sdk_status_icon_refusal_is_honest_and_unchanged_declaration_is_coalesced() {
    let (_dir, worker, host) = fixture();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    *host.status_icon_resolver.lock().unwrap() = Some(Arc::new({
        let calls = calls.clone();
        move |_, _| {
            calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Err("Unsupported declared builtin".into()) })
        }
    }));
    worker.sync_sdk_status(&reply("Unknown")).await.unwrap();
    worker.sync_sdk_status(&reply("Unknown")).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let specs = host.status_specs.lock().unwrap();
    let spec = &specs["scope"];
    assert_eq!(
        spec.icon,
        crate::extensions::process_host::sdk_status_icon_fallback(
            cfg!(target_os = "windows"),
            crate::extensions::process_host::SdkStatusIconFallback::Unavailable,
        )
    );
    assert_eq!(spec.title, "command");
    assert!(spec.tooltip.ends_with("(icon unavailable)"));
}
