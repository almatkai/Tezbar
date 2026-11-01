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
    // Includes save_queue's transaction; no source/content writes on this pass.
    assert!(
        q.commits.load(Ordering::SeqCst) <= 3,
        "unchanged completions must be batched"
    );
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
    commits: AtomicU64,
    queue_saves: AtomicU64,
    source_reads: AtomicU64,
    extractions: AtomicU64,
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
        if sql == "COMMIT" {
            probe.commits.fetch_add(1, Ordering::SeqCst);
        }
        if sql.starts_with("INSERT INTO knowledge_indexing_jobs") {
            probe.queue_saves.fetch_add(1, Ordering::SeqCst);
        }
        if sql.starts_with("SELECT id,content_hash,byte_size,modified_at,status,indexing_profile") {
            probe.source_reads.fetch_add(1, Ordering::SeqCst);
        }
        if sql.starts_with("INSERT INTO knowledge_sources(id,root_id,path,content_hash") {
            probe.extractions.fetch_add(1, Ordering::SeqCst);
        }
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
        commits: AtomicU64::new(0),
        queue_saves: AtomicU64::new(0),
        source_reads: AtomicU64::new(0),
        extractions: AtomicU64::new(0),
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
    let (core, _dir, docs) = corpus(160).await;
    let ctx = core.inner.ctx.clone();
    std::fs::write(docs.join("1.txt"), "changed longer document").unwrap();
    std::fs::remove_file(docs.join("2.txt")).unwrap();
    let root = string(&core.roots()[0], "id").to_owned();
    let queue: Vec<_> = (0..160)
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
    let processed = number(&checkpoint, "processedSources");
    assert!((3..160).contains(&processed));
    assert_eq!(q.completed.load(Ordering::SeqCst), processed as u64);
    let left = core
        .database()
        .await
        .unwrap()
        .call(|s| s.next_batch(200))
        .await
        .unwrap();
    assert_eq!(left.len(), (160 - processed) as usize);
    core.handle("knowledge:index:pause", &[])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(core.inner.cache.lock().unwrap().status["state"], "paused");
    assert_eq!(
        core.inner.cache.lock().unwrap().status["processedSources"],
        processed
    );
    core.shutdown().await.unwrap();
    let reopened = KnowledgeCore::new(ctx);
    reopened.refresh().await.unwrap();
    // Resume the durable partial batch in a new service instance. The original
    // queue contains more than a batch, so cancellation cannot drain the job.
    reopened.run_index(&reopened.cancel()).await.unwrap();
    let status = reopened.inner.cache.lock().unwrap().status.clone();
    assert_eq!(status["state"], "completed");
    assert_eq!(status["progress"], 1);
    assert_eq!(status["processedSources"], 160);
    assert_eq!(
        status["failedSources"], 0,
        "ENOENT removes sources without recording failures"
    );
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
        159
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

#[tokio::test]
async fn unselected_download_keeps_progress_observation_active_then_returns_to_sleep() {
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
    let runtime = core
        .inner
        .runtimes
        .iter()
        .find(|r| r.variant_id() != core.selected_variant())
        .unwrap();
    let mut observations = Vec::new();
    for paused in [false, true] {
        core.inner.paused.store(paused, Ordering::SeqCst);
        let before = core.inner.observer_ticks[1].load(Ordering::Relaxed);
        // Downloads are independent of indexing pause/selection. Byte progress
        // changes under the status mutex between lifecycle notifications.
        runtime.patch(json!({"modelState":"downloading"}));
        tokio::time::sleep(std::time::Duration::from_millis(650)).await;
        let active = core.inner.observer_ticks[1].load(Ordering::Relaxed);
        runtime.patch(json!({"modelState":"not-downloaded"}));
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let settled = core.inner.observer_ticks[1].load(Ordering::Relaxed);
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        observations.push((
            before,
            active,
            settled,
            core.inner.observer_ticks[1].load(Ordering::Relaxed),
        ));
    }
    core.shutdown().await.unwrap();
    for (before, active, settled, after) in observations {
        assert!(
            active >= before + 3,
            "active download must retain 250ms progress observations"
        );
        assert_eq!(
            after, settled,
            "finishing download must restore quiescent sleep"
        );
    }
}

#[tokio::test]
async fn watcher_single_file_update_is_incremental_and_preserves_job_queue() {
    let (core, _dir, docs) = corpus(8).await;
    let q = probe(&core, 0).await;
    let changed = docs.join("0.txt");
    std::fs::write(&changed, "modified single document").unwrap();
    let event = Ok(
        notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any))
            .add_path(changed.clone()),
    );
    let roots = vec![docs.clone()];
    let mut pending = PendingReconcile::default();
    assert!(pending.record(&event, &roots, &HashSet::new(), 100));
    assert!(pending.record(&event, &roots, &HashSet::new(), 200));
    assert!(pending.take_due(5199, 0).is_none());
    let work = pending.take_due(5200, 0).unwrap();
    assert!(!work.full);
    assert_eq!(work.paths.len(), 1);
    core.run_incremental(&work.paths, &core.cancel())
        .await
        .unwrap();
    assert_eq!(q.queue_saves.load(Ordering::SeqCst), 0);
    assert_eq!(q.completed.load(Ordering::SeqCst), 0);
    assert_eq!(
        q.source_reads.load(Ordering::SeqCst),
        1,
        "only the changed file is checked"
    );
    assert_eq!(q.extractions.load(Ordering::SeqCst), 1);
    let path = changed.to_string_lossy().into_owned();
    let source = core
        .database()
        .await
        .unwrap()
        .call(move |s| s.source(&path))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(source["byteSize"], 24);
    assert!(core
        .database()
        .await
        .unwrap()
        .call(|s| s.checkpoint())
        .await
        .unwrap()
        .is_none());
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn incremental_rename_and_removed_directory_prune_only_their_children() {
    let (core, _dir, docs) = corpus(1).await;
    let old = docs.join("folder");
    let new = docs.join("renamed");
    std::fs::create_dir(&old).unwrap();
    std::fs::write(old.join("child.txt"), "child document").unwrap();
    core.run_incremental(&HashSet::from([old.clone()]), &core.cancel())
        .await
        .unwrap();
    std::fs::rename(&old, &new).unwrap();
    core.run_incremental(&HashSet::from([old.clone(), new.clone()]), &core.cancel())
        .await
        .unwrap();
    let old_child = old.join("child.txt").to_string_lossy().into_owned();
    let new_child = new.join("child.txt").to_string_lossy().into_owned();
    core.database()
        .await
        .unwrap()
        .call(move |s| {
            assert!(s.source(&old_child)?.is_none());
            assert!(s.source(&new_child)?.is_some());
            Ok(())
        })
        .await
        .unwrap();
    std::fs::remove_dir_all(&new).unwrap();
    core.run_incremental(&HashSet::from([new]), &core.cancel())
        .await
        .unwrap();
    assert_eq!(
        core.database()
            .await
            .unwrap()
            .call(|s| s.counts())
            .await
            .unwrap()["sourceCount"],
        1
    );
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn unchanged_failed_source_does_not_hash_or_extract_until_change_or_transient_retry() {
    let (core, _dir, docs) = corpus(0).await;
    let file = docs.join("binary.txt");
    std::fs::write(&file, b"binary\0data").unwrap();
    let path = file.to_string_lossy().into_owned();
    core.run_index(&core.cancel()).await.unwrap();
    let q = probe(&core, 0).await;
    core.run_index(&core.cancel()).await.unwrap();
    assert_eq!(q.source_reads.load(Ordering::SeqCst), 1);
    assert_eq!(q.extractions.load(Ordering::SeqCst), 0);
    let root = core.roots()[0].clone();
    let semaphore = || Arc::new(tokio::sync::Semaphore::new(1));
    assert_eq!(
        core.index_one(&root, &path, &core.cancel(), semaphore())
            .await
            .unwrap(),
        IndexOutcome::Unchanged
    );
    let id = text::hash(&path);
    core.database()
        .await
        .unwrap()
        .call(move |s| s.failed(&id, "File changed during indexing; queued for rescan"))
        .await
        .unwrap();
    assert!(core
        .index_one(&root, &path, &core.cancel(), semaphore())
        .await
        .unwrap_err()
        .contains("binary"));
    assert_eq!(q.extractions.load(Ordering::SeqCst), 1);
    let id = text::hash(&path);
    core.database()
        .await
        .unwrap()
        .call(move |s| s.failed(&id, "File appears to contain binary data"))
        .await
        .unwrap();
    let mut changed_profile = root.clone();
    changed_profile["depth"] = json!("smart");
    assert!(core
        .index_one(&changed_profile, &path, &core.cancel(), semaphore())
        .await
        .unwrap_err()
        .contains("binary"));
    assert_eq!(
        q.extractions.load(Ordering::SeqCst),
        2,
        "a profile change retries the failed source"
    );
    std::fs::write(&file, "now normal user text").unwrap();
    assert_eq!(
        core.index_one(&root, &path, &core.cancel(), semaphore())
            .await
            .unwrap(),
        IndexOutcome::Indexed
    );
    std::fs::remove_file(&file).unwrap();
    assert_eq!(
        core.index_one(&root, &path, &core.cancel(), semaphore())
            .await
            .unwrap(),
        IndexOutcome::Unchanged
    );
    assert!(core
        .database()
        .await
        .unwrap()
        .call(move |s| s.source(&path))
        .await
        .unwrap()
        .is_none());
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn chrome_profiles_are_skipped_cached_and_previously_indexed_sources_pruned() {
    let (core, _dir, docs) = corpus(1).await;
    let profile = docs.join("agent-chrome");
    std::fs::create_dir(&profile).unwrap();
    std::fs::write(profile.join("history.log"), "formerly indexed").unwrap();
    core.run_index(&core.cancel()).await.unwrap();
    assert_eq!(
        core.database()
            .await
            .unwrap()
            .call(|s| s.counts())
            .await
            .unwrap()["sourceCount"],
        2
    );
    std::fs::write(profile.join("Local State"), "{}").unwrap();
    for name in ["Default", "Profile 2"] {
        let path = docs.join(name);
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("Preferences"), "{}").unwrap();
        std::fs::write(path.join("notes.txt"), "profile junk").unwrap();
    }
    for name in [
        "leveldb",
        "IndexedDB",
        "Local Storage",
        "Session Storage",
        "Service Worker",
        "Cache",
        "Code Cache",
        "GPUCache",
        "blob_storage",
        "shared_proto_db",
        "Crashpad",
        ".git",
        ".venv",
    ] {
        let dir = docs.join(name);
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("junk.txt"), "ignored").unwrap();
        assert!(extract::skip(name, true), "{name}");
    }
    for name in [
        "a.sqlite",
        "a.sqlite3",
        "a.db",
        "a.db-wal",
        "a.db-shm",
        "a.db-journal",
        "000003.ldb",
        "000003.log",
        "LOG",
        "LOG.old",
        "MANIFEST-000001",
        "CURRENT",
        "LOCK",
    ] {
        std::fs::write(docs.join(name), "ignored").unwrap();
        assert!(extract::skip(name, false), "{name}");
    }
    std::fs::write(docs.join("normal.log"), "normal user log").unwrap();
    core.run_index(&core.cancel()).await.unwrap();
    assert_eq!(
        core.database()
            .await
            .unwrap()
            .call(|s| s.counts())
            .await
            .unwrap()["sourceCount"],
        2
    );
    let roots = vec![docs.clone()];
    let profiles = core.inner.profile_roots.lock().unwrap().clone();
    assert_eq!(profiles.len(), 3);
    let event = Ok(
        notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any))
            .add_path(profile.join("history.log")),
    );
    let mut pending = PendingReconcile::default();
    assert!(!pending.record(&event, &roots, &profiles, 100));
    assert!(pending.deadline(0).is_none());
    core.shutdown().await.unwrap();
}

#[test]
fn watcher_skips_browser_junk_and_pending_work_has_max_wait_and_full_rate_limit() {
    use notify::{event::ModifyKind, Event, EventKind};
    let roots = vec![PathBuf::from("/docs")];
    let profiles = HashSet::new();
    let event =
        |p: &str| Ok(Event::new(EventKind::Modify(ModifyKind::Any)).add_path(PathBuf::from(p)));
    let mut pending = PendingReconcile::default();
    for path in [
        "/docs/leveldb/000003.log",
        "/docs/Session Storage/data.txt",
        "/docs/Local Storage/leveldb/a",
        "/docs/a.sqlite3",
        "/docs/a.db-wal",
        "/docs/000003.log",
        "/docs/LOG",
        "/docs/Cache/junk.txt",
        "/docs/.venv/a.py",
        "/docs/.git/a.txt",
    ] {
        let e = event(path);
        assert!(!watcher_reconcile(e.as_ref().unwrap(), &roots), "{path}");
        assert!(
            !watcher_reconcile(
                &Event::new(EventKind::Any).add_path(PathBuf::from(path)),
                &roots
            ),
            "{path}"
        );
        assert!(!pending.record(&e, &roots, &profiles, 100));
    }
    assert!(!pending.record(&event("/elsewhere/a.txt"), &roots, &profiles, 100));
    let mut lexical = PendingReconcile::default();
    assert!(lexical.record(&event("/docs/.git/../eligible"), &roots, &profiles, 100));
    assert_eq!(
        lexical.paths,
        HashSet::from([PathBuf::from("/docs/eligible")])
    );
    assert!(!lexical.full);
    for now in (100..=30_100).step_by(1000) {
        assert!(pending.record(&event("/docs/user.log"), &roots, &profiles, now));
    }
    assert!(pending.take_due(30_099, 0).is_none());
    assert_eq!(pending.take_due(30_100, 0).unwrap().paths.len(), 1);
    let overflow = Ok(Event::new(EventKind::Any).set_flag(notify::event::Flag::Rescan));
    assert!(pending.record(&overflow, &roots, &profiles, 40_000));
    assert!(pending.take_due(299_999, 0).is_none());
    assert!(pending.take_due(300_000, 0).unwrap().full);
    assert!(pending.record(
        &Err(notify::Error::generic("overflow")),
        &roots,
        &profiles,
        400_000
    ));
    assert!(pending.take_due(599_999, 300_000).is_none());
    assert!(pending.take_due(600_000, 300_000).unwrap().full);
    for i in 0..=MAX_PENDING_PATHS {
        pending.record(
            &event(&format!("/docs/{i}.txt")),
            &roots,
            &profiles,
            700_000,
        );
    }
    assert!(pending.full);
    assert!(pending.paths.is_empty());
}
