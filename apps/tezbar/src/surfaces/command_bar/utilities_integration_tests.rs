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
fn parent_utility_hide_and_show_resume_the_same_native_note_editor(cx: &mut TestAppContext) {
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
    cx.update(|w, cx| view.update(cx, |view, cx| view.hide(w, cx)));
    assert!(!utility.read_with(cx, |utility, _| utility.visible));
    let reads = adapter
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(channel, _)| channel == "search:all")
        .count();
    cx.update(|w, cx| view.update(cx, |view, cx| view.show(w, cx)));
    settle(&rt, cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.utilities.as_ref(), Some(&utility))
    });
    cx.update(|w, cx| {
        utility.read_with(cx, |utility, cx| {
            assert!(utility.visible);
            assert_eq!(utility.surface, Surface::Notes);
            assert_eq!(utility.notes.selected, Some(11));
            assert!(utility.editor.focus_handle(cx).is_focused(w));
        })
    });
    assert!(cx.debug_bounds("drag-handle").is_none());
    assert_eq!(
        adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(channel, _)| channel == "search:all")
            .count(),
        reads
    );
    cx.simulate_input("native resume");
    settle(&rt, cx);
    assert!(utility.read_with(cx, |utility, _| {
        utility
            .notes
            .document()
            .unwrap()
            .html()
            .contains("native resume")
    }));
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

#[gpui::test]
fn system_info_aliases_mount_monitor_and_restore_launcher_focus_without_shell(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let adapter = Arc::new(Fixture::default());
    let (relay, mut rx) = Relay::new();
    cx.update(|cx| cx.set_global(navigation::NavigationInbox::default()));
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    for command in [
        "show-system-monitor",
        "show-cpu-info",
        "show-macos-version",
        "show-memory-info",
        "show-disk-usage",
        "show-battery-status",
    ] {
        for native in [false, true] {
            cx.update(|w, cx| {
                view.update(cx, |v, cx| {
                    v.ready = true;
                    v.visible = true;
                    v.utility_transport = Some(adapter.clone());
                    v.model.visible = true;
                    v.model.input = "system".into();
                    v.model.session.commit("system".into(), 0);
                    v.model.session.take_request(8);
                    let mut row = Row::command(command, command);
                    if native {
                        row.action = Action::RunNativeCommand {
                            command_id: command.into(),
                        };
                    }
                    let generation = v.model.session.generation;
                    v.dispatch(
                        Input::SearchComplete {
                            generation,
                            result: Ok(vec![row]),
                        },
                        cx,
                    );
                    v.input.focus_handle(cx).focus(w, cx);
                })
            });
            cx.simulate_keystrokes("enter");
            settle(&rt, cx);
            let intent = rt.block_on(async {
                loop {
                    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
                        .await
                        .unwrap()
                        .unwrap();
                    if let Event::Navigate(intent) = event {
                        break intent;
                    }
                }
            });
            assert_eq!(
                intent.surface,
                Surface::SystemMonitor,
                "{command} native={native}"
            );
            view.update(cx, |v, cx| v.receive_navigation(intent, cx));
            settle(&rt, cx);
            cx.update(|w, cx| {
                view.read_with(cx, |v, cx| {
                    let utility = v.utilities.as_ref().expect("Monitor must mount").read(cx);
                    assert_eq!(utility.surface, Surface::SystemMonitor);
                    assert!(utility.visible);
                    assert!(
                        utility.focus_handle(cx).is_focused(w),
                        "Monitor owns keyboard focus"
                    );
                    assert!(!v.input.focus_handle(cx).is_focused(w));
                    assert!(
                        v.notice.is_none(),
                        "System aliases must not produce stdout content"
                    );
                    assert!(v.message_kind.is_none());
                })
            });
            cx.simulate_keystrokes("escape");
            settle(&rt, cx);
            cx.update(|w, cx| {
                view.read_with(cx, |v, cx| {
                    assert!(v.navigation.is_none());
                    assert!(
                        v.input.focus_handle(cx).is_focused(w),
                        "Back restores launcher focus"
                    );
                })
            });
        }
    }
    assert!(
        !adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|(channel, _)| channel == "search:execute"),
        "Aliases must never execute shell commands"
    );
}

#[gpui::test]
fn loaded_monitor_through_launcher_keeps_narrow_header_cards_and_footer_in_viewport(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let adapter = Arc::new(Fixture::default());
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    view.update(cx, |v, cx| {
        v.ready = true;
        v.visible = true;
        v.zoom = 0.85;
        v.utility_transport = Some(adapter);
        v.receive_navigation(nav(1, Surface::SystemMonitor, Payload::None), cx);
    });
    settle(&rt, cx);
    let utility = view.read_with(cx, |v, _| v.utilities.clone().unwrap());
    utility.update(cx, |v, cx| {
        v.monitor.set_auto_refresh(false, 0);
        v.monitor.value = Some(json!({
            "device":{"name":"Fixture MacBook Pro","chip":"Apple M4 Pro","model":"MacBook Pro (16-inch, 2024)","osVersion":"macOS 26.1","platform":"macOS"},
            "cpu":{"usagePercent":37.5,"userPercent":29.4,"systemPercent":8.1,"totalCores":14,"model":"Apple M4 Pro 14-core CPU","loadAverage":[2.42,2.25,2.1]},
            "memory":{"usedBytes":28672000000_u64,"totalBytes":51539607552_u64,"freeBytes":22867607552_u64},
            "battery":{"chargePercent":87,"condition":"Normal","cycleCount":112,"maximumCapacityPercent":98},
            "storage":{"freeBytes":321234567890_u64,"usedBytes":678765432110_u64,"totalBytes":1000000000000_u64,"smartStatus":"Verified","device":"APPLE SSD AP1024Z"},
            "gpu":{"model":"Apple M4 Pro","cores":20,"display":"Built-in Liquid Retina XDR 3456 × 2234"},
            "network":{"downloadBytesPerSecond":1234567,"uploadBytesPerSecond":345678},"processes":{"total":623,"running":7}
        }));
        cx.notify();
    });
    let handle = cx.update(|w, _| w.window_handle());
    for (width, height) in [(467., 700.), (900., 900.), (467., 458.), (467., 700.)] {
        cx.simulate_window_resize(handle, size(px(width), px(height)));
        cx.run_until_parked();
        let viewport = cx.update(|w, _| w.viewport_size());
        for selector in [
            "Live",
            "Refresh",
            "monitor-stat-cpu",
            "monitor-stat-memory",
            "monitor-stat-battery",
            "monitor-status",
        ] {
            let bounds = cx
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("Missing {selector}"));
            assert!(
                bounds.left() >= px(0.) && bounds.right() <= viewport.width + px(0.5),
                "{selector} must fit viewport{viewport:?}; bounds{bounds:?}"
            );
        }
        let cpu = cx.debug_bounds("monitor-stat-cpu").unwrap();
        let memory = cx.debug_bounds("monitor-stat-memory").unwrap();
        let battery = cx.debug_bounds("monitor-stat-battery").unwrap();
        assert!(
            (cpu.top() - memory.top()).abs() < px(1.)
                && (memory.top() - battery.top()).abs() < px(1.),
            "Keep source three-column summary"
        );
        let footer = cx.debug_bounds("monitor-status").unwrap();
        let monitor = cx.debug_bounds("system-monitor").unwrap();
        // Monitor owns the window's only sheet; its footer sits inside that border.
        assert_eq!(monitor.origin, point(px(0.), px(0.)));
        assert_eq!(monitor.size, viewport);
        assert!(
            (footer.bottom() - (monitor.bottom() - px(1.))).abs() < px(0.5),
            "Footer remains anchored inside the monitor border"
        );
    }
}
