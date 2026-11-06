//! Native cross-owner composition, using the production Launcher, Settings and WriteQueue.
use super::*;
use crate::surfaces::settings::tests::{Fixture, runtime, settle};
use crate::surfaces::settings::{SettingsRequest, SettingsStore, SettingsTransport};
use ::core::prelude::v1::test;
use std::sync::atomic::Ordering;
pub(crate) struct Backend(pub Arc<Fixture>);
impl RequestAdapter for Backend {
    fn send(&self, request: Request) -> crate::surfaces::settings::SettingsFuture {
        match request {
            Request::LlmConfig => self.0.durable_config(),
            Request::LlmSet(patch) => self.0.send(SettingsRequest::Save(patch)),
            Request::Settings(request) => self.0.send(request),
            _ => Box::pin(async { Ok(json!([])) }),
        }
    }
}
impl crate::surfaces::utilities::UtilityTransport for Backend {
    fn send(
        &self,
        request: crate::surfaces::utilities::UtilityRequest,
    ) -> crate::surfaces::settings::SettingsFuture {
        let (channel, _) = request.wire();
        if channel == "knowledge:snapshot" {
            self.0.send(SettingsRequest::KnowledgeSnapshot)
        } else {
            Box::pin(async { Ok(json!([])) })
        }
    }
    fn release(
        &self,
        _: String,
        _: &'static str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send>> {
        Box::pin(async { Err("No external requests in fixture".into()) })
    }
}
pub(crate) fn backend() -> Arc<Fixture> {
    let f = Arc::new(Fixture::default());
    *f.config.lock().unwrap() = json!({"provider":"ollama","model":"base","providerConfigs":{"openai":{"apiKey":"peer-secret"}},"providerModels":{"ollama":[{"id":"base"},{"id":"newer"}]},"customProviders":[],"taskProviderOverrides":{"voice":"ollama"}});
    f
}
#[gpui::test]
fn settings_task_override_and_header_picker_share_one_owner_queue(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = backend();
    let store = SettingsStore::new(f.clone());
    let settings = store.owner();
    let (relay, _) = Relay::new();
    let injected = store.clone();
    let adapter = Arc::new(Backend(f.clone()));
    let (view, cx) = cx.add_window_view(|w, cx| {
        Launcher::with_adapter_and_settings(adapter, relay, None, Some(injected), w, cx)
    });
    store.enqueue(
        settings,
        vec![tezbar_ui_model::settings::Edit::TaskSelection {
            task: "search".into(),
            provider: "openai".into(),
            model: "search-peer".into(),
        }],
    );
    view.update(cx, |v, cx| {
        v.model_config = f.config.lock().unwrap().clone();
        v.model_choices = vec![("ollama".into(), "newer".into())];
        v.select_model(0, cx);
        assert!(v.model_saving);
    });
    settle(&rt, cx);
    assert!(!view.read_with(cx, |v, _| v.model_saving));
    let c = f.config.lock().unwrap();
    assert_eq!(c["model"], "newer");
    assert_eq!(c["taskModelOverrides"]["search"], "search-peer");
    assert_eq!(c["taskProviderOverrides"]["voice"], "ollama");
    assert_eq!(c["providerConfigs"]["openai"]["apiKey"], "peer-secret");
}
#[gpui::test]
fn peer_read_failure_cannot_acknowledge_a_picker_before_its_held_write(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = backend();
    f.fail_reads.store(1, Ordering::SeqCst);
    let (tx, rx) = tokio::sync::oneshot::channel();
    *f.held.lock().unwrap() = Some(rx);
    let store = SettingsStore::new(f.clone());
    let settings = store.owner();
    store.enqueue(
        settings,
        vec![tezbar_ui_model::settings::Edit::Global {
            field: "memoryEnabled".into(),
            value: json!(false),
        }],
    );
    let injected = store.clone();
    let (relay, _) = Relay::new();
    let (view, cx) = cx.add_window_view(|w, cx| {
        Launcher::with_adapter_and_settings(
            Arc::new(Backend(f.clone())),
            relay,
            None,
            Some(injected),
            w,
            cx,
        )
    });
    view.update(cx, |v, cx| {
        v.model_config = f.config.lock().unwrap().clone();
        v.model_choices = vec![("ollama".into(), "newer".into())];
        v.select_model(0, cx);
    });
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| v.model_saving));
    tx.send(()).unwrap();
    settle(&rt, cx);
    assert!(!view.read_with(cx, |v, _| v.model_saving));
    assert!(view.read_with(cx, |v, _| v.error.is_none()));
    assert!(rt.block_on(store.drain()).is_err());
    assert_eq!(f.config.lock().unwrap()["model"], "newer");
}
#[gpui::test]
fn failed_picker_retry_cannot_replace_newer_selection_or_independent_state_error(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let f = backend();
    let store = SettingsStore::new(f.clone());
    let settings = store.owner();
    let injected = store.clone();
    let (relay, _) = Relay::new();
    let (view, cx) = cx.add_window_view(|w, cx| {
        Launcher::with_adapter_and_settings(
            Arc::new(Backend(f.clone())),
            relay,
            None,
            Some(injected),
            w,
            cx,
        )
    });
    f.fail.store(true, Ordering::SeqCst);
    view.update(cx, |v, cx| {
        v.model_config = f.config.lock().unwrap().clone();
        v.model_choices = vec![("ollama".into(), "base".into())];
        v.select_model(0, cx);
    });
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| v.error.is_some()));
    f.fail.store(false, Ordering::SeqCst);
    store.enqueue(
        settings,
        vec![tezbar_ui_model::settings::Edit::TaskSelection {
            task: "search".into(),
            provider: "openai".into(),
            model: "peer".into(),
        }],
    );
    view.update(cx, |v, cx| {
        v.state_save_error = Some("Independent N1 state failure".into());
        v.model_choices = vec![("ollama".into(), "newer".into())];
        v.select_model(0, cx);
    });
    settle(&rt, cx);
    store.retry();
    rt.block_on(store.drain()).unwrap();
    assert_eq!(f.config.lock().unwrap()["model"], "newer");
    assert_eq!(
        f.config.lock().unwrap()["taskModelOverrides"]["search"],
        "peer"
    );
    assert_eq!(
        view.read_with(cx, |v, _| v.state_write_error()).as_deref(),
        Some("Independent N1 state failure")
    );
}
#[gpui::test]
fn indexing_to_knowledge_settings_close_resumes_the_same_indexing_entity(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = backend();
    let store = SettingsStore::new(f.clone());
    let backend = Arc::new(Backend(f));
    let injected = store.clone();
    let (relay, mut rx) = Relay::new();
    let settings_relay = relay.clone();
    let (launcher, cx) = cx.add_window_view(|w, cx| {
        let mut v = Launcher::with_adapter_and_settings(
            backend.clone(),
            relay,
            None,
            Some(injected),
            w,
            cx,
        );
        v.utility_transport = Some(backend);
        v
    });
    cx.update(|w, cx| {
        launcher.update(cx, |v, cx| {
            v.show(w, cx);
            v.receive_navigation(
                Navigation {
                    nonce: 1,
                    surface: Surface::Indexing,
                    payload: Payload::None,
                },
                cx,
            );
        })
    });
    settle(&rt, cx);
    let utility = launcher.read_with(cx, |v, _| v.utilities.as_ref().unwrap().entity_id());
    launcher.update(cx, |v, cx| {
        v.navigate(
            Surface::Settings("knowledge".into()),
            Payload::Settings {
                tab: "knowledge".into(),
            },
            cx,
        )
    });
    let intent = rt.block_on(async {
        loop {
            if let Some(Event::Navigate(intent)) = rx.recv().await {
                break intent;
            }
        }
    });
    assert_eq!(
        launcher.read_with(cx, |v, _| v.navigation.as_ref().unwrap().surface.clone()),
        Surface::Indexing
    );
    let shell = cx
        .update(|_, cx| {
            cx.open_window(WindowOptions::default(), |w, cx| {
                cx.new(|cx| {
                    let mut shell = crate::windows::settings::Settings::new(settings_relay, w, cx);
                    let content = cx.new(|cx| {
                        crate::surfaces::settings::SettingsContent::new(
                            store,
                            &UiState::default(),
                            w,
                            cx,
                        )
                    });
                    content.update(cx, |v, cx| v.navigate(&intent, w, cx));
                    shell.set_content(content, cx);
                    shell
                })
            })
        })
        .unwrap();
    cx.update(|w, cx| launcher.update(cx, |v, cx| v.hide(w, cx)));
    shell.update(cx, |v, _, cx| v.begin_close(19, cx)).unwrap();
    settle(&rt, cx);
    let finished = rt.block_on(async {
        loop {
            if let Some(Event::SettingsFinish(19, result)) = rx.recv().await {
                break result;
            }
        }
    });
    finished.unwrap();
    cx.update(|w, cx| launcher.update(cx, |v, cx| v.show(w, cx)));
    shell.update(cx, |_, w, _| w.remove_window()).unwrap();
    settle(&rt, cx);
    launcher.read_with(cx, |v, cx| {
        assert_eq!(v.navigation.as_ref().unwrap().surface, Surface::Indexing);
        assert_eq!(v.utilities.as_ref().unwrap().entity_id(), utility);
        assert!(v.utilities.as_ref().unwrap().read(cx).visible);
    });
}
