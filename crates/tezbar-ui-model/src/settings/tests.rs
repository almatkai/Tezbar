use super::*;
#[test]
fn task_overrides_merge_fresh_peers_and_retry_never_reverts_newer_task_selection() {
    let mut q = WriteQueue::default();
    let mut c = config();
    let edit = |task: &str, provider: &str, model: &str| Edit::TaskSelection {
        task: task.into(),
        provider: provider.into(),
        model: model.into(),
    };
    q.enqueue(1, vec![edit("answer", "openai", "old")]);
    flush(&mut q, &mut c, false);
    c["taskProviderOverrides"]["translate"] = json!("anthropic");
    c["taskModelOverrides"] = json!({"translate":"peer"});
    q.enqueue(2, vec![edit("answer", "ollama", "new")]);
    flush(&mut q, &mut c, true);
    q.retry(false);
    while q.next_effect().is_some() {
        panic!("A superseded failure must have been retired");
    }
    assert_eq!(c["taskProviderOverrides"]["translate"], "anthropic");
    assert_eq!(c["taskModelOverrides"]["translate"], "peer");
    assert_eq!(c["taskModelOverrides"]["answer"], "new");
    q.enqueue(2, vec![edit("answer", "", "")]);
    flush(&mut q, &mut c, true);
    assert!(c["taskProviderOverrides"].get("answer").is_none());
    assert_eq!(c["taskModelOverrides"]["translate"], "peer");
}
fn field(p: &str, k: &str, v: &str) -> Edit {
    Edit::Field {
        provider: p.into(),
        field: k.into(),
        value: json!(v),
    }
}
fn flush(q: &mut WriteQueue, c: &mut Value, ok: bool) -> Option<Effect> {
    let Effect::Read { token } = q.next_effect().expect("queued read") else {
        panic!()
    };
    let Effect::Save { patch, .. } = q.read_result(token, Ok(c.clone())).expect("merge") else {
        panic!()
    };
    if ok {
        for (k, v) in patch.as_object().unwrap() {
            c[k] = v.clone();
        }
    }
    q.finish(token, ok)
}
fn config() -> Value {
    json!({"provider":"custom:a","model":"A","providerConfigs":{"custom:a":{"apiKey":"fixture-a","baseURL":"a"},"custom:b":{"apiKey":"fixture-b"}},"providerModels":{"custom:a":[{"id":"A"},{"id":"B"},{"id":"C"},{"id":"D"}]},"providerSelectedModels":{"custom:a":"A"},"customProviders":[{"id":"custom:a"},{"id":"custom:b"}],"taskProviderOverrides":{"chat":"custom:a"}})
}
#[test]
fn t_r1_rapid_removal_and_explicit_choice() {
    let mut q = WriteQueue::default();
    let mut c = config();
    q.enqueue(
        1,
        vec![
            Edit::RemoveModels {
                provider: "custom:a".into(),
                ids: ["A".into(), "C".into()].into(),
            },
            Edit::Select {
                provider: "custom:a".into(),
                model: "D".into(),
            },
        ],
    );
    flush(&mut q, &mut c, true);
    assert_eq!(c["model"], "D");
    assert_eq!(c["taskModelOverrides"]["chat"], "D");
    assert_eq!(
        c["providerModels"]["custom:a"],
        json!([{"id":"B"},{"id":"D"}])
    );
}
#[test]
fn t_r1_removing_interim_fallback_uses_final_list() {
    let mut q = WriteQueue::default();
    let mut c = config();
    q.enqueue(
        1,
        vec![Edit::RemoveModels {
            provider: "custom:a".into(),
            ids: ["A".into(), "B".into()].into(),
        }],
    );
    flush(&mut q, &mut c, true);
    assert_eq!(c["model"], "C");
}
#[test]
fn t_r2_removal_supersedes_retired_failure() {
    let mut q = WriteQueue::default();
    let mut c = config();
    q.enqueue(1, vec![field("custom:a", "apiKey", "old")]);
    flush(&mut q, &mut c, false);
    q.retire(1);
    q.enqueue(2, vec![Edit::RemoveProvider("custom:a".into())]);
    flush(&mut q, &mut c, true);
    q.retry(true);
    assert!(q.next_effect().is_none());
    assert_eq!(q.failure_count(), 0);
    assert!(c["providerConfigs"]["custom:a"].is_null());
}
#[test]
fn t_r2_failed_removal_preserves_peer_credentials_on_retry() {
    let mut q = WriteQueue::default();
    let mut c = config();
    q.enqueue(1, vec![Edit::RemoveProvider("custom:a".into())]);
    flush(&mut q, &mut c, false);
    c["providerConfigs"]["custom:b"]["apiKey"] = json!("new");
    q.retry(false);
    flush(&mut q, &mut c, true);
    assert_eq!(c["providerConfigs"]["custom:b"]["apiKey"], "new");
}
#[test]
fn t_r3_repeated_retry_cannot_resurrect_deleted_provider() {
    let mut q = WriteQueue::default();
    let mut c = config();
    q.enqueue(
        1,
        vec![
            Edit::CreateProvider {
                provider: "custom:a".into(),
                value: json!({"id":"custom:a"}),
            },
            field("custom:a", "apiKey", "old"),
        ],
    );
    flush(&mut q, &mut c, false);
    q.retry(false);
    q.enqueue(2, vec![Edit::RemoveProvider("custom:a".into())]);
    flush(&mut q, &mut c, false);
    flush(&mut q, &mut c, true);
    q.retry(false);
    assert!(q.next_effect().is_none());
}
#[test]
fn t_r3_explicit_recreation_has_new_revision() {
    let mut q = WriteQueue::default();
    let mut c = config();
    q.enqueue(1, vec![Edit::RemoveProvider("custom:a".into())]);
    flush(&mut q, &mut c, true);
    q.enqueue(
        2,
        vec![
            Edit::CreateProvider {
                provider: "custom:a".into(),
                value: json!({"id":"custom:a","title":"Recreated"}),
            },
            field("custom:a", "apiKey", "new"),
        ],
    );
    flush(&mut q, &mut c, true);
    assert_eq!(c["providerConfigs"]["custom:a"]["apiKey"], "new");
}
#[test]
fn retired_activation_cannot_target_deleted_provider_on_retry() {
    for active in ["custom:a", "custom:b"] {
        let mut q = WriteQueue::default();
        let mut c = config();
        c["provider"] = json!(active);
        c["taskProviderOverrides"]["chat"] = json!(active);
        q.enqueue(
            1,
            vec![Edit::Activate {
                provider: "custom:a".into(),
                model: "A".into(),
            }],
        );
        flush(&mut q, &mut c, false);
        q.retire(1);
        q.enqueue(2, vec![Edit::RemoveProvider("custom:a".into())]);
        flush(&mut q, &mut c, true);
        q.retry(true);
        while !q.pending.is_empty() {
            flush(&mut q, &mut c, true);
        }
        let expected = if active == "custom:a" {
            "ollama"
        } else {
            active
        };
        assert_eq!(c["provider"], expected);
        assert_eq!(c["taskProviderOverrides"]["chat"], expected);
        assert_eq!(q.failure_count(), 0);
    }
}
#[test]
fn stale_owner_cannot_activate_deleted_provider() {
    let mut q = WriteQueue::default();
    let mut c = config();
    q.register(1);
    q.enqueue(2, vec![Edit::RemoveProvider("custom:a".into())]);
    flush(&mut q, &mut c, true);
    q.enqueue(
        1,
        vec![Edit::Activate {
            provider: "custom:a".into(),
            model: "A".into(),
        }],
    );
    flush(&mut q, &mut c, true);
    assert_eq!(c["provider"], "ollama");
    assert_eq!(c["taskProviderOverrides"]["chat"], "ollama");
}
#[test]
fn explicit_recreation_can_activate_provider_again() {
    let mut q = WriteQueue::default();
    let mut c = config();
    q.enqueue(1, vec![Edit::RemoveProvider("custom:a".into())]);
    flush(&mut q, &mut c, true);
    q.enqueue(
        2,
        vec![
            Edit::CreateProvider {
                provider: "custom:a".into(),
                value: json!({"id":"custom:a","title":"Recreated"}),
            },
            Edit::Activate {
                provider: "custom:a".into(),
                model: "New".into(),
            },
        ],
    );
    flush(&mut q, &mut c, true);
    assert_eq!(c["provider"], "custom:a");
    assert_eq!(c["model"], "New");
    assert_eq!(
        c["customProviders"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["id"] == "custom:a")
            .count(),
        1
    );
}
#[test]
fn failed_creation_preserves_a_later_api_key_edit() {
    let mut q = WriteQueue::default();
    let mut c = config();
    c["customProviders"] = json!([]);
    c["providerConfigs"] = json!({});
    q.enqueue(
        1,
        vec![Edit::CreateProvider {
            provider: "custom:new".into(),
            value: json!({"id":"custom:new"}),
        }],
    );
    flush(&mut q, &mut c, false);
    q.enqueue(1, vec![field("custom:new", "apiKey", "fixture-new-key")]);
    flush(&mut q, &mut c, true);
    q.retry(false);
    while !q.pending.is_empty() {
        flush(&mut q, &mut c, true);
    }
    assert_eq!(
        c["providerConfigs"]["custom:new"]["apiKey"],
        "fixture-new-key"
    );
    assert!(c["customProviders"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["id"] == "custom:new"));
    assert_eq!(q.failure_count(), 0);
}
#[test]
fn failed_definition_retries_before_its_dependent_field_write() {
    let mut q = WriteQueue::default();
    let mut c = json!({"customProviders":[]});
    q.enqueue(
        1,
        vec![Edit::CreateProvider {
            provider: "custom:new".into(),
            value: json!({"id":"custom:new"}),
        }],
    );
    flush(&mut q, &mut c, false);
    q.enqueue(1, vec![field("custom:new", "apiKey", "fixture-new-key")]);
    flush(&mut q, &mut c, false);
    q.retry(false);
    while let Some(Effect::Read { token }) = q.next_effect() {
        let Some(Effect::Save { patch, .. }) = q.read_result(token, Ok(c.clone())) else {
            panic!()
        };
        // A real provider-dependent writer may reject a key until its definition
        // has been durably created. The retry batch must respect intent order.
        let has_definition = c["customProviders"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == "custom:new");
        let ok = patch.get("providerConfigs").is_none() || has_definition;
        if ok {
            for (key, value) in patch.as_object().unwrap() {
                c[key] = value.clone();
            }
        }
        q.finish(token, ok);
    }
    assert_eq!(q.failure_count(), 0);
    assert_eq!(
        c["providerConfigs"]["custom:new"]["apiKey"],
        "fixture-new-key"
    );
}
#[test]
fn t_r4_retries_are_deduplicated_and_keep_newer_failed_removal() {
    let mut q = WriteQueue::default();
    let mut c = config();
    q.enqueue(1, vec![field("custom:a", "baseURL", "old")]);
    flush(&mut q, &mut c, false);
    q.retry(false);
    q.retry(false);
    assert_eq!(q.pending.len(), 1);
    q.enqueue(2, vec![Edit::RemoveProvider("custom:a".into())]);
    flush(&mut q, &mut c, true);
    flush(&mut q, &mut c, false);
    assert_eq!(q.failure_count(), 1);
}
#[test]
fn t_r5_newer_field_and_activation_success_win() {
    let mut q = WriteQueue::default();
    let mut c = config();
    q.enqueue(
        1,
        vec![
            field("custom:b", "baseURL", "old"),
            Edit::Activate {
                provider: "custom:a".into(),
                model: "A".into(),
            },
        ],
    );
    flush(&mut q, &mut c, false);
    q.enqueue(
        2,
        vec![
            field("custom:b", "baseURL", "new"),
            Edit::Activate {
                provider: "custom:b".into(),
                model: "Z".into(),
            },
        ],
    );
    flush(&mut q, &mut c, true);
    q.retry(false);
    assert!(q.next_effect().is_none());
    assert_eq!(c["providerConfigs"]["custom:b"]["baseURL"], "new");
    assert_eq!(c["provider"], "custom:b");
}
#[test]
fn t_r6_all_writes_merge_only_owned_ids() {
    let mut q = WriteQueue::default();
    let mut c = config();
    q.enqueue(
        1,
        vec![
            Edit::CreateProvider {
                provider: "custom:new".into(),
                value: json!({"id":"custom:new","title":"N"}),
            },
            field("custom:new", "baseURL", "new"),
        ],
    );
    flush(&mut q, &mut c, false);
    q.enqueue(2, vec![field("custom:b", "baseURL", "peer")]);
    q.retry(false);
    flush(&mut q, &mut c, true);
    flush(&mut q, &mut c, true);
    assert!(c["customProviders"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["id"] == "custom:new"));
    assert_eq!(c["providerConfigs"]["custom:b"]["baseURL"], "peer");
}
#[test]
fn t_r7_signin_waits_for_success_and_preserves_peer_records() {
    for ok in [false, true] {
        let mut q = WriteQueue::default();
        let mut c = config();
        q.sign_in(
            1,
            vec![field("copilot", "githubOAuthClientId", "custom-client")],
        )
        .unwrap();
        let effect = flush(&mut q, &mut c, ok);
        assert_eq!(matches!(effect, Some(Effect::StartDeviceSignIn { .. })), ok);
        assert_eq!(c["providerConfigs"]["custom:b"]["apiKey"], "fixture-b");
    }
}
#[test]
fn close_failure_blocks_and_acknowledged_has_no_deadline() {
    let mut q = WriteQueue::default();
    let mut c = config();
    q.enqueue(1, vec![field("custom:a", "protocol", "anthropic")]);
    assert!(matches!(
        q.request_close(1, 0, true),
        Some(Effect::AcknowledgeClose { .. })
    ));
    assert!(q.close_status(10_000).is_none());
    flush(&mut q, &mut c, false);
    assert!(matches!(
        q.close_status(20_000),
        Some(Effect::KeepOpen { .. })
    ));
    q.retry(false);
    flush(&mut q, &mut c, true);
    assert!(matches!(q.close_status(30_000), Some(Effect::Close { .. })));
}
#[test]
fn only_unacknowledged_close_has_three_second_fallback() {
    let mut q = WriteQueue::default();
    q.request_close(1, 0, false);
    assert!(q.close_status(2999).is_none());
    assert!(matches!(q.close_status(3000), Some(Effect::Close { .. })));
}
#[test]
fn optional_discovery_failure_does_not_block_close() {
    let mut q = WriteQueue::default();
    let mut c = config();
    q.discover(
        1,
        vec![Edit::Models {
            provider: "custom:a".into(),
            models: vec![],
        }],
    );
    flush(&mut q, &mut c, false);
    assert_eq!(q.failure_count(), 0);
}
#[test]
fn dirty_hydration_and_stale_tokens_do_not_write() {
    let mut d = Draft::default();
    let t = d.read();
    let k = Key::Field("a".into(), "apiKey".into());
    d.edit(k.clone(), json!("new"));
    d.hydrate(t, [(k.clone(), json!("old"))].into());
    assert_eq!(d.value(&k), Some(&json!("new")));
}
#[test]
fn external_active_and_per_field_changes_survive() {
    let mut q = WriteQueue::default();
    let mut c = config();
    q.enqueue(1, vec![field("custom:a", "protocol", "anthropic")]);
    flush(&mut q, &mut c, false);
    c["provider"] = json!("custom:b");
    c["model"] = json!("external");
    q.enqueue(2, vec![field("custom:a", "maxTokens", "4096")]);
    flush(&mut q, &mut c, true);
    q.retry(false);
    flush(&mut q, &mut c, true);
    assert_eq!(c["provider"], "custom:b");
    assert_eq!(c["model"], "external");
    assert_eq!(c["providerConfigs"]["custom:a"]["protocol"], "anthropic");
    assert_eq!(c["providerConfigs"]["custom:a"]["maxTokens"], "4096");
}
#[test]
fn contributed_provider_records_are_not_persisted() {
    let mut q = WriteQueue::default();
    let mut c = config();
    c["customProviders"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"custom:extension","_extensionId":"fixture"}));
    q.enqueue(
        1,
        vec![field("custom:extension", "apiKey", "should-not-write")],
    );
    flush(&mut q, &mut c, true);
    assert!(c["providerConfigs"]["custom:extension"].is_null());
}
#[test]
fn core_catalog_single_source() {
    assert_eq!(recommended("ollama"), "llama3.2");
    assert!(provider_catalog()["DEFAULT_PROVIDER_MODELS"]["openai"].is_array());
}
#[test]
fn provider_reload_isolation_and_discovery_preserves_metadata() {
    let c = config();
    let mut d = ProviderDraft::default();
    d.load("custom:a".into(), &c);
    let token = d.discovery_token();
    let _ = d.edit("protocol".into(), json!("anthropic")).unwrap();
    assert!(d.discovered(token, vec!["new".into()]).is_none());
    let token = d.discovery_token();
    d.models[0]["maxTokens"] = json!(8192);
    d.discovered(token, vec!["A".into(), "new".into()]);
    assert_eq!(d.models[0]["maxTokens"], 8192);
    assert_eq!(d.field("protocol"), Some(&json!("anthropic")));
    d.load("copilot".into(), &c);
    assert!(d.field("apiKey").is_none());
    assert!(d.field("baseURL").is_none());
}
#[test]
fn device_save_failure_and_cancellation_fence_polling() {
    let mut d = DeviceSignIn::default();
    let t = d.saving();
    assert!(d.saved(t, false, None).is_none());
    assert_eq!(d.phase, DevicePhase::Failed);
    let t = d.saving();
    assert!(d.saved(t, true, Some("client".into())).is_some());
    d.started(
        t,
        "CODE".into(),
        "https://github.com/login/device".into(),
        1,
        0,
    )
    .unwrap();
    assert!(d.tick(999).is_none());
    assert!(d.tick(1000).is_some());
    d.cancel();
    d.polled(t, "success", String::new(), String::new(), 1001);
    assert_eq!(d.phase, DevicePhase::Cancelled);
}

#[test]
fn stale_definition_update_cannot_recreate_but_explicit_create_can() {
    let mut q = WriteQueue::default();
    let mut c = config();
    q.enqueue(1, vec![Edit::RemoveProvider("custom:a".into())]);
    flush(&mut q, &mut c, true);
    q.enqueue(
        2,
        vec![Edit::Definition {
            provider: "custom:a".into(),
            value: json!({"id":"custom:a","name":"stale"}),
        }],
    );
    flush(&mut q, &mut c, true);
    assert!(!c["customProviders"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["id"] == "custom:a"));
    q.enqueue(
        2,
        vec![Edit::CreateProvider {
            provider: "custom:a".into(),
            value: json!({"id":"custom:a","name":"new"}),
        }],
    );
    flush(&mut q, &mut c, true);
    assert!(c["customProviders"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["id"] == "custom:a" && p["name"] == "new"));
    // Also fence deletion by an external writer, not recorded in this queue.
    c["customProviders"] = json!([]);
    q.enqueue(
        3,
        vec![Edit::Definition {
            provider: "custom:a".into(),
            value: json!({"id":"custom:a"}),
        }],
    );
    flush(&mut q, &mut c, true);
    assert_eq!(c["customProviders"], json!([]));
}

#[test]
fn discovery_merges_fresh_models_peer_removals_metadata_and_selection() {
    let mut q = WriteQueue::default();
    let mut c = config();
    let mut d = ProviderDraft::default();
    d.load("custom:a".into(), &c);
    let edit = d
        .discovered_models(
            d.discovery_token(),
            vec![
                json!({"id":"A"}),
                json!({"id":"B"}),
                json!({"id":" gpt-6-sol ","maxTokens":4096}),
                json!({"id":"gpt-6-sol"}),
            ],
        )
        .unwrap();
    c["providerModels"]["custom:a"] = json!([{"id":"A","maxTokens":9999,"protocol":"anthropic"},{"id":"peer","capabilities":["tools"]}]);
    c["providerSelectedModels"]["custom:a"] = json!("peer");
    q.discover(1, vec![edit]);
    flush(&mut q, &mut c, true);
    let a = c["providerModels"]["custom:a"].as_array().unwrap();
    assert_eq!(
        a.iter()
            .map(|m| m["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["A", "gpt-6-sol", "peer"]
    );
    assert_eq!(a[0]["maxTokens"], 9999);
    assert_eq!(a[0]["protocol"], "anthropic");
    assert_eq!(a[1]["capabilities"], json!(["vision", "thinking", "tools"]));
    assert_eq!(c["providerSelectedModels"]["custom:a"], "peer");
    let mut c = json!({"provider":"ollama","providerModels":{"ollama":[]}});
    d.load("ollama".into(), &c);
    q.discover(
        1,
        vec![d
            .discovered(d.discovery_token(), vec!["new".into()])
            .unwrap()],
    );
    flush(&mut q, &mut c, true);
    assert_eq!(c["providerSelectedModels"]["ollama"], "new");
    assert_eq!(c["model"], "new");
}
