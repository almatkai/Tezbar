use super::*;

#[derive(Default)]
struct ReviewTransport {
    calls: Mutex<Vec<(&'static str, Vec<Value>)>>,
    output: Mutex<Vec<u8>>,
    replay_gate: Mutex<Option<Arc<tokio::sync::Notify>>>,
}
impl TerminalTransport for ReviewTransport {
    fn send(
        &self,
        r: TerminalRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Reply> + Send>> {
        let (channel, args) = r.wire();
        self.calls.lock().unwrap().push((channel, args.clone()));
        let out = self.output.lock().unwrap().clone();
        let value = match channel {
            "terminal:list" => json!([summary("a", true)]),
            "core:terminal:stream-attach" => {
                let mut v = attached("a", "");
                v["watermark"] = json!(out.len());
                v
            }
            "core:terminal:replay-read" => {
                let at = args[0]["position"].as_u64().unwrap() as usize;
                replay_value(at as u64, &out[at..], out.len() as u64)
            }
            _ => json!(true),
        };
        let gate = (channel == "core:terminal:replay-read")
            .then(|| self.replay_gate.lock().unwrap().clone())
            .flatten();
        Box::pin(async move {
            if let Some(gate) = gate {
                gate.notified().await;
            }
            Ok(value)
        })
    }
}
fn inputs(calls: &[(&'static str, Vec<Value>)]) -> Vec<Vec<u8>> {
    calls
        .iter()
        .filter(|(c, _)| *c == "core:terminal:input")
        .map(|(_, a)| STANDARD.decode(a[0]["bytes"].as_str().unwrap()).unwrap())
        .collect()
}
fn queued_inputs(s: &State) -> Vec<Vec<u8>> {
    s.work
        .iter()
        .filter_map(|w| match &w.request {
            TerminalRequest::Input { bytes, .. } => Some(bytes.clone()),
            _ => None,
        })
        .collect()
}
fn terminal_view<'a>(
    cx: &'a mut TestAppContext,
    output: &[u8],
) -> (
    tokio::runtime::Runtime,
    Arc<ReviewTransport>,
    Entity<TerminalView>,
    &'a mut VisualTestContext,
) {
    let rt = runtime(cx);
    let fixture = Arc::new(ReviewTransport::default());
    *fixture.output.lock().unwrap() = output.to_vec();
    let (view, cx) =
        cx.add_window_view(|w, cx| TerminalView::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.listeners_ready(true, true, cx);
            v.activate(payload(), 1, w, cx);
        })
    });
    settle(&rt, cx);
    (rt, fixture, view, cx)
}

// F1: real window dispatch (GPUI TestPlatform keystroke path), not input::key().
#[cfg(target_os = "macos")]
#[gpui::test]
fn review_m810_p1_real_dispatch_ctrl_letters_reach_shell_and_cmd_letters_act(
    cx: &mut TestAppContext,
) {
    let (rt, f, view, cx) = terminal_view(cx, b"$ ");
    f.calls.lock().unwrap().clear();
    cx.simulate_keystrokes("ctrl-d ctrl-x ctrl-k ctrl-p");
    settle(&rt, cx);
    let calls = f.calls.lock().unwrap().clone();
    let channels: Vec<&str> = calls.iter().map(|(c, _)| *c).collect();
    println!(
        "OPUS_P1 ctrl inputs={:?} channels={channels:?}",
        inputs(&calls)
    );
    assert_eq!(
        inputs(&calls),
        vec![vec![4u8], vec![24], vec![11], vec![16]]
    );
    assert!(
        !channels
            .iter()
            .any(|c| *c == "terminal:delete" || *c == "terminal:kill")
    );
    assert!(view.read_with(cx, |v, _| v.state.configure.is_none()
        && v.state.effects.is_empty()));
    cx.simulate_keystrokes("cmd-p");
    let effects = cx.update(|_, cx| view.update(cx, |v, _| v.drain()));
    println!("OPUS_P1 cmd-p effects={effects:?}");
    assert!(matches!(effects.as_slice(), [(_, TerminalEffect::TogglePin(id))] if id == "a"));
    f.calls.lock().unwrap().clear();
    cx.simulate_keystrokes("cmd-d");
    settle(&rt, cx);
    let channels: Vec<&str> = f.calls.lock().unwrap().iter().map(|(c, _)| *c).collect();
    println!("OPUS_P1 cmd-d channels={channels:?}");
    assert!(channels.contains(&"terminal:delete"));
}

// F2: real GPUI dispatch_keystroke: unhandled KeyDown -> platform input handler key_char.
#[cfg(target_os = "macos")]
#[gpui::test]
fn review_m810_p2_real_dispatch_option_key_char_is_committed_once_without_meta(
    cx: &mut TestAppContext,
) {
    let (rt, f, _view, cx) = terminal_view(cx, b"$ ");
    f.calls.lock().unwrap().clear();
    let window = cx.update(|w, _| w.window_handle());
    for (key, ch) in [("l", "@"), ("1", "¡"), ("8", "[")] {
        cx.dispatch_keystroke(
            window,
            Keystroke {
                key: key.into(),
                key_char: Some(ch.into()),
                modifiers: gpui::Modifiers {
                    alt: true,
                    ..Default::default()
                },
            },
        );
    }
    settle(&rt, cx);
    let inputs = inputs(&f.calls.lock().unwrap());
    println!("OPUS_P2 option inputs={inputs:?}");
    assert_eq!(
        inputs,
        vec![
            "@".as_bytes().to_vec(),
            "¡".as_bytes().to_vec(),
            b"[".to_vec()
        ]
    );
}

// F3: real ScrollEvent dispatch through the terminal cells hitbox.
#[gpui::test]
fn review_m810_p3_real_wheel_fractional_alt_screen_and_horizontal_mouse_mode(
    cx: &mut TestAppContext,
) {
    // Alternate screen (Alacritty default keeps ALTERNATE_SCROLL), no mouse mode.
    let (rt, f, view, cx) = terminal_view(cx, b"\x1b[?1049h$ ");
    let (bounds, metrics) = view.read_with(cx, |v, _| v.geometry.expect("painted geometry"));
    let at = bounds.origin + point(px(3.), px(3.));
    f.calls.lock().unwrap().clear();
    let wheel = |cx: &mut VisualTestContext, x: f32, y: f32| {
        cx.simulate_event(ScrollEvent {
            position: at,
            delta: ScrollDelta::Pixels(point(px(x), px(y))),
            ..Default::default()
        })
    };
    let step = metrics.height / 3.;
    wheel(cx, 0., step);
    wheel(cx, 0., step);
    settle(&rt, cx);
    let before = inputs(&f.calls.lock().unwrap());
    wheel(cx, 0., step * 1.2);
    settle(&rt, cx);
    let after = inputs(&f.calls.lock().unwrap());
    println!(
        "OPUS_P3 alt h={} before={before:?} after={after:?}",
        metrics.height
    );
    assert!(before.is_empty());
    assert_eq!(after, vec![b"\x1b[A".to_vec()]);
    // Horizontal-only swipe in SGR mouse mode sends nothing.
    view.update(cx, |v, _| v.state.grid.feed(b"\x1b[?1000h\x1b[?1006h"));
    f.calls.lock().unwrap().clear();
    wheel(cx, 120., 0.);
    settle(&rt, cx);
    let horizontal = inputs(&f.calls.lock().unwrap());
    wheel(cx, 0., -metrics.height * 2.5);
    settle(&rt, cx);
    let down = inputs(&f.calls.lock().unwrap());
    println!("OPUS_P3 mouse horizontal={horizontal:?} down={down:?}");
    assert!(horizontal.is_empty());
    assert_eq!(down, vec![b"\x1b[<65;1;1M\x1b[<65;1;1M".to_vec()]);
}

// F8 boundary: exactly 60 chunks / exactly 1 MiB admitted, the next refused visibly,
// and whole paste refused atomically; FIFO order preserved on flush.
#[test]
fn review_m810_p4_replay_buffer_exact_boundaries_and_fifo() {
    let mut s = state();
    s.listeners_ready(true, true);
    s.activate(boot(), 1);
    let list = s.next_work().unwrap();
    s.complete(list, Ok(json!([summary("a", true)])));
    let attach = s.next_work().unwrap();
    s.complete(attach, Ok(attached("a", "abc")));
    let replay = s.next_work().unwrap(); // replay in flight: input buffers
    for i in 0..60u8 {
        s.write_bytes(vec![b'A' + (i % 26)]);
    }
    assert!(s.error.is_none(), "60 chunks must be admitted");
    s.write_bytes(b"61".to_vec());
    let chunk_err = s.error.take();
    s.complete(replay, Ok(replay_value(0, b"abc", 3)));
    let mut flushed = vec![];
    while let Some(w) = s.next_work() {
        if let TerminalRequest::Input { bytes, .. } = w.request {
            flushed.push(bytes);
        }
    }
    println!("OPUS_P4 chunk_err={chunk_err:?} flushed={}", flushed.len());
    assert!(chunk_err.unwrap().contains("buffer is full"));
    assert_eq!(flushed.len(), 60);
    assert_eq!(flushed[0], b"A".to_vec());
    assert_eq!(flushed[59], vec![b'A' + (59 % 26)]);
    // Byte boundary: 1 MiB exactly admitted; a 2-byte paste after 1 MiB - 1 refused whole.
    let mut s = state();
    s.listeners_ready(true, true);
    s.activate(boot(), 1);
    let list = s.next_work().unwrap();
    s.complete(list, Ok(json!([summary("a", true)])));
    let attach = s.next_work().unwrap();
    s.complete(attach, Ok(attached("a", "abc")));
    let replay = s.next_work().unwrap();
    s.write_bytes(vec![b'x'; 1024 * 1024 - 1]);
    assert!(s.error.is_none());
    s.paste(s.generation, "yz");
    let paste_err = s.error.take();
    s.write_bytes(b"1".to_vec());
    assert!(s.error.is_none(), "exactly 1 MiB total is admitted");
    s.complete(replay, Ok(replay_value(0, b"abc", 3)));
    let mut sizes = vec![];
    while let Some(w) = s.next_work() {
        if let TerminalRequest::Input { bytes, .. } = w.request {
            sizes.push(bytes.len());
        }
    }
    println!("OPUS_P4 paste_err={paste_err:?} sizes={sizes:?}");
    assert!(paste_err.unwrap().contains("buffer is full"));
    assert_eq!(sizes, vec![1024 * 1024 - 1, 1]);
}

#[test]
fn review_m810_r4_failed_create_attach_resume_discards_input_visibly() {
    for kind in 0..3 {
        for malformed in 0..4 {
            let mut s = state();
            s.listeners_ready(true, true);
            s.activate(boot(), 1);
            let list = s.next_work().unwrap();
            s.complete(
                list,
                Ok(if kind == 0 {
                    json!([])
                } else {
                    json!([summary("a", true)])
                }),
            );
            let work = s.next_work().unwrap();
            let work = if kind == 2 {
                complete_attach(&mut s, work, "a", "");
                s.set_visible(false);
                while let Some(w) = s.next_work() {
                    s.complete(w, Ok(json!(true)));
                }
                s.set_visible(true);
                s.next_work().unwrap()
            } else {
                work
            };
            s.write("echo unsafe\r".into(), true);
            let result = match malformed {
                0 => Err("stream failed".into()),
                1 => Ok(Value::Null),
                2 => Ok(json!({"sessionId":"different"})),
                _ => {
                    let mut v = attached("a", "");
                    v["generation"] = json!("");
                    Ok(v)
                }
            };
            s.complete(work, result);
            // A Create may need one Attach before discovering a missing identity.
            while let Some(w) = s.next_work() {
                assert!(!matches!(&w.request, TerminalRequest::Input { .. }));
                let value = if matches!(w.purpose, Purpose::List) {
                    json!([])
                } else {
                    Value::Null
                };
                s.complete(w, Ok(value));
            }
            assert!(
                s.error.as_deref().unwrap_or("").contains("not sent"),
                "kind {kind} result {malformed}: {:?}",
                s.error
            );
            assert!(!s.running);
            s.write("later\r".into(), true);
            assert!(s.next_work().is_none());
            assert!(queued_inputs(&s).is_empty());
        }
    }
}

#[test]
fn review_m810_r5_pointer_reports_do_not_consume_replay_key_budget() {
    let mut s = state();
    s.listeners_ready(true, true);
    s.activate(boot(), 1);
    let w = s.next_work().unwrap();
    s.complete(w, Ok(json!([summary("a", true)])));
    let w = s.next_work().unwrap();
    s.complete(
        w,
        Ok(attached("a", "0123456789012345678901234567890123456789")),
    );
    let w = s.next_work().unwrap();
    let first = b"\x1b[?1003h\x1b[?1006h";
    s.complete(w, Ok(replay_value(0, first, 40)));
    let w = s.next_work().unwrap();
    for i in 0..120 {
        assert!(!s.write_pointer_bytes(format!("\x1b[<35;{};1M", i + 1).into_bytes()));
    }
    s.write("q".into(), true);
    assert!(s.error.is_none());
    s.complete(
        w,
        Ok(replay_value(
            first.len() as u64,
            &vec![b' '; 40 - first.len()],
            40,
        )),
    );
    let mut sent = vec![];
    while let Some(w) = s.next_work() {
        if let TerminalRequest::Input { bytes, .. } = &w.request {
            sent.push(bytes.clone());
        }
        s.complete(w, Ok(json!(true)));
    }
    assert_eq!(sent, vec![b"q".to_vec()]);
    assert!(s.write_pointer_bytes(b"\x1b[<35;1;1M".to_vec()));
    assert_eq!(queued_inputs(&s), vec![b"\x1b[<35;1;1M".to_vec()]);
}

fn drain_state(s: &mut State, id: &str) -> Vec<Vec<u8>> {
    let mut replies = vec![];
    while let Some(w) = s.next_work() {
        match &w.request {
            TerminalRequest::Attach { .. } => s.complete(w, Ok(attached(id, ""))),
            TerminalRequest::Replay { .. } => s.complete(w, Ok(replay_value(0, b"", 0))),
            TerminalRequest::Input { bytes, .. } => {
                replies.push(bytes.clone());
                s.complete(w, Ok(json!(true)));
            }
            _ => s.complete(w, Ok(json!(true))),
        }
    }
    replies
}

#[test]
fn review_m810_r6_stale_create_answers_first_startup_query_once() {
    let mut s = live();
    s.new_session();
    let create = loop {
        let w = s.next_work().unwrap();
        if matches!(w.purpose, Purpose::Create) {
            break w;
        }
        s.complete(w, Ok(json!(true)));
    };
    s.sessions
        .rows
        .push(serde_json::from_value(summary("fresh", true)).unwrap());
    s.select("a");
    s.complete(
        create,
        Ok(json!({"sessionId":"fresh","summary":summary("fresh",true),"generation":"gen-a"})),
    );
    drain_state(&mut s, "a");
    s.select("fresh");
    let work = loop {
        let w = s.next_work().unwrap();
        if matches!(w.purpose, Purpose::Attach(_)) {
            break w;
        }
        s.complete(w, Ok(json!(true)));
    };
    let mut v = attached("fresh", "\x1b[6n");
    v["origin"] = json!(0);
    s.complete(work, Ok(v.clone()));
    let replay = s.next_work().unwrap();
    s.complete(replay, Ok(replay_value(0, b"\x1b[6n", 4)));
    // A queued query reply must remain addressed to fresh when selection changes.
    s.select("a");
    let replies = drain_state(&mut s, "a");
    assert_eq!(replies, vec![b"\x1b[1;1R".to_vec()]);
    s.select("fresh");
    let work = loop {
        let w = s.next_work().unwrap();
        if matches!(w.purpose, Purpose::Attach(_)) {
            break w;
        }
        s.complete(w, Ok(json!(true)));
    };
    s.complete(work, Ok(v));
    let replay = s.next_work().unwrap();
    s.complete(replay, Ok(replay_value(0, b"\x1b[6n", 4)));
    assert!(
        queued_inputs(&s).is_empty(),
        "subsequent attach must not repeat historical queries"
    );
}
// F8: queued transport failure after flush discards remaining pending input visibly.
#[test]
fn review_m810_p9_flushed_write_failure_discards_rest_visibly() {
    let mut s = state();
    s.listeners_ready(true, true);
    s.activate(boot(), 1);
    let list = s.next_work().unwrap();
    s.complete(list, Ok(json!([summary("a", true)])));
    let attach = s.next_work().unwrap();
    s.complete(attach, Ok(attached("a", "abc")));
    let replay = s.next_work().unwrap();
    for c in ["1", "2", "3"] {
        s.write(c.into(), true);
    }
    s.complete(replay, Ok(replay_value(0, b"abc", 3)));
    let first = s.next_work().unwrap();
    assert!(matches!(&first.request, TerminalRequest::Input{bytes,..} if bytes==b"1"));
    s.complete(first, Err("transport closed".into()));
    let rest: Vec<_> = std::iter::from_fn(|| s.next_work())
        .filter(|w| matches!(w.request, TerminalRequest::Input { .. }))
        .collect();
    println!(
        "OPUS_P9 error={:?} remaining_inputs={}",
        s.error,
        rest.len()
    );
    assert!(rest.is_empty());
    assert!(s.error.as_deref().unwrap().contains("transport closed"));
}

#[gpui::test]
fn review_m810_r5_real_motion_during_replay_keeps_keyboard_fifo(cx: &mut TestAppContext) {
    let (rt, fixture, view, cx) = terminal_view(cx, b"\x1b[?1003h\x1b[?1006h$ ");
    let gate = Arc::new(tokio::sync::Notify::new());
    *fixture.replay_gate.lock().unwrap() = Some(gate.clone());
    fixture.calls.lock().unwrap().clear();
    view.update(cx, |v, cx| {
        v.state.set_visible(false);
        v.state.set_visible(true);
        v.pump(cx);
    });
    settle(&rt, cx);
    let (bounds, metrics) = view.read_with(cx, |v, _| v.geometry.unwrap());
    let mut last = point(px(0.), px(0.));
    for cell in 0..120 {
        last = bounds.origin
            + point(
                px((cell % 40) as f32 * metrics.width + 2.),
                px((cell / 40) as f32 * metrics.height + 2.),
            );
        cx.simulate_event(MouseMoveEvent {
            position: last,
            ..Default::default()
        });
    }
    cx.simulate_keystrokes("q");
    assert!(inputs(&fixture.calls.lock().unwrap()).is_empty());
    *fixture.replay_gate.lock().unwrap() = None;
    gate.notify_one();
    settle(&rt, cx);
    assert_eq!(inputs(&fixture.calls.lock().unwrap()), vec![b"q".to_vec()]);
    assert!(view.read_with(cx, |v, _| v.error().is_none()));
    // A suppressed same-cell report has not poisoned motion de-duplication.
    cx.simulate_event(MouseMoveEvent {
        position: last,
        ..Default::default()
    });
    settle(&rt, cx);
    assert_eq!(inputs(&fixture.calls.lock().unwrap()).len(), 2);
}

#[test]
fn review_m810_r6_stale_attach_and_partial_replay_preserve_query_progress() {
    let mut s = live();
    s.new_session();
    let create = loop {
        let w = s.next_work().unwrap();
        if matches!(w.purpose, Purpose::Create) {
            break w;
        }
        s.complete(w, Ok(json!(true)));
    };
    s.complete(
        create,
        Ok(json!({"sessionId":"fresh","summary":summary("fresh",true),"generation":"gen-a"})),
    );
    let first_attach = s.next_work().unwrap();
    assert!(matches!(first_attach.purpose, Purpose::Attach(_)));
    s.select("a");
    let mut attach = attached("fresh", "\x1b[6n\x1b[6n");
    attach["origin"] = json!(0);
    s.complete(first_attach, Ok(attach.clone()));
    drain_state(&mut s, "a");
    s.select("fresh");
    let w = loop {
        let w = s.next_work().unwrap();
        if matches!(w.purpose, Purpose::Attach(_)) {
            break w;
        }
        s.complete(w, Ok(json!(true)));
    };
    s.complete(w, Ok(attach.clone()));
    let w = s.next_work().unwrap();
    s.complete(w, Ok(replay_value(0, b"\x1b[6n", 8)));
    s.select("a");
    assert_eq!(drain_state(&mut s, "a"), vec![b"\x1b[1;1R".to_vec()]);
    s.select("fresh");
    let w = loop {
        let w = s.next_work().unwrap();
        if matches!(w.purpose, Purpose::Attach(_)) {
            break w;
        }
        s.complete(w, Ok(json!(true)));
    };
    s.complete(w, Ok(attach));
    let w = s.next_work().unwrap();
    s.complete(w, Ok(replay_value(0, b"\x1b[6n\x1b[6n", 8)));
    assert_eq!(
        queued_inputs(&s),
        vec![b"\x1b[1;1R".to_vec()],
        "first query was already answered, only the second is live"
    );
}

#[test]
fn review_followup_terminal_replay_error_then_gap_resumes_with_a_new_token() {
    let mut s = state();
    s.listeners_ready(true, true);
    s.activate(boot(), 1);
    let w = s.next_work().unwrap();
    s.complete(w, Ok(json!([summary("a", true)])));
    let w = s.next_work().unwrap();
    s.complete(w, Ok(attached("a", "abcdefgh")));
    let w = s.next_work().unwrap();
    s.complete(w, Ok(replay_value(0, b"abcd", 8)));
    let w = s.next_work().unwrap();
    s.complete(w, Err("Terminal history was truncated or modified".into()));
    s.stream_data("a", "gen-a", 8, b"$ ", true);
    let resume = loop {
        let w = s
            .next_work()
            .expect("A tokenless gap must not stay loading without work");
        if matches!(w.purpose, Purpose::Resume(_)) {
            break w;
        }
        assert!(w.cleanup());
        s.complete(w, Ok(json!(true)));
    };
    s.write("x".into(), true);
    let mut fresh = attached("a", "abcdefgh$ ");
    fresh["replay"] = json!("token-new");
    s.complete(resume, Ok(fresh));
    let w = s.next_work().unwrap();
    assert!(
        matches!(&w.request, TerminalRequest::Replay { token, position:4 } if token == "token-new")
    );
    s.complete(w, Ok(replay_value(4, b"efgh$ ", 10)));
    let w = s.next_work().unwrap();
    assert!(matches!(w.request, TerminalRequest::Input { ref bytes, .. } if bytes == b"x"));
}
#[gpui::test]
fn review_followup_terminal_wheel_during_replay_preserves_the_next_key(cx: &mut TestAppContext) {
    let (rt, f, view, cx) = terminal_view(cx, b"\x1b[?1049h$ ");
    let gate = Arc::new(tokio::sync::Notify::new());
    *f.replay_gate.lock().unwrap() = Some(gate.clone());
    f.calls.lock().unwrap().clear();
    view.update(cx, |v, cx| {
        v.state.set_visible(false);
        v.state.set_visible(true);
        v.pump(cx);
    });
    settle(&rt, cx);
    let (bounds, metrics) = view.read_with(cx, |v, _| v.geometry.unwrap());
    let at = bounds.origin + point(px(3.), px(3.));
    for _ in 0..60 {
        cx.simulate_event(ScrollEvent {
            position: at,
            delta: ScrollDelta::Pixels(point(px(0.), px(metrics.height))),
            ..Default::default()
        });
    }
    cx.simulate_keystrokes("q");
    assert!(view.read_with(cx, |v, _| {
        !v.error().is_some_and(|e| e.contains("buffer is full"))
    }));
    *f.replay_gate.lock().unwrap() = None;
    gate.notify_one();
    settle(&rt, cx);
    let bytes = inputs(&f.calls.lock().unwrap()).concat();
    let mut expected = b"\x1b[A".repeat(60);
    expected.push(b'q');
    assert_eq!(bytes, expected);
}
