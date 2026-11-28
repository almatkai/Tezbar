//! Opus review probe: actual App quit whose FINAL chat drain fails after extension drain began.
use super::m8::tests::owner;
use super::*;
use crate::core_client::{Request, RequestAdapter};
use crate::surfaces::command_bar::chat_integration::tests::{
    Backend, mount, publish, settle, submit,
};
use crate::surfaces::command_bar::navigation::{Navigation, NavigationInbox, Payload};
use ::core::prelude::v1::test;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
struct ExtBackend {
    inner: Arc<Backend>,
    calls: Mutex<Vec<(&'static str, Vec<serde_json::Value>)>>,
}
impl RequestAdapter for ExtBackend {
    fn send(
        &self,
        r: Request,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<serde_json::Value, String>> + Send>,
    > {
        if let Request::Extension(r) = r {
            let (channel, args) = r.wire();
            self.calls.lock().unwrap().push((channel, args));
            return Box::pin(async move {
                Ok(match channel {
                    "extension:dispose-session" => serde_json::json!(true),
                    _ => serde_json::json!({"ok":true,"mode":"unchanged"}),
                })
            });
        }
        self.inner.send(r)
    }
}
#[gpui::test]
fn review_m9_cancelled_final_quit_restores_active_extension(cx: &mut TestAppContext) {
    let (rt, root, f, view, cx) = mount(cx);
    let app = owner(cx, &rt, &root, &f);
    let ext = Arc::new(ExtBackend {
        inner: f.clone(),
        calls: Default::default(),
    });
    cx.update(|_, cx| {
        cx.set_global(NavigationInbox::default());
        f.core.bind_extension_runtime(rt.handle().clone());
        view.update(cx, |v, _| {
            v.review_m9_set_adapter(ext.clone());
            v.review_m9_configure_extensions(f.core.clone());
        });
    });
    view.update(cx, |v, cx| {
        v.receive_navigation(submit(1, "Probe chat before extension"), cx)
    });
    f.wait_for_runs(&rt, cx, 1);
    let run = f.run();
    publish(
        cx,
        serde_json::json!({"type":"message","runId":run["runId"],"delta":"Probe retained answer"}),
    );
    let n = Navigation {
        nonce: 2,
        surface: tezbar_ui_model::router::Surface::ExtensionRuntime,
        payload: Payload::Extension {
            result: serde_json::json!({"ok":true,"mode":"view","extensionId":"probe","commandName":"form","sessionId":"probe-session","generation":"probe-session","refreshMs":null,"viewRevision":0,
                "root":{"type":"Form","props":{"navigationTitle":"Probe Form"},"children":[{"type":"Form.TextField","props":{"id":"note","title":"Note"},"children":[]}]},
                "actions":[{"id":"submit","title":"Submit","kind":"submit-form"}]}),
        },
    };
    cx.update(|_, cx| {
        cx.global_mut::<NavigationInbox>().put(n.clone()).unwrap();
        view.update(cx, |v, cx| v.receive_navigation(n, cx));
    });
    settle(&rt, cx);
    let before = view
        .read_with(cx, |v, cx| v.review_m9_runtime_state(cx))
        .expect("runtime mounted");
    let runtime = view.read_with(cx, |v, _| v.review_m9_runtime()).unwrap();
    cx.update(|w, cx| {
        runtime.update(cx, |v, cx| {
            v.event(
                tezbar_ui_model::extensions::runtime::UiEvent::ChangeField {
                    id: "note".into(),
                    value: serde_json::json!("typed before quit"),
                },
                w,
                cx,
            )
        })
    });
    let typed = view
        .read_with(cx, |v, cx| v.review_m9_runtime_state(cx))
        .unwrap();
    eprintln!("OPUS-PROBE quit before={before:?} typed={typed:?}");
    f.fail_append.store(true, Ordering::SeqCst);
    {
        let _enter = rt.enter();
        app.update(&mut cx.cx, |o, cx| o.quit(cx));
    }
    for _ in 0..12 {
        settle(&rt, cx);
        if !app.read_with(&cx.cx, |o, _| o.lifecycle.quitting) {
            break;
        }
    }
    let quitting = app.read_with(&cx.cx, |o, _| o.lifecycle.quitting);
    let save_error = app.read_with(&cx.cx, |o, _| o.save_error.clone());
    let cancels = f.count("agent:cancel");
    let disposes: Vec<_> = ext
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == "extension:dispose-session")
        .map(|(_, a)| a[0].clone())
        .collect();
    let after_cancel = view.read_with(cx, |v, cx| {
        (v.review_m9_surface(), v.review_m9_runtime_state(cx))
    });
    eprintln!(
        "OPUS-PROBE quit cancelled: quitting={quitting} save_error={save_error:?} agent_cancel={cancels} disposes={disposes:?} after_cancel={after_cancel:?}"
    );
    f.fail_append.store(false, Ordering::SeqCst);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.visible = true;
            v.hide(w, cx);
            v.show(w, cx);
        })
    });
    settle(&rt, cx);
    let reopened = view.read_with(cx, |v, cx| {
        (v.review_m9_surface(), v.review_m9_runtime_state(cx))
    });
    let invokes_before = ext
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == "extension:invoke-action")
        .count();
    eprintln!("OPUS-PROBE quit reopened={reopened:?} invokes_before={invokes_before}");
    assert!(
        !quitting,
        "probe precondition: final chat drain failure must cancel quit"
    );
    assert!(
        save_error.is_some(),
        "probe precondition: cancelled quit records save error"
    );
    assert!(
        disposes.is_empty(),
        "cancelled quit disposed the active extension session: {disposes:?}"
    );
    let (_, state) = reopened;
    let state = state.expect("runtime remounted");
    assert_eq!(
        state.0, before.0,
        "cancelled quit replaced the retained RuntimeView entity"
    );
    assert_eq!(
        state.2,
        Some(serde_json::json!("typed before quit")),
        "cancelled quit lost typed form state"
    );
}

#[gpui::test]
fn review_m9_discarded_foreground_reply_is_disposed_but_background_status_keeps_owner(
    cx: &mut TestAppContext,
) {
    let (rt, _root, f, view, cx) = mount(cx);
    let ext = Arc::new(ExtBackend {
        inner: f.clone(),
        calls: Default::default(),
    });
    view.update(cx, |v, _| v.review_m9_set_adapter(ext.clone()));
    cx.update(|_,cx|view.update(cx,|v,cx| {
        v.review_m9_discard(&serde_json::json!({"ok":true,"mode":"view","sessionId":"orphan","root":{"type":"List"}}),cx);
        v.review_m9_discard(&serde_json::json!({"ok":true,"mode":"view","sessionId":"status","root":{"type":"MenuBarExtra"}}),cx);
    }));
    settle(&rt, cx);
    let calls = ext.calls.lock().unwrap();
    let ids = calls
        .iter()
        .filter(|(channel, _)| *channel == "extension:dispose-session")
        .map(|(_, a)| a[0]["sessionId"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["orphan"]);
}

#[gpui::test]
fn review_m9_busy_install_refuses_quit_before_cancelling_live_chat(cx: &mut TestAppContext) {
    let (rt, root, f, view, cx) = mount(cx);
    let app = owner(cx, &rt, &root, &f);
    assert!(app.read_with(&cx.cx, |v, _| v.settings.is_none()));
    view.update(cx, |v, cx| {
        v.review_m9_configure_extensions(f.core.clone());
        v.receive_navigation(submit(1, "Keep this run alive"), cx)
    });
    f.wait_for_runs(&rt, cx, 1);
    assert_eq!(f.count("agent:run"), 1);
    let _install = f.core.hold_extension_install_for_test();
    {
        let _enter = rt.enter();
        app.update(&mut cx.cx, |v, cx| v.quit(cx));
    }
    for _ in 0..16 {
        settle(&rt, cx);
        if !app.read_with(&cx.cx, |v, _| v.lifecycle.quitting) {
            break;
        }
    }
    assert!(!app.read_with(&cx.cx, |v, _| v.lifecycle.quitting));
    assert!(app.read_with(&cx.cx, |v, _| v.settings.is_none()));
    assert!(!app.read_with(&cx.cx, |v, _| v.lifecycle.settings));
    assert!(
        app.read_with(&cx.cx, |v, _| v.save_error.clone())
            .unwrap()
            .contains("Extension install in progress")
    );
    assert_eq!(
        f.count("agent:cancel"),
        0,
        "an unrelated busy install must leave chat running"
    );
}
