//! Actual GPUI entities, production controller and finite in-memory core/host wires.
use super::*;
use ::core::prelude::v1::test;
use gpui::EntityInputHandler;
use std::{collections::VecDeque, sync::Mutex};
#[derive(Default)]
struct Data {
    config: Value,
    calls: Vec<(&'static str, Vec<Value>)>,
    sessions: BTreeMap<String, Value>,
    deleted: BTreeSet<String>,
    canonical_images: BTreeMap<(String, String, usize), Value>,
    fail: BTreeSet<&'static str>,
    gates: BTreeMap<&'static str, VecDeque<tokio::sync::oneshot::Receiver<()>>>,
}
#[derive(Clone, Default)]
struct Fixture(Arc<Mutex<Data>>);
impl Fixture {
    fn new() -> Self {
        let f = Self::default();
        f.0.lock().unwrap().config = json!({"provider":"ollama","model":"fixture","providerModels":{"ollama":[{"id":"fixture"},{"id":"other"}]},"providerConfigs":{"ollama":{"baseURL":"http://example.invalid"},"openai":{"apiKey":"synthetic-peer-key"}},"agentApprovalMode":"review","taskProviderOverrides":{"summarize":"openai"},"taskModelOverrides":{"summarize":"peer-model"}});
        f
    }
    fn hold(&self, channel: &'static str) -> tokio::sync::oneshot::Sender<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.0
            .lock()
            .unwrap()
            .gates
            .entry(channel)
            .or_default()
            .push_back(rx);
        tx
    }
    fn count(&self, channel: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|(c, _)| *c == channel)
            .count()
    }
    fn last(&self, channel: &str) -> Value {
        self.0
            .lock()
            .unwrap()
            .calls
            .iter()
            .rev()
            .find(|(c, _)| *c == channel)
            .unwrap()
            .1[0]
            .clone()
    }
    fn send_wire(&self, channel: &'static str, args: Vec<Value>) -> ChatFuture {
        let gate = {
            let mut d = self.0.lock().unwrap();
            d.calls.push((channel, args.clone()));
            d.gates.get_mut(channel).and_then(VecDeque::pop_front)
        };
        let state = self.0.clone();
        Box::pin(async move {
            if let Some(gate) = gate {
                let _ = gate.await;
            }
            let mut d = state.lock().unwrap();
            if d.fail.contains(channel) {
                return Err("synthetic-secret-never-render".into());
            }
            let a = args.first().cloned().unwrap_or(Value::Null);
            Ok(match channel {
                "llm-config-get"=>d.config.clone(),
                "llm-config-set"=>{for(k,v)in a.as_object().unwrap(){d.config[k]=v.clone();}Value::Null},
                "chat:list"=>Value::Array(d.sessions.values().map(|s|json!({"id":s["id"],"title":s["title"],"updatedAt":2,"preview":"Fixture preview"})).collect()),
                "chat:get"=>d.sessions.get(a.as_str().unwrap()).cloned().unwrap_or(Value::Null),
                "chat:append"=>{let id=a["session"]["id"].as_str().unwrap().to_string();if d.deleted.contains(&id){return Err("This conversation was deleted".into());}let s=d.sessions.entry(id).or_insert_with(||{let mut s=a["session"].clone();s["turns"]=json!([]);s});let turns=s["turns"].as_array_mut().unwrap();if let Some(t)=turns.iter_mut().find(|t|t["id"]==a["turn"]["id"]){*t=a["turn"].clone();}else{turns.push(a["turn"].clone());}json!({"ok":true})},
                "chat:delete"=>{let id=a.as_str().unwrap();d.sessions.remove(id);d.deleted.insert(id.into());d.canonical_images.retain(|(s,_,_),_|s!=id);json!({"ok":true})},
                "chat:clear"=>{let ids=d.sessions.keys().cloned().chain(args.get(1).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string)).collect::<Vec<_>>();d.deleted.extend(ids);d.sessions.clear();d.canonical_images.clear();json!({"ok":true})},
                "chat:delete-turn"=>{if let Some(s)=d.sessions.get_mut(a["sessionId"].as_str().unwrap()){s["turns"].as_array_mut().unwrap().retain(|t|t["id"]!=a["turnId"]);}json!({"ok":true})},
                "agent:run"=>{for(index,image)in a["images"].as_array().into_iter().flatten().enumerate(){d.canonical_images.insert((a["sessionId"].as_str().unwrap().into(),a["userTurnId"].as_str().unwrap().into(),index),image.clone());}json!({"ok":true,"runId":a["runId"]})},
                "chat:image"=>{let session=a["sessionId"].as_str().unwrap();if d.deleted.contains(session){Value::Null}else{d.canonical_images.get(&(session.into(),a["turnId"].as_str().unwrap().into(),a["index"].as_u64().unwrap() as usize)).cloned().unwrap_or(Value::Null)}},
                "agent:capture-active-screen"=>image(),
                "ai-extensions:composer-items"=>json!([{"extensionId":"fixture","extensionName":"Fixture","kind":"prompts","name":"summarize","arguments":[{"name":"topic","required":true}]}]),
                "ai-extensions:prompt-resolve"=>json!({"text":format!("Summarize {}",args[1]["topic"].as_str().unwrap_or(""))}),
                "ai-extensions:list"=>json!({"project":{"path":"/fixture/.tezbar/ai-extension.json"}}),
                "chat:generate-meta"=>json!({"title":"Fixture title","icon":"chat"}),
                _=>json!({"ok":true}),
            })
        })
    }
}
impl ChatTransport for Fixture {
    fn send(&self, r: ChatRequest) -> ChatFuture {
        let (c, a) = r.wire();
        self.send_wire(c, a)
    }
}
impl super::super::settings::SettingsTransport for Fixture {
    fn send(
        &self,
        r: super::super::settings::SettingsRequest,
    ) -> super::super::settings::SettingsFuture {
        let (c, a) = r.wire();
        self.send_wire(c, a)
    }
    fn durable_config(&self) -> super::super::settings::SettingsFuture {
        self.send_wire("llm-config-get", vec![])
    }
}
fn image() -> Value {
    use base64::Engine as _;
    use image::ImageEncoder as _;
    let mut bytes = vec![];
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(&[255, 128, 0, 255], 1, 1, image::ExtendedColorType::Rgba8)
        .unwrap();
    json!({"mimeType":"image/png","name":"Fixture screen","data":base64::engine::general_purpose::STANDARD.encode(bytes)})
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
fn mount<'a>(
    cx: &'a mut TestAppContext,
    f: &Fixture,
) -> (Entity<ChatView>, &'a mut VisualTestContext) {
    let transport = Arc::new(f.clone());
    let store = SettingsStore::new(transport.clone());
    cx.add_window_view(|w, cx| ChatView::new(transport, store, UiState::default(), w, cx))
}
fn new_chat(v: &Entity<ChatView>, cx: &mut VisualTestContext, id: &str) {
    cx.update(|w, cx| {
        v.update(cx, |v, cx| {
            v.activate(
                Boot::NewChat {
                    session_id: id.into(),
                    cwd: Some("/fixture".into()),
                },
                identity("nav"),
                w,
                cx,
            )
            .unwrap()
        })
    });
}
fn edit(v: &Entity<ChatView>, cx: &mut VisualTestContext, text: &str) {
    let input = v.read_with(cx, |v, _| v.input.clone());
    cx.update(|w, cx| input.update(cx, |i, cx| i.replace_text_in_range(None, text, w, cx)));
}
fn action(v: &Entity<ChatView>, cx: &mut VisualTestContext, a: Action) {
    cx.update(|w, cx| v.update(cx, |v, cx| v.action(a, w, cx)));
}
fn event(v: &Entity<ChatView>, cx: &mut VisualTestContext, payload: Value) {
    cx.update(|w, cx| v.update(cx, |v, cx| v.core_event("agent:event", payload, w, cx)));
}
fn press(cx: &mut VisualTestContext, key: &'static str) {
    let keystroke = Keystroke::parse(key).unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
}
fn run(v: &Entity<ChatView>, cx: &VisualTestContext) -> String {
    v.read_with(cx, |v, _| match &v.active().unwrap().phase {
        RunPhase::Running { run_id, .. } => run_id.clone(),
        _ => panic!("No run"),
    })
}
#[gpui::test]
fn constructor_and_repeat_nonce_do_no_work_and_submit_runs_once(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    assert_eq!(f.0.lock().unwrap().calls.len(), 0);
    let boot = Boot::Submit {
        session_id: "one".into(),
        prompt: "First prompt".into(),
        cwd: Some("/fixture".into()),
    };
    cx.update(|w, cx| {
        v.update(cx, |v, cx| {
            v.activate(boot.clone(), "same".into(), w, cx).unwrap()
        })
    });
    settle(&rt, cx);
    let counts = f.0.lock().unwrap().calls.len();
    cx.update(|w, cx| v.update(cx, |v, cx| v.activate(boot, "same".into(), w, cx).unwrap()));
    settle(&rt, cx);
    assert_eq!(f.count("agent:run"), 1);
    assert_eq!(f.0.lock().unwrap().calls.len(), counts);
    let wire = f.last("agent:run");
    assert_eq!(wire["task"], "First prompt");
    assert_eq!(
        wire["runSelection"],
        json!({"provider":"ollama","model":"fixture"})
    );
    assert!(wire.get("approvalMode").is_none());
    assert!(
        !format!("{:?}", ChatRequest::Copy("synthetic-secret".into())).contains("synthetic-secret")
    );
}
#[gpui::test]
fn streams_background_and_stop_persists_partial_exact_metadata(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "A");
    settle(&rt, cx);
    edit(&v, cx, "Question A");
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    let run_a = run(&v, cx);
    event(
        &v,
        cx,
        json!({"type":"message","runId":run_a,"delta":"Partial A"}),
    );
    new_chat(&v, cx, "B");
    edit(&v, cx, "Draft B");
    event(
        &v,
        cx,
        json!({"type":"message","runId":run_a,"delta":" hidden"}),
    );
    settle(&rt, cx);
    assert_eq!(
        v.read_with(cx, |v, _| v.active().unwrap().draft.clone()),
        "Draft B"
    );
    assert!(v.read_with(cx, |v, _| v.active().unwrap().response().is_none()));
    action(&v, cx, Action::Resume("A".into()));
    action(&v, cx, Action::Stop);
    settle(&rt, cx);
    let d = f.0.lock().unwrap();
    let a = d.sessions["A"]["turns"].as_array().unwrap().last().unwrap();
    assert_eq!(a["text"], "Partial A hidden");
    assert_eq!(a["error"], "Stopped by you.");
    assert_eq!(a["responseMeta"]["model"], "fixture");
    drop(d);
    assert_eq!(f.count("agent:cancel"), 1);
    event(
        &v,
        cx,
        json!({"type":"answer","runId":run_a,"text":"Late contamination"}),
    );
    settle(&rt, cx);
    assert!(
        !f.0.lock().unwrap().sessions["A"]
            .to_string()
            .contains("Late contamination")
    );
}
#[gpui::test]
fn preparation_freezes_cwd_model_and_cannot_start_after_delete(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "A");
    settle(&rt, cx);
    edit(&v, cx, "Original");
    let gate = f.hold("chat:append");
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    new_chat(&v, cx, "B");
    action(&v, cx, Action::Model("ollama".into(), "other".into()));
    settle(&rt, cx);
    let _ = gate.send(());
    settle(&rt, cx);
    let wire = f.last("agent:run");
    assert_eq!(wire["sessionId"], "A");
    assert_eq!(wire["runSelection"]["model"], "fixture");
    assert_eq!(wire["cwd"], "/fixture");
    action(&v, cx, Action::Resume("A".into()));
    action(&v, cx, Action::Stop);
    settle(&rt, cx);
    new_chat(&v, cx, "C");
    edit(&v, cx, "Delete before run");
    let gate = f.hold("chat:append");
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    action(&v, cx, Action::Delete("C".into()));
    action(&v, cx, Action::ConfirmDelete);
    settle(&rt, cx);
    let _ = gate.send(());
    settle(&rt, cx);
    assert_eq!(f.count("agent:run"), 1);
    assert!(!f.0.lock().unwrap().sessions.contains_key("C"));
}
#[gpui::test]
fn capture_host_completion_is_fenced_by_revision_navigation_hide_and_once(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "A");
    settle(&rt, cx);
    let ticket = cx.update(|_, cx| v.update(cx, |v, _| v.attachment_ticket().unwrap()));
    edit(&v, cx, "Typed after picker opened");
    cx.update(|w, cx| {
        v.update(cx, |v, cx| {
            v.attachments_result(ticket, Ok(vec![image()]), w, cx)
        })
    });
    assert_eq!(
        v.read_with(cx, |v, _| v.active().unwrap().attachments.len()),
        0
    );
    let gate = f.hold("agent:capture-active-screen");
    action(&v, cx, Action::Capture);
    action(&v, cx, Action::Capture);
    settle(&rt, cx);
    new_chat(&v, cx, "B");
    let _ = gate.send(());
    settle(&rt, cx);
    assert_eq!(f.count("agent:capture-active-screen"), 1);
    assert_eq!(
        v.read_with(cx, |v, _| v.active().unwrap().attachments.len()),
        0
    );
    let ticket = cx.update(|_, cx| v.update(cx, |v, _| v.attachment_ticket().unwrap()));
    cx.update(|w, cx| {
        v.update(cx, |v, cx| {
            v.attachments_result(ticket, Ok(vec![image()]), w, cx)
        })
    });
    cx.update(|w, cx| {
        v.update(cx, |v, cx| {
            v.attachments_result(ticket, Ok(vec![image()]), w, cx)
        })
    });
    assert_eq!(
        v.read_with(cx, |v, _| v.active().unwrap().attachments.len()),
        1
    );
    action(&v, cx, Action::RemoveImage(0));
    assert_eq!(
        v.read_with(cx, |v, _| v.active().unwrap().attachments.len()),
        0
    );
    let ticket = cx.update(|_, cx| v.update(cx, |v, _| v.attachment_ticket().unwrap()));
    cx.update(|_, cx| v.update(cx, |v, cx| v.set_visible(false, cx)));
    cx.update(|w, cx| {
        v.update(cx, |v, cx| {
            v.attachments_result(ticket, Ok(vec![image()]), w, cx)
        })
    });
    assert_eq!(
        v.read_with(cx, |v, _| v.active().unwrap().attachments.len()),
        0
    );
}
#[gpui::test]
fn background_approval_never_focuses_mode_does_not_approve_allow_click_once(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "A");
    settle(&rt, cx);
    edit(&v, cx, "Question");
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    let run_a = run(&v, cx);
    new_chat(&v, cx, "B");
    event(
        &v,
        cx,
        json!({"type":"approval","runId":run_a,"approvalId":"approval-a","title":"Allow Fixture to use write?","command":"{}","canRemember":false}),
    );
    assert_eq!(
        v.read_with(cx, |v, _| v.controller.conversation("A").unwrap().focus),
        ApprovalFocus::Composer
    );
    action(&v, cx, Action::Resume("A".into()));
    assert!(cx.update(|w, cx| v.read(cx).allow_focus.is_focused(w)));
    action(&v, cx, Action::Mode(ApprovalMode::Turbo));
    settle(&rt, cx);
    assert_eq!(f.count("agent:approve"), 0);
    assert!(v.read_with(cx, |v, _| v.active().unwrap().approval.is_some()));
    action(&v, cx, Action::Decide(Decision::Always));
    assert_eq!(f.count("agent:approve"), 0);
    action(&v, cx, Action::Decide(Decision::Once));
    action(&v, cx, Action::Decide(Decision::Once));
    settle(&rt, cx);
    assert_eq!(f.count("agent:approve"), 1);
    assert_eq!(f.last("agent:approve")["decision"], "once");
    let cfg = &f.0.lock().unwrap().config;
    assert_eq!(cfg["agentApprovalMode"], "turbo");
    assert_eq!(
        cfg["providerConfigs"]["openai"]["apiKey"],
        "synthetic-peer-key"
    );
    assert_eq!(cfg["taskModelOverrides"]["summarize"], "peer-model");
}
#[gpui::test]
fn clear_real_wire_covers_more_than_history_limit_and_failed_clear_repersists_dirty(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let f = Fixture::new();
    for n in 0..240 {
        f.0.lock().unwrap().sessions.insert(
            format!("saved-{n}"),
            json!({"id":format!("saved-{n}"),"title":"Saved","turns":[]}),
        );
    }
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "A");
    settle(&rt, cx);
    edit(&v, cx, "Question");
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    let run_a = run(&v, cx);
    let gate = f.hold("chat:clear");
    f.0.lock().unwrap().fail.insert("chat:clear");
    action(&v, cx, Action::Clear);
    action(&v, cx, Action::ConfirmDelete);
    settle(&rt, cx);
    event(
        &v,
        cx,
        json!({"type":"answer","runId":run_a,"text":"Retained while clearing"}),
    );
    event(&v, cx, json!({"type":"done","runId":run_a}));
    let _ = gate.send(());
    settle(&rt, cx);
    assert!(
        f.0.lock().unwrap().sessions["A"]
            .to_string()
            .contains("Retained while clearing")
    );
    assert!(v.read_with(cx, |v, _| v.controller.conversation("A").is_some()));
    f.0.lock().unwrap().fail.remove("chat:clear");
    action(&v, cx, Action::Clear);
    action(&v, cx, Action::ConfirmDelete);
    settle(&rt, cx);
    assert_eq!(f.count("chat:clear"), 2);
    assert!(f.0.lock().unwrap().sessions.is_empty());
    assert!(v.read_with(cx, |v, _| v.controller.active.is_none()));
}
#[gpui::test]
fn late_history_does_not_replace_typed_draft_and_prompt_args_never_autosend(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let f = Fixture::new();
    f.0.lock().unwrap().sessions.insert("A".into(),json!({"id":"A","title":"Saved","turns":[{"id":"old","role":"user","text":"Stored","createdAt":1}]}));
    let (v, cx) = mount(cx, &f);
    let gate = f.hold("chat:get");
    action(&v, cx, Action::Resume("A".into()));
    settle(&rt, cx);
    edit(&v, cx, "Typed before history completed");
    let _ = gate.send(());
    settle(&rt, cx);
    assert_eq!(
        v.read_with(cx, |v, _| v.active().unwrap().draft.clone()),
        "Typed before history completed"
    );
    assert!(v.read_with(cx, |v, _| v.active().unwrap().turns.is_empty()));
    action(&v, cx, Action::ChoosePrompt(0));
    action(&v, cx, Action::ResolvePrompt);
    assert_eq!(f.count("ai-extensions:prompt-resolve"), 0);
    let input = v.read_with(cx, |v, _| v.arguments["topic"].clone());
    cx.update(|w, cx| {
        input.update(cx, |i, cx| {
            i.replace_text_in_range(None, "Native Rust", w, cx)
        })
    });
    action(&v, cx, Action::ResolvePrompt);
    settle(&rt, cx);
    assert_eq!(
        v.read_with(cx, |v, _| v.active().unwrap().draft.clone()),
        "Summarize Native Rust"
    );
    assert_eq!(f.count("agent:run"), 0);
    new_chat(&v, cx, "review-folder");
    settle(&rt, cx);
    action(&v, cx, Action::ReviewProject);
    let effects = cx.update(|_, cx| v.update(cx, |v, _| v.take_effects()));
    assert!(effects.iter().any(
        |e| matches!(e,ChatEffect::ReviewProject{cwd,nonce}if cwd=="/fixture"&&!nonce.is_empty())
    ));
}
#[test]
fn run_wire_keeps_minimal_unsaved_context_and_metadata_no_rendered_stage_payload() {
    let user = Arc::new(Turn {
        id: "u".into(),
        role: chat::Role::User,
        text: "Canonical".into(),
        error: None,
        response_meta: None,
        references: vec![],
        stages: vec![],
        created_at: 1,
        timeline: vec![],
    });
    let r = chat::RunRequest {
        run_id: "agent-fixture".into(),
        session_id: "s".into(),
        user_turn_id: "current".into(),
        assistant_turn_id: "response".into(),
        task: Arc::from("followup"),
        prior_turn_ids: vec!["u".into(), "a".into()],
        prior_turns: vec![user],
        references: vec![],
        selection: Selection {
            provider: Some("ollama".into()),
            provider_title: Some("Ollama".into()),
            model: Some("fixture".into()),
            approval_mode: Some(ApprovalMode::Review),
            ..Default::default()
        },
        retry: false,
        history_has_images: true,
    };
    let (_, args) = ChatRequest::Run {
        request: r,
        images: vec![],
    }
    .wire();
    let wire = &args[0];
    assert_eq!(wire["priorTurnIds"], json!(["u", "a"]));
    assert_eq!(wire["priorTurns"][0]["text"], "Canonical");
    assert!(wire["priorTurns"][0].get("stages").is_none());
    assert!(wire.get("approvalMode").is_none());
    assert_eq!(wire["historyHasImages"], true);
}
#[gpui::test]
fn retry_loads_canonical_user_reuses_assistant_and_never_sends_empty_image_ids(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "A");
    settle(&rt, cx);
    edit(&v, cx, "Original prompt");
    let ticket = cx.update(|_, cx| v.update(cx, |v, _| v.attachment_ticket().unwrap()));
    cx.update(|w, cx| {
        v.update(cx, |v, cx| {
            v.attachments_result(ticket, Ok(vec![image()]), w, cx)
        })
    });
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    let original = f.last("agent:run");
    assert_eq!(original["images"].as_array().unwrap().len(), 1);
    let run_a = run(&v, cx);
    event(
        &v,
        cx,
        json!({"type":"error","runId":run_a,"message":"Synthetic provider unavailable"}),
    );
    settle(&rt, cx);
    let assistant = original["assistantTurnId"].as_str().unwrap().to_string();
    let user = original["userTurnId"].as_str().unwrap().to_string();
    {
        let mut d = f.0.lock().unwrap();
        let turns = d.sessions.get_mut("A").unwrap()["turns"]
            .as_array_mut()
            .unwrap();
        turns.iter_mut().find(|t| t["id"] == user).unwrap()["text"] =
            json!("Canonical stored prompt");
    }
    action(&v, cx, Action::Retry(assistant.clone()));
    settle(&rt, cx);
    let retry = f.last("agent:run");
    assert_eq!(retry["task"], "Canonical stored prompt");
    assert_eq!(retry["assistantTurnId"], assistant);
    assert_eq!(retry["retry"], true);
    assert_eq!(retry["images"], json!([]));
    assert!(retry["priorTurnIds"].as_array().unwrap().is_empty());
    let run_retry = run(&v, cx);
    event(
        &v,
        cx,
        json!({"type":"answer","runId":run_retry,"text":"Recovered"}),
    );
    event(&v, cx, json!({"type":"done","runId":run_retry}));
    settle(&rt, cx);
    let d = f.0.lock().unwrap();
    assert_eq!(
        d.sessions["A"]["turns"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["id"] == assistant)
            .count(),
        1
    );
    drop(d);
    let count = f.count("agent:run");
    action(&v, cx, Action::Retry(assistant));
    settle(&rt, cx);
    assert_eq!(f.count("agent:run"), count);
}
#[gpui::test]
fn composer_shift_enter_ime_and_native_allow_enter_have_separate_activation(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "A");
    settle(&rt, cx);
    edit(&v, cx, "First line");
    let input = v.read_with(cx, |v, _| v.input.clone());
    cx.update(|w, cx| {
        input.update(cx, |i, cx| {
            i.replace_and_mark_text_in_range(None, "に", None, w, cx)
        })
    });
    press(cx, "enter");
    assert_eq!(f.count("agent:run"), 0);
    assert!(input.read_with(cx, |i, _| i.marked_range.is_some()));
    cx.update(|w, cx| input.update(cx, |i, cx| i.replace_text_in_range(None, "日本", w, cx)));
    cx.run_until_parked();
    cx.simulate_keystrokes("shift-enter");
    assert_eq!(f.count("agent:run"), 0);
    assert!(v.read_with(cx, |v, cx| v.input.read(cx).content.contains('\n')));
    press(cx, "enter");
    settle(&rt, cx);
    assert_eq!(f.count("agent:run"), 1);
    let run_a = run(&v, cx);
    event(
        &v,
        cx,
        json!({"type":"approval","runId":run_a,"approvalId":"approve","title":"Allow Fixture to use write?","command":"{}","canRemember":false}),
    );
    cx.run_until_parked();
    let input = v.read_with(cx, |v, _| v.input.clone());
    cx.update(|w, cx| {
        input.focus_handle(cx).focus(w, cx);
    });
    press(cx, "enter");
    settle(&rt, cx);
    assert_eq!(f.count("agent:approve"), 0);
    cx.update(|w, cx| {
        let focus = v.read(cx).allow_focus.clone();
        focus.focus(w, cx);
    });
    cx.run_until_parked();
    press(cx, "enter");
    settle(&rt, cx);
    assert_eq!(f.count("agent:approve"), 1);
}
#[gpui::test]
fn huge_tool_output_keeps_chronological_canonical_identity_context(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "A");
    settle(&rt, cx);
    let mut expected = vec![];
    for n in 0..10 {
        edit(&v, cx, &format!("Question {n}"));
        action(&v, cx, Action::Send);
        settle(&rt, cx);
        let wire = f.last("agent:run");
        expected.push(wire["userTurnId"].clone());
        expected.push(wire["assistantTurnId"].clone());
        let run_a = run(&v, cx);
        for index in 0..70 {
            event(
                &v,
                cx,
                json!({"type":"stage","runId":run_a,"stage":{"index":index,"label":"Fixture output","status":"done","output":"x".repeat(32768)}}),
            );
        }
        event(
            &v,
            cx,
            json!({"type":"answer","runId":run_a,"text":format!("Answer {n}")}),
        );
        event(&v, cx, json!({"type":"done","runId":run_a}));
        settle(&rt, cx);
    }
    edit(&v, cx, "Follow up");
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    let wire = f.last("agent:run");
    assert_eq!(
        wire["priorTurnIds"],
        json!(&expected[expected.len() - 16..])
    );
    assert_eq!(wire["priorTurns"], json!([]));
    assert!(wire.to_string().len() < 4096);
    assert!(v.read_with(cx, |v, _| v.projection.len() < 600));
}
#[gpui::test]
fn config_mode_write_does_not_overwrite_another_owners_chat_selection(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "A");
    settle(&rt, cx);
    edit(&v, cx, "A prompt");
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    action(&v, cx, Action::Stop);
    settle(&rt, cx);
    let (store, peer) = v.read_with(cx, |v, _| (v.store.clone(), v.store.owner()));
    store.enqueue(
        peer,
        vec![Edit::TaskSelection {
            task: "chat".into(),
            provider: "ollama".into(),
            model: "other".into(),
        }],
    );
    rt.block_on(store.drain_owner(peer)).unwrap();
    action(&v, cx, Action::Mode(ApprovalMode::Auto));
    settle(&rt, cx);
    assert_eq!(
        f.0.lock().unwrap().config["taskModelOverrides"]["chat"],
        "other"
    );
    assert_eq!(f.0.lock().unwrap().config["agentApprovalMode"], "auto");
}
mod core_loopback;
#[gpui::test]
fn document_image_update_emits_one_host_ticket_and_hide_fences_decode(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "A");
    settle(&rt, cx);
    edit(&v, cx, "Show image");
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    let run_a = run(&v, cx);
    event(
        &v,
        cx,
        json!({"type":"answer","runId":run_a,"text":"![Fixture image](https://example.invalid/image.png)"}),
    );
    event(&v, cx, json!({"type":"done","runId":run_a}));
    settle(&rt, cx);
    let requests = cx.update(|_, cx| {
        v.update(cx, |v, _| {
            v.take_effects()
                .into_iter()
                .filter_map(|e| {
                    if let ChatEffect::DocumentImage { ticket, source } = e {
                        Some((ticket, source))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
    });
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].1, "https://example.invalid/image.png");
    cx.run_until_parked();
    let requests_after_render = cx.update(|_, cx| {
        v.update(cx, |v, _| {
            v.take_effects()
                .into_iter()
                .filter(|e| matches!(e, ChatEffect::DocumentImage { .. }))
                .count()
        })
    });
    assert_eq!(requests_after_render, 0);
    let mut png = std::io::Cursor::new(vec![]);
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        2,
        2,
        image::Rgba([255, 128, 0, 255]),
    ))
    .write_to(&mut png, image::ImageFormat::Png)
    .unwrap();
    cx.update(|w, cx| {
        v.update(cx, |v, cx| {
            v.document_image_result(requests[0].0, Ok(png.into_inner()), w, cx)
        })
    });
    rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                cx.run_until_parked();
                if v.read_with(cx, |v, _| v.jobs.is_empty()) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    });
    assert!(cx.update(|_, cx| v.update(cx, |v, _| {
        v.images.get("https://example.invalid/image.png").is_some()
    })));
    cx.update(|_, cx| v.update(cx, |v, cx| v.set_visible(false, cx)));
    assert!(cx.update(|_, cx| v.update(cx, |v, _| {
        v.images.get("https://example.invalid/image.png").is_none()
    })));
}
#[test]
fn native_projection_preserves_stream_text_stage_text_and_replaces_stage_in_place() {
    let turn = Arc::new(Turn {
        id: "assistant".into(),
        role: chat::Role::Assistant,
        text: "firstlater".into(),
        error: None,
        response_meta: None,
        references: vec![],
        created_at: 1,
        timeline: vec![],
        stages: vec![chat::Stage {
            index: 1,
            label: "Tool".into(),
            status: chat::StageStatus::Done,
            detail: None,
            output: Some("Done".into()),
            tool_name: None,
            duration_ms: None,
            output_truncated: false,
        }],
    });
    let rows = view::project(
        &[turn],
        Some("assistant"),
        &[
            chat::TimelineItem::Text { start: 0, end: 5 },
            chat::TimelineItem::Stage(1),
            chat::TimelineItem::Text { start: 5, end: 10 },
        ],
    );
    assert!(matches!(&rows[1],view::Row::Text{range,..}if range==&(0..5)));
    assert!(matches!(&rows[2],view::Row::Stage(t,0)if t.stages[0].status==chat::StageStatus::Done));
    assert!(matches!(&rows[3],view::Row::Text{range,..}if range==&(5..10)));
    assert_eq!(rows.len(), 5);
    let view::Row::Header(turn) = &rows[0] else {
        panic!("first row is the turn header")
    };
    let mut settled = (**turn).clone();
    settled.timeline = vec![
        chat::TimelineItem::Text { start: 0, end: 5 },
        chat::TimelineItem::Stage(1),
        chat::TimelineItem::Text { start: 5, end: 10 },
    ];
    let rows = view::project(&[Arc::new(settled)], None, &[]);
    assert!(matches!(&rows[1],view::Row::Text{range,settled:true,..}if range==&(0..5)));
    assert!(matches!(&rows[2], view::Row::Stage(_, 0)));
    assert!(matches!(&rows[3],view::Row::Text{range,settled:true,..}if range==&(5..10)));
}
#[gpui::test]
fn native_a11y_shows_approval_and_scrubs_known_provider_diagnostics(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "A");
    settle(&rt, cx);
    edit(&v, cx, "Question");
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    let run_a = run(&v, cx);
    event(
        &v,
        cx,
        json!({"type":"approval","runId":run_a,"approvalId":"approval","title":"Allow Fixture to use write?","command":"{}","canRemember":true}),
    );
    cx.run_until_parked();
    // Same TestWindow limitation proven by M7. Assert the actual production elements'
    // AccessKit roles/names/options; physical tree/VoiceOver remain an acceptance gate.
    assert!(cx.update(|w, _| w.debug_a11y_tree_json()).is_none());
    cx.update(|w, cx| {
        v.update(cx, |v, cx| {
            let t = tezbar_ui::tokens::tokens(w);
            let card = v.approval(t, cx);
            assert_eq!(card.a11y_role(), Some(Role::AlertDialog));
            let mut node = gpui::accesskit::Node::new(Role::AlertDialog);
            card.write_a11y_info(&mut node);
            assert_eq!(node.label(), Some("Allow Fixture to use write?"));
            for (label, decision) in [
                ("Allow once", Decision::Once),
                ("Deny", Decision::Deny),
                ("Always allow this command", Decision::Always),
            ] {
                let button = v.b(label, label, Action::Decide(decision), t, cx);
                assert_eq!(button.a11y_role(), Some(Role::Button));
                let mut node = gpui::accesskit::Node::new(Role::Button);
                button.write_a11y_info(&mut node);
                assert_eq!(node.label(), Some(label));
            }
            assert_eq!(v.input.read(cx).options.name.as_ref(), "Message");
            assert!(v.input.read(cx).options.multiline);
            assert!(!node.label().unwrap().contains("synthetic-peer-key"));
        })
    });
    event(
        &v,
        cx,
        json!({"type":"log","runId":run_a,"source":"native","line":"Denied synthetic-peer-key"}),
    );
    event(
        &v,
        cx,
        json!({"type":"error","runId":run_a,"message":"Failure synthetic-peer-key"}),
    );
    settle(&rt, cx);
    action(&v, cx, Action::Expand("diagnostics".into()));
    cx.run_until_parked();
    v.read_with(cx, |v, _| {
        let diagnostics = v.diagnostics["A"].join("\n");
        let error = v
            .active()
            .unwrap()
            .turns
            .back()
            .unwrap()
            .error
            .as_deref()
            .unwrap();
        assert!(!diagnostics.contains("synthetic-peer-key"));
        assert!(diagnostics.contains("[redacted]"));
        assert!(!error.contains("synthetic-peer-key"));
        assert!(error.contains("[redacted]"));
    });
}
#[gpui::test]
fn failed_background_persistence_remains_owned_and_retry_saves_the_same_turn(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "A");
    settle(&rt, cx);
    edit(&v, cx, "Question A");
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    let wire = f.last("agent:run");
    let run_a = run(&v, cx);
    new_chat(&v, cx, "B");
    f.0.lock().unwrap().fail.insert("chat:append");
    event(
        &v,
        cx,
        json!({"type":"answer","runId":run_a,"text":"Unsaved A"}),
    );
    event(&v, cx, json!({"type":"done","runId":run_a}));
    settle(&rt, cx);
    assert!(v.read_with(cx, |v, _| v.controller.conversation("A").unwrap().unsaved()));
    assert!(
        !f.0.lock().unwrap().sessions["A"]
            .to_string()
            .contains("Unsaved A")
    );
    action(&v, cx, Action::Resume("A".into()));
    assert!(
        v.read_with(cx, |v, _| v.active().unwrap().turns.back().unwrap().text
            == "Unsaved A")
    );
    f.0.lock().unwrap().fail.remove("chat:append");
    action(&v, cx, Action::Save);
    settle(&rt, cx);
    assert!(!v.read_with(cx, |v, _| v.active().unwrap().unsaved()));
    let d = f.0.lock().unwrap();
    let turn = d.sessions["A"]["turns"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == wire["assistantTurnId"])
        .unwrap();
    assert_eq!(turn["text"], "Unsaved A");
}

#[gpui::test]
fn delayed_config_delete_clear_and_stop_never_append_or_start(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    for (n, kind) in ["delete", "clear", "stop"].iter().enumerate() {
        let id = format!("owner-{n}");
        new_chat(&v, cx, &id);
        settle(&rt, cx);
        edit(&v, cx, "Prepared but unwritten");
        let append_before = f.count("chat:append");
        let gate = f.hold("llm-config-get");
        action(&v, cx, Action::Send);
        settle(&rt, cx);
        match *kind {
            "delete" => {
                action(&v, cx, Action::Delete(id.clone()));
                action(&v, cx, Action::ConfirmDelete);
            }
            "clear" => {
                action(&v, cx, Action::Clear);
                action(&v, cx, Action::ConfirmDelete);
            }
            _ => action(&v, cx, Action::Stop),
        }
        settle(&rt, cx);
        let _ = gate.send(());
        settle(&rt, cx);
        assert_eq!(f.count("chat:append"), append_before);
        assert_eq!(f.count("agent:run"), 0);
        assert!(!f.0.lock().unwrap().sessions.contains_key(&id));
    }
}
#[gpui::test]
fn replayed_nonce_after_different_navigation_does_no_io(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    let boot = Boot::Submit {
        session_id: "A".into(),
        prompt: "Exactly once".into(),
        cwd: None,
    };
    cx.update(|w, cx| {
        v.update(cx, |v, cx| {
            v.activate(boot.clone(), "submit-once".into(), w, cx)
                .unwrap()
        })
    });
    settle(&rt, cx);
    new_chat(&v, cx, "B");
    settle(&rt, cx);
    let count = f.0.lock().unwrap().calls.len();
    cx.update(|w, cx| {
        v.update(cx, |v, cx| {
            v.activate(boot, "submit-once".into(), w, cx).unwrap()
        })
    });
    settle(&rt, cx);
    assert_eq!(f.0.lock().unwrap().calls.len(), count);
    assert_eq!(f.count("agent:run"), 1);
    assert_eq!(
        v.read_with(cx, |v, _| v.controller.active.clone()),
        Some("B".into())
    );
}
#[gpui::test]
fn capture_failure_unlocks_and_stale_failure_cannot_erase_new_capture(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "A");
    settle(&rt, cx);
    f.0.lock()
        .unwrap()
        .fail
        .insert("agent:capture-active-screen");
    action(&v, cx, Action::Capture);
    settle(&rt, cx);
    assert!(v.read_with(cx, |v, _| v.pending_capture.is_none()));
    assert!(v.read_with(cx, |v, _| {
        v.error
            .as_deref()
            .is_some_and(|e| e.contains("Screen capture failed"))
    }));
    f.0.lock()
        .unwrap()
        .fail
        .remove("agent:capture-active-screen");
    let old = f.hold("agent:capture-active-screen");
    action(&v, cx, Action::Capture);
    settle(&rt, cx);
    new_chat(&v, cx, "B");
    let fresh = f.hold("agent:capture-active-screen");
    action(&v, cx, Action::Capture);
    settle(&rt, cx);
    let _ = old.send(());
    settle(&rt, cx);
    assert!(v.read_with(cx, |v, _| v.pending_capture.is_some()));
    assert!(v.read_with(cx, |v, _| v.active().unwrap().attachments.is_empty()));
    let _ = fresh.send(());
    settle(&rt, cx);
    assert_eq!(
        v.read_with(cx, |v, _| v.active().unwrap().attachments.len()),
        1
    );
}
#[gpui::test]
fn attachment_batch_rolls_back_deduplicates_and_core_sized_payload_is_admitted(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "A");
    settle(&rt, cx);
    cx.update(|_, cx| {
        v.update(cx, |v, _| {
            assert!(
                v.attach_images("A", vec![image(), json!({"mimeType":"image/png"})])
                    .is_err()
            );
            assert!(v.blobs.values.is_empty());
            assert!(v.active().unwrap().attachments.is_empty());
            v.attach_images("A", vec![image(), image()]).unwrap();
            assert_eq!(v.active().unwrap().attachments.len(), 1);
            assert_eq!(v.blobs.values.len(), 1);
            let large = json!({"mimeType":"image/png","data":"AAAA".repeat(1024*1024)});
            v.attach_images("A", vec![large]).unwrap();
            assert_eq!(v.active().unwrap().attachments.len(), 2);
            assert!(v.attach_images("A", vec![image(); 5]).is_err());
            assert_eq!(v.active().unwrap().attachments.len(), 2);
        })
    });
}
#[gpui::test]
fn historical_image_uses_exact_canonical_read_and_navigation_rejects_late_result(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let f = Fixture::new();
    f.0.lock().unwrap().sessions.insert("A".into(),json!({"id":"A","title":"Images","turns":[{"id":"user-old","role":"user","text":"Screen","createdAt":1,"attachments":[{"name":"Historical","mimeType":"image/png"}]}]}));
    f.0.lock()
        .unwrap()
        .canonical_images
        .insert(("A".into(), "user-old".into(), 0), image());
    let gate = f.hold("chat:image");
    let (v, cx) = mount(cx, &f);
    action(&v, cx, Action::Resume("A".into()));
    settle(&rt, cx);
    assert_eq!(
        f.last("chat:image"),
        json!({"sessionId":"A","turnId":"user-old","index":0})
    );
    assert!(v.read_with(cx, |v, _| v.blobs.values.is_empty()));
    new_chat(&v, cx, "B");
    let _ = gate.send(());
    settle(&rt, cx);
    assert!(v.read_with(cx, |v, _| v.document_images.is_empty()));
    assert!(v.read_with(cx, |v, _| v.active().unwrap().attachments.is_empty()));
    assert_eq!(f.count("agent:run"), 0);
}
#[gpui::test]
fn done_overtakes_failed_individual_delete_and_background_durable_write_is_returned(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "A");
    settle(&rt, cx);
    edit(&v, cx, "Question");
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    let run_a = run(&v, cx);
    new_chat(&v, cx, "B");
    let gate = f.hold("chat:delete");
    f.0.lock().unwrap().fail.insert("chat:delete");
    action(&v, cx, Action::Delete("A".into()));
    action(&v, cx, Action::ConfirmDelete);
    settle(&rt, cx);
    event(
        &v,
        cx,
        json!({"type":"answer","runId":run_a,"text":"Deferred response A"}),
    );
    event(&v, cx, json!({"type":"done","runId":run_a}));
    assert!(
        !f.0.lock().unwrap().sessions["A"]
            .to_string()
            .contains("Deferred response A")
    );
    let _ = gate.send(());
    settle(&rt, cx);
    assert!(
        f.0.lock().unwrap().sessions["A"]
            .to_string()
            .contains("Deferred response A")
    );
    assert_eq!(
        v.read_with(cx, |v, _| v.controller.active.clone()),
        Some("B".into())
    );
    assert!(!v.read_with(cx, |v, _| v.controller.conversation("A").unwrap().unsaved()));
}
#[gpui::test]
fn actual_dispose_fences_preparation_but_hide_keeps_running(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "A");
    settle(&rt, cx);
    edit(&v, cx, "Question");
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    let run_a = run(&v, cx);
    cx.update(|_, cx| v.update(cx, |v, cx| v.set_visible(false, cx)));
    event(
        &v,
        cx,
        json!({"type":"message","runId":run_a,"delta":"Hidden partial"}),
    );
    assert_eq!(f.count("agent:cancel"), 0);
    cx.update(|w, cx| v.update(cx, |v, cx| v.dispose(w, cx)));
    settle(&rt, cx);
    assert_eq!(f.count("agent:cancel"), 1);
    assert!(
        f.0.lock().unwrap().sessions["A"]
            .to_string()
            .contains("Hidden partial")
    );
    assert!(!v.read_with(cx, |v, _| v.pending()));
    let f2 = Fixture::new();
    let transport = Arc::new(f2.clone());
    let store = SettingsStore::new(transport.clone());
    let second =
        cx.update(|w, cx| cx.new(|cx| ChatView::new(transport, store, UiState::default(), w, cx)));
    new_chat(&second, cx, "preparing");
    settle(&rt, cx);
    edit(&second, cx, "Unsent");
    let gate = f2.hold("llm-config-get");
    action(&second, cx, Action::Send);
    settle(&rt, cx);
    cx.update(|w, cx| second.update(cx, |v, cx| v.dispose(w, cx)));
    let _ = gate.send(());
    settle(&rt, cx);
    assert_eq!(f2.count("agent:run"), 0);
    assert_eq!(f2.count("chat:append"), 0);
}
#[test]
fn base64_padding_and_full_core_image_budget_are_checked_separately_from_preview() {
    assert!(references::image_bytes("A===").is_err());
    assert!(references::image_bytes("AA=A").is_err());
    assert!(references::image_bytes("AB==").is_err());
    assert!(references::image_bytes("AAA").is_err());
    assert_eq!(references::image_bytes("AA==").unwrap(), [0]);
    assert_eq!(
        references::image_bytes(&"AAAA".repeat(1024 * 1024))
            .unwrap()
            .len(),
        3 * 1024 * 1024
    );
}

#[test]
fn exact_eight_mib_attachment_matches_canonical_input_size() {
    use base64::Engine as _;
    let bytes = vec![0; 8 * 1024 * 1024];
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    assert_eq!(
        references::image_bytes(&encoded).unwrap().len(),
        bytes.len()
    );
    let over = base64::engine::general_purpose::STANDARD.encode(vec![0; bytes.len() + 1]);
    assert!(references::image_bytes(&over).is_err());
}

#[gpui::test]
fn repair_prepared_append_is_durable_work_and_attachment_queue_refusals_settle(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "repair");
    settle(&rt, cx);
    let gate = f.hold("chat:append");
    edit(&v, cx, "preparing durable turn");
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    cx.update(|w, cx| v.update(cx, |v, cx| v.prepare_quit(w, cx)));
    assert_eq!(v.read_with(cx, |v, _| v.quit_settlement()), Ok(false));
    gate.send(()).unwrap();
    settle(&rt, cx);
    cx.update(|w, cx| v.update(cx, |v, cx| v.dispose(w, cx)));
    settle(&rt, cx);
    assert_eq!(v.read_with(cx, |v, _| v.quit_settlement()), Ok(true));
    assert_eq!(
        f.0.lock().unwrap().sessions["repair"]["turns"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["role"] == "user")
            .count(),
        1
    );
    v.update(cx, |v, cx| v.revive_after_quit(cx));
    new_chat(&v, cx, "attachments");
    settle(&rt, cx);
    // Exercise the actual 64-effect refusal, beyond the independent four-ticket cap.
    v.update(cx, |v, cx| {
        v.effects.clear();
        for _ in 0..64 {
            v.emit(ChatEffect::Settings, cx);
        }
    });
    action(&v, cx, Action::ChooseImages);
    action(&v, cx, Action::ClipboardImage);
    assert!(v.read_with(cx, |v, _| v.attachment_tickets.is_empty()));
    v.update(cx, |v, cx| {
        v.effects.clear();
        for _ in 0..80 {
            v.emit(ChatEffect::Background(Vec::new()), cx);
        }
        assert_eq!(v.effects.len(), 1, "background status must coalesce");
    });
    for _ in 0..80 {
        action(&v, cx, Action::ChooseImages);
    }
    let effects = v.update(cx, |v, _| v.take_effects());
    for effect in effects {
        if let ChatEffect::ChooseImages { ticket } = effect {
            cx.update(|w, cx| {
                v.update(cx, |v, cx| {
                    v.attachments_result(ticket, Err("fixture refusal".into()), w, cx)
                })
            });
        }
    }
    assert!(v.read_with(cx, |v, _| v.attachment_tickets.is_empty()));
}

#[gpui::test]
fn review_n2_preparation_waits_for_cancelled_quit_before_starting_once(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "quit-preparing");
    settle(&rt, cx);
    let gate = f.hold("chat:append");
    edit(&v, cx, "sent just before quit");
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    assert_eq!(f.count("agent:run"), 0);
    cx.update(|w, cx| v.update(cx, |v, cx| v.prepare_quit(w, cx)));
    gate.send(()).unwrap();
    settle(&rt, cx);
    assert_eq!(
        f.count("agent:run"),
        0,
        "quit preparation started a provider run"
    );
    assert_eq!(v.read_with(cx, |v, _| v.quit_settlement()), Ok(true));
    v.update(cx, |v, cx| v.revive_after_quit(cx));
    settle(&rt, cx);
    assert_eq!(f.count("agent:run"), 1);
    v.update(cx, |v, cx| v.revive_after_quit(cx));
    settle(&rt, cx);
    assert_eq!(
        f.count("agent:run"),
        1,
        "revival replayed the prepared send"
    );
}
#[gpui::test]
fn review_n2_successful_quit_drops_prepared_run_without_provider_cost(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "quit-committed");
    settle(&rt, cx);
    let gate = f.hold("chat:append");
    edit(&v, cx, "persist but do not run");
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    cx.update(|w, cx| v.update(cx, |v, cx| v.prepare_quit(w, cx)));
    gate.send(()).unwrap();
    settle(&rt, cx);
    cx.update(|w, cx| v.update(cx, |v, cx| v.dispose(w, cx)));
    settle(&rt, cx);
    assert_eq!(f.count("agent:run"), 0);
    assert_eq!(f.count("agent:cancel"), 0);
}

#[gpui::test]
fn review_n2_reentered_quit_refences_deferred_preparation(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "quit-reentry");
    settle(&rt, cx);
    let gate = f.hold("chat:append");
    edit(&v, cx, "reenter");
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    cx.update(|w, cx| v.update(cx, |v, cx| v.prepare_quit(w, cx)));
    gate.send(()).unwrap();
    settle(&rt, cx);
    cx.update(|w, cx| {
        v.update(cx, |v, cx| {
            v.revive_after_quit(cx);
            v.prepare_quit(w, cx);
        })
    });
    settle(&rt, cx);
    assert_eq!(f.count("agent:run"), 0);
    v.update(cx, |v, cx| v.revive_after_quit(cx));
    settle(&rt, cx);
    assert_eq!(f.count("agent:run"), 1);
}
#[gpui::test]
fn review_n2_failed_user_append_refuses_quit_and_restores_draft(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "quit-append-failure");
    settle(&rt, cx);
    let gate = f.hold("chat:append");
    edit(&v, cx, "keep this draft");
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    cx.update(|w, cx| v.update(cx, |v, cx| v.prepare_quit(w, cx)));
    f.0.lock().unwrap().fail.insert("chat:append");
    gate.send(()).unwrap();
    settle(&rt, cx);
    assert!(v.read_with(cx, |v, _| v.quit_settlement()).is_err());
    v.update(cx, |v, cx| v.revive_after_quit(cx));
    settle(&rt, cx);
    assert_eq!(f.count("agent:run"), 0);
    assert_eq!(
        v.read_with(cx, |v, _| v.active().unwrap().draft.clone()),
        "keep this draft"
    );
}
#[gpui::test]
fn transcript_messages_size_to_content_and_composer_fits_narrow_window(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "A");
    settle(&rt, cx);
    edit(&v, cx, "Short question");
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    let run_a = run(&v, cx);
    event(
        &v,
        cx,
        json!({"type":"stage","runId":run_a,"stage":{"index":0,"label":"Thinking…","status":"done"}}),
    );
    event(
        &v,
        cx,
        json!({"type":"stage","runId":run_a,"stage":{"index":1,"label":"Run command","toolName":"shell","status":"done","output":"ok","durationMs":1200}}),
    );
    let paragraph =
        "The fixture answer wraps across several lines so its height must follow the text. ";
    event(
        &v,
        cx,
        json!({"type":"message","runId":run_a,"delta":format!("{}\n\nSecond paragraph.", paragraph.repeat(6))}),
    );
    settle(&rt, cx);
    // Streamed text is published on the next 16 ms frame.
    cx.executor().advance_clock(Duration::from_millis(32));
    settle(&rt, cx);
    let messages = v.read_with(cx, |v, _| {
        v.projection
            .iter()
            .filter_map(|r| match r {
                view::Row::Text { id, turn, .. } => Some((
                    &*Box::leak(format!("chat-message-{id}").into_boxed_str()),
                    turn.role == chat::Role::User,
                )),
                _ => None,
            })
            .collect::<Vec<_>>()
    });
    let user = messages.iter().find(|(_, user)| *user).unwrap().0;
    let assistant = messages.iter().find(|(_, user)| !*user).unwrap().0;
    let handle = cx.update(|w, _| w.window_handle());
    for (width, height) in [(467., 900.), (900., 900.)] {
        cx.simulate_window_resize(handle, size(px(width), px(height)));
        cx.run_until_parked();
        let viewport = cx.update(|w, _| w.viewport_size());
        let user = cx.debug_bounds(user).expect("User message is visible");
        let assistant = cx
            .debug_bounds(assistant)
            .expect("Assistant message is visible");
        assert!(
            user.size.height < px(60.),
            "A one-line user bubble hugs its text: {user:?}"
        );
        assert!(
            user.size.width < viewport.width * 0.6,
            "A short user bubble is content-width: {user:?}"
        );
        assert!(
            viewport.width - user.right() < px(30.),
            "User bubbles are right-aligned: {user:?}, {viewport:?}"
        );
        assert!(
            assistant.size.height > user.size.height * 2.,
            "Wrapped assistant prose grows with its content: {assistant:?}"
        );
        assert!(
            assistant.size.height < px(400.),
            "Assistant prose is not padded to an estimated height: {assistant:?}"
        );
        assert!(user.bottom() <= assistant.top(), "Rows never overlap");
        let composer = cx.debug_bounds("chat-composer").unwrap();
        let send = cx.debug_bounds("chat-send").unwrap();
        assert!(
            send.right() <= composer.right() && send.left() >= composer.left(),
            "The submit button stays inside the prompt input: {send:?}, {composer:?}"
        );
    }
}
#[gpui::test]
fn stop_during_running_multiline_tool_ignores_late_events(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "A");
    settle(&rt, cx);
    edit(
        &v,
        cx,
        "how can I clean storage of this pc give me full info about",
    );
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    let r = run(&v, cx);
    event(
        &v,
        cx,
        json!({"type":"stage","runId":r,"stage":{"index":0,"label":"Thinking","toolName":"thinking","status":"running"}}),
    );
    event(
        &v,
        cx,
        json!({"type":"stage","runId":r,"stage":{"index":0,"label":"Thinking","toolName":"thinking","status":"done","durationMs":900}}),
    );
    event(
        &v,
        cx,
        json!({"type":"stage","runId":r,"stage":{"index":1,"label":"powershell: Get-PSDrive -PSProvider FileSystem | Select-Object Name, Des","toolName":"powershell","status":"running"}}),
    );
    cx.run_until_parked();
    event(
        &v,
        cx,
        json!({"type":"stage","runId":r,"stage":{"index":1,"label":"powershell: Get-PSDrive -PSProvider FileSystem | Select-Object Name, Des","toolName":"powershell","status":"done","output":"Exit status: 0\nstdout:\n\r\nName Description Free(GB)\r\n---- -----------\r\nC   115.56\r\n\r\n","durationMs":4525}}),
    );
    event(
        &v,
        cx,
        json!({"type":"stage","runId":r,"stage":{"index":2,"label":"powershell: $paths = @(\n    \"$env:LOCALAPPDATA/Temp\",\n    \"$env:WINDIR/T","toolName":"powershell","status":"running"}}),
    );
    cx.run_until_parked();
    action(&v, cx, Action::Expand("assistant-2".into()));
    cx.run_until_parked();
    action(&v, cx, Action::Stop);
    settle(&rt, cx);
    cx.run_until_parked();
    event(
        &v,
        cx,
        json!({"type":"stage","runId":r,"stage":{"index":2,"label":"powershell: x","toolName":"powershell","status":"failed","output":"Operation cancelled"}}),
    );
    event(
        &v,
        cx,
        json!({"type":"error","runId":r,"message":"Operation cancelled"}),
    );
    settle(&rt, cx);
    cx.run_until_parked();
    let id = v.read_with(cx, |v, _| {
        v.active().unwrap().turns.back().unwrap().id.clone()
    });
    for n in 0..3 {
        action(&v, cx, Action::Expand(format!("{id}-{n}")));
        cx.run_until_parked();
    }
}
/// GPUI runs the transcript scroll handler while it holds the list state mutably
/// borrowed. Re-reading the list there panicked inside the platform's wheel callback,
/// which aborts the app (Windows 0xc0000409 / macOS "quit unexpectedly").
#[gpui::test]
fn wheel_scrolling_a_populated_transcript_does_not_reborrow_the_list(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Fixture::new();
    let (v, cx) = mount(cx, &f);
    new_chat(&v, cx, "A");
    settle(&rt, cx);
    edit(&v, cx, "Scroll me");
    action(&v, cx, Action::Send);
    settle(&rt, cx);
    let run_a = run(&v, cx);
    event(
        &v,
        cx,
        json!({"type":"message","runId":run_a,"delta":"Line\n\n".repeat(40)}),
    );
    settle(&rt, cx);
    cx.executor().advance_clock(Duration::from_millis(32));
    settle(&rt, cx);
    let handle = cx.update(|w, _| w.window_handle());
    cx.simulate_window_resize(handle, size(px(467.), px(458.)));
    cx.run_until_parked();
    let composer = cx.debug_bounds("chat-composer").unwrap();
    // Just above the composer is always the transcript.
    let over = point(composer.center().x, composer.top() - px(40.));
    for dy in [-120., 120., -40.] {
        cx.simulate_event(ScrollWheelEvent {
            position: over,
            delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
            ..Default::default()
        });
        cx.run_until_parked();
    }
    // Scrolling back to the bottom resumes following the stream.
    cx.simulate_event(ScrollWheelEvent {
        position: over,
        delta: ScrollDelta::Pixels(point(px(0.), px(-10_000.))),
        ..Default::default()
    });
    cx.run_until_parked();
    assert!(v.read_with(cx, |v, _| v.follow));
}
