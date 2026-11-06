//! Synthetic GPUI fixtures: no production profile, clipboard, credentials, networking or desktop launch.
use super::*;
use ::core::prelude::v1::test;
use gpui::EntityInputHandler;
use serde_json::json;
#[derive(Default)]
struct Fixture {
    calls: std::sync::Mutex<Vec<(String, Vec<Value>)>>,
    fail_save: std::sync::atomic::AtomicBool,
    image: std::sync::Mutex<Option<String>>,
    releases: std::sync::Mutex<Vec<String>>,
    release_reply: std::sync::Mutex<Option<Result<String, String>>>,
    vault_unlocked: std::sync::atomic::AtomicBool,
    vault_cancel: std::sync::atomic::AtomicBool,
    vault_copy_error: std::sync::atomic::AtomicBool,
}
impl UtilityTransport for Fixture {
    fn send(
        &self,
        r: UtilityRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Reply> + Send>> {
        let (channel, args) = r.wire();
        self.calls
            .lock()
            .unwrap()
            .push((channel.into(), args.clone()));
        let value = match channel {
            "notes:list" => Ok(
                json!([{"createdAt":11,"updatedAt":12,"text":"<div>Hello <strong>世界</strong></div>"},{"createdAt":21,"updatedAt":22,"text":"Other note"}]),
            ),
            "notes:update" if self.fail_save.load(std::sync::atomic::Ordering::SeqCst) => {
                Err("Synthetic disk full".into())
            }
            "snippets:list" => Ok(
                json!([{"id":"s1","title":"Fixture snippet","subtitle":"","trigger":";fixture","bodyTemplate":"Hello ${date}","resolvedPreview":"Hello today","readonly":false}]),
            ),
            "snippets:add" => Ok(json!({"ok":true,"id":"created-synthetic"})),
            "clipboard:image" => Ok(self
                .image
                .lock()
                .unwrap()
                .as_ref()
                .map(|url| json!({"dataUrl":url,"width":1,"height":1,"byteSize":70}))
                .unwrap_or(Value::Null)),
            "clipboard:read" => Ok(json!("<script>synthetic</script> & safe")),
            "clipboard:list" => Ok(
                json!([{"id":"secret","createdAt":1,"pinned":false,"isSecret":true,"kind":"text","text":"synthetic-private","preview":"synthetic-private","charCount":17,"lineCount":1},{"id":"plain","createdAt":2,"pinned":true,"isSecret":false,"kind":"text","text":"Hello 世界","preview":"Hello 世界","charCount":8,"lineCount":1}]),
            ),
            "system-stats:get" => {
                Ok(json!({"cpu":{"usagePercent":25},"processes":{"total":8,"running":2}}))
            }
            "background-tasks:list" => Ok(json!([])),
            "vault:status" => Ok(
                json!({"supported":true,"unlocked":self.vault_unlocked.load(std::sync::atomic::Ordering::SeqCst)}),
            ),
            "vault:unlock" => Ok(
                json!({"ok":!self.vault_cancel.load(std::sync::atomic::Ordering::SeqCst),"cancelled":self.vault_cancel.load(std::sync::atomic::Ordering::SeqCst)}),
            ),
            "vault:list" => Ok(
                json!([{"id":"vault-fixture","name":"Synthetic password","kind":"password","createdAt":1,"updatedAt":2}]),
            ),
            "vault:get" => Ok(
                json!({"id":"vault-fixture","name":"Synthetic password","kind":"password","value":"synthetic-private","note":""}),
            ),
            "vault:copy"
                if self
                    .vault_copy_error
                    .load(std::sync::atomic::Ordering::SeqCst) =>
            {
                Err("Synthetic copy failed".into())
            }
            "permissions:snapshot" => Ok(json!({"platform":"macOS","statuses":[]})),
            "knowledge:snapshot" => Ok(json!({"status":{"state":"idle"}})),
            "knowledge:sources:list" => {
                Ok(json!({"sources":[],"total":0,"offset":0,"hasMore":false}))
            }
            _ => Ok(json!(true)),
        };
        Box::pin(async move { value })
    }
    fn release(
        &self,
        url: String,
        _: &'static str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send>> {
        self.releases.lock().unwrap().push(url);
        let result = self
            .release_reply
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| Err("Synthetic unavailable release".into()));
        Box::pin(async move { result })
    }
}
fn runtime(cx: &mut TestAppContext) -> tokio::runtime::Runtime {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| {
        gpui_tokio::init_from_handle(cx, rt.handle().clone());
        tezbar_ui::text_input::bind_keys(cx);
    });
    rt
}
fn settle(rt: &tokio::runtime::Runtime, cx: &mut VisualTestContext) {
    rt.block_on(async {
        for _ in 0..64 {
            cx.run_until_parked();
            tokio::task::yield_now().await;
        }
    });
}
fn nav(surface: Surface, nonce: u64) -> Navigation {
    Navigation {
        nonce,
        surface,
        payload: Payload::None,
    }
}
#[gpui::test]
fn clipboard_native_selection_secret_boundary_and_one_restore(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.navigate(nav(Surface::Clipboard, 1), w, cx)));
    settle(&rt, cx);
    assert_eq!(view.read_with(cx, |v, _| v.clipboard.rows.len()), 2);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let secret = v
                .clipboard
                .visible()
                .iter()
                .position(|e| e.is_secret)
                .unwrap();
            v.act(Action::Select(secret), w, cx);
            assert!(
                !v.clipboard_rows()
                    .iter()
                    .any(|(_, label)| label.contains("synthetic-private"))
            );
            assert_eq!(v.clipboard.preview_text().as_deref(), Some("Secure item"));
            v.act(
                Action::Convert(tezbar_core::instant::transform::ClipboardAction::Hash),
                w,
                cx,
            );
            assert!(v.clipboard.conversion.is_none());
            let plain = v
                .clipboard
                .visible()
                .iter()
                .position(|e| !e.is_secret)
                .unwrap();
            v.act(Action::Select(plain), w, cx);
            v.act(Action::Activate, w, cx);
        })
    });
    settle(&rt, cx);
    assert_eq!(
        fixture
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| c == "clipboard:restore")
            .count(),
        1
    );
}
#[gpui::test]
fn snippet_create_actual_id_cancel_and_delete_confirmation(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.navigate(nav(Surface::Snippets, 1), w, cx)));
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.act(Action::New, w, cx);
            v.snippets.change(model::SnippetDraft {
                label: "Created".into(),
                trigger: ";created".into(),
                body: "Body".into(),
            });
            v.act(Action::Save, w, cx);
            v.act(Action::Save, w, cx);
        })
    });
    settle(&rt, cx);
    assert_eq!(
        fixture
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| c == "snippets:add")
            .count(),
        1
    );
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.act(Action::Cancel, w, cx);
            v.act(Action::Delete, w, cx);
            assert!(v.confirmation.is_some());
            v.act(Action::Confirm(false), w, cx);
        })
    });
    settle(&rt, cx);
    assert!(
        !fixture
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| c == "snippets:delete")
    );
}
#[gpui::test]
fn notes_real_rich_ime_format_save_failure_retry_and_identity(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.navigate(
                Navigation {
                    payload: Payload::Note { created_at: 11 },
                    ..nav(Surface::Notes, 1)
                },
                w,
                cx,
            )
        })
    });
    settle(&rt, cx);
    let input = view.read_with(cx, |v, cx| v.editor.read(cx).input.clone());
    cx.update(|w, cx| {
        input.update(cx, |i, cx| {
            i.replace_and_mark_text_in_range(Some(0..0), "に", None, w, cx);
        })
    });
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| {
        v.notes.document().unwrap().html().contains("Hello")
    }));
    assert!(!view.read_with(cx, |v, _| v.notes.document().unwrap().html().contains("に")));
    cx.update(|w, cx| input.update(cx, |i, cx| i.replace_text_in_range(None, "日本", w, cx)));
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| {
        v.notes.document().unwrap().html().contains("日本")
    }));
    cx.update(|w, cx| {
        input.update(cx, |i, cx| {
            let text = i.content.to_string();
            i.replace_silently(
                &text,
                tezbar_ui::text_input::ByteSelection {
                    range: 0..6,
                    reversed: true,
                },
                w,
                cx,
            );
        });
        view.read(cx)
            .editor
            .clone()
            .update(cx, |e, cx| e.format(note_model::Style::Underline, w, cx));
    });
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| {
        v.notes.document().unwrap().html().contains("<u>日本</u>")
    }));
    fixture
        .fail_save
        .store(true, std::sync::atomic::Ordering::SeqCst);
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            for e in v.notes.tick(u64::MAX) {
                v.note_effect(e, cx);
            }
        })
    });
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| matches!(
        v.notes.save_states.get(&11),
        Some(note_model::SaveState::Error(_))
    )));
    fixture
        .fail_save
        .store(false, std::sync::atomic::Ordering::SeqCst);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.act(Action::RetryNote, w, cx);
            v.edit_note(
                tezbar_ui::rich_text::Edit::Replace {
                    owner: Some(21),
                    start: 0,
                    end: 0,
                    text: "WRONG".into(),
                    styles: vec![],
                    selection: tezbar_ui::rich_text::SelectionChange {
                        before: note_model::EditorSelection {
                            start: 0,
                            end: 0,
                            reversed: false,
                        },
                        after: note_model::EditorSelection {
                            start: 0,
                            end: 0,
                            reversed: false,
                        },
                    },
                },
                cx,
            );
        })
    });
    settle(&rt, cx);
    assert!(!view.read_with(cx, |v, _| {
        v.notes.document().unwrap().html().contains("WRONG")
    }));
    let calls = fixture.calls.lock().unwrap();
    let saves = calls
        .iter()
        .filter(|(c, _)| c == "notes:update")
        .collect::<Vec<_>>();
    assert_eq!(saves.len(), 2);
    assert!(saves.iter().all(|(_, a)| a[0]["createdAt"] == 11));
}
#[gpui::test]
fn monitor_hidden_stops_polling_and_stale_reply_cannot_change_view(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.navigate(nav(Surface::SystemMonitor, 1), w, cx)
        })
    });
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| v.monitor.value.is_some()));
    let count = fixture.calls.lock().unwrap().len();
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.set_visible(false, w, cx);
            v.tick(cx);
            assert!(v.monitor.tick(u64::MAX).is_none());
            v.complete(Completion::Monitor(1), Ok(json!({"cpu":"stale"})), cx);
        })
    });
    settle(&rt, cx);
    assert_eq!(fixture.calls.lock().unwrap().len(), count);
    assert_ne!(
        view.read_with(cx, |v, _| v.monitor.value.clone().unwrap()["cpu"].clone()),
        json!("stale")
    );
}
#[gpui::test]
fn emoji_native_grid_persists_recent_and_emits_single_ordered_hide_paste(cx: &mut TestAppContext) {
    let _rt = runtime(cx);
    let (view, cx) = cx.add_window_view(|w, cx| {
        Utilities::new(Arc::new(Fixture::default()), &UiState::default(), w, cx)
    });
    let effects = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = effects.clone();
    cx.update(|_, cx| {
        cx.subscribe(&view, move |_, e: &UtilityEffect, _| {
            sink.borrow_mut().push(match e {
                UtilityEffect::RecentEmoji(_) => "recent",
                UtilityEffect::HideThenPaste(_) => "hide-paste",
                _ => "other",
            });
        })
        .detach()
    });
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.navigate(nav(Surface::Emoji, 1), w, cx);
            v.act(Action::Select(2), w, cx);
            v.act(Action::Activate, w, cx);
        })
    });
    assert_eq!(*effects.borrow(), vec!["recent", "hide-paste"]);
    assert_eq!(view.read_with(cx, |v, _| v.emoji.recent.len()), 1);
}
#[gpui::test]
fn cancelled_release_and_vault_auth_cannot_cross_navigation(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let (view, cx) = cx.add_window_view(|w, cx| {
        Utilities::new(Arc::new(Fixture::default()), &UiState::default(), w, cx)
    });
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.configure_release("2".into(), Some("# Bundled\nCurrent release".into()));
            v.navigate(nav(Surface::ReleaseNotes, 1), w, cx);
            assert!(matches!(
                v.release.model.state,
                tezbar_update::notes::State::Ready(_)
            ));
            v.navigate(nav(Surface::Vault, 2), w, cx);
            v.vault_complete(vault::Completion::Unlock(999), Ok(json!({"ok":true})), cx);
            assert!(!v.vault.model.unlocked);
        })
    });
    settle(&rt, cx);
}
#[::core::prelude::v1::test]
fn canonical_wire_shapes_and_redacted_diagnostics() {
    let (channel, args) = UtilityRequest::NoteSave {
        id: 9007199,
        html: "<div>Қазақша</div>".into(),
    }
    .wire();
    assert_eq!(channel, "notes:update");
    assert_eq!(
        args,
        json!([{"createdAt":9007199,"text":"<div>Қазақша</div>"}])
            .as_array()
            .unwrap()
            .clone()
    );
    assert!(
        !format!("{:?}", UtilityRequest::WriteText("synthetic-secret".into()))
            .contains("synthetic-secret")
    );
    let (channel, args) = UtilityRequest::SnippetWrite {
        id: Some("s".into()),
        draft: model::SnippetDraft {
            label: "L".into(),
            trigger: ";l".into(),
            body: "B".into(),
        },
    }
    .wire();
    assert_eq!(channel, "snippets:update");
    assert_eq!(args[0], "s");
    assert_eq!(args[1]["label"], "L");
}

#[gpui::test]
fn native_note_list_continuation_and_bounded_model_undo_roundtrip(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let (view, cx) = cx.add_window_view(|w, cx| {
        Utilities::new(Arc::new(Fixture::default()), &UiState::default(), w, cx)
    });
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.navigate(nav(Surface::Notes, 1), w, cx);
        })
    });
    settle(&rt, cx);
    let original = "<ul><li>one</li><li><strong>two</strong></li></ul>";
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.notes.rows.push(note_model::Note {
                created_at: 31,
                updated_at: 32,
                text: original.into(),
            });
            v.notes.select(31).unwrap();
            v.sync_note(cx);
        })
    });
    settle(&rt, cx);
    let input = view.read_with(cx, |v, cx| v.editor.read(cx).input.clone());
    cx.update(|w, cx| {
        input.update(cx, |i, cx| {
            let text = i.content.to_string();
            i.replace_silently(
                &text,
                tezbar_ui::text_input::ByteSelection {
                    range: 3..3,
                    reversed: false,
                },
                w,
                cx,
            );
            i.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("enter");
    settle(&rt, cx);
    let entered = view.read_with(cx, |v, _| v.notes.document().unwrap().html());
    assert!(entered.starts_with("<ul><li>one</li><li><br></li>"));
    assert!(entered.contains("<strong>two</strong>"));
    assert_eq!(
        input.read_with(cx, |i, _| i.selection_bytes().range.start),
        4
    );
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-z"
    } else {
        "ctrl-z"
    });
    settle(&rt, cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.notes.document().unwrap().html()),
        original
    );
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-shift-z"
    } else {
        "ctrl-y"
    });
    settle(&rt, cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.notes.document().unwrap().html()),
        entered
    );
}
#[gpui::test]
fn indexing_filter_paging_fences_and_native_open_wire(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.navigate(nav(Surface::Indexing, 1), w, cx)));
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let source = model::IndexedSource {
                id: "fixture-source".into(),
                path: "/synthetic/document.txt".into(),
                title: "Document".into(),
                byte_size: 12,
                modified_at: 1,
                indexed_at: Some(2),
                status: "indexed".into(),
                error: None,
                total_page_count: 1,
                indexed_page_count: 1,
            };
            v.indexing.page = Some(model::SourcePage {
                sources: vec![source],
                offset: 0,
                total: 2,
                has_more: true,
            });
            v.act(Action::MoreSources, w, cx);
            v.act(Action::Activate, w, cx);
            v.search_changed("latest".into(), cx);
        })
    });
    settle(&rt, cx);
    let calls = fixture.calls.lock().unwrap();
    let pages = calls
        .iter()
        .filter(|(c, _)| c == "knowledge:sources:list")
        .collect::<Vec<_>>();
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[1].1[0]["offset"], 1);
    assert_eq!(pages[1].1[0]["limit"], 200);
    assert!(
        calls
            .iter()
            .any(|(c, a)| c == "shell:open" && a[0] == "/synthetic/document.txt")
    );
    drop(calls);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.set_visible(false, w, cx);
            assert!(v.indexing.load(false).is_some());
        })
    });
}
#[gpui::test]
fn native_delete_confirmation_cannot_transfer_to_new_selection(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.navigate(nav(Surface::Clipboard, 1), w, cx)));
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.act(Action::Delete, w, cx);
            let next = if v.clipboard.selected == 0 { 1 } else { 0 };
            v.act(Action::Select(next), w, cx);
            v.act(Action::Confirm(true), w, cx);
        })
    });
    settle(&rt, cx);
    assert!(
        !fixture
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| c == "clipboard:delete")
    );
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.act(Action::Delete, w, cx);
            v.act(Action::Confirm(true), w, cx);
        })
    });
    settle(&rt, cx);
    assert_eq!(
        fixture
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| c == "clipboard:delete")
            .count(),
        1
    );
}
#[gpui::test]
fn permissions_native_action_lock_refresh_and_accessible_status(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.navigate(nav(Surface::Permissions, 1), w, cx);
        })
    });
    settle(&rt, cx);
    let descriptor = model::PermissionDescriptor {
        id: model::PermissionId::Microphone,
        title: "Microphone".into(),
        summary: "Record audio".into(),
        rationale: "For transcription".into(),
        settings_url: Some("x-apple.systempreferences:privacy".into()),
        remediation: "Enable Microphone in Settings.".into(),
    };
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.permissions.snapshot = Some(model::PermissionsSnapshot {
                platform: "macOS".into(),
                statuses: vec![model::PermissionStatus {
                    descriptor: descriptor.clone(),
                    state: model::PermissionState::NotDetermined,
                    checked_at: 1,
                }],
            });
            v.act(Action::Permission(model::PermissionId::Microphone), w, cx);
            v.act(Action::Permission(model::PermissionId::Microphone), w, cx);
            v.act(
                Action::PermissionSettings("x-apple.systempreferences:privacy".into()),
                w,
                cx,
            );
        })
    });
    settle(&rt, cx);
    let calls = fixture.calls.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .filter(|(c, _)| c == "permissions:request")
            .count(),
        1
    );
    assert!(
        calls
            .iter()
            .any(|(c, a)| c == "shell:open" && a[0] == "x-apple.systempreferences:privacy")
    );
    assert!(view.read_with(cx, |v, _| v.permissions.pending.is_none()));
    assert!(view.read_with(cx, |v, _| v.permissions.banner.is_some()));
}
#[gpui::test]
fn vault_clipboard_draft_auth_save_cleanup_uses_synthetic_wire(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.navigate(nav(Surface::Clipboard, 1), w, cx)));
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let i = v
                .clipboard
                .visible()
                .iter()
                .position(|e| e.is_secret)
                .unwrap();
            v.act(Action::Select(i), w, cx);
            v.act(Action::VaultDraft, w, cx);
            v.navigate(nav(Surface::Vault, 2), w, cx);
        })
    });
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| v.vault.model.draft.is_some()));
    cx.update(|w, cx| view.update(cx, |v, cx| v.act(Action::Unlock, w, cx)));
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.vault.model.edit_name("Synthetic item".into());
            v.act(Action::Save, w, cx);
            v.act(Action::Save, w, cx);
        })
    });
    settle(&rt, cx);
    let calls = fixture.calls.lock().unwrap();
    assert_eq!(calls.iter().filter(|(c, _)| c == "vault:save").count(), 1);
    assert_eq!(
        calls
            .iter()
            .filter(|(c, a)| c == "clipboard:delete" && a[0] == "secret")
            .count(),
        1
    );
    assert!(
        !format!(
            "{:?}",
            view.read_with(cx, |v, _| v
                .vault
                .model
                .draft
                .as_ref()
                .map(|d| format!("{d:?}")))
        )
        .contains("synthetic-private")
    );
}
#[gpui::test]
fn background_actions_are_typed_and_hidden_owner_does_not_poll(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let (view, cx) = cx.add_window_view(|w, cx| {
        Utilities::new(Arc::new(Fixture::default()), &UiState::default(), w, cx)
    });
    let emitted = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = emitted.clone();
    cx.update(|_, cx| {
        cx.subscribe(&view, move |_, e: &UtilityEffect, _| {
            sink.borrow_mut().push(match e {
                UtilityEffect::OpenExtension { .. } => "extension",
                UtilityEffect::Navigate(Surface::Indexing, _) => "indexing",
                _ => "other",
            })
        })
        .detach()
    });
    cx.update(|w,cx|view.update(cx,|v,cx|{v.navigate(nav(Surface::Emoji,1),w,cx);v.background.tasks=serde_json::from_value(json!([{"id":"index","kind":"indexing","title":"Index","detail":"Files"},{"id":"timer","kind":"timer","title":"Timer","detail":"Running","extensionId":"synthetic","commandName":"timer"}])).unwrap();v.act(Action::OpenTask(0),w,cx);v.act(Action::OpenTask(1),w,cx);}));
    settle(&rt, cx);
    assert_eq!(*emitted.borrow(), vec!["indexing", "extension"]);
}
#[gpui::test]
fn clipboard_native_lazy_image_decodes_wire_object_and_releases_on_hide(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(1, 1, image::Rgba([80, 90, 100, 255]))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    *fixture.image.lock().unwrap() = Some(clipboard::encode_png(bytes.get_ref()));
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.navigate(nav(Surface::Clipboard, 1), w, cx)));
    settle(&rt, cx);
    assert!(
        !fixture
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| c == "clipboard:image")
    );
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.clipboard.rows.push(model::ClipboardEntry {
                id: "image".into(),
                created_at: 9,
                pinned: false,
                is_secret: false,
                content: model::ClipboardContent::Image {
                    image_path: "/synthetic/image.png".into(),
                    width: 1,
                    height: 1,
                    byte_size: 70,
                },
            });
            let i = v
                .clipboard
                .visible()
                .iter()
                .position(|e| e.id == "image")
                .unwrap();
            v.act(Action::Select(i), w, cx);
        })
    });
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| v.preview_image.is_some()));
    assert_eq!(
        fixture
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| c == "clipboard:image")
            .count(),
        1
    );
    cx.update(|w, cx| view.update(cx, |v, cx| v.set_visible(false, w, cx)));
    assert!(view.read_with(cx, |v, _| v.preview_image.is_none()));
}
#[gpui::test]
fn native_paste_is_plain_safe_and_note_owned(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.navigate(
                Navigation {
                    payload: Payload::Note { created_at: 11 },
                    ..nav(Surface::Notes, 1)
                },
                w,
                cx,
            )
        })
    });
    settle(&rt, cx);
    let input = view.read_with(cx, |v, cx| v.editor.read(cx).input.clone());
    cx.update(|w, cx| input.focus_handle(cx).focus(w, cx));
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-v"
    } else {
        "ctrl-v"
    });
    settle(&rt, cx);
    let html = view.read_with(cx, |v, _| v.notes.document().unwrap().html());
    assert!(html.contains("&lt;script&gt;synthetic&lt;/script&gt; &amp; safe"));
    assert!(!html.contains("<script>"));
    assert_eq!(
        fixture
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| c == "clipboard:read")
            .count(),
        1
    );
}
#[gpui::test]
fn vault_icon_picker_source_registry_filter_keyboard_default_and_escape(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let (view, cx) = cx.add_window_view(|w, cx| {
        Utilities::new(Arc::new(Fixture::default()), &UiState::default(), w, cx)
    });
    cx.update(|w, cx| view.update(cx, |v, cx| v.navigate(nav(Surface::Vault, 1), w, cx)));
    settle(&rt, cx);
    cx.update(|w, cx| view.update(cx, |v, cx| v.act(Action::Unlock, w, cx)));
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.act(Action::New, w, cx);
            v.act(Action::ChangeIcon, w, cx);
            assert!(v.vault.icon_open);
            assert_eq!(v.vault.icons()[v.vault.icon_selected].id, "password");
            assert!(
                v.icon_filter
                    .as_ref()
                    .unwrap()
                    .focus_handle(cx)
                    .is_focused(w)
            );
            v.icon_filter
                .as_ref()
                .unwrap()
                .update(cx, |f, cx| f.set_text("biometric", w, cx));
        })
    });
    settle(&rt, cx);
    assert_eq!(
        view.read_with(cx, |v, _| v
            .vault
            .icons()
            .iter()
            .map(|i| i.id.clone())
            .collect::<Vec<_>>()),
        vec!["fingerprint"]
    );
    cx.simulate_keystrokes("escape");
    settle(&rt, cx);
    assert!(!view.read_with(cx, |v, _| v.vault.icon_open));
    assert!(view.read_with(cx, |v, _| v.vault.model.draft.is_some()));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.act(Action::ChangeIcon, w, cx);
            v.act(Action::VaultIcon("cloud".into()), w, cx);
            assert_eq!(
                v.vault.model.draft.as_ref().unwrap().icon.as_deref(),
                Some("cloud")
            );
            v.act(Action::ChangeIcon, w, cx);
            v.act(Action::DefaultIcon, w, cx);
            assert!(v.vault.model.draft.as_ref().unwrap().icon.is_none());
        })
    });
    assert_eq!(
        vault::icon_choices()
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>(),
        tezbar_ui_model::vault::ICONS
    );
}
#[gpui::test]
fn native_rich_backspace_delete_join_preserves_run_styles_and_undo_html(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let (view, cx) = cx.add_window_view(|w, cx| {
        Utilities::new(Arc::new(Fixture::default()), &UiState::default(), w, cx)
    });
    cx.update(|w, cx| view.update(cx, |v, cx| v.navigate(nav(Surface::Notes, 1), w, cx)));
    settle(&rt, cx);
    let original = "<div><strong>Қазақ</strong></div><p><em>日本</em></p>";
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.notes.rows.push(note_model::Note {
                created_at: 41,
                updated_at: 42,
                text: original.into(),
            });
            v.notes.select(41).unwrap();
            v.sync_note(cx);
        })
    });
    settle(&rt, cx);
    let input = view.read_with(cx, |v, cx| v.editor.read(cx).input.clone());
    cx.update(|w, cx| {
        input.update(cx, |i, cx| {
            let text = i.content.to_string();
            assert_eq!(text, "Қазақ\n日本");
            i.replace_silently(
                &text,
                tezbar_ui::text_input::ByteSelection {
                    range: 11..11,
                    reversed: false,
                },
                w,
                cx,
            );
            i.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("backspace");
    settle(&rt, cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.notes.document().unwrap().html()),
        "<div><strong>Қазақ</strong><em>日本</em></div>"
    );
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-z"
    } else {
        "ctrl-z"
    });
    settle(&rt, cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.notes.document().unwrap().html()),
        original
    );
    assert_eq!(
        input.read_with(cx, |i, _| i.selection_bytes()),
        tezbar_ui::text_input::ByteSelection {
            range: 11..11,
            reversed: false
        }
    );
    cx.update(|w, cx| {
        input.update(cx, |i, cx| {
            let text = i.content.to_string();
            i.replace_silently(
                &text,
                tezbar_ui::text_input::ByteSelection {
                    range: 10..10,
                    reversed: false,
                },
                w,
                cx,
            );
        })
    });
    cx.simulate_keystrokes("delete");
    settle(&rt, cx);
    assert_eq!(
        input.read_with(cx, |i, _| i.content.to_string()),
        "Қазақ日本"
    );
}
#[gpui::test]
fn native_rich_range_replace_and_empty_list_enter_keep_canonical_html(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let (view, cx) = cx.add_window_view(|w, cx| {
        Utilities::new(Arc::new(Fixture::default()), &UiState::default(), w, cx)
    });
    cx.update(|w, cx| view.update(cx, |v, cx| v.navigate(nav(Surface::Notes, 1), w, cx)));
    settle(&rt, cx);
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.notes.rows.push(note_model::Note {
                created_at: 51,
                updated_at: 52,
                text: "<div><strong>Қазақ</strong></div><p><em>日本</em></p>".into(),
            });
            v.notes.select(51).unwrap();
            v.sync_note(cx);
        })
    });
    settle(&rt, cx);
    let input = view.read_with(cx, |v, cx| v.editor.read(cx).input.clone());
    cx.update(|w, cx| {
        input.update(cx, |i, cx| {
            i.replace_text_in_range(Some(2..8), "X\nY", w, cx)
        })
    });
    settle(&rt, cx);
    let html = view.read_with(cx, |v, _| v.notes.document().unwrap().html());
    assert!(html.contains("<strong>ҚаX<br>Y</strong>"));
    assert_eq!(input.read_with(cx, |i, _| i.content.to_string()), "ҚаX\nY");
    let original = "<ul><li>one</li><li><br></li><li><em>last</em></li></ul>";
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.notes.rows.push(note_model::Note {
                created_at: 61,
                updated_at: 62,
                text: original.into(),
            });
            v.notes.select(61).unwrap();
            v.sync_note(cx);
        })
    });
    settle(&rt, cx);
    let input = view.read_with(cx, |v, cx| v.editor.read(cx).input.clone());
    cx.update(|w, cx| {
        input.update(cx, |i, cx| {
            let text = i.content.to_string();
            i.replace_silently(
                &text,
                tezbar_ui::text_input::ByteSelection {
                    range: 4..4,
                    reversed: false,
                },
                w,
                cx,
            );
            i.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("enter");
    settle(&rt, cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.notes.document().unwrap().html()),
        "<ul><li>one</li></ul><div><br></div><ul><li><em>last</em></li></ul>"
    );
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-z"
    } else {
        "ctrl-z"
    });
    settle(&rt, cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.notes.document().unwrap().html()),
        original
    );
}
#[gpui::test]
fn explicit_drain_retries_failed_note_once_and_reports_new_failure(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.navigate(
                Navigation {
                    payload: Payload::Note { created_at: 11 },
                    ..nav(Surface::Notes, 1)
                },
                w,
                cx,
            )
        })
    });
    settle(&rt, cx);
    fixture
        .fail_save
        .store(true, std::sync::atomic::Ordering::SeqCst);
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.notes.edit(v.now(), |d| d.replace(0, 0, "dirty")).unwrap();
            for e in v.notes.tick(u64::MAX) {
                v.note_effect(e, cx);
            }
        })
    });
    settle(&rt, cx);
    let result = std::rc::Rc::new(std::cell::RefCell::new(None));
    let sink = result.clone();
    let drain = cx.update(|_, cx| view.update(cx, |v, cx| v.drain(cx)));
    let _wait = cx.spawn(async move |_| {
        *sink.borrow_mut() = Some(drain.await);
    });
    settle(&rt, cx);
    cx.executor().advance_clock(Duration::from_millis(30));
    settle(&rt, cx);
    assert!(result.borrow().as_ref().unwrap().is_err());
    assert_eq!(
        fixture
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| c == "notes:update")
            .count(),
        2
    );
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(&rt, cx);
    assert_eq!(
        fixture
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| c == "notes:update")
            .count(),
        2
    );
}
#[gpui::test]
fn native_collapsed_bold_types_formatted_run_and_undo_restores_caret(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let (view, cx) = cx.add_window_view(|w, cx| {
        Utilities::new(Arc::new(Fixture::default()), &UiState::default(), w, cx)
    });
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.navigate(
                Navigation {
                    payload: Payload::Note { created_at: 11 },
                    ..nav(Surface::Notes, 1)
                },
                w,
                cx,
            )
        })
    });
    settle(&rt, cx);
    let input = view.read_with(cx, |v, cx| v.editor.read(cx).input.clone());
    cx.update(|w, cx| input.focus_handle(cx).focus(w, cx));
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-b"
    } else {
        "ctrl-b"
    });
    cx.simulate_input("太");
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| {
        v.notes
            .document()
            .unwrap()
            .html()
            .contains("<strong>太</strong>")
    }));
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-z"
    } else {
        "ctrl-z"
    });
    settle(&rt, cx);
    assert_eq!(input.read_with(cx, |i, _| i.selection_bytes().range), 0..0);
}
#[::core::prelude::v1::test]
fn structural_helper_preserves_nested_list_neighbors_and_inline_styles() {
    use tezbar_ui::rich_text::replace_visual;
    let mut doc = note_model::Document::load(
        "<div>prefix<ul><li><strong>item</strong></li><li>next</li></ul>suffix</div>",
    )
    .unwrap();
    replace_visual(&mut doc, 6, 7, "").unwrap();
    assert_eq!(
        doc.html(),
        "<div>prefix<strong>item</strong><ul><li>next</li></ul>suffix</div>"
    );
    let mut doc =
        note_model::Document::load("<div><div><em>first</em></div>suffix<div>last</div></div>")
            .unwrap();
    replace_visual(&mut doc, 5, 6, "").unwrap();
    assert_eq!(
        doc.html(),
        "<div><div><em>first</em>suffix</div><div>last</div></div>"
    );
}
#[gpui::test]
fn qr_preview_copy_and_export_are_worker_owned_and_selection_scoped(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    let exports = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = exports.clone();
    cx.update(|_, cx| {
        cx.subscribe(&view, move |_, effect: &UtilityEffect, _| {
            if let UtilityEffect::SaveQr(png) = effect {
                sink.borrow_mut().push(png.clone());
            }
        })
        .detach()
    });
    cx.update(|w, cx| view.update(cx, |v, cx| v.navigate(nav(Surface::Clipboard, 1), w, cx)));
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let i = v
                .clipboard
                .visible()
                .iter()
                .position(|e| !e.is_secret)
                .unwrap();
            v.act(Action::Select(i), w, cx);
            v.act(
                Action::Convert(tezbar_core::instant::transform::ClipboardAction::Qr),
                w,
                cx,
            );
        })
    });
    settle(&rt, cx);
    assert!(view.read_with(cx, |v, _| v.qr_image.is_some()));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.act(Action::CopyQr, w, cx);
            v.act(Action::SaveQr, w, cx);
        })
    });
    settle(&rt, cx);
    assert_eq!(exports.borrow().len(), 1);
    assert!(exports.borrow()[0].starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(
        fixture
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, a)| c == "clipboard:write-png"
                && a[0]
                    .as_str()
                    .is_some_and(|url| url.starts_with("data:image/png;base64,")))
            .count(),
        1
    );
}
#[gpui::test]
fn release_exact_version_native_enter_full_markdown_error_and_retry(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let body = format!("# Historical\n{}", "- Change **preserved**\n".repeat(100));
    *fixture.release_reply.lock().unwrap() = Some(Ok(
        json!({"tag_name":"v1","body":body,"published_at":"2026-01-02"}).to_string(),
    ));
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.configure_release("2".into(), Some("# Current".into()));
            v.navigate(nav(Surface::ReleaseNotes, 1), w, cx);
            v.search.update(cx, |f, cx| f.set_text("1", w, cx));
        })
    });
    assert!(fixture.releases.lock().unwrap().is_empty());
    cx.simulate_keystrokes("enter");
    settle(&rt, cx);
    assert_eq!(
        *fixture.releases.lock().unwrap(),
        vec!["https://api.github.com/repos/almatkai/Tezbar/releases/tags/v1"]
    );
    assert!(view.read_with(cx,|v,_|matches!(&v.release.model.state,tezbar_update::notes::State::Ready(n)if n.version=="1"&&n.body==body&&n.published_at.as_deref()==Some("2026-01-02"))));
    *fixture.release_reply.lock().unwrap() = Some(Ok(
        json!({"tag_name":"v3","body":"Wrong version"}).to_string(),
    ));
    cx.update(|w, cx| view.update(cx, |v, cx| v.act(Action::Refresh, w, cx)));
    settle(&rt, cx);
    assert!(view.read_with(cx,|v,_|matches!(&v.release.model.state,tezbar_update::notes::State::Error(e)if e.contains("does not match"))));
    *fixture.release_reply.lock().unwrap() =
        Some(Ok(json!({"tag_name":"v1","body":body}).to_string()));
    cx.update(|w, cx| view.update(cx, |v, cx| v.act(Action::Refresh, w, cx)));
    settle(&rt, cx);
    assert!(view.read_with(cx,|v,_|matches!(&v.release.model.state,tezbar_update::notes::State::Ready(n)if n.version=="1")));
}
#[gpui::test]
fn retired_note_native_ime_handler_cannot_commit_into_new_note(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let (view, cx) = cx.add_window_view(|w, cx| {
        Utilities::new(Arc::new(Fixture::default()), &UiState::default(), w, cx)
    });
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.navigate(
                Navigation {
                    payload: Payload::Note { created_at: 11 },
                    ..nav(Surface::Notes, 1)
                },
                w,
                cx,
            )
        })
    });
    settle(&rt, cx);
    let old_input = view.read_with(cx, |v, cx| v.editor.read(cx).input.clone());
    cx.update(|w, cx| {
        old_input.update(cx, |i, cx| {
            i.replace_and_mark_text_in_range(None, "に", None, w, cx)
        })
    });
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.notes.select(21).unwrap();
            v.sync_note(cx);
        })
    });
    settle(&rt, cx);
    let current_input = view.read_with(cx, |v, cx| v.editor.read(cx).input.clone());
    assert_ne!(old_input.entity_id(), current_input.entity_id());
    cx.update(|w, cx| old_input.update(cx, |i, cx| i.replace_text_in_range(None, "日本", w, cx)));
    settle(&rt, cx);
    assert!(!view.read_with(cx, |v, _| {
        v.notes.document().unwrap().html().contains("日本")
    }));
    assert_eq!(view.read_with(cx, |v, _| v.notes.selected), Some(21));
}
#[derive(Default)]
struct DelayedFixture {
    inner: Fixture,
    gate: std::sync::Mutex<Option<(&'static str, tokio::sync::oneshot::Receiver<Reply>)>>,
}
impl UtilityTransport for DelayedFixture {
    fn send(
        &self,
        request: UtilityRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Reply> + Send>> {
        let (channel, args) = request.clone().wire();
        let rx = {
            let mut gate = self.gate.lock().unwrap();
            if gate.as_ref().is_some_and(|(held, _)| *held == channel) {
                gate.take().map(|(_, rx)| rx)
            } else {
                None
            }
        };
        if let Some(rx) = rx {
            self.inner
                .calls
                .lock()
                .unwrap()
                .push((channel.into(), args));
            Box::pin(async move { rx.await.map_err(|_| "Synthetic cancelled".to_string())? })
        } else {
            self.inner.send(request)
        }
    }
    fn release(
        &self,
        url: String,
        accept: &'static str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send>> {
        self.inner.release(url, accept)
    }
}
#[gpui::test]
fn delayed_note_create_and_save_update_store_owner_without_stealing_selection(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    let fixture = Arc::new(DelayedFixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.navigate(
                Navigation {
                    payload: Payload::Note { created_at: 11 },
                    ..nav(Surface::Notes, 1)
                },
                w,
                cx,
            )
        })
    });
    settle(&rt, cx);
    let (tx, rx) = tokio::sync::oneshot::channel();
    *fixture.gate.lock().unwrap() = Some(("notes:append", rx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.act(Action::New, w, cx);
            v.notes.select(21).unwrap();
            v.sync_note(cx);
        })
    });
    settle(&rt, cx);
    tx.send(Ok(
        json!({"createdAt":101,"updatedAt":102,"text":"<div><br></div>"}),
    ))
    .unwrap();
    settle(&rt, cx);
    assert_eq!(view.read_with(cx, |v, _| v.notes.selected), Some(21));
    assert!(view.read_with(cx, |v, _| v.notes.rows.iter().any(|n| n.created_at == 101)));
    let (tx, rx) = tokio::sync::oneshot::channel();
    *fixture.gate.lock().unwrap() = Some(("notes:update", rx));
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.notes.select(11).unwrap();
            v.notes
                .edit(v.now(), |d| d.replace(0, 0, "Saved prefix "))
                .unwrap();
            for e in v.notes.tick(u64::MAX) {
                v.note_effect(e, cx);
            }
            v.notes.select(21).unwrap();
            v.sync_note(cx);
        })
    });
    settle(&rt, cx);
    tx.send(Ok(json!(true))).unwrap();
    settle(&rt, cx);
    assert_eq!(view.read_with(cx, |v, _| v.notes.selected), Some(21));
    assert!(view.read_with(cx, |v, _| {
        v.notes
            .rows
            .iter()
            .find(|n| n.created_at == 11)
            .unwrap()
            .text
            .contains("Saved prefix")
    }));
    assert_eq!(
        view.read_with(cx, |v, cx| v
            .editor
            .read(cx)
            .input
            .read(cx)
            .content
            .to_string()),
        "Other note"
    );
}
#[gpui::test]
fn delayed_restore_cannot_hide_or_navigate_new_surface(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(DelayedFixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    let events = std::rc::Rc::new(std::cell::RefCell::new(0));
    let sink = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&view, move |_, e: &UtilityEffect, _| {
            if matches!(e, UtilityEffect::Hide | UtilityEffect::Navigate(..)) {
                *sink.borrow_mut() += 1;
            }
        })
        .detach()
    });
    cx.update(|w, cx| view.update(cx, |v, cx| v.navigate(nav(Surface::Clipboard, 1), w, cx)));
    settle(&rt, cx);
    let (tx, rx) = tokio::sync::oneshot::channel();
    *fixture.gate.lock().unwrap() = Some(("clipboard:restore", rx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.act(Action::Activate, w, cx)));
    settle(&rt, cx);
    cx.update(|w, cx| view.update(cx, |v, cx| v.navigate(nav(Surface::Notes, 2), w, cx)));
    let _ = tx.send(Ok(json!(true)));
    settle(&rt, cx);
    assert_eq!(*events.borrow(), 0);
    assert!(view.read_with(cx, |v, _| v.visible && v.surface == Surface::Notes));
}
#[gpui::test]
fn vault_native_copy_checks_status_auth_cancel_and_error_without_secret_wire(
    cx: &mut TestAppContext,
) {
    let rt = runtime(cx);
    for mode in 0..4 {
        let fixture = Arc::new(Fixture::default());
        fixture
            .vault_unlocked
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let (view, window_cx) =
            cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
        window_cx
            .update(|w, cx| view.update(cx, |v, cx| v.navigate(nav(Surface::Vault, 1), w, cx)));
        settle(&rt, window_cx);
        if mode == 1 || mode == 2 {
            fixture
                .vault_unlocked
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }
        if mode == 2 {
            fixture
                .vault_cancel
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
        if mode == 3 {
            fixture
                .vault_copy_error
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
        window_cx.update(|w, cx| view.update(cx, |v, cx| v.act(Action::Copy, w, cx)));
        settle(&rt, window_cx);
        let calls = fixture.calls.lock().unwrap();
        let copies = calls
            .iter()
            .filter(|(c, _)| c == "vault:copy")
            .collect::<Vec<_>>();
        assert_eq!(copies.len(), if mode == 2 { 0 } else { 1 });
        assert!(
            copies
                .iter()
                .all(|(_, args)| args == &vec![json!("vault-fixture")])
        );
        assert_eq!(
            calls.iter().filter(|(c, _)| c == "vault:unlock").count(),
            if mode == 1 || mode == 2 { 1 } else { 0 }
        );
        let state = view.read_with(window_cx, |v, _| v.vault.model.copy_state());
        assert_eq!(
            state,
            match mode {
                2 => tezbar_ui_model::vault::CopyState::Cancelled,
                3 => tezbar_ui_model::vault::CopyState::Error,
                _ => tezbar_ui_model::vault::CopyState::Copied,
            }
        );
    }
}
#[gpui::test]
fn background_host_update_ready_is_actionable_and_idle_has_no_badge(cx: &mut TestAppContext) {
    let _rt = runtime(cx);
    let (view, cx) = cx.add_window_view(|w, cx| {
        Utilities::new(Arc::new(Fixture::default()), &UiState::default(), w, cx)
    });
    let sink = std::rc::Rc::new(std::cell::RefCell::new(0));
    let events = sink.clone();
    cx.update(|_, cx| {
        cx.subscribe(&view, move |_, e: &UtilityEffect, _| {
            if matches!(
                e,
                UtilityEffect::UpdateAction(
                    super::super::command_bar::navigation::UpdateAction::Restart
                )
            ) {
                *events.borrow_mut() += 1;
            }
        })
        .detach()
    });
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.set_update_status(
                tezbar_update::AppUpdateStatus::Ready {
                    version: "synthetic".into(),
                },
                cx,
            );
            v.navigate(nav(Surface::Emoji, 1), w, cx);
            assert_eq!(v.background.all().len(), 1);
            assert_eq!(v.background.all()[0].title, "Update ready");
            v.set_update_status(tezbar_update::AppUpdateStatus::Idle, cx);
            assert!(v.background.all().is_empty());
            v.core_event(
                &CoreEvent::Other {
                    channel: "update-status".into(),
                    payload: json!({"kind":"ready","version":"synthetic"}),
                },
                cx,
            );
            let tasks = v.background.all();
            assert_eq!(tasks.len(), 1);
            assert_eq!(tasks[0].title, "Update ready");
            v.act(Action::OpenTaskId(tasks[0].id.clone()), w, cx);
        })
    });
    assert_eq!(*sink.borrow(), 1);
}
