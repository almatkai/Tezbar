use super::*;
use ::core::prelude::v1::test;
use std::sync::Mutex;
#[derive(Default)]
struct Fake(Mutex<Vec<&'static str>>);
impl ExtensionTransport for Fake {
    fn send(&self, r: ExtensionRequest) -> ExtensionFuture {
        let (c, _) = r.wire();
        self.0.lock().unwrap().push(c);
        Box::pin(async move {
            Ok(if c == "extension:list" {
                json!([])
            } else {
                json!({"items":[{"id":"fixture","name":"Fixture","runtime":"javascript"}],"total":1,"hasMore":false})
            })
        })
    }
    fn drain(&self) -> ExtensionFuture {
        Box::pin(async { Ok(json!(true)) })
    }
}
#[gpui::test]
fn construct_and_render_do_not_start_store_requests(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    let (view, cx) = cx.add_window_view(|w, cx| StoreView::new(fake.clone(), w, cx));
    cx.run_until_parked();
    assert!(fake.0.lock().unwrap().is_empty());
    cx.update(|_, cx| view.update(cx, |v, cx| v.dispose(cx)));
    assert!(fake.0.lock().unwrap().is_empty());
}
#[test]
fn installed_wire_normalizes_and_invalid_data_is_rejected() {
    assert!(decode_installed(json!({"ok":false})).is_err());
    let rows =
        decode_installed(json!([{"id":"fixture","name":"Fixture","iconPath":"assets/icon.png"}]))
            .unwrap();
    assert_eq!(rows[0].author, "Raycast Community");
    assert_eq!(rows[0].metadata["iconUrl"], "assets/icon.png");
}
#[test]
fn wire_payload_debug_is_redacted_without_truncating_copy() {
    let text = "x".repeat(10000);
    let r = ExtensionRequest::Copy(text.clone());
    assert!(!format!("{r:?}").contains("xxxxx"));
    assert_eq!(r.wire().1[0], text);
    assert!(
        success(&json!({"ok":false,"message":"synthetic secret"}))
            .unwrap_err()
            .find("synthetic secret")
            .is_none()
    );
}
#[gpui::test]
fn remove_confirmation_defaults_to_cancel_and_traps_tab(cx: &mut TestAppContext) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| {
        gpui_tokio::init_from_handle(cx, rt.handle().clone());
        tezbar_ui::text_input::bind_keys(cx);
    });
    let fake = Arc::new(Fake::default());
    let (view, cx) = cx.add_window_view(|w, cx| StoreView::new(fake.clone(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.action(view::Action::Remove("fixture".into()), w, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|w, cx| assert!(view.read(cx).cancel_focus.is_focused(w)));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.read(|cx| assert!(view.read(cx).remove.is_none()));
    assert!(fake.0.lock().unwrap().is_empty());
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.action(view::Action::Remove("fixture".into()), w, cx)
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("shift-tab");
    cx.update(|w, cx| assert!(view.read(cx).confirm_focus.is_focused(w)));
    cx.simulate_keystrokes("tab");
    cx.update(|w, cx| assert!(view.read(cx).cancel_focus.is_focused(w)));
    cx.simulate_keystrokes("escape");
    assert!(fake.0.lock().unwrap().is_empty());
}
