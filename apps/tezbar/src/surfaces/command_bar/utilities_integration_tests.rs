use super::*;
use crate::surfaces::utilities::{Reply, UtilityEffect, UtilityRequest, UtilityTransport};
use ::core::prelude::v1::test;
use gpui::{TestAppContext, VisualTestContext};

#[derive(Default)]
struct Fixture {
    calls: std::sync::Mutex<Vec<(String, Vec<Value>)>>,
    fail_save: std::sync::atomic::AtomicBool,
}
impl Fixture {
    fn reply(&self, channel: &'static str, args: Vec<Value>) -> Reply {
        self.calls.lock().unwrap().push((channel.into(), args));
        match channel {
            "search:all" | "clipboard:list" | "background-tasks:list" => Ok(json!([])),
            "notes:list" => Ok(json!([{"createdAt":11,"updatedAt":12,"text":"<div>Hello</div>"}])),
            "notes:update" if self.fail_save.load(std::sync::atomic::Ordering::SeqCst) => {
                Err("Synthetic disk full".into())
            }
            _ => Ok(json!({"ok":true})),
        }
    }
}
impl RequestAdapter for Fixture {
    fn send(
        &self,
        request: Request,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Reply> + Send>> {
        let (channel, args) = request.wire();
        let reply = self.reply(channel, args);
        Box::pin(async move { reply })
    }
}
impl UtilityTransport for Fixture {
    fn send(
        &self,
        request: UtilityRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Reply> + Send>> {
        let (channel, args) = request.wire();
        let reply = self.reply(channel, args);
        Box::pin(async move { reply })
    }
    fn release(
        &self,
        _: String,
        _: &'static str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send>> {
        Box::pin(async { Err("No networking in integration fixtures".into()) })
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
fn settle(runtime: &tokio::runtime::Runtime, cx: &mut VisualTestContext) {
    runtime.block_on(async {
        for _ in 0..64 {
            cx.run_until_parked();
            tokio::task::yield_now().await;
        }
    });
}
fn nav(nonce: u64, surface: Surface, payload: Payload) -> Navigation {
    Navigation {
        nonce,
        surface,
        payload,
    }
}
#[gpui::test]
fn parent_utility_routes_mount_native_content_and_ack_each_admitted_intent(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let adapter = Arc::new(Fixture::default());
    let (relay, _rx) = Relay::new();
    cx.update(|cx| cx.set_global(navigation::NavigationInbox::default()));
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    view.update(cx, |view, cx| {
        view.ready = true;
        view.visible = true;
        view.utility_transport = Some(adapter.clone());
        assert!(view.utilities.is_none());
        for intent in [
            nav(1, Surface::Clipboard, Payload::None),
            nav(2, Surface::Notes, Payload::Note { created_at: 11 }),
        ] {
            cx.global_mut::<navigation::NavigationInbox>()
                .put(intent.clone())
                .unwrap();
            view.receive_navigation(intent, cx);
        }
    });
    settle(&rt, cx);
    view.read_with(cx, |view, cx| {
        let utility = view.utilities.as_ref().unwrap().read(cx);
        assert!(utility.visible);
        assert_eq!(utility.surface, Surface::Notes);
        assert_eq!(utility.notes.selected, Some(11));
        assert!(
            cx.global::<navigation::NavigationInbox>()
                .pending()
                .is_none()
        );
    });
    assert!(
        cx.debug_bounds("drag-handle").is_none(),
        "Utility owns its header; launcher chrome must be absent"
    );
    assert_eq!(
        adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(channel, _)| channel == "notes:list")
            .count(),
        1
    );
}
#[gpui::test]
fn parent_utility_back_restores_command_input_and_stops_the_hidden_surface(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let adapter = Arc::new(Fixture::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    view.update(cx, |view, cx| {
        view.ready = true;
        view.visible = true;
        view.utility_transport = Some(adapter);
        view.receive_navigation(nav(1, Surface::Notes, Payload::Note { created_at: 11 }), cx);
    });
    settle(&rt, cx);
    let utility = view.read_with(cx, |view, _| view.utilities.clone().unwrap());
    utility.update(cx, |_, cx| cx.emit(UtilityEffect::Back));
    settle(&rt, cx);
    assert!(view.read_with(cx, |view, _| view.navigation.is_none()));
    assert!(!utility.read_with(cx, |utility, _| utility.visible));
    cx.simulate_input("apps");
    settle(&rt, cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.model.input.clone()),
        "apps"
    );
}
#[gpui::test]
fn parent_utility_paste_uses_one_canonical_core_effect(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Fixture::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    view.update(cx, |view, cx| {
        view.ready = true;
        view.visible = true;
        view.utility_transport = Some(adapter.clone());
        view.receive_navigation(nav(1, Surface::Emoji, Payload::None), cx);
    });
    settle(&rt, cx);
    let utility = view.read_with(cx, |view, _| view.utilities.clone().unwrap());
    utility.update(cx, |_, cx| {
        cx.emit(UtilityEffect::HideThenPaste("😼".into()))
    });
    settle(&rt, cx);
    let calls = adapter.calls.lock().unwrap();
    let effects = calls
        .iter()
        .filter(|(channel, _)| channel == "search:execute")
        .collect::<Vec<_>>();
    assert_eq!(effects.len(), 1);
    assert_eq!(
        effects[0].1[0]["action"],
        json!({"type":"copy-and-paste-text","text":"😼"})
    );
    assert!(
        !calls
            .iter()
            .any(|(channel, _)| channel == "clipboard:write")
    );
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
            assert!(
                tokio::time::Instant::now() < deadline,
                "Utility drain stalled"
            );
            tokio::task::yield_now().await;
        }
    });
    cx.run_until_parked();
}
#[gpui::test]
fn parent_utility_quit_drain_retries_notes_without_corrupting_state_error_owner(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let adapter = Arc::new(Fixture::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    view.update(cx, |view, cx| {
        view.ready = true;
        view.visible = true;
        view.utility_transport = Some(adapter.clone());
        view.receive_navigation(nav(1, Surface::Notes, Payload::Note { created_at: 11 }), cx);
    });
    settle(&rt, cx);
    let utility = view.read_with(cx, |view, _| view.utilities.clone().unwrap());
    utility.update(cx, |utility, _| {
        utility
            .notes
            .edit(0, |document| document.replace(0, 1, "新"))
            .unwrap()
    });
    adapter
        .fail_save
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let writes = view.update(cx, |view, cx| view.drain_state(cx));
    await_writes(&rt, cx, writes);
    view.read_with(cx, |view, _| {
        assert!(view.state_write_error().is_none());
        assert!(
            view.durable_write_error()
                .unwrap()
                .contains("Synthetic disk full")
        );
    });
    adapter
        .fail_save
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let writes = view.update(cx, |view, cx| view.drain_state(cx));
    await_writes(&rt, cx, writes);
    assert!(view.read_with(cx, |view, _| view.durable_write_error().is_none()));
    assert_eq!(
        adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(channel, _)| channel == "notes:update")
            .count(),
        2
    );
}
