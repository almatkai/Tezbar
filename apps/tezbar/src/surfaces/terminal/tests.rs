//! Synthetic production-state/GPUI transport fixtures. Actual PTY cases live in core.
use super::request::Reply;
use super::state::Purpose;
use super::*;
use ::core::prelude::v1::test;
use alacritty_terminal::{selection::SelectionType, term::TermMode};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::sync::Mutex;
fn summary(id: &str, running: bool) -> Value {
    json!({"sessionId":id,"cwd":"/fixture/Downloads","name":"Downloads · git status","shell":"/synthetic/shell","status":if running{"running"}else{"exited"},"lastCommand":"git status","saveFor":"forever","keepAliveFor":"until-stop","createdAt":1,"updatedAt":2})
}
fn attached(id: &str, output: &str) -> Value {
    json!({"sessionId":id,"summary":summary(id,true),"generation":"gen-a","replay":"token-a","watermark":output.len(),"cwd":"/fixture/Downloads","shell":"/synthetic/shell"})
}
fn replay_value(start: u64, bytes: &[u8], watermark: u64) -> Value {
    json!({"generation":"gen-a","start":start,"end":start+bytes.len() as u64,"watermark":watermark,"bytes":STANDARD.encode(bytes)})
}
fn complete_attach(s: &mut State, w: state::Work, id: &str, output: &str) {
    s.complete(w, Ok(attached(id, output)));
    let w = loop {
        let w = s.next_work().unwrap();
        if matches!(w.purpose, Purpose::Replay) {
            break w;
        }
        s.complete(w, Ok(json!([summary(id, true)])));
    };
    s.complete(
        w,
        Ok(replay_value(0, output.as_bytes(), output.len() as u64)),
    );
}
fn boot() -> Boot {
    Boot {
        command: None,
        cwd: None,
        session: None,
        defaults: Defaults::default(),
    }
}
fn state() -> State {
    {
        let mut s = State::new(Defaults::default(), vec![]);
        s.set_visible(true);
        s
    }
}
fn live() -> State {
    let mut s = state();
    s.listeners_ready(true, true);
    s.activate(boot(), 1);
    let w = s.next_work().unwrap();
    s.complete(w, Ok(json!([summary("a", true), summary("b", true)])));
    let w = s.next_work().unwrap();
    assert!(matches!(&w.purpose,Purpose::Attach(id) if id=="a"));
    complete_attach(&mut s, w, "a", "old\r\n");
    s.work.clear();
    s.visible = true;
    s
}
#[test]
fn replay_startup_deduplicates() {
    replay(
        "old history\nnew prompt\n",
        "new prompt\n",
        "old history\nnew prompt",
    );
}
#[test]
fn replay_preserves_after_snapshot() {
    replay(
        "old history\nnew prompt",
        "new prompt\nnext line",
        "old history\nnew prompt\nnext line",
    );
}
#[test]
fn replay_preserves_unrelated() {
    replay(
        "old history\n",
        "brand new output\n",
        "old history\nbrand new output",
    );
}
fn replay(snapshot: &str, data: &str, expected: &str) {
    let mut s = state();
    s.listeners_ready(true, true);
    s.activate(boot(), 1);
    let w = s.next_work().unwrap();
    s.complete(w, Ok(json!([summary("a", true)])));
    let w = s.next_work().unwrap();
    s.stream_data("a", "gen-a", snapshot.len() as u64, data.as_bytes(), true);
    // Full durable stream carries the ordered bytes; no text-overlap heuristic.
    complete_attach(&mut s, w, "a", expected);
    assert!(s.grid.frame().text.starts_with(expected));
}
#[test]
fn explicit_selection_no_implicit_first() {
    let mut s = state();
    s.sessions
        .replace(serde_json::from_value(json!([summary("a", true)])).unwrap());
    assert_eq!(s.sessions.selected, None);
    assert!(s.work.is_empty());
}
#[test]
fn arrow_down_is_explicit_and_clamps() {
    let mut s = live();
    s.sessions.selected = None;
    s.sessions.down();
    assert_eq!(s.sessions.selected, Some(0));
    s.sessions.down();
    s.sessions.down();
    assert_eq!(s.sessions.selected, Some(1));
}
#[test]
fn disappearance_clears_selection() {
    let mut s = live();
    s.sessions.selected = Some(1);
    s.sessions.replace(vec![]);
    s.sessions.down();
    assert_eq!(s.sessions.selected, None);
    s.sessions
        .replace(serde_json::from_value(json!([summary("a", true)])).unwrap());
    s.sessions.selected = Some(5);
    s.sessions.down();
    assert_eq!(s.sessions.selected, Some(0));
}
#[test]
fn command_control_numbers_select_through_controller() {
    for (code, key, m, index) in [
        (
            "Digit1",
            "1",
            input::Modifiers {
                command: true,
                ..Default::default()
            },
            0,
        ),
        (
            "Numpad9",
            "9",
            input::Modifiers {
                control: true,
                ..Default::default()
            },
            8,
        ),
    ] {
        let k = input::key(key, code, m, TermMode::empty(), false);
        assert_eq!(k, input::KeyResult::Session(index));
        let mut s = live();
        s.handle_key(k);
        if index == 0 {
            assert!(
                s.work
                    .iter()
                    .any(|w| matches!(w.purpose, Purpose::Attach(_)))
            );
        } else {
            assert!(s.work.is_empty());
        }
    }
}
#[test]
fn option_symbols_and_bare_numbers_are_text() {
    for (key, m) in [
        (
            "¡",
            input::Modifiers {
                alt: true,
                ..Default::default()
            },
        ),
        ("1", Default::default()),
    ] {
        assert!(!matches!(
            input::key(key, "Digit1", m, TermMode::empty(), false),
            input::KeyResult::Session(_)
        ));
    }
}
#[test]
fn restore_same_identity_no_legacy_command_execution() {
    let mut s = live();
    s.sessions
        .replace(serde_json::from_value(json!([summary("saved", false)])).unwrap());
    s.select("saved");
    let create = s
        .work
        .iter()
        .find(|w| matches!(w.purpose, Purpose::Create))
        .unwrap();
    let (channel, args) = create.request.clone().wire();
    assert_eq!(channel, "terminal:create");
    assert_eq!(args[0]["restoreSessionId"], "saved");
    assert_eq!(args[0]["restoreCommand"], "git status");
    assert!(args[0].get("initialCommand").is_none());
    assert_eq!(args[0]["saveFor"], "forever");
    assert_eq!(args[0]["keepAliveFor"], "until-stop");
}
#[test]
fn labels_directory_and_separator() {
    let mut s = live();
    s.write("cd Downloads\r".into(), true);
    let (id, cmd, rev) = s.cwd_retries.take().unwrap();
    s.refresh_cwd(id, cmd, rev);
    let w = s.work.pop_back().unwrap();
    s.complete(w, Ok(json!("/Users/test/Downloads")));
    let r = s.work.back().unwrap().request.clone().wire();
    assert_eq!(r.1[0]["name"], "~/Downloads · cd Downloads");
}
#[test]
fn labels_use_directory_tail() {
    assert_eq!(
        tezbar_ui_model::terminal::directory_label("/Users/test/Desktop/code/aml/aml-space"),
        "aml/aml-space"
    );
    assert_eq!(
        tezbar_ui_model::terminal::directory_label("/Users/test"),
        "~"
    );
    assert_eq!(tezbar_ui_model::terminal::directory_label("/"), "/");
}
#[test]
fn labels_use_shell_autocomplete_not_typed_prefix() {
    let mut s = live();
    s.grid.feed(b"\x1b[H\x1b[2Jtest@host ~ % cd Downloads");
    s.write("cd D\r".into(), true);
    let (_, command, _) = s.cwd_retries.take().unwrap();
    assert_eq!(command, "cd Downloads");
}
#[test]
fn both_listeners_before_create_with_metadata_once() {
    let mut s = state();
    let mut b = boot();
    b.command = Some("echo synthetic".into());
    b.cwd = Some("/fixture".into());
    s.activate(b, 1);
    assert!(s.next_work().is_none());
    s.listeners_ready(true, false);
    assert!(s.next_work().is_none());
    s.listeners_ready(false, true);
    let list = s.next_work().unwrap();
    s.complete(list, Ok(json!([])));
    let w = s.next_work().unwrap();
    let (channel, args) = w.request.clone().wire();
    assert_eq!(channel, "terminal:create");
    assert_eq!(args[0]["initialCommand"], "echo synthetic");
    assert_eq!(args[0]["cwd"], "/fixture");
    s.listeners_ready(true, true);
    assert!(s.next_work().is_none());
}
#[test]
fn null_attach_has_no_summary_write() {
    let mut s = state();
    s.listeners_ready(true, true);
    s.activate(boot(), 1);
    let w = s.next_work().unwrap();
    s.complete(w, Ok(json!([summary("gone", true)])));
    let w = s.next_work().unwrap();
    s.complete(w, Ok(Value::Null));
    assert!(
        !s.work
            .iter()
            .any(|w| matches!(w.request, TerminalRequest::Update(_)))
    );
    assert!(s.active.is_none());
}
#[test]
fn data_exit_before_attach_is_not_lost_or_reordered() {
    let mut s = state();
    s.listeners_ready(true, true);
    s.activate(boot(), 1);
    let w = s.next_work().unwrap();
    s.complete(w, Ok(json!([summary("a", true)])));
    let w = s.next_work().unwrap();
    s.stream_data("a", "gen-a", 0, b"first\n", true);
    s.stream_exit("a", "gen-a", 6, 7);
    complete_attach(&mut s, w, "a", "first\n");
    let f = s.grid.frame();
    assert!(f.text.find("first").unwrap() < f.text.find("process exited with code 7").unwrap());
    assert!(!s.running);
    let before = f.text;
    s.stream_data("a", "old-gen", 6, b"late", true);
    assert_eq!(s.grid.frame().text, before);
}
#[test]
fn stale_create_and_dispose_detach_without_delete() {
    let mut s = state();
    s.listeners_ready(true, true);
    s.activate(boot(), 1);
    let w = s.next_work().unwrap();
    s.complete(w, Ok(json!([])));
    let create = s.next_work().unwrap();
    s.dispose();
    s.complete(create, Ok(attached("abandoned", "")));
    let release = s.next_work().unwrap();
    assert!(matches!(release.request, TerminalRequest::Release(_)));
    let cleanup = s.next_work().unwrap();
    assert!(
        matches!(cleanup.request,TerminalRequest::DetachGeneration{ref id,ref generation} if id=="abandoned" && generation=="gen-a")
    );
    assert!(s.next_work().is_none());
}
#[test]
fn stale_attach_cwd_and_config_cannot_transfer_selection() {
    let mut s = live();
    s.configure();
    s.write("pwd\r".into(), true);
    let (id, cmd, revision) = s.cwd_retries.take().unwrap();
    s.refresh_cwd(id, cmd, revision);
    let cwd = s.work.pop_back().unwrap();
    s.select("b");
    s.save_configure();
    s.complete(cwd, Ok(json!("/late")));
    assert!(
        !s.work
            .iter()
            .any(|w| matches!(w.request, TerminalRequest::Update(_)))
    );
    assert!(s.configure.is_none());
}
#[test]
fn detach_is_not_delete_and_pin_persistence_is_typed() {
    let mut s = live();
    s.pin();
    assert!(matches!(s.effects.pop_front(),Some((_,TerminalEffect::Pins(p))) if p==vec!["a"]));
    s.select("b");
    assert!(matches!(
        s.next_work().unwrap().request,
        TerminalRequest::DetachGeneration { .. }
    ));
    assert!(!s.work.iter().any(|w| matches!(
        w.request,
        TerminalRequest::Delete(_) | TerminalRequest::Kill(_)
    )));
}
#[test]
fn bounded_pre_attach_overflow_recovers_from_durable_snapshot() {
    let mut s = state();
    s.listeners_ready(true, true);
    s.activate(boot(), 1);
    let w = s.next_work().unwrap();
    s.complete(w, Ok(json!([summary("a", true)])));
    let w = s.next_work().unwrap();
    // Pre-attach output consumes no UI backlog; the cursor starts at known byte zero.
    for at in 0..128 {
        s.stream_data("a", "gen-a", at * 65536, &vec![b'x'; 65536], true);
    }
    complete_attach(&mut s, w, "a", "durable");
    assert!(s.grid.frame().text.starts_with("durable"));
    assert!(s.work.len() <= 2);
    assert!(s.error.is_none());
}
#[test]
fn key_sequences_app_interception_and_bracketed_paste() {
    let m = input::Modifiers::default();
    for (key, mode, expected) in [
        ("up", TermMode::empty(), "\x1b[A"),
        ("up", TermMode::APP_CURSOR, "\x1bOA"),
        ("enter", TermMode::empty(), "\r"),
        ("backspace", TermMode::empty(), "\x7f"),
        ("f5", TermMode::empty(), "\x1b[15~"),
    ] {
        assert_eq!(
            input::key(key, "", m, mode, false),
            input::KeyResult::Bytes(expected.into())
        );
    }
    for command in [true, false] {
        let m = input::Modifiers {
            command,
            control: !command,
            ..Default::default()
        };
        for (k, want) in [
            ("k", input::KeyResult::Configure),
            ("x", input::KeyResult::Stop),
            ("p", input::KeyResult::Pin),
            ("d", input::KeyResult::Delete),
        ] {
            assert_eq!(input::key(k, "", m, TermMode::empty(), false), want);
        }
    }
    assert_eq!(
        input::key(
            "c",
            "",
            input::Modifiers { control: true, ..m },
            TermMode::empty(),
            false
        ),
        input::KeyResult::Bytes("\x03".into())
    );
    assert_eq!(
        input::paste("a\n\x1bb", TermMode::BRACKETED_PASTE),
        "\x1b[200~a\rb\x1b[201~"
    );
    assert_eq!(
        input::key("escape", "", m, TermMode::empty(), true),
        input::KeyResult::PlatformText
    );
}
#[test]
fn zoom_domains_and_escape_sentinel() {
    let mut s = live();
    for _ in 0..40 {
        s.handle_key(input::KeyResult::TextZoom(input::Zoom::In));
    }
    assert_eq!(s.font_size, 24);
    s.handle_key(input::key(
        "-",
        "",
        input::Modifiers {
            command: true,
            alt: true,
            ..Default::default()
        },
        s.grid.mode(),
        false,
    ));
    assert!(matches!(
        s.effects.pop_front(),
        Some((_, TerminalEffect::Zoom(input::Zoom::Out)))
    ));
    s.configure();
    s.handle_key(input::KeyResult::Back);
    assert!(s.effects.is_empty());
    s.handle_key(input::KeyResult::Back);
    assert_eq!(
        s.effects.pop_front(),
        Some((s.generation, TerminalEffect::Back { sentinel: ">" }))
    );
}
#[test]
fn sgr_mouse_motion_release_and_modifier_table() {
    let mode = TermMode::MOUSE_DRAG | TermMode::SGR_MOUSE;
    let m = input::Modifiers::default();
    assert_eq!(
        input::mouse(input::MouseKind::Press(0), 9, 4, m, mode).as_deref(),
        Some(b"\x1b[<0;10;5M".as_slice())
    );
    assert_eq!(
        input::mouse(input::MouseKind::Release(0), 9, 4, m, mode).as_deref(),
        Some(b"\x1b[<0;10;5m".as_slice())
    );
    assert!(input::mouse(input::MouseKind::Motion(None), 0, 0, m, mode).is_none());
    assert_eq!(
        input::mouse(input::MouseKind::Motion(Some(0)), 0, 0, m, mode).as_deref(),
        Some(b"\x1b[<32;1;1M".as_slice())
    );
    assert!(
        input::mouse(
            input::MouseKind::Press(0),
            0,
            0,
            input::Modifiers { shift: true, ..m },
            mode
        )
        .is_none()
    );
}
#[test]
fn stale_paste_fence_does_not_write_to_new_session() {
    let mut s = live();
    let generation = s.generation;
    s.select("b");
    s.paste(generation, "private fixture");
    assert!(
        !s.work
            .iter()
            .any(|w| matches!(w.request, TerminalRequest::Input { .. }))
    );
}
#[test]
fn large_unicode_paste_is_atomically_admitted_in_core_sized_chunks() {
    let mut s = live();
    let text = "界".repeat(30_000);
    s.write(text.clone(), false);
    let chunks = s
        .work
        .iter()
        .filter_map(|w| match &w.request {
            TerminalRequest::Input { bytes, .. } => Some(bytes.as_slice()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(chunks.len(), 1);
    assert!(chunks.iter().all(|s| s.len() <= 1024 * 1024));
    assert_eq!(chunks.concat(), text.as_bytes());
    let first = s.next_work().unwrap();
    s.complete(first, Ok(json!(false)));
    assert!(s.error.is_some());
    assert!(
        !s.work
            .iter()
            .any(|w| matches!(w.request, TerminalRequest::Input { .. }))
    );
}
#[test]
fn defaults_validate_without_creating_another_settings_owner() {
    for (save, keep) in [
        ("day", "3h"),
        ("week", "8h"),
        ("month", "day"),
        ("forever", "until-stop"),
    ] {
        let defaults = Defaults::from_legacy(&json!({"saveFor":save,"keepAliveFor":keep}));
        let s = State::new(defaults.clone(), vec![]);
        assert_eq!(s.sessions.defaults, defaults);
        assert!(s.work.is_empty());
    }
    assert_eq!(
        Defaults::from_legacy(&json!({"saveFor":"bad","keepAliveFor":null})),
        Defaults::default()
    );
}

#[test]
fn compact_macos_home_preserves_path_tail() {
    assert_eq!(
        tezbar_ui_model::terminal::compact_path("/Users/test/Desktop/code/Raymes"),
        ".../Desktop/code/Raymes"
    );
}
#[test]
fn compact_non_user_paths_unchanged() {
    for path in ["/tmp/project", "~"] {
        assert_eq!(tezbar_ui_model::terminal::compact_path(path), path);
    }
}
#[test]
fn compact_windows_home_matches_display_format() {
    assert_eq!(
        tezbar_ui_model::terminal::compact_path("C:\\Users\\User\\Desktop\\Tezbar"),
        ".../Desktop/Tezbar"
    );
}
#[test]
fn windows_directory_label_relative_to_home() {
    assert_eq!(
        tezbar_ui_model::terminal::directory_label("C:\\Users\\User\\Desktop"),
        "~/Desktop"
    );
}
#[test]
fn explicit_cwd_accepts_posix_drive_and_unc() {
    for path in [
        "/Users/test/Desktop",
        "C:\\Users\\User\\Desktop",
        "C:/Users/User/Desktop",
        "\\\\server\\share",
    ] {
        assert!(tezbar_ui_model::terminal::is_absolute_path(path), "{path}");
    }
}
#[test]
fn explicit_cwd_rejects_relative_paths() {
    for path in ["Desktop/project", "Users/User/Desktop"] {
        assert!(!tezbar_ui_model::terminal::is_absolute_path(path));
    }
}

#[derive(Default)]
struct Fixture {
    calls: Mutex<Vec<(&'static str, Vec<Value>)>>,
}
impl TerminalTransport for Fixture {
    fn send(
        &self,
        r: TerminalRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Reply> + Send>> {
        let (channel, args) = r.wire();
        self.calls.lock().unwrap().push((channel, args.clone()));
        let value = match channel {
            "terminal:list" => json!([summary("a", true)]),
            "core:terminal:stream-attach" => attached("a", "fixture\r\n"),
            "core:terminal:replay-read" => {
                let at = args[0]["position"].as_u64().unwrap();
                replay_value(at, &b"fixture\r\n"[at as usize..], 9)
            }
            _ => json!(true),
        };
        Box::pin(async move { Ok(value) })
    }
}
fn runtime(cx: &mut TestAppContext) -> tokio::runtime::Runtime {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| gpui_tokio::init_from_handle(cx, rt.handle().clone()));
    rt
}
fn settle(rt: &tokio::runtime::Runtime, cx: &mut VisualTestContext) {
    rt.block_on(async {
        for _ in 0..64 {
            cx.run_until_parked();
            cx.executor().advance_clock(Duration::from_millis(2));
            tokio::task::yield_now().await;
        }
    });
}
fn payload() -> Payload {
    Payload::Terminal {
        command: None,
        cwd: None,
        session: None,
        defaults: Default::default(),
    }
}
#[gpui::test]
fn gpui_readonly_render_never_starts_transport(cx: &mut TestAppContext) {
    let _rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| TerminalView::new(fixture.clone(), &UiState::default(), w, cx));
    cx.run_until_parked();
    assert!(fixture.calls.lock().unwrap().is_empty());
    assert!(view.read_with(cx, |v, _| v.is_quiescent()));
}
#[gpui::test]
fn gpui_hide_settings_retains_grid_selection_and_stops_blink(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| TerminalView::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.listeners_ready(true, true, cx);
            v.activate(payload(), 1, w, cx);
        })
    });
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.state.grid.start_selection(0, 0, SelectionType::Simple);
            v.state.grid.extend_selection(3, 0);
            let selected = v.state.grid.selected_text();
            let text = v.state.grid.frame().text;
            v.set_visible(false, w, cx);
            assert!(v.blink.is_none());
            assert!(v.cwd_timer.is_none());
            v.set_visible(true, w, cx);
            assert_eq!(v.state.grid.selected_text(), selected);
            assert_eq!(v.state.grid.frame().text, text);
        })
    });
    settle(&rt, cx);
    assert!(
        view.read_with(cx, |v, _| v.error().is_none()),
        "resume must settle without replay error"
    );
    assert_eq!(
        fixture
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| *c == "core:terminal:stream-detach")
            .count(),
        1
    );
}
#[gpui::test]
fn gpui_native_ime_commit_copy_url_and_dispose_use_fake_host(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| TerminalView::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.listeners_ready(true, true, cx);
            v.activate(payload(), 1, w, cx);
        })
    });
    settle(&rt, cx);
    let before = fixture.calls.lock().unwrap().len();
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.geometry = Some((
                Bounds::new(point(px(10.), px(20.)), size(px(560.), px(300.))),
                CellMetrics {
                    width: 7.,
                    height: 14.64,
                },
            ));
            v.replace_and_mark_text_in_range(None, "に😀", Some(1..3), w, cx);
            assert!(v.blink.is_none());
            assert_eq!(v.marked_text_range(w, cx), Some(0..3));
            assert!(v.bounds_for_range(1..3, Bounds::default(), w, cx).is_some());
            assert!(v.state.work.is_empty());
        })
    });
    assert_eq!(fixture.calls.lock().unwrap().len(), before);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.replace_text_in_range(None, "日本😀", w, cx);
            assert!(v.preedit.text.is_empty());
            v.state.grid.start_selection(0, 0, SelectionType::Simple);
            v.state.grid.extend_selection(3, 0);
            v.act(input::KeyResult::Copy, w, cx);
            let effects = v.drain();
            assert!(matches!(&effects[0].1,TerminalEffect::Copy(text) if text=="fixt"));
            v.state
                .emit(TerminalEffect::OpenUrl("https://example.org/".into()));
            assert!(matches!(&v.drain()[0].1, TerminalEffect::OpenUrl(_)));
        })
    });
    settle(&rt, cx);
    let writes = fixture
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == "core:terminal:input")
        .map(|(_, args)| {
            json!(
                String::from_utf8(STANDARD.decode(args[0]["bytes"].as_str().unwrap()).unwrap())
                    .unwrap()
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(writes, vec![json!("日本😀")]);
    cx.update(|_, cx| view.update(cx, |v, cx| v.dispose(cx)));
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| v.is_quiescent()));
    assert_eq!(
        fixture
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| *c == "core:terminal:stream-detach")
            .count(),
        1
    );
}
#[gpui::test]
fn gpui_key_interception_uses_terminal_owner(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| TerminalView::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.listeners_ready(true, true, cx);
            v.activate(payload(), 1, w, cx);
        })
    });
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let event = KeyDownEvent {
                keystroke: Keystroke::parse("cmd-k").unwrap(),
                is_held: false,
                prefer_character_input: false,
            };
            v.key(&event, w, cx);
            assert!(v.state.configure.is_some());
            v.key(
                &KeyDownEvent {
                    keystroke: Keystroke::parse("escape").unwrap(),
                    ..event
                },
                w,
                cx,
            );
            assert!(v.state.configure.is_none());
            assert!(v.drain().is_empty());
        })
    });
}

#[test]
fn ordered_stream_repeated_bytes_slow_replay_utf8_vt_and_generation_fences() {
    let mut s = live();
    s.stream_data("a", "gen-a", 6, b"same\r\n", true); // gap: no text dedupe and no byte backlog
    assert!(s.work.iter().any(|w| matches!(w.purpose, Purpose::Replay)));
    s.work.clear();
    // Resume starts at the known parser position, preserving its selection and VT state.
    let data = b"old\r\nsame\r\nsame\r\n\x1b[31m\xe7\x95\x8c\x1b[0m";
    let w = state::Work {
        generation: s.generation,
        request: TerminalRequest::Replay {
            token: "token-a".into(),
            position: 5,
        },
        purpose: Purpose::Replay,
    };
    s.complete(
        w,
        Ok(replay_value(5, &data[5..data.len() - 4], data.len() as u64)),
    );
    s.stream_data("a", "gen-a", data.len() as u64, b"same", true);
    let w = s.next_work().unwrap();
    let mut end = data[data.len() - 4..].to_vec();
    end.extend(b"same");
    s.complete(
        w,
        Ok(replay_value(
            (data.len() - 4) as u64,
            &end,
            (data.len() + 4) as u64,
        )),
    );
    assert!(s.grid.frame().text.starts_with("old\nsame\nsame\n界same"));
    let before = s.grid.frame().text;
    s.stream_data("a", "old-generation", 0, b"WRONG", true);
    s.stream_exit("a", "old-generation", 0, 9);
    assert_eq!(s.grid.frame().text, before);
    s.stream_data("a", "gen-a", 5, b"same\r\n", true); // exact positional duplicate
    assert_eq!(s.grid.frame().text, before);
}
#[test]
fn replay_historical_queries_suppressed_live_queries_answered_and_expiry_recovers() {
    let mut s = live();
    s.work.clear();
    s.set_visible(false);
    s.set_visible(true);
    let detach = s.next_work().unwrap();
    s.complete(detach, Ok(json!(true)));
    let attach = s.next_work().unwrap();
    s.complete(attach, Ok(attached("a", "old\r\n\x1b[6n")));
    let w = loop {
        let w = s.next_work().unwrap();
        if matches!(w.purpose, Purpose::Replay) {
            break w;
        }
        assert!(w.cleanup());
        s.complete(w, Ok(json!(true)));
    };
    s.complete(w, Ok(replay_value(5, b"\x1b[6n\x1b[6n", 13)));
    let inputs = s
        .work
        .iter()
        .filter(|w| matches!(w.request, TerminalRequest::Input { .. }))
        .count();
    assert_eq!(inputs, 1, "only query beyond snapshot watermark replies");
    s.work.clear();
    s.stream_data("a", "gen-a", 20, b"gap", true);
    let w = s.next_work().unwrap();
    s.complete(w, Err("Terminal replay expired or released".into()));
    assert!(
        s.work
            .iter()
            .any(|w| matches!(w.purpose, Purpose::Resume(_)))
    );
}
#[test]
fn raw_mouse_bytes_and_atomic_whole_paste_no_prefix() {
    let bytes = input::mouse(
        input::MouseKind::Press(0),
        200,
        200,
        Default::default(),
        TermMode::MOUSE_REPORT_CLICK,
    )
    .unwrap();
    assert_eq!(bytes, [27, b'[', b'M', 32, 233, 233]);
    let mut s = live();
    s.write_bytes(bytes.clone());
    assert!(
        matches!(&s.work.front().unwrap().request,TerminalRequest::Input{bytes:b,..} if *b==bytes)
    );
    s.work.clear();
    s.grid.feed(b"\x1b[?2004h");
    s.paste(s.generation, &"x".repeat(1024 * 1024));
    assert!(s.work.is_empty());
    assert!(s.error.is_some());
}
#[gpui::test]
fn actual_gpui_focus_key_dispatch_resize_and_key_up_do_not_duplicate(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let f = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| TerminalView::new(f.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.listeners_ready(true, true, cx);
            v.activate(payload(), 1, w, cx);
        })
    });
    settle(&rt, cx);
    cx.simulate_keystrokes("cmd-k");
    assert!(view.read_with(cx, |v, _| v.state.configure.is_some()));
    cx.simulate_keystrokes("escape up");
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| v.state.configure.is_none()));
    let writes = f
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == "core:terminal:input")
        .map(|(_, a)| STANDARD.decode(a[0]["bytes"].as_str().unwrap()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(writes, vec![b"\x1b[A".to_vec()]);
    assert!(
        f.calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| *c == "terminal:resize")
    );
}

#[test]
fn repair_quit_preparation_preserves_terminal_and_resumes_binary_input() {
    let mut s = live();
    let grid = s.grid.frame().text;
    s.quit_preparing = true;
    s.write_bytes(b"blocked".to_vec());
    s.new_session();
    s.select("b");
    assert!(s.work.is_empty());
    assert_eq!(s.active.as_deref(), Some("a"));
    s.set_visible(false);
    assert!(!s.disposed);
    while let Some(work) = s.next_work() {
        s.complete(work, Ok(json!(true)));
    }
    s.quit_preparing = false;
    s.set_visible(true);
    let w = s.next_work().unwrap();
    s.complete(w, Ok(attached("a", "old\r\n")));
    while let Some(w) = s.next_work() {
        match w.purpose {
            Purpose::Replay => s.complete(w, Ok(replay_value(5, b"", 5))),
            _ => s.complete(w, Ok(json!(true))),
        }
    }
    assert_eq!(s.grid.frame().text, grid);
    s.write_bytes(vec![0x1b, 0x5b, 0x4d, 0xff]);
    assert!(
        matches!(s.next_work().unwrap().request,TerminalRequest::Input{bytes,..} if bytes==[0x1b,0x5b,0x4d,0xff])
    );
}
