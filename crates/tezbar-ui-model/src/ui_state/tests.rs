use super::*;
fn row(id: &str) -> Row {
    Row::command(id, id)
}
fn pins(ids: &[&str]) -> Vec<Pin> {
    let mut p = ids.iter().map(|id| Pin::from(&row(id))).collect();
    normalize_pins(&mut p);
    p
}
#[test]
fn assigns_1_based_sequential_slot_numbers() {
    assert_eq!(
        pins(&["a", "b"]).iter().map(|p| p.slot).collect::<Vec<_>>(),
        [1, 2]
    );
}
#[test]
fn caps_pins_at_max_pinned_commands() {
    let mut p = (0..20).map(|i| Pin::from(&row(&i.to_string()))).collect();
    normalize_pins(&mut p);
    assert_eq!(p.len(), 12);
    assert_eq!(p[11].slot, 12);
}
#[test]
fn appends_new_pin_to_bottom() {
    let mut p = pins(&["clipboard"]);
    insert_pin(&mut p, &row("snippets"), None);
    assert_eq!(p[0].id, "command:clipboard");
    assert_eq!(p[1].slot, 2);
}
#[test]
fn appends_third_pin_as_slot_3() {
    let mut p = pins(&["clipboard", "snippets"]);
    insert_pin(&mut p, &row("kill-process"), None);
    assert_eq!(p[2].id, "command:kill-process");
    assert_eq!(p[2].slot, 3);
}
#[test]
fn explicit_insert_index() {
    for index in [0, 1] {
        let mut p = pins(&["clipboard", "snippets"]);
        insert_pin(&mut p, &row("open-ports"), Some(index));
        assert_eq!(p[index].id, "command:open-ports");
        assert_eq!(p.iter().map(|p| p.slot).collect::<Vec<_>>(), [1, 2, 3]);
    }
}
#[test]
fn missing_dragged_id_is_unchanged() {
    let mut p = pins(&["a", "b"]);
    let old = p.clone();
    reorder_pin(&mut p, "missing", 0);
    assert_eq!(p, old);
}
#[test]
fn moves_item_and_normalizes_slots() {
    let mut p = pins(&["a", "b", "c"]);
    reorder_pin(&mut p, "command:c", 0);
    assert_eq!(
        p.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        ["command:c", "command:a", "command:b"]
    );
    assert_eq!(p.iter().map(|p| p.slot).collect::<Vec<_>>(), [1, 2, 3]);
}
#[test]
fn parses_digit_keys_into_slot_numbers() {
    for (code, key, expected) in [
        ("Digit1", "", Some(1)),
        ("Digit9", "", Some(9)),
        ("Digit0", "", Some(10)),
        ("Minus", "", Some(11)),
        ("Equal", "", Some(12)),
        ("", "3", Some(3)),
        ("", "0", Some(10)),
        ("", "x", None),
    ] {
        assert_eq!(pin_slot(code, key), expected);
    }
}
#[test]
fn formats_slot_shortcut_keys() {
    for (n, k) in [(1, "1"), (9, "9"), (10, "0"), (11, "-"), (12, "=")] {
        assert_eq!(slot_key(n), k);
    }
}
#[test]
fn allows_regular_commands_and_extensions() {
    assert!(row("cmd").pinnable());
}
#[test]
fn rejects_calculator_color_and_deep_results() {
    for category in ["calculator", "color-converter"] {
        let mut r = row("cmd");
        r.category = category.into();
        assert!(!r.pinnable());
    }
    let mut r = row("cmd");
    r.id = "deep-search:query".into();
    assert!(!r.pinnable());
}
#[test]
fn pins_storage_roundtrip() {
    let p = pins(&["a", "b"]);
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(parse_pins(&v), p);
}
#[test]
fn empty_or_corrupt_storage() {
    for raw in ["", "{invalid json}"] {
        let mut s = UiState::default();
        assert!(s.import_key(KEYS[0], raw).is_err());
        assert!(s.pins.is_empty());
    }
}
#[test]
fn all_browser_keys_import_validation_existing_state_wins() {
    let mut legacy = BTreeMap::new();
    legacy.insert(
        KEYS[0].into(),
        serde_json::to_string(&pins(&["a"])).unwrap(),
    );
    legacy.insert(KEYS[1].into(), "[\"extcmd:x:y\",\"untrusted\"]".into());
    legacy.insert(KEYS[2].into(), "[\" one\",\"two\"]".into());
    legacy.insert(KEYS[3].into(), "huge cache never parsed".into());
    legacy.insert(KEYS[4].into(), "huge cache never parsed".into());
    legacy.insert(
        KEYS[5].into(),
        r#"{"saveFor":"no","keepAliveFor":"8h"}"#.into(),
    );
    legacy.insert(KEYS[6].into(), "[\"persisted-identity\"]".into());
    legacy.insert(KEYS[7].into(), "[\"😀\"]".into());
    legacy.insert(
        KEYS[8].into(),
        r#"{"pinned":"USD","usage":{"EUR":{"count":2.9,"lastUsedAt":10}}}"#.into(),
    );
    legacy.insert(KEYS[9].into(), "dark".into());
    legacy.insert(KEYS[10].into(), "0.3.0-beta.7".into());
    legacy.insert(KEYS[11].into(), "0xFF".into());
    legacy.insert(KEYS[12].into(), "true".into());
    legacy.insert(KEYS[13].into(), "ai".into());
    legacy.insert("ignored".into(), "secret".into());
    let r = UiState::import_legacy(None, &legacy).unwrap();
    assert!(r.rejected.is_empty());
    assert_eq!(r.ignored_caches.len(), 2);
    let s = r.state;
    assert_eq!(s.launcher_query_history, [" one"]);
    assert_eq!(s.recent_extension_commands, ["extcmd:x:y"]);
    assert_eq!(
        s.terminal_defaults,
        TerminalDefaults {
            save_for: "week".into(),
            keep_alive_for: "8h".into()
        }
    );
    assert_eq!(s.last_update_check, Some(255.));
    assert_eq!(s.currency.usage["EUR"].count, 2);
    assert!(s.auto_update_enabled);
    let export = s.export_legacy();
    assert_eq!(export.len(), 12);
    let mut existing = UiState::default();
    existing
        .unknown
        .insert("future-version-data".into(), json!({"kept":true}));
    let existing = existing.encode().unwrap();
    let r = UiState::import_legacy(Some(&existing), &legacy).unwrap();
    assert!(r.used_existing);
    assert_eq!(r.state.theme, Theme::System);
    assert_eq!(r.state.unknown["future-version-data"]["kept"], true);
    assert!(UiState::import_legacy(Some("corrupt"), &legacy).is_err());
}
#[test]
fn atomic_fixture_writes_and_failure_does_not_replace_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ui-state-v1.json");
    let mut s = UiState::default();
    s.write_atomic(&path).unwrap();
    s.theme = Theme::White;
    s.write_atomic(&path).unwrap();
    assert_eq!(
        UiState::decode(&std::fs::read_to_string(&path).unwrap()).unwrap(),
        s
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    assert!(s
        .write_atomic(&dir.path().join("missing").join("state"))
        .is_err());
}
#[test]
fn theme_recognizes_preferences() {
    for (raw, t) in [
        ("white", Theme::White),
        ("dark", Theme::Dark),
        ("system", Theme::System),
    ] {
        let mut s = UiState::default();
        s.import_key(KEYS[9], raw).unwrap();
        assert_eq!(s.theme, t);
    }
    assert!(UiState::default().import_key(KEYS[9], "light").is_err());
}
#[test]
fn theme_resolves_explicit_preferences() {
    assert_eq!(Theme::White.resolved(true), Theme::White);
    assert_eq!(Theme::Dark.resolved(false), Theme::Dark);
}
#[test]
fn theme_resolves_system_preferences() {
    assert_eq!(Theme::System.resolved(true), Theme::Dark);
    assert_eq!(Theme::System.resolved(false), Theme::White);
}
