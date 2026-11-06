use super::*;
use ::core::prelude::v1::test;
#[::core::prelude::v1::test]
fn wire_activation_keeps_identity_and_one_based_rank() {
    let (channel, args) = Request::Activate {
        action: json!({"type":"copy-text","text":"fixture"}),
        query: "q".into(),
        rank: 3,
        result_id: "snippet:fixture".into(),
    }
    .wire();
    assert_eq!(channel, "search:execute");
    assert_eq!(args.len(), 1);
    assert_eq!(args[0]["context"]["rank"], 3);
    assert_eq!(args[0]["context"]["resultId"], "snippet:fixture");
    assert_eq!(args[0]["action"]["text"], "fixture");
}

#[derive(Default)]
struct Fixture {
    calls: std::sync::Mutex<Vec<(String, Vec<Value>)>>,
    fail: bool,
}
impl RequestAdapter for Fixture {
    fn send(
        &self,
        request: Request,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>> {
        let (channel, args) = request.wire();
        self.calls.lock().unwrap().push((channel.into(), args));
        let fail = self.fail;
        Box::pin(async move {
            if fail {
                Err("Synthetic failure".into())
            } else {
                Ok(json!({"ok":true}))
            }
        })
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
fn seed(v: &mut Launcher, rows: Vec<Row>) {
    v.ready = true;
    v.visible = true;
    v.model.visible = true;
    v.model.input = "fixture".into();
    v.model.session.commit("fixture".into(), 0);
    v.model.session.take_request(8);
    let generation = v.model.session.generation;
    v.model.update(
        Input::SearchComplete {
            generation,
            result: Ok(rows),
        },
        9,
    );
    v.rows = v.model.display_rows();
}
#[gpui::test]
fn native_enter_executes_once_and_failure_keeps_input(cx: &mut TestAppContext) {
    let _runtime = runtime(cx);
    let adapter = Arc::new(Fixture {
        fail: true,
        ..Default::default()
    });
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let mut row = Row::command("synthetic", "Synthetic");
            row.action = Action::CopyText {
                text: "synthetic".into(),
            };
            seed(v, vec![row]);
            v.input.update(cx, |i, cx| i.set_text("fixture", w, cx));
            v.input.focus_handle(cx).focus(w, cx);
        });
    });
    cx.simulate_keystrokes("enter");
    _runtime.block_on(async {
        for _ in 0..100 {
            cx.run_until_parked();
            tokio::task::yield_now().await;
        }
    });
    assert_eq!(
        view.read_with(cx, |v, _| v.error.clone()).as_deref(),
        Some("Synthetic failure")
    );
    assert!(view.read_with(cx, |v, _| v.visible));
    assert_eq!(view.read_with(cx, |v, _| v.model.input.clone()), "fixture");
    assert_eq!(
        adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.0 == "search:execute")
            .count(),
        1
    );
}
#[gpui::test]
fn real_input_composition_hide_reopen_and_stale_click(cx: &mut TestAppContext) {
    let _runtime = runtime(cx);
    let adapter = Arc::new(Fixture::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            seed(v, vec![Row::command("old", "Old")]);
            let old = v.model.session.generation;
            v.input.update(cx, |i, cx| {
                i.replace_and_mark_text_in_range(None, "に", None, w, cx)
            });
            assert!(adapter.calls.lock().unwrap().is_empty());
            v.hide(w, cx);
            assert!(v.input.read(cx).marked_range.is_none());
            assert!(!v.model.composing);
            v.show(w, cx);
            v.click(old, "command:old".into(), cx);
            assert!(!v.model.session.activating);
        });
    });
    assert!(
        !adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.0 == "search:execute")
    );
}
#[gpui::test]
fn native_argument_fields_validate_and_tab_without_executing(cx: &mut TestAppContext) {
    let _runtime = runtime(cx);
    cx.update(|cx| tezbar_ui::text_input::bind_keys(cx));
    let adapter = Arc::new(Fixture::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w,cx|{view.update(cx,|v,cx|{let mut r=Row::command("args","Arguments");r.action=Action::RunExtensionCommand{extension_id:"fixture".into(),command_name:"form".into(),extra:serde_json::from_value(json!({"commandArgumentDefinitions":[{"name":"secret","type":"password","required":true},{"name":"choice","type":"dropdown","required":true,"data":[{"value":"a","title":"A"}]}]})).unwrap()};seed(v,vec![r]);v.input.focus_handle(cx).focus(w,cx);});});
    cx.simulate_keystrokes("enter");
    assert_eq!(
        view.read_with(cx, |v, _| v.form.as_ref().unwrap().inputs.len()),
        2
    );
    cx.simulate_keystrokes("enter");
    assert!(view.read_with(cx, |v, _| v.error.is_some()));
    assert!(adapter.calls.lock().unwrap().is_empty());
    cx.simulate_keystrokes("tab");
    assert_eq!(
        view.read_with(cx, |v, _| v.form.as_ref().unwrap().args.selected),
        1
    );
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.key(
                &KeyDownEvent {
                    keystroke: Keystroke::parse("escape").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                },
                w,
                cx,
            );
        })
    });
    assert!(view.read_with(cx, |v, _| v.form.is_none()));
}
#[gpui::test]
fn native_grid_arrows_and_pin_picker_have_authoritative_identity(cx: &mut TestAppContext) {
    let _runtime = runtime(cx);
    let adapter = Arc::new(Fixture::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter, relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            seed(
                v,
                (0..30)
                    .map(|i| Row::command(&format!("app-{i}"), &format!("App {i}")))
                    .collect(),
            );
            v.model.input = "`".into();
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("down right");
    assert_eq!(view.read_with(cx, |v, _| v.model.session.selected), 8);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.key(
                &KeyDownEvent {
                    keystroke: Keystroke::parse(if cfg!(target_os = "macos") {
                        "cmd-p"
                    } else {
                        "ctrl-p"
                    })
                    .unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                },
                w,
                cx,
            )
        })
    });
    assert!(view.read_with(cx, |v, _| v.pin_picker.is_some()));
    cx.simulate_keystrokes("enter");
    assert_eq!(
        view.read_with(cx, |v, _| v.state.pins[0].id.clone()),
        "command:app-8"
    );
}
#[::core::prelude::v1::test]
fn update_actions_are_status_driven_and_downloading_is_inert() {
    use tezbar_update::AppUpdateStatus as S;
    for (s, a) in [
        (S::Idle, Some(UpdateAction::Check)),
        (
            S::Ready {
                version: "1".into(),
            },
            Some(UpdateAction::Restart),
        ),
        (
            S::Available {
                version: "1".into(),
                notes: "".into(),
                release_url: "".into(),
            },
            Some(UpdateAction::Download),
        ),
        (
            S::ManualAvailable {
                version: "1".into(),
                notes: "".into(),
                release_url: "".into(),
                installer_url: "https://example.test/fixture".into(),
            },
            Some(UpdateAction::Installer(
                "https://example.test/fixture".into(),
            )),
        ),
        (
            S::Downloading {
                version: "1".into(),
                downloaded: 1,
                total: Some(2),
            },
            None,
        ),
    ] {
        assert_eq!(presentation::update_action(&s), a);
    }
}

#[gpui::test]
fn render_is_pure_and_result_element_exposes_native_accessibility(cx: &mut TestAppContext) {
    let _runtime = runtime(cx);
    let adapter = Arc::new(Fixture::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    view.update(cx, |v, cx| {
        seed(v, vec![Row::command("synthetic", "Synthetic result")]);
        cx.notify();
    });
    cx.run_until_parked();
    for _ in 0..5 {
        view.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
    }
    assert!(adapter.calls.lock().unwrap().is_empty());
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let element = v.row_element(0, false, tezbar_ui::tokens::tokens(w), cx);
            assert_eq!(element.a11y_role(), Some(Role::ListBoxOption));
            let mut node = gpui::accesskit::Node::new(Role::ListBoxOption);
            element.write_a11y_info(&mut node);
            assert_eq!(node.label(), Some("Synthetic result"));
            assert_eq!(node.is_selected(), Some(true));
            assert_eq!(node.position_in_set(), Some(1));
            assert_eq!(node.size_of_set(), Some(1));
        })
    });
}
#[gpui::test]
fn ai_submit_preserves_prompt_and_directory_through_typed_navigation(cx: &mut TestAppContext) {
    let _runtime = runtime(cx);
    let adapter = Arc::new(Fixture::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            seed(v, vec![Row::command("__chat-session", "Old chat")]);
            v.model.input = " summarize fixture".into();
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("enter");
    view.read_with(cx,|v,_|{assert!(matches!(&v.navigation,Some(Navigation{surface:Surface::AiChat,payload:Payload::Ai{prompt:Some(p),resume:None,..},..})if p=="summarize fixture"));});
    assert!(adapter.calls.lock().unwrap().is_empty());
}
#[gpui::test]
fn sentinels_have_hidden_prefix_and_two_step_escape(cx: &mut TestAppContext) {
    let _runtime = runtime(cx);
    let adapter = Arc::new(Fixture::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter, relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.ready = true;
            v.visible = true;
            v.model.visible = true;
            v.edit("!fixture", w, cx);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |v, cx| {
        assert_eq!(v.model.input, "!fixture");
        assert_eq!(v.input.read(cx).content.as_ref(), "fixture");
    });
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |v, cx| {
        assert_eq!(v.model.input, "!");
        assert!(v.input.read(cx).content.is_empty());
    });
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |v, _| assert!(v.model.input.is_empty()));
}
#[::core::prelude::v1::test]
fn canonical_surface_shortcuts_have_typed_destinations() {
    for name in [
        "command",
        "ai-chat",
        "terminal",
        "extension-runtime",
        "settings",
        "settings-ai",
        "settings-extensions",
        "permissions",
        "extensions",
        "system-monitor",
        "indexing",
        "clipboard",
        "snippets",
        "notes",
        "vault",
        "emoji-picker",
        "release-notes",
        "onboarding",
    ] {
        assert!(Navigation::from_core(&json!(name), 1).is_some(), "{name}");
    }
    assert!(Navigation::from_core(&json!("nonexistent"), 1).is_none());
}
#[cfg(target_os = "macos")]
#[::core::prelude::v1::test]
fn presentation_uses_current_os_locale_for_currency_date_relative_and_count() {
    use tezbar_core::instant::{
        currency::AmountFormatter,
        transform::{PresentationFormatter, RelativeTime},
    };
    let formatter = locale::NativeLocale;
    assert!(!formatter.currency(1234.5, "EUR", 2).is_empty());
    assert!(!formatter.local_timestamp(0).is_empty());
    assert!(
        !formatter
            .relative(&RelativeTime {
                unit: "week",
                value: -1
            })
            .is_empty()
    );
    assert!(!formatter.count(12345).is_empty());
}

#[derive(Default)]
struct Loopback {
    calls: std::sync::Mutex<Vec<(String, Vec<Value>)>>,
}
impl RequestAdapter for Loopback {
    fn send(
        &self,
        request: Request,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>> {
        let (channel, args) = request.wire();
        self.calls
            .lock()
            .unwrap()
            .push((channel.into(), args.clone()));
        let value = match channel {
            "search:all" => json!([Row::command("synthetic", "Synthetic result")]),
            "query" => json!({"type":"answer"}),
            "path:complete" => {
                json!([{"id":"path:fixture","title":"Folder","subtitle":"/fixture/","kind":"directory","path":"/fixture","value":"/fixture/","score":1}])
            }
            "chat:list" => json!([{"id":"chat-fixture","title":"Saved chat"}]),
            "terminal:get-prompt-info" => json!({"cwd":"/fixture","prompt":"fixture >"}),
            "terminal:list" => {
                json!([{"sessionId":"terminal-fixture","title":"Saved terminal","cwd":"/fixture","status":"running"}])
            }
            "path:resolve-directory" => json!("/fixture/resolved"),
            "extension:launcher-search" => json!([]),
            "llm-config-get" => {
                json!({"provider":"fixture","model":"model-a","customProviders":[{"id":"fixture"}],"providerModels":{"fixture":[{"id":"model-a"},{"id":"model-b"}]}})
            }
            "vault:status" => json!({"supported":true,"unlocked":false}),
            "currency:frankfurter-latest" => json!({"base":"USD","rates":{"EUR":0.9}}),
            _ => json!({"ok":true}),
        };
        Box::pin(async move { Ok(value) })
    }
}
fn settle(runtime: &tokio::runtime::Runtime, cx: &mut VisualTestContext) {
    runtime.block_on(async {
        for _ in 0..32 {
            cx.run_until_parked();
            tokio::task::yield_now().await;
        }
    });
}
#[gpui::test]
fn committed_input_coalesces_to_one_latest_search(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.ready = true;
            v.visible = true;
            v.model.visible = true;
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_input("al");
    cx.simulate_input("pha");
    assert!(adapter.calls.lock().unwrap().is_empty());
    cx.executor().advance_clock(Duration::from_millis(8));
    settle(&rt, cx);
    let calls = adapter.calls.lock().unwrap();
    let searches = calls
        .iter()
        .filter(|c| c.0 == "search:all")
        .collect::<Vec<_>>();
    assert_eq!(searches.len(), 1);
    assert_eq!(searches[0].1[0], "alpha");
    assert!(!view.read_with(cx, |v, _| v.model.session.pending()));
}
#[gpui::test]
fn path_tab_records_visit_and_fences_completion(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.ready = true;
            v.visible = true;
            v.model.visible = true;
            v.edit("/fi", w, cx);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.executor().advance_clock(Duration::from_millis(8));
    settle(&rt, cx);
    cx.simulate_keystrokes("tab");
    settle(&rt, cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.model.input.clone()),
        "/fixture/"
    );
    assert!(
        adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, a)| c == "directory-visit:record" && a[0] == "/fixture")
    );
}
#[gpui::test]
fn directory_submit_resolves_before_navigation_and_records_visit(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            seed(v, vec![]);
            v.model.input = "/fixture  explain".into();
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("enter");
    settle(&rt, cx);
    assert!(view.read_with(cx,|v,_|matches!(&v.navigation,Some(Navigation{payload:Payload::Ai{prompt:Some(p),cwd:Some(c),..},..})if p=="explain"&&c=="/fixture/resolved")));
    assert!(
        adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| c == "directory-visit:record")
    );
}
#[gpui::test]
fn terminal_session_controls_use_core_channels_and_persist_defaults(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            seed(v, vec![]);
            v.model.terminal_mode = true;
            v.model.input = ">".into();
            v.load_terminal(cx);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    settle(&rt, cx);
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.terminal_pin(cx);
            v.terminal_action(false, cx);
        })
    });
    settle(&rt, cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.state.terminal_pinned_sessions.clone()),
        ["terminal-fixture"]
    );
    assert!(
        adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| c == "terminal:kill")
    );
}
#[gpui::test]
fn vault_copy_checks_unlock_then_copies_without_replaying_effects(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    view.update(cx, |v, cx| {
        seed(v, vec![]);
        v.vault_copy("fixture-id".into(), cx);
    });
    settle(&rt, cx);
    let channels = adapter
        .calls
        .lock()
        .unwrap()
        .iter()
        .map(|(c, _)| c.clone())
        .collect::<Vec<_>>();
    assert_eq!(channels, ["vault:status", "vault:unlock", "vault:copy"]);
    assert_eq!(
        view.read_with(cx, |v, _| v.notice.clone()).as_deref(),
        Some("Copied · clears in 30 s")
    );
}

#[derive(Default)]
struct Pending {
    calls: std::sync::Mutex<Vec<String>>,
    searches: std::sync::Mutex<std::collections::VecDeque<tokio::sync::oneshot::Sender<Value>>>,
}
impl RequestAdapter for Pending {
    fn send(
        &self,
        r: Request,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>> {
        let (channel, _) = r.wire();
        self.calls.lock().unwrap().push(channel.into());
        if channel == "search:all" {
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.searches.lock().unwrap().push_back(tx);
            Box::pin(async move { rx.await.map_err(|e| e.to_string()) })
        } else {
            Box::pin(async { Ok(json!({"ok":true})) })
        }
    }
}
#[gpui::test]
fn pending_enter_expires_at_one_second_before_async_result(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Pending::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            seed(v, vec![]);
            v.edit("new query", w, cx);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.executor().advance_clock(Duration::from_millis(8));
    settle(&rt, cx);
    cx.simulate_keystrokes("enter");
    cx.executor().advance_clock(Duration::from_millis(1000));
    settle(&rt, cx);
    adapter
        .searches
        .lock()
        .unwrap()
        .pop_front()
        .unwrap()
        .send(json!([Row::command("open-notes", "Notes")]))
        .unwrap();
    settle(&rt, cx);
    assert!(
        !adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c == "search:execute" || c == "search:record-usage")
    );
    assert!(!view.read_with(cx, |v, _| v.model.session.activating));
}
#[gpui::test]
fn composition_start_cancels_pending_enter_before_result_arrives(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Pending::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            seed(v, vec![]);
            v.edit("new query", w, cx);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.executor().advance_clock(Duration::from_millis(8));
    settle(&rt, cx);
    cx.simulate_keystrokes("enter");
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.input.update(cx, |i, cx| {
                i.replace_and_mark_text_in_range(None, "に", None, w, cx)
            })
        })
    });
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.model.composing));
    adapter
        .searches
        .lock()
        .unwrap()
        .pop_front()
        .unwrap()
        .send(json!([Row::command("open-notes", "Notes")]))
        .unwrap();
    settle(&rt, cx);
    assert!(
        !adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c == "search:execute" || c == "search:record-usage")
    );
}
#[gpui::test]
fn pin_drag_requires_six_pixels_reorders_and_unpins_outside(cx: &mut TestAppContext) {
    let _rt = runtime(cx);
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
    let a = cx.debug_bounds("pin-0").expect("first pin");
    let b = cx.debug_bounds("pin-1").expect("second pin");
    let start = a.center();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        start + point(px(5.), px(0.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    assert!(!view.read_with(cx, |v, _| v.drag.as_ref().unwrap().active));
    cx.simulate_mouse_move(b.center(), Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(b.center(), MouseButton::Left, Modifiers::default());
    assert_eq!(
        view.read_with(cx, |v, _| v
            .state
            .pins
            .iter()
            .map(|p| p.id.clone())
            .collect::<Vec<_>>()),
        ["command:b", "command:a"]
    );
    let first = cx.debug_bounds("pin-0").unwrap().center();
    let outside = first + point(px(0.), px(120.));
    cx.simulate_mouse_down(first, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(outside, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::default());
    assert_eq!(
        view.read_with(cx, |v, _| v
            .state
            .pins
            .iter()
            .map(|p| p.id.clone())
            .collect::<Vec<_>>()),
        ["command:a"]
    );
    assert!(adapter.calls.lock().unwrap().is_empty());
}

#[gpui::test]
fn starting_core_preserves_latest_text_and_searches_once_when_ready(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.show(w, cx);
        })
    });
    cx.simulate_input("latest before ready");
    assert!(adapter.calls.lock().unwrap().is_empty());
    view.update(cx, |v, cx| {
        v.ready = true;
        v.core_ready(cx);
    });
    cx.executor().advance_clock(Duration::from_millis(8));
    settle(&rt, cx);
    let searches = adapter
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| c == "search:all")
        .map(|(_, a)| a[0].clone())
        .collect::<Vec<_>>();
    assert_eq!(searches, [json!("latest before ready")]);
}
#[gpui::test]
fn model_selection_uses_fresh_model_write_owner_and_blocks_submit_until_saved(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            seed(v, vec![]);
            v.model.input = " explain".into();
            v.open_models(cx);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    settle(&rt, cx);
    view.update(cx, |v, cx| {
        let index = v
            .model_choices
            .iter()
            .position(|(p, m)| p == "fixture" && m == "model-b")
            .unwrap();
        v.select_model(index, cx);
    });
    cx.simulate_keystrokes("enter");
    assert!(view.read_with(cx, |v, _| v.navigation.is_none()));
    settle(&rt, cx);
    assert!(
        adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, a)| c == "llm-config-set"
                && a[0]["provider"] == "fixture"
                && a[0]["taskModelOverrides"]["chat"] == "model-b")
    );
    assert!(!view.read_with(cx, |v, _| v.model_saving));
}

#[gpui::test]
fn answer_stream_uses_preinstalled_subscription_and_hide_discards_updates(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    cx.update(|cx| cx.set_global(CoreEvents::new(relay.clone())));
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter, relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            seed(v, vec![]);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("enter");
    settle(&rt, cx);
    cx.update(|_, cx| {
        let hub = cx.global::<CoreEvents>().clone();
        hub.publish(
            CoreEvent::Other {
                channel: "stream-token".into(),
                payload: json!("Synthetic answer"),
            },
            cx,
        );
        hub.publish(
            CoreEvent::Other {
                channel: "stream-done".into(),
                payload: Value::Null,
            },
            cx,
        );
    });
    assert_eq!(
        view.read_with(cx, |v, _| v.answer_text.clone()),
        "Synthetic answer"
    );
    assert!(!view.read_with(cx, |v, _| v.answer_busy));
    cx.update(|w, cx| view.update(cx, |v, cx| v.hide(w, cx)));
    cx.update(|_, cx| {
        let hub = cx.global::<CoreEvents>().clone();
        hub.publish(
            CoreEvent::Other {
                channel: "stream-token".into(),
                payload: json!("late"),
            },
            cx,
        )
    });
    assert!(!view.read_with(cx, |v, _| v.answer_text.ends_with("late")));
}
#[gpui::test]
fn password_result_expires_and_releases_document_text(cx: &mut TestAppContext) {
    let _rt = runtime(cx);
    let adapter = Arc::new(Fixture::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter, relay, None, w, cx));
    view.update(cx, |v, cx| {
        seed(v, vec![]);
        v.card_message(
            "synthetic-password".into(),
            Some("password".into()),
            9000,
            cx,
        );
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(8999));
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.notice.is_some()));
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    view.read_with(cx, |v, cx| {
        assert!(v.notice.is_none());
        assert!(v.message_kind.is_none());
        assert!(
            v.message_document
                .read(cx)
                .document
                .selected_text(0..1000)
                .is_empty()
        );
    });
}
#[gpui::test]
fn currency_uses_mocked_core_rates_and_current_owner(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    view.update(cx, |v, cx| {
        seed(v, vec![]);
        v.currency("100 usd to eur".into(), v.model.session.generation, cx);
    });
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| {
        v.rows.iter().any(|r| {
            r.id == "currency:conversion"
                && matches!(&r.action,Action::CopyText{text}if !text.is_empty())
        })
    }));
    assert_eq!(
        adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| c == "currency:frankfurter-latest")
            .count(),
        1
    );
}
