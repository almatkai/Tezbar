//! Opus review probes on the actual Bun worker -> initialized NativeCore/TestHost -> RuntimeView.
//! Uses an owned prepared seed (TEZBAR_M09_PREPARED) containing the M9 fixture plus probe commands.
use super::*;
use ::core::prelude::v1::test;
#[path = "../../../../../tests/fixtures/extensions/native_harness.rs"]
mod native_harness;
use native_harness::Fixture;
fn prepared() -> (std::path::PathBuf, std::path::PathBuf) {
    (
        std::env::var_os("TEZBAR_M09_PREPARED")
            .expect("TEZBAR_M09_PREPARED")
            .into(),
        std::env::var_os("TEZBAR_M09_BUN")
            .expect("TEZBAR_M09_BUN")
            .into(),
    )
}
fn core_rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}
fn settle(rt: &tokio::runtime::Runtime, cx: &mut VisualTestContext, view: &Entity<RuntimeView>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        rt.block_on(async { tokio::task::yield_now().await });
        cx.run_until_parked();
        if view.read(&cx.app.borrow()).job.is_none() {
            break;
        }
        assert!(Instant::now() < deadline, "probe operation timed out");
        rt.block_on(async { tokio::time::sleep(Duration::from_millis(5)).await });
    }
}
#[test]
fn review_m9_noview_sdk_controls() {
    let rt = core_rt();
    let (prepared, bun) = prepared();
    let fixture = rt
        .block_on(Fixture::isolated(&prepared, bun, rt.handle().clone()))
        .unwrap();
    let mut results = vec![];
    for cmd in [
        "probe-noview-plain",
        "probe-noview-close",
        "probe-noview-prefs",
        "probe-noview-clear",
    ] {
        let r = rt.block_on(fixture.transport.run(cmd));
        eprintln!("OPUS-PROBE noview {cmd}: {r:?}");
        results.push((cmd, r));
    }
    eprintln!(
        "OPUS-PROBE noview host calls: {:?}",
        fixture.host.calls.lock().unwrap()
    );
    assert!(fixture.host.events.lock().unwrap().iter().any(|(channel,payload)|channel=="extension:ui"&&payload["kind"]=="preferences"),"in-flight no-view identity must be admitted by the real core");
    rt.block_on(fixture.transport.shutdown());
    for (cmd, r) in results {
        let r = r.expect("transport");
        assert_eq!(
            r["preferencesRequested"] == true,
            cmd == "probe-noview-prefs",
            "{cmd}"
        );
        assert_eq!(
            r["ok"], true,
            "{cmd} must complete like Raycast no-view commands: {r}"
        );
    }
}
fn push_case(cx: &mut TestAppContext, type_during_action: bool) {
    let core = core_rt();
    let (prepared, bun) = prepared();
    let fixture = core
        .block_on(Fixture::isolated(&prepared, bun, core.handle().clone()))
        .unwrap();
    let _guard = core.enter();
    let assets = Arc::new(crate::core_client::CoreClient::new(
        fixture.transport.core.clone(),
    ));
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
        .block_on(fixture.transport.run("probe-push"))
        .expect("probe run");
    let (view, cx) =
        cx.add_window_view(|w, cx| RuntimeView::new(fixture.transport.clone(), assets, w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate(reply, 1, w, cx).unwrap()));
    cx.run_until_parked();
    cx.update(|w, cx| view.update(cx, |v, cx| v.event(UiEvent::Primary, w, cx)));
    let in_flight = view.read_with(cx, |v, _| v.job.is_some());
    if type_during_action {
        cx.update(|w, cx| view.update(cx, |v, cx| v.search_text("a".into(), w, cx)));
    }
    settle(&rt, cx, &view);
    // Let any invalidation/coalesced refresh run, then sample twice.
    for _ in 0..20 {
        rt.block_on(async { tokio::time::sleep(Duration::from_millis(10)).await });
        cx.run_until_parked();
    }
    settle(&rt, cx, &view);
    let observed = view.read_with(cx, |v, _| {
        let s = v.session.as_ref().unwrap();
        (
            format!("{:?}", s.root.kind),
            s.root.string("markdown").map(str::to_owned),
            format!("{:?}", s.notice),
            v.error,
        )
    });
    eprintln!(
        "OPUS-PROBE push type_during_action={type_during_action} in_flight={in_flight} observed={observed:?} refreshes={}",
        fixture.transport.count("extension:refresh-session")
    );
    // Second action against what the native screen shows.
    cx.update(|w, cx| view.update(cx, |v, cx| v.event(UiEvent::Primary, w, cx)));
    settle(&rt, cx, &view);
    let second = view.read_with(cx, |v, _| {
        (
            v.error,
            v.session.as_ref().map(|s| format!("{:?}", s.root.kind)),
        )
    });
    eprintln!(
        "OPUS-PROBE push second action -> {second:?} last_invoke={:?}",
        fixture.transport.last("extension:invoke-action")
    );
    cx.update(|_, cx| view.update(cx, |v, cx| v.dispose(cx)));
    cx.run_until_parked();
    rt.block_on(fixture.transport.drain()).unwrap();
    rt.block_on(fixture.transport.shutdown());
    assert!(in_flight, "probe precondition: slow push must be in flight");
    assert_eq!(
        observed.0, "Detail",
        "worker pushed Detail; native must not stay on stale root"
    );
}
#[gpui::test]
fn review_m9_slow_push_control(cx: &mut TestAppContext) {
    push_case(cx, false);
}
#[gpui::test]
fn review_m9_search_during_slow_push(cx: &mut TestAppContext) {
    push_case(cx, true);
}

struct NoTransport;
impl ExtensionTransport for NoTransport {
    fn send(&self, _r: ExtensionRequest) -> ExtensionFuture {
        Box::pin(async { Ok(json!({"ok":true,"mode":"unchanged"})) })
    }
    fn drain(&self) -> ExtensionFuture {
        Box::pin(async { Ok(json!(true)) })
    }
}
struct NoAssets;
impl ExtensionAssets for NoAssets {
    fn scope(&self, _: String) -> assets::AssetFuture<AssetScope> {
        Box::pin(async { Err("no assets in probe".into()) })
    }
    fn load(
        &self,
        _: AssetScope,
        _: tezbar_ui_model::extensions::images::Source,
        _: tezbar_ui_model::extensions::images::Effect,
    ) -> assets::AssetFuture<assets::AssetResult> {
        Box::pin(async { Err("no assets in probe".into()) })
    }
}
/// Synthetic tree, no worker: Grid without `columns` (TS renderer default 4).
#[gpui::test]
fn review_m9_grid_default_columns(cx: &mut TestAppContext) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| {
        gpui_tokio::init_from_handle(cx, rt.handle().clone());
        tezbar_ui::text_input::bind_keys(cx);
    });
    let items: Vec<Value> = (0..8).map(|i| json!({"type":"Grid.Item","props":{"id":format!("g{i}"),"title":format!("G{i}"),"content":{"color":"#336699"}},"children":[]})).collect();
    let reply = json!({"ok":true,"mode":"view","extensionId":"probe","commandName":"grid","sessionId":"s","generation":"s","refreshMs":null,"root":{"type":"Grid","props":{"navigationTitle":"Grid"},"children":items},"actions":[]});
    let (view, cx) = cx.add_window_view(|w, cx| {
        RuntimeView::new(Arc::new(NoTransport), Arc::new(NoAssets), w, cx)
    });
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate(reply, 1, w, cx).unwrap()));
    cx.run_until_parked();
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.search.update(cx, |f, cx| f.set_text("abc", w, cx));
            v.search.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("left X");
    assert_eq!(
        view.read_with(cx, |v, cx| v.search.read(cx).content.to_string()),
        "abXc"
    );
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.search.update(cx, |f, cx| f.set_text("", w, cx));
            v.search_text("".into(), w, cx);
        })
    });
    cx.simulate_keystrokes("down");
    let selected = view.read_with(cx, |v, _| v.session.as_ref().unwrap().selection.clone());
    eprintln!("OPUS-PROBE grid default columns: down from g0 selects {selected:?}");
    assert_eq!(
        selected.as_deref(),
        Some("g4"),
        "Grid without columns must use the shipping default of 4 columns"
    );
}
#[test]
fn review_m9_date_default_local() {
    let rt = core_rt();
    let (prepared, bun) = prepared();
    let fixture = rt
        .block_on(Fixture::isolated(&prepared, bun, rt.handle().clone()))
        .unwrap();
    let r = rt.block_on(fixture.transport.run("probe-date")).unwrap();
    let raw: Vec<_> = r["root"]["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["props"]["defaultValue"].clone())
        .collect();
    let s = RuntimeSession::new(TreeReply::from_wire(r.clone()).unwrap(), 0).unwrap();
    let values = s.form.values().clone();
    eprintln!(
        "OPUS-PROBE date TZ={:?} raw_defaults={raw:?} form_values={values:?}",
        std::env::var("TZ").ok()
    );
    rt.block_on(fixture.transport.shutdown());
    assert_eq!(
        values["local"], "2026-11-07",
        "local-midnight default must keep its local calendar date"
    );
    assert_eq!(
        values["evening"], "2026-11-07",
        "local evening default must keep its local calendar date"
    );
    assert_eq!(values["datetime"], "2026-11-07T23:30");
}
#[test]
fn review_m9_menubar_info_items() {
    let rt = core_rt();
    let (prepared, bun) = prepared();
    let fixture = rt
        .block_on(Fixture::isolated(&prepared, bun, rt.handle().clone()))
        .unwrap();
    let r = rt
        .block_on(fixture.transport.run("probe-menubar-info"))
        .unwrap();
    let spec = fixture
        .host
        .status_specs
        .lock()
        .unwrap()
        .values()
        .next()
        .cloned();
    let summary = spec.as_ref().map(|s| {
        (
            s.title.clone(),
            s.icon.is_some(),
            s.items
                .iter()
                .map(|i| i["title"].clone())
                .collect::<Vec<_>>(),
        )
    });
    eprintln!(
        "OPUS-PROBE menubar-info reply_root_icon={:?} status={summary:?}",
        r["root"]["props"]["icon"]
    );
    rt.block_on(fixture.transport.shutdown());
    let (title, icon, items) = summary.expect("status item set");
    assert!(
        title.is_empty(),
        "an icon-only root must not duplicate its glyph as text"
    );
    assert!(
        icon,
        "the native status boundary must receive pixels on Windows too; a title alone is insufficient"
    );
    assert!(
        icon || !title.is_empty(),
        "icon-only MenuBarExtra produced a blank status item"
    );
    assert!(
        items.iter().any(|t| t == "CPU 23%"),
        "display-only MenuBarExtra.Item dropped from the status menu"
    );
}
/// Synthetic tree, no worker: List search caret must move with Left like the TS List renderer.
#[gpui::test]
fn review_m9_list_search_left_arrow_moves_caret(cx: &mut TestAppContext) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| {
        gpui_tokio::init_from_handle(cx, rt.handle().clone());
        tezbar_ui::text_input::bind_keys(cx);
    });
    let items: Vec<Value> = (0..4).map(|i| json!({"type":"List.Item","props":{"id":format!("r{i}"),"title":format!("abc {i}")},"children":[]})).collect();
    let reply = json!({"ok":true,"mode":"view","extensionId":"probe","commandName":"list","sessionId":"s","generation":"s","refreshMs":null,"root":{"type":"List","props":{"searchBarPlaceholder":"Search"},"children":items},"actions":[]});
    let (view, cx) = cx.add_window_view(|w, cx| {
        RuntimeView::new(Arc::new(NoTransport), Arc::new(NoAssets), w, cx)
    });
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate(reply, 1, w, cx).unwrap()));
    cx.run_until_parked();
    cx.simulate_keystrokes("a b c");
    cx.run_until_parked();
    let before = view.read_with(cx, |v, cx| {
        (
            v.search.read(cx).content.to_string(),
            v.session.as_ref().unwrap().selection.clone(),
        )
    });
    cx.simulate_keystrokes("down left X");
    cx.run_until_parked();
    let after = view.read_with(cx, |v, cx| {
        (
            v.search.read(cx).content.to_string(),
            v.session.as_ref().unwrap().selection.clone(),
        )
    });
    eprintln!("OPUS-PROBE list search caret: before={before:?} after_down_left_X={after:?}");
    assert_eq!(
        after.0, "abXc",
        "Left must move the caret in the focused List search field"
    );
}
#[gpui::test]
fn review_m9_inline_setup_then_noview_command(cx: &mut TestAppContext) {
    let core = core_rt();
    let (prepared, bun) = prepared();
    let fixture = core
        .block_on(Fixture::isolated(&prepared, bun, core.handle().clone()))
        .unwrap();
    let _guard = core.enter();
    let assets = Arc::new(crate::core_client::CoreClient::new(
        fixture.transport.core.clone(),
    ));
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| {
        gpui_tokio::init_from_handle(cx, rt.handle().clone());
        tezbar_ui::text_input::bind_keys(cx);
    });
    let reply = rt
        .block_on(fixture.transport.run("probe-noview-pref"))
        .expect("probe run");
    eprintln!(
        "OPUS-PROBE setup first reply mode={} root={}",
        reply["mode"], reply["root"]["type"]
    );
    let (view, cx) =
        cx.add_window_view(|w, cx| RuntimeView::new(fixture.transport.clone(), assets, w, cx));
    let back = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let captured = back.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event: &ExtensionEffectEmission, _| {
            if matches!(event.effect, ExtensionEffect::Back) {
                captured.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        })
    });
    cx.update(|w, cx| view.update(cx, |v, cx| v.activate(reply, 1, w, cx).unwrap()));
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            let key = v
                .preference
                .model
                .fields
                .iter()
                .find(|f| f.name == "token")
                .unwrap()
                .key();
            v.edit_preference(key, json!("synthetic-token"), cx);
            v.save_setup(cx);
        })
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while view.read(&cx.app.borrow()).preference.saving {
        rt.block_on(async { tokio::time::sleep(Duration::from_millis(5)).await });
        cx.run_until_parked();
        assert!(Instant::now() < deadline);
    }
    let copies = fixture.desktop.count("clipboard:write");
    let state = view.read_with(cx, |v, _| {
        (
            v.error,
            v.session.as_ref().map(|s| format!("{:?}", s.root.kind)),
        )
    });
    eprintln!("OPUS-PROBE setup after save: side_effect_copies={copies} view_state={state:?}");
    rt.block_on(fixture.transport.shutdown());
    assert_eq!(
        copies, 1,
        "probe precondition: no-view command ran once after setup"
    );
    assert_eq!(back.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_ne!(
        state.0,
        Some("Preferences saved. Run the command again."),
        "user is told to rerun a command that already executed"
    );
}
