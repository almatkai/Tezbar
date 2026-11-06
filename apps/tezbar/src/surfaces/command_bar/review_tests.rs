//! Regression fixtures for the M4 af1ccdd closure. No production profile or provider calls.
use super::*;
use ::core::prelude::v1::test;
use gpui::{TestAppContext, VisualTestContext};

struct ModelConfig {
    fail_read: bool,
}
impl RequestAdapter for ModelConfig {
    fn send(
        &self,
        request: Request,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>> {
        let result = match request {
            Request::LlmConfig if self.fail_read => Err("Synthetic config read failure".into()),
            Request::LlmConfig => Ok(json!({"provider":"ollama", "model":"fixture-model",
                "providerModels":{"ollama":[{"id":"fixture-model"}]}})),
            Request::LlmSet(_) => Ok(json!({"ok":true})),
            _ => panic!("Unexpected operation in state-save regression"),
        };
        Box::pin(async move { result })
    }
}
fn runtime(cx: &mut TestAppContext) -> tokio::runtime::Runtime {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| gpui_tokio::init_from_handle(cx, runtime.handle().clone()));
    runtime
}
fn await_writes(
    runtime: &tokio::runtime::Runtime,
    cx: &mut VisualTestContext,
    writes: Vec<Task<()>>,
) {
    runtime.block_on(async {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        while writes.iter().any(|write| !write.is_ready()) {
            cx.run_until_parked();
            assert!(tokio::time::Instant::now() < deadline, "State save stalled");
            tokio::task::yield_now().await;
        }
    });
    cx.run_until_parked();
}

#[gpui::test]
fn quit_retries_current_launcher_state_after_transient_save_failure(cx: &mut TestAppContext) {
    let runtime = runtime(cx);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ui-state-v1.json");
    // A directory in place of the file makes the actual atomic replace fail.
    std::fs::create_dir(&path).unwrap();
    let (relay, _rx) = Relay::new();
    let (view, cx) = cx.add_window_view(|w, cx| {
        Launcher::with_adapter(
            Arc::new(ModelConfig { fail_read: false }),
            relay,
            None,
            w,
            cx,
        )
    });
    let writes = view.update(cx, |view, cx| {
        view.state_path = Some(path.clone());
        view.state.launcher_query_history = vec!["first state".into()];
        view.drain_state(cx)
    });
    await_writes(&runtime, cx, writes);
    assert!(view.read_with(cx, |view, _| view.state_write_error().is_some()));
    std::fs::remove_dir(&path).unwrap();
    let writes = view.update(cx, |view, cx| {
        view.state
            .launcher_query_history
            .push("latest state".into());
        view.drain_state(cx)
    });
    await_writes(&runtime, cx, writes);
    assert!(view.read_with(cx, |view, _| view.state_write_error().is_none()));
    let saved: UiState = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        saved.launcher_query_history,
        ["first state", "latest state"]
    );
}

#[gpui::test]
fn successful_model_save_cannot_clear_a_failed_launcher_state_save(cx: &mut TestAppContext) {
    let runtime = runtime(cx);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ui-state-v1.json");
    std::fs::create_dir(&path).unwrap();
    let (relay, _rx) = Relay::new();
    let (view, cx) = cx.add_window_view(|w, cx| {
        Launcher::with_adapter(
            Arc::new(ModelConfig { fail_read: false }),
            relay,
            None,
            w,
            cx,
        )
    });
    let writes = view.update(cx, |view, cx| {
        view.state_path = Some(path);
        view.drain_state(cx)
    });
    await_writes(&runtime, cx, writes);
    let state_error = view
        .read_with(cx, |view, _| view.state_write_error())
        .unwrap();
    let writes = view.update(cx, |view, cx| {
        view.model_config = json!({"providerModels":{"ollama":[{"id":"fixture-model"}]}});
        view.model_choices = vec![("ollama".into(), "fixture-model".into())];
        view.select_model(0, cx);
        std::mem::take(&mut view.writes)
    });
    await_writes(&runtime, cx, writes);
    assert_eq!(
        view.read_with(cx, |view, _| view.model_config["model"].clone()),
        "fixture-model"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.state_write_error()),
        Some(state_error)
    );
    // A later quit still attempts the failed durable write, rather than trusting the model save.
    let writes = view.update(cx, |view, cx| view.drain_state(cx));
    await_writes(&runtime, cx, writes);
    assert!(view.read_with(cx, |view, _| view.state_write_error().is_some()));
}

#[gpui::test]
fn model_config_read_failure_does_not_block_durable_launcher_drain(cx: &mut TestAppContext) {
    let runtime = runtime(cx);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ui-state-v1.json");
    let (relay, _rx) = Relay::new();
    let (view, cx) = cx.add_window_view(|w, cx| {
        Launcher::with_adapter(
            Arc::new(ModelConfig { fail_read: true }),
            relay,
            None,
            w,
            cx,
        )
    });
    let writes = view.update(cx, |view, cx| {
        view.state_path = Some(path.clone());
        view.model_config = json!({"providerModels":{"ollama":[{"id":"fixture-model"}]}});
        view.model_choices = vec![("ollama".into(), "fixture-model".into())];
        view.select_model(0, cx);
        view.drain_state(cx)
    });
    await_writes(&runtime, cx, writes);
    assert!(view.read_with(cx, |view, _| view.error.as_deref()
        == Some("Synthetic config read failure")));
    assert!(view.read_with(cx, |view, _| view.state_write_error().is_none()));
    assert!(path.is_file());
}
