use super::*;

#[cfg(target_os = "macos")]
#[gpui::test]
fn review_m810_real_cmd_pin_effect_reaches_owner_and_merges_live_pins(cx: &mut TestAppContext) {
    let rt = crate::surfaces::settings::tests::runtime(cx);
    let f = Arc::new(Fixture::default());
    let (relay, mut rx) = Relay::new();
    cx.update(|cx| cx.set_global(CoreEvents::new(relay.clone())));
    let (launcher, cx) =
        cx.add_window_view(|w, cx| Launcher::with_adapter(f.clone(), relay, None, w, cx));
    launcher.update(cx, |v, cx| {
        v.visible = true;
        v.ready = true;
        v.receive_navigation(intent(1), cx);
    });
    settle(&rt, cx);
    // Command-bar surface pins b after the terminal view was created
    // (the same mutation + persist as interactions::terminal_pin).
    launcher.update(cx, |v, cx| {
        v.state.terminal_pinned_sessions.insert(0, "b".into());
        v.persist(cx);
    });
    let drain = |rx: &mut crate::events::Receiver| {
        let mut seen = vec![];
        rt.block_on(async {
            while let Ok(Some(e)) = tokio::time::timeout(Duration::from_millis(20), rx.recv()).await
            {
                if let crate::events::Event::TerminalEffect(source, effect) = e {
                    seen.push((source, effect));
                }
            }
        });
        seen
    };
    let _ = drain(&mut rx);
    cx.simulate_keystrokes("cmd-p");
    settle(&rt, cx);
    let mut seen = drain(&mut rx);
    println!(
        "OPUS_F4 effects={:?}",
        seen.iter().map(|(_, e)| e.clone()).collect::<Vec<_>>()
    );
    assert_eq!(seen.len(), 1);
    let (source, effect) = seen.pop().unwrap();
    assert!(launcher.read_with(cx, |v, cx| v.accepts_terminal_source(source, cx)));
    let crate::surfaces::terminal::TerminalEffect::TogglePin(id) = effect else {
        panic!("unexpected effect")
    };
    // Owner::m10_effect for an accepted, non-quitting source:
    launcher.update(cx, |v, cx| v.apply_terminal_pin_change(id, true, cx));
    let ui = launcher.read_with(cx, |v, _| v.state.terminal_pinned_sessions.clone());
    println!("OPUS_F4 ui_pins_after_toggle={ui:?}");
    assert_eq!(ui, vec!["a".to_string(), "b".to_string()]);
    // Terminal delete (cmd-d) emits Unpin with the post-delete mount, still accepted.
    cx.simulate_keystrokes("cmd-d");
    settle(&rt, cx);
    let mut seen = drain(&mut rx);
    let (source, effect) = seen.pop().expect("unpin effect");
    let accepted = launcher.read_with(cx, |v, cx| v.accepts_terminal_source(source, cx));
    println!("OPUS_F4 delete_effect={effect:?} accepted={accepted}");
    assert!(accepted);
    let crate::surfaces::terminal::TerminalEffect::Unpin(id) = effect else {
        panic!("unexpected effect")
    };
    launcher.update(cx, |v, cx| v.apply_terminal_pin_change(id, false, cx));
    let ui = launcher.read_with(cx, |v, _| v.state.terminal_pinned_sessions.clone());
    println!("OPUS_F4 ui_pins_after_delete={ui:?}");
    assert_eq!(ui, vec!["b".to_string()]);
}
