#[gpui::test]
fn session_configure_native_clipboard_reaches_retained_name(cx: &mut TestAppContext) {
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| TerminalView::new(fixture, &UiState::default(), w, cx));
    cx.update(|w, cx| {
        tezbar_ui::text_input::bind_keys(cx);
        cx.write_to_clipboard(ClipboardItem::new_string("Session Қазақ 😀".into()));
        view.update(cx, |v, cx| {
            v.state.visible = true;
            v.state.configure = Some(state::Configure {
                id: "fixture".into(),
                generation: 1,
                name: "old".into(),
                defaults: Defaults::default(),
            });
            cx.notify();
            v.name.update(cx, |f, cx| {
                f.set_text("old", w, cx);
                w.focus(&f.focus_handle(cx), cx);
            });
        });
    });
    cx.run_until_parked();
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-a cmd-v"
    } else {
        "ctrl-a ctrl-v"
    });
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).state.configure.as_ref().unwrap().name,
            "Session Қазақ 😀"
        )
    });
}

#[gpui::test]
fn configure_close_restores_terminal_focus_and_transport(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| TerminalView::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.listeners_ready(true, true, cx);
            v.activate(payload(), 1, w, cx);
        })
    });
    settle(&rt, cx);
    for action in ["cancel", "escape", "save"] {
        cx.update(|w, cx| {
            view.update(cx, |v, cx| {
                v.act(input::KeyResult::Configure, w, cx);
                v.name.update(cx, |f, cx| w.focus(&f.focus_handle(cx), cx));
            })
        });
        settle(&rt, cx);
        if action != "escape" {
            let selector = if action == "save" {
                "terminal-config-save"
            } else {
                "terminal-config-cancel"
            };
            let button = cx.debug_bounds(selector).expect("Configure button visible");
            cx.simulate_click(button.center(), Modifiers::default());
        } else {
            cx.simulate_keystrokes("escape");
        }
        settle(&rt, cx);
        cx.update(|w, cx| {
            assert!(view.read(cx).state.configure.is_none());
            assert!(
                view.read(cx).focus.is_focused(w),
                "terminal focus after Configure close"
            );
        });
        fixture.calls.lock().unwrap().clear();
        cx.simulate_keystrokes("up");
        settle(&rt, cx);
        assert!(
            fixture
                .calls
                .lock()
                .unwrap()
                .iter()
                .any(|(channel, args)| *channel == "core:terminal:input"
                    && STANDARD.decode(args[0]["bytes"].as_str().unwrap()).unwrap() == b"\x1b[A")
        );
    }
    fixture
        .reject_update
        .store(true, std::sync::atomic::Ordering::SeqCst);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.act(input::KeyResult::Configure, w, cx);
            v.name.update(cx, |f, cx| w.focus(&f.focus_handle(cx), cx));
        })
    });
    settle(&rt, cx);
    let button = cx
        .debug_bounds("terminal-config-save")
        .expect("Save visible");
    cx.simulate_click(button.center(), Modifiers::default());
    settle(&rt, cx);
    cx.update(|w, cx| {
        assert!(view.read(cx).state.configure.is_some());
        assert!(
            !view.read(cx).focus.is_focused(w),
            "failed Save must retain dialog focus"
        );
    });
}

#[gpui::test]
fn header_new_session_preserves_terminal_keyboard_target(cx: &mut TestAppContext) {
    header_action_preserves_terminal_keyboard_target(cx, "terminal-new-session", true);
}
#[gpui::test]
fn header_zoom_preserves_terminal_keyboard_target(cx: &mut TestAppContext) {
    header_action_preserves_terminal_keyboard_target(cx, "terminal-zoom-in", false);
}
fn header_action_preserves_terminal_keyboard_target(
    cx: &mut TestAppContext,
    selector: &'static str,
    create: bool,
) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| TerminalView::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.listeners_ready(true, true, cx);
            v.activate(payload(), 1, w, cx);
        })
    });
    settle(&rt, cx);
    fixture.calls.lock().unwrap().clear();
    let button = cx
        .debug_bounds(selector)
        .expect("Terminal header action visible");
    cx.simulate_click(button.center(), Modifiers::default());
    settle(&rt, cx);
    let creates = || {
        fixture
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(channel, _)| *channel == "terminal:create")
            .count()
    };
    assert_eq!(creates(), usize::from(create));
    cx.update(|w, cx| {
        assert!(
            view.read(cx).focus.is_focused(w),
            "terminal focus after {selector}"
        );
        if create {
            assert_eq!(view.read(cx).state.active.as_deref(), Some("b"));
        }
    });
    cx.simulate_keystrokes("enter up");
    settle(&rt, cx);
    assert_eq!(
        creates(),
        usize::from(create),
        "Enter must not activate header action again"
    );
    let bytes = fixture
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(channel, _)| *channel == "core:terminal:input")
        .flat_map(|(_, args)| STANDARD.decode(args[0]["bytes"].as_str().unwrap()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(bytes, b"\r\x1b[A");
}
