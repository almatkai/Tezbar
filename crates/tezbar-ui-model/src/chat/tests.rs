use super::*;
fn controller() -> ChatController {
    let mut c = ChatController::default();
    c.boot(
        Boot::NewChat {
            session_id: "a".into(),
            cwd: Some("/fixture/a".into()),
        },
        Some("1".into()),
    )
    .unwrap();
    c
}
fn start(c: &mut ChatController, id: &str, run: &str) {
    c.edit_draft(id, "Canonical question".into());
    let Effect::AppendUser { token, turn, .. } = c
        .send(id, run.into(), format!("u-{run}"), format!("s-{run}"), 1)
        .unwrap()
    else {
        panic!()
    };
    assert!(matches!(
        c.prepared(id, token, Ok(turn), 2)[0],
        Effect::Start(_)
    ));
}
fn last(c: &ChatController, id: &str) -> Arc<Turn> {
    c.owners[id].turns.back().unwrap().clone()
}
#[test]
fn history_popover_refresh_preserves_active_run_draft_and_focus() {
    let mut c = controller();
    start(&mut c, "a", "r");
    c.edit_draft("a", "Next question".into());
    c.owners.get_mut("a").unwrap().focus = ApprovalFocus::Deny;
    let generation = c.boot_generation;
    let Effect::LoadHistory { token: old } = c.refresh_history() else {
        panic!()
    };
    let Effect::LoadHistory { token: current } = c.refresh_history() else {
        panic!()
    };
    c.history_result(old, vec![]);
    assert!(c.history.is_none());
    c.history_result(current, vec![]);
    assert!(c.history.as_ref().unwrap().is_empty());
    assert_eq!(c.active.as_deref(), Some("a"));
    assert_eq!(c.boot_generation, generation);
    assert!(c.conversation("a").unwrap().busy());
    assert_eq!(c.conversation("a").unwrap().draft, "Next question");
    assert_eq!(c.conversation("a").unwrap().focus, ApprovalFocus::Deny);
}
#[test]
fn background_runs_and_unsaved_persistence_stay_with_owner() {
    let mut c = controller();
    start(&mut c, "a", "r-a");
    c.boot(
        Boot::NewChat {
            session_id: "b".into(),
            cwd: None,
        },
        None,
    )
    .unwrap();
    start(&mut c, "b", "r-b");
    c.event(Event::Message {
        run_id: "r-a".into(),
        delta: "Private A".into(),
    });
    let e = c.event(Event::Done {
        run_id: "r-a".into(),
    });
    let Effect::PersistTurn { token, .. } = e[0] else {
        panic!()
    };
    c.persisted("a", token, false);
    assert!(c.conversation("a").unwrap().unsaved());
    assert_eq!(last(&c, "a").text, "Private A");
    assert!(
        c.conversation("b")
            .unwrap()
            .response()
            .unwrap()
            .text
            .is_empty()
    );
    assert!(
        c.boot(
            Boot::Resume {
                session_id: "a".into()
            },
            Some("2".into())
        )
        .unwrap()
        .is_empty()
    );
}
#[test]
fn settled_persisted_conversation_reloads_after_owner_release() {
    let mut c = controller();
    start(&mut c, "a", "r");
    let e = c.event(Event::Error {
        run_id: "r".into(),
        message: "fixture".into(),
    });
    let Effect::PersistTurn { token, .. } = e[0] else {
        panic!()
    };
    c.persisted("a", token, true);
    assert!(matches!(
        c.boot(
            Boot::Resume {
                session_id: "a".into()
            },
            None
        )
        .unwrap()[0],
        Effect::LoadConversation { .. }
    ));
}
#[test]
fn stop_preserves_partial_and_terminal_stage_and_ignores_late_events() {
    let mut c = controller();
    start(&mut c, "a", "r");
    c.event(Event::Message {
        run_id: "r".into(),
        delta: "Partial".into(),
    });
    c.event(Event::Stage {
        run_id: "r".into(),
        stage: Stage {
            index: 1,
            label: "tool".into(),
            status: StageStatus::Running,
            detail: None,
            output: Some("success".into()),
            tool_name: None,
            duration_ms: None,
            output_truncated: false,
        },
    });
    let e = c.stop("a");
    assert!(matches!(e[0], Effect::Cancel { .. }));
    let turn = last(&c, "a");
    assert_eq!(turn.text, "Partial");
    assert_eq!(turn.stages[0].status, StageStatus::Failed);
    assert_eq!(turn.stages[0].output.as_deref(), Some("success"));
    assert!(
        c.event(Event::Answer {
            run_id: "r".into(),
            text: "late".into()
        })
        .is_empty()
    );
}
#[test]
fn retry_reuses_assistant_id_and_canonical_stored_user() {
    let mut c = controller();
    start(&mut c, "a", "r");
    c.event(Event::Error {
        run_id: "r".into(),
        message: "failed".into(),
    });
    let Effect::PrepareRetry { token, .. } = c.retry("a", "s-r", "retry".into()).unwrap() else {
        panic!()
    };
    let user = c.owners["a"].turns.front().unwrap().clone();
    let e = c.prepared("a", token, Ok(user), 3);
    let Effect::Start(r) = &e[0] else { panic!() };
    assert_eq!(r.assistant_turn_id, "s-r");
    assert!(r.retry);
    c.event(Event::Answer {
        run_id: "retry".into(),
        text: "Recovered".into(),
    });
    c.event(Event::Done {
        run_id: "retry".into(),
    });
    assert_eq!(
        c.owners["a"].turns.iter().filter(|t| t.id == "s-r").count(),
        1
    );
    assert_eq!(last(&c, "a").text, "Recovered");
}
fn approval(c: &mut ChatController, remember: Option<bool>) {
    c.event(Event::Approval {
        run_id: "r".into(),
        approval: Approval {
            approval_id: "p".into(),
            title: "Allow Fixture to use tool?".into(),
            command: "{}".into(),
            can_remember: remember,
            remember_kind: Some("tool".into()),
        },
    });
}
#[test]
fn approval_resume_focus_enter_deny_and_mode_independence() {
    let mut c = controller();
    start(&mut c, "a", "r");
    approval(&mut c, Some(true));
    assert_eq!(c.owners["a"].focus, ApprovalFocus::Composer);
    assert!(c.enter("a").is_none());
    c.focus("a", ApprovalFocus::AllowOnce);
    assert_eq!(c.owners["a"].focus, ApprovalFocus::AllowOnce);
    c.set_mode("a", ApprovalMode::Auto);
    assert!(c.owners["a"].approval.is_some());
    c.focus("a", ApprovalFocus::Composer);
    assert!(c.enter("a").is_none());
    c.boot(
        Boot::Resume {
            session_id: "a".into(),
        },
        Some("resume".into()),
    )
    .unwrap();
    assert!(c.owners["a"].approval.is_some());
    c.focus("a", ApprovalFocus::Deny);
    assert!(matches!(
        c.enter("a"),
        Some(Effect::Approve {
            decision: Decision::Deny,
            ..
        })
    ));
}
#[test]
fn persistent_approval_capability_and_expiry() {
    for remember in [Some(false), Some(true), None] {
        let mut c = controller();
        start(&mut c, "a", "r");
        approval(&mut c, remember);
        assert_eq!(
            c.decide("a", Decision::Always).is_some(),
            remember != Some(false)
        );
    }
    let mut c = controller();
    start(&mut c, "a", "r");
    approval(&mut c, None);
    c.event(Event::ApprovalExpired {
        run_id: "r".into(),
        approval_id: "p".into(),
    });
    assert!(c.enter("a").is_none());
}
#[test]
fn boot_nonce_dedup_and_nonced_new_chat() {
    let mut c = controller();
    let boot = Boot::NewChat {
        session_id: "b".into(),
        cwd: None,
    };
    c.boot(boot.clone(), Some("2".into())).unwrap();
    assert!(c.boot(boot, Some("2".into())).unwrap().is_empty());
    c.boot(
        Boot::NewChat {
            session_id: "c".into(),
            cwd: None,
        },
        Some("3".into()),
    )
    .unwrap();
    assert_eq!(c.active.as_deref(), Some("c"));
}
#[test]
fn captures_are_deduplicated_and_navigation_hide_fenced() {
    let mut c = controller();
    let Effect::Capture { token, .. } = c.capture().unwrap() else {
        panic!()
    };
    assert!(c.capture().is_none());
    c.set_visible(false);
    c.capture_result(
        token,
        Reference::SignedHistory {
            turn_id: "fixture".into(),
        },
    );
    assert!(c.owners["a"].attachments.is_empty());
    c.set_visible(true);
    let Effect::Capture { token, .. } = c.capture().unwrap() else {
        panic!()
    };
    c.boot(Boot::Panel, None).unwrap();
    c.capture_result(
        token,
        Reference::SignedHistory {
            turn_id: "late".into(),
        },
    );
    assert!(c.owners["a"].attachments.is_empty());
}
#[test]
fn late_history_does_not_clobber_composer_or_new_boot() {
    let mut c = controller();
    let e = c
        .boot(
            Boot::Resume {
                session_id: "a".into(),
            },
            None,
        )
        .unwrap();
    let Effect::LoadConversation { token, .. } = e[0] else {
        panic!()
    };
    c.edit_draft("a", "new draft".into());
    c.history_result_conversation(
        token,
        vec![],
        Selection {
            cwd: Some("wrong".into()),
            ..Default::default()
        },
    );
    assert_eq!(c.owners["a"].draft, "new draft");
    assert_eq!(c.owners["a"].selection.cwd.as_deref(), Some("/fixture/a"));
}
#[test]
fn preparation_keeps_cwd_and_images_with_original_owner() {
    let mut c = controller();
    c.edit_draft("a", "question".into());
    let image = Reference::Image {
        blob_id: "image".into(),
        mime_type: "image/png".into(),
        name: "screen".into(),
        width: None,
        height: None,
    };
    c.owners
        .get_mut("a")
        .unwrap()
        .attachments
        .push(image.clone());
    let Effect::AppendUser { token, turn, .. } =
        c.send("a", "r".into(), "u".into(), "s".into(), 0).unwrap()
    else {
        panic!()
    };
    c.boot(
        Boot::NewChat {
            session_id: "b".into(),
            cwd: None,
        },
        None,
    )
    .unwrap();
    let e = c.prepared("a", token, Ok(turn), 1);
    let Effect::Start(r) = &e[0] else { panic!() };
    assert_eq!(r.selection.cwd.as_deref(), Some("/fixture/a"));
    assert_eq!(r.references, vec![image]);
    assert_eq!(r.session_id, "a");
}
#[test]
fn deletion_during_preparation_never_launches_and_failure_allows_fresh_send() {
    let mut c = controller();
    c.edit_draft("a", "q".into());
    let Effect::AppendUser { token, turn, .. } =
        c.send("a", "r".into(), "u".into(), "s".into(), 0).unwrap()
    else {
        panic!()
    };
    let e = c.delete("a");
    let Effect::DeleteConversation {
        token: deletion, ..
    } = e[0]
    else {
        panic!()
    };
    assert!(c.prepared("a", token, Ok(turn), 1).is_empty());
    c.delete_result("a", deletion, false);
    c.edit_draft("a", "fresh".into());
    assert!(
        c.send("a", "new".into(), "u2".into(), "s2".into(), 2)
            .is_ok()
    );
}
#[test]
fn done_overtakes_delete_without_resurrection_and_failed_delete_recovers() {
    for ok in [true, false] {
        let mut c = controller();
        start(&mut c, "a", "r");
        c.event(Event::Message {
            run_id: "r".into(),
            delta: "partial".into(),
        });
        let e = c.delete("a");
        let Effect::DeleteConversation { token, .. } = e[1] else {
            panic!()
        };
        assert!(c.event(Event::Done { run_id: "r".into() }).is_empty());
        c.delete_result("a", token, ok);
        assert_eq!(c.conversation("a").is_some(), !ok);
        if ok {
            assert!(
                c.boot(
                    Boot::Resume {
                        session_id: "a".into()
                    },
                    None
                )
                .is_err()
            );
        } else {
            assert_eq!(last(&c, "a").text, "partial");
        }
    }
}
#[test]
fn hidden_runs_and_background_approval_survive_until_actual_unmount() {
    let mut c = controller();
    start(&mut c, "a", "r");
    c.boot(Boot::Panel, None).unwrap();
    approval(&mut c, None);
    c.set_visible(false);
    assert_eq!(c.background()[0].activity, RunActivity::WaitingForApproval);
    assert!(c.owners["a"].busy());
    assert!(
        c.unmount()
            .iter()
            .any(|e| matches!(e,Effect::Cancel{run_id}if run_id=="r"))
    );
}
#[test]
fn tool_only_done_rejected_and_start_timeout_visible() {
    let mut c = controller();
    start(&mut c, "a", "r");
    c.event(Event::Done { run_id: "r".into() });
    assert_eq!(
        last(&c, "a").error.as_deref(),
        Some("Agent finished without a response.")
    );
    let mut c = controller();
    start(&mut c, "a", "r");
    assert!(!c.tick(30_002).is_empty());
    assert!(last(&c, "a").error.as_ref().unwrap().contains("30 seconds"));
}
#[test]
fn canonical_answer_and_stage_order_and_thinking_redaction() {
    let mut c = controller();
    start(&mut c, "a", "r");
    c.event(Event::Message {
        run_id: "r".into(),
        delta: "partial".into(),
    });
    c.event(Event::Stage {
        run_id: "r".into(),
        stage: Stage {
            index: 1,
            label: "Thinking raw".into(),
            status: StageStatus::Running,
            detail: Some("private".into()),
            output: Some("private".into()),
            tool_name: Some("thinking".into()),
            duration_ms: None,
            output_truncated: false,
        },
    });
    c.event(Event::Message {
        run_id: "r".into(),
        delta: " text".into(),
    });
    assert_eq!(c.owners["a"].timeline.len(), 3);
    c.event(Event::Answer {
        run_id: "r".into(),
        text: "Canonical".into(),
    });
    c.event(Event::Done { run_id: "r".into() });
    let t = last(&c, "a");
    assert_eq!(t.text, "Canonical");
    assert_eq!(t.stages[0].label, "Thinking…");
    assert!(t.stages[0].output.is_none());
}
#[test]
fn frame_coalescing_force_flush_and_follow_output() {
    let mut c = FrameCoalescer::default();
    assert_eq!(c.changed(0, 16), Some(16));
    assert_eq!(c.changed(5, 16), Some(16));
    assert!(!c.flush(15, false));
    assert!(c.flush(16, false));
    assert!(!c.flush(17, true));
    c.changed(18, 16);
    assert!(c.flush(18, true));
    assert!(follow_output(40.0, 100.0, 200.0));
    assert!(!follow_output(0.0, 100.0, 201.0));
}
#[test]
fn history_loading_empty_and_title() {
    assert_ne!(history_placeholder(true), history_placeholder(false));
    assert_eq!(title("\n question\nnext"), "question");
    assert_eq!(title(""), "New chat");
    assert_eq!(title(&"x".repeat(70)).chars().count(), 62);
}
#[test]
fn wire_events_roundtrip_preserves_ids() {
    let event = Event::Approval {
        run_id: "run".into(),
        approval: Approval {
            approval_id: "approval".into(),
            title: "Allow?".into(),
            command: "{}".into(),
            can_remember: Some(false),
            remember_kind: Some("tool".into()),
        },
    };
    let v = serde_json::to_value(&event).unwrap();
    assert_eq!(v["runId"], "run");
    assert_eq!(v["approvalId"], "approval");
    let _: Event = serde_json::from_value(v).unwrap();
}
#[test]
fn chat_meta_eight_source_cases() {
    for s in ["💻", "⚽", "⚡", "🤖"] {
        assert!(meta::is_emoji(Some(s)));
    }
    for s in [Some("chat"), Some(""), None] {
        assert!(!meta::is_emoji(s));
    }
    for (p, t, i) in [
        (
            "What is using the most CPU? compiler 2-4gb? wtf it just takes 2-4gb for idle or just once build the app?",
            "CPU Usage & Compiler Memory",
            "cpu",
        ),
        ("who is messi?", "Lionel Messi", "trophy"),
        ("кто такой роналду", "Cristiano Ronaldo", "trophy"),
        (
            "how to build a react hook in typescript?",
            "Build a react hook in typescript",
            "snippets",
        ),
        (
            "Can you please tell me what is the speed of light?",
            "The speed of light",
            "chat",
        ),
        (
            "Подскажи пожалуйста что такое квантовая запутанность?",
            "Квантовая запутанность",
            "chat",
        ),
        (
            "Скажи Порты открыты t3 code",
            "Порты открыты t3 code",
            "cpu",
        ),
        ("Скажи, какие порты открыты?", "Порты открыты", "cpu"),
        (
            "MCP version 2 when it came out?",
            "MCP Version 2",
            "snippets",
        ),
        ("API Key как вообще генерить", "API Key", "key"),
    ] {
        assert_eq!(meta::heuristic(p, None), (t.into(), i.into()));
    }
    for (a, b) in [
        ("⚡", "snippets"),
        ("💻", "cpu"),
        ("🔑", "key"),
        ("⚽", "trophy"),
        ("📝", "edit"),
        ("🤖", "ai"),
        ("💬", "chat"),
        ("code", "snippets"),
        ("key", "key"),
        ("cpu", "cpu"),
    ] {
        assert_eq!(meta::icon_kind(Some(a)), b);
    }
}
#[test]
fn llm_error_source_cases() {
    let raw = "OpenAI error 400: {\"error\":{\"message\":\"The supported API model names are deepseek-v4-pro or deepseek-v4-flash, but you passed gemini-2.0-flash.\"}}";
    assert_eq!(
        format_error(raw, "AI provider"),
        "Model \"gemini-2.0-flash\" is not supported by this provider. Choose deepseek-v4-pro or deepseek-v4-flash and try again."
    );
    assert_eq!(
        format_error(
            "Gemini error 401: {\"error\":{\"message\":\"API key is invalid.\"}}",
            "Gemini"
        ),
        "Gemini request failed (401): API key is invalid."
    );
    assert_eq!(
        format_error(
            "Native model: startup failed:\nError: Model \"tezbar/deepseek-v4-pro\" not found.",
            "AI provider"
        ),
        "The selected model \"tezbar/deepseek-v4-pro\" is unavailable. Choose another model in AI settings and try again."
    );
    for raw in [
        "Failed to write to backend runner process; recovery is in progress",
        "Backend runner is still recovering",
        "Backend runner stopped before replying",
    ] {
        assert!(
            format_error(raw, "AI provider").starts_with("The background service disconnected.")
        );
    }
    assert_eq!(
        format_error("Native model: response incomplete", "AI provider"),
        "Native model: response incomplete"
    );
}
#[test]
fn failed_preparation_has_recoverable_input_error_and_canonical_images() {
    let mut c = controller();
    c.edit_draft("a", "kept".into());
    let Effect::AppendUser { token, .. } =
        c.send("a", "r".into(), "u".into(), "s".into(), 0).unwrap()
    else {
        panic!()
    };
    assert!(c.prepared("a", token, Err("fixture".into()), 1).is_empty());
    assert_eq!(c.owners["a"].draft, "kept");
    assert!(c.owners["a"].input_error.is_some());
}
#[test]
fn each_failed_turn_persistence_is_retained_independently() {
    let mut c = controller();
    start(&mut c, "a", "r1");
    let e = c.event(Event::Error {
        run_id: "r1".into(),
        message: "one".into(),
    });
    let Effect::PersistTurn { token: old, .. } = e[0] else {
        panic!()
    };
    start(&mut c, "a", "r2");
    let e = c.event(Event::Error {
        run_id: "r2".into(),
        message: "two".into(),
    });
    let Effect::PersistTurn { token: new, .. } = e[0] else {
        panic!()
    };
    c.persisted("a", new, true);
    c.persisted("a", old, false);
    assert!(c.owners["a"].unsaved());
    assert_eq!(c.retry_persistence("a").len(), 1);
}
#[test]
fn delete_during_retry_preparation_cannot_start_and_failed_delete_restores_prompt() {
    let mut c = controller();
    start(&mut c, "a", "r");
    c.event(Event::Error {
        run_id: "r".into(),
        message: "failed".into(),
    });
    let Effect::PrepareRetry { token, .. } = c.retry("a", "s-r", "retry".into()).unwrap() else {
        panic!()
    };
    let user = c.owners["a"].turns.front().unwrap().clone();
    let e = c.delete("a");
    let Effect::DeleteConversation { token: del, .. } = e[0] else {
        panic!()
    };
    assert!(c.prepared("a", token, Ok(user), 1).is_empty());
    c.delete_result("a", del, false);
    assert_eq!(c.owners["a"].draft, "Canonical question");
    assert!(!c.owners["a"].busy());
}
#[test]
fn failed_start_id_and_provider_failure_are_visible_and_retryable() {
    for result in [
        Err("Fixture provider offline".into()),
        Ok("wrong-run".into()),
    ] {
        let mut c = controller();
        start(&mut c, "a", "r");
        c.start_result("r", result);
        assert!(last(&c, "a").error.is_some());
        assert!(c.retry("a", "s-r", "retry".into()).is_ok());
    }
}
#[test]
fn prompt_resolution_is_draft_owned_and_turn_deletion_is_confirmed() {
    let mut c = controller();
    let Effect::ResolvePrompt { token, .. } = c
        .resolve_prompt("a", "prompt".into(), Default::default())
        .unwrap()
    else {
        panic!()
    };
    c.edit_draft("a", "typed meanwhile".into());
    c.prompt_resolved(token, Ok("stale prompt".into()));
    assert_eq!(c.owners["a"].draft, "typed meanwhile");
    start(&mut c, "a", "r");
    c.event(Event::Error {
        run_id: "r".into(),
        message: "fixture".into(),
    });
    let Effect::DeleteTurn { token, .. } = c.delete_turn("a", "s-r").unwrap() else {
        panic!()
    };
    assert_eq!(c.owners["a"].turns.len(), 2);
    c.turn_deleted(token, true);
    assert_eq!(c.owners["a"].turns.len(), 1);
}
#[test]
fn history_cache_distinguishes_never_loaded_empty_and_retained_rows() {
    let mut c = controller();
    assert!(c.history.is_none());
    let e = c.boot(Boot::Panel, None).unwrap();
    let Effect::LoadHistory { token } = e[0] else {
        panic!()
    };
    c.history_result(token, vec![]);
    assert_eq!(c.history, Some(vec![]));
    let e = c.boot(Boot::Panel, Some("again".into())).unwrap();
    let Effect::LoadHistory { token } = e[0] else {
        panic!()
    };
    c.history_result(
        token,
        vec![HistoryRow {
            id: "a".into(),
            title: "First".into(),
            preview: "preview".into(),
            updated_at: 2,
        }],
    );
    c.boot(
        Boot::NewChat {
            session_id: "b".into(),
            cwd: None,
        },
        None,
    )
    .unwrap();
    assert_eq!(c.history.as_ref().unwrap()[0].title, "First");
}
#[test]
fn timeline_three_source_cases_keep_stage_updates_in_place() {
    let mut c = controller();
    start(&mut c, "a", "r");
    let stage = |index, status| Event::Stage {
        run_id: "r".into(),
        stage: Stage {
            index,
            label: format!("bash: step {index}"),
            status,
            detail: None,
            output: None,
            tool_name: None,
            duration_ms: None,
            output_truncated: false,
        },
    };
    c.event(stage(0, StageStatus::Running));
    c.event(stage(1, StageStatus::Running));
    c.event(Event::Message {
        run_id: "r".into(),
        delta: "interleaved prose".into(),
    });
    c.event(stage(0, StageStatus::Done));
    assert_eq!(
        c.owners["a"].timeline,
        vec![
            TimelineItem::Stage(0),
            TimelineItem::Stage(1),
            TimelineItem::Text { start: 0, end: 17 }
        ]
    );
    assert_eq!(
        c.owners["a"].response().unwrap().stages[0].status,
        StageStatus::Done
    );
    c.event(stage(2, StageStatus::Running));
    c.event(Event::Message {
        run_id: "r".into(),
        delta: "next".into(),
    });
    assert_eq!(c.owners["a"].timeline.len(), 5);
}
#[test]
fn one_hundred_settled_surface_cycles_release_background_owners() {
    let mut c = ChatController::default();
    for n in 0..100 {
        let id = format!("session-{n}");
        c.boot(
            Boot::NewChat {
                session_id: id.clone(),
                cwd: None,
            },
            None,
        )
        .unwrap();
        start(&mut c, &id, &format!("run-{n}"));
        let e = c.event(Event::Error {
            run_id: format!("run-{n}"),
            message: "fixture failure".into(),
        });
        let Effect::PersistTurn { token, .. } = e[0] else {
            panic!()
        };
        c.persisted(&id, token, true);
        c.boot(Boot::Panel, None).unwrap();
        c.trim_idle();
        assert!(c.owners.is_empty());
    }
}

#[test]
fn large_stage_payload_retains_canonical_context_ids() {
    let mut c = controller();
    for i in 0..4 {
        let run = format!("r{i}");
        start(&mut c, "a", &run);
        for index in 0..70 {
            c.event(Event::Stage {
                run_id: run.clone(),
                stage: Stage {
                    index,
                    label: "tool".into(),
                    status: StageStatus::Done,
                    detail: None,
                    output: Some("x".repeat(32768)),
                    tool_name: Some("fixture".into()),
                    duration_ms: None,
                    output_truncated: false,
                },
            });
        }
        c.event(Event::Answer {
            run_id: run.clone(),
            text: "answer".into(),
        });
        let effects = c.event(Event::Done { run_id: run });
        if let Effect::PersistTurn { token, .. } = effects[0] {
            c.persisted("a", token, true);
        }
    }
    assert_eq!(
        c.conversation("a")
            .unwrap()
            .turns
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>(),
        vec!["u-r3", "s-r3"]
    );
    c.edit_draft("a", "follow up".into());
    let Effect::AppendUser { token, turn, .. } = c
        .send("a", "next".into(), "next-u".into(), "next-a".into(), 10)
        .unwrap()
    else {
        panic!()
    };
    let effects = c.prepared("a", token, Ok(turn), 11);
    let Effect::Start(request) = &effects[0] else {
        panic!()
    };
    assert_eq!(
        request.prior_turn_ids,
        (0..4)
            .flat_map(|i| [format!("u-r{i}"), format!("s-r{i}")])
            .collect::<Vec<_>>()
    );
}

#[test]
fn failed_delete_returns_every_dirty_persistence_intent_and_fences_tokens() {
    let mut c = controller();
    start(&mut c, "a", "one");
    c.event(Event::Error {
        run_id: "one".into(),
        message: "first".into(),
    });
    start(&mut c, "a", "two");
    let effects = c.delete("a");
    let token = effects
        .iter()
        .find_map(|e| {
            if let Effect::DeleteConversation { token, .. } = e {
                Some(*token)
            } else {
                None
            }
        })
        .unwrap();
    c.event(Event::Message {
        run_id: "two".into(),
        delta: "partial private text".into(),
    });
    assert!(
        c.event(Event::Done {
            run_id: "two".into()
        })
        .is_empty()
    );
    assert!(c.delete_result("a", token + 1, false).is_empty());
    let effects = c.delete_result("a", token, false);
    assert_eq!(effects.len(), 2);
    let mut ids = vec![];
    for e in effects {
        let Effect::PersistTurn {
            session_id,
            token,
            turn,
        } = e
        else {
            panic!()
        };
        assert_eq!(session_id, "a");
        ids.push(turn.id.clone());
        if turn.id == "s-two" {
            assert_eq!(turn.text, "partial private text");
        }
        c.persisted("a", token + 100, true);
        assert!(c.conversation("a").unwrap().unsaved());
        c.persisted("a", token, false);
        c.persisted("a", token, true);
    }
    assert_eq!(ids, vec!["s-one", "s-two"]);
    assert!(!c.conversation("a").unwrap().unsaved());
    assert!(c.delete_result("a", token, false).is_empty());
}

#[test]
fn response_meta_is_frozen_and_core_wire_roundtrips_optional_fields() {
    let mut c = controller();
    c.select(
        "a",
        Selection {
            provider: Some("fixture".into()),
            provider_title: Some("Fixture provider".into()),
            model: Some("model-1".into()),
            ..Selection::default()
        },
    );
    start(&mut c, "a", "r");
    c.select(
        "a",
        Selection {
            provider: Some("other".into()),
            model: Some("model-2".into()),
            ..Selection::default()
        },
    );
    c.token_count("wrong", 99);
    c.token_count("r", 123);
    let effects = c.event(Event::Answer {
        run_id: "r".into(),
        text: "answer".into(),
    });
    assert!(effects.is_empty());
    let effects = c.event(Event::Done { run_id: "r".into() });
    let Effect::PersistTurn { turn, .. } = &effects[0] else {
        panic!()
    };
    let wire = turn.to_core_wire();
    assert_eq!(
        wire["responseMeta"],
        serde_json::json!({"provider":"fixture","providerTitle":"Fixture provider","model":"model-1","tokenCount":123})
    );
    assert!(wire.get("references").is_none());
    assert!(wire.get("stages").is_none());
    let restored = Turn::from_core_wire(wire).unwrap();
    assert_eq!(restored.response_meta, turn.response_meta);
    let user = Turn::from_core_wire(serde_json::json!({"id":"image","role":"user","text":"look","createdAt":1,"attachments":[{"kind":"image","name":"shot","mimeType":"image/png","width":640}]})).unwrap();
    assert!(matches!(
        &user.references[0],
        Reference::Image {
            width: Some(640),
            ..
        }
    ));
    assert_eq!(
        user.to_core_wire()["attachments"][0]["mimeType"],
        "image/png"
    );
}

#[test]
fn reopened_large_history_keeps_last16_context_and_image_flags() {
    let mut c = ChatController::default();
    let effects = c
        .boot(
            Boot::Resume {
                session_id: "a".into(),
            },
            None,
        )
        .unwrap();
    let Effect::LoadConversation { token, .. } = effects[0] else {
        panic!()
    };
    let mut turns = vec![];
    for i in 0..300 {
        let mut t = Turn::from_core_wire(serde_json::json!({"id":format!("t{i}"),"role":if i%2==0 {"user"} else {"assistant"},"text":"fixture","createdAt":i})).unwrap();
        if i == 290 {
            t.references.push(Reference::Image {
                blob_id: "opaque".into(),
                name: "image".into(),
                mime_type: "image/png".into(),
                width: None,
                height: None,
            });
        }
        if i == 299 {
            t.stages = (0..70)
                .map(|index| Stage {
                    index,
                    label: "tool".into(),
                    status: StageStatus::Done,
                    detail: None,
                    output: Some("x".repeat(32768)),
                    tool_name: None,
                    duration_ms: None,
                    output_truncated: false,
                })
                .collect();
        }
        turns.push(Arc::new(t));
    }
    c.history_result_conversation(token, turns, Selection::default());
    assert_eq!(c.conversation("a").unwrap().context.len(), 256);
    c.edit_draft("a", "follow up".into());
    let Effect::AppendUser { token, turn, .. } = c
        .send("a", "r".into(), "u".into(), "s".into(), 300)
        .unwrap()
    else {
        panic!()
    };
    let effects = c.prepared("a", token, Ok(turn), 301);
    let Effect::Start(r) = &effects[0] else {
        panic!()
    };
    assert_eq!(
        r.prior_turn_ids,
        (284..300).map(|i| format!("t{i}")).collect::<Vec<_>>()
    );
    assert!(r.history_has_images);
    assert!(r.prior_turns.is_empty());
}

#[test]
fn unsaved_fallback_is_typed_and_older_retry_rejects_later_context() {
    let mut c = controller();
    start(&mut c, "a", "one");
    c.event(Event::Error {
        run_id: "one".into(),
        message: "error".into(),
    });
    assert!(
        c.conversation("a")
            .unwrap()
            .context
            .iter()
            .find(|t| t.id == "s-one")
            .unwrap()
            .has_error
    );
    c.edit_draft("a", "later".into());
    let Effect::AppendUser { token, turn, .. } = c
        .send("a", "two".into(), "u2".into(), "a2".into(), 2)
        .unwrap()
    else {
        panic!()
    };
    let effects = c.prepared("a", token, Ok(turn), 3);
    let Effect::Start(r) = &effects[0] else {
        panic!()
    };
    assert_eq!(r.prior_turn_ids, vec!["u-one", "s-one"]);
    assert_eq!(r.core_prior_turns()[0]["id"], "s-one");
    assert_eq!(r.core_prior_turns()[0]["error"], "error");
    assert!(r.core_prior_turns()[0].get("stages").is_none());
    c.event(Event::Error {
        run_id: "two".into(),
        message: "later error".into(),
    });
    assert!(c.retry("a", "s-one", "retry".into()).is_err());
    assert!(c.retry("a", "a2", "retry".into()).is_ok());
}

#[test]
fn background_approval_never_focuses_and_resume_requires_real_ack() {
    let mut c = controller();
    start(&mut c, "a", "r");
    c.boot(
        Boot::NewChat {
            session_id: "b".into(),
            cwd: None,
        },
        None,
    )
    .unwrap();
    let effects = c.event(Event::Approval {
        run_id: "r".into(),
        approval: Approval {
            approval_id: "p".into(),
            title: "Allow".into(),
            command: "{}".into(),
            can_remember: Some(true),
            remember_kind: None,
        },
    });
    assert!(effects.is_empty());
    assert_eq!(c.owners["a"].focus, ApprovalFocus::Composer);
    c.focus("a", ApprovalFocus::AllowOnce);
    assert!(c.enter("a").is_none());
    let effects = c
        .boot(
            Boot::Resume {
                session_id: "a".into(),
            },
            None,
        )
        .unwrap();
    assert!(matches!(&effects[0],Effect::FocusApproval {session_id} if session_id=="a"));
    assert!(c.enter("a").is_none());
    c.focus("a", ApprovalFocus::AllowOnce);
    assert!(matches!(
        c.enter("a"),
        Some(Effect::Approve {
            decision: Decision::Once,
            ..
        })
    ));
}

#[test]
fn late_history_list_does_not_change_owner_or_followup_context() {
    let mut c = ChatController::default();
    let effects = c.boot(Boot::Panel, None).unwrap();
    let Effect::LoadHistory { token: history } = effects[0] else {
        panic!()
    };
    c.boot(
        Boot::NewChat {
            session_id: "a".into(),
            cwd: None,
        },
        None,
    )
    .unwrap();
    start(&mut c, "a", "r");
    // Initial list request completes after composer submission, while the run owns A.
    c.history_result(history, vec![]);
    assert_eq!(c.active.as_deref(), Some("a"));
    assert!(c.conversation("a").unwrap().busy());
    assert_eq!(
        c.conversation("a").unwrap().turns.front().unwrap().text,
        "Canonical question"
    );
    c.event(Event::Answer {
        run_id: "r".into(),
        text: "Still here".into(),
    });
    c.event(Event::Done { run_id: "r".into() });
    c.edit_draft("a", "Continue it".into());
    let Effect::AppendUser { token, turn, .. } = c
        .send("a", "next".into(), "u2".into(), "a2".into(), 3)
        .unwrap()
    else {
        panic!()
    };
    let effects = c.prepared("a", token, Ok(turn), 4);
    let Effect::Start(r) = &effects[0] else {
        panic!()
    };
    assert_eq!(r.prior_turn_ids, vec!["u-r", "s-r"]);
    assert_eq!(r.core_prior_turns()[0]["text"], "Still here");
}

#[test]
fn core_normalized_floating_usage_is_accepted() {
    let t=Turn::from_core_wire(serde_json::json!({"id":"a","role":"assistant","text":"answer","createdAt":1,"responseMeta":{"provider":"fixture","providerTitle":"Fixture","model":"model","tokenCount":42.0}})).unwrap();
    assert_eq!(t.response_meta.unwrap().token_count, Some(42));
}

#[test]
fn resolved_default_selection_belongs_to_preparation_not_later_picker() {
    let mut c = controller();
    c.edit_draft("a", "question".into());
    let Effect::AppendUser { token, turn, .. } =
        c.send("a", "r".into(), "u".into(), "s".into(), 1).unwrap()
    else {
        panic!()
    };
    c.select(
        "a",
        Selection {
            provider: Some("later".into()),
            model: Some("later-model".into()),
            ..Selection::default()
        },
    );
    let resolved = Selection {
        provider: Some("resolved".into()),
        provider_title: Some("Resolved title".into()),
        model: Some("resolved-model".into()),
        ..Selection::default()
    };
    let effects = c.prepared_with_selection("a", token, Ok(turn), resolved.clone(), 2);
    let Effect::Start(r) = &effects[0] else {
        panic!()
    };
    assert_eq!(r.selection, resolved);
    let meta = c
        .conversation("a")
        .unwrap()
        .response()
        .unwrap()
        .response_meta
        .as_ref()
        .unwrap();
    assert_eq!(meta.provider, "resolved");
    assert_eq!(meta.provider_title, "Resolved title");
    assert_eq!(
        c.conversation("a").unwrap().selection.provider.as_deref(),
        Some("later")
    );
}

#[test]
fn failed_capture_unlocks_only_its_request_and_can_be_retried() {
    let mut c = controller();
    let Effect::Capture { token: old, .. } = c.capture().unwrap() else {
        panic!()
    };
    c.capture_failed(old);
    let Effect::Capture { token: new, .. } = c.capture().unwrap() else {
        panic!()
    };
    assert_ne!(old, new);
    c.capture_failed(old);
    assert!(c.capture().is_none());
    c.capture_failed(new);
    assert!(c.capture().is_some());
}
#[test]
fn manual_attachment_admission_is_owned_deduplicated_and_atomic() {
    let mut c = controller();
    let image = |i| Reference::Image {
        blob_id: format!("blob-{i}"),
        mime_type: "image/png".into(),
        name: format!("image{i}"),
        width: None,
        height: None,
    };
    assert_eq!(c.attach_references("a", vec![image(0), image(0)]), Ok(1));
    assert_eq!(c.attach_references("a", (1..8).map(image).collect()), Ok(7));
    assert!(c.attach_references("a", vec![image(8)]).is_err());
    assert_eq!(c.conversation("a").unwrap().attachments.len(), 8);
    assert!(
        c.attach_references(
            "a",
            vec![Reference::SignedHistory {
                turn_id: "secret".into()
            }]
        )
        .is_err()
    );
    c.boot(
        Boot::NewChat {
            session_id: "b".into(),
            cwd: None,
        },
        None,
    )
    .unwrap();
    assert!(c.attach_references("a", vec![image(8)]).is_err());
    assert!(c.conversation("b").unwrap().attachments.is_empty());
    c.set_visible(false);
    assert!(c.attach_references("b", vec![image(0)]).is_err());
}

#[test]
fn clear_admits_hidden_dirty_and_running_owners_and_recovers_failed_write() {
    let mut c = controller();
    start(&mut c, "a", "running-a");
    c.boot(
        Boot::NewChat {
            session_id: "b".into(),
            cwd: None,
        },
        None,
    )
    .unwrap();
    start(&mut c, "b", "settled-b");
    let effects = c.event(Event::Answer {
        run_id: "settled-b".into(),
        text: "answer".into(),
    });
    let _ = effects;
    c.event(Event::Done {
        run_id: "settled-b".into(),
    });
    c.boot(
        Boot::NewChat {
            session_id: "c".into(),
            cwd: None,
        },
        None,
    )
    .unwrap();
    c.edit_draft("c", "preparing".into());
    let Effect::AppendUser {
        token: prep, turn, ..
    } = c
        .send("c", "r-c".into(), "u-c".into(), "a-c".into(), 1)
        .unwrap()
    else {
        panic!()
    };
    let clear = c.begin_clear();
    assert!(
        clear
            .iter()
            .any(|e| matches!(e,Effect::Cancel{run_id} if run_id=="running-a"))
    );
    let tickets: Vec<_> = clear
        .into_iter()
        .filter_map(|e| match e {
            Effect::DeleteConversation { token, session_id } => Some((session_id, token)),
            _ => None,
        })
        .collect();
    assert_eq!(tickets.len(), 3);
    assert!(c.prepared("c", prep, Ok(turn), 2).is_empty());
    for (id, token) in tickets {
        let deferred = c.delete_result(&id, token, false);
        if id == "b" {
            assert!(
                deferred
                    .iter()
                    .any(|e| matches!(e,Effect::PersistTurn{session_id,..} if session_id=="b"))
            );
        }
    }
    assert_eq!(c.conversation("c").unwrap().draft, "preparing");
    let tickets = c.begin_clear();
    for e in tickets {
        if let Effect::DeleteConversation { token, session_id } = e {
            c.delete_result(&session_id, token, true);
        }
    }
    assert!(c.owners.is_empty());
    assert!(c.active.is_none());
}
fn tool(index: u64) -> Event {
    Event::Stage {
        run_id: "r".into(),
        stage: Stage {
            index,
            label: format!("bash: step {index}"),
            status: StageStatus::Done,
            detail: None,
            output: None,
            tool_name: Some("bash".into()),
            duration_ms: None,
            output_truncated: false,
        },
    }
}
fn say(delta: &str) -> Event {
    Event::Message {
        run_id: "r".into(),
        delta: delta.into(),
    }
}
#[test]
fn settled_turn_keeps_interleaved_order_and_canonical_answer_keeps_narration() {
    let mut c = controller();
    start(&mut c, "a", "r");
    c.event(tool(0));
    c.event(tool(1));
    c.event(say("Checking processes."));
    c.event(tool(2));
    c.event(say("Final draft"));
    c.event(Event::Answer {
        run_id: "r".into(),
        text: "Final answer".into(),
    });
    c.event(Event::Done { run_id: "r".into() });
    let turn = last(&c, "a");
    assert_eq!(turn.text, "Checking processes.\n\nFinal answer");
    let order: Vec<String> = turn
        .timeline
        .iter()
        .map(|i| match i {
            TimelineItem::Stage(n) => format!("stage {n}"),
            TimelineItem::Text { start, end } => turn.text[*start..*end].to_string(),
        })
        .collect();
    assert_eq!(
        order,
        [
            "stage 0",
            "stage 1",
            "Checking processes.",
            "stage 2",
            "Final answer"
        ]
    );
    let restored = Turn::from_core_wire(turn.to_core_wire()).unwrap();
    assert_eq!(restored.timeline, turn.timeline);
}
#[test]
fn stored_timeline_that_does_not_match_text_falls_back_to_legacy_layout() {
    let wire = serde_json::json!({
        "id": "t", "role": "assistant", "text": "answer", "createdAt": 1,
        "stages": [{"index": 0, "label": "bash", "status": "done"}],
        "timeline": [{"type": "text", "text": "missing"}, {"type": "stage", "stage": {"index": 0, "label": "bash", "status": "done"}}]
    });
    assert!(Turn::from_core_wire(wire).unwrap().timeline.is_empty());
    let legacy =
        serde_json::json!({"id": "t", "role": "assistant", "text": "answer", "createdAt": 1});
    assert!(Turn::from_core_wire(legacy).unwrap().timeline.is_empty());
}

#[test]
fn start_deadlines_follow_earliest_owner_and_survive_hidden_navigation() {
    let mut c = controller();
    assert_eq!(c.next_start_deadline(), None);
    start(&mut c, "a", "a-run");
    assert_eq!(c.next_start_deadline(), Some(30_002));
    c.boot(
        Boot::NewChat {
            session_id: "b".into(),
            cwd: None,
        },
        None,
    )
    .unwrap();
    start(&mut c, "b", "b-run");
    if let RunPhase::Running { deadline, .. } = &mut c.owners.get_mut("b").unwrap().phase {
        *deadline = 40_000;
    }
    c.set_visible(false);
    assert_eq!(c.next_start_deadline(), Some(30_002));
    assert!(c.tick(30_001).is_empty());
    assert!(c
        .tick(30_002)
        .iter()
        .any(|e| matches!(e, Effect::Cancel { run_id } if run_id == "a-run")));
    assert_eq!(c.next_start_deadline(), Some(40_000));
    c.event(Event::Start {
        run_id: "b-run".into(),
        task: "Question".into(),
    });
    assert_eq!(c.next_start_deadline(), None);
    assert!(c.tick(40_000).is_empty());
    c.stop("b");
    start(&mut c, "b", "retry");
    assert_eq!(c.next_start_deadline(), Some(30_002));
    let effects = c.delete("b");
    let token = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::DeleteConversation { token, .. } => Some(*token),
            _ => None,
        })
        .unwrap();
    c.delete_result("b", token, true);
    assert_eq!(c.next_start_deadline(), None);
}
