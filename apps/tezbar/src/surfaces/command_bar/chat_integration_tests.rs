//! Real Launcher, ChatView, SettingsStore, NativeCore storage and persistent CoreEvents.
//! Only agent execution is scripted; there is no mirrored navigation/controller harness.
use super::*;
use crate::surfaces::settings::{SettingsStore, tests::runtime};
use ::core::prelude::v1::test;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(crate) fn settle(rt: &tokio::runtime::Runtime, cx: &mut VisualTestContext) {
    rt.block_on(async {
        let until = std::time::Instant::now() + Duration::from_millis(120);
        while std::time::Instant::now() < until {
            cx.run_until_parked();
            tokio::task::yield_now().await;
        }
    });
}
pub(crate) struct Backend {
    pub relay: Relay,
    pub receiver: Mutex<Option<crate::events::Receiver>>,
    pub core: CoreClient,
    pub calls: Mutex<Vec<(&'static str, Vec<Value>)>>,
    pub fail_append: AtomicBool,
    hold_aux: AtomicBool,
    aux_dropped: Arc<AtomicBool>,
}
impl RequestAdapter for Backend {
    fn send(&self, request: Request) -> ChatFuture {
        let (channel, args) = request.clone().wire();
        self.calls.lock().unwrap().push((channel, args.clone()));
        if channel == "chat:list" && self.hold_aux.load(Ordering::SeqCst) {
            struct DropFlag(Arc<AtomicBool>);
            impl Drop for DropFlag {
                fn drop(&mut self) {
                    self.0.store(true, Ordering::SeqCst);
                }
            }
            let flag = DropFlag(self.aux_dropped.clone());
            return Box::pin(async move {
                let _flag = flag;
                std::future::pending().await
            });
        }
        if matches!(
            channel,
            "ai-extensions:list" | "ai-extensions:composer-items"
        ) {
            return Box::pin(async { Ok(json!([])) });
        }
        if matches!(
            channel,
            "chat:generate-meta" | "agent:cancel" | "agent:approve"
        ) {
            return Box::pin(async { Ok(json!({"ok":true})) });
        }
        // The fixture NEVER forwards Settings auxiliary channels to OS/provider facilities.
        if matches!(&request, Request::Settings(_))
            && !matches!(channel, "llm-config-get" | "llm-config-set")
        {
            return Box::pin(async { Ok(Value::Null) });
        }
        if channel == "agent:capture-active-screen" {
            return Box::pin(async {
                use base64::Engine;
                let mut data = std::io::Cursor::new(Vec::new());
                image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
                    1,
                    1,
                    image::Rgba([1, 2, 3, 255]),
                ))
                .write_to(&mut data, image::ImageFormat::Png)
                .unwrap();
                Ok(
                    json!({"mimeType":"image/png","data":base64::engine::general_purpose::STANDARD.encode(data.into_inner()),"width":1,"height":1}),
                )
            });
        }
        if channel == "agent:run" {
            return Box::pin(async move { Ok(json!({"ok":true,"runId":args[0]["runId"]})) });
        }
        if channel == "chat:append" && self.fail_append.load(Ordering::SeqCst) {
            return Box::pin(async { Err("Fixture persistence failure".into()) });
        }
        let core = self.core.clone();
        Box::pin(async move { core.request(request).await })
    }
}
impl crate::surfaces::settings::SettingsTransport for Backend {
    fn send(
        &self,
        request: crate::surfaces::settings::SettingsRequest,
    ) -> crate::surfaces::settings::SettingsFuture {
        RequestAdapter::send(self, Request::Settings(request))
    }
    fn durable_config(&self) -> crate::surfaces::settings::SettingsFuture {
        let config = self.core.0.ctx.config.clone();
        Box::pin(async move { config.read() })
    }
}
impl Backend {
    pub fn count(&self, channel: &str) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| *c == channel)
            .count()
    }
    pub fn run(&self) -> Value {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(c, _)| *c == "agent:run")
            .unwrap()
            .1[0]
            .clone()
    }
}
pub(crate) fn mount(
    cx: &mut TestAppContext,
) -> (
    tokio::runtime::Runtime,
    tempfile::TempDir,
    Arc<Backend>,
    Entity<Launcher>,
    &mut VisualTestContext,
) {
    let rt = runtime(cx);
    let root = tempfile::tempdir().unwrap();
    let host = Arc::new(tezbar_core::host::TestHost::default());
    let paths = tezbar_core::common::CorePaths {
        config_dir: root.path().join("config"),
        data_dir: root.path().join("data"),
        home_dir: root.path().join("home"),
        temp_dir: root.path().join("tmp"),
        resource_dir: root.path().join("resources"),
    };
    let context = tezbar_core::common::CoreContext::new(paths, Some(host));
    context.config.patch(&json!({"provider":"ollama","model":"fixture","providerModels":{"ollama":[{"id":"fixture"}]},"agentApprovalMode":"review"})).unwrap();
    let core = tezbar_core::NativeCore::new(context).unwrap();
    rt.block_on(core.start_with_options(tezbar_core::StartupOptions::ISOLATED_PREVIEW))
        .unwrap();
    let client = CoreClient::new(core);
    let (relay, receiver) = Relay::new();
    let backend = Arc::new(Backend {
        relay: relay.clone(),
        receiver: Mutex::new(Some(receiver)),
        core: client,
        calls: Mutex::new(vec![]),
        fail_append: AtomicBool::new(false),
        hold_aux: AtomicBool::new(false),
        aux_dropped: Arc::new(AtomicBool::new(false)),
    });
    cx.update(|cx| cx.set_global(CoreEvents::new(relay.clone())));
    let store = SettingsStore::new(backend.clone());
    let injected = backend.clone();
    let (view, cx) = cx.add_window_view(|w, cx| {
        Launcher::with_adapter_and_settings(injected, relay, None, Some(store), w, cx)
    });
    view.update(cx, |v, _| {
        v.chat.assets = Some(backend.core.clone());
        v.ready = true;
        v.visible = true;
    });
    (rt, root, backend, view, cx)
}
pub(crate) fn submit(nonce: u64, text: &str) -> Navigation {
    Navigation {
        nonce,
        surface: Surface::AiChat,
        payload: Payload::Ai {
            prompt: Some(text.into()),
            cwd: None,
            resume: None,
            capture: false,
        },
    }
}
pub(crate) fn publish(cx: &mut VisualTestContext, payload: Value) {
    cx.cx.update(|cx| {
        cx.global::<CoreEvents>().clone().publish(
            CoreEvent::Other {
                channel: "agent:event".into(),
                payload,
            },
            cx,
        )
    });
}

#[test]
fn boot_payloads_keep_exact_identity_and_every_fresh_nonce() {
    let mut n = submit(3, "  exact prompt\n");
    if let Payload::Ai { cwd, .. } = &mut n.payload {
        *cwd = Some("/fixture/project".into());
    }
    assert_eq!(
        boot(&n, 7).unwrap(),
        Boot::Submit {
            session_id: "native-chat-7-3".into(),
            prompt: "  exact prompt\n".into(),
            cwd: Some("/fixture/project".into())
        }
    );
    n.nonce = 4;
    assert_ne!(
        boot(&n, 7).unwrap(),
        boot(&submit(3, "  exact prompt\n"), 7).unwrap()
    );
    n.payload = Payload::Ai {
        prompt: None,
        cwd: None,
        resume: Some("stored".into()),
        capture: false,
    };
    assert_eq!(
        boot(&n, 7).unwrap(),
        Boot::Resume {
            session_id: "stored".into()
        }
    );
    n.payload = Payload::Ai {
        prompt: None,
        cwd: None,
        resume: None,
        capture: true,
    };
    assert_eq!(
        boot(&n, 7).unwrap(),
        Boot::Screen {
            session_id: "native-chat-7-4".into()
        }
    );
}

#[gpui::test]
fn actual_launcher_prompt_mounts_once_acknowledges_and_retains_across_return(
    cx: &mut TestAppContext,
) {
    let (rt, _root, f, view, cx) = mount(cx);
    assert_eq!(f.count("agent:run"), 0);
    assert!(view.read_with(cx, |v, _| v.chat.view.is_none()));
    let mut intent = submit(1, "Parent composition prompt");
    let project = _root
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    if let Payload::Ai { cwd, .. } = &mut intent.payload {
        *cwd = Some(project.clone());
    }
    cx.update(|_, cx| {
        cx.set_global(navigation::NavigationInbox::default());
        cx.global_mut::<navigation::NavigationInbox>()
            .put(intent.clone())
            .unwrap();
    });
    view.update(cx, |v, cx| v.receive_navigation(intent.clone(), cx));
    settle(&rt, cx);
    assert_eq!(
        f.count("agent:run"),
        1,
        "calls: {:?}, error: {:?}",
        f.calls.lock().unwrap(),
        view.read_with(cx, |v, cx| v.chat.view.as_ref().and_then(|v| v
            .read(cx)
            .controller
            .active
            .as_deref()
            .and_then(|id| v.read(cx).controller.conversation(id))
            .and_then(|c| c.input_error.clone())))
    );
    let chat = view.read_with(cx, |v, _| v.chat.view.clone().unwrap());
    assert_eq!(
        chat.read_with(cx, |v, _| v
            .controller
            .conversation(v.controller.active.as_deref().unwrap())
            .unwrap()
            .selection
            .cwd
            .clone()),
        Some(project)
    );
    cx.update(|_, cx| {
        assert!(
            cx.global::<navigation::NavigationInbox>()
                .pending()
                .is_none()
        )
    });
    view.update(cx, |v, cx| v.receive_navigation(intent, cx));
    settle(&rt, cx);
    assert_eq!(f.count("agent:run"), 1);
    cx.update(|w, cx| view.update(cx, |v, cx| v.return_home(w, cx)));
    assert!(!chat.read_with(cx, |v, _| v.controller.visible));
    view.update(cx, |v, cx| {
        v.receive_navigation(submit(2, "Second invocation"), cx)
    });
    settle(&rt, cx);
    assert_eq!(f.count("agent:run"), 2);
    assert_eq!(
        view.read_with(cx, |v, _| v.chat.view.as_ref().unwrap().entity_id()),
        chat.entity_id()
    );
}

#[gpui::test]
fn persistent_events_reach_hidden_run_and_approval_without_switching_active_owner(
    cx: &mut TestAppContext,
) {
    let (rt, _root, f, view, cx) = mount(cx);
    view.update(cx, |v, cx| v.receive_navigation(submit(1, "A"), cx));
    settle(&rt, cx);
    let a = f.run();
    view.update(cx, |v, cx| v.receive_navigation(submit(2, "B"), cx));
    settle(&rt, cx);
    let b = f.run();
    publish(
        cx,
        json!({"type":"message","runId":a["runId"],"delta":"Background A"}),
    );
    publish(
        cx,
        json!({"type":"approval","runId":a["runId"],"approvalId":"permission-a","title":"Allow write?","command":"{}","canRemember":false}),
    );
    settle(&rt, cx);
    let chat = view.read_with(cx, |v, _| v.chat.view.clone().unwrap());
    chat.read_with(cx, |v, _| {
        assert_eq!(v.controller.active.as_deref(), b["sessionId"].as_str());
        let a = v
            .controller
            .conversation(a["sessionId"].as_str().unwrap())
            .unwrap();
        assert!(a.approval.is_some());
        assert_eq!(a.response().unwrap().text, "Background A");
    });
    assert!(view.read_with(cx, |v, _| {
        v.chat.background.iter().any(|s| {
            s.session_id == a["sessionId"].as_str().unwrap()
                && s.activity == RunActivity::WaitingForApproval
        })
    }));
    cx.update(|w, cx| view.update(cx, |v, cx| v.hide(w, cx)));
    publish(
        cx,
        json!({"type":"message","runId":b["runId"],"delta":"While hidden"}),
    );
    settle(&rt, cx);
    assert_eq!(
        chat.read_with(cx, |v, _| v
            .controller
            .conversation(b["sessionId"].as_str().unwrap())
            .unwrap()
            .response()
            .unwrap()
            .text
            .clone()),
        "While hidden"
    );
    assert_eq!(f.count("agent:cancel"), 0);
}

#[gpui::test]
fn quit_disposes_once_failed_persist_remains_owned_and_retry_saves_same_turn(
    cx: &mut TestAppContext,
) {
    let (rt, _root, f, view, cx) = mount(cx);
    view.update(cx, |v, cx| v.receive_navigation(submit(1, "Keep this"), cx));
    settle(&rt, cx);
    let run = f.run();
    publish(
        cx,
        json!({"type":"message","runId":run["runId"],"delta":"Partial durable answer"}),
    );
    f.fail_append.store(true, Ordering::SeqCst);
    let drain = cx.update(|w, cx| view.update(cx, |v, cx| v.drain_chat(w, cx)));
    settle(&rt, cx);
    assert!(drain.is_ready());
    assert!(rt.block_on(drain).is_err());
    assert_eq!(f.count("agent:cancel"), 1);
    let id = view.read_with(cx, |v, _| v.chat.view.as_ref().unwrap().entity_id());
    f.fail_append.store(false, Ordering::SeqCst);
    let retry = cx.update(|w, cx| view.update(cx, |v, cx| v.drain_chat(w, cx)));
    settle(&rt, cx);
    assert!(retry.is_ready());
    rt.block_on(retry).unwrap();
    assert_eq!(f.count("agent:cancel"), 1);
    assert_eq!(
        view.read_with(cx, |v, _| v.chat.view.as_ref().unwrap().entity_id()),
        id
    );
    let stored = rt
        .block_on(f.core.request(Request::Chat(ChatRequest::Get(
            run["sessionId"].as_str().unwrap().into(),
        ))))
        .unwrap();
    let saved = stored["turns"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == run["assistantTurnId"])
        .unwrap();
    assert_eq!(saved["text"], "Partial durable answer");
}

#[gpui::test]
fn actual_chat_uses_parent_footer_entity_and_hide_stops_its_polling(cx: &mut TestAppContext) {
    let (rt, _root, f, view, cx) = mount(cx);
    assert_eq!(f.count("background-tasks:list"), 0);
    view.update(cx, |v, cx| {
        v.receive_navigation(submit(1, "Footer owner"), cx)
    });
    settle(&rt, cx);
    let (chat, footer) = view.read_with(cx, |v, _| {
        (v.chat.view.clone().unwrap(), v.task_indicator.entity_id())
    });
    assert_eq!(
        chat.read_with(cx, |v, _| v.footer_entity_id()),
        Some(footer)
    );
    assert_eq!(f.count("background-tasks:list"), 1);
    cx.update(|w, cx| view.update(cx, |v, cx| v.hide(w, cx)));
    let calls = f.count("background-tasks:list");
    cx.executor().advance_clock(Duration::from_secs(30));
    settle(&rt, cx);
    assert_eq!(f.count("background-tasks:list"), calls);
    cx.update(|w, cx| view.update(cx, |v, cx| v.show(w, cx)));
    settle(&rt, cx);
    assert!(f.count("background-tasks:list") > calls);
    assert_eq!(
        chat.read_with(cx, |v, _| v.footer_entity_id()),
        Some(footer)
    );
}

#[gpui::test]
fn same_retained_entity_rejects_previous_mount_effect_before_host_io(cx: &mut TestAppContext) {
    let (rt, _root, _f, view, cx) = mount(cx);
    view.update(cx, |v, cx| v.receive_navigation(submit(1, "Source A"), cx));
    settle(&rt, cx);
    let source = view.read_with(cx, |v, cx| {
        let chat = v.chat.view.as_ref().unwrap();
        ChatSource {
            entity: chat.entity_id(),
            mount: chat.read(cx).host_generation(),
        }
    });
    assert!(view.read_with(cx, |v, cx| v.accepts_chat_source(source, cx)));
    view.update(cx, |v, cx| v.receive_navigation(submit(2, "Source B"), cx));
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| v.owns_chat(source.entity)));
    assert!(!view.read_with(cx, |v, cx| v.accepts_chat_source(source, cx)));
    let entered = Arc::new(AtomicBool::new(false));
    let seen = entered.clone();
    view.update(cx, |v, cx| {
        v.chat_image_job(
            source,
            1,
            Box::pin(async move {
                seen.store(true, Ordering::SeqCst);
                Err("Must not enter stale file/clipboard future".into())
            }),
            cx,
        )
    });
    settle(&rt, cx);
    assert!(!entered.load(Ordering::SeqCst));
}

#[gpui::test]
fn actual_navigation_screen_resume_back_and_footer_use_fresh_identity(cx: &mut TestAppContext) {
    let (rt, _root, f, view, cx) = mount(cx);
    let mut screen = submit(1, "");
    screen.payload = Payload::Ai {
        prompt: None,
        cwd: None,
        resume: None,
        capture: true,
    };
    view.update(cx, |v, cx| v.receive_navigation(screen.clone(), cx));
    settle(&rt, cx);
    assert_eq!(f.count("agent:capture-active-screen"), 1);
    let chat = view.read_with(cx, |v, _| v.chat.view.clone().unwrap());
    let a = chat.read_with(cx, |v, _| v.controller.active.clone().unwrap());
    chat.read_with(cx, |v, _| {
        assert_eq!(v.controller.conversation(&a).unwrap().attachments.len(), 1)
    });
    view.update(cx, |v, cx| v.receive_navigation(screen.clone(), cx));
    settle(&rt, cx);
    assert_eq!(f.count("agent:capture-active-screen"), 1);
    screen.nonce = 2;
    view.update(cx, |v, cx| v.receive_navigation(screen, cx));
    settle(&rt, cx);
    assert_eq!(f.count("agent:capture-active-screen"), 2);
    view.update(cx, |v, cx| {
        v.receive_navigation(submit(3, "Footer run"), cx)
    });
    settle(&rt, cx);
    let run = f.run();
    let row_id = format!("chat:{}", run["sessionId"].as_str().unwrap());
    let source = ChatSource {
        entity: chat.entity_id(),
        mount: chat.read_with(cx, |v, _| v.host_generation()),
    };
    cx.update(|w, cx| view.update(cx, |v, cx| v.chat_back(source, w, cx)));
    cx.executor().advance_clock(Duration::from_millis(8));
    settle(&rt, cx);
    view.read_with(cx, |v, _| {
        assert_eq!(
            launcher::input_mode(&v.model.input, v.model.terminal_mode).mode,
            Mode::Ai
        );
    });
    let activity = |cx: &mut VisualTestContext| {
        view.read_with(cx, |v, _| {
            let row = v.rows.iter().find(|row| row.id == row_id).unwrap();
            v.chat_row_status(row)
        })
    };
    assert_eq!(activity(cx), Some(RunActivity::Starting));
    publish(
        cx,
        json!({"type":"start","runId":run["runId"],"task":"Footer run"}),
    );
    settle(&rt, cx);
    assert_eq!(activity(cx), Some(RunActivity::Generating));
    for (index, label, tool, expected) in [
        (0, "Thinking…", "thinking", RunActivity::Thinking),
        (1, "Read file", "read_file", RunActivity::UsingTools),
    ] {
        publish(
            cx,
            json!({"type":"stage","runId":run["runId"],"stage":{"index":index,"label":label,"toolName":tool,"status":"running"}}),
        );
        settle(&rt, cx);
        assert_eq!(activity(cx), Some(expected));
        publish(
            cx,
            json!({"type":"stage","runId":run["runId"],"stage":{"index":index,"label":label,"toolName":tool,"status":"done"}}),
        );
        settle(&rt, cx);
        assert_eq!(activity(cx), Some(RunActivity::Generating));
    }
    assert_eq!(f.count("agent:cancel"), 0);
    publish(
        cx,
        json!({"type":"approval","runId":run["runId"],"approvalId":"footer-approval","title":"Allow?","command":"{}","canRemember":false}),
    );
    settle(&rt, cx);
    assert_eq!(activity(cx), Some(RunActivity::WaitingForApproval));
    let task = crate::surfaces::utilities::background_tasks::TaskStatus {
        id: row_id,
        kind: "chat".into(),
        title: "AI Chat".into(),
        detail: "Waiting".into(),
        progress: None,
        remaining_seconds: None,
        extension_id: None,
        command_name: None,
    };
    view.update(cx, |v, cx| v.open_background_task(&task, cx));
    // App's navigation mailbox re-delivers this retained intent, once.
    let intent = view.read_with(cx, |v, _| v.navigation.clone().unwrap());
    assert!(
        matches!(&intent.payload,Payload::Ai{resume:Some(id),..} if Some(id.as_str())==run["sessionId"].as_str())
    );
    view.update(cx, |v, cx| v.receive_navigation(intent, cx));
    settle(&rt, cx);
    chat.read_with(cx, |v, _| {
        assert_eq!(v.controller.active.as_deref(), run["sessionId"].as_str());
        assert!(
            v.controller
                .conversation(v.controller.active.as_deref().unwrap())
                .unwrap()
                .approval
                .is_some()
        );
    });
    assert_eq!(f.count("agent:run"), 1);
    assert_eq!(f.count("agent:approve"), 0);
}
#[gpui::test]
fn shared_picker_and_config_payload_events_change_next_snapshot_without_approving(
    cx: &mut TestAppContext,
) {
    let (rt, _root, f, view, cx) = mount(cx);
    f.core
        .0
        .ctx
        .config
        .patch(&json!({"providerModels":{"ollama":[{"id":"fixture"},{"id":"next"}]}}))
        .unwrap();
    view.update(cx, |v, cx| {
        v.model_choices = vec![("ollama".into(), "next".into())];
        v.select_model(0, cx);
    });
    settle(&rt, cx);
    view.update(cx, |v, cx| {
        v.receive_navigation(submit(1, "Selected via parent picker"), cx)
    });
    settle(&rt, cx);
    assert_eq!(
        f.run()["runSelection"],
        json!({"provider":"ollama","model":"next"})
    );
    assert!(f.run().get("approvalMode").is_none());
    let run = f.run();
    publish(
        cx,
        json!({"type":"approval","runId":run["runId"],"approvalId":"shared-mode","title":"Allow?","command":"{}","canRemember":false}),
    );
    settle(&rt, cx);
    let chat = view.read_with(cx, |v, _| v.chat.view.clone().unwrap());
    cx.update(|w, cx| {
        chat.update(cx, |v, cx| {
            v.action(
                crate::surfaces::ai_chat::Action::Mode(tezbar_ui_model::chat::ApprovalMode::Turbo),
                w,
                cx,
            )
        })
    });
    settle(&rt, cx);
    assert_eq!(
        f.core.0.ctx.config.read().unwrap()["agentApprovalMode"],
        "turbo"
    );
    assert_eq!(f.count("agent:approve"), 0);
    chat.read_with(cx, |v, _| {
        assert!(
            v.controller
                .conversation(run["sessionId"].as_str().unwrap())
                .unwrap()
                .approval
                .is_some()
        )
    });
    let before = f.count("llm-config-get");
    cx.cx.update(|cx| {
        cx.global::<CoreEvents>().clone().publish(
            CoreEvent::Other {
                channel: "llm-config-changed".into(),
                payload: json!({"revision":"actual-payload"}),
            },
            cx,
        )
    });
    settle(&rt, cx);
    assert!(f.count("llm-config-get") > before);
}

#[gpui::test]
fn dispose_cancels_hung_auxiliary_read_and_waits_only_for_mutation_jobs(cx: &mut TestAppContext) {
    let (rt, _root, f, view, cx) = mount(cx);
    view.update(cx, |v, cx| v.receive_navigation(submit(1, "Busy run"), cx));
    settle(&rt, cx);
    f.hold_aux.store(true, Ordering::SeqCst);
    let chat = view.read_with(cx, |v, _| v.chat.view.clone().unwrap());
    cx.update(|w, cx| {
        chat.update(cx, |v, cx| {
            v.action(crate::surfaces::ai_chat::Action::Panel, w, cx)
        })
    });
    settle(&rt, cx);
    assert_eq!(f.count("chat:list"), 1);
    assert!(!f.aux_dropped.load(Ordering::SeqCst));
    let drain = cx.update(|w, cx| view.update(cx, |v, cx| v.drain_chat(w, cx)));
    settle(&rt, cx);
    assert!(drain.is_ready());
    rt.block_on(drain).unwrap();
    assert!(f.aux_dropped.load(Ordering::SeqCst));
    assert_eq!(f.count("agent:cancel"), 1);
}
