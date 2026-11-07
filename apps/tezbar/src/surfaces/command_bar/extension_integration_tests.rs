use super::chat_integration::tests as chat_integration_tests;
use super::*;
use crate::surfaces::extensions_store::request::ExtensionRequest;
use crate::surfaces::extensions_store::{StoreEffect, StoreEffectEmission};
use ::core::prelude::v1::test;
struct Backend {
    inner: Arc<chat_integration_tests::Backend>,
    calls: std::sync::Mutex<Vec<(&'static str, Vec<Value>)>>,
}
impl RequestAdapter for Backend {
    fn send(&self, r: Request) -> crate::surfaces::extensions_store::request::ExtensionFuture {
        if let Request::Extension(r) = r {
            let (channel, args) = r.wire();
            self.calls.lock().unwrap().push((channel, args));
            return Box::pin(async move {
                Ok(match channel {
                    "extension:list" => json!([]),
                    "extension:search-store" => json!({"items":[],"total":0,"hasMore":false}),
                    _ => json!({"ok":true,"mode":"unchanged"}),
                })
            });
        }
        self.inner.send(r)
    }
}
fn intent(nonce: u64, surface: Surface) -> Navigation {
    let payload = if surface == Surface::ExtensionRuntime {
        Payload::Extension {
            result: json!({"ok":true,"mode":"view","extensionId":"fixture","commandName":"run","sessionId":format!("session-{nonce}"),"generation":"g","refreshMs":null,"root":{"type":"Detail","props":{"markdown":"Retained runtime"}},"actions":[]}),
        }
    } else {
        Payload::None
    };
    Navigation {
        nonce,
        surface,
        payload,
    }
}
#[gpui::test]
fn retained_store_runtime_ack_source_fence_and_settings_route(cx: &mut TestAppContext) {
    let (rt, _root, inner, launcher, cx) = chat_integration_tests::mount(cx);
    let backend = Arc::new(Backend {
        inner: inner.clone(),
        calls: Default::default(),
    });
    cx.update(|_, cx| {
        cx.set_global(navigation::NavigationInbox::default());
        inner.core.bind_extension_runtime(rt.handle().clone());
        launcher.update(cx, |v, _| {
            v.adapter = backend.clone();
            v.configure_extensions(inner.core.clone());
        });
    });
    for nonce in 1..=100 {
        let n = intent(nonce, Surface::Extensions);
        cx.update(|_, cx| {
            cx.global_mut::<navigation::NavigationInbox>()
                .put(n.clone())
                .unwrap();
            launcher.update(cx, |v, cx| v.receive_navigation(n, cx));
        });
        chat_integration_tests::settle(&rt, cx);
        cx.read(|cx| {
            assert!(
                cx.global::<navigation::NavigationInbox>()
                    .pending()
                    .is_none()
            )
        });
        cx.update(|w, cx| {
            launcher.update(cx, |v, cx| {
                assert!(v.extension_mounted());
                v.hide(w, cx);
                v.show(w, cx);
            })
        });
    }
    let store = launcher.read_with(cx, |v, _| v.extensions.store.clone().unwrap());
    let old = store.read_with(cx, |v, _| v.host_generation());
    cx.update(|w, cx| {
        launcher.update(cx, |v, cx| {
            v.hide(w, cx);
            v.show(w, cx);
        })
    });
    cx.update(|_, cx| {
        store.update(cx, |_, cx| {
            cx.emit(StoreEffectEmission {
                generation: old,
                effect: StoreEffect::Back,
            })
        })
    });
    chat_integration_tests::settle(&rt, cx);
    assert!(launcher.read_with(cx, |v, _| {
        v.navigation
            .as_ref()
            .is_some_and(|n| n.surface == Surface::Extensions)
    }));
    let n = intent(101, Surface::ExtensionRuntime);
    cx.update(|_, cx| {
        cx.global_mut::<navigation::NavigationInbox>()
            .put(n.clone())
            .unwrap();
        launcher.update(cx, |v, cx| v.receive_navigation(n, cx));
    });
    chat_integration_tests::settle(&rt, cx);
    let runtime = launcher.read_with(cx, |v, _| v.extensions.runtime.clone().unwrap());
    let id = runtime.entity_id();
    let mount = runtime.read_with(cx, |v, _| v.host_generation());
    cx.update(|w, cx| {
        launcher.update(cx, |v, cx| {
            assert!(v.extension_source(id, mount, cx));
            v.hide(w, cx);
            v.show(w, cx);
            assert!(!v.extension_source(id, mount, cx));
        })
    });
    assert_eq!(
        id,
        launcher.read_with(cx, |v, _| v
            .extensions
            .runtime
            .as_ref()
            .unwrap()
            .entity_id())
    );
    // Repeated route deliveries are idempotent and do not dispose this session.
    assert_eq!(
        backend
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| *c == "extension:dispose-session")
            .count(),
        0
    );
    cx.update(|_, cx| {
        let n = intent(102, Surface::Extensions);
        launcher.update(cx, |v, cx| v.receive_navigation(n, cx));
    });
    chat_integration_tests::settle(&rt, cx);
    cx.update(|_, cx| {
        store.update(cx, |v, cx| {
            cx.emit(StoreEffectEmission {
                generation: v.host_generation(),
                effect: StoreEffect::Settings,
            })
        })
    });
    chat_integration_tests::settle(&rt, cx);
    let mut receiver = inner.receiver.lock().unwrap().take().unwrap();
    let mut found = false;
    for _ in 0..16 {
        match rt.block_on(async {
            tokio::time::timeout(Duration::from_millis(1), receiver.recv()).await
        }) {
            Ok(Some(Event::Navigate(n))) if n.surface == Surface::Settings("extensions".into()) => {
                found = true;
                break;
            }
            Ok(Some(_)) => {}
            _ => break,
        }
    }
    assert!(found, "Store Settings must reach the existing M7 route");
    for nonce in [103, 104] {
        cx.update(|_, cx| {
            launcher.update(cx, |v, cx| {
                v.receive_navigation(intent(nonce, Surface::ExtensionRuntime), cx)
            })
        });
        chat_integration_tests::settle(&rt, cx);
        if nonce == 103 {
            cx.update(|w, cx| launcher.update(cx, |v, cx| v.extension_back(w, cx)));
            chat_integration_tests::settle(&rt, cx);
        }
    }
    cx.cx.update(|cx| {
        let hub = cx.global::<CoreEvents>().clone();
        hub.publish(
            CoreEvent::Other {
                channel: "extension:session-invalidated".into(),
                payload: json!({"sessionId":"session-104","generation":"g","revision":1}),
            },
            cx,
        );
    });
    chat_integration_tests::settle(&rt, cx);
    assert!(
        backend
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|(channel, _)| *channel == "extension:refresh-session"),
        "Reopened runtime must reattach its weak event subscription"
    );
    let drain = cx.update(|_, cx| launcher.update(cx, |v, cx| v.drain_extensions(cx)));
    rt.block_on(drain).unwrap();
    chat_integration_tests::settle(&rt, cx);
    assert_eq!(
        backend
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| *c == "extension:dispose-session")
            .count(),
        3
    );
    let disposed = backend
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(channel, _)| *channel == "extension:dispose-session")
        .map(|(_, args)| args[0]["sessionId"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(disposed, vec!["session-101", "session-103", "session-104"]);
    let _ = ExtensionRequest::List;
}
