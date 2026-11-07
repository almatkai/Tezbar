//! Promoted real-keyboard dropdown probes from the independent fec3257 review.
use super::*;
use ::core::prelude::v1::test;
use std::sync::Mutex;
fn gpui_rt(cx: &mut TestAppContext) -> tokio::runtime::Runtime {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| {
        gpui_tokio::init_from_handle(cx, rt.handle().clone());
        tezbar_ui::text_input::bind_keys(cx);
    });
    rt
}
fn settle(rt: &tokio::runtime::Runtime, cx: &mut VisualTestContext) {
    rt.block_on(async {
        for _ in 0..128 {
            cx.run_until_parked();
            tokio::task::yield_now().await;
        }
    });
}
struct NoAssets;
impl ExtensionAssets for NoAssets {
    fn scope(&self, _: String) -> assets::AssetFuture<AssetScope> {
        Box::pin(async { Err("no assets in probe".into()) })
    }
    fn load(
        &self,
        _: AssetScope,
        _: tezbar_ui_model::extensions::images::Source,
        _: tezbar_ui_model::extensions::images::Effect,
    ) -> assets::AssetFuture<assets::AssetResult> {
        Box::pin(async { Err("no assets in probe".into()) })
    }
}
/// Records requests; answers an invoke with a scripted re-render (as the worker would).
#[derive(Default)]
struct Scripted {
    calls: Mutex<Vec<(&'static str, Vec<Value>)>>,
    invoke_reply: Mutex<Option<Value>>,
}
impl ExtensionTransport for Scripted {
    fn send(&self, r: ExtensionRequest) -> ExtensionFuture {
        let (channel, args) = r.wire();
        self.calls.lock().unwrap().push((channel, args));
        let reply = if channel == "extension:invoke-action" {
            self.invoke_reply.lock().unwrap().clone()
        } else {
            None
        };
        Box::pin(async move { Ok(reply.unwrap_or_else(|| json!({"ok":true,"mode":"unchanged"}))) })
    }
    fn drain(&self) -> ExtensionFuture {
        Box::pin(async { Ok(json!(true)) })
    }
}
fn view_reply(root: Value) -> Value {
    json!({"ok":true,"mode":"view","extensionId":"probe","commandName":"probe","sessionId":"session","generation":"session","refreshMs":null,"viewRevision":0,"root":root,"actions":[]})
}

fn dropdown_list(accessory_props: Value, rows: &[&str]) -> Value {
    view_reply(
        json!({"type":"List","props":{"searchBarAccessory":{"type":"List.Dropdown","props":accessory_props,"children":[{"type":"List.Dropdown.Section","props":{"title":"Countries"},"children":[
        {"type":"List.Dropdown.Item","props":{"title":"Kazakhstan","value":"kz"}},
        {"type":"List.Dropdown.Item","props":{"title":"Japan","value":"jp"}}]}]}},
        "children":rows.iter().map(|r| json!({"type":"List.Item","props":{"id":r,"title":r}})).collect::<Vec<_>>()}),
    )
}
fn press_enter(cx: &mut VisualTestContext) {
    let enter = Keystroke::parse("enter").unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: enter.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke: enter });
}
/// Same native keyboard path as the root's own D12 test: focus trigger, Enter opens,
/// arrow moves, Enter chooses (RuntimeView::key -> choose_dropdown -> UiEvent::Dropdown).
fn choose_dropdown_by_keyboard(
    view: &Entity<RuntimeView>,
    rt: &tokio::runtime::Runtime,
    cx: &mut VisualTestContext,
    step: &str,
) {
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.dropdown_focus.focus(w, cx);
            cx.notify();
        })
    });
    settle(rt, cx);
    press_enter(cx);
    settle(rt, cx);
    assert!(
        view.read_with(cx, |v, _| v.dropdown_open),
        "probe precondition: keyboard opened dropdown"
    );
    cx.simulate_keystrokes(step);
    press_enter(cx);
    settle(rt, cx);
}
/// Uncontrolled Raycast dropdown (defaultValue + onChange, no `value`). The worker
/// re-serializes the SAME props after onChange (extension-runner.ts:3514-3527). Intended
/// (D12 "current value" + TS list-runtime.tsx:541-567 local accessoryValue): after the
/// user picks Kazakhstan the trigger and selected option show Kazakhstan.
#[gpui::test]
fn review_m9_followup_d12_uncontrolled_dropdown_shows_users_choice(cx: &mut TestAppContext) {
    let rt = gpui_rt(cx);
    let transport = Arc::new(Scripted::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| RuntimeView::new(transport.clone(), Arc::new(NoAssets), w, cx));
    let props = json!({"actionId":"filter","defaultValue":"jp"});
    *transport.invoke_reply.lock().unwrap() = Some(dropdown_list(props.clone(), &["kz-row"]));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.activate(dropdown_list(props, &["jp-row"]), 1, w, cx)
                .unwrap()
        })
    });
    settle(&rt, cx);
    let before = view.read_with(cx, |v, _| v.dropdown_choices().map(|c| c.1));
    choose_dropdown_by_keyboard(&view, &rt, cx, "up"); // jp -> kz
    let invoked = transport.calls.lock().unwrap().iter().any(|(c, a)| {
        *c == "extension:invoke-action"
            && a[0]["actionId"] == "filter"
            && a[0]["formValues"]["value"] == "kz"
    });
    let after = view.read_with(cx, |v, _| {
        (
            v.dropdown_choices().map(|c| c.1),
            v.rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
        )
    });
    eprintln!(
        "OPUS-D12 uncontrolled defaultValue: before={before:?} invoked_kz={invoked} after(selected,rows)={after:?}"
    );
    assert!(
        invoked,
        "probe precondition: actual onChange callback invoked with kz"
    );
    assert_eq!(
        after.1,
        vec!["kz-row".to_owned()],
        "probe precondition: re-render applied"
    );
    assert_eq!(
        after.0.as_deref(),
        Some("kz"),
        "dropdown trigger reverted to defaultValue after the user's choice"
    );
}
/// Same without defaultValue: TS shows the chosen option; native falls back to the first.
#[gpui::test]
fn review_m9_followup_d12_uncontrolled_dropdown_without_default_shows_users_choice(
    cx: &mut TestAppContext,
) {
    let rt = gpui_rt(cx);
    let transport = Arc::new(Scripted::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| RuntimeView::new(transport.clone(), Arc::new(NoAssets), w, cx));
    let props = json!({"actionId":"filter"});
    *transport.invoke_reply.lock().unwrap() = Some(dropdown_list(props.clone(), &["jp-row"]));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.activate(dropdown_list(props, &["kz-row"]), 1, w, cx)
                .unwrap()
        })
    });
    settle(&rt, cx);
    choose_dropdown_by_keyboard(&view, &rt, cx, "down"); // kz (first) -> jp
    let invoked = transport
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|(c, a)| *c == "extension:invoke-action" && a[0]["formValues"]["value"] == "jp");
    assert!(
        invoked,
        "probe precondition: actual onChange callback invoked with jp"
    );
    let after = view.read_with(cx, |v, _| {
        (
            v.dropdown_choices().map(|c| c.1),
            v.rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
        )
    });
    eprintln!("OPUS-D12 uncontrolled no default: after(selected,rows)={after:?}");
    assert_eq!(
        after.1,
        vec!["jp-row".to_owned()],
        "probe precondition: re-render applied"
    );
    assert_eq!(
        after.0.as_deref(),
        Some("jp"),
        "dropdown trigger fell back to the first option after the user chose Japan"
    );
}
/// Control: controlled dropdown whose worker re-render carries the new `value` is correct.
#[gpui::test]
fn review_m9_followup_d12_controlled_dropdown_control(cx: &mut TestAppContext) {
    let rt = gpui_rt(cx);
    let transport = Arc::new(Scripted::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| RuntimeView::new(transport.clone(), Arc::new(NoAssets), w, cx));
    *transport.invoke_reply.lock().unwrap() = Some(dropdown_list(
        json!({"actionId":"filter","value":"kz"}),
        &["kz-row"],
    ));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.activate(
                dropdown_list(json!({"actionId":"filter","value":"jp"}), &["jp-row"]),
                1,
                w,
                cx,
            )
            .unwrap()
        })
    });
    settle(&rt, cx);
    choose_dropdown_by_keyboard(&view, &rt, cx, "up"); // jp -> kz
    let after = view.read_with(cx, |v, _| v.dropdown_choices().map(|c| c.1));
    eprintln!("OPUS-D12 controlled control: after={after:?}");
    assert_eq!(after.as_deref(), Some("kz"));
}
