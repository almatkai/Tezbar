//! Nine actual SDK → Bun → NativeCore → GPUI scenario runners.
//! Registered scenarios use explicit private production worker launch and TestHost.
use super::*;
use ::core::prelude::v1::test;
#[path = "../../../../../tests/fixtures/extensions/native_harness.rs"]
mod native_harness;
use native_harness::Fixture;
fn settle(rt: &tokio::runtime::Runtime, cx: &mut VisualTestContext, view: &Entity<RuntimeView>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        rt.block_on(async {
            tokio::task::yield_now().await;
        });
        cx.run_until_parked();
        if !view.read(&cx.app.borrow()).job.is_some() {
            break;
        }
        assert!(Instant::now() < deadline, "Real SDK operation timed out");
        rt.block_on(async { tokio::time::sleep(Duration::from_millis(5)).await });
    }
}
fn action(
    view: &Entity<RuntimeView>,
    title: &str,
    rt: &tokio::runtime::Runtime,
    cx: &mut VisualTestContext,
) {
    // Open and filter the actual rendered ActionPanel. Enter dispatches the same
    // production control used by keyboard users, never a mirrored node harness.
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.event(UiEvent::OpenActions, w, cx);
            v.action_search.update(cx, |f, cx| f.set_text(title, w, cx));
            v.action_search.focus_handle(cx).focus(w, cx);
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    settle(rt, cx, view);
    cx.update(|w, cx| view.update(cx, |v, cx| v.event(UiEvent::CloseActions, w, cx)));
}
fn node_count(root: &Node, kind: NodeKind) -> usize {
    let mut nodes = vec![];
    root.visit(&mut nodes);
    let direct = nodes.iter().filter(|n| n.kind == kind).count();
    direct
        + nodes
            .iter()
            .filter_map(|n| n.props.get("detail"))
            .filter_map(|v| serde_json::from_value::<Node>(v.clone()).ok())
            .map(|n| node_count(&n, kind))
            .sum::<usize>()
}
fn text(view: &Entity<RuntimeView>, cx: &VisualTestContext) -> String {
    view.read(&cx.app.borrow())
        .session
        .as_ref()
        .unwrap()
        .root
        .string("markdown")
        .unwrap_or("")
        .to_owned()
}
/// Runners consume the real response obtained via Fixture::transport.run. Factory
/// builds the same initialized core, worker and native controls used in production.
pub fn verify(
    case: &str,
    fixture: Fixture,
    assets: Arc<dyn ExtensionAssets>,
    cx: &mut TestAppContext,
) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| {
        gpui_tokio::init_from_handle(cx, rt.handle().clone());
        tezbar_ui::text_input::bind_keys(cx);
        tezbar_ui::markdown::bind_keys(cx);
    });
    let reply = rt
        .block_on(fixture.transport.run(case))
        .expect("Real fixture run failed");
    let (view, cx) =
        cx.add_window_view(|w, cx| RuntimeView::new(fixture.transport.clone(), assets, w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate(reply, 1, w, cx).unwrap()));
    cx.run_until_parked();
    match case {
        "list" => {
            let root = view.read_with(cx, |v, _| v.session.as_ref().unwrap().root.clone());
            assert_eq!(node_count(&root, NodeKind::ListItem), 30);
            assert!(node_count(&root, NodeKind::ListSection) > 0);
            assert!(node_count(&root, NodeKind::ItemMetadata) > 0);
            cx.simulate_keystrokes("down");
            let selected = view
                .read(&cx.app.borrow())
                .session
                .as_ref()
                .unwrap()
                .selection
                .clone();
            action(&view, "Toggle Loading", &rt, cx);
            assert_eq!(
                view.read(&cx.app.borrow())
                    .session
                    .as_ref()
                    .unwrap()
                    .selection,
                selected
            );
            assert_eq!(
                view.read(&cx.app.borrow())
                    .session
                    .as_ref()
                    .unwrap()
                    .root
                    .props["isLoading"],
                true
            );
            action(&view, "Grow to 1000", &rt, cx);
            for _ in 0..40 {
                if view.read_with(cx, |v, _| v.rows.len()) == 1000 {
                    break;
                }
                cx.update(|w, cx| {
                    view.update(cx, |v, cx| {
                        let last = v.rows.last().unwrap().id.clone();
                        v.event(UiEvent::Select(last), w, cx);
                    })
                });
                settle(&rt, cx, &view);
            }
            assert_eq!(view.read_with(cx, |v, _| v.rows.len()), 1000);
        }
        "grid" => {
            let root = view.read_with(cx, |v, _| v.session.as_ref().unwrap().root.clone());
            assert_eq!(root.kind, NodeKind::Grid);
            assert_eq!(root.props["columns"], 3);
            assert_eq!(node_count(&root, NodeKind::GridItem), 7);
            // Grid arrows navigate tiles only when the search input is not focused.
            // This scenario tests the native grid owner, while the caret regression
            // separately exercises the search field's real dispatch path.
            cx.update(|w, cx| {
                let focus = view.read(cx).focus.clone();
                focus.focus(w, cx);
            });
            cx.simulate_keystrokes("right down");
            assert_eq!(
                view.read(&cx.app.borrow())
                    .session
                    .as_ref()
                    .unwrap()
                    .selection
                    .as_deref(),
                Some("colour-4")
            );
            let deadline = Instant::now() + Duration::from_secs(10);
            while view.read_with(cx, |v, _| v.image_probe().1) == 0 {
                rt.block_on(async {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                });
                cx.run_until_parked();
                assert!(
                    Instant::now() < deadline,
                    "Grid artwork must decode from installed canonical assets"
                );
            }
            assert!(view.read_with(cx, |v, _| v.image_dimensions().contains(&(1, 1))));
            action(&view, "Empty Grid", &rt, cx);
            assert!(view.read(&cx.app.borrow()).rows.is_empty());
        }
        "detail" => {
            assert!(text(&view, cx).contains("| SDK | Real |"));
            let deadline = Instant::now() + Duration::from_secs(10);
            while view.read_with(cx, |v, _| v.image_probe().1) == 0 {
                rt.block_on(async {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                });
                cx.run_until_parked();
                assert!(Instant::now() < deadline, "Actual SDK image did not decode");
            }
            let app = cx.app.borrow();
            let v = view.read(&app);
            assert!(!v.documents.is_empty());
            assert_eq!(v.image_dimensions(), vec![(1, 1)]);
            let doc = v.documents.values().next().unwrap().view.read(&app);
            assert!(doc.document.blocks.iter().any(|b| !b.images.is_empty()));
            assert!(node_count(&v.session.as_ref().unwrap().root, NodeKind::MetadataLink) > 0);
        }
        "form" => {
            assert_eq!(
                view.read(&cx.app.borrow())
                    .session
                    .as_ref()
                    .unwrap()
                    .root
                    .kind,
                NodeKind::PreferenceSetup
            );
            cx.update(|w, cx| {
                view.update(cx, |v, cx| {
                    for (name, value) in [("name", "Fixture"), ("password", "synthetic-password")] {
                        let key = v
                            .preference
                            .model
                            .fields
                            .iter()
                            .find(|f| f.name == name)
                            .unwrap()
                            .key();
                        v.edit_preference(key, json!(value), cx);
                    }
                    v.save_setup(cx);
                    let _ = w;
                })
            });
            let deadline = Instant::now() + Duration::from_secs(10);
            while view.read(&cx.app.borrow()).preference.saving {
                rt.block_on(async {
                    tokio::task::yield_now().await;
                });
                cx.run_until_parked();
                assert!(Instant::now() < deadline);
            }
            let app = cx.app.borrow();
            let v = view.read(&app);
            let root = &v.session.as_ref().unwrap().root;
            for kind in [
                NodeKind::TextField,
                NodeKind::TextArea,
                NodeKind::PasswordField,
                NodeKind::Checkbox,
                NodeKind::DatePicker,
                NodeKind::Dropdown,
                NodeKind::TagPicker,
                NodeKind::FilePicker,
                NodeKind::Description,
                NodeKind::FormSeparator,
            ] {
                assert!(node_count(&root, kind) > 0);
            }
            assert!(v.fields["password"].input.read(&app).options.password);
            let values = v.session.as_ref().unwrap().form.values();
            assert_eq!(values["area"], "Line one\nLine two");
            assert_eq!(values["checked"], true);
            assert_eq!(values["date"], "2026-11-07");
            assert_eq!(values["dropdown"], "kz");
            assert!(root.children.iter().any(|n| n.kind == NodeKind::Description
                && n.string("text").is_some_and(|s| s.contains("kept"))));
            drop(app);
            action(&view, "Submit Fixture", &rt, cx);
            assert!(view.read_with(cx, |v, _| {
                v.session
                    .as_ref()
                    .unwrap()
                    .form
                    .fields()
                    .any(|f| f.error.is_some())
            }));
            cx.update(|w, cx| {
                view.update(cx, |v, cx| {
                    v.event(
                        UiEvent::ChangeField {
                            id: "text".into(),
                            value: json!("actual input"),
                        },
                        w,
                        cx,
                    );
                    v.event(
                        UiEvent::ChangeField {
                            id: "password".into(),
                            value: json!("synthetic"),
                        },
                        w,
                        cx,
                    );
                })
            });
            action(&view, "Submit Fixture", &rt, cx);
            assert!(
                fixture
                    .transport
                    .last("extension:invoke-action")
                    .is_some_and(|r| r["formValues"]["text"] == "actual input"
                        && r["formValues"]["password"] == "synthetic")
            );
        }
        "actions" => {
            action(&view, "Copy", &rt, cx);
            assert_eq!(fixture.desktop.count("clipboard:write"), 1);
            assert_eq!(fixture.desktop.copied_lengths(), vec![10010]);
            for name in ["Paste", "Open", "Reveal"] {
                action(&view, name, &rt, cx);
            }
            let counts = [
                fixture.desktop.count("clipboard:write"),
                fixture.desktop.count("shell:open"),
                fixture.desktop.count("core:platform:reveal-file"),
            ];
            // Sixty real SDK actions roll the worker's 50-record diagnostics window.
            for _ in 0..60 {
                action(&view, "Effect Wave", &rt, cx);
            }
            assert_eq!(fixture.desktop.count("clipboard:write"), counts[0] + 60);
            assert!(
                fixture
                    .desktop
                    .copied_lengths()
                    .iter()
                    .all(|n| *n == 10010 || *n == "Fixture paste Қаза🐎".len())
            );
            assert_eq!(fixture.transport.diagnostics_count(), 50);
            assert!(fixture.host.clipboard_text.lock().unwrap().len() == 10010);
            let before = fixture.desktop.count("clipboard:write");
            for _ in 0..3 {
                cx.update(|w,cx|view.update(cx,|v,cx|{let session=v.session.as_ref().unwrap();let revision=1000+fixture.transport.count("extension:refresh-session")as u64;v.core_event(&CoreEvent::Other{channel:"extension:session-invalidated".into(),payload:json!({"sessionId":session.key.session_id,"generation":session.generation,"revision":revision})},w,cx);}));
                settle(&rt, cx, &view);
            }
            assert_eq!(fixture.desktop.count("clipboard:write"), before);
            action(&view, "Push", &rt, cx);
            assert_eq!(text(&view, cx), "Pushed child");
            cx.simulate_keystrokes("escape");
            settle(&rt, cx, &view);
            assert!(text(&view, cx).starts_with("Actions:"));
        }
        "prompts" => {
            fixture
                .host
                .dialogs
                .lock()
                .unwrap()
                .push_back(Ok(tezbar_core::host::DialogReply::Confirm(true)));
            action(&view, "Confirm", &rt, cx);
            assert_eq!(text(&view, cx), "Answer: true");
            assert!(
                fixture
                    .host
                    .calls
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|c| c.starts_with("dialog:"))
            );
        }
        "feedback" => {
            action(&view, "Toast", &rt, cx);
            assert!(
                view.read(&cx.app.borrow())
                    .session
                    .as_ref()
                    .unwrap()
                    .notice_message
                    .is_some()
            );
            action(&view, "HUD", &rt, cx);
            assert_eq!(
                view.read(&cx.app.borrow())
                    .session
                    .as_ref()
                    .unwrap()
                    .notice_message
                    .as_deref(),
                Some("Fixture HUD")
            );
            action(&view, "HUD and Close", &rt, cx);
            assert!(
                fixture
                    .host
                    .events
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|(channel, payload)| channel == "extension:ui"
                        && payload["kind"] == "close"
                        && payload["sessionId"].as_str().is_some())
            );
        }
        "menubar" => {
            assert_eq!(
                view.read(&cx.app.borrow())
                    .session
                    .as_ref()
                    .unwrap()
                    .root
                    .kind,
                NodeKind::MenuBarExtra
            );
            assert_eq!(fixture.host.status.lock().unwrap().len(), 1);
            {
                let specs = fixture.host.status_specs.lock().unwrap();
                let spec = specs.values().next().unwrap();
                for kind in ["section", "submenu", "separator"] {
                    assert!(spec.items.iter().any(|i| i["kind"] == kind));
                }
            }
            cx.update(|w, cx| view.update(cx, |v, cx| v.set_visible(false, w, cx)));
            let callback = fixture
                .host
                .status
                .lock()
                .unwrap()
                .values()
                .next()
                .unwrap()
                .clone();
            let action_id = view.read_with(cx, |v, _| {
                v.session
                    .as_ref()
                    .unwrap()
                    .actions
                    .iter()
                    .find(|a| a.title == "Count 0")
                    .unwrap()
                    .id
                    .clone()
            });
            callback(action_id);
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let updated = fixture
                    .host
                    .calls
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|c| c.starts_with("status:set:"))
                    .count();
                if updated >= 2 {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "Hidden tray callback did not update"
                );
                rt.block_on(async {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                });
            }
            cx.update(|w,cx|view.update(cx,|v,cx|{v.set_visible(true,w,cx);let s=v.session.as_ref().unwrap();v.core_event(&CoreEvent::Other{channel:"extension:session-invalidated".into(),payload:json!({"sessionId":s.key.session_id,"generation":s.generation,"revision":100})},w,cx);}));
            settle(&rt, cx, &view);
            assert_eq!(
                view.read(&cx.app.borrow())
                    .session
                    .as_ref()
                    .unwrap()
                    .root
                    .string("title"),
                Some("Fixture 1")
            );
            action(&view, "Later", &rt, cx);
            cx.update(|w, cx| view.update(cx, |v, cx| v.set_visible(false, w, cx)));
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if fixture
                    .host
                    .status_specs
                    .lock()
                    .unwrap()
                    .values()
                    .any(|s| s.title == "Fixture 2")
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "Hidden MenuBarExtra invalidation did not update status"
                );
                rt.block_on(async {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                });
                cx.run_until_parked();
            }
            cx.update(|w,cx|view.update(cx,|v,cx|{v.set_visible(true,w,cx);let s=v.session.as_ref().unwrap();v.core_event(&CoreEvent::Other{channel:"extension:session-invalidated".into(),payload:json!({"sessionId":s.key.session_id,"generation":s.generation,"revision":200})},w,cx);}));
            settle(&rt, cx, &view);
            assert_eq!(
                view.read_with(cx, |v, _| v
                    .session
                    .as_ref()
                    .unwrap()
                    .root
                    .string("title")
                    .map(str::to_owned)),
                Some("Fixture 2".into())
            );
            // A background menu may temporarily render null. Retiring its GPUI
            // screen and opening another command must not stop its worker timer.
            action(&view, "Hide briefly", &rt, cx);
            assert!(fixture.host.status.lock().unwrap().is_empty());
            let ordinary = rt.block_on(fixture.transport.run("list")).unwrap();
            cx.update(|w, cx| view.update(cx, |v, cx| v.activate(ordinary, 2, w, cx).unwrap()));
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if fixture
                    .host
                    .status_specs
                    .lock()
                    .unwrap()
                    .values()
                    .any(|s| s.title == "Fixture 2")
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "Background menu did not return after its view was replaced"
                );
                rt.block_on(async { tokio::time::sleep(Duration::from_millis(5)).await });
                cx.run_until_parked();
            }
            assert_eq!(
                view.read_with(cx, |v, _| v.session.as_ref().unwrap().root.kind),
                NodeKind::List
            );
            let remove = fixture
                .host
                .status_specs
                .lock()
                .unwrap()
                .values()
                .next()
                .unwrap()
                .items
                .iter()
                .find(|item| item["title"] == "Remove")
                .unwrap()["id"]
                .as_str()
                .unwrap()
                .to_owned();
            let callback = fixture
                .host
                .status
                .lock()
                .unwrap()
                .values()
                .next()
                .unwrap()
                .clone();
            callback(remove);
            let deadline = Instant::now() + Duration::from_secs(10);
            while !fixture.host.status.lock().unwrap().is_empty() {
                assert!(
                    Instant::now() < deadline,
                    "Background Remove callback did not clear status"
                );
                rt.block_on(async { tokio::time::sleep(Duration::from_millis(5)).await });
            }
            for _ in 0..130 {
                let _ = rt.block_on(fixture.transport.run("menubar")).unwrap();
                assert_eq!(fixture.host.status.lock().unwrap().len(), 1);
            }
            let callback = fixture
                .host
                .status
                .lock()
                .unwrap()
                .values()
                .next()
                .unwrap()
                .clone();
            rt.block_on(fixture.transport.core.request(
                "toggle-command-enabled",
                &[
                    json!("extcmd:raycast.m09-native-fixtures:menubar"),
                    json!(false),
                ],
            ))
            .unwrap();
            assert!(fixture.host.status.lock().unwrap().is_empty());
            callback("ext-action-0-stale".into());
            rt.block_on(fixture.transport.run("menubar")).unwrap();
            assert_eq!(fixture.host.status.lock().unwrap().len(), 1);
            rt.block_on(fixture.transport.core.request(
                "extension:uninstall",
                &[json!("raycast.m09-native-fixtures")],
            ))
            .unwrap();
            assert!(fixture.host.status.lock().unwrap().is_empty());
        }
        "invalidation" => {
            let deadline = Instant::now() + Duration::from_secs(3);
            while !fixture
                .host
                .events
                .lock()
                .unwrap()
                .iter()
                .any(|(c, _)| c == "extension:session-invalidated")
            {
                rt.block_on(async { tokio::time::sleep(Duration::from_millis(5)).await });
                assert!(Instant::now() < deadline);
            }
            let events = fixture
                .host
                .events
                .lock()
                .unwrap()
                .iter()
                .filter(|(c, _)| c == "extension:session-invalidated")
                .cloned()
                .collect::<Vec<_>>();
            let before = fixture.transport.count("extension:refresh-session");
            for (channel, payload) in events {
                cx.update(|w, cx| {
                    view.update(cx, |v, cx| {
                        let e = CoreEvent::Other { channel, payload };
                        v.core_event(&e, w, cx);
                        v.core_event(&e, w, cx);
                    })
                });
            }
            settle(&rt, cx, &view);
            assert_eq!(
                fixture.transport.count("extension:refresh-session"),
                before + 1
            );
            assert_eq!(text(&view, cx), "Revision 1");
        }
        _ => panic!("Unknown fixture"),
    }
    cx.update(|_, cx| view.update(cx, |v, cx| v.dispose(cx)));
    cx.run_until_parked();
    rt.block_on(fixture.transport.drain()).unwrap();
    rt.block_on(fixture.transport.shutdown());
}

fn run_case(case: &str, cx: &mut TestAppContext) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let prepared = std::env::var_os("TEZBAR_M09_PREPARED")
        .expect("Run tests/fixtures/extensions/prepare.mjs and set TEZBAR_M09_PREPARED");
    let bun = std::env::var_os("TEZBAR_M09_BUN").expect("Set TEZBAR_M09_BUN to provisioned Bun");
    let fixture = rt
        .block_on(Fixture::isolated(
            std::path::Path::new(&prepared),
            bun.into(),
            rt.handle().clone(),
        ))
        .unwrap();
    let _guard = rt.enter();
    let assets = Arc::new(crate::core_client::CoreClient::new(
        fixture.transport.core.clone(),
    ));
    verify(case, fixture, assets, cx);
}
macro_rules! sdk_case {
    ($name:ident, $case:literal) => {
        #[gpui::test]
        fn $name(cx: &mut TestAppContext) {
            run_case($case, cx);
        }
    };
}
sdk_case!(sdk_list, "list");
sdk_case!(sdk_grid, "grid");
sdk_case!(sdk_detail, "detail");
sdk_case!(sdk_form, "form");
sdk_case!(sdk_actions, "actions");
sdk_case!(sdk_prompts, "prompts");
sdk_case!(sdk_feedback, "feedback");
sdk_case!(sdk_menubar, "menubar");
sdk_case!(sdk_invalidation, "invalidation");
