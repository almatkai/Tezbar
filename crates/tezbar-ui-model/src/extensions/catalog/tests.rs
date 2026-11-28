use super::*;
fn entry(id: &str) -> Entry {
    serde_json::from_value(json!({"id":id,"name":id})).unwrap()
}
fn native() -> Value {
    json!({"id":"native","totalCommands":1,"commands":[{"name":"open","runtime":"rust","platforms":["macOS","Windows"]}],"status":"complete","verification":[]})
}
#[test]
fn installed_artwork_and_legacy_icon_normalization() {
    let mut e = entry("a");
    e.metadata.insert("artwork".into(), json!("/artwork"));
    e.metadata.insert("iconPath".into(), json!("/icon"));
    let e = e.normalize_installed();
    assert_eq!(e.metadata["iconUrl"], "/icon");
    assert_eq!(
        merge_catalog(&[], &[e], "")[0].metadata["artwork"],
        "/artwork"
    );
}
#[test]
fn stale_loads_are_ignored_and_commit_atomically() {
    let mut c = Catalog::default();
    c.search("a".into(), false);
    c.search("b".into(), false);
    c.loaded(2, Ok((vec![entry("b")], vec![], 1, false)), false);
    c.loaded(1, Ok((vec![entry("a")], vec![], 1, false)), false);
    assert!(!c.loading);
    assert_eq!(c.store[0].id, "b");
}
#[test]
fn pagination_selection_progress_and_errors() {
    let mut c = Catalog::default();
    c.search("".into(), false);
    c.loaded(1, Ok((vec![entry("a")], vec![], 50, true)), false);
    assert!(c.search("".into(), true).is_some());
    c.loaded(2, Ok((vec![entry("b")], vec![], 50, true)), true);
    assert_eq!(c.selected.as_deref(), Some("b"));
    assert_eq!(c.store.len(), 2);
    c.progress("b", 52);
    c.progress("b", 100);
    assert!(c.installing.is_empty());
    c.search("".into(), true);
    c.loaded(3, Err("Network error".into()), false);
    assert_eq!(c.message.as_deref(), Some("Network error"));
}
#[test]
fn runtime_claims_require_complete_current_platform_metadata() {
    let mut e = entry("native");
    e.metadata.insert("runtime".into(), json!("rust"));
    assert_eq!(e.effective_runtime("macOS"), None);
    e.metadata.insert("nativeV2".into(), native());
    assert_eq!(e.effective_runtime("macOS"), Some("rust"));
    e.metadata.get_mut("nativeV2").unwrap()["commands"][0]["platforms"] = json!(["macOS"]);
    assert_eq!(e.effective_runtime("Windows"), Some("javascript"));
    assert_eq!(
        e.runtime_description("Windows").unwrap().1,
        "No commands available on Windows"
    );
}
#[test]
fn mixed_runtime_counts_and_unavailable_status() {
    let mut e = entry("native");
    let mut n = native();
    n["commands"] = json!([{"name":"a","runtime":"rust","platforms":["macOS"]},{"name":"b","runtime":"javascript","platforms":["macOS"]}]);
    n["totalCommands"] = json!(2);
    e.metadata.insert("nativeV2".into(), n);
    assert_eq!(e.effective_runtime("macOS"), Some("mixed"));
    assert_eq!(e.runtime_description("macOS").unwrap().0, "Rust 1 of 2");
    assert_eq!(
        command_status(&json!({"runtime":"rust","platforms":["macOS"]}), "Windows"),
        "Not on Windows"
    );
}
#[test]
fn native_local_import_and_malformed_acquisition_cannot_remote_install() {
    let mut e = entry("native");
    e.metadata.insert("nativeV2".into(), native());
    for a in [
        json!(null),
        json!({}),
        json!({"type":"unknown"}),
        json!([]),
        json!({"type":"local-import-only"}),
    ] {
        e.metadata.insert("acquisition".into(), a);
        assert!(!e.can_install("macOS"));
        assert!(e.acquisition_guidance().is_some());
    }
    e.metadata
        .insert("acquisition".into(), json!({"type":"remote"}));
    assert!(e.can_install("macOS"));
}
#[test]
fn installed_javascript_overrides_native_metadata_and_missing_rows_filter() {
    let mut store = entry("raycast.foo");
    store.metadata.insert("nativeV2".into(), native());
    let mut local = entry("raycast.foo");
    local.metadata.insert("runtime".into(), json!("javascript"));
    let rows = merge_catalog(&[store], &[local, entry("raycast.local")], "foo");
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].metadata.contains_key("nativeV2"));
    assert_eq!(rows[0].effective_runtime("macOS"), Some("javascript"));
}
#[test]
fn native_and_raycast_twins_stay_independent() {
    let rows = merge_catalog(
        &[
            entry("before"),
            entry("raycast.foo"),
            entry("native"),
            entry("after"),
        ],
        &[],
        "",
    );
    assert_eq!(
        rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        vec!["before", "raycast.foo", "native", "after"]
    );
}
#[test]
fn latest_dated_checks_and_platform_order() {
    let mut e = entry("native");
    let mut n = native();
    n["verification"] = json!([{"platform":"macOS","state":"passed"},{"platform":"macOS","state":"passed","checkedAt":100},{"platform":"macOS","state":"failed","checkedAt":200,"note":"Latest failed"},{"platform":"Windows","state":"passed","checkedAt":100,"method":"automated"}]);
    e.metadata.insert("nativeV2".into(), n);
    let lines = e.checked_lines("Windows", |_| "Today".into());
    assert_eq!(
        lines,
        vec![
            "Windows — passed Today (automated)",
            "macOS — failed Today: Latest failed"
        ]
    );
}
#[test]
fn commands_only_in_native_metadata_are_appended() {
    let mut e = entry("native");
    e.metadata
        .insert("commands".into(), json!([{"name":"old","title":"Old"}]));
    e.metadata.insert("nativeV2".into(), native());
    assert_eq!(e.commands().len(), 2);
}
#[test]
fn github_url_search_does_not_query_store() {
    let mut c = Catalog::default();
    assert!(c.search("https://github.com/a/b".into(), false).is_none());
    assert!(c.store.is_empty());
    assert!(c.search("foo".into(), false).is_some());
}
#[test]
fn visible_event_refresh_coalesces_hides_and_recovers_on_show() {
    let mut v = VisibleRefresh::default();
    assert!(v.start(0));
    v.changed(0);
    v.changed(10);
    assert!(!v.tick(149));
    assert!(v.tick(150));
    assert!(!v.visibility(false, 151));
    v.changed(152);
    assert!(!v.tick(30_000));
    assert!(v.visibility(true, 30_001));
    assert!(v.visibility(true, 30_002));
    assert!(v.tick(50_002));
    v.stop();
    assert!(!v.tick(90_000));
}

#[test]
fn paging_guards_retry_stale_queries_and_no_progress() {
    let mut c = Catalog::default();
    c.search("old".into(), false);
    assert!(c.search("old".into(), true).is_none());
    c.loaded(1, Ok((vec![entry("a")], vec![], 100, true)), false);
    c.selected = Some("a".into());
    assert_eq!(
        c.search("old".into(), true),
        Some(Effect::Search {
            token: 2,
            query: "old".into(),
            offset: 1,
            limit: 25,
        })
    );
    assert!(c.search("old".into(), true).is_none());
    c.loaded(2, Err("offline".into()), false);
    assert!(c.pagination_failed);
    c.search("old".into(), true);
    assert!(!c.pagination_failed);
    c.loaded(
        3,
        Ok((vec![entry("a"), entry("b")], vec![], 100, true)),
        false,
    );
    assert_eq!(c.store.len(), 2);
    assert_eq!(c.selected.as_deref(), Some("a"));
    assert!(c.message.is_none());
    assert!(matches!(
        c.search("old".into(), true),
        Some(Effect::Search { offset: 3, .. })
    ));
    c.search("new".into(), false);
    c.loaded(4, Err("stale failure".into()), false);
    assert!(c.loading);
    assert!(!c.pagination_failed);
    c.loaded(5, Ok((vec![entry("new")], vec![], 100, true)), false);
    c.search("new".into(), true);
    c.loaded(6, Ok((vec![entry("new")], vec![], 100, true)), false);
    assert!(!c.has_more);
    assert_eq!(c.store.len(), 1);
    assert!(c.search("new".into(), true).is_none());
}

#[test]
fn empty_next_page_stops_even_when_server_claims_more() {
    let mut c = Catalog::default();
    c.search("".into(), false);
    c.loaded(1, Ok((vec![entry("a")], vec![], 100, true)), false);
    c.search("".into(), true);
    c.loaded(2, Ok((vec![], vec![], 100, true)), false);
    assert!(!c.has_more);
}

#[test]
fn failed_replacement_query_cannot_page_old_rows() {
    let mut c = Catalog::default();
    c.search("old".into(), false);
    c.loaded(1, Ok((vec![entry("old")], vec![], 100, true)), false);
    c.search("new".into(), false);
    c.loaded(2, Err("offline".into()), false);
    assert!(!c.has_more);
    assert!(c.search("new".into(), true).is_none());
    c.search("new".into(), false);
    c.loaded(3, Ok((vec![entry("new")], vec![], 100, true)), false);
    assert!(matches!(
        c.search("new".into(), true),
        Some(Effect::Search { offset: 1, .. })
    ));
}
