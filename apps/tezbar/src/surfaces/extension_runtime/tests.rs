//! Surface tests exercise the retained GPUI entity and authoritative model.
use super::*;
use ::core::prelude::v1::test;
use std::sync::Mutex;
#[derive(Default)]
struct Fake {
    calls: Mutex<Vec<(&'static str, Vec<Value>)>>,
}
impl ExtensionTransport for Fake {
    fn send(&self, r: ExtensionRequest) -> ExtensionFuture {
        self.calls.lock().unwrap().push(r.wire());
        Box::pin(async { Ok(json!({"ok":true,"mode":"unchanged"})) })
    }
    fn drain(&self) -> ExtensionFuture {
        Box::pin(async { Ok(json!(true)) })
    }
}
struct NoAssets;
impl ExtensionAssets for NoAssets {
    fn scope(&self, _: String) -> assets::AssetFuture<AssetScope> {
        Box::pin(async { Err("No fixture assets".into()) })
    }
    fn load(
        &self,
        _: AssetScope,
        _: tezbar_ui_model::extensions::images::Source,
        _: tezbar_ui_model::extensions::images::Effect,
    ) -> assets::AssetFuture<assets::AssetResult> {
        Box::pin(async { Err("No fixture assets".into()) })
    }
}
fn reply(root: Value) -> Value {
    json!({"ok":true,"mode":"view","extensionId":"fixture","commandName":"run","sessionId":"session","generation":"generation","refreshMs":null,"root":root,"actions":[]})
}
fn runtime(cx: &mut TestAppContext) -> tokio::runtime::Runtime {
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
#[gpui::test]
fn failed_activation_and_duplicate_nonce_preserve_live_screen(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fake = Arc::new(Fake::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| RuntimeView::new(fake.clone(), Arc::new(NoAssets), w, cx));
    cx.update(|w,cx|view.update(cx,|v,cx|{v.activate(reply(json!({"type":"List","children":[{"type":"List.Item","props":{"id":"a","title":"Actual row"}}]})),1,w,cx).unwrap();let old=v.session.as_ref().unwrap().key.clone();assert!(v.activate(json!({"ok":false}),2,w,cx).is_err());assert_eq!(v.session.as_ref().unwrap().key,old);v.activate(json!({"ok":false}),1,w,cx).unwrap();assert_eq!(v.rows[0].node.string("title"),Some("Actual row"));}));
    settle(&rt, cx);
    assert!(fake.calls.lock().unwrap().is_empty());
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.dispose(cx);
            v.dispose(cx);
        })
    });
    settle(&rt, cx);
    assert_eq!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| *c == "extension:dispose-session")
            .count(),
        1
    );
}
#[gpui::test]
fn form_password_local_values_validation_and_action_submit(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fake = Arc::new(Fake::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| RuntimeView::new(fake.clone(), Arc::new(NoAssets), w, cx));
    cx.update(|w,cx|view.update(cx,|v,cx|{
        let mut wire=reply(json!({"type":"Form","children":[{"type":"Form.PasswordField","props":{"id":"secret","title":"Password","required":true}},{"type":"Form.TagPicker","props":{"id":"tags"},"children":[{"type":"Form.TagPicker.Item","props":{"title":"One","value":"one"}}]}]}));wire["actions"]=json!([{"id":"submit","title":"Submit","kind":"submit-form"}]);v.activate(wire,1,w,cx).unwrap();assert!(v.fields["secret"].input.read(cx).options.password);
        v.event(UiEvent::Action("submit".into()),w,cx);assert!(v.error.is_some());assert!(v.job.is_none());
        v.event(UiEvent::ChangeField{id:"secret".into(),value:json!("synthetic-password")},w,cx);v.event(UiEvent::ChangeField{id:"tags".into(),value:json!(["one"])},w,cx);v.event(UiEvent::Action("submit".into()),w,cx);
    }));
    settle(&rt, cx);
    let calls = fake.calls.lock().unwrap();
    let a = &calls
        .iter()
        .find(|(c, _)| *c == "extension:invoke-action")
        .unwrap()
        .1[0];
    assert_eq!(a["formValues"]["secret"], "synthetic-password");
    assert_eq!(a["formValues"]["tags"], json!(["one"]));
}
#[test]
fn diagnostics_are_not_desktop_commands_through_sixty_sliding_records() {
    use tezbar_ui_model::extensions::runtime::WireEffect;
    let mut s = RuntimeSession::new(
        TreeReply::from_wire(reply(
            json!({"type":"Detail","props":{"markdown":"Records"}}),
        ))
        .unwrap(),
        0,
    )
    .unwrap();
    for end in 1u64..=60 {
        let mut updated = reply(json!({"type":"Detail","props":{"markdown":"Records"}}));
        updated["effectSequence"] = json!(end);
        updated["effects"] = serde_json::to_value(
            (end.saturating_sub(49)..=end)
                .map(|n| WireEffect::Clipboard {
                    value: format!("copy:{n}"),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let Effect::Refresh(token) = s.invalidate("session", Some("generation"), end).unwrap()
        else {
            panic!("refresh")
        };
        s.refresh_finished(&token, Some(TreeReply::from_wire(updated).unwrap()), end)
            .unwrap();
        assert_eq!(s.drain_effects().count(), 0);
    }
}
#[test]
fn row_projection_preserves_production_fallback_ids_and_section_values() {
    let s=RuntimeSession::new(TreeReply::from_wire(reply(json!({"type":"List","children":[{"type":"List.Section","props":{"title":"Group"},"children":[{"type":"List.Item","props":{"title":"No id"}}]}]}))).unwrap(),0).unwrap();
    let rows = view::rows(&s);
    assert_eq!(rows[0].id, s.filtered_ids()[0]);
    assert_eq!(rows[0].section.as_deref(), Some("Group"));
}

#[gpui::test]
fn actual_search_keys_filter_rows_and_actions_without_render_requests(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fake = Arc::new(Fake::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| RuntimeView::new(fake.clone(), Arc::new(NoAssets), w, cx));
    cx.update(|w,cx|view.update(cx,|v,cx|{
        let mut wire=reply(json!({"type":"List","props":{"searchBarPlaceholder":"Find native rows"},"children":[{"type":"List.Item","props":{"id":"a","title":"Alpha","keywords":["one"]}},{"type":"List.Item","props":{"id":"b","title":"Beta"}}]}));
        wire["actions"]=json!([{"id":"copy","title":"Copy result","kind":"copy"},{"id":"remove","title":"Remove result","style":"destructive","kind":"action"}]);v.activate(wire,1,w,cx).unwrap();
    }));
    settle(&rt, cx);
    cx.simulate_input("Beta");
    cx.run_until_parked();
    cx.read(|cx| assert_eq!(view.read(cx).rows.len(), 1));
    cx.read(|cx| assert_eq!(view.read(cx).rows[0].id, "b"));
    assert!(fake.calls.lock().unwrap().is_empty());
    cx.update(|w, cx| view.update(cx, |v, cx| v.event(UiEvent::OpenActions, w, cx)));
    cx.simulate_input("Remove");
    cx.run_until_parked();
    cx.update(|w, cx| assert!(view.read(cx).action_search.focus_handle(cx).is_focused(w)));
    cx.simulate_keystrokes("enter");
    settle(&rt, cx);
    assert_eq!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| *c == "extension:invoke-action")
            .count(),
        1
    );
}

#[gpui::test]
fn hidden_runtime_releases_owned_images_and_has_no_event_driven_timer(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fake = Arc::new(Fake::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| RuntimeView::new(fake.clone(), Arc::new(NoAssets), w, cx));
    cx.update(|w,cx|view.update(cx,|v,cx|{v.activate(reply(json!({"type":"Detail","props":{"markdown":"![Image](https://example.invalid/image.png)"}})),1,w,cx).unwrap();assert!(v.timer.is_none());v.set_visible(false,w,cx);assert!(v.scope.is_none());v.core_event(&CoreEvent::Other{channel:"extension:session-invalidated".into(),payload:json!({"sessionId":"session","generation":"generation","revision":1})},w,cx);assert!(v.job.is_none());assert!(v.timer.is_none());v.set_visible(true,w,cx);}));
    settle(&rt, cx);
    assert_eq!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| *c == "extension:refresh-session")
            .count(),
        1
    );
}

#[gpui::test]
fn queued_native_controls_keep_emit_source_and_reject_prior_visibility(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fake = Arc::new(Fake::default());
    let (view, cx) = cx.add_window_view(|w, cx| RuntimeView::new(fake, Arc::new(NoAssets), w, cx));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let _sub = cx.update(|_, cx| {
        cx.subscribe(&view, move |_, e: &ExtensionEffectEmission, _| {
            record.lock().unwrap().push(e.generation);
        })
    });
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.activate(
                reply(json!({"type":"Detail","props":{"markdown":"Fixture"}})),
                1,
                w,
                cx,
            )
            .unwrap();
            let old = v.host_generation();
            v.set_visible(false, w, cx);
            v.set_visible(true, w, cx);
            v.core_event(
                &CoreEvent::Other {
                    channel: "extension:ui".into(),
                    payload: json!({"sessionId":"session","source":old,"kind":"close"}),
                },
                w,
                cx,
            );
        })
    });
    settle(&rt, cx);
    assert!(seen.lock().unwrap().is_empty());
    let expected = cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let source = v.host_generation();
            v.core_event(
                &CoreEvent::Other {
                    channel: "extension:ui".into(),
                    payload: json!({"sessionId":"session","source":source,"kind":"preferences"}),
                },
                w,
                cx,
            );
            v.set_visible(false, w, cx);
            v.set_visible(true, w, cx);
            source
        })
    });
    settle(&rt, cx);
    assert_eq!(*seen.lock().unwrap(), vec![expected]);
}
#[gpui::test]
fn native_wheel_and_file_results_use_actual_form_callback_and_visibility_tickets(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let fake = Arc::new(Fake::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| RuntimeView::new(fake.clone(), Arc::new(NoAssets), w, cx));
    cx.update(|w,cx|view.update(cx,|v,cx|{
        let mut wire=reply(json!({"type":"Form","props":{"nativeColorWheel":{"hue":0,"saturation":100,"value":100,"alpha":1,"changeActionId":"wheel"}},"children":[{"type":"Form.FilePicker","props":{"id":"files","allowMultipleSelection":true}}]}));
        wire["actions"]=json!([{"id":"wheel","title":"Update wheel"}]);v.activate(wire,1,w,cx).unwrap();
        v.wheel_update([120.,30.,40.,1.],w,cx);
    }));
    settle(&rt, cx);
    assert!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, a)| *c == "extension:invoke-action"
                && a[0]["formValues"]["hue"] == 120.
                && a[0]["formValues"]["saturation"] == 30.)
    );
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.event(UiEvent::PickFiles("files".into()), w, cx);
            let ticket = v.next_ticket;
            v.set_visible(false, w, cx);
            v.set_visible(true, w, cx);
            v.files_chosen(ticket, Ok(vec!["/fixture/late".into()]), w, cx);
            assert_eq!(
                v.session.as_ref().unwrap().form.values()["files"],
                json!([])
            );
            v.event(UiEvent::PickFiles("files".into()), w, cx);
            let ticket = v.next_ticket;
            v.files_chosen(
                ticket,
                Ok(vec!["/fixture/a".into(), "/fixture/b".into()]),
                w,
                cx,
            );
            assert_eq!(
                v.session.as_ref().unwrap().form.values()["files"],
                json!(["/fixture/a", "/fixture/b"])
            );
        })
    });
}

#[gpui::test]
fn color_detail_formats_use_canonical_rust_rows_and_full_owned_copy(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fake = Arc::new(Fake::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| RuntimeView::new(fake.clone(), Arc::new(NoAssets), w, cx));
    cx.update(|w,cx|view.update(cx,|v,cx|{
        v.activate(reply(json!({"type":"Detail","props":{"markdown":"![Color wheel](pixel.png)","initialColor":{"red":0,"green":1,"blue":0,"alpha":0.5}}})),1,w,cx).unwrap();
        assert_eq!(v.color_formats().len(),9);
        assert_eq!(v.color_formats()[0].value,"#00FF00");
        assert_eq!(v.color_formats()[5].value,"rgba(0, 255, 0, 0.5)");
        v.copy_color_format("rgba",cx);
        v.wheel_update([240.,100.,100.,0.5],w,cx);
        assert_eq!(v.color_formats()[0].value,"#0000FF");
    }));
    settle(&rt, cx);
    assert!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .any(|(channel, args)| *channel == "clipboard:write"
                && args[0] == "rgba(0, 255, 0, 0.5)")
    );
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.set_visible(false, w, cx);
            v.copy_color_format("hex", cx);
        })
    });
    assert_eq!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| *c == "clipboard:write")
            .count(),
        1
    );
}

#[gpui::test]
fn review_m9_action_panel_scrolls_across_sections_and_resets_on_reopen(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fake = Arc::new(Fake::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| RuntimeView::new(fake.clone(), Arc::new(NoAssets), w, cx));
    let mut wire = reply(json!({"type":"Detail","props":{"markdown":"Actions"}}));
    wire["actions"]=json!((0..24).map(|i|json!({"id":format!("a{i}"),"title":format!("Action {i}"),"section":if i<12 {"First"} else {"Second"}})).collect::<Vec<_>>());
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.activate(wire, 1, w, cx).unwrap();
            v.event(UiEvent::OpenActions, w, cx);
        })
    });
    settle(&rt, cx);
    for _ in 0..18 {
        cx.simulate_keystrokes("down");
    }
    settle(&rt, cx);
    view.read_with(cx, |v, cx| {
        assert_eq!(v.action_index, 18);
        let scroll = v.action_scroll.0.borrow();
        let offset = f32::from(scroll.base_handle.offset().y);
        let height = f32::from(scroll.base_handle.bounds().size.height);
        let selected_row = v.action_rows_for_test_index(cx);
        assert!(offset < 0., "selected action must scroll its real viewport");
        let top = selected_row as f32 * tezbar_ui::tokens::ROW + offset;
        assert!(
            top >= 0. && top + tezbar_ui::tokens::ROW <= height + 1.,
            "selected action outside viewport: {top}/{height}"
        );
    });
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.event(UiEvent::CloseActions, w, cx);
            v.event(UiEvent::OpenActions, w, cx);
        })
    });
    settle(&rt, cx);
    view.read_with(cx, |v, _| {
        assert_eq!(v.action_index, 0);
        assert_eq!(
            f32::from(v.action_scroll.0.borrow().base_handle.offset().y),
            0.
        );
    });
}
#[gpui::test]
fn review_m9_dropdown_keeps_groups_current_choice_keyboard_and_escape(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fake = Arc::new(Fake::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| RuntimeView::new(fake.clone(), Arc::new(NoAssets), w, cx));
    let wire = reply(
        json!({"type":"List","props":{"searchBarAccessory":{"type":"List.Dropdown","props":{"actionId":"filter","value":"jp"},"children":[{"type":"List.Dropdown.Section","props":{"title":"Countries"},"children":[{"type":"List.Dropdown.Item","props":{"title":"Kazakhstan","value":"kz"}},{"type":"List.Dropdown.Item","props":{"title":"Japan","value":"jp"}}]}]}},"children":[{"type":"List.Item","props":{"id":"a","title":"A"}}]}),
    );
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.activate(wire, 1, w, cx).unwrap();
            let (_, selected, items) = v.dropdown_choices().unwrap();
            assert_eq!(selected, "jp");
            assert_eq!(items[0].0.as_deref(), Some("Countries"));
            v.dropdown_focus.focus(w, cx);
            cx.notify();
        })
    });
    settle(&rt, cx);
    let enter = Keystroke::parse("enter").unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: enter.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent {
        keystroke: enter.clone(),
    });
    assert!(
        view.read_with(cx, |v, _| v.dropdown_open),
        "native keyboard click opens the current-choice dropdown exactly once"
    );
    settle(&rt, cx);
    cx.simulate_keystrokes("up");
    cx.simulate_event(KeyDownEvent {
        keystroke: enter.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke: enter });
    settle(&rt, cx);
    assert!(!view.read_with(cx, |v, _| v.dropdown_open));
    assert!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .any(|(channel, args)| *channel == "extension:invoke-action"
                && args[0]["actionId"] == "filter"
                && args[0]["formValues"]["value"] == "kz")
    );
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.dropdown_open = true;
            cx.notify();
        })
    });
    cx.simulate_keystrokes("escape");
    assert!(!view.read_with(cx, |v, _| v.dropdown_open));
}
#[gpui::test]
fn review_m9_grid_sections_use_same_ranges_for_render_selection_and_leases(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let fake = Arc::new(Fake::default());
    let (view, cx) = cx.add_window_view(|w, cx| RuntimeView::new(fake, Arc::new(NoAssets), w, cx));
    let section = |title: &str, start: usize, n: usize| json!({"type":"Grid.Section","props":{"title":title},"children":(start..start+n).map(|i|json!({"type":"Grid.Item","props":{"id":format!("g{i}"),"title":format!("Item {i}")}})).collect::<Vec<_>>()});
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.activate(
                reply(
                    json!({"type":"Grid","children":[section("First",0,2),section("Second",2,6)]}),
                ),
                1,
                w,
                cx,
            )
            .unwrap()
        })
    });
    settle(&rt, cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.grid_ranges()),
        vec![0..2, 2..6, 6..8]
    );
    cx.simulate_keystrokes("down");
    assert_eq!(
        view.read_with(cx, |v, _| v.session.as_ref().unwrap().selection.clone()),
        Some("g2".into())
    );
}
