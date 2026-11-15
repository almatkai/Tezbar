use super::*;
use ::core::prelude::v1::test;
use std::sync::Mutex;
#[derive(Default)]
struct Fake(Mutex<Vec<&'static str>>);
impl ExtensionTransport for Fake {
    fn send(&self, r: ExtensionRequest) -> ExtensionFuture {
        let (c, _) = r.wire();
        self.0.lock().unwrap().push(c);
        Box::pin(async move {
            Ok(if c == "extension:list" {
                json!([])
            } else {
                json!({"items":[{"id":"fixture","name":"Fixture","runtime":"javascript"}],"total":1,"hasMore":false})
            })
        })
    }
    fn drain(&self) -> ExtensionFuture {
        Box::pin(async { Ok(json!(true)) })
    }
}
#[gpui::test]
fn construct_and_render_do_not_start_store_requests(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    let (view, cx) = cx.add_window_view(|w, cx| StoreView::new(fake.clone(), w, cx));
    cx.run_until_parked();
    assert!(fake.0.lock().unwrap().is_empty());
    cx.update(|_, cx| view.update(cx, |v, cx| v.dispose(cx)));
    assert!(fake.0.lock().unwrap().is_empty());
}
#[test]
fn installed_wire_normalizes_and_invalid_data_is_rejected() {
    assert!(decode_installed(json!({"ok":false})).is_err());
    let rows =
        decode_installed(json!([{"id":"fixture","name":"Fixture","iconPath":"assets/icon.png"}]))
            .unwrap();
    assert_eq!(rows[0].author, "Raycast Community");
    assert_eq!(rows[0].metadata["iconUrl"], "assets/icon.png");
}
#[test]
fn wire_payload_debug_is_redacted_without_truncating_copy() {
    let text = "x".repeat(10000);
    let r = ExtensionRequest::Copy(text.clone());
    assert!(!format!("{r:?}").contains("xxxxx"));
    assert_eq!(r.wire().1[0], text);
    assert!(
        success(&json!({"ok":false,"message":"synthetic secret"}))
            .unwrap_err()
            .find("synthetic secret")
            .is_none()
    );
}
#[gpui::test]
fn remove_confirmation_defaults_to_cancel_and_traps_tab(cx: &mut TestAppContext) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| {
        gpui_tokio::init_from_handle(cx, rt.handle().clone());
        tezbar_ui::text_input::bind_keys(cx);
    });
    let fake = Arc::new(Fake::default());
    let (view, cx) = cx.add_window_view(|w, cx| StoreView::new(fake.clone(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.action(view::Action::Remove("fixture".into()), w, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|w, cx| assert!(view.read(cx).cancel_focus.is_focused(w)));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.read(|cx| assert!(view.read(cx).remove.is_none()));
    assert!(fake.0.lock().unwrap().is_empty());
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.action(view::Action::Remove("fixture".into()), w, cx)
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("shift-tab");
    cx.update(|w, cx| assert!(view.read(cx).confirm_focus.is_focused(w)));
    cx.simulate_keystrokes("tab");
    cx.update(|w, cx| assert!(view.read(cx).cancel_focus.is_focused(w)));
    cx.simulate_keystrokes("escape");
    assert!(fake.0.lock().unwrap().is_empty());
}

struct RunReply(std::sync::Mutex<Vec<(&'static str, Vec<Value>)>>);
impl ExtensionTransport for RunReply {
    fn send(&self, r: ExtensionRequest) -> ExtensionFuture {
        let (channel, args) = r.wire();
        self.0.lock().unwrap().push((channel, args));
        Box::pin(async move {
            Ok(if channel == "extension:run-command" {
                json!({"ok":true,"mode":"view","sessionId":"late-store-session","root":{"type":"List"}})
            } else {
                json!(true)
            })
        })
    }
    fn drain(&self) -> ExtensionFuture {
        Box::pin(async { Ok(json!(true)) })
    }
}
#[gpui::test]
fn review_m9_store_disposes_run_reply_after_hiding(cx: &mut TestAppContext) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| gpui_tokio::init_from_handle(cx, rt.handle().clone()));
    let fake = Arc::new(RunReply(Default::default()));
    let (view, cx) = cx.add_window_view(|w, cx| StoreView::new(fake.clone(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.visible = true;
            v.run("fixture".into(), "run".into(), cx);
            v.set_visible(false, w, cx);
        })
    });
    rt.block_on(async {
        for _ in 0..128 {
            cx.run_until_parked();
            tokio::task::yield_now().await;
        }
    });
    assert!(
        fake.0
            .lock()
            .unwrap()
            .iter()
            .any(|(channel, args)| *channel == "extension:dispose-session"
                && args[0]["sessionId"] == "late-store-session")
    );
}

struct PagingTransport {
    searches: Mutex<Vec<(usize, usize)>>,
    replies: Mutex<std::collections::VecDeque<Result<Value, String>>>,
}
impl ExtensionTransport for PagingTransport {
    fn send(&self, request: ExtensionRequest) -> ExtensionFuture {
        let reply = match request {
            ExtensionRequest::Search { offset, limit, .. } => {
                self.searches.lock().unwrap().push((offset, limit));
                self.replies
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("unexpected page")
            }
            ExtensionRequest::List => Ok(json!([])),
            _ => panic!("unexpected request"),
        };
        Box::pin(async move { reply })
    }
    fn drain(&self) -> ExtensionFuture {
        Box::pin(async { Ok(json!(true)) })
    }
}

#[gpui::test]
fn virtual_range_paging_guards_failure_retry_and_append(cx: &mut TestAppContext) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| gpui_tokio::init_from_handle(cx, rt.handle().clone()));
    let fake = Arc::new(PagingTransport {
        searches: Default::default(),
        replies: Mutex::new(
            [
                Err("offline".into()),
                Ok(json!({"items":[{"id":"next","name":"Next"}],"total":100,"hasMore":true})),
                Ok(json!({"items":[],"total":100,"hasMore":true})),
            ]
            .into(),
        ),
    });
    let (view, cx) = cx.add_window_view(|w, cx| StoreView::new(fake.clone(), w, cx));
    view.update(cx, |v, cx| {
        v.catalog.search("".into(), false);
        let items = (0..25)
            .map(|i| {
                serde_json::from_value(json!({"id":i.to_string(),"name":i.to_string()})).unwrap()
            })
            .collect();
        v.catalog.loaded(1, Ok((items, vec![], 100, true)), false);
        v.catalog.selected = Some("4".into());
        v.rebuild(cx);
        v.visible_range(21..25, cx); // hidden
        assert!(!v.catalog.loading_more);
        v.visible = true;
        v.disposed = true;
        v.visible_range(21..25, cx);
        assert!(!v.catalog.loading_more);
        v.disposed = false;
        v.remove = Some("4".into());
        v.visible_range(21..25, cx);
        assert!(!v.catalog.loading_more);
        v.remove = None;
        v.visible_range(0..1, cx); // uniform_list's measurement pass
        assert!(!v.catalog.loading_more);
        v.visible_range(21..25, cx);
        v.visible_range(21..25, cx);
        assert!(v.catalog.loading_more);
    });
    rt.block_on(async {
        for _ in 0..64 {
            cx.run_until_parked();
            tokio::task::yield_now().await;
        }
    });
    view.update(cx, |v, cx| {
        assert!(v.catalog.pagination_failed);
        for _ in 0..3 {
            v.visible_range(21..25, cx);
        }
        assert!(!v.catalog.loading_more);
    });
    assert_eq!(*fake.searches.lock().unwrap(), vec![(25, 25)]);
    cx.update(|w, cx| view.update(cx, |v, cx| v.action(view::Action::More, w, cx)));
    rt.block_on(async {
        for _ in 0..64 {
            cx.run_until_parked();
            tokio::task::yield_now().await;
        }
    });
    view.update(cx, |v, cx| {
        assert_eq!(v.rows.len(), 26);
        assert_eq!(v.catalog.selected.as_deref(), Some("4"));
        assert!(!v.catalog.pagination_failed);
        v.visible_range(23..26, cx);
    });
    rt.block_on(async {
        for _ in 0..64 {
            cx.run_until_parked();
            tokio::task::yield_now().await;
        }
    });
    view.update(cx, |v, cx| {
        assert!(!v.catalog.has_more);
        v.visible_range(23..26, cx);
        v.more(cx);
    });
    assert_eq!(
        *fake.searches.lock().unwrap(),
        vec![(25, 25), (25, 25), (26, 25)]
    );
}

#[gpui::test]
fn scrolling_the_rendered_list_requests_a_page_without_moving_selection(cx: &mut TestAppContext) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| gpui_tokio::init_from_handle(cx, rt.handle().clone()));
    let fake = Arc::new(PagingTransport {
        searches: Default::default(),
        replies: Mutex::new(
            [Ok(
                json!({"items":[{"id":"next","name":"Next"}],"total":26,"hasMore":false}),
            )]
            .into(),
        ),
    });
    let (view, cx) = cx.add_window_view(|w, cx| StoreView::new(fake.clone(), w, cx));
    view.update(cx, |v, cx| {
        v.catalog.search("".into(), false);
        let items = (0..25)
            .map(|i| {
                serde_json::from_value(json!({"id":i.to_string(),"name":i.to_string()})).unwrap()
            })
            .collect();
        v.catalog.loaded(1, Ok((items, vec![], 26, true)), false);
        v.visible = true;
        v.catalog.selected = Some("4".into());
        v.rebuild(cx);
        cx.notify();
    });
    cx.update(|w, cx| {
        w.draw(cx).clear(cx);
        w.simulate_next_frame(cx);
    });
    assert!(fake.searches.lock().unwrap().is_empty());
    view.update(cx, |v, cx| {
        v.scroll.scroll_to_item(24, ScrollStrategy::Nearest);
        cx.notify();
    });
    cx.update(|w, cx| {
        w.draw(cx).clear(cx);
        w.simulate_next_frame(cx);
    });
    let before = view.read_with(cx, |v, _| v.scroll.logical_scroll_top_index());
    rt.block_on(async {
        for _ in 0..64 {
            cx.run_until_parked();
            tokio::task::yield_now().await;
        }
    });
    cx.update(|w, cx| {
        w.draw(cx).clear(cx);
        w.simulate_next_frame(cx);
    });
    assert_eq!(*fake.searches.lock().unwrap(), vec![(25, 25)]);
    view.read_with(cx, |v, _| {
        assert_eq!(v.rows.len(), 26);
        assert_eq!(v.catalog.selected.as_deref(), Some("4"));
        assert_eq!(v.scroll.logical_scroll_top_index(), before);
    });
}

#[gpui::test]
fn deferred_page_is_invalidated_by_query_and_hide_and_keyboard_still_pages(
    cx: &mut TestAppContext,
) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| gpui_tokio::init_from_handle(cx, rt.handle().clone()));
    let fake = Arc::new(PagingTransport {
        searches: Default::default(),
        replies: Mutex::new([Ok(json!({"items":[],"total":25,"hasMore":false}))].into()),
    });
    let (view, cx) = cx.add_window_view(|w, cx| StoreView::new(fake.clone(), w, cx));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            let items = || {
                (0..25)
                    .map(|i| {
                        serde_json::from_value(json!({"id":i.to_string(),"name":i.to_string()}))
                            .unwrap()
                    })
                    .collect()
            };
            v.visible = true;
            v.catalog.search("old".into(), false);
            v.catalog.loaded(1, Ok((items(), vec![], 100, true)), false);
            v.rebuild(cx);
            v.schedule_visible_range(21..25, w, cx);
            v.schedule_visible_range(21..25, w, cx);
            assert!(!v.catalog.loading_more); // admission waits until the next frame
            v.query("new".into(), cx);
            v.debounce = None;
            v.catalog.loaded(2, Ok((items(), vec![], 100, true)), false);
            v.rebuild(cx);
        })
    });
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
    });
    view.read_with(cx, |v, _| assert!(!v.catalog.loading_more));
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.schedule_visible_range(21..25, w, cx);
            v.set_visible(false, w, cx);
        })
    });
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
    });
    view.read_with(cx, |v, _| assert!(!v.catalog.loading_more));
    assert!(fake.searches.lock().unwrap().is_empty());
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            v.visible = true;
            v.catalog.selected = Some("24".into());
            v.search.focus_handle(cx).focus(w, cx);
        })
    });
    cx.simulate_keystrokes("down down");
    rt.block_on(async {
        for _ in 0..64 {
            cx.run_until_parked();
            tokio::task::yield_now().await;
        }
    });
    assert_eq!(*fake.searches.lock().unwrap(), vec![(25, 25)]);
}

#[gpui::test]
fn source_switch_clears_rows_and_rejects_old_page(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    let (view, cx) = cx.add_window_view(|w, cx| StoreView::new(fake, w, cx));
    view.update(cx, |v, cx| {
        let own: Entry =
            serde_json::from_value(json!({"id":"own", "name":"Own", "source":"tezbar"})).unwrap();
        let community: Entry = serde_json::from_value(
            json!({"id":"community", "name":"Community", "source":"raycast"}),
        )
        .unwrap();
        v.catalog.search("".into(), false);
        v.catalog.loaded(
            1,
            Ok((vec![own.clone()], vec![own, community], 1, false)),
            false,
        );
        v.rebuild(cx);
        assert_eq!(
            v.rows.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["own"]
        );
        let Effect::Search { token, .. } = v.catalog.search("".into(), false).unwrap() else {
            panic!()
        };
        v.switch_source(StoreSource::Raycast, cx);
        v.catalog
            .loaded(token, Ok((vec![], vec![], 999, true)), false);
        assert_eq!(v.catalog.total, 0, "stale source page must be ignored");
        assert_eq!(
            v.rows.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["community"]
        );
        assert!(!v.catalog.has_more);
        assert_eq!(v.source, StoreSource::Raycast);
        for source in [StoreSource::Tezbar, StoreSource::Raycast] {
            let (_, args) = ExtensionRequest::Search {
                query: "hello".into(),
                source,
                offset: 25,
                limit: 25,
            }
            .wire();
            assert_eq!(
                args[0],
                json!({"query":"hello", "source":source.id(), "offset":25, "limit":25})
            );
        }
    });
}
