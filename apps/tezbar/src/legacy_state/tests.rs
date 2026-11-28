//! Source-only regression fixtures, all UNRUN until parent Cargo/export gate.
use super::*;
fn complete(values: &[(&str, &str)]) -> ReadReport {
    ReadReport {
        values: values
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        complete: true,
        ..ReadReport::default()
    }
}
#[test]
fn malformed_values_defaults_boolean_theme_timeouts_and_all_browser_keys() {
    assert_eq!(KEYS.len(), 14); // Ledger P-15 is native layout, not a browser key.
    let p = prepare(
        None,
        1,
        complete(&[
            (KEYS[0], "{broken"),
            (KEYS[5], r#"{"saveFor":"no","keepAliveFor":"8h"}"#),
            (KEYS[9], "invalid"),
            (KEYS[11], "Infinity"),
            (KEYS[12], "TRUE"),
            (KEYS[13], "bad"),
        ]),
    )
    .unwrap();
    let s = commit_candidate(&p, 1, false).unwrap();
    assert!(s.pins.is_empty());
    assert_eq!(s.theme, tezbar_ui_model::ui_state::Theme::System);
    assert!(!s.auto_update_enabled);
    assert_eq!(s.terminal_defaults.save_for, "week");
    assert_eq!(s.terminal_defaults.keep_alive_for, "8h");
    assert_eq!(s.last_update_check, None);
    assert_eq!(s.settings_tab, "general");
    assert!(s.imported_legacy);
}
#[test]
fn pins_limit_and_order_reuse_existing_normalizer() {
    let pins:Vec<_>=(0..20).map(|i|serde_json::json!({"id":format!("id-{i}"),"title":format!("Pin {i}"),"subtitle":"","category":"command","action":{"type":"invoke-command","commandId":"open-settings"},"slot":999})).collect();
    let raw = serde_json::to_string(&pins).unwrap();
    let plan = prepare(None, 0, complete(&[(KEYS[0], &raw)])).unwrap();
    let state = commit_candidate(&plan, 0, false).unwrap();
    assert_eq!(state.pins.len(), 12);
    for (i, p) in state.pins.iter().enumerate() {
        assert_eq!(p.id, format!("id-{i}"));
        assert_eq!(p.slot, i + 1);
    }
}
#[test]
fn existing_native_even_defaults_unknown_fields_and_failed_write_retry_win() {
    let mut state = UiState::default();
    state.unknown.insert("future".into(), serde_json::json!(1));
    let raw = state.encode().unwrap();
    let p = prepare(Some(&raw), 7, complete(&[(KEYS[9], "dark")])).unwrap();
    match p {
        Prepared::Existing(s) => assert_eq!(s, state),
        _ => panic!("existing native state must win"),
    };
    assert!(prepare(Some("broken"), 0, complete(&[])).is_err());
    let p = prepare(None, 7, complete(&[(KEYS[9], "dark")])).unwrap();
    // An atomic-write failure does not ack or mutate this plan. Retry is admitted only
    // while the owner still has revision 7 and the native destination is still absent.
    assert!(commit_candidate(&p, 7, false).is_some());
    assert!(commit_candidate(&p, 7, false).is_some());
    assert!(commit_candidate(&p, 8, false).is_none());
    assert!(commit_candidate(&p, 7, true).is_none());
    assert!(matches!(
        prepare(None, 0, ReadReport::unavailable(Diagnostic::Missing)).unwrap(),
        Prepared::Retry(_)
    ));
}
struct FixtureOrigin;
impl webkit::OriginVerifier for FixtureOrigin {
    fn verify(&self, path: &std::path::Path, origin: &str) -> Result<(), Diagnostic> {
        if origin == webkit::ORIGIN
            && path.file_name().and_then(|p| p.to_str()) == Some("synthetic.sqlite3")
        {
            Ok(())
        } else {
            Err(Diagnostic::OriginUnverified)
        }
    }
}
#[test]
fn sqlite_fixture_bytes_utf16_corruption_wal_and_read_only_immutability() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("synthetic.sqlite3");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE ItemTable(key TEXT UNIQUE, value BLOB NOT NULL);")
        .unwrap();
    let value: Vec<u8> = "dark".encode_utf16().flat_map(u16::to_le_bytes).collect();
    db.execute(
        "INSERT INTO ItemTable VALUES(?1,?2)",
        rusqlite::params![KEYS[9], value],
    )
    .unwrap();
    drop(db);
    let before = std::fs::read(&path).unwrap();
    let report = webkit::read(&path, &FixtureOrigin);
    assert!(report.complete);
    assert_eq!(report.values[KEYS[9]], "dark");
    assert_eq!(before, std::fs::read(&path).unwrap());
    assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
    std::fs::write(
        d.path().join("synthetic.sqlite3-wal"),
        b"synthetic WAL, never replay",
    )
    .unwrap();
    assert!(!webkit::read(&path, &FixtureOrigin).complete);
    std::fs::remove_file(d.path().join("synthetic.sqlite3-wal")).unwrap();
    std::fs::write(&path, b"broken").unwrap();
    assert!(!webkit::read(&path, &FixtureOrigin).complete);
}
#[test]
fn cache_payloads_are_rebuilt_and_partial_transport_does_not_set_marker() {
    let p = prepare(
        None,
        0,
        complete(&[
            (KEYS[3], "huge cache never parsed"),
            (KEYS[4], "broken cache"),
            (KEYS[12], "true"),
        ]),
    )
    .unwrap();
    assert!(commit_candidate(&p, 0, false).unwrap().auto_update_enabled);
    let mut partial = complete(&[(KEYS[9], "dark")]);
    partial.complete = false;
    assert!(commit_candidate(&prepare(None, 0, partial).unwrap(), 0, false).is_none());
}

#[test]
fn failed_atomic_write_does_not_record_marker_and_retry_uses_same_plan() {
    let d = tempfile::tempdir().unwrap();
    let destination = d.path().join("missing-parent/ui-state-v1.json");
    let p = prepare(None, 9, complete(&[(KEYS[9], "dark")])).unwrap();
    let candidate = commit_candidate(&p, 9, false).unwrap();
    assert!(candidate.write_atomic(&destination).is_err());
    assert!(!destination.exists());
    std::fs::create_dir(destination.parent().unwrap()).unwrap();
    commit_candidate(&p, 9, false)
        .unwrap()
        .write_atomic(&destination)
        .unwrap();
    let stored = std::fs::read_to_string(&destination).unwrap();
    assert!(UiState::decode(&stored).unwrap().imported_legacy);
    assert!(matches!(
        prepare(Some(&stored), 10, complete(&[(KEYS[9], "white")])).unwrap(),
        Prepared::Existing(_)
    ));
}

#[test]
fn every_reachable_browser_key_maps_to_the_existing_state_without_a_second_model() {
    let mut expected = UiState::default();
    expected.imported_legacy = true;
    expected.theme = tezbar_ui_model::ui_state::Theme::Dark;
    expected.auto_update_enabled = true;
    expected.release_notes_version = Some("fixture-version".into());
    expected.last_update_check = Some(123456.0);
    expected.recent_extension_commands = vec!["extcmd:fixture:command".into()];
    expected.launcher_query_history = vec!["query".into()];
    expected.terminal_pinned_sessions = vec!["fixture-session".into()];
    expected.recent_emojis = vec!["😀".into()];
    expected.settings_tab = "storage".into();
    let mut values = expected.export_legacy();
    values.insert(KEYS[3].into(), "invalid cache bytes".into());
    values.insert(KEYS[4].into(), "invalid cache bytes".into());
    assert_eq!(values.len(), 14);
    let p = prepare(
        None,
        1,
        ReadReport {
            values,
            complete: true,
            ..ReadReport::default()
        },
    )
    .unwrap();
    assert_eq!(commit_candidate(&p, 1, false).unwrap(), &expected);
}

#[test]
fn bootstrap_existing_defaults_win_missing_and_corrupt_preserve_retry_without_any_write() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("ui-state-v1.json");
    let source = Source::WebKit(d.path().join("missing-origin"));
    assert!(bootstrap(&path, 0, &source).is_err());
    assert!(!path.exists());
    let state = UiState::default();
    state.write_atomic(&path).unwrap();
    assert_eq!(bootstrap(&path, 0, &source).unwrap(), state);
    std::fs::write(&path, b"broken original").unwrap();
    assert!(bootstrap(&path, 0, &source).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"broken original");
}
#[test]
fn atomic_bootstrap_cannot_replace_a_newer_native_state_even_when_plan_was_prepared() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("ui-state-v1.json");
    let p = prepare(None, 0, complete(&[(KEYS[9], "dark")])).unwrap();
    UiState::default().write_atomic(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(
        commit_candidate(&p, 0, false)
            .unwrap()
            .write_atomic_if_absent(&path)
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
#[test]
fn real_sqlite_wal_is_recovered_only_in_private_copy_and_original_bytes_remain_identical() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("synthetic.sqlite3");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE ItemTable(key TEXT UNIQUE,value BLOB NOT NULL);").unwrap();
    let bytes: Vec<u8> = "dark".encode_utf16().flat_map(u16::to_le_bytes).collect();
    db.execute(
        "INSERT INTO ItemTable VALUES(?1,?2)",
        rusqlite::params![KEYS[9], bytes],
    )
    .unwrap();
    let snapshot = || {
        [
            "synthetic.sqlite3",
            "synthetic.sqlite3-wal",
            "synthetic.sqlite3-shm",
        ]
        .map(|n| std::fs::read(d.path().join(n)).unwrap())
    };
    let before = snapshot();
    assert!(!webkit::read(&path, &FixtureOrigin).complete);
    let report = webkit::read_consistent(&path, &FixtureOrigin);
    assert!(report.complete, "{:?}", report.diagnostics);
    assert_eq!(report.values[KEYS[9]], "dark");
    assert_eq!(snapshot(), before);
    drop(db);
}
#[test]
fn first_launch_discovers_attested_modern_store_commits_marker_and_existing_native_wins() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("WebsiteData/Default");
    let client = root.join("top-hash/client-hash");
    std::fs::create_dir_all(client.join("LocalStorage")).unwrap();
    fn text(s: &str) -> Vec<u8> {
        [
            (s.len() as u32).to_le_bytes().to_vec(),
            vec![1],
            s.as_bytes().to_vec(),
        ]
        .concat()
    }
    let origin = [text("tauri"), text("localhost"), vec![0]].concat();
    std::fs::write(client.join("origin"), [origin.clone(), origin].concat()).unwrap();
    let db = rusqlite::Connection::open(client.join("LocalStorage/localstorage.sqlite3")).unwrap();
    db.execute_batch("CREATE TABLE ItemTable(key TEXT UNIQUE,value BLOB NOT NULL);")
        .unwrap();
    let value: Vec<u8> = "dark".encode_utf16().flat_map(u16::to_le_bytes).collect();
    db.execute(
        "INSERT INTO ItemTable VALUES(?1,?2)",
        rusqlite::params![KEYS[9], value],
    )
    .unwrap();
    drop(db);
    let destination = d.path().join("ui-state-v1.json");
    let source = Source::WebKit(root);
    let state = bootstrap(&destination, 0, &source).unwrap();
    assert!(state.imported_legacy);
    assert_eq!(state.theme, tezbar_ui_model::ui_state::Theme::Dark);
    assert_eq!(
        UiState::decode(&std::fs::read_to_string(&destination).unwrap()).unwrap(),
        state
    );
    UiState::default().write_atomic(&destination).unwrap();
    assert_eq!(
        bootstrap(&destination, 1, &source).unwrap(),
        UiState::default()
    );
}

#[test]
fn review_m11_fresh_install_defaults_but_missing_upgrade_profile_still_refuses() {
    let d = tempfile::tempdir().unwrap();
    let config = d.path().join("config");
    let data = d.path().join("data");
    let store = d.path().join("WebKit/com.tezbar.app/WebsiteData/Default");
    let source = Source::WebKit(store.clone()).for_install(&config, &data);
    // Production snapshots FirstInstall, then creates only data/log directories
    // before Launcher hydrates. It does not create ~/.openray in advance.
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(data.join("logs")).unwrap();
    assert!(!config.exists());
    let path = config.join("ui-state.json");
    let state = bootstrap(&path, 0, &source).unwrap();
    assert!(!state.imported_legacy);
    assert!(path.is_file());
    assert_eq!(bootstrap(&path, 1, &source).unwrap(), state);
    let upgrade = Source::WebKit(store).for_install(&config, &data);
    let untouched = config.join("upgrade.json");
    assert!(bootstrap(&untouched, 0, &upgrade).is_err());
    assert!(!untouched.exists());
}
