// Real GPUI regressions from the independent c2a36b5 review; shared fixture only.
fn review_note(
    html: &str,
    caret: usize,
    keys: &str,
    cx: &mut TestAppContext,
) -> (String, String, usize) {
    let rt = runtime(cx);
    let (view, cx) = cx.add_window_view(|w, cx| {
        Utilities::new(Arc::new(Fixture::default()), &UiState::default(), w, cx)
    });
    cx.update(|w, cx| view.update(cx, |v, cx| v.navigate(nav(Surface::Notes, 1), w, cx)));
    settle(&rt, cx);
    let html = html.to_string();
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.notes.rows.push(note_model::Note {
                created_at: 77,
                updated_at: 78,
                text: html,
            });
            v.notes.select(77).unwrap();
            v.note_editing = true;
            v.sync_note(cx);
        })
    });
    settle(&rt, cx);
    let input = view.read_with(cx, |v, cx| v.editor.read(cx).input.clone());
    cx.update(|w, cx| {
        input.update(cx, |i, cx| {
            let text = i.content.to_string();
            eprintln!("OPUS projection before: {text:?}");
            i.replace_silently(
                &text,
                tezbar_ui::text_input::ByteSelection {
                    range: caret..caret,
                    reversed: false,
                },
                w,
                cx,
            );
            i.focus_handle(cx).focus(w, cx);
        })
    });
    if !keys.is_empty() {
        cx.simulate_keystrokes(keys);
    }
    settle(&rt, cx);
    let out = view.read_with(cx, |v, _| v.notes.document().unwrap().html());
    let input = view.read_with(cx, |v, cx| v.editor.read(cx).input.clone());
    let (text, caret) = input.read_with(cx, |i, _| {
        (i.content.to_string(), i.selection_bytes().range.start)
    });
    eprintln!("OPUS after keys {keys:?}: html={out:?} visual={text:?} caret={caret}");
    (out, text, caret)
}
#[gpui::test]
fn review_type_into_empty_note(cx: &mut TestAppContext) {
    let (html, text, _) = review_note("<div><br></div>", 0, "a", cx);
    assert_eq!(html, "<div>a</div>", "visual={text:?}");
}
#[gpui::test]
fn review_enter_then_type(cx: &mut TestAppContext) {
    let (html, text, _) = review_note("<div>abc</div>", 3, "enter d", cx);
    assert_eq!(html, "<div>abc</div><div>d</div>", "visual={text:?}");
}
#[gpui::test]
fn review_list_enter_then_type(cx: &mut TestAppContext) {
    let (html, text, _) = review_note("<ul><li>one</li></ul>", 3, "enter x", cx);
    assert_eq!(html, "<ul><li>one</li><li>x</li></ul>", "visual={text:?}");
}
#[gpui::test]
fn review_empty_paragraph_projection_line_count(cx: &mut TestAppContext) {
    let (_, text, _) = review_note("<div>a</div><div><br></div><div>b</div>", 0, "", cx);
    assert_eq!(text, "a\n\nb");
}
#[gpui::test]
fn review_forward_delete_bold_char_before_identical_plain_char(cx: &mut TestAppContext) {
    let (html, text, _) = review_note("<div><strong>a</strong>a</div>", 0, "delete", cx);
    assert_eq!(html, "<div>a</div>", "visual={text:?}");
}
#[gpui::test]
fn review_vault_save_then_hide_still_deletes_clipboard_source(cx: &mut TestAppContext) {
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
    cx.update(|w, cx| view.update(cx, |v, cx| v.act(Action::Unlock, w, cx)));
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.vault.model.edit_name("Synthetic item".into());
            v.act(Action::Save, w, cx);
            // User presses Escape / the launcher blur-hides before vault:save replies.
            v.set_visible(false, w, cx);
        })
    });
    settle(&rt, cx);
    let calls = fixture.calls.lock().unwrap();
    eprintln!(
        "OPUS vault calls: {:?}",
        calls.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>()
    );
    assert_eq!(calls.iter().filter(|(c, _)| c == "vault:save").count(), 1);
    assert_eq!(
        calls
            .iter()
            .filter(|(c, a)| c == "clipboard:delete" && a[0] == "secret")
            .count(),
        1,
        "saved secret must leave clipboard history even when the view hid first"
    );
}
#[gpui::test]
fn review_invalid_snippet_form_does_not_block_quit_drain(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| view.update(cx, |v, cx| v.navigate(nav(Surface::Snippets, 1), w, cx)));
    settle(&rt, cx);
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.act(Action::New, w, cx);
            v.act(Action::Save, w, cx); // empty title: local validation error
            eprintln!("OPUS snippet error after save: {:?}", v.snippets.error);
            v.set_visible(false, w, cx);
        })
    });
    settle(&rt, cx);
    let drain = view.update(cx, |v, cx| v.drain(cx));
    let result = rt.block_on(async {
        for _ in 0..64 {
            cx.run_until_parked();
            tokio::task::yield_now().await;
        }
        drain.await
    });
    eprintln!("OPUS drain result: {result:?}");
    assert!(
        result.is_ok(),
        "an unsaved invalid snippet form blocks quit: {result:?}"
    );
}
#[gpui::test]
fn review_enter_in_note_editor_does_not_copy_note(cx: &mut TestAppContext) {
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
    cx.update(|w, cx| input.update(cx, |i, cx| i.focus_handle(cx).focus(w, cx)));
    cx.simulate_keystrokes("enter");
    settle(&rt, cx);
    let calls = fixture.calls.lock().unwrap();
    assert_eq!(
        calls.iter().filter(|(c, _)| c == "clipboard:write").count(),
        0
    );
}
#[gpui::test]
fn review_new_release_navigation_opens_current_not_last_historical(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture = Arc::new(Fixture::default());
    let (view, cx) =
        cx.add_window_view(|w, cx| Utilities::new(fixture.clone(), &UiState::default(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.configure_release("0.3.0-beta.7".into(), Some("Current bundled notes".into()));
            v.navigate(nav(Surface::ReleaseNotes, 1), w, cx);
            v.act(Action::ReleaseVersion("0.2.0".into()), w, cx);
            v.navigate(nav(Surface::Clipboard, 2), w, cx);
            v.navigate(nav(Surface::ReleaseNotes, 3), w, cx);
        })
    });
    settle(&rt, cx);
    let shown = view.read_with(cx, |v, _| v.release.version.clone());
    eprintln!(
        "OPUS release urls {:?} shown={shown:?}",
        fixture.releases.lock().unwrap()
    );
    assert_eq!(shown.as_deref(), Some("0.3.0-beta.7"));
}

// Windows desktop: Esc on What's new / Permissions did nothing because focus went to the
// search field those headers never paint.
#[gpui::test]
fn escape_leaves_surfaces_without_a_search_field(cx: &mut TestAppContext) {
    for (index, surface) in [Surface::ReleaseNotes, Surface::Permissions]
        .into_iter()
        .enumerate()
    {
        let rt = runtime(cx);
        let (view, vcx) = cx.add_window_view(|w, cx| {
            Utilities::new(Arc::new(Fixture::default()), &UiState::default(), w, cx)
        });
        let backs = std::rc::Rc::new(std::cell::Cell::new(0));
        let sink = backs.clone();
        vcx.update(|_, cx| {
            cx.subscribe(&view, move |_, e: &UtilityEffect, _| {
                if matches!(e, UtilityEffect::Back) {
                    sink.set(sink.get() + 1);
                }
            })
            .detach()
        });
        vcx.update(|w, cx| {
            view.update(cx, |v, cx| {
                v.configure_release("0.3.0-beta.7".into(), Some("Notes".into()));
                v.navigate(nav(surface.clone(), index as u64 + 1), w, cx)
            })
        });
        settle(&rt, vcx);
        vcx.simulate_keystrokes("escape");
        settle(&rt, vcx);
        assert_eq!(backs.get(), 1, "{surface:?}: Esc must go back");
    }
}

#[gpui::test]
fn review_backspace_identical_styled_text_and_delete_paragraph_boundary(cx: &mut TestAppContext) {
    let (html, _, _) = review_note("<div><strong>a</strong>a</div>", 1, "backspace", cx);
    assert_eq!(html, "<div>a</div>");
    let (html, text, _) = review_note("<div>a<br></div><div>a</div>", 1, "delete", cx);
    assert_eq!(html, "<div>aa</div>");
    assert_eq!(text, "aa");
}
#[gpui::test]
fn review_ime_commit_into_empty_paragraph_uses_its_native_block_owner(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let (view, cx) = cx.add_window_view(|w, cx| {
        Utilities::new(Arc::new(Fixture::default()), &UiState::default(), w, cx)
    });
    cx.update(|w, cx| view.update(cx, |v, cx| v.navigate(nav(Surface::Notes, 1), w, cx)));
    settle(&rt, cx);
    view.update(cx, |v, cx| {
        v.notes.rows.push(note_model::Note {
            created_at: 77,
            updated_at: 78,
            text: "<div><br></div>".into(),
        });
        v.notes.select(77).unwrap();
        v.sync_note(cx);
    });
    settle(&rt, cx);
    let input = view.read_with(cx, |v, cx| v.editor.read(cx).input.clone());
    cx.update(|w, cx| {
        input.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(None, "にほん", None, w, cx)
        })
    });
    settle(&rt, cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.notes.document().unwrap().html()),
        "<div><br></div>"
    );
    cx.update(|w, cx| {
        input.update(cx, |input, cx| {
            input.replace_text_in_range(None, "日本", w, cx)
        })
    });
    settle(&rt, cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.notes.document().unwrap().html()),
        "<div>日本</div>"
    );
}
#[test]
fn review_current_build_contains_its_actual_nonempty_release_notes() {
    let notes = include_str!(concat!(env!("OUT_DIR"), "/release-notes.md"));
    assert!(!notes.trim().is_empty());
    assert!(notes.contains(env!("CARGO_PKG_VERSION")));
}

#[gpui::test]
fn review_monitor_formats_native_rows_and_notes_cache_summary_and_date(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let fixture=Arc::new(Fixture::default());
    *fixture.notes_html.lock().unwrap()=Some("<div>Hello</div><div>世界</div>".into());
    let (view,cx)=cx.add_window_view(|w,cx|Utilities::new(fixture,&UiState::default(),w,cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.navigate(nav(Surface::SystemMonitor, 1), w, cx)
        })
    });
    settle(&rt, cx);
    view.read_with(cx, |v, _| {
        let rows = v.monitor_rows();
        assert!(
            rows.iter()
                .any(|(_, label)| label.contains("CPU load: 25%"))
        );
        assert!(rows.iter().all(|(_, label)| !label.contains('{')));
        assert!(
            v.monitor_display
                .iter()
                .any(|r| r.id == "cpu.usagePercent" && r.percent == Some(25.))
        );
    });
    cx.update(|w, cx| view.update(cx, |v, cx| v.navigate(nav(Surface::Notes, 2), w, cx)));
    settle(&rt, cx);
    view.read_with(cx, |v, _| {
        let label = v.note_labels.get(&11).unwrap();
        assert!(label.title.contains("Hello"));
        assert!(label.summary.contains("世界"));
        assert!(!label.date.is_empty());
        assert_ne!(label.date, "—");
    });
}
#[gpui::test]
fn review_reopening_forms_releases_field_subscriptions(cx: &mut TestAppContext) {
    let rt = runtime(cx);
    let (view, cx) = cx.add_window_view(|w, cx| {
        Utilities::new(Arc::new(Fixture::default()), &UiState::default(), w, cx)
    });
    let baseline = view.read_with(cx, |v, _| v.subscriptions.len());
    for i in 0..12 {
        cx.update(|w, cx| {
            view.update(cx, |v, cx| {
                v.navigate(nav(Surface::Snippets, i + 1), w, cx);
                v.act(Action::New, w, cx);
            })
        });
        settle(&rt, cx);
        view.read_with(cx, |v, _| {
            assert!(!v.fields.is_empty());
            assert!(!v.field_subscriptions.is_empty());
            assert_eq!(v.subscriptions.len(), baseline);
        });
        cx.update(|w, cx| view.update(cx, |v, cx| v.act(Action::Cancel, w, cx)));
        settle(&rt, cx);
        view.read_with(cx, |v, _| {
            assert!(v.fields.is_empty());
            assert!(v.field_subscriptions.is_empty());
            assert_eq!(v.subscriptions.len(), baseline);
        });
    }
}
#[::core::prelude::v1::test]
fn review_png_base64_is_strict_and_labels_match_product_terms() {
    assert_eq!(
        clipboard::encode_png(&[1, 2, 3]),
        "data:image/png;base64,AQID"
    );
    assert_eq!(
        clipboard::decode_url("data:image/png;base64,AQID"),
        Some(vec![1, 2, 3])
    );
    assert!(clipboard::decode_url("data:image/png;base64,AAA!").is_none());
    assert_eq!(
        view::kind_label(tezbar_ui_model::vault::Kind::ApiKey),
        "API Key"
    );
    assert_eq!(
        view::kind_label(tezbar_ui_model::vault::Kind::SshKey),
        "SSH Key"
    );
}
