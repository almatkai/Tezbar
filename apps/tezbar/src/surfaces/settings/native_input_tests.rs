use super::*;
use ::core::prelude::v1::test;

#[gpui::test]
fn settings_field_native_clipboard_reaches_retained_value(cx: &mut TestAppContext) {
    let rt = tests::runtime(cx);
    let store = SettingsStore::new(Arc::new(tests::Fixture::default()));
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store, &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate(w, cx)));
    tests::settle(&rt, cx);
    cx.update(|w, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("key-Қазақ-😀".into()));
        view.update(cx, |v, cx| {
            v.action(Action::Tab(Tab::Ai), w, cx);
            v.action(Action::Provider("openai".into()), w, cx);
            cx.notify();
        });
    });
    tests::settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let input = v.fields["provider.openai.apiKey"].input.clone();
            input.update(cx, |f, cx| w.focus(&f.focus_handle(cx), cx));
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-a cmd-v"
    } else {
        "ctrl-a ctrl-v"
    });
    tests::settle(&rt, cx);
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).fields["provider.openai.apiKey"].value,
            "key-Қазақ-😀"
        )
    });
}
