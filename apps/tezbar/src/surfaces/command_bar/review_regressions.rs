// Included in tests.rs to exercise the same real-view fixture and transport contracts.
struct ReviewRuns {
    base: Loopback,
    extension: Value,
}
impl RequestAdapter for ReviewRuns {
    fn send(
        &self,
        request: Request,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>> {
        let result = match &request {
            Request::ConfirmQuit => json!(false),
            Request::ExtensionRun(_) => self.extension.clone(),
            _ => return self.base.send(request),
        };
        let (channel, args) = request.wire();
        self.base.calls.lock().unwrap().push((channel.into(), args));
        Box::pin(async move { Ok(result) })
    }
}
#[gpui::test]
fn review_directory_ai_prompt_preserves_greater_than(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let (relay, _rx) = Relay::new();
    let (view, cx) = cx.add_window_view(|w, cx| {
        Launcher::with_adapter(Arc::new(Loopback::default()), relay, None, w, cx)
    });
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.ready = true;
            v.show(w, cx);
        })
    });
    cx.simulate_input("/fixture  why is a ");
    cx.simulate_input("> b");
    settle(&rt, cx);
    view.read_with(cx, |v, _| {
        assert_eq!(
            launcher::input_mode(&v.model.input, v.model.terminal_mode).mode,
            Mode::Ai
        );
        assert_eq!(v.model.input, "/fixture  why is a > b");
        assert!(!v.model.terminal_mode);
        assert!(v.terminal_cwd.is_none());
    });
}
#[gpui::test]
fn review_cancelled_quit_and_update_keep_the_current_query_usable(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(ReviewRuns {
        base: Loopback::default(),
        extension: json!({"ok":true}),
    });
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    for command in ["quit-tezbar", "check-for-updates"] {
        cx.update(|w, cx| {
            view.update(cx, |v, cx| {
                seed(v, vec![Row::command(command, "Local route")]);
                v.model.input = "update".into();
                v.model.session.query = "update".into();
                v.input
                    .update(cx, |field, cx| field.set_text("update", w, cx));
                v.input.focus_handle(cx).focus(w, cx);
            })
        });
        cx.simulate_keystrokes("enter");
        settle(&rt, cx);
        view.read_with(cx, |v, _| {
            assert!(v.model.visible);
            assert!(!v.model.session.pending());
            assert!(!v.model.session.activating);
            assert_eq!(v.model.input, "update");
        });
    }
    assert_eq!(
        adapter
            .base
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(channel, _)| channel == "app:confirm-quit")
            .count(),
        1
    );
    view.update(cx, |v, cx| {
        v.core_event(
            &CoreEvent::Other {
                channel: "update-status".into(),
                payload: json!({"kind":"checking"}),
            },
            cx,
        );
        let mut pin = Row::command("review-pin", "Saved action");
        pin.action = Action::CopyText {
            text: "synthetic".into(),
        };
        ui_state::insert_pin(&mut v.state.pins, &pin, None);
    });
    assert!(view.read_with(cx, |v, _| {
        v.rows.iter().any(|row| row.title.contains("Checking"))
    }));
    cx.simulate_keystrokes("alt-1");
    settle(&rt, cx);
    assert_eq!(
        adapter
            .base
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(channel, args)| channel == "search:execute"
                && args[0]["context"]["resultId"] == "command:review-pin")
            .count(),
        1
    );
}
fn review_hotkey_result(result: Value, expected_view: bool, cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(ReviewRuns {
        base: Loopback::default(),
        extension: result,
    });
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    view.update(cx, |v, cx| {
        v.ready = true;
        v.core_event(
            &CoreEvent::Other {
                channel: "run-extension-command-from-hotkey".into(),
                payload: json!({"extensionId":"fixture","commandName":"fixture"}),
            },
            cx,
        );
    });
    settle(&rt, cx);
    assert_eq!(
        view.read_with(cx, |v, _| v
            .navigation
            .as_ref()
            .is_some_and(|n| n.surface == Surface::ExtensionRuntime)),
        expected_view
    );
    if !expected_view {
        cx.update(|w, cx| view.update(cx, |v, cx| v.show(w, cx)));
        cx.simulate_input("abc");
        settle(&rt, cx);
        assert_eq!(view.read_with(cx, |v, _| v.model.input.clone()), "abc");
    }
    assert_eq!(
        adapter
            .base
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(channel, _)| channel == "extension:run-command")
            .count(),
        1
    );
    assert!(
        !adapter
            .base
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|(channel, _)| channel == "search:execute")
    );
}
#[gpui::test]
fn review_hotkey_no_view_does_not_hijack_next_open(cx: &mut TestAppContext) {
    review_hotkey_result(json!({"ok":true,"message":"Done"}), false, cx);
}
#[gpui::test]
fn review_hotkey_failed_result_does_not_open_runtime(cx: &mut TestAppContext) {
    review_hotkey_result(
        json!({"ok":false,"mode":"view","message":"Synthetic failure"}),
        false,
        cx,
    );
}
#[gpui::test]
fn review_hotkey_view_result_keeps_runtime_handoff(cx: &mut TestAppContext) {
    review_hotkey_result(
        json!({"ok":true,"mode":"view","sessionId":"fixture"}),
        true,
        cx,
    );
}
#[gpui::test]
fn review_pin_click_after_reorder_runs_on_first_click(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Fixture::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    view.update(cx, |v, cx| {
        let a = Row::command("a", "First");
        let b = Row::command("b", "Second");
        seed(v, vec![a.clone(), b.clone()]);
        v.model.input.clear();
        ui_state::insert_pin(&mut v.state.pins, &a, None);
        ui_state::insert_pin(&mut v.state.pins, &b, None);
        cx.notify();
    });
    cx.run_until_parked();
    let a = cx.debug_bounds("pin-0").unwrap().center();
    let b = cx.debug_bounds("pin-1").unwrap().center();
    cx.simulate_mouse_down(a, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(b, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(b, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    let first = cx.debug_bounds("pin-0").unwrap().center();
    cx.simulate_click(first, Modifiers::default());
    settle(&rt, cx);
    assert_eq!(
        adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(channel, _)| channel == "search:execute")
            .count(),
        1
    );
}
#[derive(Default)]
struct ReviewFileRows {
    calls: std::sync::Mutex<Vec<(String, Vec<Value>)>>,
}
impl RequestAdapter for ReviewFileRows {
    fn send(
        &self,
        request: Request,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>> {
        let (channel, args) = request.wire();
        self.calls.lock().unwrap().push((channel.into(), args));
        let result = match channel {
            "search:all" => json!(
                (0..20)
                    .map(|index| {
                        let mut row = Row::command(&format!("new-{index}"), "File");
                        row.action = Action::OpenFile {
                            path: format!("/synthetic/new/{index}.txt"),
                        };
                        row
                    })
                    .collect::<Vec<_>>()
            ),
            "asset-icon:data-url" => Value::Null,
            _ => json!({"ok":true}),
        };
        Box::pin(async move { Ok(result) })
    }
}
#[gpui::test]
fn review_icon_frame_tracks_the_latest_generation(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(ReviewFileRows::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    view.update(cx, |v, cx| {
        seed(
            v,
            (0..20)
                .map(|index| {
                    let mut row = Row::command(&format!("old-{index}"), "File");
                    row.action = Action::OpenFile {
                        path: format!("/synthetic/old/{index}.txt"),
                    };
                    row
                })
                .collect(),
        );
        v.defer_icon_viewport(cx);
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|w, cx| w.simulate_next_frame(cx));
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.schedule_icon_viewport(w, cx);
            v.dispatch(Input::Invalidate { text: None }, cx);
        })
    });
    cx.update(|w, cx| w.simulate_next_frame(cx));
    cx.executor().advance_clock(Duration::from_millis(20));
    settle(&rt, cx);
    for _ in 0..4 {
        cx.update(|w, cx| w.simulate_next_frame(cx));
        settle(&rt, cx);
    }
    assert!(adapter.calls.lock().unwrap().iter().any(|(channel, args)| {
        channel == "asset-icon:data-url"
            && args[0]["path"]
                .as_str()
                .is_some_and(|path| path.contains("/new/"))
    }));
}
#[gpui::test]
fn review_optional_only_arguments_execute_without_a_form(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Fixture::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w,cx|view.update(cx,|v,cx|{
        let mut row=Row::command("optional","Optional");
        row.action=Action::RunExtensionCommand{extension_id:"fixture".into(),command_name:"optional".into(),extra:serde_json::from_value(json!({"commandArgumentDefinitions":[{"name":"a","required":false},{"name":"b","required":false}]})).unwrap()};
        seed(v,vec![row]);v.input.focus_handle(cx).focus(w,cx);
    }));
    cx.simulate_keystrokes("enter");
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| v.form.is_none()));
    assert_eq!(
        adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(channel, _)| channel == "extension:run-command")
            .count(),
        1
    );
}
