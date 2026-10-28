//! Real service/DB-worker regression seams; no runtime/model downloads.
use super::*;
use crate::native_core::common::CorePaths;
#[derive(Default)]
struct Queries {
    counts: AtomicU64,
    pending: AtomicU64,
    cancel_count: Mutex<Option<Arc<AtomicU64>>>,
}
unsafe extern "C" fn trace(
    kind: u32,
    context: *mut std::ffi::c_void,
    _: *mut std::ffi::c_void,
    sql: *mut std::ffi::c_void,
) -> i32 {
    if kind == rusqlite::ffi::SQLITE_TRACE_STMT as u32 && !sql.is_null() {
        // Store retains this Arc until its SQLite connection has closed.
        let q = unsafe { &*context.cast::<Queries>() };
        let sql = unsafe { std::ffi::CStr::from_ptr(sql.cast()) }.to_string_lossy();
        if sql.starts_with("SELECT COUNT(g.chunk_id)") {
            q.counts.fetch_add(1, Ordering::SeqCst);
            if let Some(epoch) = q.cancel_count.lock().unwrap().take() {
                epoch.fetch_add(1, Ordering::SeqCst);
            }
        }
        if sql.starts_with("SELECT c.rowid,c.id,c.text,s.path") {
            q.pending.fetch_add(1, Ordering::SeqCst);
        }
    }
    0
}
async fn fixture() -> (KnowledgeCore, Arc<Queries>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let core = KnowledgeCore::new(CoreContext::headless(CorePaths {
        data_dir: dir.path().join("data"),
        config_dir: dir.path().join("config"),
        resource_dir: dir.path().join("resources"),
        temp_dir: dir.path().join("tmp"),
        home_dir: dir.path().join("home"),
    }));
    let q = Arc::new(Queries::default());
    let probe = q.clone();
    core.database().await.unwrap().call(move |s| {
        s.db.execute_batch("INSERT INTO knowledge_roots VALUES('r','/isolated-docs','inherit','local',1,0,0);
            INSERT INTO knowledge_sources(id,root_id,path,content_hash,byte_size,modified_at,status) VALUES('s','r','/isolated-docs/a','a',1,0,'indexed');
            INSERT INTO knowledge_chunks(id,source_id,text,embedding_json) VALUES('c','s','body','[]');").map_err(|e|e.to_string())?;
        s.db.execute("INSERT INTO knowledge_gemma_embeddings VALUES('c',?,X'00')", [EmbeddingVariantId::Q8_0.descriptor().model_key]).map_err(|e|e.to_string())?;
        let mut settings = s.settings()?;
        settings["embeddingModel"] = json!("embeddinggemma-2");
        settings["indexingSpeed"] = json!("fast");
        s.set_meta("settings", &settings)?;
        s.trace_owner = Some(probe.clone());
        unsafe { rusqlite::ffi::sqlite3_trace_v2(s.db.handle(), rusqlite::ffi::SQLITE_TRACE_STMT as u32, Some(trace), Arc::as_ptr(&probe).cast_mut().cast()); }
        Ok(())
    }).await.unwrap();
    core.refresh().await.unwrap();
    // These DB-service fixtures do not load a model. Supply the availability
    // state explicitly; real verification/inference have runtime fixtures.
    for runtime in &core.inner.runtimes {
        runtime.patch(json!({"modelState":"ready","integrityState":"verified"}));
    }
    (core, q, dir)
}

fn semantic(core: &KnowledgeCore) -> Value {
    core.semantic_snapshot(&core.inner.cache.lock().unwrap())
}
#[tokio::test]
async fn inactive_or_unavailable_model_never_counts_or_advertises_fresh_progress() {
    let (core, q, _dir) = fixture().await;
    core.refresh_semantic_counts().await.unwrap();
    assert_eq!(semantic(&core)["countsStale"], false);
    assert_eq!(semantic(&core)["variants"][1]["countsStale"], true);
    let runtime = core.selected_runtime();
    for (model, integrity) in [
        ("ready", "unchecked"),
        ("ready", "verifying"),
        ("error", "invalid"),
        ("downloading", "unchecked"),
        ("not-downloaded", "missing"),
    ] {
        runtime.patch(json!({"modelState":model,"integrityState":integrity}));
        let status = semantic(&core);
        assert_eq!(status["countsStale"], true, "{model}/{integrity}");
        assert!(status["indexVariant"].is_null());
        assert_eq!(status["indexedChunks"], 1, "keep stale numeric progress");
        assert_eq!(status["variants"][0]["countsStale"], true);
        core.refresh_semantic_counts().await.unwrap();
        assert_eq!(q.counts.load(Ordering::SeqCst), 1);
    }
    runtime.patch(json!({"modelState":"ready","integrityState":"verified"}));
    core.refresh_semantic_counts().await.unwrap();
    assert_eq!(semantic(&core)["countsStale"], false);
    runtime.remove().await.unwrap();
    core.refresh_semantic_counts().await.unwrap();
    assert_eq!(semantic(&core)["countsStale"], true);
    assert_eq!(q.counts.load(Ordering::SeqCst), 1);

    // Built-in selection is inactive even if Gemma remains installed/verified.
    runtime.patch(json!({"modelState":"ready","integrityState":"verified"}));
    core.handle("knowledge:embedding:set", &[json!("built-in")])
        .await.unwrap().unwrap();
    core.refresh_semantic_counts().await.unwrap();
    assert_eq!(semantic(&core)["countsStale"], true);
    assert!(semantic(&core)["indexVariant"].is_null());
    assert_eq!(q.counts.load(Ordering::SeqCst), 1);
    core.shutdown().await.unwrap();
}
#[tokio::test]
async fn pause_resume_scope_and_variant_never_publish_other_counts() {
    let (core, q, _dir) = fixture().await;
    core.refresh_semantic_counts().await.unwrap();
    assert_eq!(semantic(&core)["indexedChunks"], 1);
    core.inner.paused.store(true, Ordering::SeqCst);
    core.inner
        .semantic_generation
        .fetch_add(1, Ordering::SeqCst);
    for _ in 0..12 {
        core.refresh_semantic_counts().await.unwrap();
    }
    assert_eq!(q.counts.load(Ordering::SeqCst), 1);
    assert_eq!(semantic(&core)["countsStale"], true);
    assert!(semantic(&core)["indexVariant"].is_null());
    core.inner.paused.store(false, Ordering::SeqCst);
    core.inner
        .semantic_generation
        .fetch_add(1, Ordering::SeqCst);
    core.refresh_semantic_counts().await.unwrap();
    assert_eq!(semantic(&core)["countsStale"], false);
    assert_eq!(q.counts.load(Ordering::SeqCst), 1);
    let db = core.database().await.unwrap();
    db.call(|s| {
        s.db.execute_batch("UPDATE knowledge_roots SET depth='basic' WHERE id='r'")
            .map_err(|e| e.to_string())
    })
    .await
    .unwrap();
    core.refresh().await.unwrap();
    assert_eq!(
        q.counts.load(Ordering::SeqCst),
        1,
        "settings refresh must not count"
    );
    assert_eq!(semantic(&core)["countsStale"], true);
    assert_eq!(semantic(&core)["totalChunks"], 0);
    core.refresh_semantic_counts().await.unwrap();
    assert_eq!(semantic(&core)["totalChunks"], 0);
    db.call(|s| {
        s.db.execute_batch("UPDATE knowledge_roots SET depth='smart' WHERE id='r'")
            .map_err(|e| e.to_string())?;
        let mut settings = s.settings()?;
        settings["embeddingVariant"] = json!("q4_k_m");
        s.set_meta("settings", &settings)
    })
    .await
    .unwrap();
    core.refresh().await.unwrap();
    assert!(semantic(&core)["indexVariant"].is_null());
    core.refresh_semantic_counts().await.unwrap();
    assert_eq!(semantic(&core)["indexVariant"], "q4_k_m");
    assert_eq!(semantic(&core)["indexedChunks"], 0);
    assert_eq!(semantic(&core)["totalChunks"], 1);
    assert_eq!(q.counts.load(Ordering::SeqCst), 2);
    core.inner.closed.store(true, Ordering::SeqCst);
    core.inner
        .semantic_generation
        .fetch_add(1, Ordering::SeqCst);
    core.refresh_semantic_counts().await.unwrap();
    assert_eq!(q.counts.load(Ordering::SeqCst), 2);
    assert_eq!(semantic(&core)["countsStale"], true);
    let _ = db.close().await;
}
#[tokio::test]
async fn queued_old_scope_and_dropped_read_do_not_execute_or_publish() {
    use std::future::Future;
    use std::task::Poll;
    let (core, q, _dir) = fixture().await;
    let db = core.database().await.unwrap();
    let (entered, started) = tokio::sync::oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let worker = db.clone();
    let blocker = tokio::spawn(async move {
        worker
            .call(move |_| {
                let _ = entered.send(());
                blocked.recv().unwrap();
                Ok(())
            })
            .await
    });
    started.await.unwrap();
    let mut old = Box::pin(core.refresh_semantic_counts());
    std::future::poll_fn(|cx| {
        assert!(old.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    let scope = core.semantic_scope();
    let mut dropped = Box::pin(db.call_search(
        db::SearchToken::background(core.semantic_cancel()),
        move |s| s.semantic_counts_if_due(&scope, std::time::Instant::now()),
    ));
    std::future::poll_fn(|cx| {
        assert!(dropped.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    drop(dropped);
    core.inner.semantic_changing.store(true, Ordering::SeqCst);
    core.inner
        .semantic_generation
        .fetch_add(1, Ordering::SeqCst);
    core.inner.cache.lock().unwrap().settings["embeddingVariant"] = json!("q4_k_m");
    release.send(()).unwrap();
    blocker.await.unwrap().unwrap();
    assert!(old.await.is_err());
    assert_eq!(q.counts.load(Ordering::SeqCst), 0);
    assert!(semantic(&core)["indexVariant"].is_null());
    db.call(|s| {
        let mut settings = s.settings()?;
        settings["embeddingVariant"] = json!("q4_k_m");
        s.set_meta("settings", &settings)
    })
    .await
    .unwrap();
    core.refresh().await.unwrap();
    core.inner.semantic_changing.store(false, Ordering::SeqCst);
    core.refresh_semantic_counts().await.unwrap();
    assert_eq!(q.counts.load(Ordering::SeqCst), 1);
    assert_eq!(semantic(&core)["indexVariant"], "q4_k_m");
    assert_eq!(semantic(&core)["indexedChunks"], 0);
    let _ = db.close().await;
}
#[tokio::test]
async fn cancellation_during_real_count_does_not_publish_and_retries() {
    let (core, q, _dir) = fixture().await;
    *q.cancel_count.lock().unwrap() = Some(core.inner.semantic_generation.clone());
    assert!(core.refresh_semantic_counts().await.is_err());
    assert!(core.inner.cache.lock().unwrap().semantic_counts.is_none());
    assert_eq!(semantic(&core)["countsStale"], true);
    core.refresh_semantic_counts().await.unwrap();
    assert_eq!(q.counts.load(Ordering::SeqCst), 2);
    assert_eq!(semantic(&core)["indexedChunks"], 1);
    assert_eq!(semantic(&core)["countsStale"], false);
    let _ = core.database().await.unwrap().close().await;
}
#[tokio::test]
async fn metadata_refresh_and_snapshot_polling_never_count() {
    let (core, q, _dir) = fixture().await;
    for _ in 0..20 {
        core.refresh().await.unwrap();
        let _ = semantic(&core);
    }
    assert_eq!(q.counts.load(Ordering::SeqCst), 0);
    assert_eq!(q.pending.load(Ordering::SeqCst), 0);
    let _ = core.database().await.unwrap().close().await;
}

#[tokio::test]
async fn unchanged_service_ticks_and_completed_batches_use_one_expensive_query() {
    let (core, q, _dir) = fixture().await;
    for _ in 0..12 {
        core.refresh_semantic_counts().await.unwrap();
        core.learned_batch(&core.semantic_cancel()).await.unwrap();
    }
    assert_eq!(q.counts.load(Ordering::SeqCst), 1);
    assert_eq!(q.pending.load(Ordering::SeqCst), 1);
    // Real mini-batch DML; UPSERT UPDATE is not an increment in cardinality.
    for i in 0..12 {
        core.database()
            .await
            .unwrap()
            .call(move |s| {
                s.db.execute(
                    "UPDATE knowledge_gemma_embeddings SET vector=?",
                    [vec![i as u8]],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            })
            .await
            .unwrap();
        core.refresh_semantic_counts().await.unwrap();
    }
    assert_eq!(q.counts.load(Ordering::SeqCst), 1);
    assert_eq!(semantic(&core)["countsStale"], true);
    let scope = core.semantic_scope();
    core.database()
        .await
        .unwrap()
        .call(move |s| {
            s.semantic_counts_if_due(
                &scope,
                std::time::Instant::now() + std::time::Duration::from_secs(11),
            )
        })
        .await
        .unwrap();
    core.refresh_semantic_counts().await.unwrap();
    assert_eq!(q.counts.load(Ordering::SeqCst), 2);
    assert_eq!(semantic(&core)["indexedChunks"], 1);
    assert_eq!(semantic(&core)["totalChunks"], 1);
    core.database().await.unwrap().close().await.unwrap();
}
