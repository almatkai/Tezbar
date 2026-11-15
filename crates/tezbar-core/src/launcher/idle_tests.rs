//! Synthetic idle-index replays. No OS watcher, personal profile or desktop services.
use super::*;
use crate::common::CorePaths;
use notify::{event::ModifyKind, Event, EventKind};
use std::time::{Duration, Instant, UNIX_EPOCH};

fn profile(root: &std::path::Path) -> CoreContext {
    CoreContext::headless(CorePaths {
        data_dir: root.join("data"),
        config_dir: root.join("config"),
        resource_dir: root.join("resources"),
        temp_dir: root.join("temp"),
        home_dir: root.join("home"),
    })
}

async fn attempts(db: &db::Database) -> usize {
    db.call(|_| Ok(json!(db::UPSERT_ATTEMPTS.with(|n| n.replace(0)))))
        .await
        .unwrap()
        .as_u64()
        .unwrap() as usize
}

fn fixture(path: &std::path::Path, seconds: u64) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "synthetic").unwrap();
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(seconds))
        .unwrap();
}

#[tokio::test]
async fn repeated_reconciliation_skips_unchanged_file_upserts() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = profile(dir.path());
    let root = ctx.paths.home_dir.join("Desktop");
    for n in 0..801 {
        fixture(&root.join(format!("fixture{n:04}.rs")), 1_700_000_000);
    }
    let db = db::Database::new(ctx.paths.data_dir.join("search/index.sqlite3"));
    let stop = Arc::new(AtomicBool::new(false));
    providers::scan(ctx.clone(), db.clone(), stop.clone())
        .await
        .unwrap();
    assert_eq!(
        attempts(&db).await,
        801,
        "cross the 400-document batch boundary twice"
    );
    let before = db.call(|c| Ok(json!(c.total_changes()))).await.unwrap();
    let started = Instant::now();
    for _ in 0..3 {
        // This is the same scan -> batching -> Database::upsert path used after overflow.
        providers::scan(ctx.clone(), db.clone(), stop.clone())
            .await
            .unwrap();
    }
    let count = attempts(&db).await;
    let after = db.call(|c| Ok(json!(c.total_changes()))).await.unwrap();
    println!("unchanged reconciliation: 801 files x 3, {count} document SQL attempts, {:?}, total_changes {before}->{after}", started.elapsed());
    db.shutdown().await.unwrap();
    assert_eq!(
        before, after,
        "unchanged documents and FTS must not be rewritten"
    );
    assert_eq!(
        count, 0,
        "unchanged files must not rebuild/execute per-document UPSERTs"
    );
}

#[tokio::test]
async fn ignored_watcher_burst_does_not_trigger_reconciliation() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = profile(dir.path());
    let root = ctx.paths.home_dir.join("Desktop");
    fixture(&root.join("kept.rs"), 1_700_000_000);
    let db = db::Database::new(ctx.paths.data_dir.join("search/index.sqlite3"));
    let stop = Arc::new(AtomicBool::new(false));
    providers::scan(ctx.clone(), db.clone(), stop.clone())
        .await
        .unwrap();
    attempts(&db).await;
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let overflow = AtomicBool::new(false);
    for n in 0..1024 {
        providers::queue_watch_event(
            Ok(Event::new(EventKind::Modify(ModifyKind::Any))
                .add_path(root.join(format!("project/target/debug/generated{n}.rs")))),
            &tx,
            &overflow,
        );
    }
    let rescan = overflow.swap(false, Ordering::AcqRel);
    if rescan {
        // Replay the production overflow branch, not just a path predicate.
        providers::scan(ctx, db.clone(), stop).await.unwrap();
    }
    let count = attempts(&db).await;
    let queued = rx.try_recv().is_ok();
    println!(
        "ignored watcher burst: rescan={rescan}, queued={queued}, document SQL attempts={count}"
    );
    db.shutdown().await.unwrap();
    assert!(
        !rescan,
        "ignored build output must not overflow into a full file scan"
    );
    assert!(
        !queued,
        "ignored paths must be filtered before the bounded queue"
    );
    assert_eq!(count, 0);
}

#[test]
fn watcher_filter_preserves_rename_delete_errors_and_loss_signals() {
    use notify::event::{AccessKind, Flag, RemoveKind, RenameMode};
    let root = PathBuf::from("synthetic-home").join("Desktop");
    let (tx, mut rx) = tokio::sync::mpsc::channel(2);
    let overflow = AtomicBool::new(false);
    let ignored = root.join("project/target/old.rs");
    let visible = root.join("visible.rs");
    let rename = EventKind::Modify(ModifyKind::Name(RenameMode::Both));
    providers::queue_watch_event(
        Ok(Event::new(rename)
            .add_path(ignored.clone())
            .add_path(visible.clone())),
        &tx,
        &overflow,
    );
    let event = rx.try_recv().unwrap().unwrap();
    assert_eq!(event.kind, rename);
    assert_eq!(event.paths, vec![visible.clone()]);
    // The other rename direction must keep the old visible path for deletion.
    providers::queue_watch_event(
        Ok(Event::new(rename)
            .add_path(visible.clone())
            .add_path(ignored.clone())),
        &tx,
        &overflow,
    );
    assert_eq!(rx.try_recv().unwrap().unwrap().paths, vec![visible.clone()]);
    // An unsupported extension may be a deleted directory containing indexed files.
    let deleted_dir = root.join("old.bundle");
    providers::queue_watch_event(
        Ok(Event::new(EventKind::Remove(RemoveKind::Any)).add_path(deleted_dir.clone())),
        &tx,
        &overflow,
    );
    assert_eq!(rx.try_recv().unwrap().unwrap().paths, vec![deleted_dir]);
    providers::queue_watch_event(
        Ok(Event::new(EventKind::Access(AccessKind::Any)).add_path(visible.clone())),
        &tx,
        &overflow,
    );
    assert!(rx.try_recv().is_err());
    assert!(!overflow.load(Ordering::Acquire));
    // notify loss signals take priority over ignored paths and even Access kinds.
    providers::queue_watch_event(
        Ok(Event::new(EventKind::Access(AccessKind::Any))
            .add_path(ignored)
            .set_flag(Flag::Rescan)),
        &tx,
        &overflow,
    );
    assert!(overflow.swap(false, Ordering::AcqRel));
    providers::queue_watch_event(
        Err(notify::Error::generic("synthetic watcher failure")),
        &tx,
        &overflow,
    );
    assert!(overflow.swap(false, Ordering::AcqRel));
    for _ in 0..3 {
        providers::queue_watch_event(
            Ok(Event::new(EventKind::Any).add_path(visible.clone())),
            &tx,
            &overflow,
        );
    }
    assert!(
        overflow.load(Ordering::Acquire),
        "real queue overflow still requires reconciliation"
    );
}

#[tokio::test]
async fn incremental_reconciliation_keeps_edits_renames_deletions_and_source_changes_fresh() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = profile(dir.path());
    let root = ctx.paths.home_dir.join("Desktop");
    for name in [
        "unchanged.rs",
        "edited.rs",
        "removed.rs",
        "oldfolder/renamed.rs",
        "target/hidden.rs",
    ] {
        fixture(&root.join(name), 1_700_000_000);
    }
    let db = db::Database::new(ctx.paths.data_dir.join("search/index.sqlite3"));
    let stop = Arc::new(AtomicBool::new(false));
    providers::scan(ctx.clone(), db.clone(), stop.clone())
        .await
        .unwrap();
    assert_eq!(attempts(&db).await, 4);
    let unchanged = format!("file:{}", root.join("unchanged.rs").display());
    let id = unchanged.clone();
    let row_before = db
        .call(move |c| {
            c.query_row(
                "SELECT rowid,updated_at FROM documents WHERE id=?",
                [id],
                |r| Ok(json!([r.get::<_, i64>(0)?, r.get::<_, i64>(1)?])),
            )
            .map_err(db::err)
        })
        .await
        .unwrap();
    fixture(&root.join("edited.rs"), 1_700_000_001);
    fixture(&root.join("added.json"), 1_700_000_000);
    std::fs::remove_file(root.join("removed.rs")).unwrap();
    std::fs::rename(root.join("oldfolder"), root.join("newfolder")).unwrap();
    std::fs::rename(root.join("target/hidden.rs"), root.join("visible.rs")).unwrap();
    // Force actual bounded-queue loss, then execute the production reconciliation.
    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let overflow = AtomicBool::new(false);
    for _ in 0..2 {
        providers::queue_watch_event(
            Ok(Event::new(EventKind::Any).add_path(root.clone())),
            &tx,
            &overflow,
        );
    }
    assert!(overflow.swap(false, Ordering::AcqRel));
    providers::scan(ctx.clone(), db.clone(), stop.clone())
        .await
        .unwrap();
    assert_eq!(attempts(&db).await, 4);
    let id = unchanged.clone();
    let row_after = db
        .call(move |c| {
            c.query_row(
                "SELECT rowid,updated_at FROM documents WHERE id=?",
                [id],
                |r| Ok(json!([r.get::<_, i64>(0)?, r.get::<_, i64>(1)?])),
            )
            .map_err(db::err)
        })
        .await
        .unwrap();
    assert_eq!(row_before, row_after);
    for query in ["oldfolder", "removed", "hidden"] {
        assert!(
            db.search(query.into(), true)
                .await
                .unwrap()
                .as_array()
                .unwrap()
                .is_empty(),
            "stale query {query}"
        );
    }
    assert!(db
        .search("newfolder".into(), true)
        .await
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["id"] == format!("file:{}", root.join("newfolder/renamed.rs").display())));
    assert!(db
        .search("visible".into(), true)
        .await
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["title"] == "visible.rs"));
    // A same-mtime file must not be skipped when its category/source identity
    // has changed in the database since the previous reconciliation.
    for sql in [
        "UPDATE documents SET category='other' WHERE id=?",
        "UPDATE documents SET source_path='synthetic-other-source' WHERE id=?",
        "UPDATE documents SET source_mtime=1 WHERE id=?",
        "UPDATE documents SET updated_at=1 WHERE id=?",
    ] {
        let id = unchanged.clone();
        db.call(move |c| {
            c.execute(sql, [id]).map_err(db::err)?;
            Ok(Value::Null)
        })
        .await
        .unwrap();
        providers::scan(ctx.clone(), db.clone(), stop.clone())
            .await
            .unwrap();
        assert_eq!(attempts(&db).await, 1, "{sql}");
    }
    db.call(|c| {
        c.execute_batch(
            "INSERT INTO documents_fts(documents_fts,rank) VALUES('integrity-check',1)",
        )
        .map_err(db::err)?;
        Ok(Value::Null)
    })
    .await
    .unwrap();
    db.shutdown().await.unwrap();
}

#[tokio::test]
async fn terminal_idle_prune_does_not_write_synthetic_profile() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = profile(dir.path());
    let terminal = terminal::TerminalCore::new(ctx.clone());
    terminal.tick().await.unwrap();
    assert!(
        !ctx.config.path().exists(),
        "idle prune must not create a default profile"
    );
    std::fs::create_dir_all(&ctx.paths.config_dir).unwrap();
    let original = b"{\"terminalSessions\":[{\"sessionId\":\"fixture\",\"status\":\"exited\",\"saveUntil\":18446744073709551615},{\"sessionId\":\"running\",\"status\":\"running\",\"saveUntil\":1},{\"sessionId\":\"forever\",\"status\":\"exited\"}],\"future\":7}\n";
    std::fs::write(ctx.config.path(), original).unwrap();
    let modified = std::fs::metadata(ctx.config.path())
        .unwrap()
        .modified()
        .unwrap();
    for _ in 0..3 {
        terminal.tick().await.unwrap();
    }
    assert_eq!(std::fs::read(ctx.config.path()).unwrap(), original);
    assert_eq!(
        std::fs::metadata(ctx.config.path())
            .unwrap()
            .modified()
            .unwrap(),
        modified
    );
}
