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
    assert!(c
        .conversation("b")
        .unwrap()
        .response()
        .unwrap()
        .text
        .is_empty());
    assert!(c
        .boot(
            Boot::Resume {
                session_id: "a".into()
            },
            Some("2".into())
        )
        .unwrap()
        .is_empty());
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
    assert!(c
        .event(Event::Answer {
            run_id: "r".into(),
            text: "late".into()
        })
        .is_empty());
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
    assert!(c
        .send("a", "new".into(), "u2".into(), "s2".into(), 2)
        .is_ok());
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
            assert!(c
                .boot(
                    Boot::Resume {
                        session_id: "a".into()
                    },
                    None
                )
                .is_err());
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
    assert_eq!(c.background()[0].waiting_approval, true);
    assert!(c.owners["a"].busy());
    assert!(c
        .unmount()
        .iter()
        .any(|e| matches!(e,Effect::Cancel{run_id}if run_id=="r")));
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
    for(p,t,i)in [("What is using the most CPU? compiler 2-4gb? wtf it just takes 2-4gb for idle or just once build the app?","CPU Usage & Compiler Memory","cpu"),("who is messi?","Lionel Messi","trophy"),("кто такой роналду","Cristiano Ronaldo","trophy"),("how to build a react hook in typescript?","Build a react hook in typescript","snippets"),("Can you please tell me what is the speed of light?","The speed of light","chat"),("Подскажи пожалуйста что такое квантовая запутанность?","Квантовая запутанность","chat"),("Скажи Порты открыты t3 code","Порты открыты t3 code","cpu"),("Скажи, какие порты открыты?","Порты открыты","cpu"),("MCP version 2 when it came out?","MCP Version 2","snippets"),("API Key как вообще генерить","API Key","key")]{assert_eq!(meta::heuristic(p,None),(t.into(),i.into()));}
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
    let raw="OpenAI error 400: {\"error\":{\"message\":\"The supported API model names are deepseek-v4-pro or deepseek-v4-flash, but you passed gemini-2.0-flash.\"}}";
    assert_eq!(format_error(raw,"AI provider"),"Model \"gemini-2.0-flash\" is not supported by this provider. Choose deepseek-v4-pro or deepseek-v4-flash and try again.");
    assert_eq!(
        format_error(
            "Gemini error 401: {\"error\":{\"message\":\"API key is invalid.\"}}",
            "Gemini"
        ),
        "Gemini request failed (401): API key is invalid."
    );
    assert_eq!(format_error("Native model: startup failed:\nError: Model \"tezbar/deepseek-v4-pro\" not found.","AI provider"),"The selected model \"tezbar/deepseek-v4-pro\" is unavailable. Choose another model in AI settings and try again.");
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
