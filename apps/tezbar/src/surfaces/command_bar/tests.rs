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
    settle(&_runtime, cx);
    let calls = adapter.calls.lock().unwrap();
    assert!(!calls.iter().any(|(channel, _)| matches!(
        channel.as_str(),
        "extension:run-command" | "search:execute"
    )));
    assert_eq!(
        calls
            .iter()
            .filter(|(channel, _)| channel == "search:record-usage")
            .count(),
        1
    );
    drop(calls);
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
            "terminal:get-prompt-info" => json!({"user":"fixture","host":"synthetic","dir":"~"}),
            "terminal:list" => {
                json!([{"sessionId":"terminal-fixture","title":"Saved terminal","cwd":"/fixture","status":"running"}])
            }
            "path:resolve-directory" => json!("/fixture/resolved"),
            "extension:launcher-search" => json!([]),
            "llm-config-get" => {
                json!({"provider":"fixture","model":"model-a","customProviders":[{"id":"fixture"}],"providerModels":{"fixture":[{"id":"model-a"},{"id":"model-b"}]},"providerConfigs":{"fixture":{"apiKey":"synthetic-fixture-key"}}})
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
    let outside = first + point(px(120.), px(0.));
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
    let owner = view.read_with(cx, |v, _| v.answer_owner.clone().unwrap());
    cx.update(|_, cx| {
        let hub = cx.global::<CoreEvents>().clone();
        hub.publish(
            CoreEvent::Other {
                channel: "stream-token".into(),
                payload: json!({"owner":owner,"value":"Synthetic answer"}),
            },
            cx,
        );
        hub.publish(
            CoreEvent::Other {
                channel: "stream-done".into(),
                payload: json!({"owner":owner,"value":null}),
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
                payload: json!({"owner":owner,"value":"late"}),
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

#[gpui::test]
fn picker_search_provider_preview_ime_hidden_selection_and_setup_guard(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            seed(v, vec![]);
            v.open_models(cx);
            v.model_search.focus_handle(cx).focus(w, cx);
        })
    });
    settle(&rt, cx);
    view.update(cx,|v,cx|{v.model_config["providerModels"]["fixture"]=json!([{"id":"model-a","capabilities":["vision"],"hiddenFromPicker":true},{"id":"model-b","capabilities":["tools"]},{"id":"hidden-model","hiddenFromPicker":true}]);v.rebuild_models(cx);});
    assert_eq!(view.read_with(cx, |v, _| v.model_choices.len()), 2);
    cx.simulate_input("vision");
    assert_eq!(
        view.read_with(cx, |v, _| v.model_choices.clone()),
        [("fixture".into(), "model-a".into())]
    );
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.model_search.update(cx, |field, cx| {
                field.replace_and_mark_text_in_range(Some(0..6), "に", None, w, cx)
            });
        })
    });
    assert_eq!(view.read_with(cx, |v, _| v.model_query.clone()), "vision");
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.model_search.update(cx, |field, cx| {
                field.replace_text_in_range(None, "tools", w, cx)
            });
        })
    });
    assert_eq!(
        view.read_with(cx, |v, _| v.model_choices.clone()),
        [("fixture".into(), "model-b".into())]
    );
    cx.update(|w, cx| view.update(cx, |v, cx| v.preview_model_provider("openai".into(), w, cx)));
    view.read_with(cx, |v, cx| {
        assert!(v.model_query.is_empty());
        assert!(v.model_search.read(cx).content.is_empty());
        assert_eq!(v.model_provider, "openai");
    });
    view.update(cx, |v, cx| v.select_model(0, cx));
    settle(&rt, cx);
    assert!(
        !adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| c == "llm-config-set")
    );
    cx.simulate_keystrokes("escape");
    cx.update(|w, cx| {
        view.read_with(cx, |v, cx| {
            assert!(!v.model_menu);
            assert!(v.input.focus_handle(cx).is_focused(w));
        })
    });
}
#[gpui::test]
fn alt_slot_runs_persisted_pin_absent_from_current_results_once(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Fixture::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            seed(v, vec![Row::command("unrelated", "Unrelated")]);
            let mut pin = Row::command("saved", "Saved action");
            pin.action = Action::CopyText {
                text: "synthetic pin".into(),
            };
            ui_state::insert_pin(&mut v.state.pins, &pin, None);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("alt-1 alt-1");
    settle(&rt, cx);
    let calls = adapter.calls.lock().unwrap();
    let executed = calls
        .iter()
        .filter(|(c, _)| c == "search:execute")
        .collect::<Vec<_>>();
    assert_eq!(executed.len(), 1);
    assert_eq!(executed[0].1[0]["context"]["resultId"], "command:saved");
    assert_eq!(executed[0].1[0]["action"]["text"], "synthetic pin");
}
#[gpui::test]
fn queued_old_owner_and_legacy_events_cannot_change_replacement_answer(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    cx.update(|cx| cx.set_global(CoreEvents::new(relay.clone())));
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            seed(v, vec![]);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("enter");
    settle(&rt, cx);
    let first = view.read_with(cx, |v, _| v.answer_owner.clone().unwrap());
    cx.simulate_keystrokes("enter");
    settle(&rt, cx);
    let second = view.read_with(cx, |v, _| v.answer_owner.clone().unwrap());
    assert_ne!(first, second);
    cx.update(|_, cx| {
        let hub = cx.global::<CoreEvents>().clone();
        for (channel, payload) in [
            ("stream-token", json!({"owner":first,"value":"wrong"})),
            ("stream-error", json!({"owner":first,"value":"old error"})),
            ("stream-done", json!({"owner":first,"value":null})),
            ("stream-token", json!("legacy")),
            ("stream-token", json!({"owner":second,"value":"current"})),
        ] {
            hub.publish(
                CoreEvent::Other {
                    channel: channel.into(),
                    payload,
                },
                cx,
            );
        }
    });
    view.read_with(cx, |v, _| {
        assert_eq!(v.answer_text, "current");
        assert!(v.answer_busy);
        assert!(v.error.is_none());
    });
    let calls = adapter.calls.lock().unwrap();
    let queries = calls
        .iter()
        .filter(|(c, _)| c == "query")
        .collect::<Vec<_>>();
    assert_eq!(queries.len(), 2);
    assert_eq!(queries[0].1[1], first);
    assert_eq!(queries[1].1[1], second);
}

#[gpui::test]
fn pin_activation_uses_saved_action_when_live_row_has_same_id(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Fixture::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let mut live = Row::command("shared", "Live");
            live.action = Action::CopyText {
                text: "live replacement".into(),
            };
            seed(v, vec![live.clone()]);
            live.action = Action::CopyText {
                text: "persisted pin".into(),
            };
            ui_state::insert_pin(&mut v.state.pins, &live, None);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("alt-1");
    settle(&rt, cx);
    let calls = adapter.calls.lock().unwrap();
    assert_eq!(
        calls.iter().find(|(c, _)| c == "search:execute").unwrap().1[0]["action"]["text"],
        "persisted pin"
    );
}

#[gpui::test]
fn ai_history_does_not_replace_home_while_reopen_refresh_is_pending(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter, relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.ready = true;
            v.show(w, cx);
        })
    });
    cx.executor().advance_clock(Duration::from_millis(8));
    settle(&rt, cx);
    cx.update(|w, cx| view.update(cx, |v, cx| v.edit(" ", w, cx)));
    cx.executor().advance_clock(Duration::from_millis(8));
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| v.rows.iter().any(|row| row.category == "chat")));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.hide(w, cx);
            v.show(w, cx);
        })
    });
    cx.executor().advance_clock(Duration::from_millis(8));
    cx.run_until_parked();
    assert!(
        !view.read_with(cx, |v, _| v.rows.iter().any(|row| row.category == "chat")),
        "AI history cannot become optimistic home rows while the home transport is pending"
    );
    settle(&rt, cx);
}

#[gpui::test]
fn off_query_saved_pin_preserves_title_category_and_declared_argument_form(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w,cx|view.update(cx,|v,cx|{seed(v,vec![Row::command("other","Other")]);let mut pin=Row::command("rich","Saved title");pin.category="extensions".into();pin.subtitle="Saved description".into();pin.action=Action::RunExtensionCommand{extension_id:"synthetic.extension".into(),command_name:"fixture".into(),extra:serde_json::from_value(json!({"commandArgumentDefinitions":[{"name":"token","title":"Fixture token","type":"password","required":true}]})).unwrap()};ui_state::insert_pin(&mut v.state.pins,&pin,None);v.input.focus_handle(cx).focus(w,cx);}));
    cx.simulate_keystrokes("alt-1");
    view.read_with(cx, |v, _| {
        let form = v.form.as_ref().unwrap();
        assert_eq!(form.row.title, "Saved title");
        assert_eq!(form.row.category, "extensions");
        assert_eq!(form.row.subtitle, "Saved description");
        assert_eq!(form.args.fields[0].name, "token");
        assert!(form.labels[0].password);
    });
    assert!(
        !adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| c == "extension:run-command")
    );
    cx.simulate_input("synthetic-value");
    cx.simulate_keystrokes("enter");
    settle(&rt, cx);
    let calls = adapter.calls.lock().unwrap();
    let executed = calls
        .iter()
        .filter(|(c, _)| c == "extension:run-command")
        .collect::<Vec<_>>();
    assert_eq!(executed.len(), 1);
    assert_eq!(
        executed[0].1[0]["argumentValues"]["token"],
        "synthetic-value"
    );
}
#[gpui::test]
fn pinned_argument_form_supersedes_earlier_pending_enter(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Pending::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            seed(v, vec![]);
            let mut pin = Row::command("form", "Explicit pin");
            pin.category = "extensions".into();
            pin.action = Action::RunExtensionCommand {
                extension_id: "fixture".into(),
                command_name: "form".into(),
                extra: serde_json::from_value(
                    json!({"commandArgumentDefinitions":[{"name":"argument","required":true}]}),
                )
                .unwrap(),
            };
            ui_state::insert_pin(&mut v.state.pins, &pin, None);
            v.edit("pending query", w, cx);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.executor().advance_clock(Duration::from_millis(8));
    settle(&rt, cx);
    cx.simulate_keystrokes("enter alt-1");
    assert!(view.read_with(cx, |v, _| v.form.is_some()));
    let mut late = Row::command("late", "pending query");
    late.action = Action::CopyText {
        text: "must not auto execute".into(),
    };
    adapter
        .searches
        .lock()
        .unwrap()
        .pop_front()
        .unwrap()
        .send(json!([late]))
        .unwrap();
    settle(&rt, cx);
    view.read_with(cx, |v, _| {
        let form = v
            .form
            .as_ref()
            .expect("the explicitly activated pin form must remain open");
        assert_eq!(v.model.input, "pending query");
        assert_eq!(form.generation, v.model.session.generation);
    });
    assert!(
        !adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c == "search:execute"),
        "the earlier pending Enter must be retired when the user explicitly activates a pin"
    );
}

#[gpui::test]
fn native_answer_tokens_coalesce_to_one_frame_revision(cx: &mut TestAppContext) {
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
    let (owner, before) = view.read_with(cx, |v, _| {
        (v.answer_owner.clone().unwrap(), v.answer_revision)
    });
    cx.update(|_, cx| {
        let hub = cx.global::<CoreEvents>().clone();
        for _ in 0..100 {
            hub.publish(
                CoreEvent::Other {
                    channel: "stream-token".into(),
                    payload: json!({"owner":owner,"value":"x"}),
                },
                cx,
            );
        }
        view.read_with(cx, |v, _| {
            assert_eq!(v.answer_revision, before);
            assert_eq!(v.answer_text.len(), 100);
        });
    });
    cx.run_until_parked();
    cx.update(|w, cx| {
        assert_eq!(w.simulate_next_frame(cx), 1);
    });
    assert_eq!(view.read_with(cx, |v, _| v.answer_revision), before + 1);
}

#[derive(Default)]
struct DeferredConfig {
    replies: std::sync::Mutex<std::collections::VecDeque<tokio::sync::oneshot::Sender<Value>>>,
}
impl RequestAdapter for DeferredConfig {
    fn send(
        &self,
        request: Request,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>> {
        assert!(matches!(request, Request::LlmConfig));
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.replies.lock().unwrap().push_back(tx);
        Box::pin(async move { rx.await.map_err(|e| e.to_string()) })
    }
}
#[gpui::test]
fn provider_preview_survives_earlier_config_reply(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(DeferredConfig::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            seed(v, vec![]);
            v.open_models(cx);
            v.preview_model_provider("openai".into(), w, cx);
        })
    });
    settle(&rt, cx);
    adapter.replies.lock().unwrap().pop_front().unwrap().send(json!({"provider":"ollama","model":"synthetic","providerModels":{"openai":[{"id":"synthetic-openai-model"}]}})).unwrap();
    settle(&rt, cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.model_provider.clone()),
        "openai"
    );
    assert_eq!(
        view.read_with(cx, |v, _| v.model_choices.clone()),
        [("openai".into(), "synthetic-openai-model".into())]
    );
}
#[gpui::test]
fn inline_argument_ime_escape_and_terminal_defaults_enter_keep_their_owners(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| {
        let mut row = Row::command("form", "Synthetic argument");
        row.action = Action::RunExtensionCommand {extension_id:"fixture".into(),command_name:"form".into(),extra:serde_json::from_value(json!({"commandArgumentDefinitions":[{"name":"text","title":"Input","required":true}]})).unwrap()};
        seed(v, vec![row]);
        v.input.focus_handle(cx).focus(w, cx);
    }));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let form = v.form.as_ref().unwrap();
            assert!(form.inline());
            assert!(form.inputs[0].focus_handle(cx).is_focused(w));
            form.inputs[0].update(cx, |field, cx| {
                field.replace_and_mark_text_in_range(None, "に", None, w, cx)
            });
        })
    });
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |v, cx| {
        assert!(
            v.form.as_ref().unwrap().inputs[0]
                .read(cx)
                .marked_range
                .is_none()
        )
    });
    assert!(
        !adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| c == "extension:run-command")
    );
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.return_home(w, cx);
            v.model.terminal_mode = true;
            v.model.input = ">".into();
            v.terminal_settings = true;
            v.terminal_draft.save_for = "forever".into();
            v.terminal_draft.keep_alive_for = "until-stop".into();
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("enter");
    settle(&rt, cx);
    view.read_with(cx, |v, _| {
        assert!(!v.terminal_settings);
        assert!(v.navigation.is_none());
        assert_eq!(v.state.terminal_defaults.save_for, "forever");
        assert_eq!(v.state.terminal_defaults.keep_alive_for, "until-stop");
    });
}

#[gpui::test]
fn directory_terminal_sentinel_prompt_and_ai_escape_preserve_native_payload(
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
            v.edit("/synthetic/project > echo fixture", w, cx);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    settle(&rt, cx);
    view.read_with(cx, |v, cx| {
        assert_eq!(v.input.read(cx).content.as_ref(), " echo fixture");
        assert_eq!(v.terminal_prompt, "fixture@synthetic /synthetic/project %");
    });
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |v, _| assert!(matches!(&v.navigation,Some(Navigation{surface:Surface::Terminal,payload:Payload::Terminal{command:Some(command),cwd:Some(cwd),session:None,..},..}) if command=="echo fixture"&&cwd=="/synthetic/project")));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.return_home(w, cx);
            v.edit("/synthetic/project  summarize fixture", w, cx);
        })
    });
    cx.simulate_keystrokes("escape");
    assert_eq!(
        view.read_with(cx, |v, _| v.model.input.clone()),
        "/synthetic/project  "
    );
    cx.simulate_keystrokes("escape");
    assert_eq!(
        view.read_with(cx, |v, _| v.model.input.clone()),
        "/synthetic/project"
    );
}
#[gpui::test]
fn query_recall_respects_top_boundary_and_ai_pin_shortcut_is_inert(cx: &mut TestAppContext) {
    let _rt = runtime(cx);
    let adapter = Arc::new(Fixture::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let row = Row::command("pin", "Saved");
            seed(v, vec![row.clone(), Row::command("next", "Next")]);
            ui_state::insert_pin(&mut v.state.pins, &row, None);
            v.model.input.clear();
            v.state.record_launcher_query("last query");
            v.model.session.selected = 1;
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("up");
    assert_eq!(view.read_with(cx, |v, _| v.model.session.selected), 0);
    assert!(view.read_with(cx, |v, _| v.model.input.is_empty()));
    cx.simulate_keystrokes("up");
    assert_eq!(
        view.read_with(cx, |v, _| v.model.input.clone()),
        "last query"
    );
    cx.update(|w, cx| view.update(cx, |v, cx| v.edit(" prompt", w, cx)));
    cx.simulate_keystrokes("alt-1");
    assert!(
        !adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| c == "search:execute")
    );
    assert!(view.read_with(cx, |v, _| v.navigation.is_none()));
}

#[gpui::test]
fn native_result_menu_quicklook_uses_clicked_row_and_list_scroll_keeps_selection(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            seed(
                v,
                (0..500)
                    .map(|index| {
                        let mut row =
                            Row::command(&format!("file-{index}"), &format!("Fixture {index}"));
                        row.category = "files".into();
                        row.action = Action::OpenFile {
                            path: format!("/synthetic/{index}.txt"),
                        };
                        row
                    })
                    .collect(),
            );
            v.input.focus_handle(cx).focus(w, cx);
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("result-499").is_none(),
        "the entire list must not be painted"
    );
    let second = cx.debug_bounds("result-1").unwrap().center();
    cx.simulate_mouse_down(second, MouseButton::Right, Modifiers::default());
    cx.simulate_keystrokes("down down down enter");
    settle(&rt, cx);
    let calls = adapter.calls.lock().unwrap();
    let quicklook = calls.iter().find(|(c, _)| c == "file:quick-look").unwrap();
    assert_eq!(quicklook.1[0][0], "/synthetic/1.txt");
    drop(calls);
    let third = cx.debug_bounds("result-2").unwrap().center();
    cx.simulate_mouse_move(third, None, Modifiers::default());
    assert_eq!(view.read_with(cx, |v, _| v.model.session.selected), 2);
    view.update(cx, |v, cx| {
        v.scroll.scroll_to_item(200, ScrollStrategy::Top);
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_mouse_move(third, None, Modifiers::default());
    assert_eq!(
        view.read_with(cx, |v, _| v.model.session.selected),
        2,
        "stationary pointer/scroll cannot select the row newly under it"
    );
}
#[gpui::test]
fn native_path_sections_keep_row_height_and_keyboard_identity(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter, relay, None, w, cx));
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
    view.update(cx, |v, cx| {
        let path = v.paths.get_mut("path:fixture").unwrap();
        path["section"] = json!("recommended");
        v.path_list.reset(v.rows.len());
        cx.notify();
    });
    cx.run_until_parked();
    let heading = cx
        .debug_bounds("path-section-0")
        .expect("completion section heading");
    let row = cx.debug_bounds("result-0").unwrap();
    assert_eq!(row.size.height, px(36.));
    assert!(row.origin.y >= heading.bottom());
    cx.simulate_keystrokes("tab");
    assert_eq!(
        view.read_with(cx, |v, _| v.model.input.clone()),
        "/fixture/"
    );
}

#[gpui::test]
fn native_command_ids_route_and_record_usage_once(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    for (command, surface) in [
        ("open-settings", Surface::Settings("general".into())),
        ("open-providers", Surface::Settings("ai".into())),
        ("open-settings-updates", Surface::Settings("general".into())),
        (
            "open-extensions-settings",
            Surface::Settings("extensions".into()),
        ),
        ("open-extensions", Surface::Extensions),
        ("open-snippets", Surface::Snippets),
        ("open-vault", Surface::Vault),
        ("open-notes", Surface::Notes),
        ("open-quick-notes", Surface::Notes),
        ("open-emoji-picker", Surface::Emoji),
        ("open-indexing", Surface::Indexing),
        ("start-onboarding", Surface::Onboarding),
        ("show-updated-version", Surface::ReleaseNotes),
        ("show-system-monitor", Surface::SystemMonitor),
        ("open-clipboard-history", Surface::Clipboard),
    ] {
        cx.update(|w, cx| {
            view.update(cx, |v, cx| {
                v.navigation = None;
                seed(v, vec![Row::command(command, command)]);
                v.input.focus_handle(cx).focus(w, cx);
            })
        });
        cx.simulate_keystrokes("enter");
        settle(&rt, cx);
        view.read_with(cx, |v, _| {
            assert_eq!(
                v.navigation.as_ref().map(|n| &n.surface),
                Some(&surface),
                "{command}"
            )
        });
        let calls = adapter.calls.lock().unwrap();
        assert_eq!(
            calls
                .iter()
                .filter(|(channel, args)| channel == "search:record-usage"
                    && args[0]["action"]["commandId"] == command)
                .count(),
            1,
            "{command}"
        );
        assert!(
            !calls.iter().any(|(channel, _)| channel == "search:execute"),
            "locally routed commands must not execute twice"
        );
    }
}
#[gpui::test]
fn update_command_activation_uses_current_status_not_saved_row_id(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, mut rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter, relay, None, w, cx));
    for command in [
        "download-app-update",
        "restart-app-update",
        "check-for-updates",
    ] {
        cx.update(|w, cx| {
            view.update(cx, |v, cx| {
                seed(v, vec![Row::command(command, "Saved update command")]);
                v.update_status = tezbar_update::AppUpdateStatus::Ready {
                    version: "synthetic".into(),
                };
                v.input.focus_handle(cx).focus(w, cx);
            })
        });
        cx.simulate_keystrokes("enter");
        let action = rt.block_on(async {
            tokio::time::timeout(Duration::from_millis(100), async {
                loop {
                    if let Some(Event::Update(action)) = rx.recv().await {
                        return action;
                    }
                }
            })
            .await
            .unwrap()
        });
        assert_eq!(action, UpdateAction::Restart);
    }
}

#[gpui::test]
fn backend_epoch_retires_native_form_and_answer_owners(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    cx.update(|cx| cx.set_global(CoreEvents::new(relay.clone())));
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter, relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let mut row = Row::command("argument", "Synthetic argument");
            row.action = Action::RunExtensionCommand {
                extension_id: "fixture".into(),
                command_name: "argument".into(),
                extra: serde_json::from_value(
                    json!({"commandArgumentDefinitions":[{"name":"value","required":true}]}),
                )
                .unwrap(),
            };
            seed(v, vec![row]);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("enter");
    assert!(view.read_with(cx, |v, _| v.form.is_some()));
    cx.update(|_, cx| {
        let hub = cx.global::<CoreEvents>().clone();
        hub.publish(CoreEvent::Backend(json!({"status":"ready"})), cx);
    });
    view.read_with(cx, |v, _| {
        assert!(v.form.is_none());
        assert!(v.form_activation.is_none());
        assert!(!v.model.session.activating);
    });
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            seed(v, vec![]);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("enter");
    settle(&rt, cx);
    let owner = view.read_with(cx, |v, _| v.answer_owner.clone().unwrap());
    cx.update(|_, cx| {
        let hub = cx.global::<CoreEvents>().clone();
        hub.publish(CoreEvent::Backend(json!({"status":"ready"})), cx);
        hub.publish(
            CoreEvent::Other {
                channel: "stream-token".into(),
                payload: json!({"owner":owner,"value":"retired answer"}),
            },
            cx,
        );
    });
    view.read_with(cx, |v, _| {
        assert!(!v.answer_active);
        assert!(!v.answer_busy);
        assert!(v.answer_owner.is_none());
        assert!(v.answer_text.is_empty());
    });
}

#[gpui::test]
fn argument_escape_retires_declared_filter_and_late_contribution_owner(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    cx.update(|cx| cx.set_global(CoreEvents::new(relay.clone())));
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter, relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let mut row = Row::command("argument", "Synthetic argument");
            row.action = Action::RunExtensionCommand {
                extension_id: "fixture".into(),
                command_name: "argument".into(),
                extra: serde_json::from_value(
                    json!({"commandArgumentDefinitions":[{"name":"value","required":true}]}),
                )
                .unwrap(),
            };
            seed(v, vec![row]);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("enter");
    cx.simulate_input("old argument");
    settle(&rt, cx);
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |v, _| {
        assert_eq!(v.form.as_ref().unwrap().args.values[0], "")
    });
    cx.simulate_keystrokes("escape");
    let generation = view.read_with(cx, |v, _| {
        assert!(v.form.is_none());
        assert!(v.argument_task.is_none());
        assert!(v.argument_rows.is_empty());
        v.model.session.generation
    });
    cx.update(|_, cx| {
        cx.global::<CoreEvents>().clone().publish(
            CoreEvent::Invalidated {
                text: Some("old argument".into()),
            },
            cx,
        )
    });
    assert_eq!(
        view.read_with(cx, |v, _| v.model.session.generation),
        generation
    );
}

#[gpui::test]
fn saved_note_pin_preserves_note_id_and_quit_command_only_emits_explicit_intent(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, mut rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let mut note = Row::command("saved-note", "Saved note");
            note.id = "note:12345".into();
            note.category = "quick-notes".into();
            seed(v, vec![]);
            ui_state::insert_pin(&mut v.state.pins, &note, None);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("alt-1");
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| matches!(
        &v.navigation,
        Some(Navigation {
            surface: Surface::Notes,
            payload: Payload::Note { created_at: 12345 },
            ..
        })
    )));
    let calls = adapter.calls.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .filter(|(c, args)| c == "search:record-usage"
                && args[0]["context"]["resultId"] == "note:12345")
            .count(),
        1
    );
    assert!(!calls.iter().any(|(c, _)| c == "search:execute"));
    drop(calls);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.navigation = None;
            seed(v, vec![Row::command("quit-tezbar", "Quit Tezbar")]);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("enter");
    settle(&rt, cx);
    assert_eq!(
        adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(channel, _)| channel == "app:confirm-quit")
            .count(),
        1
    );
    let direct_quit = rt.block_on(async {
        tokio::time::timeout(Duration::from_millis(5), async {
            while let Some(event) = rx.recv().await {
                if matches!(event, Event::Quit) {
                    return true;
                }
            }
            false
        })
        .await
        .unwrap_or(false)
    });
    assert!(
        !direct_quit,
        "The core confirmation must own the quit decision"
    );
}

#[gpui::test]
fn native_action_variants_keep_the_canonical_execute_envelope(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    for (index, action) in [
        Action::OpenApp {
            app_name: "Synthetic App".into(),
            app_path: Some("/synthetic/App.app".into()),
        },
        Action::OpenFile {
            path: "/synthetic/file.txt".into(),
        },
        Action::OpenUrl {
            url: "https://fixture.invalid/legacy-quicklink".into(),
        },
        Action::OpenWithApp {
            path: "/synthetic/file.txt".into(),
            app_name: Some("Synthetic App".into()),
        },
        Action::CopyText {
            text: "Synthetic snippet".into(),
        },
        Action::CopyAndPasteText {
            text: "Synthetic paste".into(),
        },
        Action::AddNote {
            text: "Synthetic note".into(),
        },
        Action::RunNativeCommand {
            command_id: "fixture-native-command".into(),
        },
        Action::RunShell {
            command: "synthetic shell command".into(),
        },
        Action::InstallExtension {
            extra: serde_json::from_value(json!({"extensionId":"fixture.extension"})).unwrap(),
        },
    ]
    .into_iter()
    .enumerate()
    {
        cx.update(|w, cx| {
            view.update(cx, |v, cx| {
                let mut row = Row::command(&format!("action-{index}"), "Synthetic activation");
                row.action = action.clone();
                seed(v, vec![row]);
                v.input.focus_handle(cx).focus(w, cx);
            })
        });
        cx.simulate_keystrokes("enter");
        settle(&rt, cx);
        let calls = adapter.calls.lock().unwrap();
        let executions = calls
            .iter()
            .filter(|(channel, _)| channel == "search:execute")
            .collect::<Vec<_>>();
        assert_eq!(executions.len(), index + 1);
        let envelope = &executions[index].1;
        assert_eq!(envelope.len(), 1);
        assert_eq!(envelope[0]["action"], json!(action));
        assert_eq!(
            envelope[0]["context"]["resultId"],
            format!("command:action-{index}")
        );
        assert_eq!(envelope[0]["context"]["rank"], 1);
    }
}
#[gpui::test]
fn failed_start_retry_keeps_editable_input_and_emits_only_retry_intent(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, mut rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.visible = true;
            v.ready = false;
            v.error = Some("Synthetic startup failure".into());
            v.input.focus_handle(cx).focus(w, cx);
            cx.notify();
        })
    });
    cx.simulate_input("preserved query");
    cx.run_until_parked();
    let button = cx.debug_bounds("retry-core").unwrap().center();
    cx.simulate_mouse_down(button, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(button, MouseButton::Left, Modifiers::default());
    rt.block_on(async {
        tokio::time::timeout(Duration::from_millis(100), async {
            loop {
                if matches!(rx.recv().await, Some(Event::RetryCore)) {
                    break;
                }
            }
        })
        .await
        .unwrap()
    });
    view.read_with(cx, |v, cx| {
        assert_eq!(v.input.read(cx).content.as_ref(), "preserved query")
    });
    assert!(adapter.calls.lock().unwrap().is_empty());
}

#[gpui::test]
fn deep_recommendation_is_model_backed_and_enters_hidden_prefix_mode(cx: &mut TestAppContext) {
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
            v.edit("unmatched fixture phrase", w, cx);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.executor().advance_clock(Duration::from_millis(8));
    settle(&rt, cx);
    view.read_with(cx, |v, _| {
        assert!(v.rows[0].id.starts_with("deep-search:"));
        assert!(!v.rows[0].pinnable());
    });
    cx.simulate_keystrokes("enter");
    settle(&rt, cx);
    view.read_with(cx, |v, cx| {
        assert_eq!(v.model.input, "!unmatched fixture phrase");
        assert_eq!(
            v.input.read(cx).content.as_ref(),
            "unmatched fixture phrase"
        );
    });
    let calls = adapter.calls.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .filter(|(c, _)| c == "search:record-usage")
            .count(),
        1
    );
    assert!(!calls.iter().any(|(c, _)| c == "search:execute"));
}

#[gpui::test]
fn native_argument_dropdown_keeps_default_and_submits_wire_value_not_title(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w,cx|view.update(cx,|v,cx|{
        let mut row=Row::command("dropdown","Synthetic dropdown");
        row.action=Action::RunExtensionCommand{extension_id:"fixture".into(),command_name:"dropdown".into(),extra:serde_json::from_value(json!({"commandArgumentDefinitions":[{"name":"choice","title":"Choose option","type":"dropdown","required":true,"default":"wire-a","data":[{"value":"wire-a","title":"First display title"},{"value":"wire-b","title":"Second display title"}]},{"name":"text","title":"Text","required":true,"defaultValue":"synthetic default"}]})).unwrap()};
        seed(v,vec![row]);v.input.focus_handle(cx).focus(w,cx);
    }));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |v, _| {
        assert_eq!(
            v.form.as_ref().unwrap().args.values,
            ["wire-a", "synthetic default"]
        )
    });
    cx.simulate_keystrokes("space down enter");
    view.read_with(cx, |v, _| {
        let form = v.form.as_ref().unwrap();
        assert_eq!(form.args.values[0], "wire-b");
        assert!(form.dropdown.is_none());
    });
    cx.simulate_keystrokes("tab enter");
    settle(&rt, cx);
    let calls = adapter.calls.lock().unwrap();
    let runs = calls
        .iter()
        .filter(|(c, _)| c == "extension:run-command")
        .collect::<Vec<_>>();
    assert_eq!(runs.len(), 1);
    assert_eq!(
        runs[0].1[0]["argumentValues"],
        json!({"choice":"wire-b","text":"synthetic default"})
    );
    assert_eq!(
        calls
            .iter()
            .filter(|(c, _)| c == "search:record-usage")
            .count(),
        1
    );
}

#[gpui::test]
fn cached_ai_history_paints_on_reentry_before_its_new_request_completes(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter, relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.ready = true;
            v.show(w, cx);
        })
    });
    cx.executor().advance_clock(Duration::from_millis(8));
    settle(&rt, cx);
    cx.update(|w, cx| view.update(cx, |v, cx| v.edit(" ", w, cx)));
    cx.executor().advance_clock(Duration::from_millis(8));
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.edit("", w, cx);
            v.edit(" ", w, cx);
        })
    });
    view.read_with(cx, |v, _| {
        assert!(v.model.session.pending());
        assert_eq!(v.rows[0].id, "chat:chat-fixture");
    });
}

#[derive(Default)]
struct DeferredDirectory {
    fixture: Loopback,
    reply: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<Value>>>,
}
impl RequestAdapter for DeferredDirectory {
    fn send(
        &self,
        request: Request,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>> {
        if matches!(request, Request::ResolveDirectory(_)) {
            let (tx, rx) = tokio::sync::oneshot::channel();
            *self.reply.lock().unwrap() = Some(tx);
            Box::pin(async move { rx.await.map_err(|error| error.to_string()) })
        } else {
            self.fixture.send(request)
        }
    }
}
#[gpui::test]
fn typed_command_return_retires_directory_ticket_and_clears_answer_resources(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let adapter = Arc::new(DeferredDirectory::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            seed(v, vec![]);
            v.edit("/synthetic  pending task", w, cx);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("enter");
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| v.model.session.activating));
    view.update(cx, |v, cx| {
        v.answer_text = "synthetic retired answer".into();
        v.update_answer(true, cx);
        v.receive_navigation(
            Navigation {
                nonce: 123,
                surface: Surface::Command,
                payload: Payload::None,
            },
            cx,
        );
    });
    cx.run_until_parked();
    view.read_with(cx, |v, cx| {
        assert!(!v.model.session.activating);
        assert!(v.model.input.is_empty());
        assert!(v.answer_text.is_empty());
        assert!(v.answer.read(cx).document.blocks.is_empty());
    });
    adapter
        .reply
        .lock()
        .unwrap()
        .take()
        .unwrap()
        .send(json!("/synthetic"))
        .unwrap();
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| v.navigation.is_none()));
}

#[gpui::test]
fn icon_requests_follow_virtual_viewport_past_initial_cap_and_stop_while_idle(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let adapter = Arc::new(Loopback::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    view.update(cx, |v, cx| {
        seed(
            v,
            (0..500)
                .map(|index| {
                    let mut row = Row::command(&format!("file-{index}"), "Fixture file");
                    row.action = Action::OpenFile {
                        path: format!("/synthetic/{index}.txt"),
                    };
                    row
                })
                .collect(),
        );
        v.defer_icon_viewport(cx);
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
    });
    settle(&rt, cx);
    view.update(cx, |v, cx| {
        v.scroll.scroll_to_item(200, ScrollStrategy::Top);
        v.defer_icon_viewport(cx);
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
    });
    settle(&rt, cx);
    let count = {
        let calls = adapter.calls.lock().unwrap();
        let assets = calls
            .iter()
            .filter(|(c, _)| c == "asset-icon:data-url")
            .collect::<Vec<_>>();
        assert!(
            assets
                .iter()
                .any(|(_, args)| args[0]["path"] == "/synthetic/200.txt")
        );
        assert!(
            assets.len() < 120,
            "admission follows the viewport, not all search rows"
        );
        assets.len()
    };
    assert!(view.read_with(cx, |v, _| v.icon_admitted.len() <= 120));
    for _ in 0..5 {
        view.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
    }
    settle(&rt, cx);
    assert_eq!(
        adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| c == "asset-icon:data-url")
            .count(),
        count
    );
    cx.update(|w, cx| view.update(cx, |v, cx| v.hide(w, cx)));
    view.read_with(cx, |v, _| {
        assert!(v.icons.is_empty());
        assert!(v.icon_admitted.is_empty());
        assert!(v.icon_queue.is_empty());
    });
}

include!("review_regressions.rs");
