use super::*;
use crate::native_core::common::CorePaths;

async fn corpus(n: usize) -> (KnowledgeCore, tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let docs = dir.path().join("docs");
    std::fs::create_dir(&docs).unwrap();
    for i in 0..n {
        std::fs::write(
            docs.join(format!("{i}.txt")),
            format!("fixture document {i}"),
        )
        .unwrap();
    }
    let core = KnowledgeCore::new(CoreContext::headless(CorePaths {
        data_dir: dir.path().join("data"),
        config_dir: dir.path().join("config"),
        resource_dir: dir.path().join("resources"),
        temp_dir: dir.path().join("tmp"),
        home_dir: dir.path().join("home"),
    }));
    let root = docs.clone();
    core.database()
        .await
        .unwrap()
        .call(move |s| {
            s.root_add(&root)?;
            let mut settings = s.settings()?;
            settings["indexingSpeed"] = json!("fast");
            settings["knowledgeDepth"] = json!("basic");
            s.db.execute("UPDATE knowledge_roots SET depth='basic'", [])
                .map_err(|e| e.to_string())?;
            s.set_meta("settings", &settings)
        })
        .await
        .unwrap();
    core.refresh().await.unwrap();
    core.run_index(&core.cancel()).await.unwrap();
    core.inner.cache.lock().unwrap().settings["indexingSpeed"] = json!("medium");
    core.inner.progress_events.store(0, Ordering::SeqCst);
    core.inner.pacing_delays.store(0, Ordering::SeqCst);
    (core, dir, docs)
}

#[tokio::test]
async fn unchanged_real_pass_coalesces_progress_without_extraction_delays() {
    let (core, _dir, _) = corpus(64).await;
    let q = probe(&core, 0).await;
    core.run_index(&core.cancel()).await.unwrap();
    let delays = core.inner.pacing_delays.load(Ordering::SeqCst);
    let events = core.inner.progress_events.load(Ordering::SeqCst);
    core.shutdown().await.unwrap();
    eprintln!("unchanged pass: durable completions={}, metadata writes={}, status events={events}, pacing calls={delays}",
        q.completed.load(Ordering::SeqCst), q.metadata_writes.load(Ordering::SeqCst));
    assert!(q.metadata_writes.load(Ordering::SeqCst) <= 4);
    assert_eq!(q.completed.load(Ordering::SeqCst), 64);
    assert_eq!(
        delays, 0,
        "unchanged files must not pay medium extraction pacing"
    );
    assert!(
        events <= 6,
        "bounded progress instead of per-file UI: {events}"
    );
}

#[test]
fn watcher_uses_scanner_exclusions_but_keeps_uncertain_and_removed_directories() {
    use notify::{
        event::{ModifyKind, RenameMode},
        Event, EventKind,
    };
    let roots = vec![PathBuf::from("/docs")];
    let event = |paths: &[&str]| {
        paths.iter().fold(
            Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both))),
            |e, p| e.add_path(PathBuf::from(p)),
        )
    };
    assert!(!watcher_reconcile(
        &event(&["/docs/.git/objects/x", "/docs/node_modules/pkg/x"]),
        &roots
    ));
    assert!(watcher_reconcile(
        &event(&["/docs/.git/x", "/docs/eligible"]),
        &roots
    ));
    assert!(watcher_reconcile(
        &event(&["/docs/removed-directory"]),
        &roots
    ));
    assert!(watcher_reconcile(&event(&[]), &roots));
    assert!(watcher_requests_reconcile(
        &Err(notify::Error::generic("fixture watcher error")),
        &roots
    ));
    let overflow = event(&["/docs/.git/x"]).set_flag(notify::event::Flag::Rescan);
    assert!(watcher_reconcile(&overflow, &roots));
    assert!(watcher_reconcile(
        &event(&["/docs/.git/../eligible"]),
        &roots
    ));
    let access_overflow = Event::new(EventKind::Access(notify::event::AccessKind::Any))
        .add_path(PathBuf::from("/docs/.git/x"))
        .set_flag(notify::event::Flag::Rescan);
    assert!(watcher_reconcile(&access_overflow, &roots));
    assert!(watcher_reconcile(&Event::new(EventKind::Any), &roots));
    assert!(watcher_reconcile(&event(&["/elsewhere/.git/x"]), &roots));
    assert!(watcher_reconcile(&event(&["/docs"]), &roots));
    // An explicitly selected excluded-looking root is scanned at depth zero.
    assert!(watcher_reconcile(
        &event(&["/docs/.git/config"]),
        &[PathBuf::from("/docs/.git")]
    ));
}

struct QueueProbe {
    completed: AtomicU64,
    metadata_writes: AtomicU64,
    cancel_at: u64,
    generation: Arc<AtomicU64>,
}
unsafe extern "C" fn queue_trace(
    kind: u32,
    context: *mut std::ffi::c_void,
    _: *mut std::ffi::c_void,
    sql: *mut std::ffi::c_void,
) -> i32 {
    if kind == rusqlite::ffi::SQLITE_TRACE_STMT as u32 && !sql.is_null() {
        let probe = unsafe { &*context.cast::<QueueProbe>() };
        let sql = unsafe { std::ffi::CStr::from_ptr(sql.cast()) }.to_string_lossy();
        if sql.starts_with("DELETE FROM knowledge_indexing_queue WHERE job_id=") {
            let n = probe.completed.fetch_add(1, Ordering::SeqCst) + 1;
            if n == probe.cancel_at {
                probe.generation.fetch_add(1, Ordering::SeqCst);
            }
        }
        if sql.starts_with("INSERT INTO knowledge_metadata(key,value_json)") {
            probe.metadata_writes.fetch_add(1, Ordering::SeqCst);
        }
    }
    0
}
async fn probe(core: &KnowledgeCore, cancel_at: u64) -> Arc<QueueProbe> {
    let probe = Arc::new(QueueProbe {
        completed: AtomicU64::new(0),
        metadata_writes: AtomicU64::new(0),
        cancel_at,
        generation: core.inner.generation.clone(),
    });
    let owner = probe.clone();
    core.database()
        .await
        .unwrap()
        .call(move |s| {
            s.trace_owner = Some(owner.clone());
            unsafe {
                rusqlite::ffi::sqlite3_trace_v2(
                    s.db.handle(),
                    rusqlite::ffi::SQLITE_TRACE_STMT as u32,
                    Some(queue_trace),
                    Arc::as_ptr(&owner).cast_mut().cast(),
                );
            }
            Ok(())
        })
        .await
        .unwrap();
    probe
}

#[tokio::test]
async fn mixed_concurrent_pass_cancels_durably_and_reopens_without_double_counts() {
    let (core, _dir, docs) = corpus(6).await;
    let ctx = core.inner.ctx.clone();
    std::fs::write(docs.join("1.txt"), "changed longer document").unwrap();
    std::fs::remove_file(docs.join("2.txt")).unwrap();
    let root = string(&core.roots()[0], "id").to_owned();
    let queue: Vec<_> = (0..6)
        .map(|i| {
            (
                root.clone(),
                docs.join(format!("{i}.txt")).to_string_lossy().into_owned(),
            )
        })
        .collect();
    core.database()
        .await
        .unwrap()
        .call(move |s| s.save_queue(&queue))
        .await
        .unwrap();
    core.inner.cache.lock().unwrap().settings["maxConcurrentExtractors"] = json!(3);
    let q = probe(&core, 3).await;
    assert!(core.run_index(&core.cancel()).await.is_err());
    let checkpoint = core
        .database()
        .await
        .unwrap()
        .call(|s| s.checkpoint())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(checkpoint["processedSources"], 3);
    assert_eq!(q.completed.load(Ordering::SeqCst), 3);
    let left = core
        .database()
        .await
        .unwrap()
        .call(|s| s.next_batch(8))
        .await
        .unwrap();
    assert_eq!(left.len(), 3);
    core.handle("knowledge:index:pause", &[])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(core.inner.cache.lock().unwrap().status["state"], "paused");
    assert_eq!(
        core.inner.cache.lock().unwrap().status["processedSources"],
        3
    );
    core.shutdown().await.unwrap();
    let reopened = KnowledgeCore::new(ctx);
    reopened.refresh().await.unwrap();
    reopened
        .handle("knowledge:index:resume", &[])
        .await
        .unwrap()
        .unwrap();
    reopened.run_index(&reopened.cancel()).await.unwrap();
    let status = reopened.inner.cache.lock().unwrap().status.clone();
    assert_eq!(status["state"], "completed");
    assert_eq!(status["progress"], 1);
    assert_eq!(status["processedSources"], 6);
    assert_eq!(status["failedSources"], 1);
    assert!(reopened
        .database()
        .await
        .unwrap()
        .call(|s| s.checkpoint())
        .await
        .unwrap()
        .is_none());
    let path = docs.join("1.txt").to_string_lossy().into_owned();
    let source = reopened
        .database()
        .await
        .unwrap()
        .call(move |s| s.source(&path))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(source["status"], "indexed");
    assert_eq!(source["byteSize"], 23);
    // Mutation after completing an unchanged pass remains eligible on rescan.
    std::fs::write(docs.join("0.txt"), "new content after resume").unwrap();
    reopened.run_index(&reopened.cancel()).await.unwrap();
    assert_eq!(
        reopened.inner.cache.lock().unwrap().status["processedSources"],
        5
    );
    reopened.shutdown().await.unwrap();
}

#[tokio::test]
async fn profile_mutation_reindexes_unchanged_metadata_and_changed_work_keeps_pacing() {
    let (core, _dir, docs) = corpus(2).await;
    let path = docs.join("0.txt").to_string_lossy().into_owned();
    let read = path.clone();
    let before = core
        .database()
        .await
        .unwrap()
        .call(move |s| s.source(&read))
        .await
        .unwrap()
        .unwrap();
    core.database()
        .await
        .unwrap()
        .call(|s| {
            s.db.execute("UPDATE knowledge_roots SET depth='smart'", [])
                .map_err(|e| e.to_string())?;
            Ok(())
        })
        .await
        .unwrap();
    core.refresh().await.unwrap();
    core.inner.cache.lock().unwrap().settings["indexingSpeed"] = json!("medium");
    core.run_index(&core.cancel()).await.unwrap();
    assert_eq!(core.inner.pacing_delays.load(Ordering::SeqCst), 2);
    let after = core
        .database()
        .await
        .unwrap()
        .call(move |s| s.source(&path))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(before["contentHash"], after["contentHash"]);
    assert_ne!(before["indexingProfile"], after["indexingProfile"]);
    assert_eq!(after["status"], "indexed");
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn quiescent_observers_sleep_and_shutdown_wakes_them() {
    let dir = tempfile::tempdir().unwrap();
    let core = KnowledgeCore::new(CoreContext::headless(CorePaths {
        data_dir: dir.path().join("data"),
        config_dir: dir.path().join("config"),
        resource_dir: dir.path().join("resources"),
        temp_dir: dir.path().join("tmp"),
        home_dir: dir.path().join("home"),
    }));
    core.start().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let before = core
        .inner
        .observer_ticks
        .each_ref()
        .map(|n| n.load(Ordering::Relaxed));
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    let after = core
        .inner
        .observer_ticks
        .each_ref()
        .map(|n| n.load(Ordering::Relaxed));
    let start = std::time::Instant::now();
    core.shutdown().await.unwrap();
    assert!(
        start.elapsed() < std::time::Duration::from_millis(200),
        "shutdown must wake a sleeping observer"
    );
    assert_eq!(
        after, before,
        "unchanged state must not clone/probe every 250 ms"
    );
}

#[tokio::test]
async fn runtime_changes_wake_both_observers_but_identical_status_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let core = KnowledgeCore::new(CoreContext::headless(CorePaths {
        data_dir: dir.path().join("data"),
        config_dir: dir.path().join("config"),
        resource_dir: dir.path().join("resources"),
        temp_dir: dir.path().join("tmp"),
        home_dir: dir.path().join("home"),
    }));
    core.start().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let before = core
        .inner
        .observer_ticks
        .each_ref()
        .map(|n| n.load(Ordering::Relaxed));
    let runtime = core.selected_runtime();
    runtime.patch(json!({"error":"observer fixture"}));
    tokio::time::timeout(std::time::Duration::from_millis(200), async {
        while core
            .inner
            .observer_ticks
            .iter()
            .zip(before)
            .any(|(n, b)| n.load(Ordering::Relaxed) <= b)
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("a runtime transition must wake both independent observers");
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let before = core
        .inner
        .observer_ticks
        .each_ref()
        .map(|n| n.load(Ordering::Relaxed));
    runtime.patch(json!({"error":"observer fixture"}));
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let after = core
        .inner
        .observer_ticks
        .each_ref()
        .map(|n| n.load(Ordering::Relaxed));
    core.shutdown().await.unwrap();
    assert_eq!(
        after, before,
        "identical runtime status must not create a feedback loop"
    );
}
