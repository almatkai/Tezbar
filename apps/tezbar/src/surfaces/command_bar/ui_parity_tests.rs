use super::*;
use ::core::prelude::v1::test;

struct Fixture;
impl RequestAdapter for Fixture {
    fn send(
        &self,
        _: Request,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>> {
        Box::pin(async { Ok(json!({"ok":true})) })
    }
}
fn complete(v: &mut Launcher, rows: Vec<Row>, cx: &mut Context<Launcher>) {
    complete_query(v, "fixture", rows, cx);
}
fn complete_query(v: &mut Launcher, query: &str, rows: Vec<Row>, cx: &mut Context<Launcher>) {
    v.ready = true;
    v.visible = true;
    v.model.visible = true;
    v.model.input = query.into();
    v.model.session.commit(query.into(), 0);
    v.model.session.take_request(8);
    let generation = v.model.session.generation;
    v.dispatch(
        Input::SearchComplete {
            generation,
            result: Ok(rows),
        },
        cx,
    );
}
#[gpui::test]
fn application_grid_keeps_seven_equal_columns_and_bounded_titles(cx: &mut TestAppContext) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| {
        gpui_tokio::init_from_handle(cx, rt.handle().clone());
        tezbar_ui::text_input::bind_keys(cx);
    });
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(Arc::new(Fixture), relay, None, w, cx));
    let window = cx.update(|w, _| w.window_handle());
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            complete_query(
                v,
                "`",
                (0..16)
                    .map(|i| {
                        Row::command(
                            &format!("application-{i}"),
                            match i % 3 {
                                0 => "An application with a very long name that must never widen the grid",
                                1 => "AOSUIPrefPaneLauncherBluetoothSetupAssistantXXXX",
                                _ => "Notes",
                            },
                        )
                    })
                    .collect(),
                cx,
            );
            v.input.focus_handle(cx).focus(w, cx);
        });
    });
    for (width, zoom) in [(760., 1.), (520., 1.), (760., 1.5), (520., 1.5)] {
        cx.simulate_window_resize(window, size(px(width), px(900.)));
        view.update(cx, |v, cx| {
            v.zoom = zoom;
            cx.notify();
        });
        cx.run_until_parked();
        let first = cx.debug_bounds("result-0").unwrap();
        let mut right = first.left();
        for selector in [
            "result-0", "result-1", "result-2", "result-3", "result-4", "result-5",
            "result-6", "result-14", "result-15",
        ] {
            let tile = cx.debug_bounds(selector).expect("Visible app tile");
            assert!(
                (tile.size.width - first.size.width).abs() <= px(1.),
                "All columns, including the last partial row, must have equal width: {selector} at {width}px/{zoom}: {tile:?}, first={first:?}"
            );
            assert!(tile.left() >= px(0.) && tile.right() <= px(width));
            assert_eq!(tile.size.height, px(96. * zoom));
            if selector != "result-14" && selector != "result-15" {
                assert!(tile.left() >= right - px(1.));
                right = tile.right();
            }
        }
        for (tile, text) in [
            ("result-0", "result-text-0"),
            ("result-1", "result-text-1"),
            ("result-14", "result-text-14"),
        ] {
            let tile = cx.debug_bounds(tile).unwrap();
            let text = cx.debug_bounds(text).unwrap();
            assert!(text.left() >= tile.left() && text.right() <= tile.right());
            assert!(text.size.height <= px(32. * zoom));
            assert!(text.top() >= tile.top() && text.bottom() <= tile.bottom());
        }
    }
    cx.update(|w, cx| {
        view.update(cx, |v, cx| v.input.focus_handle(cx).focus(w, cx));
    });
    cx.run_until_parked();
    for (key, selected) in [("down", 7), ("right", 8), ("up", 1), ("left", 0)] {
        cx.simulate_keystrokes(key);
        cx.run_until_parked();
        assert_eq!(view.read_with(cx, |v, _| v.model.session.selected), selected);
    }
}

#[gpui::test]
fn mixed_instant_rows_remeasure_same_count_replacement_and_reveal_selection(
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
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(Arc::new(Fixture), relay, None, w, cx));
    let rows = || {
        (0..60)
            .map(|i| Row::command(&format!("fixture-{i}"), "Fixture"))
            .collect::<Vec<_>>()
    };
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let mut mixed = rows();
            mixed[0].category = "calculator".into();
            mixed[0].title = "42".into();
            mixed[0].subtitle = "6 * 7".into();
            mixed[0].action = Action::CopyText { text: "42".into() };
            complete(v, mixed, cx);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("result-0").unwrap().size.height, px(52.));
    assert_eq!(cx.debug_bounds("result-1").unwrap().size.height, px(36.));
    assert!(
        cx.debug_bounds("result-59").is_none(),
        "Rows remain virtualized"
    );
    cx.update(|_, cx| view.update(cx, |v, cx| complete(v, rows(), cx)));
    cx.run_until_parked();
    assert_eq!(
        cx.debug_bounds("result-0").unwrap().size.height,
        px(36.),
        "Same-count replacement must discard measured instant height"
    );
    cx.simulate_keystrokes(
        &std::iter::repeat_n("down", 40)
            .collect::<Vec<_>>()
            .join(" "),
    );
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |v, _| v.model.session.selected), 40);
    assert!(
        cx.debug_bounds("result-40").is_some(),
        "Keyboard reveal follows the variable-height list"
    );
}

#[gpui::test]
fn command_content_stays_in_card_and_plain_status_stays_in_footer(cx: &mut TestAppContext) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| gpui_tokio::init_from_handle(cx, rt.handle().clone()));
    let (relay, _rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(Arc::new(Fixture), relay, None, w, cx));
    view.update(cx, |v, cx| {
        complete(v, vec![], cx);
        v.card_message("Package A\nPackage B".into(), Some("info".into()), 7000, cx);
        assert_eq!(v.notice.as_deref(), Some("Package A\nPackage B"));
        assert!(
            v.footer_notice().is_none(),
            "Content must not be repeated in the narrow footer"
        );
        assert!(
            !v.message_document.read(cx).document.blocks.is_empty(),
            "Shell content remains available in its document"
        );
        v.message("First line\nSecond line".into(), 4000, cx);
        assert!(
            v.message_kind.is_some(),
            "Multiline notices require the content card"
        );
        assert!(v.footer_notice().is_none());
        v.message("Copied".into(), 2500, cx);
        assert_eq!(v.footer_notice(), Some("Copied"));
        assert!(v.message_kind.is_none());
        v.error = Some("A required argument is missing".into());
        assert_eq!(v.error.as_deref(), Some("A required argument is missing"));
    });
}

mod footer_layout {
    use super::*;
    use crate::surfaces::command_bar::chat_integration::tests as mounted;
    struct FailedTerminal(Arc<mounted::Backend>);
    impl RequestAdapter for FailedTerminal {
        fn send(&self, request: Request) -> crate::surfaces::ai_chat::ChatFuture {
            let (channel, _) = request.clone().wire();
            let summary = json!({"sessionId":"layout-session","cwd":"/fixture","name":"Fixture shell","shell":"/synthetic/shell","status":"running","saveFor":"forever","keepAliveFor":"until-stop","createdAt":1,"updatedAt":2});
            if channel == "terminal:list" {
                return Box::pin(async move { Ok(json!([summary])) });
            }
            if channel == "core:terminal:stream-attach" {
                return Box::pin(async move {
                    Ok(
                        json!({"sessionId":"layout-session","summary":summary,"generation":"layout-generation","replay":"layout-replay","watermark":0}),
                    )
                });
            }
            if channel.starts_with("terminal:") || channel.starts_with("core:terminal:") {
                return Box::pin(async { Err("Synthetic terminal unavailable".into()) });
            }
            if channel.starts_with("extension:") {
                return Box::pin(async { Err("Synthetic extension catalog unavailable".into()) });
            }
            self.0.send(request)
        }
    }
    fn assert_bottom(cx: &mut VisualTestContext, selector: &'static str) {
        let viewport = cx.update(|window, _| window.viewport_size());
        let bar = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("Missing bottom action/status bar {selector}"));
        let inset = viewport.height - bar.bottom();
        assert!(inset >= px(-0.5) && inset <= px(2.1), "{selector} must meet the window bottom (only sheet borders allowed): viewport={viewport:?}, footer={bar:?}, inset={inset:?}");
    }
    #[gpui::test]
    fn terminal_error_and_background_tasks_cannot_add_a_row_below_the_actionbar(
        cx: &mut TestAppContext,
    ) {
        let (rt, _root, backend, launcher, cx) = mounted::mount(cx);
        launcher.update(cx, |view, cx| {
            view.adapter = Arc::new(FailedTerminal(backend));
            view.receive_navigation(
                Navigation {
                    nonce: 1,
                    surface: Surface::Terminal,
                    payload: Payload::Terminal {
                        command: None,
                        cwd: None,
                        session: None,
                        defaults: Default::default(),
                    },
                },
                cx,
            );
        });
        mounted::settle(&rt, cx);
        let terminal = launcher.read_with(cx, |view, _| {
            view.terminal
                .view
                .clone()
                .expect("Real parent-mounted Terminal")
        });
        assert!(
            terminal.read_with(cx, |view, _| view.error().is_some()),
            "The actual failed synthetic transport produces the terminal error state"
        );
        launcher.update(cx, |view, cx| {
            view.task_indicator.update(cx, |indicator, cx| {
                indicator.set_visible(true, cx);
                indicator.set_update(
                    &tezbar_update::AppUpdateStatus::Ready {
                        version: "fixture".into(),
                    },
                    cx,
                );
            })
        });
        cx.run_until_parked();
        let handle = cx.update(|window, _| window.window_handle());
        for (width, height) in [(467., 458.), (900., 900.), (467., 700.), (467., 458.)] {
            cx.simulate_window_resize(handle, size(px(width), px(height)));
            cx.run_until_parked();
            assert_bottom(cx, "surface-footer");
            let footer = cx.debug_bounds("surface-footer").unwrap();
            for selector in ["terminal-error", "background-task-toggle"] {
                let bounds = cx.debug_bounds(selector).unwrap();
                assert!(
                    bounds.top() >= footer.top() && bounds.bottom() <= footer.bottom(),
                    "Status and tasks live inside the same bottom row"
                );
            }
        }
        let configure = cx
            .debug_bounds("terminal-configure-action")
            .expect("Actual Configure action");
        cx.simulate_click(configure.center(), Modifiers::default());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("terminal-configure").is_some(),
            "Real retained session enters its existing configure overlay"
        );
        resized_bottom(cx, "surface-footer");
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(cx.debug_bounds("terminal-configure").is_none());
        assert_bottom(cx, "surface-footer");
    }
    #[derive(Default)]
    struct UtilityLayout;
    impl crate::surfaces::utilities::UtilityTransport for UtilityLayout {
        fn send(
            &self,
            request: crate::surfaces::utilities::UtilityRequest,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = crate::surfaces::utilities::Reply> + Send>,
        > {
            let (channel, _) = request.wire();
            let reply = match channel {
                "clipboard:list"
                | "notes:list"
                | "snippets:list"
                | "vault:list"
                | "background-tasks:list" => json!([]),
                "vault:status" => json!({"supported":true,"unlocked":true}),
                "knowledge:sources:list" => {
                    json!({"sources":[],"total":0,"offset":0,"hasMore":false})
                }
                "knowledge:snapshot" => json!({"status":{"state":"idle"}}),
                "permissions:snapshot" => json!({"platform":"macOS","statuses":[]}),
                "system-stats:get" => {
                    json!({"cpu":{"usagePercent":1},"processes":{"total":1,"running":1}})
                }
                _ => json!(true),
            };
            Box::pin(async move { Ok(reply) })
        }
        fn release(
            &self,
            _: String,
            _: &'static str,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send>>
        {
            Box::pin(async { Err("Synthetic release unavailable".into()) })
        }
    }
    fn resized_bottom(cx: &mut VisualTestContext, selector: &'static str) {
        let handle = cx.update(|window, _| window.window_handle());
        for (width, height) in [(467., 458.), (900., 900.), (467., 700.), (467., 458.)] {
            cx.simulate_window_resize(handle, size(px(width), px(height)));
            cx.run_until_parked();
            assert_bottom(cx, selector);
            if selector == "settings-footer" {
                let footer = cx.debug_bounds(selector).unwrap();
                let viewport = cx.update(|window, _| window.viewport_size());
                assert!(
                    (footer.left() - px(1.)).abs() < px(0.1)
                        && (footer.right() - (viewport.width - px(1.))).abs() < px(0.1)
                        && (footer.bottom() - (viewport.height - px(1.))).abs() < px(0.1),
                    "Mounted Settings must have one outer sheet border: {footer:?}, {viewport:?}"
                );
            }
            if matches!(
                selector,
                "surface-footer" | "utility-footer" | "monitor-status"
            ) {
                if let Some(header) = cx
                    .debug_bounds("utility-header")
                    .or_else(|| cx.debug_bounds("surface-header"))
                {
                    assert!(
                        (header.top() - px(1.)).abs() < px(0.1),
                        "Every screen has exactly one sheet border above its header: {header:?}"
                    );
                }
            }
            if let Some(transcript) = cx.debug_bounds("chat-empty-transcript") {
                let footer = cx.debug_bounds(selector).unwrap();
                assert!(
                    transcript.bottom() <= footer.top(),
                    "Empty chat content must shrink or scroll above its bottom controls"
                );
            }
        }
    }
    #[gpui::test]
    fn parent_mounted_command_chat_extensions_and_all_utility_bars_keep_the_window_bottom(
        cx: &mut TestAppContext,
    ) {
        let (rt, _root, backend, launcher, cx) = mounted::mount(cx);
        launcher.update(cx, |view, cx| {
            view.adapter = Arc::new(FailedTerminal(backend.clone()));
            view.configure_extensions(backend.core.clone());
            view.utility_transport = Some(Arc::new(UtilityLayout));
            cx.notify();
        });
        mounted::settle(&rt, cx);
        resized_bottom(cx, "surface-footer");
        let surfaces = [
            Surface::AiChat,
            Surface::Extensions,
            Surface::ExtensionRuntime,
            Surface::Clipboard,
            Surface::Snippets,
            Surface::Notes,
            Surface::Vault,
            Surface::Emoji,
            Surface::Indexing,
            Surface::Permissions,
            Surface::ReleaseNotes,
            Surface::SystemMonitor,
        ];
        for (index, surface) in surfaces.into_iter().enumerate() {
            let payload = match surface {
                Surface::AiChat => Payload::Ai {
                    prompt: None,
                    cwd: None,
                    resume: None,
                    capture: false,
                },
                Surface::ExtensionRuntime => Payload::Extension {
                    result: json!({"ok":true,"mode":"view","extensionId":"layout-fixture","commandName":"detail","sessionId":"layout-session","generation":"g","refreshMs":null,"root":{"type":"Detail","props":{"markdown":"Fixture body"}},"actions":[]}),
                },
                _ => Payload::None,
            };
            let selector = match surface {
                Surface::SystemMonitor => "monitor-status",
                Surface::Clipboard
                | Surface::Snippets
                | Surface::Notes
                | Surface::Vault
                | Surface::Emoji
                | Surface::Indexing
                | Surface::Permissions
                | Surface::ReleaseNotes => "utility-footer",
                _ => "surface-footer",
            };
            launcher.update(cx, |view, cx| {
                view.receive_navigation(
                    Navigation {
                        nonce: index as u64 + 10,
                        surface: surface.clone(),
                        payload,
                    },
                    cx,
                )
            });
            mounted::settle(&rt, cx);
            resized_bottom(cx, selector);
            if let Some(utility) = launcher
                .read_with(cx, |view, _| view.utilities.clone())
                .filter(|_| matches!(selector, "utility-footer" | "monitor-status"))
            {
                utility.update(cx, |view,cx| { view.error=Some("Synthetic layout error with a long remediation message that must stay in the bottom bar".into()); cx.notify(); });
                cx.run_until_parked();
                resized_bottom(cx, selector);
                utility.update(cx, |view, cx| {
                    view.error = None;
                    match surface {
                        Surface::Clipboard => view.clipboard.loading = true,
                        Surface::Snippets => view.snippets.loading = true,
                        Surface::Notes => view.notes.loading = true,
                        Surface::Vault => view.vault.loading = true,
                        Surface::Indexing => view.indexing.loading = true,
                        Surface::Permissions => view.permissions.snapshot = None,
                        Surface::ReleaseNotes => {
                            view.release.model.state = tezbar_update::notes::State::Loading {
                                version: "fixture".into(),
                            }
                        }
                        Surface::SystemMonitor => {
                            view.monitor.set_auto_refresh(false, 0);
                            view.monitor.value = None;
                            view.monitor.error = None;
                        }
                        _ => {}
                    }
                    cx.notify();
                });
                cx.run_until_parked();
                resized_bottom(cx, selector);
                if matches!(surface, Surface::Snippets | Surface::Vault) {
                    cx.update(|window, cx| {
                        utility.update(cx, |view, cx| {
                            view.act(crate::surfaces::utilities::Action::New, window, cx)
                        })
                    });
                    mounted::settle(&rt, cx);
                    resized_bottom(cx, selector);
                    cx.update(|window, cx| {
                        utility.update(cx, |view, cx| {
                            view.act(crate::surfaces::utilities::Action::Cancel, window, cx)
                        })
                    });
                    mounted::settle(&rt, cx);
                }
            }
        }
    }
    #[gpui::test]
    fn settings_distinct_save_close_bar_keeps_bottom_for_every_tab_and_errors(
        cx: &mut TestAppContext,
    ) {
        use crate::surfaces::settings::{SettingsContent, SettingsStore, Tab};
        let rt = crate::surfaces::settings::tests::runtime(cx);
        let fixture = Arc::new(crate::surfaces::settings::tests::Fixture::default());
        let store = SettingsStore::new(fixture);
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let content = cx.new(|cx| {
                SettingsContent::new(store, &UiState::default(), window, cx)
            });
            let (relay, _events) = Relay::new();
            let mut shell = crate::windows::settings::Settings::new(relay, window, cx);
            shell.set_content(content, cx);
            shell
        });
        let settings = shell.read_with(cx, |view, _| view.content.clone().unwrap());
        for tab in Tab::ALL {
            cx.update(|window, cx| {
                settings.update(cx, |view, cx| view.activate_tab(tab, window, cx))
            });
            crate::surfaces::settings::tests::settle(&rt, cx);
            resized_bottom(cx, "settings-footer");
            settings.update(cx, |view, cx| {
                view.error = Some("Synthetic settings error".into());
                cx.notify();
            });
            cx.run_until_parked();
            resized_bottom(cx, "settings-footer");
            settings.update(cx, |view, cx| {
                view.error = None;
                cx.notify();
            });
        }
    }
    #[gpui::test]
    fn preference_recovery_keeps_the_launcher_frame_at_every_size(cx: &mut TestAppContext) {
        let (rt, _root, _backend, launcher, cx) = mounted::mount(cx);
        launcher.update(cx, |view, cx| {
            view.migration = Some((
                std::path::PathBuf::from("/synthetic/preferences.json"),
                crate::legacy_state::Source::WebKit("/synthetic/legacy".into()),
            ));
            view.m7_state_unavailable = true;
            view.error = Some("Synthetic preferences unavailable".into());
            cx.notify();
        });
        mounted::settle(&rt, cx);
        let handle = cx.update(|window, _| window.window_handle());
        for zoom in [1., 0.85] {
            cx.update(|window, cx| {
                window.set_rem_size(px(16. * zoom));
                launcher.update(cx, |view, cx| {
                    view.zoom = zoom;
                    cx.notify();
                });
            });
            for (width, height) in [(467., 458.), (900., 900.)] {
                cx.simulate_window_resize(handle, size(px(width), px(height)));
                cx.run_until_parked();
                let frame = cx.debug_bounds("preference-recovery-frame").unwrap();
                let retry = cx.debug_bounds("retry-legacy-preferences").unwrap();
                let viewport = cx.update(|window, _| window.viewport_size());
                assert_eq!(frame.origin, point(px(0.), px(0.)));
                assert_eq!(frame.size, viewport);
                assert!(
                    (retry.left() - px(24. * zoom + 1.)).abs() < px(0.1),
                    "Recovery padding sits inside exactly one frame border: {retry:?}"
                );
            }
        }
    }
    #[gpui::test]
    fn parent_mounted_onboarding_existing_footer_keeps_bottom_for_each_step_and_error(
        cx: &mut TestAppContext,
    ) {
        let (rt, _root, _backend, launcher, cx) = mounted::mount(cx);
        cx.update(|window, cx| {
            launcher.update(cx, |view, cx| view.mount_onboarding(window, cx).unwrap())
        });
        mounted::settle(&rt, cx);
        let onboarding = launcher.read_with(cx, |view, _| view.onboarding.clone().unwrap());
        let steps = onboarding.read_with(cx, |view, _| view.steps.len());
        for index in 0..steps {
            cx.update(|window, cx| onboarding.update(cx, |view, cx| view.go(index, window, cx)));
            mounted::settle(&rt, cx);
            resized_bottom(cx, "onboarding-footer");
            onboarding.update(cx, |view, cx| {
                view.error = Some("Synthetic onboarding error".into());
                cx.notify();
            });
            cx.run_until_parked();
            resized_bottom(cx, "onboarding-footer");
            onboarding.update(cx, |view, cx| {
                view.error = None;
                cx.notify();
            });
        }
    }
}
