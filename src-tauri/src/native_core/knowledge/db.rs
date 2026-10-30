use super::{runtime::MODEL_KEY, text};
use crate::native_core::common::{now_ms, CoreContext};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::sync::{mpsc, oneshot};
#[path = "search_cache.rs"]
mod search_cache;
#[path = "semantic_work.rs"]
pub(super) mod semantic_work;
pub(crate) use search_cache::{SearchGuard, SearchToken};
use std::{
    cell::RefCell,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
type Job = Box<dyn FnOnce(&mut Store) + Send>;
#[derive(Clone)]
pub struct Database {
    sender: mpsc::Sender<Option<Job>>,
    cache_control: Arc<search_cache::Control>,
}
impl Database {
    pub async fn open(ctx: CoreContext) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel::<Option<Job>>(64);
        let (ready, rx) = oneshot::channel();
        let cache_control = Arc::new(search_cache::Control::default());
        let control = cache_control.clone();
        let runtime = tokio::runtime::Handle::current();
        std::thread::Builder::new()
            .name("knowledge-sqlite".into())
            .spawn(move || {
                let store = match Store::open(&ctx) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = ready.send(Err(e));
                        return;
                    }
                };
                store.search_cache.borrow_mut().control = control;
                let _ = ready.send(Ok(()));
                run_worker(store, receiver, runtime);
            })
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())??;
        Ok(Self {
            sender,
            cache_control,
        })
    }
    pub fn pause_search_cache(&self, paused: bool) {
        self.cache_control.paused.store(paused, Ordering::SeqCst);
    }
    /// Read-only search jobs only. The scoped SQLite callback must never span
    /// indexing/mutation work; those continue to use `call` and their own tokens.
    pub async fn call_search<T: Send + 'static>(
        &self,
        token: SearchToken,
        f: impl FnOnce(&mut Store) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        token.check()?;
        let receiver =
            search_cache::ReceiverGuard(Arc::new(std::sync::atomic::AtomicBool::new(true)));
        let token = token.with_receiver(receiver.0.clone());
        let (tx, rx) = oneshot::channel();
        let reply_token = token.clone();
        self.sender
            .send(Some(Box::new(move |s| {
                if tx.is_closed() || token.check().is_err() {
                    return;
                }
                s.search_cancel = Some(token.clone());
                let result = {
                    let _sql = search_cache::SqlSearchGuard::new(&s.db, token.clone());
                    f(s).and_then(|value| {
                        token.check()?;
                        Ok(value)
                    })
                };
                s.search_cancel = None;
                let _ = tx.send(result);
            })))
            .await
            .map_err(|_| "Knowledge database stopped")?;
        let result = rx.await.map_err(|e| e.to_string())?;
        reply_token.check()?;
        result
    }
    pub async fn close(&self) -> Result<(), String> {
        self.cache_control.stopped.store(true, Ordering::SeqCst);
        self.sender
            .send(None)
            .await
            .map_err(|_| "Knowledge database stopped".to_owned())?;
        self.sender.closed().await;
        Ok(())
    }
    pub async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Store) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Some(Box::new(move |s| {
                let _ = tx.send(f(s));
            })))
            .await
            .map_err(|_| "Knowledge database stopped")?;
        rx.await.map_err(|e| e.to_string())?
    }
}
fn run_worker(
    mut store: Store,
    mut receiver: mpsc::Receiver<Option<Job>>,
    runtime: tokio::runtime::Handle,
) {
    loop {
        let next = match receiver.try_recv() {
            Ok(job) => Some(job),
            Err(mpsc::error::TryRecvError::Disconnected) => break,
            Err(mpsc::error::TryRecvError::Empty) => {
                if store.search_cache.borrow().needs_work() {
                    let _ = store.search_cache_maintenance();
                    std::thread::yield_now();
                    continue;
                }
                // No cache means no idle/pressure eviction deadline. A job or
                // close message will wake the worker; mutation maintenance
                // below still clears the transactional dirty journal.
                if !store.search_cache.borrow().has_requested_cache() {
                    match receiver.blocking_recv() {
                        Some(Some(job)) => {
                            job(&mut store);
                            let _ = store.search_cache_maintenance();
                            continue;
                        }
                        _ => break,
                    }
                }
                match runtime.block_on(async {
                    tokio::time::timeout(Duration::from_secs(1), receiver.recv()).await
                }) {
                    Ok(job) => job,
                    Err(_) => {
                        let _ = store.search_cache_maintenance();
                        continue;
                    }
                }
            }
        };
        match next {
            Some(Some(job)) => job(&mut store),
            _ => break,
        }
        // At most one bounded batch per foreground/indexing job; check the queue
        // again between batches. No 250ms-per-batch warmup throttle.
        let _ = store.search_cache_maintenance();
    }
    store.clear_search_cache();
}
pub struct Store {
    search_cache: RefCell<search_cache::Cache>,
    search_cancel: Option<SearchToken>,
    semantic_work: RefCell<semantic_work::Work>,
    pub db: Connection,
    pub vector: bool,
    rank_join: bool,
    pub path: PathBuf,
    // Drop after db: trace callbacks may run until the connection closes, even
    // when a regression test panics before its explicit asynchronous close.
    #[cfg(test)]
    pub(super) trace_owner: Option<Arc<dyn Send + Sync>>,
}
fn err(e: rusqlite::Error) -> String {
    e.to_string()
}
// Legacy SQLite TEXT can contain invalid UTF-8. Decode only document content;
// IDs, paths, JSON metadata and non-TEXT storage classes keep strict conversion.
fn content_bytes<'a>(row: &'a rusqlite::Row<'_>, index: usize) -> rusqlite::Result<&'a [u8]> {
    match row.get_ref(index)? {
        rusqlite::types::ValueRef::Text(bytes) => Ok(bytes),
        value => Err(rusqlite::Error::InvalidColumnType(
            index,
            row.as_ref().column_name(index)?.to_owned(),
            value.data_type(),
        )),
    }
}
fn content_text(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<String> {
    Ok(String::from_utf8_lossy(content_bytes(row, index)?).into_owned())
}
fn optional_content_text(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<Option<String>> {
    if row.get_ref(index)? == rusqlite::types::ValueRef::Null {
        Ok(None)
    } else {
        content_text(row, index).map(Some)
    }
}
pub fn normalize_root(path: &Path) -> Result<PathBuf, String> {
    let input = PathBuf::from(path.to_string_lossy().trim());
    let input = if input.is_absolute() {
        input
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(input)
    };
    let mut result = PathBuf::new();
    for component in input.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    Ok(result)
}
pub fn title(path: &str) -> String {
    Path::new(path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
pub fn default_settings() -> Value {
    json!({"depth":"smart","embeddingModel":"built-in","embeddingVariant":"q8_0","indexingSpeed":"medium","runOnBattery":false,"onlyRunHeavyJobsWhenIdle":true,"maxConcurrentExtractors":1,"maxConcurrentOcrJobs":1})
}
pub fn default_status() -> Value {
    json!({"state":"idle","backend":"local","progress":0,"queuedSources":0,"processedSources":0,"failedSources":0,"sourceCount":0,"chunkCount":0,"indexedPageCount":0,"totalPageCount":0,"partialSourceCount":0,"sourceBytes":0})
}
impl Store {
    pub(crate) fn from_connection(db: Connection, vector: bool, path: PathBuf) -> Self {
        let rank_join = crate::native_core::fts_rank::supported()
            && crate::native_core::fts_rank::register(&db).is_ok();
        let store = Self {
            db,
            vector,
            rank_join,
            path,
            search_cache: RefCell::default(),
            search_cancel: None,
            semantic_work: RefCell::default(),
            #[cfg(test)]
            trace_owner: None,
        };
        // Install TEMP transactional tracking outside cancellable search
        // savepoints. Optional-cache setup failure cannot disable streaming.
        let _ = store.search_cache.borrow_mut().journal(&store.db);
        store
    }
    fn open(ctx: &CoreContext) -> Result<Self, String> {
        let directory = ctx.paths.data_dir.join("knowledge");
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let path = directory.join("knowledge.sqlite3");
        let db = Connection::open(&path).map_err(err)?;
        db.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(err)?;
        db.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;",
        )
        .map_err(err)?;
        let ext = if cfg!(target_os = "macos") {
            "dylib"
        } else if cfg!(windows) {
            "dll"
        } else {
            "so"
        };
        let mut candidates = vec![];
        for base in [
            &ctx.paths.resource_dir,
            &ctx.paths.resource_dir.join("_up_"),
            &ctx.paths.resource_dir.join(".."),
        ] {
            candidates.push(base.join(format!("dist-backend/vec0.{ext}")));
            candidates.push(base.join(format!("vec0.{ext}")));
            candidates.push(base.join("dist-backend/vec0"));
            candidates.push(base.join("vec0"));
        }
        let mut vector = false;
        for p in candidates {
            if p.is_file() {
                unsafe {
                    db.load_extension_enable().map_err(err)?;
                    let result = db.load_extension(p, None);
                    db.load_extension_disable().map_err(err)?;
                    if result.is_ok() {
                        vector = true;
                        break;
                    }
                }
            }
        }
        db.execute_batch(include_str!("schema.sql")).map_err(err)?;
        super::embedding_schema::ensure_embedding_schema(&db)?;
        // Additive migration for databases predating page and depth metadata.
        for (table, column, definition) in [
            (
                "knowledge_sources",
                "indexing_profile",
                "TEXT NOT NULL DEFAULT ''",
            ),
            ("knowledge_sources", "total_pages", "INTEGER"),
            (
                "knowledge_sources",
                "indexed_pages",
                "INTEGER NOT NULL DEFAULT 0",
            ),
            (
                "knowledge_pages",
                "extraction_status",
                "TEXT NOT NULL DEFAULT 'not-indexed'",
            ),
            (
                "knowledge_pages",
                "ocr_status",
                "TEXT NOT NULL DEFAULT 'not-indexed'",
            ),
            (
                "knowledge_pages",
                "embedding_status",
                "TEXT NOT NULL DEFAULT 'not-indexed'",
            ),
            (
                "knowledge_pages",
                "image_embedding_status",
                "TEXT NOT NULL DEFAULT 'not-indexed'",
            ),
            ("knowledge_pages", "content_hash", "TEXT"),
            ("knowledge_pages", "processing_cost_ms", "INTEGER"),
            ("knowledge_pages", "last_accessed_at", "INTEGER"),
        ] {
            let mut stmt = db
                .prepare(&format!("PRAGMA table_info({table})"))
                .map_err(err)?;
            let columns = stmt
                .query_map([], |r| r.get::<_, String>(1))
                .map_err(err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?;
            if !columns.iter().any(|s| s == column) {
                db.execute_batch(&format!(
                    "ALTER TABLE {table} ADD COLUMN {column} {definition}"
                ))
                .map_err(err)?;
            }
        }
        // A missing extension must not make legacy vec triggers prevent text mutations.
        if vector {
            db.execute_batch("CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_chunk_vectors_binary USING vec0(embedding bit[384],root_id text); CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_gemma_vectors USING vec0(embedding float[256] distance_metric=cosine,root_id text); CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_gemma_vectors_q4 USING vec0(embedding float[256] distance_metric=cosine,root_id text); DROP TRIGGER IF EXISTS knowledge_gemma_vector_delete; CREATE TRIGGER knowledge_gemma_vector_delete BEFORE DELETE ON knowledge_chunks BEGIN DELETE FROM knowledge_gemma_vectors WHERE rowid=OLD.rowid; DELETE FROM knowledge_gemma_vectors_q4 WHERE rowid=OLD.rowid; END;").map_err(err)?;
        } else {
            db.execute_batch("DROP TRIGGER IF EXISTS knowledge_gemma_vector_delete;")
                .map_err(err)?;
        }
        db.execute_batch("CREATE INDEX IF NOT EXISTS knowledge_sources_reuse_idx ON knowledge_sources(content_hash,indexing_profile,status,indexed_at DESC); UPDATE knowledge_roots SET depth='inherit' WHERE depth IN ('text','text-and-images');").map_err(err)?;
        let store = Self::from_connection(db, vector, path);
        if store
            .db
            .query_row("SELECT id FROM knowledge_stats WHERE id=1", [], |r| {
                r.get::<_, i64>(0)
            })
            .optional()
            .map_err(err)?
            .is_none()
        {
            let status = store.meta("status", default_status())?;
            store
                .db
                .execute(
                    "INSERT INTO knowledge_stats VALUES(1,?,?,?,?,?,?)",
                    params![
                        number(&status, "sourceCount"),
                        number(&status, "chunkCount"),
                        number(&status, "indexedPageCount"),
                        number(&status, "totalPageCount"),
                        number(&status, "partialSourceCount"),
                        number(&status, "sourceBytes")
                    ],
                )
                .map_err(err)?;
        }
        if !store.vector {
            store.set_meta("rust-vector-rebuild-needed-v1", &json!(true))?;
        } else if store.meta("rust-vector-rebuild-needed-v1", json!(false))? == true {
            // Only acceleration tables are discarded; original float embeddings remain authoritative.
            store.db.execute_batch("DELETE FROM knowledge_chunk_vectors_binary;DELETE FROM knowledge_gemma_vectors;DELETE FROM knowledge_gemma_vectors_q4;").map_err(err)?;
            store.set_meta("vector-binary-backfill-cursor-v1", &json!(0))?;
            store.set_meta("rust-gemma-vector-cursor-v1", &json!(0))?;
            store.set_meta(
                &gemma_cursor(
                    super::runtime::EmbeddingVariantId::Q4KM
                        .descriptor()
                        .model_key,
                ),
                &json!(0),
            )?;
            store.set_meta("rust-vector-rebuild-needed-v1", &json!(false))?;
        }
        // One-time index repair, never on launcher status ticks.
        let repaired = store.meta("rust-fts-rowids-v1", json!(false))?;
        if repaired != true {
            store.db.execute_batch("DELETE FROM knowledge_sources_fts; INSERT INTO knowledge_sources_fts(rowid,id,path) SELECT rowid,id,path FROM knowledge_sources; DELETE FROM knowledge_chunks_fts; INSERT INTO knowledge_chunks_fts(rowid,id,source_id,text) SELECT rowid,id,source_id,text FROM knowledge_chunks;").map_err(err)?;
            store.set_meta("rust-fts-rowids-v1", &json!(true))?;
        }
        // Foreign keys and the compatibility triggers both preserve derived deletions.
        Ok(store)
    }
    pub fn meta(&self, key: &str, fallback: Value) -> Result<Value, String> {
        let s: Option<String> = self
            .db
            .query_row(
                "SELECT value_json FROM knowledge_metadata WHERE key=?",
                [key],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?;
        Ok(s.and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(fallback))
    }
    pub fn set_meta(&self, key: &str, value: &Value) -> Result<(), String> {
        self.db.execute("INSERT INTO knowledge_metadata(key,value_json) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json",params![key,value.to_string()]).map_err(err)?;
        Ok(())
    }
    pub fn settings(&self) -> Result<Value, String> {
        let mut settings = default_settings();
        if let Some(map) = self.meta("settings", json!({}))?.as_object() {
            for (k, v) in map {
                settings[k] = v.clone();
            }
        }
        if !["off", "basic", "smart", "deep"].contains(&settings["depth"].as_str().unwrap_or("")) {
            settings["depth"] = json!("smart");
        }
        if !["built-in", "embeddinggemma-2"]
            .contains(&settings["embeddingModel"].as_str().unwrap_or(""))
        {
            settings["embeddingModel"] = json!("built-in");
        }
        if !["q8_0", "q4_k_m"].contains(&settings["embeddingVariant"].as_str().unwrap_or("")) {
            settings["embeddingVariant"] = json!("q8_0");
        }
        if !["slow", "medium", "fast"].contains(&settings["indexingSpeed"].as_str().unwrap_or("")) {
            settings["indexingSpeed"] = json!("medium");
        }
        Ok(settings)
    }
    pub fn counts(&self) -> Result<Value, String> {
        self.db.query_row("SELECT source_count,chunk_count,indexed_page_count,total_page_count,partial_source_count,source_bytes FROM knowledge_stats WHERE id=1",[],|r|Ok(json!({"sourceCount":r.get::<_,i64>(0)?,"chunkCount":r.get::<_,i64>(1)?,"indexedPageCount":r.get::<_,i64>(2)?,"totalPageCount":r.get::<_,i64>(3)?,"partialSourceCount":r.get::<_,i64>(4)?,"sourceBytes":r.get::<_,i64>(5)?}))).map_err(err)
    }
    pub fn roots(&self) -> Result<Vec<Value>, String> {
        let mut q=self.db.prepare("SELECT id,path,depth,processing_backend,enabled,created_at,updated_at FROM knowledge_roots ORDER BY created_at").map_err(err)?;
        let result=q.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"path":r.get::<_,String>(1)?,"depth":r.get::<_,String>(2)?,"processingBackend":r.get::<_,String>(3)?,"enabled":r.get::<_,bool>(4)?,"createdAt":r.get::<_,i64>(5)?,"updatedAt":r.get::<_,i64>(6)?}))).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
        Ok(result)
    }
    pub fn storage(&self) -> u64 {
        let s = self.path.to_string_lossy();
        [
            self.path.clone(),
            PathBuf::from(format!("{s}-wal")),
            PathBuf::from(format!("{s}-shm")),
        ]
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum()
    }
    pub fn root_add(&self, path: &Path) -> Result<(), String> {
        let path = normalize_root(path)?;
        if !path.is_dir() {
            return Err("Choose an existing folder".into());
        }
        let now = now_ms();
        self.db.execute("INSERT INTO knowledge_roots(id,path,depth,processing_backend,enabled,created_at,updated_at) VALUES(?,?,'inherit','local',1,?,?) ON CONFLICT(path) DO NOTHING",params![uuid::Uuid::new_v4().to_string(),path.to_string_lossy(),now,now]).map_err(err)?;
        Ok(())
    }
    pub fn root_remove(&mut self, id: &str) -> Result<(), String> {
        self.db.execute_batch("BEGIN IMMEDIATE").map_err(err)?;
        let result = (|| {
            for source in self.source_ids(id)? {
                self.delete_content(&source)?;
                self.db
                    .execute("DELETE FROM knowledge_sources WHERE id=?", [source])
                    .map_err(err)?;
            }
            self.db
                .execute("DELETE FROM knowledge_roots WHERE id=?", [id])
                .map_err(err)?;
            Ok(())
        })();
        match result {
            Ok(()) => self.db.execute_batch("COMMIT").map_err(err),
            Err(e) => {
                let _ = self.db.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }
    fn source_ids(&self, root: &str) -> Result<Vec<String>, String> {
        let mut s = self
            .db
            .prepare("SELECT id FROM knowledge_sources WHERE root_id=?")
            .map_err(err)?;
        let rows = s
            .query_map([root], |r| r.get(0))
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        Ok(rows)
    }
    pub fn clear_checkpoint(&self) -> Result<(), String> {
        self.db
            .execute_batch(
                "DELETE FROM knowledge_indexing_queue;DELETE FROM knowledge_indexing_jobs;",
            )
            .map_err(err)
    }
    pub fn checkpoint(&self) -> Result<Option<Value>, String> {
        let job=self.db.query_row("SELECT job_id,total_sources,processed_sources,failed_sources FROM knowledge_indexing_jobs WHERE id=1",[],|r|Ok(json!({"jobId":r.get::<_,String>(0)?,"totalSources":r.get::<_,i64>(1)?,"processedSources":r.get::<_,i64>(2)?,"failedSources":r.get::<_,i64>(3)?}))).optional().map_err(err)?;
        Ok(job)
    }
    pub fn save_queue(&mut self, roots_paths: &[(String, String)]) -> Result<(), String> {
        let tx = self.db.transaction().map_err(err)?;
        tx.execute_batch(
            "DELETE FROM knowledge_indexing_queue;DELETE FROM knowledge_indexing_jobs;",
        )
        .map_err(err)?;
        let job = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO knowledge_indexing_jobs VALUES(1,?,?,0,0,?)",
            params![job, roots_paths.len() as i64, now_ms()],
        )
        .map_err(err)?;
        for (position, (root, path)) in roots_paths.iter().enumerate() {
            tx.execute(
                "INSERT INTO knowledge_indexing_queue VALUES(?,?,?,?)",
                params![job, position as i64, root, path],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)
    }
    pub fn next_batch(&self, limit: usize) -> Result<Vec<(String, i64, String, String)>, String> {
        let mut q=self.db.prepare("SELECT job_id,position,root_id,path FROM knowledge_indexing_queue WHERE job_id=(SELECT job_id FROM knowledge_indexing_jobs WHERE id=1) ORDER BY position LIMIT ?").map_err(err)?;
        let rows = q
            .query_map([limit as i64], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        Ok(rows)
    }
    pub fn complete(&mut self, job: &str, position: i64, failed: bool) -> Result<(), String> {
        let tx = self.db.transaction().map_err(err)?;
        let n = tx
            .execute(
                "DELETE FROM knowledge_indexing_queue WHERE job_id=? AND position=?",
                params![job, position],
            )
            .map_err(err)?;
        if n > 0 {
            tx.execute("UPDATE knowledge_indexing_jobs SET processed_sources=processed_sources+1,failed_sources=failed_sources+? WHERE id=1 AND job_id=?",params![failed as i64,job]).map_err(err)?;
        }
        tx.commit().map_err(err)
    }
    pub fn source(&self, path: &str) -> Result<Option<Value>, String> {
        self.db.query_row("SELECT id,content_hash,byte_size,modified_at,status,indexing_profile FROM knowledge_sources WHERE path=?",[path],|r|Ok(json!({"id":r.get::<_,String>(0)?,"contentHash":r.get::<_,String>(1)?,"byteSize":r.get::<_,u64>(2)?,"modifiedAt":r.get::<_,f64>(3)?,"status":r.get::<_,String>(4)?,"indexingProfile":r.get::<_,String>(5)?}))).optional().map_err(err)
    }
    pub fn pending(
        &self,
        source: &str,
        root: &str,
        path: &str,
        fingerprint: &Value,
        profile: &str,
    ) -> Result<(), String> {
        self.db.execute("INSERT INTO knowledge_sources(id,root_id,path,content_hash,byte_size,modified_at,media_type,status,indexing_profile) VALUES(?,?,?,?,?,?,?,'pending',?) ON CONFLICT(id) DO UPDATE SET root_id=excluded.root_id,content_hash=excluded.content_hash,byte_size=excluded.byte_size,modified_at=excluded.modified_at,indexing_profile=excluded.indexing_profile,status='pending',error=NULL",params![source,root,path,string(fingerprint,"contentHash"),number(fingerprint,"byteSize"),fingerprint["modifiedAt"].as_f64().unwrap_or(0.),super::extract::extension(Path::new(path)),profile]).map_err(err)?;
        Ok(())
    }
    pub fn failed(&self, source: &str, error: &str) -> Result<(), String> {
        self.db
            .execute(
                "UPDATE knowledge_sources SET status='failed',error=?,indexed_at=? WHERE id=?",
                params![text::truncate(error, 2000), now_ms(), source],
            )
            .map_err(err)?;
        Ok(())
    }
    fn delete_content(&self, id: &str) -> Result<(), String> {
        if self.vector {
            self.db.execute("DELETE FROM knowledge_chunk_vectors_binary WHERE rowid IN (SELECT rowid FROM knowledge_chunks WHERE source_id=?)",[id]).map_err(err)?;
        }
        self.db.execute("DELETE FROM knowledge_chunks_fts WHERE rowid IN (SELECT rowid FROM knowledge_chunks WHERE source_id=?) AND source_id=?",params![id,id]).map_err(err)?;
        for table in ["knowledge_pages", "knowledge_images", "knowledge_chunks"] {
            self.db
                .execute(&format!("DELETE FROM {table} WHERE source_id=?"), [id])
                .map_err(err)?;
        }
        Ok(())
    }
    pub fn remove_source(&mut self, id: &str) -> Result<(), String> {
        self.db.execute_batch("BEGIN IMMEDIATE").map_err(err)?;
        let result = (|| {
            self.delete_content(id)?;
            self.db
                .execute("DELETE FROM knowledge_sources WHERE id=?", [id])
                .map_err(err)?;
            Ok(())
        })();
        match result {
            Ok(()) => self.db.execute_batch("COMMIT").map_err(err),
            Err(e) => {
                let _ = self.db.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }
    pub fn prune(
        &mut self,
        root: &str,
        present: &std::collections::HashSet<String>,
    ) -> Result<(), String> {
        for id in self.source_ids(root)? {
            let path: String = self
                .db
                .query_row(
                    "SELECT path FROM knowledge_sources WHERE id=?",
                    [&id],
                    |r| r.get(0),
                )
                .map_err(err)?;
            if !present.contains(&path) {
                self.remove_source(&id)?;
            }
        }
        Ok(())
    }
    pub fn save_result(&mut self, id: &str, result: &Value) -> Result<(), String> {
        self.db.execute_batch("BEGIN IMMEDIATE").map_err(err)?;
        let save = (|| {
            self.delete_content(id)?;
            let pages = result["pages"].as_array().ok_or("Missing indexed pages")?;
            let chunks = result["chunks"]
                .as_array()
                .ok_or("Missing indexed chunks")?;
            for page in pages {
                let extracted = page["extractedText"].as_str();
                let ocr = page["ocr"]["text"].as_str();
                let page_no = number(page, "pageNumber");
                let embedded = chunks
                    .iter()
                    .any(|c| number(c, "pageNumber") == page_no && c["embedding"].is_array());
                self.db.execute("INSERT INTO knowledge_pages(source_id,page_number,extracted_text,ocr_text,ocr_blocks_json,ocr_confidence,extraction_status,ocr_status,embedding_status,content_hash) VALUES(?,?,?,?,?,?,?,?,?,?)",params![id,page_no,extracted,ocr,page.get("ocr").map(|o|o["blocks"].to_string()),page["ocr"]["averageConfidence"].as_f64(),if extracted.unwrap_or("").is_empty(){"metadata-only"}else{"text-indexed"},if ocr.is_some(){"ocr-indexed"}else{"not-indexed"},if embedded{"embedded"}else{"not-indexed"},text::hash(format!("{}\n{}",extracted.unwrap_or(""),ocr.unwrap_or("")).trim())]).map_err(err)?;
            }
            let root: String = self
                .db
                .query_row(
                    "SELECT root_id FROM knowledge_sources WHERE id=?",
                    [id],
                    |r| r.get(0),
                )
                .map_err(err)?;
            for c in chunks {
                let embedding: Vec<f64> =
                    serde_json::from_value(c["embedding"].clone()).unwrap_or_default();
                let bytes = if embedding.is_empty() {
                    None
                } else {
                    Some(text::encode(&embedding))
                };
                self.db.execute("INSERT INTO knowledge_chunks(id,source_id,page_number,text,embedding_json,start_offset,end_offset) VALUES(?,?,?,?,?,?,?)",params![string(c,"id"),id,c["pageNumber"].as_i64(),string(c,"text"),bytes,c["startOffset"].as_i64(),c["endOffset"].as_i64()]).map_err(err)?;
                let row = self.db.last_insert_rowid();
                self.db
                    .execute(
                        "INSERT INTO knowledge_chunks_fts(rowid,id,source_id,text) VALUES(?,?,?,?)",
                        params![row, string(c, "id"), id, string(c, "text")],
                    )
                    .map_err(err)?;
                if self.vector && embedding.len() == 384 {
                    self.db.execute("INSERT INTO knowledge_chunk_vectors_binary(rowid,embedding,root_id) VALUES(?,vec_bit(?),?)",params![row,text::binary(&embedding),root]).map_err(err)?;
                }
            }
            for image in result["images"].as_array().into_iter().flatten() {
                self.db.execute("INSERT INTO knowledge_images(id,source_id,page_number,ocr_text,description,embedding_json,width,height) VALUES(?,?,?,?,?,?,?,?)",params![string(image,"id"),id,image["pageNumber"].as_i64(),image["ocrText"].as_str(),image["description"].as_str(),image.get("embedding").map(Value::to_string),number(image,"width"),number(image,"height")]).map_err(err)?;
            }
            self.db.execute("UPDATE knowledge_sources SET status='indexed',error=NULL,total_pages=?,indexed_pages=?,indexed_at=? WHERE id=?",params![result["totalPageCount"].as_u64().unwrap_or(pages.len() as u64),pages.len() as i64,now_ms(),id]).map_err(err)?;
            let source_hash: String = self
                .db
                .query_row(
                    "SELECT content_hash FROM knowledge_sources WHERE id=?",
                    [id],
                    |r| r.get(0),
                )
                .map_err(err)?;
            for capability in result["completedCapabilities"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                let (processor, version, processor_json) = if capability == "text-embeddings" {
                    ("tezbar-multilingual-feature-v1","1.0.0",r#"{"id":"tezbar-multilingual-feature-v1","version":"1.0.0","dimensions":384}"#.to_owned())
                } else {
                    let id = result["extractor"]["id"].as_str().unwrap_or("plain-text");
                    let version = result["extractor"]["version"].as_str().unwrap_or("1.0.0");
                    (
                        id,
                        version,
                        format!("{{\"id\":{},\"version\":{}}}", json!(id), json!(version)),
                    )
                };
                let settings_hash = text::hash(format!(
                    "{{\"capability\":{},\"processor\":{processor_json}}}",
                    json!(capability)
                ));
                let artifact_id = text::hash(format!(
                    "{source_hash}:{capability}:{processor}:{version}:{settings_hash}"
                ));
                self.db
                    .execute(
                        "INSERT OR IGNORE INTO knowledge_artifacts VALUES(?,?,?,?,?,?,?)",
                        params![
                            artifact_id,
                            source_hash,
                            capability,
                            processor,
                            version,
                            settings_hash,
                            now_ms()
                        ],
                    )
                    .map_err(err)?;
            }
            Ok(())
        })();
        match save {
            Ok(()) => self.db.execute_batch("COMMIT").map_err(err),
            Err(e) => {
                let _ = self.db.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }
    pub fn reusable(&self, hash: &str, id: &str, profile: &str) -> Result<Option<Value>, String> {
        let source:Option<(String,i64)>=self.db.query_row("SELECT id,COALESCE(total_pages,0) FROM knowledge_sources WHERE content_hash=? AND indexing_profile=? AND status='indexed' AND id<>? ORDER BY indexed_at DESC LIMIT 1",params![hash,profile,id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(err)?;
        let Some((source, total)) = source else {
            return Ok(None);
        };
        let mut q=self.db.prepare("SELECT page_number,extracted_text,ocr_text,ocr_blocks_json,ocr_confidence FROM knowledge_pages WHERE source_id=? ORDER BY page_number").map_err(err)?;
        let pages=q.query_map([&source],|r|{let ocr=optional_content_text(r,2)?;let mut p=json!({"pageNumber":r.get::<_,i64>(0)?,"extractedText":optional_content_text(r,1)?});if let Some(ocr)=ocr{p["ocr"]=json!({"text":ocr,"blocks":r.get::<_,Option<String>>(3)?.and_then(|s|serde_json::from_str::<Value>(&s).ok()).unwrap_or(json!([])),"averageConfidence":r.get::<_,Option<f64>>(4)?});}Ok(p)}).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
        let mut q=self.db.prepare("SELECT id,page_number,text,embedding_json,start_offset,end_offset FROM knowledge_chunks WHERE source_id=? ORDER BY rowid").map_err(err)?;
        let chunks=q.query_map([&source],|r|{let old:String=r.get(0)?;let v=text::decode(r.get_ref(3)?);let mut c=json!({"id":text::hash(format!("{id}:{old}")),"pageNumber":r.get::<_,Option<i64>>(1)?,"text":content_text(r,2)?,"startOffset":r.get::<_,Option<i64>>(4)?,"endOffset":r.get::<_,Option<i64>>(5)?});if !v.is_empty(){c["embedding"]=json!(v);}Ok(c)}).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
        let mut q=self.db.prepare("SELECT id,page_number,ocr_text,description,embedding_json,width,height FROM knowledge_images WHERE source_id=? ORDER BY rowid").map_err(err)?;
        let images=q.query_map([&source],|r|{let old:String=r.get(0)?;Ok(json!({"id":text::hash(format!("{id}:{old}")),"pageNumber":r.get::<_,Option<i64>>(1)?,"ocrText":optional_content_text(r,2)?,"description":optional_content_text(r,3)?,"embedding":text::decode(r.get_ref(4)?),"width":r.get::<_,i64>(5)?,"height":r.get::<_,i64>(6)?}))}).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
        Ok(Some(
            json!({"pages":pages,"chunks":chunks,"images":images,"totalPageCount":total,"extractor":{"id":"content-addressed-cache","version":"1.0.0"},"completedCapabilities":["extracted-text","chunks"],"warnings":["Reused locally cached artifacts for identical content."]}),
        ))
    }
    pub fn list_sources(&self, input: Value) -> Result<Value, String> {
        let query = input["query"].as_str().unwrap_or("").trim();
        let offset = input["offset"].as_f64().unwrap_or(0.).round().max(0.) as i64;
        let limit = input["limit"]
            .as_f64()
            .unwrap_or(200.)
            .round()
            .clamp(1., 500.) as i64;
        let pattern = format!("%{query}%");
        let total: i64 = self
            .db
            .query_row(
                "SELECT COUNT(*) FROM knowledge_sources WHERE (?='' OR path LIKE ?)",
                params![query, pattern],
                |r| r.get(0),
            )
            .map_err(err)?;
        let mut q=self.db.prepare("SELECT id,path,byte_size,modified_at,indexed_at,status,error,COALESCE(total_pages,0),indexed_pages FROM knowledge_sources WHERE (?='' OR path LIKE ?) ORDER BY CASE status WHEN 'pending' THEN 0 WHEN 'failed' THEN 1 ELSE 2 END,indexed_at DESC,path ASC LIMIT ? OFFSET ?").map_err(err)?;
        let rows=q.query_map(params![query,pattern,limit,offset],|r|{let path:String=r.get(1)?;let mut v=json!({"id":r.get::<_,String>(0)?,"title":title(&path),"path":path,"byteSize":r.get::<_,u64>(2)?,"modifiedAt":r.get::<_,f64>(3)?,"status":r.get::<_,String>(5)?,"totalPageCount":r.get::<_,i64>(7)?,"indexedPageCount":r.get::<_,i64>(8)?});if let Some(at)=r.get::<_,Option<i64>>(4)?{v["indexedAt"]=json!(at);}if let Some(error)=r.get::<_,Option<String>>(6)?{v["error"]=json!(error);}Ok(v)}).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
        Ok(
            json!({"hasMore":(offset + rows.len() as i64) < total,"sources":rows,"total":total,"offset":offset}),
        )
    }
    pub fn read(&self, id: &str, max: usize, roots: &[String]) -> Result<Value, String> {
        let selected:Option<(i64,String,Option<i64>,String,String)>=self.db.query_row("SELECT c.rowid,c.source_id,c.page_number,s.path,s.root_id FROM knowledge_chunks c JOIN knowledge_sources s ON s.id=c.source_id JOIN knowledge_roots r ON r.id=s.root_id WHERE c.id=? AND r.enabled=1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(err)?;
        let Some((row, source, page, path, root)) = selected else {
            return Ok(Value::Null);
        };
        if !roots.contains(&root) {
            return Ok(Value::Null);
        }
        let mut q=self.db.prepare("SELECT text FROM knowledge_chunks WHERE source_id=? AND rowid BETWEEN ? AND ? ORDER BY rowid").map_err(err)?;
        let texts = q
            .query_map(params![source, row - 2, row + 2], |r| content_text(r, 0))
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        self.db
            .execute(
                "UPDATE knowledge_pages SET last_accessed_at=? WHERE source_id=? AND page_number=?",
                params![now_ms(), source, page.unwrap_or(1)],
            )
            .map_err(err)?;
        let mut v = json!({"resultId":id,"sourceId":source,"path":path,"title":title(&path),"text":text::truncate(&texts.join("\n\n"),max.max(500))});
        if let Some(p) = page {
            v["pageNumber"] = json!(p);
        }
        Ok(v)
    }
    pub fn search_metadata(
        &self,
        query: &str,
        limit: usize,
        roots: &[String],
    ) -> Result<Value, String> {
        let fts = content_fts_query(query);
        if fts.is_empty() || limit == 0 || roots.is_empty() {
            return Ok(json!([]));
        }
        let placeholders = vec!["?"; roots.len()].join(",");
        let sql = format!("SELECT s.id,s.path FROM knowledge_sources_fts f JOIN knowledge_sources s ON s.rowid=f.rowid JOIN knowledge_roots r ON r.id=s.root_id WHERE knowledge_sources_fts MATCH ? AND r.enabled=1 AND r.id IN ({placeholders}) ORDER BY bm25(knowledge_sources_fts) LIMIT ?");
        let mut values = vec![rusqlite::types::Value::Text(fts)];
        values.extend(roots.iter().cloned().map(rusqlite::types::Value::Text));
        values.push(rusqlite::types::Value::Integer(limit as i64));
        let mut statement = self.db.prepare(&sql).map_err(err)?;
        let mut rows = statement
            .query_map(rusqlite::params_from_iter(values), |row| {
                let path: String = row.get(1)?;
                Ok(json!({"sourceId":row.get::<_,String>(0)?, "title":title(&path), "path":path}))
            })
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        let count = rows.len().max(1) as f64;
        for (index, row) in rows.iter_mut().enumerate() {
            row["score"] = json!((1.0 - index as f64 / count).max(0.2));
        }
        Ok(json!(rows))
    }

    pub fn search(
        &self,
        query: &str,
        limit: usize,
        roots: &[String],
        lexical_only: bool,
        learned: Option<(&[f64], &[String])>,
    ) -> Result<Vec<Value>, String> {
        self.search_with_model(
            query,
            limit,
            roots,
            lexical_only,
            learned.map(|(v, r)| (v, r, MODEL_KEY)),
        )
    }

    pub(crate) fn search_with_model(
        &self,
        query: &str,
        limit: usize,
        roots: &[String],
        lexical_only: bool,
        learned: Option<(&[f64], &[String], &str)>,
    ) -> Result<Vec<Value>, String> {
        self.search_ranked(query, limit, roots, lexical_only, learned, false)
    }

    /// Launcher results are files: select source representatives in both lanes
    /// before candidate limits, then keep the best chunk per source after fusion.
    pub fn search_sources(
        &self,
        query: &str,
        limit: usize,
        roots: &[String],
        lexical_only: bool,
        learned: Option<(&[f64], &[String])>,
    ) -> Result<Vec<Value>, String> {
        self.search_sources_with_model(
            query,
            limit,
            roots,
            lexical_only,
            learned.map(|(v, r)| (v, r, MODEL_KEY)),
        )
    }

    /// Exact persisted model identity, ready for the variant registry. The
    /// compatibility entry point above continues to use the existing Q8 key.
    pub(crate) fn search_sources_with_model(
        &self,
        query: &str,
        limit: usize,
        roots: &[String],
        lexical_only: bool,
        learned: Option<(&[f64], &[String], &str)>,
    ) -> Result<Vec<Value>, String> {
        self.search_checkpoint()?;
        self.db
            .execute_batch("SAVEPOINT knowledge_source_search")
            .map_err(err)?;
        let result = self.search_ranked(query, limit, roots, lexical_only, learned, true);
        let release = self
            .db
            .execute_batch("RELEASE knowledge_source_search")
            .map_err(err);
        result.and_then(|hits| {
            release?;
            self.search_checkpoint()?;
            Ok(hits)
        })
    }

    fn search_ranked(
        &self,
        query: &str,
        limit: usize,
        roots: &[String],
        lexical_only: bool,
        learned: Option<(&[f64], &[String], &str)>,
        distinct_sources: bool,
    ) -> Result<Vec<Value>, String> {
        use std::collections::{HashMap, HashSet};
        self.search_checkpoint()?;
        let exact_model_key = learned.map(|(_, _, key)| key).unwrap_or(MODEL_KEY);
        let model_key = exact_model_key.replace('\'', "''");
        let learned_filter = learned.map(|(_, ids, _)| {
            let mut ids = ids.to_vec();
            ids.sort();
            ids.dedup();
            ids.iter()
                .map(|id| format!("'{}'", id.replace('\'', "''")))
                .collect::<Vec<_>>()
                .join(",")
        });
        let learned = learned.map(|(vector, _, _)| vector);
        let learned_table = gemma_vector_table(exact_model_key);
        let use_knn = self.vector && (learned.is_none() || learned_table.is_some());
        if query.trim().is_empty() || limit == 0 || roots.is_empty() {
            return Ok(vec![]);
        }
        let fts = content_fts_query(query);
        let mut canonical_roots = roots.to_vec();
        canonical_roots.sort();
        canonical_roots.dedup();
        let root_filter = canonical_roots
            .iter()
            .map(|id| format!("'{}'", id.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        let mut candidates: HashMap<String, (Value, Vec<f64>, f64)> = HashMap::new();
        let mut lexical = vec![];
        let mut exact = HashSet::new();
        // A separate indexed phrase query cannot be crowded out by short,
        // high-BM25 matches in the normal prefix/AND candidate window.
        let phrase = content_fts_phrase(query);
        for (fts_query, is_exact) in [(phrase.as_str(), true), (fts.as_str(), false)] {
            if fts_query.is_empty() {
                continue;
            }
            let sizes = if self.rank_join { "LEFT JOIN knowledge_chunks_fts_docsize sz ON sz.id=f.rowid" } else { "" };
            let matches = format!("FROM knowledge_chunks_fts f JOIN knowledge_chunks c ON c.rowid=f.rowid JOIN knowledge_sources s ON s.id=c.source_id JOIN knowledge_roots r ON r.id=s.root_id {sizes} WHERE knowledge_chunks_fts MATCH ? AND s.status='indexed' AND r.enabled=1 AND r.id IN ({root_filter})");
            let budget = limit.saturating_mul(5).max(40);
            let rows = if distinct_sources {
                self.lexical_source_candidates(&matches, fts_query, budget)?
            } else {
                let rank = self.lexical_rank();
                let sql = format!("SELECT c.id,c.source_id,c.page_number,c.text,s.path,c.embedding_json {matches} ORDER BY {rank},c.id LIMIT ?");
                let mut q = self.db.prepare(&sql).map_err(err)?;
                let rows = q
                    .query_map(params![fts_query, budget as i64], hit_row)
                    .map_err(err)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(err)?;
                rows
            };
            for (hit, v) in rows {
                let id = string(&hit, "chunkId").to_owned();
                if is_exact {
                    exact.insert(id.clone());
                }
                if !candidates.contains_key(&id) {
                    lexical.push(id.clone());
                    candidates.insert(id, (hit, v, 0.));
                }
            }
        }
        let len = lexical.len().max(1) as f64;
        for (i, id) in lexical.iter().enumerate() {
            candidates.get_mut(id).unwrap().2 = (1. - i as f64 / len).max(0.15);
        }
        if !lexical_only {
            let requested_roots = &root_filter;
            let root_filter = learned_filter.as_deref().unwrap_or(requested_roots);
            let sql = if let Some(vector) = learned {
                if vector.len() != 256 {
                    return Err("Invalid vector for the Gemma index".into());
                }
                if use_knn {
                    let table = learned_table.unwrap();
                    format!("WITH nearest AS (SELECT rowid,distance FROM {table} WHERE embedding MATCH ? AND k=? AND root_id IN ({root_filter})) SELECT c.id,c.source_id,c.page_number,c.text,s.path,g.vector FROM nearest JOIN knowledge_chunks c ON c.rowid=nearest.rowid JOIN knowledge_sources s ON s.id=c.source_id JOIN knowledge_gemma_embeddings g ON g.chunk_id=c.id JOIN knowledge_roots r ON r.id=s.root_id WHERE s.status='indexed' AND r.enabled=1 AND g.model_key='{model_key}' ORDER BY nearest.distance")
                } else {
                    format!("SELECT c.id,c.source_id,c.page_number,c.text,s.path,g.vector FROM knowledge_gemma_embeddings g JOIN knowledge_chunks c ON c.id=g.chunk_id JOIN knowledge_sources s ON s.id=c.source_id JOIN knowledge_roots r ON r.id=s.root_id WHERE g.model_key='{model_key}' AND s.status='indexed' AND r.enabled=1 AND r.id IN ({root_filter})")
                }
            } else if self.vector {
                format!("WITH nearest AS (SELECT rowid,distance FROM knowledge_chunk_vectors_binary WHERE embedding MATCH vec_bit(?) AND k=? AND root_id IN ({root_filter})) SELECT c.id,c.source_id,c.page_number,c.text,s.path,c.embedding_json FROM nearest JOIN knowledge_chunks c ON c.rowid=nearest.rowid JOIN knowledge_sources s ON s.id=c.source_id JOIN knowledge_roots r ON r.id=s.root_id WHERE s.status='indexed' AND r.enabled=1 ORDER BY nearest.distance")
            } else {
                format!("SELECT c.id,c.source_id,c.page_number,c.text,s.path,c.embedding_json FROM knowledge_chunks c JOIN knowledge_sources s ON s.id=c.source_id JOIN knowledge_roots r ON r.id=s.root_id WHERE c.embedding_json IS NOT NULL AND s.status='indexed' AND r.enabled=1 AND r.id IN ({root_filter}) ORDER BY c.rowid DESC LIMIT 1200")
            };
            let query_vector = learned
                .map(|v| v.to_vec())
                .unwrap_or_else(|| text::embed(query));
            let mut semantic = vec![];
            let mut add_candidate = |(hit, v): (Value, Vec<f64>)| {
                let similarity = text::cosine(&query_vector, &v).max(0.);
                let id = string(&hit, "chunkId").to_owned();
                semantic.push((id.clone(), similarity));
                let score = candidates.get(&id).map(|t| t.2).unwrap_or(0.);
                candidates.insert(id, (hit, v, score));
            };
            if distinct_sources {
                for candidate in self.semantic_source_candidates(
                    &query_vector,
                    limit,
                    root_filter,
                    requested_roots,
                    learned.map(|_| exact_model_key),
                )? {
                    add_candidate(candidate);
                }
            } else {
                let mut q = self.db.prepare(&sql).map_err(err)?;
                let mut rows = if use_knn {
                    q.query(params![
                        if learned.is_some() {
                            text::encode(&query_vector)
                        } else {
                            text::binary(&query_vector)
                        },
                        limit.saturating_mul(32).max(320) as i64
                    ])
                    .map_err(err)?
                } else {
                    q.query([]).map_err(err)?
                };
                while let Some(row) = rows.next().map_err(err)? {
                    self.search_checkpoint()?;
                    add_candidate(hit_row(row).map_err(err)?);
                }
            }
            semantic.sort_by(|a, b| {
                b.1.total_cmp(&a.1).then_with(|| {
                    if distinct_sources {
                        a.0.cmp(&b.0)
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
            });
            if learned.is_some() {
                let mut hits = vec![];
                for (id, (mut hit, _, ls)) in candidates {
                    let lr = lexical.iter().position(|v| v == &id);
                    let sr = semantic.iter().position(|v| v.0 == id);
                    let sim = sr.map(|i| semantic[i].1).unwrap_or(0.);
                    let score = lr.map(|i| 30. / (61 + i) as f64).unwrap_or(0.)
                        + sr.map(|i| 30. / (61 + i) as f64).unwrap_or(0.);
                    hit["score"] = json!(score);
                    hit["lexicalScore"] = json!(ls);
                    hit["semanticScore"] = json!(sim);
                    hits.push(hit);
                }
                sort_hits(&mut hits, limit, &exact, distinct_sources);
                return Ok(hits);
            }
        }
        let vector = text::embed(query);
        let mut hits = vec![];
        for (_, (mut hit, v, ls)) in candidates {
            let sim = if lexical_only {
                0.
            } else {
                text::cosine(&vector, &v).max(0.)
            };
            let score = ls * 0.72 + sim * 0.28;
            if score <= 0.08 {
                continue;
            }
            hit["score"] = json!(score);
            hit["lexicalScore"] = json!(ls);
            hit["semanticScore"] = json!(sim);
            hits.push(hit);
        }
        sort_hits(&mut hits, limit, &exact, distinct_sources);
        Ok(hits)
    }
    fn lexical_rank(&self) -> &'static str {
        if self.rank_join { "tezbar_bm25(knowledge_chunks_fts,sz.sz,1.0)" } else { "bm25(knowledge_chunks_fts,1.0)" }
    }
    /// Reduce matching IDs to their exact best chunk/source before hydrating.
    /// The first row/source in global (BM25, chunk ID) order is exactly the
    /// window-function winner. Stop at the source budget, never a raw chunk cap:
    /// arbitrarily many chunks from one source cannot starve the next source.
    fn lexical_source_candidates(
        &self,
        matches: &str,
        query: &str,
        budget: usize,
    ) -> Result<Vec<(Value, Vec<f64>)>, String> {
        let mut seen = std::collections::HashSet::<String>::new();
        let mut selected = Vec::new();
        let rank = self.lexical_rank();
        let mut stmt = self
            .db
            .prepare(&format!(
                "SELECT c.rowid,c.source_id {matches} ORDER BY {rank},c.id"
            ))
            .map_err(err)?;
        let mut rows = stmt.query([query]).map_err(err)?;
        let mut count = 0;
        while let Some(row) = rows.next().map_err(err)? {
            if count % 256 == 0 {
                self.search_checkpoint()?;
            }
            count += 1;
            let source: String = row.get(1).map_err(err)?;
            if seen.insert(source) {
                selected.push(row.get::<_, i64>(0).map_err(err)?);
                if selected.len() == budget {
                    break;
                }
            }
        }
        let mut hydrate = self.db.prepare_cached("SELECT c.id,c.source_id,c.page_number,c.text,s.path,c.embedding_json FROM knowledge_chunks c JOIN knowledge_sources s ON s.id=c.source_id WHERE c.rowid=?").map_err(err)?;
        selected
            .into_iter()
            .map(|rowid| {
                self.search_checkpoint()?;
                hydrate.query_row([rowid], hit_row).map_err(err)
            })
            .collect()
    }
    /// Exact source representatives before the unchanged 80@16 budget. Cache
    /// readiness is proven by a complete authoritative scan plus transactional
    /// deltas, never by accelerated-row counts or a Hamming/KNN shortlist.
    fn semantic_source_candidates(
        &self,
        vector: &[f64],
        limit: usize,
        semantic_roots: &str,
        requested_roots: &str,
        model_key: Option<&str>,
    ) -> Result<Vec<(Value, Vec<f64>)>, String> {
        let key = search_cache::Key {
            model: model_key.map(str::to_owned),
            dimensions: vector.len(),
            semantic_roots: semantic_roots.to_owned(),
            requested_roots: requested_roots.to_owned(),
        };
        let winners = self.exact_source_winners(&key, vector, limit.saturating_mul(5).max(40))?;
        let embedding = key.embedding();
        let gemma_join = key.gemma_join();
        let model_filter = key
            .model
            .as_ref()
            .map(|m| format!(" AND g.model_key='{}'", m.replace('\'', "''")))
            .unwrap_or_default();
        let mut stmt = self.db.prepare(&format!("SELECT c.id,c.source_id,c.page_number,c.text,s.path,{embedding} FROM knowledge_chunks c JOIN knowledge_sources s ON s.id=c.source_id {gemma_join} WHERE c.rowid=?{model_filter}")).map_err(err)?;
        winners
            .into_iter()
            .map(|(rowid, _, _)| stmt.query_row([rowid], hit_row).map_err(err))
            .collect()
    }

    pub fn gemma_pending(&self, roots: &[String], limit: usize) -> Result<Vec<Value>, String> {
        self.gemma_pending_for_model(roots, limit, MODEL_KEY)
    }
    pub fn gemma_pending_for_model(
        &self,
        roots: &[String],
        limit: usize,
        model_key: &str,
    ) -> Result<Vec<Value>, String> {
        if roots.is_empty() {
            return Ok(vec![]);
        }
        let filter = roots
            .iter()
            .map(|s| format!("'{}'", s.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        let mut q=self.db.prepare(&format!("SELECT c.rowid,c.id,c.text,s.path FROM knowledge_chunks c CROSS JOIN knowledge_sources s LEFT JOIN knowledge_gemma_embeddings g ON g.chunk_id=c.id AND g.model_key=? WHERE s.id=c.source_id AND s.status='indexed' AND c.embedding_json IS NOT NULL AND s.root_id IN ({filter}) AND g.chunk_id IS NULL ORDER BY c.rowid LIMIT ?")).map_err(err)?;
        let rows = q.query_map(params![model_key, limit as i64], |r| {
            let path: String = r.get(3)?;
            let raw = content_bytes(r, 2)?;
            // Internal only: carry authoritative bytes across inference without
            // exposing them or rewriting the legacy source. Never hash lossy text.
            Ok(json!({"rowId":r.get::<_,i64>(0)?,"id":r.get::<_,String>(1)?,"text":String::from_utf8_lossy(raw),"title":title(&path),"_textFingerprint":text::hash(raw)}))
        }).map_err(err)?.collect::<Result<Vec<_>, _>>().map_err(err)?;
        Ok(rows)
    }
    pub fn gemma_save(&mut self, chunks: &[Value], vectors: &[Vec<f64>]) -> Result<usize, String> {
        self.gemma_save_for_model(chunks, vectors, MODEL_KEY)
    }
    pub fn gemma_save_for_model(
        &mut self,
        chunks: &[Value],
        vectors: &[Vec<f64>],
        model_key: &str,
    ) -> Result<usize, String> {
        if chunks.len() != vectors.len() {
            return Err("Embedding batch size does not match".into());
        }
        let tx = self.db.transaction().map_err(err)?;
        let mut saved = 0;
        for (c, v) in chunks.iter().zip(vectors) {
            if v.len() != 256 || v.iter().any(|n| !n.is_finite()) {
                return Err("Invalid vector for the Gemma index".into());
            }
            let current = tx.query_row(
                "SELECT c.rowid,s.root_id,c.text FROM knowledge_chunks c JOIN knowledge_sources s ON s.id=c.source_id WHERE c.id=? AND s.status='indexed' AND c.embedding_json IS NOT NULL",
                [string(c, "id")],
                |r| {
                    let raw = content_bytes(r, 2)?;
                    let matches = match c.get("_textFingerprint") {
                        Some(Value::String(fingerprint)) => *fingerprint == text::hash(raw),
                        // Compatibility for existing manually constructed valid
                        // chunks. Lossy text alone must never authorize bad bytes.
                        None => raw == string(c, "text").as_bytes(),
                        Some(_) => false,
                    };
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, matches))
                },
            ).optional().map_err(err)?;
            if let Some((row, root, true)) = current {
                let bytes = text::encode(v);
                tx.execute(
                    "INSERT INTO knowledge_gemma_embeddings(chunk_id,model_key,vector) VALUES(?,?,?) ON CONFLICT(chunk_id,model_key) DO UPDATE SET vector=excluded.vector",
                    params![string(c, "id"), model_key, bytes],
                )
                .map_err(err)?;
                if let Some(table) = gemma_vector_table(model_key).filter(|_| self.vector) {
                    tx.execute(&format!("DELETE FROM {table} WHERE rowid=?"), [row])
                        .map_err(err)?;
                    tx.execute(
                        &format!("INSERT INTO {table}(rowid,embedding,root_id) VALUES(?,?,?)"),
                        params![row, bytes, root],
                    )
                    .map_err(err)?;
                }
                saved += 1;
            }
        }
        tx.commit().map_err(err)?;
        Ok(saved)
    }
    pub fn gemma_clear(&self) -> Result<(), String> {
        self.gemma_clear_for_model(MODEL_KEY)
    }
    pub fn gemma_clear_for_model(&self, model_key: &str) -> Result<(), String> {
        let tx = self.db.unchecked_transaction().map_err(err)?;
        tx.execute(
            "DELETE FROM knowledge_gemma_embeddings WHERE model_key=?",
            [model_key],
        )
        .map_err(err)?;
        if let Some(table) = gemma_vector_table(model_key).filter(|_| self.vector) {
            tx.execute(&format!("DELETE FROM {table}"), [])
                .map_err(err)?;
        }
        tx.execute(
            "DELETE FROM knowledge_metadata WHERE key=?",
            [gemma_cursor(model_key)],
        )
        .map_err(err)?;
        tx.commit().map_err(err)
    }
    pub fn gemma_has(&self, roots: &[String]) -> Result<bool, String> {
        self.gemma_has_for_model(roots, MODEL_KEY)
    }
    pub fn gemma_has_for_model(&self, roots: &[String], model_key: &str) -> Result<bool, String> {
        if roots.is_empty() {
            return Ok(false);
        }
        let filter = roots
            .iter()
            .map(|s| format!("'{}'", s.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        self.db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM knowledge_gemma_embeddings g JOIN knowledge_chunks c ON c.id=g.chunk_id JOIN knowledge_sources s ON s.id=c.source_id WHERE g.model_key=? AND s.status='indexed' AND s.root_id IN ({filter}) LIMIT 1)"),[model_key],|r|r.get(0)).map_err(err)
    }
    pub fn gemma_counts(&self, roots: &[String]) -> Result<(u64, u64), String> {
        self.gemma_counts_for_model(roots, MODEL_KEY)
    }
    pub fn gemma_counts_for_model(
        &self,
        roots: &[String],
        model_key: &str,
    ) -> Result<(u64, u64), String> {
        if roots.is_empty() {
            return Ok((0, 0));
        }
        let filter = roots
            .iter()
            .map(|s| format!("'{}'", s.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        self.db.query_row(&format!("SELECT COUNT(g.chunk_id),COUNT(*) FROM knowledge_chunks c JOIN knowledge_sources s ON s.id=c.source_id LEFT JOIN knowledge_gemma_embeddings g ON g.chunk_id=c.id AND g.model_key=? WHERE s.status='indexed' AND c.embedding_json IS NOT NULL AND s.root_id IN ({filter})"),[model_key],|r|Ok((r.get(0)?,r.get(1)?))).map_err(err)
    }
    pub fn vector_backfill(&self) -> Result<bool, String> {
        self.vector_backfill_for_model(MODEL_KEY)
    }
    pub fn vector_backfill_for_model(&self, model_key: &str) -> Result<bool, String> {
        if !self.vector {
            return Ok(false);
        }
        let cursor = self
            .meta("vector-binary-backfill-cursor-v1", json!(0))?
            .as_i64()
            .unwrap_or(0);
        let mut q=self.db.prepare("SELECT c.rowid,c.embedding_json,s.root_id FROM knowledge_chunks c JOIN knowledge_sources s ON s.id=c.source_id WHERE c.rowid>? AND c.embedding_json IS NOT NULL ORDER BY c.rowid LIMIT 256").map_err(err)?;
        let rows = q
            .query_map([cursor], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    text::decode(r.get_ref(1)?),
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        for (id, v, root) in &rows {
            if v.len() == 384 {
                self.db
                    .execute(
                        "DELETE FROM knowledge_chunk_vectors_binary WHERE rowid=?",
                        [id],
                    )
                    .map_err(err)?;
                self.db.execute("INSERT INTO knowledge_chunk_vectors_binary(rowid,embedding,root_id) VALUES(?,vec_bit(?),?)",params![id,text::binary(v),root]).map_err(err)?;
            }
        }
        if let Some((id, _, _)) = rows.last() {
            self.set_meta("vector-binary-backfill-cursor-v1", &json!(id))?;
        }
        let Some(table) = gemma_vector_table(model_key) else {
            return Ok(rows.len() == 256);
        };
        let cursor_key = gemma_cursor(model_key);
        let cursor = self.meta(&cursor_key, json!(0))?.as_i64().unwrap_or(0);
        let mut stmt=self.db.prepare("SELECT c.rowid,g.vector,s.root_id FROM knowledge_chunks c JOIN knowledge_sources s ON s.id=c.source_id JOIN knowledge_gemma_embeddings g ON g.chunk_id=c.id WHERE c.rowid>? AND g.model_key=? ORDER BY c.rowid LIMIT 256").map_err(err)?;
        let learned = stmt
            .query_map(params![cursor, model_key], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Vec<u8>>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        for (id, v, root) in &learned {
            if v.len() == 256 * 4 {
                self.db
                    .execute(&format!("DELETE FROM {table} WHERE rowid=?"), [id])
                    .map_err(err)?;
                self.db
                    .execute(
                        &format!("INSERT INTO {table}(rowid,embedding,root_id) VALUES(?,?,?)"),
                        params![id, v, root],
                    )
                    .map_err(err)?;
            }
        }
        if let Some((id, _, _)) = learned.last() {
            self.set_meta(&cursor_key, &json!(id))?;
        }
        Ok(rows.len() == 256 || learned.len() == 256)
    }
}
// SQL identifiers come only from this closed registry, never from a request.
// The legacy table has always been Q8; Q4 never reads or writes it.
fn gemma_vector_table(key: &str) -> Option<&'static str> {
    if key == MODEL_KEY {
        Some("knowledge_gemma_vectors")
    } else if key
        == super::runtime::EmbeddingVariantId::Q4KM
            .descriptor()
            .model_key
    {
        Some("knowledge_gemma_vectors_q4")
    } else {
        None
    }
}
fn gemma_cursor(key: &str) -> String {
    if key == MODEL_KEY {
        "rust-gemma-vector-cursor-v1".into()
    } else {
        format!("rust-gemma-vector-cursor-v2:{key}")
    }
}
pub fn string<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}
pub fn number(v: &Value, k: &str) -> i64 {
    v[k].as_i64().unwrap_or(0)
}
fn hit_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<(Value, Vec<f64>)> {
    let path: String = r.get(4)?;
    let mut v = json!({"chunkId":r.get::<_,String>(0)?,"sourceId":r.get::<_,String>(1)?,"text":content_text(r,3)?,"path":path,"title":title(&path),"score":0,"lexicalScore":0,"semanticScore":0});
    if let Some(p) = r.get::<_, Option<i64>>(2)? {
        v["pageNumber"] = json!(p);
    }
    Ok((v, text::decode(r.get_ref(5)?)))
}
fn sort_hits(
    hits: &mut Vec<Value>,
    limit: usize,
    exact: &std::collections::HashSet<String>,
    distinct_sources: bool,
) {
    if !exact.is_empty() {
        // Disjoint score bands preserve phrase priority both here and when the
        // launcher sorts the returned scores alongside its other result types.
        for hit in hits.iter_mut() {
            let score = hit["score"].as_f64().unwrap_or(0.).clamp(0., 1.);
            hit["score"] = json!(if exact.contains(string(hit, "chunkId")) {
                0.75 + score * 0.25
            } else {
                score * 0.74
            });
        }
    }
    hits.sort_by(|a, b| {
        b["score"]
            .as_f64()
            .unwrap_or(0.)
            .total_cmp(&a["score"].as_f64().unwrap_or(0.))
            .then_with(|| string(a, "chunkId").cmp(string(b, "chunkId")))
    });
    if distinct_sources {
        let mut seen = std::collections::HashSet::new();
        hits.retain(|hit| seen.insert(string(hit, "sourceId").to_owned()));
    }
    hits.truncate(limit);
}

fn content_word_regex() -> &'static regex::Regex {
    static WORDS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    WORDS.get_or_init(|| regex::Regex::new(r"[\p{L}\p{N}]+").unwrap())
}
fn content_fts_phrase(query: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let normalized: String = query.to_lowercase().nfkc().collect();
    let words: Vec<_> = content_word_regex()
        .find_iter(&normalized)
        .map(|m| m.as_str())
        .collect();
    // Include short words: dropping them would turn "plan of lesson" into a
    // different phrase. Only sanitized tokens enter FTS syntax; MATCH is bound.
    if words.len() < 2 {
        return String::new();
    }
    format!("\"{}\"", words.join(" "))
}

fn content_fts_query(query: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let normalized: String = query.to_lowercase().nfkc().collect();
    let terms: Vec<_> = content_word_regex()
        .find_iter(&normalized)
        .map(|m| m.as_str())
        .filter(|w| w.encode_utf16().count() >= 3)
        .map(|w| {
            if w.encode_utf16().count() >= 4 {
                format!("{w}*")
            } else {
                w.to_owned()
            }
        })
        .collect();
    terms.join(" AND ")
}

#[cfg(test)]
mod ranking_tests {
    use super::*;

    fn store() -> Store {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("schema.sql")).unwrap();
        let store = Store::from_connection(db, false, "/synthetic/knowledge.sqlite3".into());
        for root in ["enabled", "disabled", "outside"] {
            store
                .db
                .execute(
                    "INSERT INTO knowledge_roots VALUES(?,?, 'smart','local',?,0,0)",
                    params![
                        root,
                        format!("/synthetic/{root}"),
                        i64::from(root != "disabled")
                    ],
                )
                .unwrap();
        }
        store
    }

    fn index(store: &mut Store, id: &str, root: &str, pages: Vec<Value>) {
        store
            .pending(
                id,
                root,
                &format!("/synthetic/{id}.png"),
                &json!({"contentHash":id,"byteSize":2000,"modifiedAt":0}),
                "smart",
            )
            .unwrap();
        let chunks = text::chunks(id, &pages, true);
        store
            .save_result(
                id,
                &json!({"pages":pages,"chunks":chunks,"images":[],"totalPageCount":1}),
            )
            .unwrap();
    }

    fn legacy_utf8_fixture() -> Store {
        let mut s = store();
        for (id, body) in [("bad", "lesson archival"), ("healthy", "healthy unchanged")] {
            index(
                &mut s,
                id,
                "enabled",
                vec![json!({"pageNumber":1,"extractedText":body})],
            );
        }
        s
    }

    fn corrupt_legacy_body(s: &Store) {
        s.db.execute_batch("UPDATE knowledge_chunks SET text=CAST(CAST(text AS BLOB)||x'ff' AS TEXT) WHERE source_id='bad'; UPDATE knowledge_chunks_fts SET text=CAST(CAST(text AS BLOB)||x'ff' AS TEXT) WHERE source_id='bad';").unwrap();
    }

    fn legacy_raw_body(s: &Store) -> Vec<u8> {
        s.db.query_row(
            "SELECT CAST(text AS BLOB) FROM knowledge_chunks WHERE source_id='bad'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn legacy_utf8_pending_save_finishes_each_model_without_rewriting_text() {
        let mut s = legacy_utf8_fixture();
        corrupt_legacy_body(&s);
        let raw = legacy_raw_body(&s);
        let roots = vec!["enabled".to_owned()];
        let q4 = super::super::EmbeddingVariantId::Q4KM
            .descriptor()
            .model_key;
        for key in [MODEL_KEY, q4] {
            let pending = s.gemma_pending_for_model(&roots, 8, key).unwrap();
            assert_eq!(pending.len(), 2);
            assert_eq!(pending[0]["text"], "lesson archival\u{fffd}");
            assert_eq!(pending[1]["text"], "healthy unchanged");
            assert_eq!(
                s.gemma_save_for_model(&pending, &vec![vec![1.; 256]; 2], key)
                    .unwrap(),
                2
            );
            assert!(s
                .gemma_pending_for_model(&roots, 8, key)
                .unwrap()
                .is_empty());
            assert_eq!(s.gemma_counts_for_model(&roots, key).unwrap(), (2, 2));
            assert_eq!(legacy_raw_body(&s), raw);
        }
        assert_eq!(
            s.db.query_row("SELECT COUNT(*) FROM knowledge_gemma_embeddings", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            4
        );
    }

    #[test]
    fn legacy_utf8_save_checks_original_bytes_and_legacy_valid_manual_values() {
        let mut s = legacy_utf8_fixture();
        corrupt_legacy_body(&s);
        let raw = legacy_raw_body(&s);
        let id: String =
            s.db.query_row(
                "SELECT id FROM knowledge_chunks WHERE source_id='bad'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let c = json!({"id":id,"text":String::from_utf8_lossy(&raw),"_textFingerprint":text::hash(&raw)});
        let v = vec![1.; 256];
        assert_eq!(s.gemma_save(&[c.clone()], &[v.clone()]).unwrap(), 1);
        s.gemma_clear().unwrap();
        // Different invalid bytes render identically; a decoded-string fence is insufficient.
        s.db.execute("UPDATE knowledge_chunks SET text=CAST(x'6c6573736f6e20617263686976616cfe' AS TEXT) WHERE source_id='bad'", []).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&legacy_raw_body(&s)),
            c["text"].as_str().unwrap()
        );
        assert_eq!(s.gemma_save(&[c], &[v.clone()]).unwrap(), 0);
        let mut pending = s.gemma_pending(&["enabled".to_owned()], 8).unwrap();
        pending[0]
            .as_object_mut()
            .unwrap()
            .remove("_textFingerprint");
        assert_eq!(s.gemma_save(&pending, &vec![v.clone(); 2]).unwrap(), 1);
        // Manual valid objects retain the old compatibility contract and stale-text fence.
        s.gemma_clear().unwrap();
        s.db.execute(
            "UPDATE knowledge_chunks SET text='changed' WHERE source_id='healthy'",
            [],
        )
        .unwrap();
        assert_eq!(s.gemma_save(&pending[1..], &[v]).unwrap(), 0);
    }

    #[test]
    fn legacy_utf8_search_and_read_preserve_healthy_hits_and_scores() {
        let mut s = legacy_utf8_fixture();
        let roots = vec!["enabled".to_owned()];
        let pending = s.gemma_pending(&roots, 8).unwrap();
        let v = vec![1.; 256];
        s.gemma_save(&pending, &vec![v.clone(); 2]).unwrap();
        let mut before = Vec::new();
        for sources in [false, true] {
            for (query, lexical, learned) in [
                ("lesson", true, None),
                ("lesson", false, None),
                ("unrelated", false, Some((v.as_slice(), roots.as_slice()))),
            ] {
                let hits = if sources {
                    s.search_sources(query, 8, &roots, lexical, learned)
                } else {
                    s.search(query, 8, &roots, lexical, learned)
                }
                .unwrap();
                assert!(!hits.is_empty());
                before.push((sources, query, lexical, learned, hits));
            }
        }
        corrupt_legacy_body(&s);
        for (sources, query, lexical, learned, mut expected) in before {
            for hit in &mut expected {
                if hit["sourceId"] == "bad" {
                    hit["text"] = json!("lesson archival\u{fffd}");
                }
            }
            let actual = if sources {
                s.search_sources(query, 8, &roots, lexical, learned)
            } else {
                s.search(query, 8, &roots, lexical, learned)
            }
            .unwrap();
            assert_eq!(actual, expected);
        }
        for c in &pending {
            let read = s.read(string(c, "id"), 1000, &roots).unwrap();
            let expected = if c["text"] == "lesson archival" {
                "lesson archival\u{fffd}"
            } else {
                "healthy unchanged"
            };
            assert_eq!(read["text"], expected);
        }
    }

    #[test]
    fn legacy_utf8_reusable_content_is_sanitized_and_copies_finish_pending() {
        let mut s = legacy_utf8_fixture();
        s.db.execute_batch("INSERT INTO knowledge_images(id,source_id,ocr_text,description,width,height) VALUES('image','bad','image words','description',1,1); UPDATE knowledge_pages SET ocr_text='ocr words',ocr_blocks_json='[]' WHERE source_id='bad';").unwrap();
        let healthy = s.reusable("healthy", "copy", "smart").unwrap();
        corrupt_legacy_body(&s);
        s.db.execute_batch("UPDATE knowledge_pages SET extracted_text=CAST(CAST(extracted_text AS BLOB)||x'ff' AS TEXT),ocr_text=CAST(CAST(ocr_text AS BLOB)||x'fe' AS TEXT) WHERE source_id='bad'; UPDATE knowledge_images SET ocr_text=CAST(CAST(ocr_text AS BLOB)||x'ff' AS TEXT),description=CAST(CAST(description AS BLOB)||x'fe' AS TEXT) WHERE source_id='bad';").unwrap();
        let raw = legacy_raw_body(&s);
        let reused = s.reusable("bad", "copy", "smart").unwrap().unwrap();
        assert_eq!(reused["chunks"][0]["text"], "lesson archival\u{fffd}");
        assert_eq!(
            reused["pages"][0]["extractedText"],
            "lesson archival\u{fffd}"
        );
        assert_eq!(reused["pages"][0]["ocr"]["text"], "ocr words\u{fffd}");
        assert_eq!(reused["images"][0]["ocrText"], "image words\u{fffd}");
        assert_eq!(reused["images"][0]["description"], "description\u{fffd}");
        assert_eq!(s.reusable("healthy", "copy", "smart").unwrap(), healthy);
        s.pending(
            "copy",
            "enabled",
            "/synthetic/copy.txt",
            &json!({"contentHash":"bad","byteSize":1,"modifiedAt":0}),
            "smart",
        )
        .unwrap();
        s.save_result("copy", &reused).unwrap();
        let roots = vec!["enabled".to_owned()];
        let pending = s.gemma_pending(&roots, 8).unwrap();
        assert_eq!(pending.len(), 3);
        assert_eq!(s.gemma_save(&pending, &vec![vec![1.; 256]; 3]).unwrap(), 3);
        assert!(s.gemma_pending(&roots, 8).unwrap().is_empty());
        assert_eq!(legacy_raw_body(&s), raw);
    }

    #[test]
    fn legacy_utf8_keeps_blob_content_and_path_identity_strict() {
        let s = legacy_utf8_fixture();
        let roots = vec!["enabled".to_owned()];
        s.db.execute_batch("UPDATE knowledge_chunks SET text=x'ff' WHERE source_id='bad'")
            .unwrap();
        assert!(s.gemma_pending(&roots, 8).is_err());
        assert!(s.reusable("bad", "copy", "smart").is_err());
        assert!(s.search("lesson", 8, &roots, true, None).is_err());
        let id: String =
            s.db.query_row(
                "SELECT id FROM knowledge_chunks WHERE source_id='bad'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(s.read(&id, 1000, &roots).is_err());
        s.db.execute_batch("UPDATE knowledge_chunks SET text='valid'; UPDATE knowledge_sources SET path=CAST(x'ff' AS TEXT) WHERE id='bad'").unwrap();
        assert!(s.gemma_pending(&roots, 8).is_err());
        assert!(s.search("lesson", 8, &roots, true, None).is_err());
        assert!(s.read(&id, 1000, &roots).is_err());
    }

    fn phrase_fixture(distractors: usize) -> Store {
        let mut store = store();
        for i in 0..distractors {
            index(
                &mut store,
                &format!("short-{i}"),
                "enabled",
                vec![json!({"pageNumber":1,"extractedText":"Материалы для учителя"})],
            );
        }
        index(
            &mut store,
            "ocr-target",
            "enabled",
            vec![
                json!({"pageNumber":1,"ocr":{"text":format!("Материалы учителя. {}", "Описание учебного занятия и план урока. ".repeat(25))}}),
            ],
        );
        store
    }

    fn enable_vec0(store: &mut Store) -> bool {
        let ext = if cfg!(target_os = "macos") {
            "dylib"
        } else if cfg!(windows) {
            "dll"
        } else {
            "so"
        };
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(format!("dist-backend/vec0.{ext}"));
        if !path.is_file() {
            eprintln!("No bundled vec0; exercising the no-extension fallback only");
            return false;
        }
        unsafe {
            store.db.load_extension_enable().unwrap();
            store.db.load_extension(path, None).unwrap();
            store.db.load_extension_disable().unwrap();
        }
        store.db.execute_batch("CREATE VIRTUAL TABLE knowledge_chunk_vectors_binary USING vec0(embedding bit[384],root_id text); CREATE VIRTUAL TABLE knowledge_gemma_vectors USING vec0(embedding float[256] distance_metric=cosine,root_id text);").unwrap();
        store.vector = true;
        while store.vector_backfill().unwrap() {}
        true
    }

    #[test]
    fn exact_ocr_phrase_beats_short_separated_words_in_all_ranking_modes() {
        // 16 reproduces the final top-16 loss; 100 also exceeds the 80-row FTS window.
        for (count, vector) in [(16, false), (100, false), (100, true)] {
            let mut store = phrase_fixture(count);
            if vector && !enable_vec0(&mut store) {
                continue;
            }
            let roots = vec!["enabled".to_owned()];
            let mut query_vector = vec![0.; 256];
            query_vector[0] = 1.;
            let pending = store.gemma_pending(&roots, 1000).unwrap();
            let vectors = pending
                .iter()
                .map(|c| {
                    let mut v = vec![0.; 256];
                    // Give the short distractors the strongest possible semantic advantage.
                    v[usize::from(c["text"].as_str().unwrap().starts_with("Материалы учителя"))] =
                        1.;
                    v
                })
                .collect::<Vec<_>>();
            store.gemma_save(&pending, &vectors).unwrap();
            for (lexical_only, learned) in [
                (true, None),
                (false, None),
                (false, Some((query_vector.as_slice(), roots.as_slice()))),
            ] {
                let hits = store
                    .search("Материалы учителя", 16, &roots, lexical_only, learned)
                    .unwrap();
                assert_eq!(
                    hits[0]["sourceId"],
                    "ocr-target",
                    "distractors={count}, lexical_only={lexical_only}, learned={}",
                    learned.is_some()
                );
                assert!(hits[0]["score"].as_f64().unwrap() > hits[1]["score"].as_f64().unwrap());
                let files = store
                    .search_sources("Материалы учителя", 16, &roots, lexical_only, learned)
                    .unwrap();
                assert_eq!(files[0]["sourceId"], "ocr-target");
            }
        }
    }

    #[test]
    fn deep_search_limit_counts_sources_before_chunk_candidate_window() {
        let mut store = store();
        // More than the normal FTS and vector windows, all from one document.
        index(
            &mut store,
            "many-pages",
            "enabled",
            (1..=600)
                .map(|n| json!({"pageNumber":n,"extractedText":"espresso calibration"}))
                .collect(),
        );
        for i in 0..18 {
            index(
                &mut store,
                &format!("other-{i}"),
                "enabled",
                vec![
                    json!({"pageNumber":1,"extractedText":format!("espresso calibration {}", "Detailed instructions for preparing the machine. ".repeat(20))}),
                ],
            );
        }
        let roots = vec!["enabled".to_owned()];
        let chunks = store
            .search("espresso calibration", 16, &roots, true, None)
            .unwrap();
        assert_eq!(chunks.len(), 16);
        assert!(chunks.iter().all(|h| h["sourceId"] == "many-pages"));
        let mut vector = vec![0.; 256];
        vector[0] = 1.;
        let pending = store.gemma_pending(&roots, 1000).unwrap();
        store
            .gemma_save(&pending, &vec![vector.clone(); pending.len()])
            .unwrap();
        for (lexical_only, learned) in [
            (true, None),
            (false, None),
            (false, Some((vector.as_slice(), roots.as_slice()))),
        ] {
            let hits = store
                .search_sources("espresso calibration", 16, &roots, lexical_only, learned)
                .unwrap();
            let sources = hits
                .iter()
                .map(|h| string(h, "sourceId"))
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(
                sources.len(),
                16,
                "chunk duplicates must not consume the file limit"
            );
        }
    }

    // Semantic-only fixtures exercise Store itself, not a parallel ranking model.
    fn semantic_fixture(duplicate_sources: usize, duplicates: usize) -> (Store, Vec<f64>) {
        let mut store = store();
        for i in 0..duplicate_sources + 16 {
            index(
                &mut store,
                &format!("semantic-{i:03}"),
                "enabled",
                (1..=if i < duplicate_sources { duplicates } else { 1 })
                    .map(|n| json!({"pageNumber":n,"extractedText":"archived unrelated record"}))
                    .collect(),
            );
        }
        for (id, root) in [
            ("disabled-vector", "disabled"),
            ("outside-vector", "outside"),
            ("pending-vector", "enabled"),
            ("failed-vector", "enabled"),
            ("wrong-model", "enabled"),
        ] {
            index(
                &mut store,
                id,
                root,
                (1..=600)
                    .map(|n| json!({"pageNumber":n,"extractedText":"archived unrelated record"}))
                    .collect(),
            );
        }
        let query = text::embed("espresso calibration");
        let pending = store
            .gemma_pending(
                &["enabled".into(), "disabled".into(), "outside".into()],
                100000,
            )
            .unwrap();
        let mut vectors = vec![];
        for chunk in &pending {
            let id = string(chunk, "id");
            let source: String = store
                .db
                .query_row(
                    "SELECT source_id FROM knowledge_chunks WHERE id=?",
                    [id],
                    |r| r.get(0),
                )
                .unwrap();
            let ordinal = source
                .strip_prefix("semantic-")
                .and_then(|n| n.parse::<usize>().ok());
            let near = ordinal.map(|i| i < duplicate_sources).unwrap_or(true);
            let mut builtin = query.clone();
            if !near {
                let zero = builtin.iter().position(|n| *n == 0.).unwrap();
                builtin[zero] = -0.3;
            }
            store
                .db
                .execute(
                    "UPDATE knowledge_chunks SET embedding_json=? WHERE id=?",
                    params![text::encode(&builtin), id],
                )
                .unwrap();
            let mut learned = vec![0.; 256];
            learned[0] = if near { 1. } else { 0.95 };
            learned[1] = if near { 0. } else { 0.3122498999 };
            vectors.push(learned);
        }
        store.gemma_save(&pending, &vectors).unwrap();
        store.db.execute_batch("UPDATE knowledge_sources SET status='pending' WHERE id='pending-vector'; UPDATE knowledge_sources SET status='failed' WHERE id='failed-vector'; UPDATE knowledge_gemma_embeddings SET model_key='obsolete' WHERE chunk_id IN (SELECT id FROM knowledge_chunks WHERE source_id='wrong-model');").unwrap();
        let mut vector = vec![0.; 256];
        vector[0] = 1.;
        (store, vector)
    }

    fn assert_semantic_sources(learned: bool, accelerated: bool) {
        // Eight duplicate-heavy sources cross several normal 512-row budgets;
        // 600 alone is the original failure. Include fewer actual sources than
        // the source-candidate target to exercise finite exhaustion as well.
        for duplicate_sources in [1, 8] {
            let (mut store, vector) = semantic_fixture(duplicate_sources, 600);
            if accelerated && !enable_vec0(&mut store) {
                continue;
            }
            let roots = vec!["enabled".to_owned(), "disabled".to_owned()];
            let learned_query = learned.then_some((vector.as_slice(), roots.as_slice()));
            assert!(store
                .search_sources("espresso calibration", 16, &roots, true, None)
                .unwrap()
                .is_empty());
            // Ordinary chunk API must still count chunks, not files.
            let chunks = store
                .search("espresso calibration", 16, &roots, false, learned_query)
                .unwrap();
            assert_eq!(chunks.len(), 16);
            assert!(
                chunks
                    .iter()
                    .map(|h| string(h, "sourceId"))
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    < 16
            );
            let hits = store
                .search_sources("espresso calibration", 16, &roots, false, learned_query)
                .unwrap();
            let sources = hits
                .iter()
                .map(|h| string(h, "sourceId"))
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(
                sources.len(),
                16,
                "learned={learned}, vec0={accelerated}, duplicate_sources={duplicate_sources}"
            );
            assert!(sources
                .iter()
                .all(|id| id.starts_with("semantic-") || (!learned && *id == "wrong-model")));
            let all = store
                .search_sources("espresso calibration", 100, &roots, false, learned_query)
                .unwrap();
            assert_eq!(all.len(), duplicate_sources + 16 + usize::from(!learned));
            // The learned root subset must not bypass the caller's enabled roots.
            if learned {
                let outside = vec!["outside".to_owned()];
                assert!(store
                    .search_sources(
                        "espresso calibration",
                        16,
                        &roots,
                        false,
                        Some((&vector, &outside))
                    )
                    .unwrap()
                    .is_empty());
            }
        }
    }

    #[test]
    fn semantic_learned_sources_vec0() {
        assert_semantic_sources(true, true);
    }
    #[test]
    fn semantic_learned_sources_without_vec0() {
        assert_semantic_sources(true, false);
    }
    #[test]
    fn semantic_builtin_sources_vec0() {
        assert_semantic_sources(false, true);
    }
    #[test]
    fn semantic_builtin_sources_without_vec0() {
        assert_semantic_sources(false, false);
    }

    #[test]
    fn semantic_sources_survive_partial_acceleration_backfill() {
        let (mut store, vector) = semantic_fixture(1, 1);
        if !enable_vec0(&mut store) {
            return;
        }
        // Authoritative embeddings have 17 valid files, but startup repair has
        // backfilled only one row. A short vec0 result is not source exhaustion.
        for table in ["knowledge_gemma_vectors", "knowledge_chunk_vectors_binary"] {
            store.db.execute(&format!("DELETE FROM {table} WHERE rowid NOT IN (SELECT rowid FROM knowledge_chunks WHERE source_id='semantic-000')"), []).unwrap();
        }
        let roots = vec!["enabled".to_owned(), "disabled".to_owned()];
        for learned in [None, Some((vector.as_slice(), roots.as_slice()))] {
            store.vector = false;
            let expected = store
                .search_sources("espresso calibration", 16, &roots, false, learned)
                .unwrap();
            assert_eq!(expected.len(), 16);
            store.vector = true;
            let hits = store
                .search_sources("espresso calibration", 16, &roots, false, learned)
                .unwrap();
            assert_eq!(hits, expected, "learned={}", learned.is_some());
        }
    }

    #[test]
    fn semantic_sources_fallback_updates_partially_accelerated_winners() {
        let (mut store, vector) = semantic_fixture(1, 2);
        if !enable_vec0(&mut store) {
            return;
        }
        // The accelerated row of a represented source can be weaker than its
        // unbackfilled row. Fallback must rescore represented sources too.
        let roots = vec!["enabled".to_owned()];
        let mut weaker = vec![0.; 256];
        weaker[0] = 0.8;
        weaker[1] = 0.6;
        store.db.execute("UPDATE knowledge_gemma_embeddings SET vector=? WHERE chunk_id=(SELECT id FROM knowledge_chunks WHERE source_id='semantic-000' ORDER BY rowid LIMIT 1)", [text::encode(&weaker)]).unwrap();
        store.db.execute_batch("DELETE FROM knowledge_gemma_vectors; INSERT INTO knowledge_gemma_vectors(rowid,embedding,root_id) SELECT c.rowid,g.vector,'enabled' FROM knowledge_chunks c JOIN knowledge_gemma_embeddings g ON g.chunk_id=c.id WHERE c.source_id='semantic-000' ORDER BY c.rowid LIMIT 1;").unwrap();
        let learned = Some((vector.as_slice(), roots.as_slice()));
        store.vector = false;
        let expected = store
            .search_sources("espresso calibration", 16, &roots, false, learned)
            .unwrap();
        assert_eq!(expected[0]["sourceId"], "semantic-000");
        store.vector = true;
        let hits = store
            .search_sources("espresso calibration", 16, &roots, false, learned)
            .unwrap();
        assert_eq!(hits, expected);
    }

    #[test]
    fn semantic_source_candidates_survive_unavailable_knn_tables() {
        let (mut store, vector) = semantic_fixture(1, 600);
        // Simulate a vec0 install whose KNN tables/constraints are unavailable.
        // Prepare fails, and the authoritative no-extension path must still work.
        store.vector = true;
        let roots = vec!["enabled".to_owned(), "disabled".to_owned()];
        for learned in [None, Some((vector.as_slice(), roots.as_slice()))] {
            let hits = store
                .search_sources("espresso calibration", 16, &roots, false, learned)
                .unwrap();
            assert_eq!(hits.len(), 16);
            assert!(hits
                .iter()
                .all(|h| string(h, "sourceId").starts_with("semantic-")
                    || (learned.is_none() && h["sourceId"] == "wrong-model")));
        }
    }

    #[test]
    fn exact_phrase_preserves_short_words_and_token_boundaries() {
        let mut store = store();
        index(
            &mut store,
            "separated",
            "enabled",
            vec![json!({"pageNumber":1,"extractedText":"plan lesson"})],
        );
        index(
            &mut store,
            "substring",
            "enabled",
            vec![json!({"pageNumber":1,"extractedText":"planet of lessons"})],
        );
        index(
            &mut store,
            "adjacent",
            "enabled",
            vec![
                json!({"pageNumber":1,"ocr":{"text":format!("PLAN of LESSON. {}", "Detailed teaching instructions. ".repeat(20))}}),
            ],
        );
        for lexical_only in [true, false] {
            let hits = store
                .search("plan of lesson", 3, &["enabled".into()], lexical_only, None)
                .unwrap();
            assert_eq!(hits[0]["sourceId"], "adjacent");
            assert_eq!(
                hits.len(),
                3,
                "ordinary prefix AND matches remain available"
            );
        }
    }

    #[test]
    fn phrase_search_keeps_prefixes_and_respects_roots_and_source_status() {
        let mut store = store();
        for root in ["enabled", "disabled", "outside"] {
            index(
                &mut store,
                root,
                root,
                vec![json!({"pageNumber":1,"ocr":{"text":"МАТЕРИАЛЫ УЧИТЕЛЯ"}})],
            );
        }
        index(
            &mut store,
            "failed",
            "enabled",
            vec![json!({"pageNumber":1,"ocr":{"text":"Материалы учителя"}})],
        );
        store
            .db
            .execute(
                "UPDATE knowledge_sources SET status='failed' WHERE id='failed'",
                [],
            )
            .unwrap();
        let roots = vec!["enabled".to_owned(), "disabled".to_owned()];
        for query in [
            "Материалы учителя",
            "МАТЕРИАЛЫ,\nУЧИТЕЛЯ",
            "матер учит",
            "учит матер",
            "учит",
            "\"матер\" NOT учит*",
            "материалы для учителя",
        ] {
            let hits = store.search(query, 16, &roots, true, None).unwrap();
            if query == "материалы для учителя" || query.contains("NOT") {
                assert!(
                    hits.is_empty(),
                    "query operators are literal tokens, not FTS syntax"
                );
            } else {
                assert_eq!(hits.len(), 1, "query={query}");
                assert_eq!(hits[0]["sourceId"], "enabled");
            }
        }
    }
    fn review_insert(
        s: &Store,
        row: i64,
        source: &str,
        root: &str,
        status: &str,
        key: &str,
        learned: &[f64],
        builtin: &[f64],
        accelerated: bool,
    ) {
        s.db.execute("INSERT INTO knowledge_sources(id,root_id,path,content_hash,byte_size,modified_at,status) VALUES(?,?,?,'',0,0,?)",params![source,root,format!("/synthetic/{source}.txt"),status]).unwrap();
        let id = format!("review-{row:05}");
        s.db.execute("INSERT INTO knowledge_chunks(rowid,id,source_id,text,embedding_json) VALUES(?,?,?,'unrelated archive',?)",params![row,id,source,text::encode(builtin)]).unwrap();
        s.db.execute(
            "INSERT INTO knowledge_gemma_embeddings VALUES(?,?,?)",
            params![id, key, text::encode(learned)],
        )
        .unwrap();
        if accelerated {
            // Stale acceleration root deliberately claims enabled for every row.
            s.db.execute("INSERT INTO knowledge_gemma_vectors(rowid,embedding,root_id) VALUES(?,?,'enabled')",params![row,text::encode(learned)]).unwrap();
            s.db.execute("INSERT INTO knowledge_chunk_vectors_binary(rowid,embedding,root_id) VALUES(?,vec_bit(?),'enabled')",params![row,text::binary(builtin)]).unwrap();
        }
    }
    fn review_vectors() -> (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>) {
        let mut q = vec![0.; 256];
        q[0] = 1.;
        let mut weak = q.clone();
        weak[0] = 0.9;
        weak[1] = (1f64 - 0.81).sqrt();
        let b = text::embed("espresso calibration");
        let mut wb = b.clone();
        let zero = wb.iter().position(|v| *v == 0.).unwrap();
        wb[zero] = -0.3;
        (q, weak, b, wb)
    }
    #[test]
    fn partial_full_budget_must_find_stronger_unbackfilled_represented_chunk() {
        let mut s = store();
        assert!(enable_vec0(&mut s));
        let (q, w, b, _) = review_vectors();
        for i in 0..80 {
            review_insert(
                &s,
                i + 1,
                &format!("visible-{i:03}"),
                "enabled",
                "indexed",
                MODEL_KEY,
                &w,
                &b,
                true,
            );
        }
        s.db.execute("INSERT INTO knowledge_chunks(rowid,id,source_id,text,embedding_json) VALUES(1001,'strong-hidden','visible-079','unrelated archive',?)",[text::encode(&b)]).unwrap();
        s.db.execute(
            "INSERT INTO knowledge_gemma_embeddings VALUES('strong-hidden',?,?)",
            params![MODEL_KEY, text::encode(&q)],
        )
        .unwrap();
        let roots = vec!["enabled".to_owned()];
        let hits = s
            .search_sources(
                "espresso calibration",
                16,
                &roots,
                false,
                Some((&q, &roots)),
            )
            .unwrap();
        assert_eq!(hits[0]["chunkId"], "strong-hidden");
    }

    #[test]
    fn review_partial_full_budget_must_find_best() {
        let mut s = store();
        assert!(enable_vec0(&mut s));
        let (q, w, b, _) = review_vectors();
        for i in 0..80 {
            review_insert(
                &s,
                i + 1,
                &format!("visible-{i:03}"),
                "enabled",
                "indexed",
                MODEL_KEY,
                &w,
                &b,
                true,
            );
        }
        review_insert(
            &s,
            1001,
            "best-unbackfilled",
            "enabled",
            "indexed",
            MODEL_KEY,
            &q,
            &b,
            false,
        );
        let roots = vec!["enabled".to_owned()];
        let actual = s
            .search_sources(
                "espresso calibration",
                16,
                &roots,
                false,
                Some((&q, &roots)),
            )
            .unwrap();
        assert_eq!(
            actual[0]["sourceId"], "best-unbackfilled",
            ">=80 partial coverage must not hide authoritative winner"
        );
    }
}
