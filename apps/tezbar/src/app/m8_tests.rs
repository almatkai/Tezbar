use super::*;
use ::core::prelude::v1::test;
use base64::Engine;
fn png() -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        1,
        1,
        image::Rgba([1, 2, 3, 255]),
    ))
    .write_to(&mut bytes, image::ImageFormat::Png)
    .unwrap();
    bytes.into_inner()
}
#[test]
fn actual_gpui_clipboard_payload_preserves_canonical_bytes_and_mime() {
    let bytes = png();
    let item = ClipboardItem::new_image(&gpui::Image::from_bytes(
        gpui::ImageFormat::Png,
        bytes.clone(),
    ));
    let images = clipboard_images(Some(item)).unwrap();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0]["mimeType"], "image/png");
    assert_eq!(images[0]["width"], 1);
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(images[0]["data"].as_str().unwrap())
            .unwrap(),
        bytes
    );
}
#[test]
fn clipboard_atomic_batch_rejects_empty_text_unsupported_and_five_images() {
    assert!(clipboard_images(None).is_err());
    assert!(clipboard_images(Some(ClipboardItem::new_string("private text".into()))).is_err());
    let mut item =
        ClipboardItem::new_image(&gpui::Image::from_bytes(gpui::ImageFormat::Png, png()));
    item.entries
        .extend((0..4).map(|_| {
            ClipboardEntry::Image(gpui::Image::from_bytes(gpui::ImageFormat::Png, png()))
        }));
    assert!(clipboard_images(Some(item)).is_err());
    let item = ClipboardItem::new_image(&gpui::Image::from_bytes(
        gpui::ImageFormat::Gif,
        b"GIF89a".to_vec(),
    ));
    assert!(clipboard_images(Some(item)).is_err());
}

use crate::surfaces::ai_chat::{Action as ChatAction, ChatRequest};
use crate::surfaces::command_bar::chat_integration::tests::{
    Backend, mount, publish, settle, submit,
};
use gpui::EntityInputHandler;
use std::sync::atomic::Ordering;
pub(in crate::app) fn owner(
    cx: &mut VisualTestContext,
    rt: &tokio::runtime::Runtime,
    root: &tempfile::TempDir,
    backend: &Arc<Backend>,
) -> Entity<Owner> {
    let core = backend.core.clone();
    let relay = backend.relay.clone();
    let launcher = cx.update(|w, _| w.window_handle().downcast::<Launcher>().unwrap());
    let preview = Preview {
        root: root.path().into(),
        resources: root.path().join("resources"),
        show: false,
        allow_desktop_integration: false,
        allow_desktop_services: false,
        launcher_shortcut: None,
        theme: "dark".into(),
        home: None,
        #[cfg(feature = "bench-control")]
        control_token: None,
    };
    let log = Box::leak(Box::new(
        tezbar_host::logging::FileLog::open(&root.path().join("logs"), "m8-fixture").unwrap(),
    ));
    let _enter = rt.enter();
    cx.cx.update(|cx| {
        cx.new(|cx| {
            let mut m7 = m7::State::new(launcher.read(cx).unwrap().settings_store.clone().unwrap());
            m7.fail_open();
            m7.channel_loaded = true;
            let mut lifecycle = windows::Lifecycle::default();
            lifecycle.visible = true;
            Owner {
                updater: None,
                updater_quiescing: false,
                updater_paused: false,
                production_plan: None,
                pending_update: None,
                #[cfg(target_os = "macos")]
                staged_update: None,
                installed_executable: None,
                layout_path: root.path().join("config/launcher-layouts-v1.json"),
                m7,
                settings_close: Default::default(),
                visibility_generation: 0,
                previous_foreground: None,
                panel: None,
                panel_visible: false,
                #[cfg(feature = "bench-control")]
                _control: None,
                #[cfg(feature = "bench-control")]
                shutdown_gate: Arc::default(),
                snap: Default::default(),
                overlay: None,
                writer: windows::persistence::Writer::new(
                    core.0.ctx.config.clone(),
                    root.path().join("config/launcher-layouts-v1.json"),
                ),
                core: core.clone(),
                host: Arc::new(GpuiHost::fixture(relay.clone())),
                instance: None,
                tray: None,
                launcher,
                settings: None,
                lifecycle,
                relay: relay.clone(),
                startup: None,
                show_on_ready: false,
                save_error: None,
                tasks: vec![],
                idle: None,
                blur: None,
                snap_timer: None,
                drag_task: None,
                settings_close_timer: None,
                positions: Default::default(),
                layouts: Default::default(),
                monitor: String::new(),
                config: core.0.ctx.config.read().unwrap(),
                preview,
                log,
                events: cx.global::<events::CoreEvents>().clone(),
            }
        })
    })
}
fn source(view: &Entity<Launcher>, cx: &mut VisualTestContext) -> ChatSource {
    view.read_with(cx, |v, cx| {
        let chat = v.chat_entity().unwrap();
        ChatSource {
            entity: chat.entity_id(),
            mount: chat.read(cx).host_generation(),
        }
    })
}
fn pop_effect(f: &Backend, rt: &tokio::runtime::Runtime) -> Event {
    let mut receiver = f.receiver.lock().unwrap();
    rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(1), async {
            for _ in 0..128 {
                let event = receiver.as_mut().unwrap().recv().await.unwrap();
                if matches!(event, Event::ChatEffect(..) | Event::DialogClosed) {
                    return event;
                }
            }
            panic!("host effect exceeded bounded fixture mailbox");
        })
        .await
        .unwrap()
    })
}
fn chat_action(view: &Entity<Launcher>, action: ChatAction, cx: &mut VisualTestContext) {
    let chat = view.read_with(cx, |v, _| v.chat_entity().unwrap());
    cx.update(|w, cx| chat.update(cx, |v, cx| v.action(action, w, cx)));
}
#[gpui::test]
fn actual_app_review_settings_routes_exact_source_cwd_nonce_and_retains_chat(
    cx: &mut TestAppContext,
) {
    let (rt, root, f, view, cx) = mount(cx);
    view.update(cx, |v, cx| v.receive_navigation(submit(1, "A"), cx));
    settle(&rt, cx);
    let a = source(&view, cx);
    let app = owner(cx, &rt, &root, &f);
    view.update(cx, |v, cx| v.receive_navigation(submit(2, "B"), cx));
    // Reject the already queued A effect before deferred activation of B.
    app.update(&mut cx.cx, |v, cx| {
        v.m8_effect(
            a,
            ChatEffect::ReviewProject {
                cwd: "/must-not-open".into(),
                nonce: "stale".into(),
            },
            cx,
        )
    });
    assert!(app.read_with(&cx.cx, |v, _| v.settings.is_none()));
    settle(&rt, cx);
    let b = source(&view, cx);
    let project = root
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    app.update(&mut cx.cx, |v, cx| {
        v.m8_effect(
            b,
            ChatEffect::ReviewProject {
                cwd: project.clone(),
                nonce: "review-b".into(),
            },
            cx,
        )
    });
    settle(&rt, cx);
    assert!(
        f.calls
            .lock()
            .unwrap()
            .iter()
            .any(|(channel, args)| *channel == "ai-extensions:review-project"
                && args == &vec![serde_json::json!(project)])
    );
    assert_eq!(f.count("ai-extensions:review-project"), 1);
    let settings = app.read_with(&cx.cx, |v, _| v.settings.unwrap());
    settings
        .read_with(&cx.cx, |v, cx| {
            assert_eq!(
                v.content.as_ref().unwrap().read(cx).tab,
                crate::surfaces::settings::Tab::Ai
            )
        })
        .unwrap();
    assert_eq!(
        view.read_with(cx, |v, _| v.chat_entity().unwrap().entity_id()),
        b.entity
    );
    assert!(!view.read_with(cx, |v, cx| v.accepts_chat_source(b, cx)));
    assert_eq!(f.count("agent:cancel"), 0);
    // A stale chooser still releases its reserved native dialog lease.
    f.relay
        .send(Event::ChatEffect(
            a,
            ChatEffect::ChooseImages { ticket: 999 },
        ))
        .unwrap();
    let event = pop_effect(&f, &rt);
    app.update(&mut cx.cx, |v, cx| v.event(event, cx));
    assert!(!cx.did_prompt_for_paths());
    assert!(matches!(pop_effect(&f, &rt), Event::DialogClosed));
}
#[gpui::test]
fn actual_app_dialog_late_choice_and_clipboard_ticket_never_attach_to_b(cx: &mut TestAppContext) {
    let (rt, root, f, view, cx) = mount(cx);
    let app = owner(cx, &rt, &root, &f);
    view.update(cx, |v, cx| v.receive_navigation(submit(1, "A"), cx));
    settle(&rt, cx);
    chat_action(&view, ChatAction::New, cx);
    settle(&rt, cx);
    chat_action(&view, ChatAction::ChooseImages, cx);
    settle(&rt, cx);
    let event = pop_effect(&f, &rt);
    assert!(matches!(
        event,
        Event::ChatEffect(_, ChatEffect::ChooseImages { .. })
    ));
    app.update(&mut cx.cx, |v, cx| v.event(event, cx));
    assert!(cx.did_prompt_for_paths());
    let path = root.path().join("chosen.png");
    std::fs::write(&path, png()).unwrap();
    view.update(cx, |v, cx| v.receive_navigation(submit(2, "B"), cx));
    settle(&rt, cx);
    cx.simulate_path_prompt_response(|options| {
        assert!(options.multiple);
        Some(vec![path])
    });
    settle(&rt, cx);
    assert!(matches!(pop_effect(&f, &rt), Event::DialogClosed));
    let chat = view.read_with(cx, |v, _| v.chat_entity().unwrap());
    chat.read_with(cx, |v, _| {
        assert!(
            v.controller
                .conversation(v.controller.active.as_deref().unwrap())
                .unwrap()
                .attachments
                .is_empty()
        )
    });
    // Positive chooser goes through the actual host prompt and core asset service.
    chat_action(&view, ChatAction::New, cx);
    settle(&rt, cx);
    chat_action(&view, ChatAction::ChooseImages, cx);
    settle(&rt, cx);
    let event = pop_effect(&f, &rt);
    app.update(&mut cx.cx, |v, cx| v.event(event, cx));
    let path = root.path().join("accepted.png");
    std::fs::write(&path, png()).unwrap();
    cx.simulate_path_prompt_response(|_| Some(vec![path]));
    settle(&rt, cx);
    assert!(matches!(pop_effect(&f, &rt), Event::DialogClosed));
    chat.read_with(cx, |v, _| {
        assert_eq!(
            v.controller
                .conversation(v.controller.active.as_deref().unwrap())
                .unwrap()
                .attachments
                .len(),
            1
        )
    });
    // Clipboard uses a fake future at the same ticket boundary; no OS clipboard access.
    chat_action(&view, ChatAction::ClipboardImage, cx);
    settle(&rt, cx);
    let Event::ChatEffect(stamp, ChatEffect::ClipboardImage { ticket }) = pop_effect(&f, &rt)
    else {
        panic!("missing clipboard effect")
    };
    let (send, receive) = tokio::sync::oneshot::channel();
    view.update(cx, |v, cx| {
        v.chat_clipboard_job(
            stamp,
            ticket,
            Box::pin(async move { receive.await.unwrap() }),
            cx,
        )
    });
    view.update(cx, |v, cx| v.receive_navigation(submit(3, "C"), cx));
    settle(&rt, cx);
    let _ = send.send(clipboard_images(Some(ClipboardItem::new_image(
        &gpui::Image::from_bytes(gpui::ImageFormat::Png, png()),
    ))));
    settle(&rt, cx);
    chat.read_with(cx, |v, _| {
        assert!(
            v.controller
                .conversation(v.controller.active.as_deref().unwrap())
                .unwrap()
                .attachments
                .is_empty()
        )
    });
}

#[gpui::test]
fn actual_app_quit_save_failure_cancels_shutdown_then_fresh_retry_is_durable(
    cx: &mut TestAppContext,
) {
    let (rt, root, f, view, cx) = mount(cx);
    let app = owner(cx, &rt, &root, &f);
    view.update(cx, |v, cx| {
        v.receive_navigation(submit(1, "Save on actual quit"), cx)
    });
    settle(&rt, cx);
    let run = f.run();
    publish(
        cx,
        serde_json::json!({"type":"message","runId":run["runId"],"delta":"Quit retained answer"}),
    );
    f.fail_append.store(true, Ordering::SeqCst);
    {
        let _enter = rt.enter();
        app.update(&mut cx.cx, |v, cx| v.quit(cx));
    }
    settle(&rt, cx);
    assert!(!app.read_with(&cx.cx, |v, _| v.lifecycle.quitting));
    assert!(app.read_with(&cx.cx, |v, _| v.save_error.is_some()));
    assert_eq!(f.count("agent:cancel"), 1);
    // Core still accepts canonical reads; the response has not been durably written.
    let stored = rt
        .block_on(
            f.core
                .request(crate::core_client::Request::Chat(ChatRequest::Get(
                    run["sessionId"].as_str().unwrap().into(),
                ))),
        )
        .unwrap();
    assert!(!stored.to_string().contains("Quit retained answer"));
    let id = view.read_with(cx, |v, _| v.chat_entity().unwrap().entity_id());
    f.fail_append.store(false, Ordering::SeqCst);
    {
        let _enter = rt.enter();
        app.update(&mut cx.cx, |v, cx| v.quit(cx));
    }
    settle(&rt, cx);
    assert_eq!(f.count("agent:cancel"), 1);
    assert_eq!(
        view.read_with(cx, |v, _| v.chat_entity().unwrap().entity_id()),
        id
    );
    assert!(app.read_with(&cx.cx, |v, _| v.save_error.is_none()));
    assert!(!view.read_with(cx, |v, _| v.ready));
    // Read canonical storage directly after successful shutdown, without starting core again.
    let files = std::fs::read_dir(root.path().join("data")).unwrap().count();
    assert!(files > 0);
}
#[gpui::test]
fn actual_launcher_input_actions_focus_and_ime_do_not_fall_through_search(cx: &mut TestAppContext) {
    let (rt, _root, f, view, cx) = mount(cx);
    let mut new = submit(1, "");
    if let Payload::Ai { prompt, .. } = &mut new.payload {
        *prompt = None;
    }
    view.update(cx, |v, cx| v.receive_navigation(new, cx));
    settle(&rt, cx);
    let chat = view.read_with(cx, |v, _| v.chat_entity().unwrap());
    let input = chat.read_with(cx, |v, _| v.input.clone());
    cx.update(|w, cx| {
        input.update(cx, |v, cx| {
            v.replace_and_mark_text_in_range(None, "に", None, w, cx)
        })
    });
    let press = |cx: &mut VisualTestContext, key: &'static str| {
        let keystroke = Keystroke::parse(key).unwrap();
        cx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        cx.simulate_event(KeyUpEvent { keystroke });
    };
    press(cx, "enter");
    settle(&rt, cx);
    assert_eq!(f.count("agent:run"), 0);
    assert!(input.read_with(cx, |v, _| v.marked_range.is_some()));
    cx.update(|w, cx| input.update(cx, |v, cx| v.replace_text_in_range(None, "日本", w, cx)));
    cx.run_until_parked();
    press(cx, "shift-enter");
    assert!(input.read_with(cx, |v, _| v.content.contains('\n')));
    press(cx, "enter");
    settle(&rt, cx);
    assert_eq!(f.count("agent:run"), 1);
    assert!(view.read_with(cx, |v, cx| v.input.read(cx).content.is_empty()));
    let run = f.run();
    publish(
        cx,
        serde_json::json!({"type":"approval","runId":run["runId"],"approvalId":"actual-focus","title":"Allow Fixture?","command":"{}","canRemember":false}),
    );
    cx.executor().advance_clock(Duration::from_millis(17));
    settle(&rt, cx);
    assert!(!cx.update(|w, cx| input.focus_handle(cx).is_focused(w)));
    press(cx, "enter");
    settle(&rt, cx);
    assert_eq!(f.count("agent:approve"), 1);
    press(
        cx,
        if cfg!(target_os = "macos") {
            "cmd-."
        } else {
            "ctrl-."
        },
    );
    settle(&rt, cx);
    assert_eq!(f.count("agent:cancel"), 1);
    let id = chat.entity_id();
    press(
        cx,
        if cfg!(target_os = "macos") {
            "cmd-n"
        } else {
            "ctrl-n"
        },
    );
    settle(&rt, cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.chat_entity().unwrap().entity_id()),
        id
    );
    assert_eq!(f.count("agent:run"), 1);
}

fn repair_failed_quit_continue(cx: &mut TestAppContext, failure: &str) {
    let (rt, root, f, view, cx) = mount(cx);
    let app = owner(cx, &rt, &root, &f);
    view.update(cx, |v, cx| {
        v.receive_navigation(submit(1, "Old session"), cx)
    });
    settle(&rt, cx);
    let old = f.run();
    publish(
        cx,
        serde_json::json!({"type":"message","runId":old["runId"],"delta":"Retain this answer"}),
    );
    publish(
        cx,
        serde_json::json!({"type":"approval","runId":old["runId"],"approvalId":"old-approval","title":"Allow Fixture?","command":"{}","canRemember":false}),
    );
    let _enter = rt.enter();
    app.update(&mut cx.cx, |o, _| {
        if failure == "append" {
            f.fail_append.store(true, Ordering::SeqCst);
        } else if failure == "settings" {
            let id = o.m7.barrier.begin("repair");
            o.m7.barrier.finish("repair", id, false);
        } else {
            let p = root.path().join("layout-is-directory");
            std::fs::create_dir(&p).unwrap();
            o.writer = windows::persistence::Writer::new(o.core.0.ctx.config.clone(), p);
            o.positions.dirty = true;
        }
    });
    {
        let _enter = rt.enter();
        app.update(&mut cx.cx, |o, cx| o.quit(cx));
    }
    for _ in 0..8 {
        settle(&rt, cx);
        if !app.read_with(&cx.cx, |o, _| o.lifecycle.quitting) {
            break;
        }
    }
    assert!(!app.read_with(&cx.cx, |o, _| o.lifecycle.quitting));
    assert!(app.read_with(&cx.cx, |o, _| o.save_error.is_some()));
    assert_eq!(f.count("agent:cancel"), usize::from(failure == "append"));
    let chat = view.read_with(cx, |v, _| v.chat_entity().unwrap());
    let retained = chat.entity_id();
    view.update(cx, |v, cx| {
        v.visible = true;
        v.receive_navigation(
            Navigation {
                nonce: 5,
                surface: Surface::AiChat,
                payload: Payload::Ai {
                    prompt: None,
                    cwd: None,
                    resume: Some(old["sessionId"].as_str().unwrap().into()),
                    capture: false,
                },
            },
            cx,
        );
    });
    settle(&rt, cx);
    if failure != "append" {
        chat_action(
            &view,
            ChatAction::Decide(tezbar_ui_model::chat::Decision::Once),
            cx,
        );
        settle(&rt, cx);
        assert_eq!(
            f.count("agent:approve"),
            1,
            "pending approval lost by unrelated save failure"
        );
    }
    f.fail_append.store(false, Ordering::SeqCst);
    app.update(&mut cx.cx, |o, _| {
        let id = o.m7.barrier.begin("repair");
        o.m7.barrier.finish("repair", id, true);
    });
    view.update(cx, |v, cx| {
        v.receive_navigation(submit(6, "New send after failed quit"), cx)
    });
    settle(&rt, cx);
    assert_eq!(f.count("agent:run"), 2);
    assert!(view.read_with(cx, |v, _| v.visible));
    let new = f.run();
    let body = serde_json::to_string_pretty(
        &serde_json::json!({"path":"fixture","options":{"dryRun":true}}),
    )
    .unwrap();
    publish(
        cx,
        serde_json::json!({"type":"approval","runId":new["runId"],"approvalId":"new-approval","title":"Allow Fixture to use write?","command":body,"canRemember":false}),
    );
    settle(&rt, cx);
    assert_eq!(
        chat.read_with(cx, |v, _| v.approval_body()),
        Some(body.clone())
    );
    assert!(body.contains("\n  ")); // indented JSON at rendered seam
    chat_action(
        &view,
        ChatAction::Decide(tezbar_ui_model::chat::Decision::Once),
        cx,
    );
    settle(&rt, cx);
    assert_eq!(
        f.count("agent:approve"),
        if failure == "append" { 1 } else { 2 }
    );
    {
        let _enter = rt.enter();
        app.update(&mut cx.cx, |o, cx| o.quit(cx));
    }
    settle(&rt, cx);
    assert!(app.read_with(&cx.cx, |o, _| o.save_error.is_none()));
    assert_eq!(chat.entity_id(), retained);
    let calls = f.calls.lock().unwrap();
    let mut cancelled = std::collections::BTreeSet::new();
    for (_, args) in calls.iter().filter(|(c, _)| *c == "agent:cancel") {
        assert!(cancelled.insert(args[0].to_string()), "duplicate Cancel");
    }
    assert_eq!(cancelled.len(), 2);
}
#[gpui::test]
fn repair_actual_app_append_failure_then_resume_send_approve_quit(cx: &mut TestAppContext) {
    repair_failed_quit_continue(cx, "append");
}
#[gpui::test]
fn repair_actual_app_settings_failure_preserves_live_approval(cx: &mut TestAppContext) {
    repair_failed_quit_continue(cx, "settings");
}
#[gpui::test]
fn repair_actual_app_layout_failure_preserves_live_approval(cx: &mut TestAppContext) {
    repair_failed_quit_continue(cx, "layout");
}

// M7 regressions use the actual Owner and Settings window, with the same isolated transport.
#[gpui::test]
fn review_m7_queued_zoom_is_applied_before_settings_refresh(cx: &mut TestAppContext) {
    let (rt, root, backend, launcher, cx) = mount(cx);
    let owner = owner(cx, &rt, &root, &backend);
    owner.update(cx, |o, cx| o.settings_intent(None, cx));
    settle(&rt, cx);
    let content = owner.read_with(cx, |o, cx| {
        o.settings
            .unwrap()
            .read(cx)
            .unwrap()
            .content
            .clone()
            .unwrap()
    });
    launcher.update(cx, |v, _| v.zoom = 1.05);
    owner.update(cx, |o, cx| {
        let ticket = o.m7.barrier.begin("layout");
        o.m7.queued_layout = Some((
            content.entity_id(),
            crate::surfaces::settings::SettingsEffect::LauncherLayout {
                width: None,
                height: None,
                zoom: Some(1.10),
                reset: false,
            },
            ticket,
        ));
        o.m7.layout_pending = false;
        o.m7_continue_layout(cx);
    });
    // TestPlatform refuses physical native placement after updating the zoom. This checks
    // real composition ordering and the third-click input, not OS geometry or durable layout.
    let zoom = content.read_with(cx, |v, _| {
        crate::surfaces::settings::tests::presentation_zoom(v)
    });
    assert!((zoom - 1.10).abs() < 0.0001);
    assert!((zoom + 0.05 - 1.15).abs() < 0.0001);
    assert!(owner.read_with(cx, |o, _| o.m7.queued_layout.is_none()));
    rt.block_on(backend.core.0.shutdown());
}
#[gpui::test]
fn review_m7_failed_core_start_delivers_pending_settings_once(cx: &mut TestAppContext) {
    let (rt, root, backend, _launcher, cx) = mount(cx);
    let owner = owner(cx, &rt, &root, &backend);
    owner.update(cx, |o, cx| {
        o.m7.boot = m7::Boot::Waiting;
        o.m7.decided = false;
        o.settings_intent(None, cx);
        assert!(o.m7.pending_settings.is_some());
        assert!(o.settings.is_none());
        o.m7.fail_open();
        o.m7_present_ready(cx);
        assert!(o.m7.pending_settings.is_none());
        assert!(o.settings.is_some());
    });
    settle(&rt, cx);
    let settings = owner.read_with(cx, |o, _| o.settings.unwrap());
    owner.update(cx, |o, cx| o.m7_ready(cx));
    assert_eq!(owner.read_with(cx, |o, _| o.settings.unwrap()), settings);
    rt.block_on(backend.core.0.shutdown());
}
