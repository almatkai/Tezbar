use super::*;
use ::core::prelude::v1::test;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::sync::Mutex;
#[derive(Default)]
struct Fixture {
    calls: Mutex<Vec<(&'static str, Vec<Value>)>>,
    bytes: Mutex<Vec<u8>>,
}
fn row(id: &str) -> Value {
    json!({"sessionId":id,"name":id,"status":"running","cwd":"/fixture"})
}
impl RequestAdapter for Fixture {
    fn send(
        &self,
        r: Request,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>> {
        let (c, args) = r.wire();
        self.calls.lock().unwrap().push((c, args.clone()));
        let mut bytes = self.bytes.lock().unwrap();
        if bytes.is_empty() {
            bytes.extend(b"test");
        }
        let v = match c {
            "terminal:list" => json!([row("a"), row("b")]),
            "core:terminal:stream-attach" => {
                json!({"sessionId":args[0]["sessionId"],"summary":row(args[0]["sessionId"].as_str().unwrap()),"generation":"g","replay":"t","watermark":bytes.len()})
            }
            "core:terminal:replay-read" => {
                let at = args[0]["position"].as_u64().unwrap() as usize;
                json!({"generation":"g","start":at,"end":bytes.len(),"watermark":bytes.len(),"bytes":STANDARD.encode(&bytes[at..])})
            }
            "terminal:create" => json!({"sessionId":"new","summary":row("new"),"generation":"g"}),
            _ => json!([]),
        };
        Box::pin(async move { Ok(v) })
    }
}
fn settle(rt: &tokio::runtime::Runtime, cx: &mut VisualTestContext) {
    rt.block_on(async {
        for _ in 0..128 {
            cx.run_until_parked();
            cx.executor().advance_clock(Duration::from_millis(2));
            tokio::task::yield_now().await;
        }
    });
}
fn intent(nonce: u64) -> Navigation {
    Navigation {
        nonce,
        surface: Surface::Terminal,
        payload: Payload::Terminal {
            command: None,
            cwd: None,
            session: Some("a".into()),
            defaults: Default::default(),
        },
    }
}
#[gpui::test]
fn retained_launcher_actual_navigation_events_panel_and_back(cx: &mut TestAppContext) {
    let rt = crate::surfaces::settings::tests::runtime(cx);
    let f = Arc::new(Fixture::default());
    let (relay, _rx) = Relay::new();
    cx.update(|cx| cx.set_global(CoreEvents::new(relay.clone())));
    let (launcher, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(f.clone(), relay, None, w, cx));
    launcher.update(cx, |v, cx| {
        v.visible = true;
        v.ready = true;
        v.receive_navigation(intent(1), cx);
    });
    settle(&rt, cx);
    let view = launcher.read_with(cx, |v, _| v.terminal.view.clone().unwrap());
    let source = view.read_with(cx, |v, _| TerminalSource {
        entity: view.entity_id(),
        mount: v.host_generation(),
    });
    assert!(launcher.read_with(cx, |v, cx| v.accepts_terminal_source(source, cx)));
    f.bytes.lock().unwrap().extend(b"test");
    cx.cx.update(|cx|cx.global::<CoreEvents>().clone().publish(CoreEvent::Other{channel:"core:terminal:data".into(),payload:json!({"sessionId":"a","generation":"g","start":4,"end":8,"bytes":STANDARD.encode(b"test"),"durable":true})},cx));
    cx.update(|w, cx| {
        launcher.update(cx, |v, cx| v.hide(w, cx));
    });
    settle(&rt, cx);
    assert!(!launcher.read_with(cx, |v, cx| v.accepts_terminal_source(source, cx)));
    cx.update(|w, cx| {
        launcher.update(cx, |v, cx| v.show(w, cx));
    });
    settle(&rt, cx);
    assert_eq!(
        launcher.read_with(cx, |v, _| v.terminal.view.as_ref().unwrap().entity_id()),
        view.entity_id()
    );
    assert_eq!(
        launcher.read_with(cx, |v, cx| v.terminal_panel_rows(cx).len()),
        2
    );
    cx.update(|w, cx| {
        launcher.update(cx, |v, cx| v.terminal_panel_action(Some("b".into()), w, cx))
    });
    settle(&rt, cx);
    assert!(!launcher.read_with(cx, |v, cx| v.accepts_terminal_source(source, cx)));
    cx.update(|w, cx| launcher.update(cx, |v, cx| v.terminal_panel_action(None, w, cx)));
    settle(&rt, cx);
    assert!(
        f.calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| *c == "terminal:create")
    );
    let source = view.read_with(cx, |v, _| TerminalSource {
        entity: view.entity_id(),
        mount: v.host_generation(),
    });
    cx.update(|w, cx| launcher.update(cx, |v, cx| v.terminal_back(source, w, cx)));
    settle(&rt, cx);
    assert!(launcher.read_with(cx, |v, _| v.navigation.is_none()));
    assert_eq!(launcher.read_with(cx, |v, _| v.model.input.clone()), ">");
    assert!(
        !f.calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| matches!(*c, "terminal:kill" | "terminal:delete"))
    );
}
#[gpui::test]
fn quit_refusal_retains_exact_entity_and_cleanup_can_retry(cx: &mut TestAppContext) {
    struct Refusing {
        base: Fixture,
        fail: std::sync::atomic::AtomicBool,
    }
    impl RequestAdapter for Refusing {
        fn send(
            &self,
            r: Request,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>>
        {
            if matches!(
                &r,
                Request::Terminal(TerminalRequest::DetachGeneration { .. })
            ) && self.fail.load(std::sync::atomic::Ordering::SeqCst)
            {
                return Box::pin(async { Err("fixture detach failure".into()) });
            }
            self.base.send(r)
        }
    }
    let rt = crate::surfaces::settings::tests::runtime(cx);
    let f = Arc::new(Refusing {
        base: Default::default(),
        fail: true.into(),
    });
    let (relay, _rx) = Relay::new();
    cx.update(|cx| cx.set_global(CoreEvents::new(relay.clone())));
    let (launcher, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(f.clone(), relay, None, w, cx));
    launcher.update(cx, |v, cx| {
        v.visible = true;
        v.ready = true;
        v.receive_navigation(intent(1), cx);
    });
    settle(&rt, cx);
    let id = launcher.read_with(cx, |v, _| v.terminal.view.as_ref().unwrap().entity_id());
    let drain = cx.update(|w, cx| launcher.update(cx, |v, cx| v.drain_terminal(w, cx)));
    settle(&rt, cx);
    let result = rt.block_on(drain);
    assert!(result.is_err());
    assert_eq!(
        launcher.read_with(cx, |v, _| v.terminal.view.as_ref().unwrap().entity_id()),
        id
    );
    f.fail.store(false, std::sync::atomic::Ordering::SeqCst);
    let drain = cx.update(|w, cx| launcher.update(cx, |v, cx| v.drain_terminal(w, cx)));
    settle(&rt, cx);
    assert!(rt.block_on(drain).is_ok());
}
