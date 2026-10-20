use super::{runtime::MODEL_KEY, text};
use crate::native_core::common::{now_ms, CoreContext};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::sync::{mpsc, oneshot};
type Job = Box<dyn FnOnce(&mut Store) + Send>;
#[derive(Clone)]
pub struct Database {
    sender: mpsc::Sender<Option<Job>>,
}
impl Database {
    pub async fn open(ctx: CoreContext) -> Result<Self, String> {
        let (sender, mut receiver) = mpsc::channel::<Option<Job>>(64);
        let (ready, rx) = oneshot::channel();
        std::thread::Builder::new()
            .name("knowledge-sqlite".into())
            .spawn(move || {
                let mut store = match Store::open(&ctx) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = ready.send(Err(e));
                        return;
                    }
                };
                let _ = ready.send(Ok(()));
                while let Some(Some(job)) = receiver.blocking_recv() {
                    job(&mut store);
                }
            })
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())??;
        Ok(Self { sender })
    }
    pub async fn close(&self) -> Result<(), String> {
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
pub struct Store {
    pub db: Connection,
    pub vector: bool,
    pub path: PathBuf,
}
fn err(e: rusqlite::Error) -> String {
    e.to_string()
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
    json!({"depth":"smart","embeddingModel":"built-in","indexingSpeed":"medium","runOnBattery":false,"onlyRunHeavyJobsWhenIdle":true,"maxConcurrentExtractors":1,"maxConcurrentOcrJobs":1})
}
pub fn default_status() -> Value {
    json!({"state":"idle","backend":"local","progress":0,"queuedSources":0,"processedSources":0,"failedSources":0,"sourceCount":0,"chunkCount":0,"indexedPageCount":0,"totalPageCount":0,"partialSourceCount":0,"sourceBytes":0})
}
impl Store {
    fn open(ctx: &CoreContext) -> Result<Self, String> {
        let directory = ctx.paths.data_dir.join("knowledge");
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let path = directory.join("knowledge.sqlite3");
        let db = Connection::open(&path).map_err(err)?;
        db.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(err)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
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
            db.execute_batch("CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_chunk_vectors_binary USING vec0(embedding bit[384],root_id text); CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_gemma_vectors USING vec0(embedding float[256] distance_metric=cosine,root_id text); CREATE TRIGGER IF NOT EXISTS knowledge_gemma_vector_delete BEFORE DELETE ON knowledge_chunks BEGIN DELETE FROM knowledge_gemma_vectors WHERE rowid=OLD.rowid; END;").map_err(err)?;
        } else {
            db.execute_batch("DROP TRIGGER IF EXISTS knowledge_gemma_vector_delete;")
                .map_err(err)?;
        }
        db.execute_batch("CREATE INDEX IF NOT EXISTS knowledge_sources_reuse_idx ON knowledge_sources(content_hash,indexing_profile,status,indexed_at DESC); UPDATE knowledge_roots SET depth='inherit' WHERE depth IN ('text','text-and-images');").map_err(err)?;
        let store = Self { db, vector, path };
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
            store.db.execute_batch("DELETE FROM knowledge_chunk_vectors_binary;DELETE FROM knowledge_gemma_vectors;").map_err(err)?;
            store.set_meta("vector-binary-backfill-cursor-v1", &json!(0))?;
            store.set_meta("rust-gemma-vector-cursor-v1", &json!(0))?;
            store.set_meta("rust-vector-rebuild-needed-v1", &json!(false))?;
        }
        // One-time index repair, never on launcher status ticks.
        let repaired = store.meta("rust-fts-rowids-v1", json!(false))?;
        if repaired != true {
            store.db.execute_batch("DELETE FROM knowledge_sources_fts; INSERT INTO knowledge_sources_fts(rowid,id,path) SELECT rowid,id,path FROM knowledge_sources; DELETE FROM knowledge_chunks_fts; INSERT INTO knowledge_chunks_fts(rowid,id,source_id,text) SELECT rowid,id,source_id,text FROM knowledge_chunks;").map_err(err)?;
            store.set_meta("rust-fts-rowids-v1", &json!(true))?;
        }
        // SQLite foreign keys were historically disabled. Explicit deletions below also maintain vec0.
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
        let mut q=self.db.prepare("SELECT job_id,position,root_id,path FROM knowledge_indexing_queue ORDER BY position LIMIT ?").map_err(err)?;
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
        let pages=q.query_map([&source],|r|{let ocr:Option<String>=r.get(2)?;let mut p=json!({"pageNumber":r.get::<_,i64>(0)?,"extractedText":r.get::<_,Option<String>>(1)?});if let Some(ocr)=ocr{p["ocr"]=json!({"text":ocr,"blocks":r.get::<_,Option<String>>(3)?.and_then(|s|serde_json::from_str::<Value>(&s).ok()).unwrap_or(json!([])),"averageConfidence":r.get::<_,Option<f64>>(4)?});}Ok(p)}).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
        let mut q=self.db.prepare("SELECT id,page_number,text,embedding_json,start_offset,end_offset FROM knowledge_chunks WHERE source_id=? ORDER BY rowid").map_err(err)?;
        let chunks=q.query_map([&source],|r|{let old:String=r.get(0)?;let v=text::decode(r.get_ref(3)?);let mut c=json!({"id":text::hash(format!("{id}:{old}")),"pageNumber":r.get::<_,Option<i64>>(1)?,"text":r.get::<_,String>(2)?,"startOffset":r.get::<_,Option<i64>>(4)?,"endOffset":r.get::<_,Option<i64>>(5)?});if !v.is_empty(){c["embedding"]=json!(v);}Ok(c)}).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
        let mut q=self.db.prepare("SELECT id,page_number,ocr_text,description,embedding_json,width,height FROM knowledge_images WHERE source_id=? ORDER BY rowid").map_err(err)?;
        let images=q.query_map([&source],|r|{let old:String=r.get(0)?;Ok(json!({"id":text::hash(format!("{id}:{old}")),"pageNumber":r.get::<_,Option<i64>>(1)?,"ocrText":r.get::<_,Option<String>>(2)?,"description":r.get::<_,Option<String>>(3)?,"embedding":text::decode(r.get_ref(4)?),"width":r.get::<_,i64>(5)?,"height":r.get::<_,i64>(6)?}))}).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
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
            .query_map(params![source, row - 2, row + 2], |r| r.get::<_, String>(0))
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
        use std::collections::HashMap;
        let learned_filter = learned.map(|(_, ids)| {
            ids.iter()
                .map(|id| format!("'{}'", id.replace('\'', "''")))
                .collect::<Vec<_>>()
                .join(",")
        });
        let learned = learned.map(|(vector, _)| vector);
        if query.trim().is_empty() || limit == 0 || roots.is_empty() {
            return Ok(vec![]);
        }
        let fts = content_fts_query(query);
        let root_filter = roots
            .iter()
            .map(|id| format!("'{}'", id.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        let mut candidates: HashMap<String, (Value, Vec<f64>, f64)> = HashMap::new();
        let mut lexical = vec![];
        if !fts.is_empty() {
            let sql=format!("SELECT c.id,c.source_id,c.page_number,c.text,s.path,c.embedding_json FROM knowledge_chunks_fts f JOIN knowledge_chunks c ON c.rowid=f.rowid JOIN knowledge_sources s ON s.id=c.source_id JOIN knowledge_roots r ON r.id=s.root_id WHERE knowledge_chunks_fts MATCH ? AND r.enabled=1 AND r.id IN ({root_filter}) ORDER BY bm25(knowledge_chunks_fts,1.0) LIMIT ?");
            let mut q = self.db.prepare(&sql).map_err(err)?;
            let rows = q
                .query_map(
                    params![fts, limit.saturating_mul(5).max(40) as i64],
                    hit_row,
                )
                .map_err(err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?;
            let len = rows.len().max(1) as f64;
            for (i, (hit, v)) in rows.into_iter().enumerate() {
                let id = string(&hit, "chunkId").to_owned();
                lexical.push(id.clone());
                candidates.insert(id, (hit, v, (1. - i as f64 / len).max(0.15)));
            }
        }
        if !lexical_only {
            let root_filter = learned_filter.as_deref().unwrap_or(&root_filter);
            let sql = if let Some(vector) = learned {
                if vector.len() != 256 {
                    return Err("Invalid vector for the Gemma index".into());
                }
                if self.vector {
                    format!("WITH nearest AS (SELECT rowid,distance FROM knowledge_gemma_vectors WHERE embedding MATCH ? AND k=? AND root_id IN ({root_filter})) SELECT c.id,c.source_id,c.page_number,c.text,s.path,g.vector FROM nearest JOIN knowledge_chunks c ON c.rowid=nearest.rowid JOIN knowledge_sources s ON s.id=c.source_id JOIN knowledge_gemma_embeddings g ON g.chunk_id=c.id JOIN knowledge_roots r ON r.id=s.root_id WHERE s.status='indexed' AND r.enabled=1 AND g.model_key='{MODEL_KEY}' ORDER BY nearest.distance")
                } else {
                    format!("SELECT c.id,c.source_id,c.page_number,c.text,s.path,g.vector FROM knowledge_gemma_embeddings g JOIN knowledge_chunks c ON c.id=g.chunk_id JOIN knowledge_sources s ON s.id=c.source_id JOIN knowledge_roots r ON r.id=s.root_id WHERE g.model_key='{MODEL_KEY}' AND s.status='indexed' AND r.enabled=1 AND r.id IN ({root_filter})")
                }
            } else if self.vector {
                format!("WITH nearest AS (SELECT rowid,distance FROM knowledge_chunk_vectors_binary WHERE embedding MATCH vec_bit(?) AND k=? AND root_id IN ({root_filter})) SELECT c.id,c.source_id,c.page_number,c.text,s.path,c.embedding_json FROM nearest JOIN knowledge_chunks c ON c.rowid=nearest.rowid JOIN knowledge_sources s ON s.id=c.source_id JOIN knowledge_roots r ON r.id=s.root_id WHERE s.status='indexed' AND r.enabled=1 ORDER BY nearest.distance")
            } else {
                format!("SELECT c.id,c.source_id,c.page_number,c.text,s.path,c.embedding_json FROM knowledge_chunks c JOIN knowledge_sources s ON s.id=c.source_id JOIN knowledge_roots r ON r.id=s.root_id WHERE c.embedding_json IS NOT NULL AND s.status='indexed' AND r.enabled=1 AND r.id IN ({root_filter}) ORDER BY c.rowid DESC LIMIT 1200")
            };
            let query_vector = learned
                .map(|v| v.to_vec())
                .unwrap_or_else(|| text::embed(query));
            let mut q = self.db.prepare(&sql).map_err(err)?;
            let mut rows = if self.vector {
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
            let mut semantic = vec![];
            while let Some(row) = rows.next().map_err(err)? {
                let (hit, v) = hit_row(row).map_err(err)?;
                let similarity = text::cosine(&query_vector, &v).max(0.);
                let id = string(&hit, "chunkId").to_owned();
                semantic.push((id.clone(), similarity));
                let score = candidates.get(&id).map(|t| t.2).unwrap_or(0.);
                candidates.insert(id, (hit, v, score));
            }
            semantic.sort_by(|a, b| b.1.total_cmp(&a.1));
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
                sort_hits(&mut hits, limit);
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
        sort_hits(&mut hits, limit);
        Ok(hits)
    }
    pub fn gemma_pending(&self, roots: &[String], limit: usize) -> Result<Vec<Value>, String> {
        if roots.is_empty() {
            return Ok(vec![]);
        }
        let filter = roots
            .iter()
            .map(|s| format!("'{}'", s.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        let mut q=self.db.prepare(&format!("SELECT c.rowid,c.id,c.text,s.path FROM knowledge_chunks c CROSS JOIN knowledge_sources s LEFT JOIN knowledge_gemma_embeddings g ON g.chunk_id=c.id WHERE s.id=c.source_id AND s.status='indexed' AND c.embedding_json IS NOT NULL AND s.root_id IN ({filter}) AND (g.chunk_id IS NULL OR g.model_key!=?) ORDER BY c.rowid LIMIT ?")).map_err(err)?;
        let rows=q.query_map(params![MODEL_KEY,limit as i64],|r|{let path:String=r.get(3)?;Ok(json!({"rowId":r.get::<_,i64>(0)?,"id":r.get::<_,String>(1)?,"text":r.get::<_,String>(2)?,"title":title(&path)}))}).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
        Ok(rows)
    }
    pub fn gemma_save(&mut self, chunks: &[Value], vectors: &[Vec<f64>]) -> Result<usize, String> {
        if chunks.len() != vectors.len() {
            return Err("Embedding batch size does not match".into());
        }
        let tx = self.db.transaction().map_err(err)?;
        let mut saved = 0;
        for (c, v) in chunks.iter().zip(vectors) {
            if v.len() != 256 || v.iter().any(|n| !n.is_finite()) {
                return Err("Invalid vector for the Gemma index".into());
            }
            let current:Option<(i64,String)>=tx.query_row("SELECT c.rowid,s.root_id FROM knowledge_chunks c JOIN knowledge_sources s ON s.id=c.source_id WHERE c.id=? AND c.text=? AND s.status='indexed' AND c.embedding_json IS NOT NULL",params![string(c,"id"),string(c,"text")],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(err)?;
            if let Some((row, root)) = current {
                let bytes = text::encode(v);
                tx.execute(
                    "INSERT OR REPLACE INTO knowledge_gemma_embeddings VALUES(?,?,?)",
                    params![string(c, "id"), MODEL_KEY, bytes],
                )
                .map_err(err)?;
                if self.vector {
                    tx.execute("DELETE FROM knowledge_gemma_vectors WHERE rowid=?", [row])
                        .map_err(err)?;
                    tx.execute("INSERT INTO knowledge_gemma_vectors(rowid,embedding,root_id) VALUES(?,?,?)",params![row,bytes,root]).map_err(err)?;
                }
                saved += 1;
            }
        }
        tx.commit().map_err(err)?;
        Ok(saved)
    }
    pub fn gemma_clear(&self) -> Result<(), String> {
        self.db
            .execute("DELETE FROM knowledge_gemma_embeddings", [])
            .map_err(err)?;
        if self.vector {
            self.db
                .execute("DELETE FROM knowledge_gemma_vectors", [])
                .map_err(err)?;
        }
        Ok(())
    }
    pub fn gemma_has(&self, roots: &[String]) -> Result<bool, String> {
        if roots.is_empty() {
            return Ok(false);
        }
        let filter = roots
            .iter()
            .map(|s| format!("'{}'", s.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        self.db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM knowledge_gemma_embeddings g JOIN knowledge_chunks c ON c.id=g.chunk_id JOIN knowledge_sources s ON s.id=c.source_id WHERE g.model_key=? AND s.status='indexed' AND s.root_id IN ({filter}) LIMIT 1)"),[MODEL_KEY],|r|r.get(0)).map_err(err)
    }
    pub fn gemma_counts(&self, roots: &[String]) -> Result<(u64, u64), String> {
        if roots.is_empty() {
            return Ok((0, 0));
        }
        let filter = roots
            .iter()
            .map(|s| format!("'{}'", s.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        self.db.query_row(&format!("SELECT COUNT(g.chunk_id),COUNT(*) FROM knowledge_chunks c JOIN knowledge_sources s ON s.id=c.source_id LEFT JOIN knowledge_gemma_embeddings g ON g.chunk_id=c.id AND g.model_key=? WHERE s.status='indexed' AND c.embedding_json IS NOT NULL AND s.root_id IN ({filter})"),[MODEL_KEY],|r|Ok((r.get(0)?,r.get(1)?))).map_err(err)
    }
    pub fn vector_backfill(&self) -> Result<bool, String> {
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
        let cursor = self
            .meta("rust-gemma-vector-cursor-v1", json!(0))?
            .as_i64()
            .unwrap_or(0);
        let mut stmt=self.db.prepare("SELECT c.rowid,g.vector,s.root_id FROM knowledge_chunks c JOIN knowledge_sources s ON s.id=c.source_id JOIN knowledge_gemma_embeddings g ON g.chunk_id=c.id WHERE c.rowid>? AND g.model_key=? ORDER BY c.rowid LIMIT 256").map_err(err)?;
        let learned = stmt
            .query_map(params![cursor, MODEL_KEY], |r| {
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
                    .execute("DELETE FROM knowledge_gemma_vectors WHERE rowid=?", [id])
                    .map_err(err)?;
                self.db.execute("INSERT INTO knowledge_gemma_vectors(rowid,embedding,root_id) VALUES(?,?,?)",params![id,v,root]).map_err(err)?;
            }
        }
        if let Some((id, _, _)) = learned.last() {
            self.set_meta("rust-gemma-vector-cursor-v1", &json!(id))?;
        }
        Ok(rows.len() == 256 || learned.len() == 256)
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
    let mut v = json!({"chunkId":r.get::<_,String>(0)?,"sourceId":r.get::<_,String>(1)?,"text":r.get::<_,String>(3)?,"path":path,"title":title(&path),"score":0,"lexicalScore":0,"semanticScore":0});
    if let Some(p) = r.get::<_, Option<i64>>(2)? {
        v["pageNumber"] = json!(p);
    }
    Ok((v, text::decode(r.get_ref(5)?)))
}
fn sort_hits(hits: &mut Vec<Value>, limit: usize) {
    hits.sort_by(|a, b| {
        b["score"]
            .as_f64()
            .unwrap_or(0.)
            .total_cmp(&a["score"].as_f64().unwrap_or(0.))
    });
    hits.truncate(limit);
}

fn content_fts_query(query: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let normalized: String = query.to_lowercase().nfkc().collect();
    let terms: Vec<_> = regex::Regex::new(r"[\p{L}\p{N}]+")
        .unwrap()
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
