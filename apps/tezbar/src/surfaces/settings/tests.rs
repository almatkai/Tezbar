//! Actual native entities and production executors, synthetic transport only.
use super::*;
use ::core::prelude::v1::test;
use gpui::EntityInputHandler;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
#[derive(Default)]
pub(crate) struct Fixture {
    pub config: Mutex<Value>,
    pub calls: Mutex<Vec<(&'static str, Vec<Value>)>>,
    pub fail: AtomicBool,
    pub fail_reads: AtomicUsize,
    pub reads: AtomicUsize,
    pub read_gate: Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
    pub held: Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
    pub(crate) hold_channel: Mutex<Option<&'static str>>,
    pub(crate) fail_channel: Mutex<Option<&'static str>>,
    pub(crate) responses: Mutex<BTreeMap<&'static str, Value>>,
}
impl SettingsTransport for Fixture {
    fn durable_config(&self) -> SettingsFuture {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let failed = self
            .fail_reads
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok();
        let v = self.config.lock().unwrap().clone();
        let gate = self.read_gate.lock().unwrap().take();
        Box::pin(async move {
            if let Some(gate) = gate {
                let _ = gate.await;
            }
            if failed {
                Err("Synthetic unavailable config".into())
            } else {
                Ok(v)
            }
        })
    }
    fn send(&self, request: SettingsRequest) -> SettingsFuture {
        let (channel, args) = request.wire();
        self.calls.lock().unwrap().push((channel, args.clone()));
        let value = if let Some(value) = self.responses.lock().unwrap().get(channel).cloned() {
            Ok(value)
        } else {
            match channel {
                "llm-config-get" => Ok(self.config.lock().unwrap().clone()),
                "llm-config-set" => {
                    if self.fail.load(Ordering::SeqCst) {
                        Err("fixture-key-must-not-leak".into())
                    } else {
                        let mut c = self.config.lock().unwrap();
                        if !c.is_object() {
                            *c = json!({});
                        }
                        for (k, v) in args[0].as_object().unwrap() {
                            c[k] = v.clone();
                        }
                        Ok(Value::Null)
                    }
                }
                "knowledge:snapshot" | "knowledge:root:add" | "knowledge:depth:set" => {
                    Ok(snapshot())
                }
                "ai-extensions:list" => Ok(
                    json!({"extensions":[{"id":"fixture","name":"Fixture","enabled":false,"connections":[{"id":"one","command":"fixture-command","args":["line1","line2"],"cwd":"/fixture","env":{"KEY":{"secret":"opaque-fixture-ref","label":"Key"}},"tested":false}],"prompts":[],"commands":[],"providers":[],"agents":[]}],"globalInstructions":"Canonical"}),
                ),
                "ai-extensions:parse-import" => Ok(
                    json!({"previewId":"synthetic-preview","manifest":{"name":"Fixture","connections":[]}}),
                ),
                "path:complete" => Ok(
                    json!([{"id":"folder","kind":"directory","value":"/fixture/","title":"Fixture folder","subtitle":"/fixture/"}]),
                ),
                "permissions:snapshot" => Ok(json!({"platform":"macos","statuses":[]})),
                "voice:models:list" => Ok(json!([])),
                "voice:model:get-selected" => Ok(json!({"modelId":"fixture-model"})),
                "storage:breakdown" => {
                    Ok(json!({"totalBytes":64,"items":[{"id":"db","label":"Database","bytes":64}]}))
                }
                "storage:clipboard-config:get" => {
                    Ok(json!({"watchEnabled":true,"captureImages":false,"maxImageMegapixels":4}))
                }
                "safety:dry-run:get" => Ok(json!(false)),
                "ai-accounts:antigravity:list"
                | "extension:list"
                | "get-installed-extensions-settings-schema"
                | "llm-provider-statuses"
                | "pi-extensions:list" => Ok(json!([])),
                "get-settings" => {
                    Ok(json!({"commandAliases":{},"commandHotkeys":{},"disabledCommands":{}}))
                }
                "github-device-start" => Ok(
                    json!({"user_code":"SYNTHETIC","verification_uri":"https://example.invalid/device","interval":1}),
                ),
                "ai-extensions:oauth-start" => Ok(json!({"url":"https://fixture.invalid/oauth"})),
                "llm-list-models" => Ok(json!([{"id":"discovered","contextWindow":1234}])),
                _ => Ok(json!({"ok":true})),
            }
        };
        let held_channel = *self.hold_channel.lock().unwrap();
        let gate = if held_channel.map_or(
            matches!(channel, "knowledge:root:add" | "llm-config-set"),
            |held| channel == held,
        ) {
            self.held.lock().unwrap().take()
        } else {
            None
        };
        let fail = *self.fail_channel.lock().unwrap() == Some(channel);
        Box::pin(async move {
            if let Some(gate) = gate {
                let _ = gate.await;
            }
            if fail {
                Err("Synthetic operation failed".into())
            } else {
                value
            }
        })
    }
}
pub(crate) fn snapshot() -> Value {
    json!({"settings":{"depth":"smart","embeddingModel":"built-in","embeddingVariant":null,"indexingSpeed":"medium"},"roots":[],"status":{"state":"idle"},"semanticSearch":{"supported":true,"modelState":"not-downloaded"}})
}
pub(crate) fn runtime(cx: &mut TestAppContext) -> tokio::runtime::Runtime {
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
pub(crate) fn settle(rt: &tokio::runtime::Runtime, cx: &mut VisualTestContext) {
    rt.block_on(async {
        for _ in 0..128 {
            cx.run_until_parked();
            tokio::task::yield_now().await;
        }
    })
}
fn config() -> Value {
    json!({"provider":"ollama","model":"base","providerModels":{"ollama":[{"id":"base"}]},"providerConfigs":{"ollama":{"baseURL":"http://fixture.invalid"},"openai":{"apiKey":"fixture-peer"}},"customProviders":[],"commandAliases":{"peer":"keep"}})
}
fn fixture() -> Arc<Fixture> {
    Arc::new(Fixture {
        config: Mutex::new(config()),
        ..Default::default()
    })
}
#[gpui::test]
fn all_eight_native_tabs_hydrate_without_writes_and_live_navigation(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = fixture();
    let store = SettingsStore::new(f.clone());
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store, &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate(w, cx)));
    settle(&rt, cx);
    for tab in Tab::ALL {
        cx.update(|w, cx| view.update(cx, |v, cx| v.action(Action::Tab(tab), w, cx)));
        settle(&rt, cx);
        assert_eq!(view.read_with(cx, |v, _| v.tab), tab);
    }
    assert!(
        !f.calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| *c == "llm-config-set")
    );
    let intent = Navigation {
        nonce: 91,
        surface: tezbar_ui_model::router::Surface::Settings("knowledge".into()),
        payload: Payload::Settings {
            tab: "knowledge".into(),
        },
    };
    cx.update(|w, cx| view.update(cx, |v, cx| v.navigate(&intent, w, cx)));
    settle(&rt, cx);
    assert_eq!(view.read_with(cx, |v, _| v.tab), Tab::Knowledge);
}
#[gpui::test]
fn actual_text_input_provider_edit_is_scoped_and_survives_initial_read(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = fixture();
    let store = SettingsStore::new(f.clone());
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store, &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.prepare_fields(w, cx);
            v.provider.load("ollama".into(), &config());
            v.provider_fields(w, cx);
        })
    });
    let input = view.read_with(cx, |v, _| v.fields["provider.ollama.baseURL"].input.clone());
    cx.update(|w, cx| {
        input.update(cx, |v, cx| {
            v.replace_text_in_range(Some(0..v.content.len()), "http://new.invalid", w, cx)
        })
    });
    settle(&rt, cx);
    cx.update(|w, cx| view.update(cx, |v, cx| v.hydrate(config(), w, cx)));
    settle(&rt, cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.value("provider.ollama.baseURL")),
        "http://new.invalid"
    );
    assert_eq!(
        f.config.lock().unwrap()["providerConfigs"]["openai"]["apiKey"],
        "fixture-peer"
    );
    let saves = f.calls.lock().unwrap();
    let patch = &saves
        .iter()
        .find(|(c, _)| *c == "llm-config-set")
        .unwrap()
        .1[0];
    assert!(patch.get("apiKey").is_none());
}
#[gpui::test]
fn close_failure_retry_uses_real_store_and_keeps_secrets_out_of_errors(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = fixture();
    f.fail.store(true, Ordering::SeqCst);
    let store = SettingsStore::new(f.clone());
    let barrier = store.clone();
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store, &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.action(Action::Toggle("memoryEnabled", false), w, cx)
        })
    });
    settle(&rt, cx);
    assert!(rt.block_on(barrier.drain()).is_err());
    assert!(!view.read_with(cx, |v, _| v.error.as_ref().unwrap().contains("fixture-key")));
    f.fail.store(false, Ordering::SeqCst);
    cx.update(|w, cx| view.update(cx, |v, cx| v.action(Action::Retry, w, cx)));
    settle(&rt, cx);
    assert!(rt.block_on(barrier.drain()).is_ok());
    assert_eq!(f.config.lock().unwrap()["memoryEnabled"], false);
}
#[gpui::test]
fn knowledge_mutation_completion_belongs_to_retained_tab(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = fixture();
    let (tx, rx) = tokio::sync::oneshot::channel();
    *f.held.lock().unwrap() = Some(rx);
    let store = SettingsStore::new(f.clone());
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store, &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.activate(w, cx);
            v.set_tab(Tab::Knowledge, w, cx)
        })
    });
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.action(
                Action::Knowledge(knowledge::Operation::AddRoot("/fixture".into())),
                w,
                cx,
            )
        })
    });
    settle(&rt, cx);
    assert!(matches!(
        view.read_with(cx, |v, _| v.knowledge.phase.clone()),
        knowledge::Phase::Pending { .. }
    ));
    cx.update(|w, cx| view.update(cx, |v, cx| v.action(Action::Tab(Tab::General), w, cx)));
    tx.send(()).unwrap();
    settle(&rt, cx);
    assert_eq!(view.read_with(cx, |v, _| v.tab), Tab::General);
    assert!(view.read_with(cx, |v, _| v.knowledge.snapshot.is_some()));
    assert_eq!(
        view.read_with(cx, |v, _| v.knowledge.phase.clone()),
        knowledge::Phase::Idle
    );
}
#[gpui::test]
fn ai_extension_import_review_and_command_edit_preserve_secret_refs(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = fixture();
    let store = SettingsStore::new(f.clone());
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store, &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.activate(w, cx);
            v.set_tab(Tab::Ai, w, cx)
        })
    });
    settle(&rt, cx);
    assert!(!view.read_with(cx, |v, _| {
        v.contributions
            .display
            .to_string()
            .contains("opaque-fixture-ref")
    }));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.fields.get_mut("connection.fixture.0.args").unwrap().value = "one\n\nthree\n".into();
            v.action(
                Action::Contribution(ContributionIntent::SaveRow(
                    "fixture".into(),
                    "connections".into(),
                    0,
                    "args".into(),
                )),
                w,
                cx,
            )
        })
    });
    settle(&rt, cx);
    let calls = f.calls.lock().unwrap();
    let args = &calls
        .iter()
        .find(|(c, _)| *c == "ai-extensions:update")
        .unwrap()
        .1;
    assert_eq!(
        args[1]["connections"][0]["args"],
        json!(["one", "", "three", ""])
    );
    assert_eq!(
        args[1]["connections"][0]["env"]["KEY"]["secret"],
        "opaque-fixture-ref"
    );
    assert!(!calls.iter().any(|(c, _)| *c == "ai-extensions:test"));
}
#[gpui::test]
fn copilot_signin_save_failure_is_an_effect_barrier(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = fixture();
    f.fail.store(true, Ordering::SeqCst);
    let store = SettingsStore::new(f.clone());
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store, &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.store.enqueue(
                v.owner,
                vec![Edit::Field {
                    provider: "copilot".into(),
                    field: "githubOAuthClientId".into(),
                    value: json!("synthetic-client"),
                }],
            );
            v.action(Action::SignIn, w, cx)
        })
    });
    settle(&rt, cx);
    assert!(
        !f.calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| *c == "github-device-start")
    );
    assert!(view.read_with(cx, |v, _| v.error.is_some()));
}
#[tokio::test]
async fn cancelled_native_waiter_does_not_cancel_owned_write() {
    let f = fixture();
    let store = SettingsStore::new(f.clone());
    let owner = store.owner();
    store.enqueue(
        owner,
        vec![Edit::Global {
            field: "memoryMaxItems".into(),
            value: json!(7),
        }],
    );
    let copy = store.clone();
    let waiter = tokio::spawn(async move { copy.drain().await });
    tokio::task::yield_now().await;
    waiter.abort();
    assert!(store.drain().await.is_ok());
    assert_eq!(f.config.lock().unwrap()["memoryMaxItems"], 7);
}
#[tokio::test]
async fn settings_and_onboarding_share_fresh_canonical_provider_ownership() {
    let f = fixture();
    let store = SettingsStore::new(f.clone());
    let a = store.owner();
    let b = store.owner();
    store.enqueue(
        a,
        vec![Edit::Field {
            provider: "openai".into(),
            field: "baseURL".into(),
            value: json!("https://fixture.invalid"),
        }],
    );
    store.enqueue(
        b,
        vec![Edit::Global {
            field: "hasCompletedOnboarding".into(),
            value: json!(true),
        }],
    );
    store.drain().await.unwrap();
    let c = f.config.lock().unwrap();
    assert_eq!(c["providerConfigs"]["openai"]["apiKey"], "fixture-peer");
    assert_eq!(
        c["providerConfigs"]["openai"]["baseURL"],
        "https://fixture.invalid"
    );
    assert_eq!(c["hasCompletedOnboarding"], true);
}
#[tokio::test]
async fn invalid_draft_blocks_close_until_corrected() {
    let f = fixture();
    let store = SettingsStore::new(f);
    let owner = store.owner();
    store.validation(owner, "headers", Some("Invalid headers"));
    assert!(store.drain().await.is_err());
    store.validation(owner, "headers", None);
    assert!(store.drain().await.is_ok());
}
#[::core::prelude::v1::test]
fn finite_request_diagnostics_redact_all_payloads() {
    let r = SettingsRequest::Save(json!({"apiKey":"synthetic-secret"}));
    assert!(!format!("{r:?}").contains("synthetic-secret"));
    assert_eq!(
        SettingsRequest::Knowledge(knowledge::Operation::Cancel(knowledge::Variant::Compact))
            .wire(),
        ("knowledge:embedding:cancel-download", vec![json!("q4_k_m")])
    )
}

#[gpui::test]
fn native_tab_keyboard_roles_and_password_control_use_shared_semantics(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = fixture();
    let store = SettingsStore::new(f);
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store, &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.activate(w, cx);
            w.focus(&v.tabs[0], cx);
        })
    });
    settle(&rt, cx);
    let keystroke = Keystroke::parse("right").unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
    settle(&rt, cx);
    assert_eq!(view.read_with(cx, |v, _| v.tab), Tab::Ai);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.action(Action::Provider("openai".into()), w, cx)
        })
    });
    settle(&rt, cx);
    // The pinned TestWindow does not implement PlatformWindow::a11y_init. Test the
    // real production elements directly; a full AccessKit tree remains a platform gate.
    assert!(cx.update(|w, _| w.debug_a11y_tree_json()).is_none());
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let t = tezbar_ui::tokens::tokens(w);
            for (i, tab) in Tab::ALL.into_iter().enumerate() {
                assert_eq!(v.tab_control(i, tab, t, cx).a11y_role(), Some(Role::Tab));
            }
            let input = v.fields["provider.openai.apiKey"].input.clone();
            assert!(input.read(cx).options.password);
            assert_eq!(input.read(cx).options.name.as_ref(), "API key");
        })
    });
}
#[tokio::test]
async fn alias_failure_followed_by_peer_alias_preserves_both_intents() {
    let f = fixture();
    let store = SettingsStore::new(f.clone());
    let owner = store.owner();
    f.fail.store(true, Ordering::SeqCst);
    store.alias(owner, "a".into(), "first".into());
    assert!(store.drain().await.is_err());
    f.fail.store(false, Ordering::SeqCst);
    store.alias(owner, "b".into(), "second".into());
    assert!(store.drain().await.is_ok());
    let c = f.config.lock().unwrap();
    assert_eq!(c["commandAliases"]["a"], "first");
    assert_eq!(c["commandAliases"]["b"], "second");
    assert_eq!(c["commandAliases"]["peer"], "keep");
}
#[gpui::test]
fn settings_window_acknowledges_before_failed_drain_and_keeps_same_content(
    cx: &mut TestAppContext,
) {
    use crate::events::{Event, Relay};
    let rt = runtime(cx);
    let f = fixture();
    f.fail.store(true, Ordering::SeqCst);
    let store = SettingsStore::new(f.clone());
    let owner = store.owner();
    store.enqueue(
        owner,
        vec![Edit::Global {
            field: "thinking".into(),
            value: json!(true),
        }],
    );
    let (relay, mut rx) = Relay::new();
    let (shell, cx) = cx.add_window_view(|w, cx| {
        let mut shell = crate::windows::settings::Settings::new(relay, w, cx);
        let content = cx.new(|cx| SettingsContent::new(store, &UiState::default(), w, cx));
        shell.set_content(content, cx);
        shell
    });
    shell.update(cx, |v, cx| v.begin_close(7, cx));
    assert!(matches!(
        rt.block_on(rx.recv()),
        Some(Event::SettingsAck(7))
    ));
    settle(&rt, cx);
    assert!(matches!(
        rt.block_on(rx.recv()),
        Some(Event::SettingsFinish(7, Err(_)))
    ));
    assert!(shell.read_with(cx, |v, _| v.content.is_some()));
    shell.update(cx, |v, cx| {
        v.close_failed(7, "Current failed save".into(), cx);
        assert_eq!(v.error.as_deref(), Some("Current failed save"));
        v.begin_close(8, cx);
        v.error = None;
        v.close_failed(7, "Stale failure".into(), cx);
        assert!(
            v.error.is_none(),
            "Old completion cannot replace retry feedback"
        );
    });
    let reopened = cx.update(|w, cx| {
        cx.new(|cx| {
            let (relay, _) = Relay::new();
            crate::windows::settings::Settings::new(relay, w, cx)
        })
    });
    reopened.update(cx, |v, cx| {
        v.close_failed(7, "Previous window failure".into(), cx);
        assert!(
            v.error.is_none(),
            "A new window does not own the old completion"
        );
    });
}

#[gpui::test]
fn signin_clicked_during_a_slow_save_waits_once_and_freezes_its_provider(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = fixture();
    let (tx, rx) = tokio::sync::oneshot::channel();
    *f.held.lock().unwrap() = Some(rx);
    let store = SettingsStore::new(f.clone());
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store, &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.provider.load("copilot".into(), &config());
            let _ = v
                .provider
                .edit("githubOAuthClientId".into(), json!("frozen-client"));
            v.queue(
                vec![Edit::Field {
                    provider: "copilot".into(),
                    field: "githubOAuthClientId".into(),
                    value: json!("frozen-client"),
                }],
                cx,
            );
            v.action(Action::SignIn, w, cx);
            v.action(Action::SignIn, w, cx);
            v.action(Action::Provider("openai".into()), w, cx);
        })
    });
    settle(&rt, cx);
    assert!(
        !f.calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| *c == "github-device-start")
    );
    tx.send(()).unwrap();
    settle(&rt, cx);
    assert_eq!(
        f.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| *c == "github-device-start")
            .count(),
        1
    );
}

#[tokio::test]
async fn production_queue_retries_do_not_resurrect_removed_provider_or_lose_recreation() {
    let f = fixture();
    f.config.lock().unwrap()["customProviders"] = json!([{"id":"custom:a","title":"A"}]);
    let store = SettingsStore::new(f.clone());
    let owner = store.owner();
    f.fail.store(true, Ordering::SeqCst);
    store.enqueue(
        owner,
        vec![Edit::Field {
            provider: "custom:a".into(),
            field: "apiKey".into(),
            value: json!("old-key"),
        }],
    );
    assert!(store.drain().await.is_err());
    f.fail.store(false, Ordering::SeqCst);
    store.enqueue(owner, vec![Edit::RemoveProvider("custom:a".into())]);
    store.drain().await.unwrap();
    store.retry();
    store.drain().await.unwrap();
    assert!(
        f.config.lock().unwrap()["providerConfigs"]
            .get("custom:a")
            .is_none()
    );
    store.enqueue(
        owner,
        vec![
            Edit::CreateProvider {
                provider: "custom:a".into(),
                value: json!({"id":"custom:a","title":"Recreated"}),
            },
            Edit::Field {
                provider: "custom:a".into(),
                field: "apiKey".into(),
                value: json!("new-key"),
            },
        ],
    );
    store.drain().await.unwrap();
    assert_eq!(
        f.config.lock().unwrap()["providerConfigs"]["custom:a"]["apiKey"],
        "new-key"
    );
}
#[tokio::test]
async fn production_discovery_uses_fresh_models_and_keeps_peer_selection() {
    let f = fixture();
    let store = SettingsStore::new(f.clone());
    let owner = store.owner();
    let mut draft = ProviderDraft::default();
    draft.load("ollama".into(), &config());
    let edit = draft
        .discovered(draft.discovery_token(), vec!["discovered".into()])
        .unwrap();
    {
        let mut c = f.config.lock().unwrap();
        c["providerModels"]["ollama"] =
            json!([{"id":"peer","protocol":"ollama","maxTokens":"1024","capabilities":[]}]);
        c["providerSelectedModels"] = json!({"ollama":"peer"});
    }
    store.discover(owner, edit);
    store.drain().await.unwrap();
    let c = f.config.lock().unwrap();
    assert_eq!(c["providerSelectedModels"]["ollama"], "peer");
    assert!(
        c["providerModels"]["ollama"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == "peer")
    );
    assert!(
        !c["providerModels"]["ollama"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == "base")
    );
}

#[tokio::test]
async fn cancellation_ticket_is_not_held_behind_an_unrelated_hung_operation() {
    let f = fixture();
    let (tx, rx) = tokio::sync::oneshot::channel();
    *f.held.lock().unwrap() = Some(rx);
    let store = SettingsStore::new(f.clone());
    let start = store
        .admit(SettingsRequest::Knowledge(knowledge::Operation::AddRoot(
            "/fixture".into(),
        )))
        .unwrap();
    let copy = store.clone();
    let waiter = tokio::spawn(async move { copy.operation_result(start).await });
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    assert!(
        f.calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| *c == "knowledge:root:add")
    );
    let cancel = store
        .admit(SettingsRequest::Knowledge(knowledge::Operation::Cancel(
            knowledge::Variant::Compact,
        )))
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), store.operation_result(cancel))
            .await
            .unwrap()
            .is_ok()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(20), store.drain())
            .await
            .is_err()
    );
    tx.send(()).unwrap();
    assert!(waiter.await.unwrap().is_ok());
    store.drain().await.unwrap();
}
#[gpui::test]
fn native_saved_secret_uses_only_opaque_scoped_slot_and_clears_password(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = fixture();
    let store = SettingsStore::new(f.clone());
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store, &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.activate(w, cx);
            v.set_tab(Tab::Ai, w, cx);
        })
    });
    settle(&rt, cx);
    let input = view.read_with(cx, |v, _| v.fields["secret.slot.fixture.0"].input.clone());
    cx.update(|w, cx| {
        input.update(cx, |v, cx| {
            v.replace_text_in_range(None, "new-fixture-value", w, cx)
        })
    });
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.action(
                Action::Contribution(ContributionIntent::SaveSavedSecret("fixture".into(), 0)),
                w,
                cx,
            )
        })
    });
    settle(&rt, cx);
    let calls = f.calls.lock().unwrap();
    let args = &calls
        .iter()
        .find(|(c, _)| *c == "ai-extensions:set-secret")
        .unwrap()
        .1;
    assert_eq!(
        args,
        &vec![
            json!("fixture"),
            json!("opaque-fixture-ref"),
            json!("new-fixture-value"),
            json!("one")
        ]
    );
    assert!(
        view.read_with(cx, |v, _| v.value("secret.slot.fixture.0"))
            .is_empty()
    );
    assert!(
        !view
            .read_with(cx, |v, _| v.contributions.display.to_string())
            .contains("new-fixture-value")
    );
}
#[tokio::test]
async fn task_override_uses_typed_fresh_map_and_preserves_peer_task() {
    let f = fixture();
    f.config.lock().unwrap()["taskProviderOverrides"] = json!({"search":"ollama"});
    let store = SettingsStore::new(f.clone());
    let owner = store.owner();
    store.enqueue(
        owner,
        vec![Edit::TaskSelection {
            task: "chat".into(),
            provider: "openai".into(),
            model: "synthetic".into(),
        }],
    );
    store.drain().await.unwrap();
    assert_eq!(
        f.config.lock().unwrap()["taskProviderOverrides"]["search"],
        "ollama"
    );
    assert_eq!(
        f.config.lock().unwrap()["taskModelOverrides"]["chat"],
        "synthetic"
    );
    store.enqueue(
        owner,
        vec![Edit::TaskSelection {
            task: "chat".into(),
            provider: String::new(),
            model: String::new(),
        }],
    );
    store.drain().await.unwrap();
    assert!(
        f.config.lock().unwrap()["taskProviderOverrides"]
            .get("chat")
            .is_none()
    );
    assert_eq!(
        SettingsRequest::Voice(VoiceAction::CancelDownload("fixture-model".into())).wire(),
        (
            "voice:model:cancel-download",
            vec![json!({"modelId":"fixture-model"})]
        )
    );
}

#[gpui::test]
fn project_review_handoff_coalesces_ui_nonce_and_uses_core_review_identity(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let f = fixture();
    f.responses.lock().unwrap().insert("ai-extensions:list", json!({"extensions":[],"project":{"folder":"/fixture","entryHash":"reviewed-hash"},"projectReviewId":"core-review-nonce"}));
    let store = SettingsStore::new(f.clone());
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store, &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.review_project("/fixture".into(), "ui-delivery-nonce".into(), w, cx);
            v.review_project("/fixture".into(), "ui-delivery-nonce".into(), w, cx);
        })
    });
    settle(&rt, cx);
    assert_eq!(view.read_with(cx, |v, _| v.tab), Tab::Ai);
    assert_eq!(
        f.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| *c == "ai-extensions:review-project")
            .count(),
        1
    );
    assert!(!f.calls.lock().unwrap().iter().any(|(c, _)| matches!(
        *c,
        "ai-extensions:test" | "ai-extensions:set-enabled" | "ai-extensions:trust-project"
    )));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.action(
                Action::Contribution(ContributionIntent::TrustProject),
                w,
                cx,
            )
        })
    });
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.action(
                Action::Contribution(ContributionIntent::DismissProject),
                w,
                cx,
            )
        })
    });
    settle(&rt, cx);
    let calls = f.calls.lock().unwrap();
    assert!(
        calls
            .iter()
            .any(|(c, a)| *c == "ai-extensions:trust-project"
                && *a
                    == vec![
                        json!("/fixture"),
                        json!("reviewed-hash"),
                        json!("core-review-nonce")
                    ])
    );
    assert!(
        calls
            .iter()
            .any(|(c, a)| *c == "ai-extensions:dismiss-project-review"
                && *a == vec![json!("core-review-nonce")])
    );
}
#[gpui::test]
fn cancelled_oauth_start_cannot_open_a_late_browser(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = fixture();
    let (tx, rx) = tokio::sync::oneshot::channel();
    *f.held.lock().unwrap() = Some(rx);
    *f.hold_channel.lock().unwrap() = Some("ai-extensions:oauth-start");
    let store = SettingsStore::new(f.clone());
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store, &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.activate_tab(Tab::Ai, w, cx);
            v.action(
                Action::Contribution(ContributionIntent::Operation(
                    "fixture".into(),
                    ContributionAction::OAuthStart,
                    vec![json!("fixture"), json!("one")],
                )),
                w,
                cx,
            );
        })
    });
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.action(
                Action::Contribution(ContributionIntent::Operation(
                    "fixture".into(),
                    ContributionAction::OAuthCancel,
                    vec![json!("fixture"), json!("one")],
                )),
                w,
                cx,
            )
        })
    });
    settle(&rt, cx);
    tx.send(()).unwrap();
    settle(&rt, cx);
    assert!(
        !f.calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| *c == "open-external-url")
    );
}
#[gpui::test]
fn native_voice_selects_uninstalled_models_and_keeps_downloads_independent(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let f = fixture();
    f.responses.lock().unwrap().insert("voice:models:list",json!([
        {"id":"not-installed","name":"Not installed","status":"not-downloaded","downloadedBytes":0,"runtime":{"ready":false,"label":"Whisper","installCommand":"fixture-install"}},
        {"id":"partial","name":"Partial","status":"error","downloadedBytes":12,"runtime":{"ready":true}},
        {"id":"downloading","name":"Downloading","status":"downloading","stage":"downloading-weights","progress":0.25,"runtime":{"ready":true}}
    ]));
    f.responses.lock().unwrap().insert(
        "voice:model:get-selected",
        json!({"modelId":"not-installed"}),
    );
    f.responses
        .lock()
        .unwrap()
        .insert("voice:stt:modes", json!(["local-model-assets"]));
    let store = SettingsStore::new(f.clone());
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store, &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate_tab(Tab::Voice, w, cx)));
    settle(&rt, cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.selected_voice.clone()),
        "not-installed"
    );
    assert_eq!(
        view.read_with(cx, |v, _| v.voice_modes.clone()),
        ["local-model-assets"]
    );
    let button = cx
        .debug_bounds("settings-Use Not installed")
        .unwrap()
        .center();
    cx.simulate_click(button, Modifiers::default());
    settle(&rt, cx);
    let button = cx.debug_bounds("settings-Delete Partial").unwrap().center();
    cx.simulate_click(button, Modifiers::default());
    settle(&rt, cx);
    let button = cx
        .debug_bounds("settings-Cancel Downloading download")
        .unwrap()
        .center();
    cx.simulate_click(button, Modifiers::default());
    settle(&rt, cx);
    let calls = f.calls.lock().unwrap();
    for (channel, id) in [
        ("voice:model:set-selected", "not-installed"),
        ("voice:model:delete", "partial"),
        ("voice:model:cancel-download", "downloading"),
    ] {
        assert_eq!(
            calls
                .iter()
                .filter(|(c, a)| *c == channel && a == &vec![json!({"modelId":id})])
                .count(),
            1
        );
    }
    assert!(!calls.iter().any(|(c, _)| *c == "voice:model:download"));
}
#[tokio::test]
async fn settled_picker_owner_does_not_wait_for_an_unrelated_held_download() {
    let f = fixture();
    let (tx, rx) = tokio::sync::oneshot::channel();
    *f.held.lock().unwrap() = Some(rx);
    *f.hold_channel.lock().unwrap() = Some("voice:model:download");
    let store = SettingsStore::new(f.clone());
    let id = store
        .admit(SettingsRequest::Voice(VoiceAction::Download("peer".into())))
        .unwrap();
    let copy = store.clone();
    let download = tokio::spawn(async move { copy.operation_result(id).await });
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
    let owner = store.owner();
    store.enqueue(
        owner,
        vec![
            Edit::Select {
                provider: "ollama".into(),
                model: "new".into(),
            },
            Edit::Activate {
                provider: "ollama".into(),
                model: "new".into(),
            },
        ],
    );
    tokio::time::timeout(Duration::from_millis(100), store.drain_owner(owner))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(f.config.lock().unwrap()["model"], "new");
    assert!(!download.is_finished());
    tx.send(()).unwrap();
    download.await.unwrap().unwrap();
    store.drain().await.unwrap();
}
#[tokio::test]
async fn failed_setter_retry_cannot_replace_a_newer_successful_value_or_replay_auth() {
    let f = fixture();
    let store = SettingsStore::new(f.clone());
    *f.fail_channel.lock().unwrap() = Some("storage:clipboard-config:set");
    let id = store
        .admit(SettingsRequest::ClipboardSet(json!({"watchEnabled":false})))
        .unwrap();
    assert!(store.operation_result(id).await.is_err());
    // Operational failures are reported to this ticket, not retained as an
    // undismissable Settings-close failure (Opus N-2).
    store.drain().await.unwrap();
    store.retry();
    store.drain().await.unwrap();
    assert_eq!(
        f.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| *c == "storage:clipboard-config:set")
            .count(),
        1
    );
    *f.fail_channel.lock().unwrap() = None;
    let id = store
        .admit(SettingsRequest::ClipboardSet(json!({"watchEnabled":true})))
        .unwrap();
    store.operation_result(id).await.unwrap();
    *f.fail_channel.lock().unwrap() = Some("ai-extensions:test");
    let id = store
        .admit(SettingsRequest::Contribution {
            action: ContributionAction::Test,
            args: vec![json!("fixture")],
        })
        .unwrap();
    assert!(store.operation_result(id).await.is_err());
    store.retry();
    store.drain().await.unwrap();
    let calls = f.calls.lock().unwrap();
    let saves = calls
        .iter()
        .filter(|(c, _)| *c == "storage:clipboard-config:set")
        .collect::<Vec<_>>();
    assert_eq!(saves.len(), 2);
    assert_eq!(saves[1].1, vec![json!({"watchEnabled":true})]);
    assert_eq!(
        calls
            .iter()
            .filter(|(c, _)| *c == "ai-extensions:test")
            .count(),
        1
    );
}
#[tokio::test]
async fn close_drain_rechecks_config_edits_admitted_during_parent_persistence() {
    let f = fixture();
    let store = SettingsStore::new(f.clone());
    let owner = store.owner();
    let barrier = EffectBarrier::default();
    let ticket = barrier.begin("ui-state");
    let drain = store.drain_with(barrier.clone());
    let task = tokio::spawn(drain);
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
    store.enqueue(
        owner,
        vec![Edit::Global {
            field: "memoryMaxItems".into(),
            value: json!(23),
        }],
    );
    barrier.finish("ui-state", ticket, true);
    task.await.unwrap().unwrap();
    assert_eq!(f.config.lock().unwrap()["memoryMaxItems"], 23);
    assert!(!store.pending());
}

// Regression cases reproduced by the independent 290e414 review.
#[gpui::test]
fn review_failed_operational_delete_is_retryable_but_never_dischargeable(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = fixture();
    // A real core delete that keeps failing (file in use / permission denied).
    *f.fail_channel.lock().unwrap() = Some("voice:model:delete");
    let store = SettingsStore::new(f.clone());
    let barrier = EffectBarrier::default();
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate(w, cx)));
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.action(
                Action::Voice(VoiceAction::Delete("fixture-model".into())),
                w,
                cx,
            )
        })
    });
    settle(&rt, cx);
    let first = rt.block_on(store.drain_with(barrier.clone()));
    for _ in 0..3 {
        cx.update(|w, cx| view.update(cx, |v, cx| v.action(Action::Retry, w, cx)));
        settle(&rt, cx);
    }
    let after_retries = rt.block_on(store.drain_with(barrier.clone()));
    let deletes = f
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == "voice:model:delete")
        .count();
    eprintln!(
        "OPUS close barrier: first={first:?} after 3 retries={after_retries:?} deletes={deletes} failures={}",
        store.failures()
    );
    assert_eq!(
        deletes, 1,
        "Retry saved changes must not repeat an operational deletion"
    );
    assert!(first.is_ok());
    assert!(
        after_retries.is_ok(),
        "a failed operational mutation keeps the all-owner close/quit barrier failing forever"
    );
}
#[gpui::test]
fn review_unrelated_failed_operation_blocks_copilot_signin(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = fixture();
    *f.fail_channel.lock().unwrap() = Some("voice:model:delete");
    let store = SettingsStore::new(f.clone());
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate(w, cx)));
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.action(
                Action::Voice(VoiceAction::Delete("fixture-model".into())),
                w,
                cx,
            )
        })
    });
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let config = v.config.clone();
            v.provider.load("copilot".into(), &config);
            v.action(Action::SignIn, w, cx)
        })
    });
    settle(&rt, cx);
    let starts = f
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == "github-device-start")
        .count();
    eprintln!(
        "OPUS sign-in starts={starts} error={:?} message={:?}",
        view.read_with(cx, |v, _| v.error.clone()),
        view.read_with(cx, |v, _| v.message.clone())
    );
    assert_eq!(
        starts, 1,
        "an unrelated failed Voice delete blocked Copilot sign-in"
    );
}
#[gpui::test]
fn review_successful_toggle_reports_failure_after_unrelated_operation_failed(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let f = fixture();
    *f.fail_channel.lock().unwrap() = Some("voice:model:delete");
    let store = SettingsStore::new(f.clone());
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate(w, cx)));
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.action(
                Action::Voice(VoiceAction::Delete("fixture-model".into())),
                w,
                cx,
            )
        })
    });
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.error = None;
            v.action(Action::Toggle("memoryEnabled", false), w, cx)
        })
    });
    settle(&rt, cx);
    let saved = f.config.lock().unwrap()["memoryEnabled"].clone();
    let error = view.read_with(cx, |v, _| v.error.clone());
    eprintln!("OPUS toggle saved={saved} error={error:?}");
    assert_eq!(saved, false);
    assert!(
        error.is_none(),
        "durable toggle saved, yet the view reports {error:?}"
    );
}
#[test]
fn review_plain_f_key_is_not_a_global_shortcut() {
    let k = gpui::Keystroke::parse("f").unwrap();
    let got = view::accelerator(&k);
    eprintln!("OPUS accelerator(f)={got:?}");
    assert_eq!(got, None, "TS accepts modifier-less F1..F24 only");
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn review_owner_drain_cannot_strand_behind_an_exiting_pump() {
    let f = fixture();
    let store = SettingsStore::new(f.clone());
    let owner = store.owner();
    let mut stranded = 0;
    for i in 0..20_000u32 {
        store.enqueue(
            owner,
            vec![Edit::Global {
                field: "memoryMaxItems".into(),
                value: json!(i),
            }],
        );
        if tokio::time::timeout(Duration::from_millis(500), store.drain_owner(owner))
            .await
            .is_err()
        {
            stranded += 1;
            // Recover the probe so later iterations still run.
            let _ = store.drain().await;
        }
    }
    eprintln!("OPUS stranded drain_owner waits: {stranded}/20000");
    assert_eq!(stranded, 0);
}
#[gpui::test]
fn review_repeated_failed_operations_exhaust_admission(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = fixture();
    *f.fail_channel.lock().unwrap() = Some("voice:model:delete");
    let store = SettingsStore::new(f.clone());
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate(w, cx)));
    settle(&rt, cx);
    for _ in 0..15 {
        cx.update(|w, cx| {
            view.update(cx, |v, cx| {
                v.action(
                    Action::Voice(VoiceAction::Delete("fixture-model".into())),
                    w,
                    cx,
                )
            })
        });
        settle(&rt, cx);
    }
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.action(
                Action::Voice(VoiceAction::Select("other-model".into())),
                w,
                cx,
            )
        })
    });
    settle(&rt, cx);
    let selects = f
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == "voice:model:set-selected")
        .count();
    eprintln!(
        "OPUS after 15 failed deletes: select calls={selects} error={:?} failures={}",
        view.read_with(cx, |v, _| v.error.clone()),
        store.failures()
    );
    assert_eq!(
        selects, 1,
        "retained failed operations consume the 15-slot admission bound"
    );
}
#[gpui::test]
fn review_tab_switch_hides_unresolved_durable_failure(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = fixture();
    f.fail.store(true, Ordering::SeqCst);
    let store = SettingsStore::new(f.clone());
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate(w, cx)));
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.action(Action::Toggle("memoryEnabled", false), w, cx)
        })
    });
    settle(&rt, cx);
    let before = view.read_with(cx, |v, _| v.error.clone());
    cx.update(|w, cx| view.update(cx, |v, cx| v.action(Action::Tab(Tab::Voice), w, cx)));
    settle(&rt, cx);
    let after = view.read_with(cx, |v, _| v.durable_error().map(str::to_owned));
    eprintln!(
        "OPUS durable failure before tab={before:?} after tab={after:?} store failures={}",
        store.failures()
    );
    assert!(
        after.is_some(),
        "the unresolved durable write failure disappears on any action"
    );
}

#[gpui::test]
fn review_peer_durable_failure_does_not_cancel_own_save_or_signin(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = fixture();
    let store = SettingsStore::new(f.clone());
    let peer = store.owner();
    f.fail.store(true, Ordering::SeqCst);
    store.enqueue(
        peer,
        vec![Edit::Global {
            field: "memoryMaxItems".into(),
            value: json!(77),
        }],
    );
    assert!(rt.block_on(store.drain()).is_err());
    f.fail.store(false, Ordering::SeqCst);
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate(w, cx)));
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.action(Action::Toggle("memoryEnabled", false), w, cx)
        })
    });
    settle(&rt, cx);
    assert_eq!(f.config.lock().unwrap()["memoryEnabled"], false);
    assert!(view.read_with(cx, |v, _| v.error.is_none()));
    assert_eq!(store.failures(), 1);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let config = v.config.clone();
            v.provider.load("copilot".into(), &config);
            v.action(Action::SignIn, w, cx);
        })
    });
    settle(&rt, cx);
    assert_eq!(
        f.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| *c == "github-device-start")
            .count(),
        1
    );
    assert!(
        rt.block_on(store.drain()).is_err(),
        "the peer write is still unsaved"
    );
    store.retry();
    rt.block_on(store.drain()).unwrap();
}
#[gpui::test]
fn review_followup_does_not_wait_for_unrelated_download(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = fixture();
    let store = SettingsStore::new(f.clone());
    let (release, gate) = tokio::sync::oneshot::channel();
    *f.held.lock().unwrap() = Some(gate);
    *f.hold_channel.lock().unwrap() = Some("knowledge:root:add");
    let ticket = store
        .admit(SettingsRequest::Knowledge(knowledge::Operation::AddRoot(
            "/fixture".into(),
        )))
        .unwrap();
    let operation = rt.spawn({
        let store = store.clone();
        async move { store.operation_result(ticket).await }
    });
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate(w, cx)));
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let config = v.config.clone();
            v.provider.load("copilot".into(), &config);
            v.action(Action::SignIn, w, cx);
        })
    });
    settle(&rt, cx);
    assert!(!operation.is_finished());
    assert_eq!(
        f.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| *c == "github-device-start")
            .count(),
        1
    );
    release.send(()).unwrap();
    rt.block_on(operation).unwrap().unwrap();
    rt.block_on(store.drain()).unwrap();
}
#[gpui::test]
fn review_hidden_settings_and_idle_ai_release_the_tick_task(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let store = SettingsStore::new(fixture());
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store, &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate_tab(Tab::Knowledge, w, cx)));
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| v.timer.is_some()));
    view.update(cx, |v, cx| v.set_visible(false, cx));
    assert!(view.read_with(cx, |v, _| v.timer.is_none()));
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate_tab(Tab::Ai, w, cx)));
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| v.timer.is_none()));
}
#[gpui::test]
fn review_choices_reflect_hydrated_values_and_readable_details(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = fixture();
    let store = SettingsStore::new(f);
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store, &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.activate_tab(Tab::Knowledge, w, cx);
            v.update_channel = "beta".into();
            v.state.theme = Theme::Dark;
        })
    });
    settle(&rt, cx);
    view.read_with(cx, |v, _| {
        assert!(v.choice_selected(&Action::Theme(Theme::Dark)));
        assert!(!v.choice_selected(&Action::Theme(Theme::White)));
        assert!(v.choice_selected(&Action::UpdateChannel("beta".into())));
        assert!(
            v.choice_selected(&Action::Knowledge(knowledge::Operation::Depth(
                "smart".into()
            )))
        );
        assert!(
            v.choice_selected(&Action::Knowledge(knowledge::Operation::Speed(
                "medium".into()
            )))
        );
        assert!(
            !v.choice_selected(&Action::Knowledge(knowledge::Operation::Depth(
                "deep".into()
            )))
        );
    });
    let details = view::safe_display(
        &json!({"cwd":"/project", "env":{"NODE_OPTIONS":"--loader ./custom.js"}}),
    );
    assert!(details.contains("/project") && details.contains("--loader ./custom.js"));
    assert!(
        details.contains("Env · NODE_OPTIONS"),
        "execution-affecting variable names must be exact"
    );
    assert!(!details.contains('{'));
    assert_eq!(
        view::download_progress(1_000_000, None),
        "1.0 MB downloaded"
    );
}
#[test]
fn review_function_key_range_requires_exact_f1_through_f24() {
    for key in ["f", "f0", "f25", "f01", "foo"] {
        assert_eq!(
            view::accelerator(&gpui::Keystroke::parse(key).unwrap()),
            None,
            "{key}"
        );
    }
    for key in ["f1", "f12", "f24", "ctrl-f"] {
        assert!(
            view::accelerator(&gpui::Keystroke::parse(key).unwrap()).is_some(),
            "{key}"
        );
    }
}

#[gpui::test]
fn review_scale_stepper_uses_each_click_before_parent_save_returns(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let store = SettingsStore::new(fixture());
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store, &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.activate(w, cx);
            v.set_layout(
                "Fixture display".into(),
                tezbar_host::layout::Profile::default(),
                w,
                cx,
            );
        })
    });
    settle(&rt, cx);
    let first = cx.debug_bounds("settings-Increase scale").unwrap().center();
    cx.simulate_click(first, Modifiers::default());
    settle(&rt, cx);
    let second = cx.debug_bounds("settings-Increase scale").unwrap().center();
    cx.simulate_click(second, Modifiers::default());
    settle(&rt, cx);
    assert!((view.read_with(cx, |v, _| v.layout.as_ref().unwrap().1.zoom) - 1.1).abs() < 0.001);
}

#[tokio::test]
async fn review_executor_failure_keeps_inflight_edit_retryable() {
    struct PanicOnce {
        fixture: Arc<Fixture>,
        panic: AtomicBool,
    }
    impl SettingsTransport for PanicOnce {
        fn durable_config(&self) -> SettingsFuture {
            if self.panic.swap(false, Ordering::SeqCst) {
                Box::pin(async { panic!("Synthetic executor interruption") })
            } else {
                self.fixture.durable_config()
            }
        }
        fn send(&self, request: SettingsRequest) -> SettingsFuture {
            self.fixture.send(request)
        }
    }
    let f = fixture();
    let store = SettingsStore::new(Arc::new(PanicOnce {
        fixture: f.clone(),
        panic: AtomicBool::new(true),
    }));
    let owner = store.owner();
    store.enqueue(
        owner,
        vec![Edit::Global {
            field: "memoryMaxItems".into(),
            value: json!(81),
        }],
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(1), store.drain())
            .await
            .unwrap()
            .is_err()
    );
    store.retry();
    tokio::time::timeout(Duration::from_secs(1), store.drain())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(f.config.lock().unwrap()["memoryMaxItems"], 81);
    assert_eq!(store.failures(), 0);
}

// Regressions reproduced by the independent 409dbf3 closure review.
struct ShortcutConflict(Arc<Fixture>);
impl SettingsTransport for ShortcutConflict {
    fn durable_config(&self) -> SettingsFuture {
        self.0.durable_config()
    }
    fn send(&self, request: SettingsRequest) -> SettingsFuture {
        if let SettingsRequest::Save(patch) = &request {
            if patch.get("raymesHotkey").is_some() {
                self.0
                    .calls
                    .lock()
                    .unwrap()
                    .push(("opus-hotkey-save", vec![]));
                return Box::pin(async {
                    Ok(json!({"ok":false,"accelerator":"Control+K","error":"in use"}))
                });
            }
        }
        self.0.send(request)
    }
}
#[gpui::test]
fn review_closure_settings_rejection_during_inflight_save_is_reported_and_does_not_cancel_next_action(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let f = fixture();
    let store = SettingsStore::new(Arc::new(ShortcutConflict(f.clone())));
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate(w, cx)));
    settle(&rt, cx);
    // Hold the first llm-config-set (an ordinary toggle save).
    let (release, gate) = tokio::sync::oneshot::channel();
    *f.held.lock().unwrap() = Some(gate);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.action(Action::Toggle("memoryEnabled", false), w, cx)
        })
    });
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| v.saving));
    // While that save is in flight the user records a shortcut the OS rejects
    // (this is exactly what the hotkey key handler does: self.queue(..)).
    view.update(cx, |v, cx| {
        v.queue(
            vec![Edit::Global {
                field: "raymesHotkey".into(),
                value: json!("Control+K"),
            }],
            cx,
        )
    });
    settle(&rt, cx);
    let _ = release.send(());
    settle(&rt, cx);
    let after_save_error = view.read_with(cx, |v, _| v.error.clone());
    let after_save_message = view.read_with(cx, |v, _| v.message.clone());
    let hotkey_saves = f
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == "opus-hotkey-save")
        .count();
    // Next, unrelated follow-up: Copilot sign-in (no new edit is enqueued by SignIn).
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let config = v.config.clone();
            v.provider.load("copilot".into(), &config);
            v.action(Action::SignIn, w, cx)
        })
    });
    settle(&rt, cx);
    let starts = f
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == "github-device-start")
        .count();
    let signin_error = view.read_with(cx, |v, _| v.error.clone());
    eprintln!(
        "OPUS2 rejection-in-flight: hotkey_saves={hotkey_saves} after_save error={after_save_error:?} message={after_save_message:?} | signin starts={starts} signin_error={signin_error:?} failures={}",
        store.failures()
    );
    assert_eq!(
        starts, 1,
        "stale shortcut rejection cancelled an unrelated Sign-in"
    );
    assert!(
        after_save_error.is_some(),
        "the rejected shortcut was reported as Saved"
    );
}
#[gpui::test]
fn review_closure_validation_error_is_not_a_retryable_durable_save(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = fixture();
    let store = SettingsStore::new(f.clone());
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate(w, cx)));
    settle(&rt, cx);
    view.update(cx, |v, cx| {
        if let Some(field) = v.fields.get_mut("memoryMaxItems") {
            field.dirty = true;
            field.value = "abc".into();
        }
        v.field_changed("memoryMaxItems", cx);
    });
    settle(&rt, cx);
    let local = view.read_with(cx, |v, _| v.error.clone());
    let durable = view.read_with(cx, |v, _| v.durable_error().map(str::to_owned));
    let retry_visible = cx.debug_bounds("settings-Retry saved changes").is_some();
    cx.update(|w, cx| view.update(cx, |v, cx| v.action(Action::Retry, w, cx)));
    settle(&rt, cx);
    let after_retry = view.read_with(cx, |v, _| v.error.clone());
    let after_retry_durable = view.read_with(cx, |v, _| v.durable_error().map(str::to_owned));
    eprintln!(
        "OPUS2 validation: local={local:?} durable={durable:?} retry_button={retry_visible} | after Retry local={after_retry:?} durable={after_retry_durable:?}"
    );
    assert!(
        !retry_visible,
        "a local validation error offers 'Retry saved changes' (D-4)"
    );
    assert_eq!(
        after_retry, local,
        "Retry replaced the field-specific validation message with the generic save error"
    );
}
#[gpui::test]
fn review_closure_confirmation_sheet_occludes_footer_controls(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = fixture();
    let store = SettingsStore::new(f.clone());
    let closes = std::rc::Rc::new(std::cell::Cell::new(0));
    let count = closes.clone();
    let (view, cx) = cx.add_window_view(|w, cx| {
        let v = SettingsContent::new(store.clone(), &UiState::default(), w, cx);
        let sub = cx.subscribe(&cx.entity(), move |_, _, e: &SettingsEffect, _| {
            if matches!(e, SettingsEffect::Close) {
                count.set(count.get() + 1)
            }
        });
        std::mem::forget(sub);
        v
    });
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate(w, cx)));
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.confirm(Action::Uninstall("fixture".into()), w, cx)
        })
    });
    settle(&rt, cx);
    let dialog = cx.debug_bounds("settings-confirmation");
    let close = cx.debug_bounds("settings-Close");
    if let Some(close) = close {
        cx.simulate_click(close.center(), Modifiers::default());
        settle(&rt, cx);
    }
    let still_open = view.read_with(cx, |v, _| v.confirmation.is_some());
    eprintln!(
        "OPUS2 modal: dialog={dialog:?} footer_close={close:?} close_events={} confirmation_still_open={still_open}",
        closes.get()
    );
    assert_eq!(
        closes.get(),
        0,
        "footer Close is clickable through the open confirmation sheet"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn review_closure_alias_racing_pump_exit_is_durable_for_both_owners() {
    tokio::time::timeout(Duration::from_secs(40), async {
        for i in 0..50_000u32 {
            let f = fixture();
            let store = SettingsStore::new(f.clone());
            let peer = store.owner();
            let alias = store.owner();
            store.enqueue(
                peer,
                vec![Edit::Global {
                    field: "memoryMaxItems".into(),
                    value: json!(i),
                }],
            );
            let peer_store = store.clone();
            let mut peer_wait = tokio::spawn(async move { peer_store.drain_owner(peer).await });
            let alias_store = store.clone();
            let mut alias_wait = tokio::spawn(async move {
                for _ in 0..i % 13 {
                    tokio::task::yield_now().await;
                }
                alias_store.alias(alias, "fixture".into(), format!("v{i}"));
                alias_store.drain_owner(alias).await
            });
            let result = tokio::time::timeout(Duration::from_secs(1), async {
                (&mut peer_wait).await.unwrap().unwrap();
                (&mut alias_wait).await.unwrap().unwrap();
                store.drain().await.unwrap();
            })
            .await;
            peer_wait.abort();
            alias_wait.abort();
            assert!(result.is_ok(), "owner stranded at iteration {i}");
            assert_eq!(
                f.config.lock().unwrap()["commandAliases"]["fixture"],
                json!(format!("v{i}"))
            );
            assert!(!store.pending());
        }
    })
    .await
    .expect("bounded alias race regression exceeded 40 seconds");
}
pub(crate) fn presentation_zoom(view: &SettingsContent) -> f64 {
    view.layout.as_ref().unwrap().1.zoom
}
#[test]
fn review_closure_field_labels_preserve_familiar_acronyms() {
    for (source, label) in [
        ("baseURL", "Base URL"),
        ("url", "URL"),
        ("apiKey", "API Key"),
        ("clientId", "Client ID"),
    ] {
        assert_eq!(view::display_name(source), label);
    }
}

#[gpui::test]
fn review_closure_rejected_shortcut_does_not_discard_an_already_queued_signin(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let f = fixture();
    let store = SettingsStore::new(Arc::new(ShortcutConflict(f.clone())));
    let (view, cx) =
        cx.add_window_view(|w, cx| SettingsContent::new(store.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate(w, cx)));
    settle(&rt, cx);
    // Hold the first llm-config-set (an ordinary toggle save).
    let (release, gate) = tokio::sync::oneshot::channel();
    *f.held.lock().unwrap() = Some(gate);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.action(Action::Toggle("memoryEnabled", false), w, cx)
        })
    });
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| v.saving));
    // While that save is in flight the user records a shortcut the OS rejects
    // (this is exactly what the hotkey key handler does: self.queue(..)).
    view.update(cx, |v, cx| {
        v.queue(
            vec![Edit::Global {
                field: "raymesHotkey".into(),
                value: json!("Control+K"),
            }],
            cx,
        )
    });
    settle(&rt, cx);
    // Queue the follow-up before the in-flight save finishes.
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let config = v.config.clone();
            v.provider.load("copilot".into(), &config);
            v.action(Action::SignIn, w, cx)
        })
    });
    settle(&rt, cx);
    let _ = release.send(());
    settle(&rt, cx);
    let after_save_error = view.read_with(cx, |v, _| v.error.clone());
    let after_save_message = view.read_with(cx, |v, _| v.message.clone());
    let hotkey_saves = f
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == "opus-hotkey-save")
        .count();
    let starts = f
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == "github-device-start")
        .count();
    let signin_error = view.read_with(cx, |v, _| v.error.clone());
    eprintln!(
        "OPUS2 rejection-in-flight: hotkey_saves={hotkey_saves} after_save error={after_save_error:?} message={after_save_message:?} | signin starts={starts} signin_error={signin_error:?} failures={}",
        store.failures()
    );
    assert_eq!(
        starts, 1,
        "stale shortcut rejection cancelled an unrelated Sign-in"
    );
    assert!(
        after_save_error.is_some(),
        "the rejected shortcut was reported as Saved"
    );
}

#[tokio::test]
async fn review_closure_dropped_barrier_releases_its_shortcut_rejection_claim() {
    let f = fixture();
    let store = SettingsStore::new(Arc::new(ShortcutConflict(f)));
    let owner = store.owner();
    store.enqueue(
        owner,
        vec![Edit::Global {
            field: "raymesHotkey".into(),
            value: json!("Control+K"),
        }],
    );
    let abandoned = store.drain_owner(owner);
    store.drain().await.unwrap();
    store.enqueue(
        owner,
        vec![Edit::Global {
            field: "memoryMaxItems".into(),
            value: json!(81),
        }],
    );
    let next = store.drain_owner(owner);
    drop(abandoned);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), next)
            .await
            .unwrap(),
        Err(store::REJECTED_CHANGE.into())
    );
    store.drain_owner(owner).await.unwrap();
    assert!(!store.owner_pending(owner));
}

// N-B1 regressions from the independent b626f28 closure review.
pub(crate) struct RerecordTransport {
    pub inner: Arc<Fixture>,
    pub hold: Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
    pub entered: Arc<AtomicBool>,
}
impl RerecordTransport {
    pub(crate) fn new(
        inner: Arc<Fixture>,
        hold: Option<tokio::sync::oneshot::Receiver<()>>,
    ) -> Self {
        Self {
            inner,
            hold: Mutex::new(hold),
            entered: Arc::new(AtomicBool::new(false)),
        }
    }
}
impl SettingsTransport for RerecordTransport {
    fn durable_config(&self) -> SettingsFuture {
        self.inner.durable_config()
    }
    fn send(&self, request: SettingsRequest) -> SettingsFuture {
        if let SettingsRequest::Save(patch) = &request {
            if patch.get("raymesHotkey") == Some(&json!("Control+K")) {
                self.entered.store(true, Ordering::SeqCst);
                let hold = self.hold.lock().unwrap().take();
                return Box::pin(async move {
                    if let Some(hold) = hold {
                        let _ = hold.await;
                    }
                    Ok(json!({"ok":false,"accelerator":"Control+K","error":"in use"}))
                });
            }
        }
        self.inner.send(request)
    }
}
fn shortcut_edit(v: &str) -> Vec<Edit> {
    vec![Edit::Global {
        field: "raymesHotkey".into(),
        value: json!(v),
    }]
}

// Store-level model of Onboarding re-recording: the first shortcut's GPUI task is replaced
// (gpui_tokio aborts the tokio task, dropping its waiter) while its rejected save is in flight.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn review_nb1_rerecorded_shortcut_does_not_inherit_abandoned_rejection() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let f = fixture();
        let (release, hold) = tokio::sync::oneshot::channel();
        let t = Arc::new(RerecordTransport::new(f.clone(), Some(hold)));
        let store = SettingsStore::new(t.clone());
        let owner = store.owner();
        store.enqueue(owner, shortcut_edit("Control+K"));
        let first = tokio::spawn(store.drain_owner(owner));
        while !t.entered.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        store.enqueue(owner, shortcut_edit("Control+J"));
        let second = store.drain_owner(owner);
        first.abort();
        let _ = first.await;
        let _ = release.send(());
        let result = tokio::time::timeout(Duration::from_secs(2), second)
            .await
            .unwrap();
        let saved = f.config.lock().unwrap()["raymesHotkey"].clone();
        eprintln!(
            "B6 dropped-waiter: second_result={result:?} durable_hotkey={saved} owner_pending={}",
            store.owner_pending(owner)
        );
        assert_eq!(saved, json!("Control+J"));
        assert_eq!(
            result,
            Ok(()),
            "the accepted later shortcut was reported as the earlier rejection"
        );
    })
    .await
    .expect("probe deadline");
}

#[tokio::test]
async fn review_nb1_replacement_supersedes_an_already_recorded_rejection() {
    let f = fixture();
    let store = SettingsStore::new(Arc::new(RerecordTransport::new(f.clone(), None)));
    let owner = store.owner();
    store.enqueue(owner, shortcut_edit("Control+K"));
    let abandoned = store.drain_owner(owner);
    tokio::time::timeout(Duration::from_secs(1), store.drain())
        .await
        .unwrap()
        .unwrap();
    assert!(store.owner_pending(owner));
    store.enqueue(owner, shortcut_edit("Control+J"));
    drop(abandoned);
    tokio::time::timeout(Duration::from_secs(1), store.drain_owner(owner))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(f.config.lock().unwrap()["raymesHotkey"], json!("Control+J"));
    assert!(!store.owner_pending(owner));
}
#[tokio::test]
async fn review_nb1_another_owners_shortcut_cannot_consume_the_rejection() {
    let f = fixture();
    let store = SettingsStore::new(Arc::new(RerecordTransport::new(f, None)));
    let first = store.owner();
    let second = store.owner();
    store.enqueue(first, shortcut_edit("Control+K"));
    let abandoned = store.drain_owner(first);
    tokio::time::timeout(Duration::from_secs(1), store.drain())
        .await
        .unwrap()
        .unwrap();
    drop(abandoned);
    store.enqueue(second, shortcut_edit("Control+J"));
    tokio::time::timeout(Duration::from_secs(1), store.drain_owner(second))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), store.drain_owner(first))
            .await
            .unwrap(),
        Err(store::REJECTED_CHANGE.into())
    );
}
