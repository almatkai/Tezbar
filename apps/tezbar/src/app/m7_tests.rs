//! Production native composition with disposable files and synthetic core transport.
use super::*;
use crate::surfaces::{
    command_bar::settings_integration_tests::{Backend, backend},
    settings::tests::{runtime, settle},
    settings::{EffectBarrier, SettingsEffect, SettingsStore},
};
use ::core::prelude::v1::test;
use tezbar_ui_model::ui_state::{Theme, UiState};
fn await_ui(
    rt: &tokio::runtime::Runtime,
    cx: &mut VisualTestContext,
    task: Task<Result<(), String>>,
) -> Result<(), String> {
    rt.block_on(async {
        let until = tokio::time::Instant::now() + Duration::from_secs(3);
        while !task.is_ready() {
            cx.run_until_parked();
            assert!(
                tokio::time::Instant::now() < until,
                "UI persistence did not settle"
            );
            tokio::task::yield_now().await;
        }
        task.await
    })
}
#[gpui::test]
fn settings_preferences_use_the_launcher_atomic_owner_and_retry_real_file_failure(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let f = backend();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ui-state-v1.json");
    let store = SettingsStore::new(f.clone());
    let (relay, _) = Relay::new();
    let (view, cx) = cx.add_window_view(|w, cx| {
        Launcher::with_adapter_and_settings(
            Arc::new(Backend(f)),
            relay,
            Some(path.clone()),
            Some(store),
            w,
            cx,
        )
    });
    // Fail the actual atomic replacement after a valid initial load. An unreadable
    // initial file deliberately disables writes to preserve the original bytes.
    std::fs::create_dir(&path).unwrap();
    let write = view.update(cx, |v, cx| {
        v.save_settings_ui(&SettingsEffect::Theme(Theme::Dark), cx)
    });
    assert!(await_ui(&rt, cx, write).is_err());
    assert!(view.read_with(cx, |v, _| v.state_write_error().is_some()));
    std::fs::remove_dir(&path).unwrap();
    let write = view.update(cx, |v, cx| {
        v.state.currency.pinned = Some("KZT".into());
        v.save_settings_ui(&SettingsEffect::RetryPersistence, cx)
    });
    await_ui(&rt, cx, write).unwrap();
    let saved: UiState = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(saved.theme, Theme::Dark);
    assert_eq!(saved.currency.pinned.as_deref(), Some("KZT"));
}
#[gpui::test]
fn boot_gate_mounts_without_a_launcher_flash_and_finish_is_source_fenced(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = backend();
    f.fail_reads.store(1, std::sync::atomic::Ordering::SeqCst);
    let store = SettingsStore::new(f.clone());
    let (relay, mut rx) = Relay::new();
    let injected = store.clone();
    let (view, cx) = cx.add_window_view(|w, cx| {
        Launcher::with_adapter_and_settings(
            Arc::new(Backend(f.clone())),
            relay,
            None,
            Some(injected),
            w,
            cx,
        )
    });
    assert!(!view.read_with(cx, |v, _| v.visible));
    assert!(
        rt.block_on(crate::surfaces::onboarding::boot_gate(store))
            .unwrap()
    );
    assert!(!view.read_with(cx, |v, _| v.visible));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.ready = true;
            v.mount_onboarding(w, cx).unwrap();
        })
    });
    settle(&rt, cx);
    let onboarding = view.read_with(cx, |v, _| v.onboarding.as_ref().unwrap().clone());
    assert!(view.read_with(cx, |v, _| v.onboarding_active));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.hide(w, cx);
            v.show(w, cx);
        })
    });
    assert_eq!(
        view.read_with(cx, |v, _| v.onboarding.as_ref().unwrap().entity_id()),
        onboarding.entity_id()
    );
    f.fail.store(true, std::sync::atomic::Ordering::SeqCst);
    onboarding.update(cx, |v, cx| v.finish(cx));
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| v.onboarding_active));
    f.fail.store(false, std::sync::atomic::Ordering::SeqCst);
    onboarding.update(cx, |v, cx| v.retry_finish(cx));
    settle(&rt, cx);
    let id = rt.block_on(async {
        loop {
            if let Some(Event::OnboardingFinished(id)) = rx.recv().await {
                break id;
            }
        }
    });
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            assert!(v.finish_onboarding(id, w, cx));
            assert!(!v.finish_onboarding(id, w, cx));
        })
    });
    assert!(!view.read_with(cx, |v, _| v.onboarding_active));
    assert_eq!(f.config.lock().unwrap()["hasCompletedOnboarding"], true);
}
#[tokio::test]
async fn close_barrier_keeps_latest_failure_and_waits_for_all_parent_writes() {
    let barrier = EffectBarrier::default();
    let old = barrier.begin("ui-state");
    let newer = barrier.begin("ui-state");
    assert!(barrier.finish("ui-state", newer, false));
    assert!(!barrier.finish("ui-state", old, true));
    assert!(barrier.drain().await.is_err());
    let retry = barrier.begin("ui-state");
    assert!(
        tokio::time::timeout(Duration::from_millis(5), barrier.drain())
            .await
            .is_err()
    );
    barrier.finish("ui-state", retry, true);
    barrier.drain().await.unwrap();
}
