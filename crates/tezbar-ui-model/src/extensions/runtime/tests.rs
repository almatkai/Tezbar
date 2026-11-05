use super::*;
use serde_json::json;
fn node(kind: &str, props: Value, children: Vec<Node>) -> Node {
    serde_json::from_value(json!({"type":kind,"props":props,"children":children})).unwrap()
}
fn root(ids: &[&str]) -> Node {
    node(
        "List",
        json!({"pagination":{"hasMore":true}}),
        ids.iter()
            .map(|id| {
                node(
                    "List.Item",
                    json!({"id":id,"title":id,"actionIds":["open"]}),
                    vec![],
                )
            })
            .collect(),
    )
}
fn reply(root: Node) -> TreeReply {
    TreeReply {
        key: SessionKey {
            extension_id: "ext".into(),
            command_name: "cmd".into(),
            session_id: "session".into(),
        },
        generation: Some("generation".into()),
        root,
        actions: vec![Action {
            id: "open".into(),
            title: "Open".into(),
            style: None,
            kind: Some("action".into()),
            shortcut: None,
        }],
        refresh: PollPolicy::EventDriven,
        effects: vec![],
        effect_sequence: None,
    }
}
fn session() -> RuntimeSession {
    RuntimeSession::new(reply(root(&["a", "b", "c", "d"])), 0).unwrap()
}
#[test]
fn static_views_only_refresh_for_new_owned_invalidations() {
    let mut s = session();
    assert!(s.tick(60_000).is_none());
    assert!(s.invalidate("other", Some("generation"), 1).is_none());
    assert!(s.invalidate("session", Some("old"), 1).is_none());
    assert!(s.invalidate("session", Some("generation"), 2).is_some());
    assert!(s.invalidate("session", Some("generation"), 2).is_none());
}
#[test]
fn legacy_polling_coalesces_and_disposal_stops() {
    let mut r = reply(root(&[]));
    r.refresh = PollPolicy::Legacy;
    let mut s = RuntimeSession::new(r, 0).unwrap();
    let Effect::Refresh(token) = s.tick(500).unwrap() else {
        panic!()
    };
    assert!(s.invalidate("session", Some("generation"), 1).is_none());
    assert!(s.invalidate("session", Some("generation"), 2).is_none());
    assert!(s.refresh_finished(&token, None, 501).unwrap().is_some());
    assert!(s.dispose().is_some());
    assert!(s.dispose().is_none());
    assert!(s.tick(50_000).is_none());
}
#[test]
fn hidden_invalidations_wait_and_resume_once() {
    let mut s = session();
    s.visibility(false, 0);
    assert!(s.invalidate("session", Some("generation"), 1).is_none());
    assert!(s.invalidate("session", Some("generation"), 2).is_none());
    assert!(s.tick(60_000).is_none());
    assert!(s.visibility(true, 60_001).is_some());
    assert!(s.visibility(true, 60_002).is_none());
}
#[test]
fn stable_selection_reorder_removal_and_pagination() {
    let mut s = session();
    s.select("b".into());
    let t = s.token();
    s.refresh_finished(&t, Some(reply(root(&["d", "c", "b", "a"]))), 1)
        .unwrap();
    assert_eq!(s.selection.as_deref(), Some("b"));
    let t = s.token();
    s.refresh_finished(&t, Some(reply(root(&["d", "c"]))), 2)
        .unwrap();
    assert_eq!(s.selection.as_deref(), Some("c"));
    assert!(matches!(s.select("c".into()), Some(Effect::LoadMore(_))));
    assert!(s.select("c".into()).is_none());
}
#[test]
fn client_filter_and_server_search_are_distinct() {
    let mut s = session();
    assert!(s.search("b".into()).is_none());
    assert_eq!(s.filtered_ids(), vec!["b"]);
    let mut r = root(&["a", "b"]);
    r.props.insert("__hasServerSearch".into(), json!(true));
    let t = s.token();
    s.refresh_finished(&t, Some(reply(r)), 0).unwrap();
    assert!(matches!(
        s.search("unmatched".into()),
        Some(Effect::Search { .. })
    ));
    assert_eq!(s.filtered_ids(), vec!["a", "b"]);
}
#[test]
fn callback_ids_are_session_owned_and_latest_reply_wins() {
    let mut s = session();
    assert!(s.invoke("foreign", None).is_err());
    let Effect::Invoke { token, .. } = s.invoke("open", None).unwrap() else {
        panic!()
    };
    s.search("b".into());
    assert!(s
        .refresh_finished(&token, Some(reply(root(&["wrong"]))), 1)
        .unwrap()
        .is_none());
    assert_eq!(s.filtered_ids(), vec!["b"]);
}
#[test]
fn navigation_push_pop_and_back_callback() {
    let mut s = session();
    s.push(node("Detail", json!({"markdown":"body"}), vec![]))
        .unwrap();
    assert!(s.back().unwrap().is_none());
    assert_eq!(s.root.kind, NodeKind::List);
    let mut n = node("Detail", json!({"backActionId":"back"}), vec![]);
    n.props.insert("markdown".into(), json!("nested"));
    s.push(n).unwrap();
    assert!(matches!(s.back().unwrap(),Some(Effect::Invoke{action_id,..})if action_id=="back"));
}
#[test]
fn action_shortcut_panel_filter_and_submit() {
    let mut s = session();
    s.actions[0].shortcut = Some(Shortcut {
        modifiers: vec!["cmd".into()],
        key: Some("o".into()),
    });
    assert!(s.shortcut("o", &["cmd".into()]).unwrap().is_some());
    assert!(s.shortcut("o", &[]).unwrap().is_none());
    s.ui_event(UiEvent::OpenActions).unwrap();
    assert!(s.action_panel);
    assert_eq!(s.action_filter("OPEN").len(), 1);
}
#[test]
fn wire_rejects_unknown_and_oversize_or_duplicate_fields() {
    assert!(serde_json::from_value::<Node>(json!({"type":"WebView"})).is_err());
    let f = node("Form.TextField", json!({"id":"same"}), vec![]);
    assert!(node("Form", json!({}), vec![f.clone(), f])
        .validate()
        .is_err());
    let wire = json!({"ok":true,"mode":"view","extensionId":"e","commandName":"c","sessionId":"s","generation":"g","refreshMs":null,"root":{"type":"Detail"},"actions":[]});
    assert!(TreeReply::from_wire(wire).is_ok());
}
#[test]
fn all_worker_node_kinds_roundtrip_and_validate() {
    let names = [
        "List",
        "List.Section",
        "List.Item",
        "List.Item.Detail",
        "List.Item.Detail.Metadata",
        "List.Item.Detail.Metadata.Label",
        "List.Item.Detail.Metadata.Link",
        "List.Item.Detail.Metadata.TagList",
        "List.Item.Detail.Metadata.TagList.Item",
        "List.Item.Detail.Metadata.Separator",
        "List.EmptyView",
        "List.Dropdown",
        "List.Dropdown.Section",
        "List.Dropdown.Item",
        "Grid",
        "Grid.Section",
        "Grid.Item",
        "Grid.EmptyView",
        "Detail",
        "Detail.Metadata",
        "Detail.Metadata.Label",
        "Detail.Metadata.Link",
        "Detail.Metadata.TagList",
        "Detail.Metadata.TagList.Item",
        "Detail.Metadata.Separator",
        "Form",
        "Form.TextField",
        "Form.TextArea",
        "Form.PasswordField",
        "Form.Checkbox",
        "Form.DatePicker",
        "Form.Dropdown",
        "Form.Dropdown.Section",
        "Form.Dropdown.Item",
        "Form.TagPicker",
        "Form.TagPicker.Item",
        "Form.FilePicker",
        "Form.Description",
        "Form.Separator",
        "ActionPanel",
        "ActionPanel.Section",
        "Action",
        "Action.CopyToClipboard",
        "Action.OpenInBrowser",
        "Action.Paste",
        "Action.Push",
        "Action.Pop",
        "Action.ShowInFinder",
        "Action.SubmitForm",
        "MenuBarExtra",
        "MenuBarExtra.Item",
        "MenuBarExtra.Section",
        "MenuBarExtra.Submenu",
        "MenuBarExtra.Separator",
        "Tezbar.PreferenceSetup",
    ];
    for name in names {
        let n = node(name, json!({}), vec![]);
        n.validate().unwrap();
        assert_eq!(serde_json::to_value(&n).unwrap()["type"], name);
    }
}
#[test]
fn adaptive_grid_colors_and_effects_are_emitted_once() {
    for (value, want) in [
        (
            json!({"color":{"light":"#7CFC00","dark":"#008000"}}),
            "#008000",
        ),
        (json!({"color":"#0F0"}), "#0F0"),
        (json!({"value":{"color":{"light":"#ADFF2F"}}}), "#ADFF2F"),
    ] {
        assert_eq!(color_from_grid_content(&value), Some(want));
    }
    let mut r = reply(root(&[]));
    r.effects = vec![
        WireEffect::Clipboard {
            value: "fixture".into(),
        },
        WireEffect::AppleScript {
            value: Some("already executed".into()),
        },
    ];
    let mut s = RuntimeSession::new(r, 0).unwrap();
    assert_eq!(s.drain_effects().count(), 0);
    assert_eq!(s.drain_effects().count(), 0);
}
#[test]
fn submit_guard_releases_on_view_no_view_failure_and_ignores_replaced_session() {
    let form = node(
        "Form",
        json!({}),
        vec![node(
            "Form.TextArea",
            json!({"id":"prompt","value":"Question"}),
            vec![],
        )],
    );
    let mut r = reply(form);
    r.actions = vec![Action {
        id: "submit".into(),
        title: "Submit".into(),
        kind: Some("submit-form".into()),
        style: None,
        shortcut: None,
    }];
    let mut s = RuntimeSession::new(r, 0).unwrap();
    let Some(Effect::Invoke { token, .. }) = s.primary(false).unwrap() else {
        panic!()
    };
    assert!(s.primary(false).is_err());
    s.ui_event(UiEvent::OpenActions).unwrap();
    assert!(s.action_panel);
    s.no_view(&token, String::new(), vec![], None, 1);
    assert!(s.notice.is_none());
    let Some(Effect::Invoke { token, .. }) = s.primary(false).unwrap() else {
        panic!()
    };
    s.failed(&token, 2);
    assert_eq!(s.notice, Some(Notice::Error));
    let Some(Effect::Invoke { token, .. }) = s.primary(false).unwrap() else {
        panic!()
    };
    s.no_view(&token, "Saved answer".into(), vec![], None, 3);
    assert_eq!(s.notice_message.as_deref(), Some("Saved answer"));
    s.dispose();
    s.no_view(&token, "stale".into(), vec![], None, 4);
    assert_ne!(s.notice_message.as_deref(), Some("stale"));
}

#[test]
fn cumulative_records_never_execute_host_actions() {
    let mut r = reply(root(&[]));
    r.effects = vec![
        WireEffect::Clipboard {
            value: "truncated file copy".into(),
        },
        WireEffect::Open {
            value: "https://example.test".into(),
        },
        WireEffect::ShowInFinder {
            value: "/fixture/file".into(),
        },
    ];
    let mut s = RuntimeSession::new(r, 0).unwrap();
    assert_eq!(
        s.drain_effects().count(),
        0,
        "worker records already executed"
    );
}

#[test]
fn sliding_50_records_and_identical_new_notices_are_sequence_owned() {
    let mut s = session();
    for total in 1..=120 {
        let mut r = reply(root(&[]));
        r.effect_sequence = Some(total);
        r.effects = (0..total.min(50))
            .map(|_| WireEffect::Hud {
                title: None,
                message: Some("identical notice".into()),
            })
            .collect();
        let t = s.token();
        s.refresh_finished(&t, Some(r), total).unwrap();
        assert_eq!(s.notice_message.as_deref(), Some("identical notice"));
        assert_eq!(s.records.sequence, total);
        assert_eq!(s.drain_effects().count(), 0);
        let mut repeated = reply(root(&[]));
        repeated.effect_sequence = Some(total);
        repeated.effects = (0..total.min(50))
            .map(|_| WireEffect::Hud {
                title: None,
                message: Some("identical notice".into()),
            })
            .collect();
        let t = s.token();
        s.refresh_finished(&t, Some(repeated), total).unwrap();
        assert!(s.notice_message.is_none(), "poll repeats no notices");
    }
}

#[test]
fn no_view_and_legacy_records_never_replay_long_clipboard_file_open_or_scripts() {
    let records = || {
        vec![
            WireEffect::Clipboard {
                value: "x".repeat(10_000),
            },
            WireEffect::Open {
                value: "https://example.test".into(),
            },
            WireEffect::ShowInFinder {
                value: "/fixture/image.png".into(),
            },
            WireEffect::AppleScript {
                value: Some("never execute".into()),
            },
            WireEffect::PowershellScript {
                value: Some("never execute".into()),
            },
            WireEffect::Hud {
                title: None,
                message: Some("copied".into()),
            },
        ]
    };
    let mut r = reply(root(&[]));
    r.effects = records();
    let mut s = RuntimeSession::new(r, 0).unwrap();
    assert_eq!(s.notice_message.as_deref(), Some("copied"));
    let t = s.token();
    s.no_view(&t, String::new(), records(), None, 1);
    assert!(s.notice_message.is_none());
    assert_eq!(s.drain_effects().count(), 0);
    let t = s.token();
    s.no_view(&t, String::new(), records(), Some(7), 2);
    assert_eq!(s.notice_message.as_deref(), Some("copied"));
    let t = s.token();
    s.no_view(&t, "copied".into(), records(), Some(7), 3);
    assert!(s.notice_message.is_none());
    assert_eq!(s.drain_effects().count(), 0);
}
