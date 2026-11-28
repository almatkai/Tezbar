use super::*;
use serde_json::json;
fn snapshot() -> Snapshot {
    serde_json::from_value(json!({"settings":{"depth":"smart","embeddingModel":"embeddinggemma-2","indexingSpeed":"medium"},"roots":[{"id":"a"}],"status":{"state":"indexing","sourceCount":4,"chunkCount":8},"semanticSearch":{"supported":true,"modelState":"ready","indexedChunks":120,"totalChunks":800,"indexing":true}})).unwrap()
}
fn ready() -> KnowledgeSettings {
    let mut k = KnowledgeSettings::default();
    k.activate(true, 0);
    k.read_result(1, Ok(snapshot()), 0);
    k
}
fn read_token(e: &[Effect]) -> u64 {
    match e[0] {
        Effect::Read { token } => token,
        _ => panic!(),
    }
}
fn mutation_token(e: &[Effect]) -> u64 {
    match e[0] {
        Effect::Mutate { token, .. } => token,
        _ => panic!(),
    }
}
#[test]
fn unknown_loading_and_no_actions_before_snapshot() {
    let mut k = KnowledgeSettings::default();
    k.activate(true, 0);
    assert!(k.loading);
    assert_eq!(k.selected(), "");
    assert!(k.files().is_empty());
    assert!(k.begin(Operation::AddMajor, "major".into(), 0).is_empty());
}
#[test]
fn hung_initial_load_is_bounded_and_late_result_ignored() {
    let mut k = KnowledgeSettings::default();
    k.activate(true, 0);
    k.tick(10_000);
    assert!(k.error.as_ref().unwrap().contains("timed out"));
    assert!(k.tick(90_000).is_empty());
    let t = read_token(&k.refresh(90_000, false));
    k.read_result(t, Ok(snapshot()), 90_001);
    k.read_result(1, Err("late".into()), 90_002);
    assert!(k.error.is_none());
}
#[test]
fn refresh_supersedes_initial_and_rejection() {
    let mut k = KnowledgeSettings::default();
    k.activate(true, 0);
    let t = read_token(&k.refresh(1, false));
    k.read_result(t, Ok(snapshot()), 2);
    k.read_result(1, Err("obsolete".into()), 3);
    assert!(k.error.is_none());
}
#[test]
fn zero_roots_only_after_success() {
    let mut k = KnowledgeSettings::default();
    k.activate(true, 0);
    k.read_result(1, Err("unavailable".into()), 1);
    assert!(k.snapshot.is_none());
    let t = read_token(&k.refresh(2, false));
    let mut s = snapshot();
    s.roots.clear();
    k.read_result(t, Ok(s), 3);
    assert!(k.snapshot.as_ref().unwrap().roots.is_empty());
}
#[test]
fn failed_refresh_preserves_snapshot_controls_and_downloads() {
    let mut k = ready();
    let t = read_token(&k.refresh(1, false));
    k.read_result(t, Err("failed".into()), 2);
    assert_eq!(k.snapshot.as_ref().unwrap().roots.len(), 1);
    assert_eq!(k.buttons(Variant::Compact)[0].label, "Download");
    assert_eq!(k.selected(), "embeddinggemma-2:q8_0");
}
#[test]
fn polling_is_serial_and_quiet_and_failure_requires_retry() {
    let mut k = ready();
    let t = read_token(&k.tick(1500));
    assert!(!k.loading);
    assert!(k.tick(3000).is_empty());
    k.read_result(t, Err("poll failed".into()), 3001);
    assert!(k.tick(30_000).is_empty());
    assert!(k.visible(false, 30_001).is_empty());
    assert!(k.visible(true, 30_002).is_empty());
}
#[test]
fn visibility_refreshes_once_and_pauses_polling() {
    let mut k = ready();
    assert!(k.visible(false, 1).is_empty());
    assert!(k.tick(2000).is_empty());
    assert_eq!(k.visible(true, 2001).len(), 1);
    assert!(!k.loading);
    assert!(k.visible(true, 2002).is_empty());
}
#[test]
fn mutation_snapshot_invalidates_older_read() {
    let mut k = ready();
    let read = read_token(&k.refresh(1, false));
    let t = mutation_token(&k.begin(
        Operation::Download(Variant::Compact),
        "download:q4_k_m".into(),
        2,
    ));
    let mut s = snapshot();
    s.settings.embedding_variant = Some(Variant::Compact);
    k.mutation_result(t, Ok(s), 3);
    k.read_result(read, Ok(snapshot()), 4);
    assert_eq!(k.selected(), "embeddinggemma-2:q4_k_m");
}
#[test]
fn tab_exit_unmount_and_obsolete_loads() {
    let mut k = ready();
    let t = read_token(&k.refresh(1, false));
    k.activate(false, 2);
    k.read_result(t, Err("late".into()), 3);
    assert!(k.error.is_none());
    assert!(k.tick(50_000).is_empty());
    assert_eq!(k.activate(true, 50_001).len(), 1);
    assert!(k.activate(true, 50_002).is_empty());
}
#[test]
fn enable_installed_prefers_standard_without_download() {
    let mut k = ready();
    k.snapshot.as_mut().unwrap().settings.embedding_model = "built-in".into();
    assert_eq!(k.enable_variant(), Some(Variant::Standard));
    assert!(matches!(
        k.begin(
            Operation::Switch(Some(Variant::Standard)),
            "enable".into(),
            0
        )[0],
        Effect::Mutate {
            operation: Operation::Switch(_),
            ..
        }
    ));
}
#[test]
fn legacy_variants_and_weights() {
    let k = ready();
    let files = k.files();
    assert_eq!(files[0].weight_bytes, 309_855_456);
    assert_eq!(files[1].weight_bytes, 181_695_104);
    assert_eq!(files[1].state, FileState::NotDownloaded);
    assert_eq!(k.selected(), "embeddinggemma-2:q8_0");
}
#[test]
fn counts_only_for_selected_index_variant() {
    let mut k = ready();
    k.snapshot.as_mut().unwrap().settings.embedding_variant = Some(Variant::Compact);
    assert!(k.semantic_summary().unwrap().1.is_none());
    assert_eq!(k.snapshot.as_ref().unwrap().status["chunkCount"], 8);
}
#[test]
fn legacy_startup_and_integrity_are_attributed_and_sanitized() {
    let mut k = ready();
    let s = &mut k.snapshot.as_mut().unwrap().semantic_search;
    s.error = Some("private-hash".into());
    s.error_source = Some("integrity".into());
    assert!(k.alerts().iter().all(|s| !s.contains("private-hash")));
    assert_eq!(k.buttons(Variant::Standard)[0].label, "Repair");
    let s = &mut k.snapshot.as_mut().unwrap().semantic_search;
    s.error_source = Some("runtime".into());
    assert_eq!(k.buttons(Variant::Standard)[0].label, "Retry start");
    assert!(k.semantic_summary().unwrap().0.contains("couldn't start"));
}
#[test]
fn switch_timeout_reverts_selection_and_requires_fresh_read() {
    let mut k = ready();
    k.begin(
        Operation::Switch(Some(Variant::Compact)),
        "select".into(),
        0,
    );
    assert_eq!(k.selected(), "embeddinggemma-2:q4_k_m");
    let t = read_token(&k.tick(15_000));
    assert_eq!(k.selected(), "embeddinggemma-2:q8_0");
    assert_eq!(k.phase, Phase::Reconciling);
    k.status_event(json!({"state":"completed"}));
    assert_eq!(k.phase, Phase::Reconciling);
    k.read_result(t, Ok(snapshot()), 15_001);
    assert_eq!(k.phase, Phase::Idle);
}
#[test]
fn failed_reconcile_keeps_lock_until_explicit_refresh() {
    let mut k = ready();
    k.begin(Operation::Speed("fast".into()), "speed:fast".into(), 0);
    let t = read_token(&k.tick(15_000));
    k.read_result(t, Err("no".into()), 15_001);
    assert_eq!(k.phase, Phase::Unconfirmed);
    assert!(k
        .begin(Operation::Speed("slow".into()), "speed:slow".into(), 15_002)
        .is_empty());
    let t = read_token(&k.refresh(15_003, false));
    k.read_result(t, Ok(snapshot()), 15_004);
    assert_eq!(k.phase, Phase::Idle);
}
#[test]
fn trigger_is_focusable_inert_and_double_activation_blocked() {
    let mut k = ready();
    k.begin(
        Operation::Download(Variant::Compact),
        "download:q4_k_m".into(),
        0,
    );
    let b = &k.buttons(Variant::Compact)[0];
    assert!(b.inert && b.focusable);
    assert!(k
        .begin(Operation::Download(Variant::Compact), b.trigger.clone(), 1)
        .is_empty());
    assert_eq!(k.refresh(2, false).len(), 1);
}
#[test]
fn cancel_supersedes_hung_operation_but_cannot_clear_unknown_barrier() {
    let mut k = ready();
    let old = mutation_token(&k.begin(
        Operation::Remove(Variant::Standard),
        "remove:q8_0".into(),
        0,
    ));
    let cancel = mutation_token(&k.begin(
        Operation::Cancel(Variant::Compact),
        "cancel:q4_k_m".into(),
        1,
    ));
    assert!(k.mutation_result(old, Ok(snapshot()), 2).is_empty());
    let t = read_token(&k.mutation_result(cancel, Ok(snapshot()), 3));
    assert_eq!(k.phase, Phase::Reconciling);
    k.read_result(t, Ok(snapshot()), 4);
    assert_eq!(k.phase, Phase::Idle);
}
#[test]
fn unrelated_mutation_and_old_poll_do_not_reconcile() {
    let mut k = ready();
    let old = read_token(&k.refresh(0, false));
    k.begin(Operation::Switch(None), "select".into(), 0);
    k.tick(15_000);
    k.read_result(old, Ok(snapshot()), 15_001);
    assert_ne!(k.phase, Phase::Idle);
}
#[test]
fn remove_does_not_auto_select_other_variant() {
    let mut k = ready();
    let t = mutation_token(&k.begin(
        Operation::Remove(Variant::Standard),
        "remove:q8_0".into(),
        0,
    ));
    let mut s = snapshot();
    s.settings.embedding_model = "built-in".into();
    k.mutation_result(t, Ok(s), 1);
    assert_eq!(k.selected(), "built-in");
}
#[test]
fn late_mutation_after_unmount_is_consumed() {
    let mut k = ready();
    let t = mutation_token(&k.begin(
        Operation::Download(Variant::Compact),
        "download:q4_k_m".into(),
        0,
    ));
    k.activate(false, 1);
    k.mutation_result(t, Err("late".into()), 2);
    assert!(k.local_error.as_ref().unwrap().contains("late"));
}
#[test]
fn legacy_verification_is_not_download_or_cancel() {
    let mut k = ready();
    k.snapshot.as_mut().unwrap().semantic_search.integrity_state = Some("verifying".into());
    assert!(k.buttons(Variant::Standard).is_empty());
    assert!(k.semantic_summary().unwrap().0.starts_with("Checking"));
}
#[test]
fn actual_legacy_download_stays_cancellable_during_integrity_flag() {
    let mut k = ready();
    let s = &mut k.snapshot.as_mut().unwrap().semantic_search;
    s.integrity_state = Some("verifying".into());
    s.model_state = FileState::Downloading;
    assert_eq!(k.buttons(Variant::Standard)[0].label, "Cancel");
}
#[test]
fn focus_never_lands_on_new_destructive_remove() {
    let k = ready();
    assert_eq!(
        k.recover_focus(Variant::Standard, "start:q8_0"),
        Effect::Focus {
            target: "row:q8_0".into()
        }
    );
}
#[test]
fn download_focus_moves_to_cancel_in_same_row() {
    let mut k = ready();
    k.snapshot.as_mut().unwrap().semantic_search.model_state = FileState::Downloading;
    assert_eq!(
        k.recover_focus(Variant::Standard, "download:q8_0"),
        Effect::Focus {
            target: "cancel:q8_0".into()
        }
    );
}
#[test]
fn verifying_repair_focus_falls_back_to_own_row() {
    let mut k = ready();
    k.snapshot.as_mut().unwrap().semantic_search.integrity_state = Some("verifying".into());
    assert_eq!(
        k.recover_focus(Variant::Standard, "download:q8_0"),
        Effect::Focus {
            target: "row:q8_0".into()
        }
    );
}
#[test]
fn progress_below_one_hundred_and_unknown_total() {
    for (p, want) in [
        (0.0, 0),
        (0.999, 99),
        (1.0, 99),
        (1.5, 99),
        (-1.0, 0),
        (f64::NAN, 0),
    ] {
        assert_eq!(download_percent(p), want);
    }
    assert_eq!(ready().files()[0].download_total_bytes, 0);
}
#[test]
fn authoritative_weights_override_download_totals() {
    let mut k = ready();
    let s = &mut k.snapshot.as_mut().unwrap().semantic_search;
    s.weight_bytes = Some(123);
    s.download_total_bytes = 999;
    assert_eq!(k.files()[0].weight_bytes, 123);
}
#[test]
fn counts_do_not_change_live_transition() {
    let mut k = ready();
    let a = k.semantic_summary().unwrap().0;
    k.snapshot.as_mut().unwrap().semantic_search.indexed_chunks += 20;
    assert_eq!(k.semantic_summary().unwrap().0, a);
}
#[test]
fn unsupported_models_have_no_actions() {
    let mut k = ready();
    k.snapshot.as_mut().unwrap().semantic_search.supported = false;
    assert!(k.buttons(Variant::Standard).is_empty());
    assert_eq!(k.enable_variant(), None);
}
#[test]
fn pause_resume_and_selection_have_distinct_effects() {
    let mut k = ready();
    for op in [
        Operation::Pause,
        Operation::Resume,
        Operation::Switch(Some(Variant::Standard)),
    ] {
        let t = mutation_token(&k.begin(op.clone(), "fixture".into(), 0));
        assert!(matches!(&k.phase,Phase::Pending{operation,..}if *operation==op));
        k.mutation_result(t, Ok(snapshot()), 1);
    }
}
#[test]
fn post_timeout_read_barrier_is_after_synchronous_read_completion() {
    let mut k = ready();
    k.begin(Operation::Switch(None), "select".into(), 0);
    let t = read_token(&k.refresh(1, false));
    k.read_result(t, Ok(snapshot()), 2);
    let fresh = read_token(&k.tick(15_000));
    assert_eq!(k.phase, Phase::Reconciling);
    k.read_result(t, Ok(snapshot()), 15_001);
    assert_eq!(k.phase, Phase::Reconciling);
    k.read_result(fresh, Ok(snapshot()), 15_002);
    assert_eq!(k.phase, Phase::Idle);
}
fn variants() -> Snapshot {
    let mut s = snapshot();
    let mut files = s.semantic_search.files();
    files[1].state = FileState::Ready;
    s.semantic_search.variants = Some(files);
    s.semantic_search.index_variant = Some(Variant::Standard);
    s
}
#[test]
fn late_switch_resolve_and_reject_after_reconciliation() {
    for result in [Ok(snapshot()), Err("obsolete".into())] {
        let mut k = ready();
        let token = mutation_token(&k.begin(
            Operation::Switch(Some(Variant::Compact)),
            "select".into(),
            0,
        ));
        let read = read_token(&k.tick(15_000));
        k.read_result(read, Ok(snapshot()), 15_001);
        assert!(k.mutation_result(token, result, 15_002).is_empty());
        assert_eq!(k.phase, Phase::Idle);
        assert!(k.local_error.is_none());
    }
}
#[test]
fn all_four_mutation_timeout_variants_ignore_late_rejection() {
    for op in [
        Operation::Download(Variant::Compact),
        Operation::Remove(Variant::Standard),
        Operation::Cancel(Variant::Compact),
        Operation::Speed("slow".into()),
    ] {
        let mut k = ready();
        let token = mutation_token(&k.begin(op, "trigger".into(), 0));
        k.tick(15_000);
        assert_eq!(k.phase, Phase::Reconciling);
        k.mutation_result(token, Err("late failure".into()), 15_001);
        assert!(k.local_error.is_none());
    }
}
#[test]
fn structured_verification_both_variants_has_no_cancel_or_download() {
    for id in [Variant::Standard, Variant::Compact] {
        let mut k = ready();
        let mut s = variants();
        s.settings.embedding_variant = Some(id);
        s.semantic_search
            .variants
            .as_mut()
            .unwrap()
            .iter_mut()
            .find(|v| v.id == id)
            .unwrap()
            .state = FileState::Verifying;
        k.snapshot = Some(s);
        assert!(k.buttons(id).is_empty());
        assert_eq!(k.progress()[0].fraction, None);
        assert!(k.semantic_summary().unwrap().0.contains("Checking"));
    }
}
#[test]
fn variant_failure_isolation_and_alert_order_dedup() {
    let mut k = ready();
    let mut s = variants();
    let files = s.semantic_search.variants.as_mut().unwrap();
    files[0].failure = Some(Failure {
        stage: FailureStage::Startup,
        message: "spawn failed".into(),
    });
    files[1].state = FileState::Error;
    files[1].failure = Some(Failure {
        stage: FailureStage::Download,
        message: "disk full".into(),
    });
    k.snapshot = Some(s);
    let t = mutation_token(&k.begin(
        Operation::Download(Variant::Compact),
        "download:q4_k_m".into(),
        0,
    ));
    k.mutation_result(t, Err("disk full".into()), 1);
    assert_eq!(
        k.alerts(),
        vec![
            "Compact (Q4_K_M) download failed: disk full",
            "Standard (Q8_0) is installed but couldn't start: spawn failed"
        ]
    );
    assert_eq!(k.files()[0].state, FileState::Ready);
    assert_eq!(k.buttons(Variant::Compact)[0].label, "Retry download");
}
#[test]
fn separate_progress_and_unknown_totals() {
    let mut k = ready();
    let mut s = variants();
    let f = s.semantic_search.variants.as_mut().unwrap();
    f[0].state = FileState::Verifying;
    f[1].state = FileState::Downloading;
    f[1].download_progress = 0.9999;
    f[1].download_total_bytes = 0;
    k.snapshot = Some(s);
    let p = k.progress();
    assert_eq!(p[0].fraction, None);
    assert_eq!(p[1].percent, Some(99));
    assert_eq!(p[1].bytes, None);
    assert_eq!(k.file_label(&k.files()[1]), "Downloading · 99%");
}
#[test]
fn model_action_focus_does_not_steal_deliberate_destination() {
    let k = ready();
    for destination in ["body", "refresh"] {
        assert_eq!(
            k.recover_owned_focus(Some(Variant::Standard), destination, false),
            None
        );
    }
    for action in ["enable", "retry-index", "start-index"] {
        assert_eq!(
            k.recover_owned_focus(None, action, true),
            Some(Effect::Focus {
                target: "refresh".into()
            })
        );
    }
}
#[test]
fn all_pending_accessible_names_match_visible_verb_and_variant() {
    for (op, trigger, verb) in [
        (
            Operation::Download(Variant::Compact),
            "download:q4_k_m",
            "Starting",
        ),
        (
            Operation::Remove(Variant::Standard),
            "remove:q8_0",
            "Removing",
        ),
        (
            Operation::Cancel(Variant::Standard),
            "cancel:q8_0",
            "Cancelling",
        ),
        (
            Operation::Switch(Some(Variant::Standard)),
            "start:q8_0",
            "Starting",
        ),
    ] {
        let mut k = ready();
        if matches!(op, Operation::Cancel(_)) {
            k.snapshot.as_mut().unwrap().semantic_search.model_state = FileState::Downloading;
        }
        if matches!(op, Operation::Switch(_)) {
            let s = &mut k.snapshot.as_mut().unwrap().semantic_search;
            s.error = Some("spawn failed".into());
            s.error_source = Some("runtime".into());
        }
        k.begin(op, trigger.into(), 0);
        let v = if trigger.ends_with("q4_k_m") {
            Variant::Compact
        } else {
            Variant::Standard
        };
        let b = k
            .buttons(v)
            .into_iter()
            .find(|b| b.trigger == trigger)
            .unwrap();
        assert!(b.accessible_name.starts_with(verb));
        assert!(b.accessible_name.contains(v.name()));
    }
}
#[test]
fn speed_and_switch_control_stay_focusable_but_inert_through_timeout() {
    for (op, trigger) in [
        (Operation::Speed("slow".into()), "speed:slow"),
        (Operation::Switch(Some(Variant::Compact)), "select"),
    ] {
        let mut k = ready();
        k.begin(op.clone(), trigger.into(), 0);
        for now in [1, 15_000, 25_000] {
            k.tick(now);
            let b = k.control(trigger);
            assert!(b.focusable && b.inert);
            assert!(k.begin(op.clone(), trigger.into(), now).is_empty());
        }
    }
}
#[test]
fn only_available_compact_can_enable_without_download() {
    let mut k = ready();
    let mut s = variants();
    s.semantic_search.variants.as_mut().unwrap()[0].state = FileState::NotDownloaded;
    s.settings.embedding_model = "built-in".into();
    k.snapshot = Some(s);
    assert_eq!(k.enable_variant(), Some(Variant::Compact));
    let e = k.begin(Operation::Switch(k.enable_variant()), "enable".into(), 0);
    assert!(matches!(
        e[0],
        Effect::Mutate {
            operation: Operation::Switch(Some(Variant::Compact)),
            ..
        }
    ));
}
#[test]
fn missing_failed_and_unattributed_errors_never_claim_healthy_semantics() {
    for state in [FileState::NotDownloaded, FileState::Error, FileState::Ready] {
        let mut k = ready();
        k.snapshot.as_mut().unwrap().semantic_search.model_state = state;
        k.snapshot.as_mut().unwrap().semantic_search.error = Some("Index unavailable".into());
        assert!(k.semantic_summary().unwrap().0.contains("off"));
    }
}
#[test]
fn core_wire_variant_positions_and_root_mutations() {
    use serde_json::json;
    for v in [Variant::Standard, Variant::Compact] {
        let r = Operation::Switch(Some(v)).request();
        assert_eq!(r.channel, "knowledge:embedding:set");
        assert_eq!(r.args, vec![json!("embeddinggemma-2"), json!(v.id())]);
        for op in [
            Operation::Download(v),
            Operation::Cancel(v),
            Operation::Remove(v),
        ] {
            assert_eq!(op.request().args, vec![json!(v.id())]);
        }
    }
    assert_eq!(
        Operation::RootDepth {
            id: "root".into(),
            depth: "inherit".into()
        }
        .request()
        .args,
        vec![json!({"rootId":"root","depth":"inherit"})]
    );
}
#[test]
fn structured_authoritative_weight_bytes_and_file_labels() {
    let mut k = ready();
    let mut s = variants();
    let f = s.semantic_search.variants.as_mut().unwrap();
    f[0].weight_bytes = 311_600_000;
    f[0].download_total_bytes = 324_000_000;
    f[1].weight_bytes = 183_600_000;
    k.snapshot = Some(s);
    assert_eq!(k.files()[0].weight_bytes, 311_600_000);
    assert_eq!(k.files()[1].weight_bytes, 183_600_000);
    assert_eq!(k.file_label(&k.files()[0]), "Installed · in use");
}
#[test]
fn legacy_variant_argument_contract() {
    for action in [
        EmbeddingAction::Download,
        EmbeddingAction::Cancel,
        EmbeddingAction::Remove,
    ] {
        assert!(embedding_request(action, None).args.is_empty());
        for variant in [Variant::Standard, Variant::Compact] {
            assert_eq!(
                embedding_request(action, Some(variant)).args,
                vec![json!(variant.id())]
            );
        }
    }
}

#[test]
fn mutation_ownership_survives_tab_switches_and_stale_results() {
    for hidden_completion in [false, true] {
        let mut k = ready();
        let token = mutation_token(&k.begin(Operation::AddMajor, "major".into(), 0));
        k.activate(false, 1);
        if hidden_completion {
            k.mutation_result(token, Err("disk failure".into()), 2);
        }
        k.activate(true, 3);
        if hidden_completion {
            assert!(k.local_error.as_ref().unwrap().contains("disk failure"));
        } else {
            assert!(k.begin(Operation::AddMajor, "other".into(), 4).is_empty());
            k.mutation_result(token + 100, Err("stale".into()), 5);
            assert!(matches!(k.phase, Phase::Pending { .. }));
            k.mutation_result(token, Err("disk failure".into()), 6);
            assert!(k.local_error.as_ref().unwrap().contains("disk failure"));
        }
        assert_eq!(k.phase, Phase::Idle);
    }
}
#[test]
fn cancel_accessible_name_includes_download() {
    let mut k = ready();
    let mut s = snapshot();
    s.semantic_search.model_state = FileState::Downloading;
    k.snapshot = Some(s);
    let cancel = k
        .buttons(Variant::Standard)
        .into_iter()
        .find(|b| b.label == "Cancel")
        .unwrap();
    assert_eq!(cancel.accessible_name, "Cancel Standard (Q8_0) download");
}

#[test]
fn failed_tab_read_cannot_replace_cancel_mutation_owner() {
    let mut k = ready();
    k.begin(
        Operation::Switch(Some(Variant::Compact)),
        "switch".into(),
        0,
    );
    let cancel = mutation_token(&k.begin(Operation::Cancel(Variant::Compact), "cancel".into(), 1));
    k.activate(false, 2);
    let read = read_token(&k.activate(true, 3));
    k.read_result(read, Err("read failed".into()), 4);
    assert!(matches!(k.phase,Phase::Pending {token,..} if token==cancel));
    assert!(k.begin(Operation::AddMajor, "major".into(), 5).is_empty());
    let effects = k.mutation_result(cancel, Err("cancel failed".into()), 6);
    assert!(k.local_error.as_ref().unwrap().contains("cancel failed"));
    assert_eq!(k.phase, Phase::Reconciling);
    assert!(!effects.is_empty());
}
