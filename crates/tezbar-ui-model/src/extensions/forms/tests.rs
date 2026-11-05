use super::*;
fn form(fields: Value) -> Node {
    serde_json::from_value(json!({"type":"Form","children":fields})).unwrap()
}
fn f(id: &str, kind: &str, value: Option<Value>, callback: bool) -> Value {
    let mut f = json!({"type":kind,"props":{"id":id}});
    if let Some(v) = value {
        f["props"]["value"] = v;
    }
    if callback {
        f["props"]["actionId"] = json!("change");
    }
    f
}
#[test]
fn submit_only_defaults_keep_draft_while_changed_server_defaults_reset() {
    let mut s = FormState::default();
    let fields = vec![
        f("seconds", "Form.TextField", Some(json!("")), false),
        f("name", "Form.TextField", Some(json!("")), false),
    ];
    s.reconcile(&form(json!(fields)));
    s.change("seconds", json!("5")).unwrap();
    s.change("name", json!("Draft")).unwrap();
    s.reconcile(&form(json!(fields)));
    assert_eq!(s.values()["seconds"], "5");
    s.reconcile(&form(json!([
        f("seconds", "Form.TextField", Some(json!("10")), false),
        fields[1]
    ])));
    assert_eq!(s.values()["seconds"], "10");
    assert_eq!(s.values()["name"], "Draft");
}
#[test]
fn controlled_updates_are_authoritative() {
    let mut s = FormState::default();
    let root = form(json!([f(
        "seconds",
        "Form.TextField",
        Some(json!("")),
        true
    )]));
    s.reconcile(&root);
    assert_eq!(
        s.change("seconds", json!("5")).unwrap(),
        Some("change".into())
    );
    s.reconcile(&root);
    assert_eq!(s.values()["seconds"], "");
}
#[test]
fn initialize_new_drop_removed_and_changed_types() {
    let mut s = FormState::default();
    s.reconcile(&form(json!([
        f("keep", "Form.TextField", None, false),
        f("changed", "Form.TextField", None, false)
    ])));
    s.change("keep", json!("draft")).unwrap();
    s.reconcile(&form(json!([
        f("keep", "Form.TextField", None, false),
        f("changed", "Form.Checkbox", None, false),
        f("tags", "Form.TagPicker", None, false),
        f("files", "Form.FilePicker", None, false),
        f("text", "Form.TextArea", None, false)
    ])));
    assert_eq!(s.values()["keep"], "draft");
    assert_eq!(s.values()["changed"], false);
    assert_eq!(s.values()["tags"], json!([]));
    assert_eq!(s.values()["files"], json!([]));
    assert_eq!(s.values()["text"], "");
    s.reconcile(&form(json!([f(
        "minutes",
        "Form.TextField",
        Some(json!("2")),
        false
    )])));
    assert!(!s.values().contains_key("keep"));
}
#[test]
fn acknowledgement_then_server_reset() {
    let mut s = FormState::default();
    s.reconcile(&form(json!([f(
        "seconds",
        "Form.TextField",
        Some(json!("")),
        false
    )])));
    s.change("seconds", json!("5")).unwrap();
    s.reconcile(&form(json!([f(
        "seconds",
        "Form.TextField",
        Some(json!("5")),
        false
    )])));
    assert_eq!(s.values()["seconds"], "5");
    s.reconcile(&form(json!([f(
        "seconds",
        "Form.TextField",
        Some(json!("")),
        false
    )])));
    assert_eq!(s.values()["seconds"], "");
}
#[test]
fn rebuilt_arrays_compare_by_value() {
    let mut s = FormState::default();
    let fields = json!([f("tags", "Form.TagPicker", Some(json!(["server"])), false)]);
    s.reconcile(&form(fields.clone()));
    s.change("tags", json!(["draft"])).unwrap();
    s.reconcile(&form(fields));
    assert_eq!(s.values()["tags"], json!(["draft"]));
    s.reconcile(&form(json!([f(
        "tags",
        "Form.TagPicker",
        Some(json!([])),
        false
    )])));
    assert_eq!(s.values()["tags"], json!([]));
}
#[test]
fn wheel_waits_for_all_coordinate_acknowledgements() {
    let mut s = FormState::default();
    s.wheel = Some([120.0, 75.0, 60.0, 85.0]);
    let coords = |alpha| {
        form(json!([
            f("hue", "Form.TextField", Some(json!("120")), false),
            f("saturation", "Form.TextField", Some(json!("75")), false),
            f("value", "Form.TextField", Some(json!("60")), false),
            f("alpha", "Form.TextField", Some(json!(alpha)), false)
        ]))
    };
    s.reconcile(&coords("80"));
    assert!(s.wheel.is_some());
    s.reconcile(&coords("85"));
    assert!(s.wheel.is_none());
}
#[test]
fn submit_key_table_and_repeat_ime_matrix() {
    let cases = [
        (true, false, false, false, true),
        (true, true, false, false, false),
        (true, true, true, false, true),
        (true, true, false, true, false),
        (false, false, false, false, true),
        (false, true, false, false, false),
        (false, true, false, true, true),
        (false, true, true, false, false),
    ];
    for (mac, textarea, command, control, expected) in cases {
        assert_eq!(
            should_submit(SubmitKey {
                enter: true,
                mac,
                textarea,
                command,
                control,
                ..Default::default()
            }),
            expected
        );
    }
    for mac in [true, false] {
        for textarea in [true, false] {
            for modifiers in 0..16 {
                for repeat in [true, false] {
                    let k = SubmitKey {
                        enter: true,
                        mac,
                        textarea,
                        command: modifiers & 1 != 0,
                        control: modifiers & 2 != 0,
                        alt: modifiers & 4 != 0,
                        shift: modifiers & 8 != 0,
                        repeat,
                        composing: !repeat,
                    };
                    assert!(!should_submit(k));
                }
            }
        }
    }
}
#[test]
fn field_errors_required_and_file_constraints() {
    let mut s = FormState::default();
    s.reconcile(&form(json!([{"type":"Form.PasswordField","props":{"id":"secret","required":true}},{"type":"Form.FilePicker","props":{"id":"files"}}])));
    assert!(s.validate().is_err());
    s.change("secret", json!("fixture")).unwrap();
    assert!(s.change("files", json!(["a", "b"])).is_err());
    assert!(s.validate().is_ok());
}
#[test]
fn preferences_keep_command_scopes_and_partition() {
    let field = |name: &str, cmd: Option<&str>| PreferenceField {
        name: name.into(),
        command: cmd.map(str::to_owned),
        kind: "password".into(),
        required: false,
        options: vec![],
    };
    let p = Preferences::merge(vec![
        (
            None,
            vec![field("token", None)],
            [("token".into(), json!("fixture"))].into(),
        ),
        (
            Some("search".into()),
            vec![field("token", None), field("limit", Some("search"))],
            [("limit".into(), json!("10"))].into(),
        ),
        (
            Some("recent".into()),
            vec![field("limit", Some("recent"))],
            [("limit".into(), json!("25"))].into(),
        ),
    ]);
    assert_eq!(p.fields.len(), 3);
    let parts = p.partition().unwrap();
    assert_eq!(parts[&Some("search".into())]["limit"], "10");
    assert_eq!(parts[&Some("recent".into())]["limit"], "25");
    assert_eq!(parts[&None]["token"], "fixture");
}
