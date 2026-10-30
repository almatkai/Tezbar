//! Synthetic SQLite regressions. No model/runtime/profile or timing thresholds.
use super::*;
use rusqlite::Connection;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

#[derive(Default)]
struct Queries {
    counts: AtomicU64,
    pending: AtomicU64,
    queue: Mutex<Vec<String>>,
    expensive_sql: Mutex<Vec<String>>,
    cancel_count: Mutex<Option<Arc<AtomicU64>>>,
    external_on_count: Mutex<Option<PathBuf>>,
}
unsafe extern "C" fn trace(
    kind: u32,
    context: *mut std::ffi::c_void,
    _: *mut std::ffi::c_void,
    sql: *mut std::ffi::c_void,
) -> i32 {
    if kind == rusqlite::ffi::SQLITE_TRACE_STMT as u32 && !sql.is_null() {
        // SAFETY: Observed drops its connection before this boxed context.
        let queries = unsafe { &*context.cast::<Queries>() };
        let sql = unsafe { std::ffi::CStr::from_ptr(sql.cast()) }.to_string_lossy();
        if sql.starts_with("SELECT COUNT(g.chunk_id)")
            || sql.starts_with("SELECT c.rowid,c.id,c.text,s.path")
        {
            queries.expensive_sql.lock().unwrap().push(sql.to_string());
        }
        if sql.starts_with("SELECT COUNT(g.chunk_id)") {
            queries.counts.fetch_add(1, Ordering::SeqCst);
            if let Some(generation) = queries.cancel_count.lock().unwrap().take() {
                generation.fetch_add(1, Ordering::SeqCst);
            }
            if let Some(path) = queries.external_on_count.lock().unwrap().take() {
                // Separate synthetic WAL writer commits at the real statement
                // boundary, after the reader captured its invalidation stamp.
                let writer = Connection::open(path).unwrap();
                writer
                    .execute_batch("UPDATE knowledge_chunks SET embedding_json='[]' WHERE id='old'")
                    .unwrap();
            }
        }
        if sql.starts_with("SELECT c.rowid,c.id,c.text,s.path") {
            queries.pending.fetch_add(1, Ordering::SeqCst);
        }
        if sql.starts_with("SELECT job_id,position,root_id,path") {
            queries.queue.lock().unwrap().push(sql.into_owned());
        }
    }
    0
}
struct Observed {
    // Field drop order keeps the trace context alive until SQLite closes.
    store: Store,
    queries: Box<Queries>,
}
fn observed(db: Connection) -> Observed {
    let store = Store::from_connection(db, false, PathBuf::new());
    let queries = Box::<Queries>::default();
    unsafe {
        rusqlite::ffi::sqlite3_trace_v2(
            store.db.handle(),
            rusqlite::ffi::SQLITE_TRACE_STMT as u32,
            Some(trace),
            (&*queries as *const Queries).cast_mut().cast(),
        );
    }
    Observed { store, queries }
}
fn seed(db: &Connection) {
    db.execute_batch(include_str!("schema.sql")).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;
        INSERT INTO knowledge_roots VALUES('r','/r','inherit','local',1,0,0),('other','/other','deep','local',1,0,0);
        INSERT INTO knowledge_sources(id,root_id,path,content_hash,byte_size,modified_at,status) VALUES
            ('s','r','/r/a','a',1,0,'indexed'),('other','other','/other/a','b',1,0,'failed');
        INSERT INTO knowledge_chunks(id,source_id,text,embedding_json) VALUES('a','s','a','[]'),('old','s','old',NULL),('hidden','other','hidden','[]');
        INSERT INTO knowledge_gemma_embeddings VALUES('a','q8',X'00'),('hidden','q8',X'00');").unwrap();
}
fn fixture() -> Observed {
    let db = Connection::open_in_memory().unwrap();
    seed(&db);
    observed(db)
}
fn scope(s: &Store, model: &str) -> Scope {
    Scope::new(&s.settings().unwrap(), &s.roots().unwrap(), model)
}
fn count(s: &Store, key: &Scope, at: Instant) -> CountObservation {
    s.semantic_counts_if_due(key, at).unwrap()
}

#[test]
fn exact_counts_and_gates_cover_real_mutations_and_rollback() {
    let f = fixture();
    let s = &f.store;
    let mut at = Instant::now();
    let key = scope(s, "q8");
    assert_eq!(count(s, &key, at).value, Some((1, 1)));
    for sql in [
        "INSERT INTO knowledge_chunks(id,source_id,text,embedding_json) VALUES('b','s','b','[]')",
        "INSERT INTO knowledge_gemma_embeddings VALUES('b','q8',X'00') ON CONFLICT(chunk_id,model_key) DO UPDATE SET vector=excluded.vector",
        "INSERT INTO knowledge_gemma_embeddings VALUES('b','q8',X'01') ON CONFLICT(chunk_id,model_key) DO UPDATE SET vector=excluded.vector",
        "DELETE FROM knowledge_gemma_embeddings WHERE chunk_id='a'",
        "UPDATE knowledge_sources SET status='failed' WHERE id='s'",
        "UPDATE knowledge_sources SET status='indexed',root_id='other' WHERE id='s'",
        "UPDATE knowledge_chunks SET embedding_json='[]' WHERE id='old'",
        "DELETE FROM knowledge_chunks WHERE id='b'",
    ] {
        s.db.execute_batch(sql).unwrap();
        let n = f.queries.counts.load(Ordering::SeqCst);
        // Changed mini-batches do NOT bypass the completion-based interval.
        assert!(!count(s, &key, at).fresh, "{sql}");
        assert_eq!(f.queries.counts.load(Ordering::SeqCst), n);
        at += Duration::from_secs(11);
        let result = count(s, &key, at);
        assert!(result.fresh);
        assert_eq!(f.queries.counts.load(Ordering::SeqCst), n + 1);
        assert_eq!(result.value, Some(s.gemma_counts_for_model(&key.roots, &key.model).unwrap()), "{sql}");
    }
    let n = f.queries.counts.load(Ordering::SeqCst);
    s.set_meta("status", &json!({"progress":99})).unwrap();
    s.db.execute_batch("INSERT INTO knowledge_indexing_jobs VALUES(1,'job',1,0,0,0); INSERT INTO knowledge_indexing_queue VALUES('job',0,'r','/r/a');").unwrap();
    s.db.execute_batch("BEGIN; UPDATE knowledge_sources SET status='failed'; DELETE FROM knowledge_chunks; ROLLBACK;").unwrap();
    at += Duration::from_secs(100);
    assert!(count(s, &key, at).fresh);
    assert_eq!(
        f.queries.counts.load(Ordering::SeqCst),
        n,
        "metadata, queue DML and rollback must not trigger COUNT"
    );
}

#[test]
fn identical_upsert_is_not_new_embedding_and_does_not_invalidate() {
    let f = fixture();
    let key = scope(&f.store, "q8");
    let at = Instant::now();
    assert_eq!(count(&f.store, &key, at).value, Some((1, 1)));
    f.store.db.execute_batch("INSERT INTO knowledge_gemma_embeddings VALUES('a','q8',X'00') ON CONFLICT(chunk_id,model_key) DO UPDATE SET vector=excluded.vector;").unwrap();
    assert_eq!(
        count(&f.store, &key, at + Duration::from_secs(100)).value,
        Some((1, 1))
    );
    assert_eq!(f.queries.counts.load(Ordering::SeqCst), 1);
}

#[test]
fn scope_key_covers_enabled_depth_settings_and_model() {
    let f = fixture();
    let s = &f.store;
    let at = Instant::now();
    let initial = scope(s, "q8");
    assert_eq!(count(s, &initial, at).value, Some((1, 1)));
    s.db.execute_batch("UPDATE knowledge_roots SET enabled=0 WHERE id='r'")
        .unwrap();
    assert!(s.semantic_counts_if_due(&initial, at).is_err());
    let disabled = scope(s, "q8");
    assert_eq!(count(s, &disabled, at).value, Some((0, 0)));
    s.db.execute_batch("UPDATE knowledge_roots SET enabled=1,depth='basic' WHERE id='r'")
        .unwrap();
    let basic = scope(s, "q8");
    assert_ne!(basic, disabled);
    assert_eq!(count(s, &basic, at).value, Some((0, 0)));
    s.db.execute_batch("UPDATE knowledge_roots SET depth='inherit' WHERE id='r'")
        .unwrap();
    let mut settings = s.settings().unwrap();
    settings["depth"] = json!("deep");
    settings["embeddingVariant"] = json!("q4_k_m");
    s.set_meta("settings", &settings).unwrap();
    let q4 = scope(s, "q4");
    assert_ne!(initial, q4);
    assert_eq!(count(s, &q4, at).value, Some((0, 1)));
    // Source/root move changes both numerator and denominator, not global stats.
    s.db.execute_batch(
        "UPDATE knowledge_sources SET root_id='r',status='indexed' WHERE id='other'",
    )
    .unwrap();
    assert_eq!(
        count(s, &q4, at + Duration::from_secs(11)).value,
        Some((0, 2))
    );
    s.db.execute_batch("DELETE FROM knowledge_roots WHERE id='r'")
        .unwrap();
    assert!(s.semantic_counts_if_due(&q4, at).is_err());
    assert_eq!(count(s, &scope(s, "q4"), at).value, Some((0, 0)));
}

#[test]
fn pending_completion_wakes_for_old_rowid_model_roots_and_forced_recheck() {
    let f = fixture();
    let s = &f.store;
    let at = Instant::now();
    let key = scope(s, "q8");
    for _ in 0..20 {
        assert!(s.semantic_pending(&key, 8, at).unwrap().is_empty());
    }
    assert_eq!(f.queries.pending.load(Ordering::SeqCst), 1);
    s.set_meta("status", &json!({"state":"completed"})).unwrap();
    assert!(s.semantic_pending(&key, 8, at).unwrap().is_empty());
    assert_eq!(f.queries.pending.load(Ordering::SeqCst), 1);
    assert!(s
        .semantic_pending(&key, 8, at + Duration::from_secs(31))
        .unwrap()
        .is_empty());
    assert_eq!(f.queries.pending.load(Ordering::SeqCst), 2);
    // No rowid cursor: existing old row becomes eligible.
    s.db.execute_batch("UPDATE knowledge_chunks SET embedding_json='[]' WHERE id='old'")
        .unwrap();
    let rows = s.semantic_pending(&key, 8, at).unwrap();
    assert_eq!(rows[0]["id"], "old");
    s.db.execute_batch("INSERT INTO knowledge_gemma_embeddings VALUES('old','q8',X'00')")
        .unwrap();
    assert!(s.semantic_pending(&key, 8, at).unwrap().is_empty());
    s.db.execute_batch("UPDATE knowledge_sources SET status='indexed' WHERE id='other'; DELETE FROM knowledge_gemma_embeddings WHERE chunk_id='hidden';").unwrap();
    assert_eq!(s.semantic_pending(&key, 8, at).unwrap()[0]["id"], "hidden");
    let q4 = scope(s, "q4");
    assert_eq!(s.semantic_pending(&q4, 8, at).unwrap().len(), 3);
    s.db.execute_batch("UPDATE knowledge_roots SET enabled=0 WHERE id='r'")
        .unwrap();
    assert!(s.semantic_pending(&key, 8, at).is_err());
    assert_eq!(s.semantic_pending(&scope(s, "q8"), 8, at).unwrap().len(), 1);
}

#[test]
fn readers_do_not_consume_search_cache_dirty_rows() {
    let f = fixture();
    let s = &f.store;
    s.db.execute_batch("UPDATE knowledge_chunks SET embedding_json='[1]' WHERE id='a'")
        .unwrap();
    let dirty = || {
        s.db.query_row("SELECT group_concat(rowid) FROM (SELECT rowid FROM temp.knowledge_search_dirty ORDER BY rowid)", [], |r| r.get::<_, Option<String>>(0)).unwrap()
    };
    let before = dirty();
    assert!(before.is_some());
    let key = scope(s, "q8");
    count(s, &key, Instant::now());
    s.semantic_pending(&key, 8, Instant::now()).unwrap();
    assert_eq!(dirty(), before);
}

#[test]
fn failed_and_cancelled_count_remain_dirty() {
    let f = fixture();
    let s = &f.store;
    let key = scope(s, "q8");
    let at = Instant::now();
    count(s, &key, at);
    s.db.execute_batch("UPDATE knowledge_chunks SET embedding_json='[]' WHERE id='old'; ALTER TABLE knowledge_gemma_embeddings RENAME TO hidden_embeddings").unwrap();
    let next = at + Duration::from_secs(11);
    assert!(s.semantic_counts_if_due(&key, next).is_err());
    s.db.execute_batch("ALTER TABLE hidden_embeddings RENAME TO knowledge_gemma_embeddings")
        .unwrap();
    assert_eq!(count(s, &key, next).value, Some((1, 2)));
    // Cancellation at the actual COUNT statement boundary, not a helper-only mock.
    let epoch = Arc::new(AtomicU64::new(0));
    let token = super::super::SearchToken::background(super::super::super::extract::Cancellation {
        generation: epoch.clone(),
        expected: 0,
    });
    s.db.execute_batch("UPDATE knowledge_chunks SET embedding_json='[1]' WHERE id='old'")
        .unwrap();
    *f.queries.cancel_count.lock().unwrap() = Some(epoch.clone());
    // Direct seam uses the same progress guard/cancellation checkpoint as call_search.
    let mut s = f;
    s.store.search_cancel = Some(token.clone());
    let later = next + Duration::from_secs(11);
    {
        let _sql = super::super::search_cache::SqlSearchGuard::new(&s.store.db, token);
        assert!(s.store.semantic_counts_if_due(&key, later).is_err());
    }
    s.store.search_cancel = None;
    let n = s.queries.counts.load(Ordering::SeqCst);
    assert_eq!(count(&s.store, &key, later).value, Some((1, 2)));
    assert_eq!(s.queries.counts.load(Ordering::SeqCst), n + 1);
}

#[test]
fn external_commit_and_reconnect_invalidate_without_total_changes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("isolated.sqlite");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA journal_mode=WAL").unwrap();
    seed(&db);
    let f = observed(db);
    let key = scope(&f.store, "q8");
    let at = Instant::now();
    assert_eq!(count(&f.store, &key, at).value, Some((1, 1)));
    assert!(f.store.semantic_pending(&key, 8, at).unwrap().is_empty());
    let total = f.store.db.total_changes();
    let writer = Connection::open(&path).unwrap();
    writer
        .execute_batch("UPDATE knowledge_chunks SET embedding_json='[]' WHERE id='old'")
        .unwrap();
    assert_eq!(f.store.db.total_changes(), total);
    assert!(!count(&f.store, &key, at).fresh);
    assert_eq!(
        count(&f.store, &key, at + Duration::from_secs(11)).value,
        Some((1, 2))
    );
    assert_eq!(
        f.store.semantic_pending(&key, 8, at).unwrap()[0]["id"],
        "old"
    );
    writer
        .execute_batch("UPDATE knowledge_roots SET depth='basic' WHERE id='r'")
        .unwrap();
    assert!(f.store.semantic_counts_if_due(&key, at).is_err());
    drop(f);
    let fresh = observed(Connection::open(&path).unwrap());
    let new_key = scope(&fresh.store, "q8");
    assert_eq!(count(&fresh.store, &new_key, at).value, Some((0, 0)));
    assert_eq!(fresh.queries.counts.load(Ordering::SeqCst), 1);
}

#[test]
fn queue_plan_active_checkpoint_resume_stale_completion_and_cancel() {
    let mut f = fixture();
    f.store.db.execute_batch("INSERT INTO knowledge_indexing_jobs VALUES(1,'active',2,0,0,0); INSERT INTO knowledge_indexing_queue VALUES('stale',0,'r','wrong'),('active',2,'r','second'),('active',1,'r','first');").unwrap();
    let rows = f.store.next_batch(1).unwrap();
    assert_eq!(rows[0].3, "first");
    let sql = f.queries.queue.lock().unwrap()[0].clone();
    let plan = f
        .store
        .db
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .unwrap()
        .query_map([1], |r| r.get::<_, String>(3))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    println!("SQLite {} active queue: {:?}", rusqlite::version(), plan);
    assert!(plan
        .iter()
        .any(|p| p.contains("SEARCH knowledge_indexing_queue") && p.contains("job_id=?")));
    assert!(!plan
        .iter()
        .any(|p| p.contains("SCAN knowledge_indexing_queue") || p.contains("TEMP B-TREE")));
    f.store.complete("active", 1, false).unwrap();
    assert_eq!(f.store.next_batch(8).unwrap().len(), 1);
    assert_eq!(
        f.store.checkpoint().unwrap().unwrap()["processedSources"],
        1
    );
    f.store.complete("active", 1, false).unwrap();
    f.store.complete("stale", 0, true).unwrap();
    assert_eq!(
        f.store.checkpoint().unwrap().unwrap()["processedSources"],
        1
    );
    // Missing checkpoint must not resurrect abandoned queue rows.
    f.store
        .db
        .execute_batch("DELETE FROM knowledge_indexing_jobs")
        .unwrap();
    assert!(f.store.next_batch(8).unwrap().is_empty());
    f.store.clear_checkpoint().unwrap();
    assert!(f.store.next_batch(8).unwrap().is_empty());
}

#[test]
fn external_commit_during_count_discards_result_and_retries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("isolated-race.sqlite");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA journal_mode=WAL").unwrap();
    seed(&db);
    let f = observed(db);
    let key = scope(&f.store, "q8");
    *f.queries.external_on_count.lock().unwrap() = Some(path);
    let at = Instant::now();
    assert!(f.store.semantic_counts_if_due(&key, at).is_err());
    assert_eq!(count(&f.store, &key, at).value, Some((1, 2)));
    assert_eq!(f.queries.counts.load(Ordering::SeqCst), 2);
}

#[test]
fn sqlite_progress_interrupts_large_count_and_is_removed_before_mutation() {
    let mut f = fixture();
    f.store.db.execute_batch("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<4096) INSERT INTO knowledge_chunks(id,source_id,text,embedding_json) SELECT 'large-'||x,'s','body','[]' FROM n").unwrap();
    let epoch = Arc::new(AtomicU64::new(0));
    let token = super::super::SearchToken::background(super::super::super::extract::Cancellation {
        generation: epoch.clone(),
        expected: 0,
    });
    *f.queries.cancel_count.lock().unwrap() = Some(epoch);
    let key = scope(&f.store, "q8");
    f.store.search_cancel = Some(token.clone());
    {
        let _sql = super::super::search_cache::SqlSearchGuard::new(&f.store.db, token);
        let error = f
            .store
            .semantic_counts_if_due(&key, Instant::now())
            .err()
            .unwrap();
        assert!(
            error.contains("interrupted"),
            "actual SQLite VM must be interrupted: {error}"
        );
    }
    f.store.search_cancel = None;
    f.store.set_meta("after-cancel", &json!(true)).unwrap();
    assert_eq!(count(&f.store, &key, Instant::now()).value, Some((1, 4097)));
}

#[test]
fn queue_restart_resumes_only_active_job() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("queue.sqlite");
    let db = Connection::open(&path).unwrap();
    seed(&db);
    let mut f = observed(db);
    f.store.db.execute_batch("INSERT INTO knowledge_indexing_jobs VALUES(1,'resume',2,0,0,0); INSERT INTO knowledge_indexing_queue VALUES('resume',0,'r','one'),('resume',1,'r','two'),('abandoned',0,'r','wrong');").unwrap();
    f.store.complete("resume", 0, false).unwrap();
    drop(f);
    let f = observed(Connection::open(&path).unwrap());
    let rows = f.store.next_batch(8).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].3, "two");
    assert_eq!(
        f.store.checkpoint().unwrap().unwrap()["processedSources"],
        1
    );
}

#[test]
fn explain_exact_count_and_pending_on_bundled_sqlite() {
    let f = fixture();
    let key = scope(&f.store, "q8");
    count(&f.store, &key, Instant::now());
    f.store.semantic_pending(&key, 8, Instant::now()).unwrap();
    let statements = f.queries.expensive_sql.lock().unwrap().clone();
    for sql in statements {
        let mut explain = f
            .store
            .db
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .unwrap();
        let plan = if sql.starts_with("SELECT COUNT") {
            explain
                .query_map(["q8"], |r| r.get::<_, String>(3))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        } else {
            explain
                .query_map(rusqlite::params!["q8", 8], |r| r.get::<_, String>(3))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        println!("SQLite {} {:?}: {:?}", rusqlite::version(), sql, plan);
    }
}

#[test]
fn drained_known_incomplete_progress_reconciles_once_without_per_chunk_counts() {
    let f = fixture();
    let s = &f.store;
    let at = Instant::now();
    s.db.execute_batch("UPDATE knowledge_chunks SET embedding_json='[]' WHERE id='old'").unwrap();
    let key = scope(s, "q8");
    assert_eq!(count(s, &key, at).value, Some((1, 2)));
    s.db.execute_batch("INSERT INTO knowledge_gemma_embeddings VALUES('old','q8',X'00'); INSERT INTO knowledge_gemma_embeddings VALUES('a','q4',X'00'),('old','q4',X'00');").unwrap();
    // An empty other-model queue must not bypass this scope's count budget.
    let other = scope(s, "q4");
    assert!(s.semantic_pending(&other, 8, at).unwrap().is_empty());
    assert!(!count(s, &key, at).fresh);
    assert_eq!(f.queries.counts.load(Ordering::SeqCst), 1);
    assert!(s.semantic_pending(&key, 8, at).unwrap().is_empty());
    let complete = count(s, &key, at + Duration::from_millis(1));
    assert!(complete.fresh && complete.reconciled, "known incomplete progress must become fresh when its exact queue drains");
    assert_eq!(complete.value, Some((2, 2)));
    assert_eq!(f.queries.counts.load(Ordering::SeqCst), 2);
    // A stream of already-complete tiny additions retains normal coalescing.
    for i in 0..20 {
        s.db.execute_batch(&format!("INSERT INTO knowledge_chunks(id,source_id,text,embedding_json) VALUES('extra{i}','s','extra','[]'); INSERT INTO knowledge_gemma_embeddings VALUES('extra{i}','q8',X'00');")).unwrap();
        assert!(s.semantic_pending(&key, 8, at).unwrap().is_empty());
        assert!(!count(s, &key, at + Duration::from_millis(2)).fresh);
    }
    assert_eq!(f.queries.counts.load(Ordering::SeqCst), 2, "no COUNT per already-complete mini-batch");
}
