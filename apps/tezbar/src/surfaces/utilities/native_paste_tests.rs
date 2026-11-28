use super::*;
use ::core::prelude::v1::test;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Default)]
struct ClipboardRace {
    text: Arc<Mutex<String>>,
    gate: Arc<tokio::sync::Notify>,
    reads: AtomicUsize,
}
impl UtilityTransport for ClipboardRace {
    fn send(
        &self,
        request: UtilityRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Reply> + Send>> {
        if matches!(request, UtilityRequest::ReadText) {
            self.reads.fetch_add(1, Ordering::SeqCst);
            let gate = self.gate.clone();
            let text = self.text.clone();
            return Box::pin(async move {
                gate.notified().await;
                Ok(serde_json::json!(text.lock().unwrap().clone()))
            });
        }
        let (channel, _) = request.wire();
        Box::pin(async move {
            Ok(if channel == "notes:list" {
                serde_json::json!([{"createdAt":11,"updatedAt":12,"text":"<div>old</div>"}])
            } else {
                serde_json::json!([])
            })
        })
    }
    fn release(
        &self,
        _: String,
        _: &'static str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send>> {
        Box::pin(async { Err("No network in native paste fixture".into()) })
    }
}
fn settle(rt: &tokio::runtime::Runtime, cx: &mut VisualTestContext) {
    rt.block_on(async {
        for _ in 0..32 {
            cx.run_until_parked();
            tokio::task::yield_now().await;
        }
    });
}
#[gpui::test]
fn utility_search_paste_captures_gesture_clipboard_before_it_changes(cx: &mut TestAppContext) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| {
        gpui_tokio::init_from_handle(cx, rt.handle().clone());
        tezbar_ui::text_input::bind_keys(cx);
    });
    let fixture = Arc::new(ClipboardRace::default());
    *fixture.text.lock().unwrap() = "bugreport".into();
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.navigate(
                Navigation {
                    nonce: 1,
                    surface: Surface::Snippets,
                    payload: Payload::None,
                },
                w,
                cx,
            );
            v.search
                .update(cx, |input, cx| input.set_text("----123", w, cx));
            v.search.focus_handle(cx).focus(w, cx);
            cx.write_to_clipboard(ClipboardItem::new_string("bugreport".into()));
        })
    });
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-a cmd-v"
    } else {
        "ctrl-a ctrl-v"
    });
    cx.update(|_, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("restored-after-gesture".into()))
    });
    *fixture.text.lock().unwrap() = "restored-after-gesture".into();
    fixture.gate.notify_one();
    settle(&rt, cx);
    assert_eq!(
        view.read_with(cx, |v, cx| v.search.read(cx).content.to_string()),
        "bugreport",
        "Paste must capture the clipboard belonging to its input gesture"
    );
    assert_eq!(
        fixture.reads.load(Ordering::SeqCst),
        0,
        "Native text paste must not queue a later core clipboard read"
    );
}

#[gpui::test]
fn note_editor_paste_captures_plain_clipboard_before_it_changes(cx: &mut TestAppContext) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| {
        gpui_tokio::init_from_handle(cx, rt.handle().clone());
        tezbar_ui::text_input::bind_keys(cx);
    });
    let fixture = Arc::new(ClipboardRace::default());
    *fixture.text.lock().unwrap() = "<b>gesture text</b>".into();
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.navigate(
                Navigation {
                    nonce: 1,
                    surface: Surface::Notes,
                    payload: Payload::Note { created_at: 11 },
                },
                w,
                cx,
            )
        })
    });
    settle(&rt, cx);
    let input = view.read_with(cx, |v, cx| v.editor.read(cx).input.clone());
    cx.update(|w, cx| {
        view.update(cx, |v, cx| v.act(Action::EditNote(true), w, cx));
        assert!(input.focus_handle(cx).is_focused(w));
        cx.write_to_clipboard(ClipboardItem::new_string("<b>gesture text</b>".into()));
    });
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-a cmd-v"
    } else {
        "ctrl-a ctrl-v"
    });
    cx.update(|_, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("restored-after-gesture".into()))
    });
    *fixture.text.lock().unwrap() = "restored-after-gesture".into();
    fixture.gate.notify_one();
    settle(&rt, cx);
    let html = view.read_with(cx, |v, _| v.notes.document().unwrap().html());
    assert!(
        html.contains("&lt;b&gt;gesture text&lt;/b&gt;"),
        "Native note paste must retain plain-text escaping and the gesture clipboard"
    );
    assert!(!html.contains("restored-after-gesture"));
    assert_eq!(fixture.reads.load(Ordering::SeqCst), 0);
}

#[gpui::test]
fn snippet_field_native_paste_preserves_selection_unicode_and_followup_edit(
    cx: &mut TestAppContext,
) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| {
        gpui_tokio::init_from_handle(cx, rt.handle().clone());
        tezbar_ui::text_input::bind_keys(cx);
    });
    let fixture = Arc::new(ClipboardRace::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.navigate(
                Navigation {
                    nonce: 1,
                    surface: Surface::Snippets,
                    payload: Payload::None,
                },
                w,
                cx,
            )
        })
    });
    settle(&rt, cx);
    cx.update(|w, cx| view.update(cx, |v, cx| v.act(Action::New, w, cx)));
    let field = view.read_with(cx, |v, _| v.fields[0].clone());
    cx.update(|w, cx| {
        field.update(cx, |input, cx| input.set_text("old title", w, cx));
        field.focus_handle(cx).focus(w, cx);
        cx.write_to_clipboard(ClipboardItem::new_string("Draft α世界".into()));
    });
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-a cmd-v"
    } else {
        "ctrl-a ctrl-v"
    });
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("changed".into())));
    cx.simulate_input(" + edit");
    settle(&rt, cx);
    assert_eq!(
        field.read_with(cx, |field, _| field.content.to_string()),
        "Draft α世界 + edit"
    );
    assert_eq!(fixture.reads.load(Ordering::SeqCst), 0);
    cx.update(|w, cx| view.update(cx, |v, cx| v.set_visible(false, w, cx)));
    field.update(cx, |_, cx| {
        cx.emit(tezbar_ui::text_input::ClipboardEffect::Read)
    });
    settle(&rt, cx);
    assert_eq!(
        field.read_with(cx, |field, _| field.content.to_string()),
        "Draft α世界 + edit",
        "Hidden owner must not paste a later clipboard value"
    );
}
