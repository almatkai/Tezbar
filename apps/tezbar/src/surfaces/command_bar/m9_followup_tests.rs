//! Independent Opus D13 follow-up probes (test cfg only, private analysis copy).
//! Drives the REAL launcher activation branch (Enter -> held extension:run-command ->
//! hide/new query -> late reply), not the dispose helper seam used by the root test.
use super::*;
use ::core::prelude::v1::test;
use std::sync::Mutex;

#[derive(Default)]
struct HeldRun {
    calls: Mutex<Vec<(String, Vec<Value>)>>,
    held: Mutex<Option<tokio::sync::oneshot::Sender<Value>>>,
}
impl RequestAdapter for HeldRun {
    fn send(
        &self,
        request: Request,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>> {
        let (channel, args) = request.wire();
        self.calls.lock().unwrap().push((channel.into(), args));
        if channel == "extension:run-command" {
            let (tx, rx) = tokio::sync::oneshot::channel();
            *self.held.lock().unwrap() = Some(tx);
            return Box::pin(async move { rx.await.map_err(|e| e.to_string()) });
        }
        Box::pin(async { Ok(json!({"ok":true})) })
    }
}
fn runtime(cx: &mut TestAppContext) -> tokio::runtime::Runtime {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| gpui_tokio::init_from_handle(cx, runtime.handle().clone()));
    runtime
}
fn settle(rt: &tokio::runtime::Runtime, cx: &mut VisualTestContext) {
    rt.block_on(async {
        for _ in 0..128 {
            cx.run_until_parked();
            tokio::task::yield_now().await;
        }
    });
}
/// Same seed as command_bar/tests.rs (private there).
fn seed(v: &mut Launcher, rows: Vec<Row>) {
    v.ready = true;
    v.visible = true;
    v.model.visible = true;
    v.model.input = "fixture".into();
    v.model.session.commit("fixture".into(), 0);
    v.model.session.take_request(8);
    let generation = v.model.session.generation;
    v.model.update(
        Input::SearchComplete {
            generation,
            result: Ok(rows),
        },
        9,
    );
    v.rows = v.model.display_rows();
}
fn view_reply(root: Value) -> Value {
    json!({"ok":true,"mode":"view","extensionId":"fixture","commandName":"list","sessionId":"orphan","generation":"orphan","refreshMs":null,"root":root,"actions":[]})
}
/// Returns the session ids the launcher disposed after the late reply.
fn run_then(cx: &mut TestAppContext, discard: &str, reply: Value) -> (Vec<String>, Vec<String>) {
    let rt = runtime(cx);
    let adapter = Arc::new(HeldRun::default());
    let (relay, _rx) = Relay::new();
    cx.update(|cx| cx.set_global(CoreEvents::new(relay.clone())));
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let mut row = Row::command("ext", "Synthetic extension command");
            row.action = Action::RunExtensionCommand {
                extension_id: "fixture".into(),
                command_name: "list".into(),
                extra: serde_json::from_value(json!({})).unwrap(),
            };
            seed(v, vec![row]);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("enter");
    settle(&rt, cx);
    assert!(
        adapter.held.lock().unwrap().is_some(),
        "probe precondition: real activation sent extension:run-command"
    );
    match discard {
        "hide" => cx.update(|w, cx| view.update(cx, |v, cx| v.hide(w, cx))),
        "query" => cx.update(|w, cx| view.update(cx, |v, cx| v.edit("newer query", w, cx))),
        _ => {}
    }
    settle(&rt, cx);
    let tx = adapter.held.lock().unwrap().take().unwrap();
    let _ = tx.send(reply);
    settle(&rt, cx);
    let calls = adapter.calls.lock().unwrap();
    let disposed = calls
        .iter()
        .filter(|(c, _)| c == "extension:dispose-session")
        .map(|(_, a)| a[0]["sessionId"].as_str().unwrap_or("").to_owned())
        .collect::<Vec<_>>();
    let channels = calls.iter().map(|(c, _)| c.clone()).collect::<Vec<_>>();
    (disposed, channels)
}
#[gpui::test]
fn review_m9_followup_d13_hidden_launcher_disposes_late_view_reply(cx: &mut TestAppContext) {
    let (disposed, channels) = run_then(
        cx,
        "hide",
        view_reply(json!({"type":"List","props":{},"children":[]})),
    );
    eprintln!("OPUS-D13 hide/List: disposed={disposed:?} channels={channels:?}");
    assert_eq!(disposed, vec!["orphan".to_owned()]);
}
#[gpui::test]
fn review_m9_followup_d13_new_query_disposes_late_view_reply(cx: &mut TestAppContext) {
    let (disposed, channels) = run_then(
        cx,
        "query",
        view_reply(json!({"type":"List","props":{},"children":[]})),
    );
    eprintln!("OPUS-D13 query/List: disposed={disposed:?} channels={channels:?}");
    assert_eq!(disposed, vec!["orphan".to_owned()]);
}
#[gpui::test]
fn review_m9_followup_d13_hidden_launcher_keeps_background_menubar_owner(cx: &mut TestAppContext) {
    let (disposed, channels) = run_then(
        cx,
        "hide",
        view_reply(json!({"type":"MenuBarExtra","props":{"title":"Status"},"children":[]})),
    );
    eprintln!("OPUS-D13 hide/MenuBarExtra: disposed={disposed:?} channels={channels:?}");
    assert!(disposed.is_empty());
}
/// Visible launcher, but the native model refuses the view (duplicate Form field ids,
/// a plain SDK author error; validate() also rejects >20k nodes / >2 MiB props / unknown
/// kinds). receive_navigation rejects it with "Extension view is unavailable" (mod.rs
/// ~1586-1600). Intended (D13 principle): a view reply the UI will never own is disposed.
#[gpui::test]
fn review_m9_followup_d13_rejected_view_reply_is_disposed(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let adapter = Arc::new(HeldRun::default());
    let (relay, _rx) = Relay::new();
    cx.update(|cx| cx.set_global(CoreEvents::new(relay.clone())));
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    let root = tempfile::tempdir().unwrap();
    let paths = tezbar_core::common::CorePaths {
        config_dir: root.path().join("config"),
        data_dir: root.path().join("data"),
        home_dir: root.path().join("home"),
        temp_dir: root.path().join("tmp"),
        resource_dir: root.path().join("resources"),
    };
    let context = tezbar_core::common::CoreContext::new(
        paths,
        Some(Arc::new(tezbar_core::host::TestHost::default())),
    );
    let core = tezbar_core::NativeCore::new(context).unwrap();
    let client = CoreClient::new(core);
    let invalid = view_reply(json!({"type":"Form","props":{},"children":[
        {"type":"Form.TextField","props":{"id":"same","title":"A"}},
        {"type":"Form.TextField","props":{"id":"same","title":"B"}}]}));
    assert!(
        tezbar_ui_model::extensions::runtime::TreeReply::from_wire(invalid.clone()).is_err(),
        "probe precondition: native model rejects this reply"
    );
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            seed(v, vec![]);
            v.configure_extensions(client);
            v.receive_navigation(
                Navigation {
                    nonce: 7,
                    surface: Surface::ExtensionRuntime,
                    payload: Payload::Extension { result: invalid },
                },
                cx,
            );
        })
    });
    settle(&rt, cx);
    let error = view.read_with(cx, |v, _| v.error.clone());
    let disposed = adapter
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| c == "extension:dispose-session")
        .map(|(_, a)| a[0]["sessionId"].as_str().unwrap_or("").to_owned())
        .collect::<Vec<_>>();
    eprintln!("OPUS-D13 rejected view: error={error:?} disposed={disposed:?}");
    assert!(
        error.is_some(),
        "probe precondition: launcher rejected the view"
    );
    assert_eq!(
        disposed,
        vec!["orphan".to_owned()],
        "rejected view reply leaves its core/worker session alive"
    );
}

// ------------------------------------------------------------------ real worker
#[path = "../../../../../tests/fixtures/extensions/native_harness.rs"]
mod native_harness;
/// Production CoreClient does the extension work (admission + detached core task, real
/// Bun worker via the private prepared bundle, fake TestHost). Only the DELIVERY of the
/// run reply to the launcher is gated, so hide/new-query happen deterministically while
/// the core run is in flight. Non-extension launcher channels are stubbed locally.
struct GatedCore {
    client: CoreClient,
    calls: Mutex<Vec<String>>,
    gate: Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
}
impl RequestAdapter for GatedCore {
    fn send(
        &self,
        request: Request,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>> {
        let (channel, _) = request.clone().wire();
        self.calls.lock().unwrap().push(channel.into());
        if channel.starts_with("extension:") {
            let work = RequestAdapter::send(&self.client, request); // core task spawned now
            if channel == "extension:run-command" {
                let gate = self.gate.lock().unwrap().take();
                return Box::pin(async move {
                    if let Some(gate) = gate {
                        let _ = gate.await;
                    }
                    work.await
                });
            }
            return work;
        }
        Box::pin(async move {
            Ok(if channel == "search:all" {
                json!([])
            } else {
                json!({"ok":true})
            })
        })
    }
}
fn real_worker_case(
    cx: &mut TestAppContext,
    discard: &str,
) -> (
    Vec<String>,
    Vec<String>,
    Option<Result<Value, String>>,
    Vec<String>,
) {
    let core_rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let prepared: std::path::PathBuf = std::env::var_os("TEZBAR_M09_PREPARED")
        .expect("TEZBAR_M09_PREPARED")
        .into();
    let bun: std::path::PathBuf = std::env::var_os("TEZBAR_M09_BUN")
        .expect("TEZBAR_M09_BUN")
        .into();
    let fixture = core_rt
        .block_on(native_harness::Fixture::isolated(
            &prepared,
            bun,
            core_rt.handle().clone(),
        ))
        .unwrap();
    let rt = runtime(cx);
    let client = CoreClient::new(fixture.transport.core.clone());
    client.bind_extension_runtime(core_rt.handle().clone());
    let (open_gate, gate) = tokio::sync::oneshot::channel();
    let adapter = Arc::new(GatedCore {
        client,
        calls: Default::default(),
        gate: Mutex::new(Some(gate)),
    });
    let (relay, _rx) = Relay::new();
    cx.update(|cx| cx.set_global(CoreEvents::new(relay.clone())));
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let mut row = Row::command("ext", "Fixture invalidation");
            row.action = Action::RunExtensionCommand {
                extension_id: "raycast.m09-native-fixtures".into(),
                command_name: "invalidation".into(),
                extra: serde_json::from_value(json!({})).unwrap(),
            };
            seed(v, vec![row]);
            v.input.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("enter");
    settle(&rt, cx);
    assert!(
        adapter
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c == "extension:run-command"),
        "probe precondition: real activation reached the production CoreClient"
    );
    match discard {
        "hide" => cx.update(|w, cx| view.update(cx, |v, cx| v.hide(w, cx))),
        "query" => cx.update(|w, cx| view.update(cx, |v, cx| v.edit("newer query", w, cx))),
        _ => unreachable!(),
    }
    settle(&rt, cx);
    let _ = open_gate.send(()); // a hidden launcher already aborted its waiter; harmless
    // Real core run completes; the fixture's effect timer fires 100 ms after render.
    // A cold worker start (Bun + esbuild compile) can exceed the observation window on
    // slower hosts such as Windows: wait for the run's first session event, bounded.
    let session_event = || {
        fixture.host.events.lock().unwrap().iter().any(|(c, _)| {
            c == "extension:session-disposed" || c == "extension:session-invalidated"
        })
    };
    for _ in 0..400 {
        if session_event() {
            break;
        }
        core_rt.block_on(async { tokio::time::sleep(std::time::Duration::from_millis(25)).await });
        settle(&rt, cx);
    }
    for _ in 0..40 {
        core_rt.block_on(async { tokio::time::sleep(std::time::Duration::from_millis(25)).await });
        settle(&rt, cx);
    }
    let events = fixture.host.events.lock().unwrap().clone();
    let pick = |name: &str| {
        events
            .iter()
            .filter(|(c, _)| c == name)
            .filter_map(|(_, p)| p["sessionId"].as_str().map(str::to_owned))
            .collect::<Vec<_>>()
    };
    let invalidated = pick("extension:session-invalidated");
    let disposed_before = pick("extension:session-disposed");
    // Ask the REAL core whether it still owns the run's session (true = never disposed).
    let still_owned = invalidated.first().or(disposed_before.first()).map(|id| {
        core_rt.block_on(
            fixture
                .transport
                .core
                .request("extension:dispose-session", &[json!({"sessionId":id})]),
        )
    });
    let calls = adapter.calls.lock().unwrap().clone();
    core_rt.block_on(fixture.transport.shutdown());
    drop(fixture);
    (invalidated, disposed_before, still_owned, calls)
}
#[gpui::test]
fn review_m9_followup_d13_real_worker_hidden_launcher_orphans_live_session(
    cx: &mut TestAppContext,
) {
    let (invalidated, disposed_before, still_owned, calls) = real_worker_case(cx, "hide");
    eprintln!(
        "OPUS-D13 real/hide: invalidated={invalidated:?} disposed_events_before={disposed_before:?} core_still_owned={still_owned:?} launcher_calls={calls:?}"
    );
    assert!(
        !disposed_before.is_empty(),
        "hidden run must be disposed before its effect timer continues"
    );
    assert_eq!(
        still_owned,
        Some(Ok(json!(false))),
        "hidden launcher left the run's core/worker session alive (live effects, no Dispose)"
    );
}
#[gpui::test]
fn review_m9_followup_d13_real_worker_new_query_control_disposes(cx: &mut TestAppContext) {
    let (invalidated, disposed_before, still_owned, calls) = real_worker_case(cx, "query");
    eprintln!(
        "OPUS-D13 real/query control: invalidated={invalidated:?} disposed_events_before={disposed_before:?} core_still_owned={still_owned:?} launcher_calls={calls:?}"
    );
    assert!(calls.iter().any(|c| c == "extension:dispose-session"));
    assert!(!disposed_before.is_empty());
    assert_eq!(still_owned, Some(Ok(json!(false))));
}

#[gpui::test]
fn review_m9_followup_noview_preferences_use_the_admitted_caller_and_visibility(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let adapter = Arc::new(HeldRun::default());
    let (relay, mut rx) = Relay::new();
    let (view, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(adapter.clone(), relay, None, w, cx));
    for hidden in [false, true] {
        cx.update(|w, cx| {
            view.update(cx, |v, cx| {
                let mut row = Row::command("ext", "Extension");
                row.action = Action::RunExtensionCommand {
                    extension_id: "fixture".into(),
                    command_name: "prefs".into(),
                    extra: Default::default(),
                };
                seed(v, vec![row]);
                v.input.focus_handle(cx).focus(w, cx);
            })
        });
        cx.simulate_keystrokes("enter");
        settle(&rt, cx);
        if hidden {
            cx.update(|w, cx| view.update(cx, |v, cx| v.hide(w, cx)));
        }
        adapter.held.lock().unwrap().take().unwrap().send(json!({"ok":true,"mode":"no-view","preferencesRequested":true,"extensionId":"foreign"})).unwrap();
        settle(&rt, cx);
        let mut targets = Vec::new();
        while let Ok(Some(event)) =
            rt.block_on(async { tokio::time::timeout(Duration::from_millis(1), rx.recv()).await })
        {
            if let Event::Navigate(n) = event {
                if let Payload::ExtensionPreferences { extension, command } = n.payload {
                    targets.push((extension, command));
                }
            }
        }
        assert_eq!(
            targets,
            if hidden {
                vec![]
            } else {
                vec![("fixture".into(), None)]
            }
        );
    }
}
