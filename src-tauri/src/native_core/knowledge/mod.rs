//! Native, profile-scoped knowledge service. SQLite is confined to one bounded
//! dedicated thread; extraction and inference never execute JavaScript workers.
mod db;
mod extract;
mod runtime;
#[cfg(test)]
mod tests;
mod text;
use crate::native_core::common::{now_ms, CoreContext, CoreResult, HandlerResult};
use db::{number, string, Database};
use extract::Cancellation;
use notify::{RecursiveMode, Watcher};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use tokio::sync::{Mutex as AsyncMutex, Notify, OnceCell};

pub const CHANNELS: &[&str] = &[
    "knowledge:snapshot",
    "knowledge:embedding:set",
    "knowledge:embedding:download",
    "knowledge:embedding:cancel-download",
    "knowledge:embedding:remove",
    "knowledge:sources:list",
    "knowledge:choose-folder",
    "knowledge:root:add",
    "knowledge:roots:add-major",
    "knowledge:root:remove",
    "knowledge:root:set-enabled",
    "knowledge:depth:set",
    "knowledge:speed:set",
    "knowledge:root:set-depth",
    "knowledge:index:start",
    "knowledge:index:pause",
    "knowledge:index:resume",
    "core:knowledge:search",
    "core:knowledge:search-metadata",
    "core:knowledge:read",
    "core:knowledge:background",
    "core:knowledge:shutdown",
];
#[derive(Clone)]
pub struct KnowledgeCore {
    inner: Arc<Inner>,
}
struct Inner {
    ctx: CoreContext,
    database: OnceCell<Database>,
    cache: Mutex<Cache>,
    runtime: runtime::Runtime,
    generation: Arc<AtomicU64>,
    closed: AtomicBool,
    paused: AtomicBool,
    requested: AtomicBool,
    dirty: Arc<AtomicU64>,
    wake: Notify,
    task: AsyncMutex<Option<tokio::task::JoinHandle<()>>>,
    run: AsyncMutex<()>,
    mutation: AsyncMutex<()>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
}
struct Cache {
    settings: Value,
    status: Value,
    roots: Vec<Value>,
    storage: u64,
}
impl KnowledgeCore {
    pub fn new(ctx: CoreContext) -> Self {
        let runtime = runtime::Runtime::new(ctx.clone());
        Self {
            inner: Arc::new(Inner {
                ctx,
                database: OnceCell::new(),
                cache: Mutex::new(Cache {
                    settings: db::default_settings(),
                    status: db::default_status(),
                    roots: vec![],
                    storage: 0,
                }),
                runtime,
                generation: Arc::new(AtomicU64::new(0)),
                closed: AtomicBool::new(false),
                paused: AtomicBool::new(false),
                requested: AtomicBool::new(false),
                dirty: Arc::new(AtomicU64::new(0)),
                wake: Notify::new(),
                task: AsyncMutex::new(None),
                run: AsyncMutex::new(()),
                mutation: AsyncMutex::new(()),
                watcher: Mutex::new(None),
            }),
        }
    }
    async fn database(&self) -> Result<Database, String> {
        let ctx = self.inner.ctx.clone();
        let db = self
            .inner
            .database
            .get_or_try_init(|| async move { Database::open(ctx).await })
            .await?;
        Ok(db.clone())
    }
    pub async fn start(&self) -> Result<(), String> {
        if self.inner.closed.load(Ordering::SeqCst) {
            return Err("Knowledge service is shut down".into());
        }
        let mut task = self.inner.task.lock().await;
        if task.is_some() {
            return Ok(());
        }
        let db = self.database().await?;
        let (settings, mut status, roots, storage) = db
            .call(|s| {
                Ok((
                    s.settings()?,
                    s.meta("status", db::default_status())?,
                    s.roots()?,
                    s.storage(),
                ))
            })
            .await?;
        let mut merged = db::default_status();
        if let Some(fields) = status.as_object() {
            for (k, v) in fields {
                merged[k] = v.clone();
            }
        }
        status = merged;
        let paused = status["state"] == "paused";
        self.inner.paused.store(paused, Ordering::SeqCst);
        {
            let mut cache = self.inner.cache.lock().unwrap();
            cache.settings = settings;
            cache.status = status;
            cache.roots = roots;
            cache.storage = storage;
        }
        let checkpoint = db.call(|s| s.checkpoint()).await?;
        if let Some(job) = checkpoint {
            let left = (number(&job, "totalSources") - number(&job, "processedSources")).max(0);
            if left > 0 {
                self.status(json!({"state":if paused{"paused"}else{"indexing"},"jobId":job["jobId"],"progress":number(&job,"processedSources") as f64/number(&job,"totalSources").max(1) as f64,"queuedSources":left,"processedSources":job["processedSources"],"failedSources":job["failedSources"],"detail":format!("Ready to resume {left} files")}));
            } else {
                db.call(|s| s.clear_checkpoint()).await?;
                self.status(json!({"state":"completed","progress":1,"queuedSources":0,"processedSources":job["processedSources"],"failedSources":job["failedSources"]}));
            }
        } else if matches!(
            self.inner.cache.lock().unwrap().status["state"].as_str(),
            Some("scanning" | "indexing")
        ) {
            self.status(json!({"state":"idle","progress":0,"detail":null}));
        }
        self.refresh_counts().await?;
        self.inner.runtime.initialize().await?;
        self.sync_watchers().await?;
        let this = self.clone();
        *task = Some(tokio::spawn(async move {
            this.background_loop().await;
        }));
        Ok(())
    }
    pub async fn shutdown(&self) -> Result<(), String> {
        if self.inner.closed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        self.inner.generation.fetch_add(1, Ordering::SeqCst);
        self.inner.wake.notify_waiters();
        self.inner.watcher.lock().unwrap().take();
        self.inner.runtime.shutdown().await;
        if let Some(task) = self.inner.task.lock().await.take() {
            task.await.map_err(|e| e.to_string())?;
        }
        if self.inner.database.get().is_some() {
            self.persist().await?;
            self.database().await?.close().await?;
        }
        Ok(())
    }
    fn cancel(&self) -> Cancellation {
        Cancellation {
            generation: self.inner.generation.clone(),
            expected: self.inner.generation.load(Ordering::SeqCst),
        }
    }
    fn settings(&self) -> Value {
        self.inner.cache.lock().unwrap().settings.clone()
    }
    fn roots(&self) -> Vec<Value> {
        let cache = self.inner.cache.lock().unwrap();
        cache
            .roots
            .iter()
            .filter(|r| r["enabled"] == true && depth(r, &cache.settings) != "off")
            .cloned()
            .collect()
    }
    fn root_ids(&self) -> Vec<String> {
        self.roots()
            .iter()
            .map(|r| string(r, "id").into())
            .collect()
    }
    fn learned_root_ids(&self) -> Vec<String> {
        let settings = self.settings();
        self.roots()
            .iter()
            .filter(|r| depth(r, &settings) != "basic")
            .map(|r| string(r, "id").into())
            .collect()
    }
    fn background(&self) -> Value {
        let cache = self.inner.cache.lock().unwrap();
        json!({"status":cache.status,"settings":{"embeddingModel":cache.settings["embeddingModel"]},"semanticSearch":self.inner.runtime.snapshot()})
    }
    fn snapshot(&self) -> Value {
        let c = self.inner.cache.lock().unwrap();
        json!({"roots":c.roots,"status":c.status,"localBackendAvailable":true,"cloudBackendAvailable":false,"settings":c.settings,"storageBytes":c.storage,"semanticSearch":self.inner.runtime.snapshot()})
    }
    fn status(&self, patch: Value) {
        let status = {
            let mut c = self.inner.cache.lock().unwrap();
            if let Some(fields) = patch.as_object() {
                for (k, v) in fields {
                    c.status[k] = v.clone();
                }
            }
            c.status.clone()
        };
        self.inner.ctx.emit("knowledge:status", status);
    }
    async fn persist(&self) -> Result<(), String> {
        let status = self.inner.cache.lock().unwrap().status.clone();
        self.database()
            .await?
            .call(move |s| s.set_meta("status", &status))
            .await
    }
    async fn refresh_counts(&self) -> Result<(), String> {
        let counts = self.database().await?.call(|s| s.counts()).await?;
        let mut c = self.inner.cache.lock().unwrap();
        for (k, v) in counts.as_object().unwrap() {
            c.status[k] = v.clone();
        }
        Ok(())
    }
    async fn refresh(&self) -> Result<(), String> {
        let (roots, settings, storage) = self
            .database()
            .await?
            .call(|s| Ok((s.roots()?, s.settings()?, s.storage())))
            .await?;
        {
            let mut c = self.inner.cache.lock().unwrap();
            c.roots = roots;
            c.settings = settings;
            c.storage = storage;
        }
        self.refresh_counts().await?;
        Ok(())
    }
    fn request_index(&self) {
        self.inner.requested.store(true, Ordering::SeqCst);
        self.inner.wake.notify_one();
    }
    async fn sync_watchers(&self) -> Result<(), String> {
        let roots = self.roots();
        let dirty = self.inner.dirty.clone();
        let watcher = tokio::task::spawn_blocking(move || {
            let mut watcher = notify::recommended_watcher(
                move |event: notify::Result<notify::Event>| match event {
                    Ok(event) if !matches!(event.kind, notify::EventKind::Access(_)) => {
                        dirty.store(now_ms(), Ordering::SeqCst);
                    }
                    Err(e) => {
                        log::warn!("Knowledge watcher: {e}");
                        dirty.store(now_ms(), Ordering::SeqCst);
                    }
                    _ => {}
                },
            )
            .map_err(|e| e.to_string())?;
            for root in roots {
                if let Err(error) =
                    watcher.watch(Path::new(string(&root, "path")), RecursiveMode::Recursive)
                {
                    log::warn!("Knowledge folder watch failed: {error}");
                }
            }
            Ok::<_, String>(watcher)
        })
        .await
        .map_err(|e| e.to_string())??;
        *self.inner.watcher.lock().unwrap() = Some(watcher);
        Ok(())
    }
    async fn idle(&self, cancel: &Cancellation) -> Result<(), String> {
        while self.inner.ctx.foreground_active() {
            cancel.check()?;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        cancel.check()
    }
    async fn background_loop(&self) {
        let startup = now_ms() + 8000;
        let mut started = false;
        let mut backfill = true;
        let mut last_vector = 0;
        let mut last_counts = 0;
        let mut last_rescan = now_ms();
        let mut last_semantic = Value::Null;
        let mut retry_learned_after = 0;
        while !self.inner.closed.load(Ordering::SeqCst) {
            if !started && now_ms() >= startup {
                started = true;
                if !self.inner.paused.load(Ordering::SeqCst) {
                    self.request_index();
                }
            }
            let dirty = self.inner.dirty.load(Ordering::SeqCst);
            if dirty > 0 && now_ms().saturating_sub(dirty) >= 5000 {
                self.inner.dirty.store(0, Ordering::SeqCst);
                self.request_index();
            }
            // Periodic reconciliation also recovers from OS watcher overflow/missing permissions.
            if now_ms().saturating_sub(last_rescan) > 300000 {
                last_rescan = now_ms();
                self.request_index();
            }
            if !self.inner.paused.load(Ordering::SeqCst)
                && self.inner.requested.swap(false, Ordering::SeqCst)
            {
                let _run = self.inner.run.lock().await;
                let cancel = self.cancel();
                if !self.inner.paused.load(Ordering::SeqCst)
                    && !self.inner.closed.load(Ordering::SeqCst)
                {
                    if let Err(error) = self.run_index(&cancel).await {
                        if cancel.check().is_ok() {
                            self.status(json!({"state":"failed","error":error}));
                            let _ = self.persist().await;
                        }
                    }
                }
            }
            if self.inner.closed.load(Ordering::SeqCst) {
                break;
            }
            if !self.inner.paused.load(Ordering::SeqCst) && !self.inner.ctx.foreground_active() {
                if backfill && now_ms().saturating_sub(last_vector) > 250 && now_ms() >= startup {
                    last_vector = now_ms();
                    if let Ok(db) = self.database().await {
                        match db.call(|s| s.vector_backfill()).await {
                            Ok(more) => backfill = more,
                            Err(e) => {
                                log::warn!("Knowledge vector backfill: {e}");
                                backfill = false;
                            }
                        }
                    }
                }
                if self.settings()["embeddingModel"] == "embeddinggemma-2"
                    && self.inner.runtime.snapshot()["modelState"] == "ready"
                    && now_ms() >= retry_learned_after
                {
                    let _run = self.inner.run.lock().await;
                    let cancel = self.cancel();
                    if let Err(error) = self.learned_batch(&cancel).await {
                        if cancel.check().is_ok() {
                            retry_learned_after = now_ms() + 30000;
                            self.inner
                                .runtime
                                .patch(json!({"error":error,"indexing":false}));
                        }
                    }
                    if now_ms().saturating_sub(last_counts) > 10000 {
                        last_counts = now_ms();
                        let ids = self.learned_root_ids();
                        if let Ok(db) = self.database().await {
                            if let Ok((indexed, total)) =
                                db.call(move |s| s.gemma_counts(&ids)).await
                            {
                                self.inner
                                    .runtime
                                    .patch(json!({"indexedChunks":indexed,"totalChunks":total}));
                            }
                        }
                    }
                }
            }
            self.inner.runtime.stop_idle().await;
            let semantic = self.inner.runtime.snapshot();
            if semantic != last_semantic {
                last_semantic = semantic;
                self.status(json!({}));
            }
            tokio::select! {_=self.inner.wake.notified()=>{},_=tokio::time::sleep(std::time::Duration::from_millis(250))=>{}}
        }
    }
    async fn learned_batch(&self, cancel: &Cancellation) -> Result<(), String> {
        let settings = self.settings();
        let speed = string(&settings, "indexingSpeed");
        let count = match speed {
            "slow" => 4,
            "fast" => 16,
            _ => 8,
        };
        let ids = self.learned_root_ids();
        let chunks = self
            .database()
            .await?
            .call(move |s| s.gemma_pending(&ids, count))
            .await?;
        if chunks.is_empty() {
            self.inner.runtime.patch(json!({"indexing":false}));
            return Ok(());
        }
        self.inner.runtime.patch(json!({"indexing":true}));
        let inputs = chunks
            .iter()
            .map(|c| {
                format!(
                    "title: {} | text: {}",
                    string(c, "title"),
                    string(c, "text")
                )
            })
            .collect();
        let vectors = self.inner.runtime.embed(inputs, speed, cancel).await?;
        cancel.check()?;
        let condition = cancel.clone();
        self.database()
            .await?
            .call(move |s| {
                condition.check()?;
                s.gemma_save(&chunks, &vectors)
            })
            .await?;
        let delay = match speed {
            "slow" => 350,
            "fast" => 0,
            _ => 75,
        };
        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        Ok(())
    }
    async fn run_index(&self, cancel: &Cancellation) -> Result<(), String> {
        let db = self.database().await?;
        if self.roots().is_empty() {
            db.call(|s| s.clear_checkpoint()).await?;
            self.status(json!({"state":"idle","progress":1,"queuedSources":0,"detail":"No active content-indexing folders"}));
            self.persist().await?;
            return Ok(());
        }
        if db.call(|s| s.checkpoint()).await?.is_none() {
            self.status(json!({"state":"scanning","progress":0,"error":null,"detail":"Scanning knowledge folders"}));
            let roots = self.roots();
            let mut queue = vec![];
            for root in roots {
                self.idle(cancel).await?;
                let root_path = PathBuf::from(string(&root, "path"));
                let path = root_path.clone();
                let cancellation = cancel.clone();
                let ctx = self.inner.ctx.clone();
                let (paths, complete) =
                    tokio::task::spawn_blocking(move || scan(&path, &cancellation, &ctx))
                        .await
                        .map_err(|e| e.to_string())??;
                cancel.check()?;
                let present: HashSet<_> = paths.iter().cloned().collect();
                let root_id = string(&root, "id").to_owned();
                if complete {
                    let id = root_id.clone();
                    let condition = cancel.clone();
                    db.call(move |s| {
                        condition.check()?;
                        s.prune(&id, &present)
                    })
                    .await?;
                }
                queue.extend(paths.into_iter().map(|path| (root_id.clone(), path)));
            }
            let condition = cancel.clone();
            db.call(move |s| {
                condition.check()?;
                s.save_queue(&queue)
            })
            .await?;
        }
        if let Some(checkpoint) = db.call(|s| s.checkpoint()).await? {
            let processed = number(&checkpoint, "processedSources");
            let total = number(&checkpoint, "totalSources");
            self.status(json!({"state":"indexing","jobId":checkpoint["jobId"],"progress":if total>0{processed as f64/total as f64}else{1.},"queuedSources":(total-processed).max(0),"processedSources":processed,"failedSources":checkpoint["failedSources"],"detail":format!("Indexing {} files",(total-processed).max(0))}));
        }
        let settings = self.settings();
        let concurrency = settings["maxConcurrentExtractors"]
            .as_u64()
            .unwrap_or(1)
            .clamp(1, 8) as usize;
        let ocr = Arc::new(tokio::sync::Semaphore::new(
            settings["maxConcurrentOcrJobs"]
                .as_u64()
                .unwrap_or(1)
                .clamp(1, 4) as usize,
        ));
        loop {
            self.idle(cancel).await?;
            let jobs = db.call(move |s| s.next_batch(concurrency)).await?;
            if jobs.is_empty() {
                break;
            }
            use futures_util::{stream::FuturesUnordered, StreamExt};
            let mut futures = FuturesUnordered::new();
            for (job, position, root_id, path) in jobs {
                let root = self
                    .roots()
                    .into_iter()
                    .find(|r| string(r, "id") == root_id);
                let ocr = ocr.clone();
                futures.push(async move {
                    let result = if let Some(root) = root {
                        self.index_one(&root, &path, cancel, ocr).await
                    } else {
                        Ok(())
                    };
                    (job, position, path, result)
                });
            }
            while let Some((job, position, path, result)) = futures.next().await {
                cancel.check()?;
                let failed = if let Err(error) = result {
                    let id = text::hash(&path);
                    let condition = cancel.clone();
                    db.call(move |s| {
                        condition.check()?;
                        s.failed(&id, &error)
                    })
                    .await?;
                    true
                } else {
                    false
                };
                let condition = cancel.clone();
                let checkpoint = db
                    .call(move |s| {
                        condition.check()?;
                        s.complete(&job, position, failed)?;
                        s.checkpoint()
                    })
                    .await?
                    .ok_or("Knowledge queue checkpoint disappeared")?;
                self.refresh_counts().await?;
                let processed = number(&checkpoint, "processedSources");
                let total = number(&checkpoint, "totalSources");
                self.status(json!({"state":"indexing","jobId":checkpoint["jobId"],"processedSources":processed,"failedSources":checkpoint["failedSources"],"queuedSources":(total-processed).max(0),"progress":if total>0{processed as f64/total as f64}else{1.},"detail":format!("Processed {}",db::title(&path))}));
                self.persist().await?;
                let settings = self.settings();
                let delay = match string(&settings, "indexingSpeed") {
                    "slow" => 120,
                    "fast" => 0,
                    _ => 40,
                };
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }
        }
        cancel.check()?;
        let condition = cancel.clone();
        db.call(move |s| {
            condition.check()?;
            s.clear_checkpoint()
        })
        .await?;
        self.refresh().await?;
        self.status(json!({"state":"completed","progress":1,"queuedSources":0,"detail":"Knowledge index is up to date","lastCompletedAt":now_ms()}));
        self.persist().await
    }
    async fn index_one(
        &self,
        root: &Value,
        path: &str,
        cancel: &Cancellation,
        ocr: Arc<tokio::sync::Semaphore>,
    ) -> Result<(), String> {
        let settings = self.settings();
        let selected_depth = depth(root, &settings);
        if selected_depth == "off" {
            return Ok(());
        }
        let profile = text::profile(selected_depth);
        let path_owned = path.to_owned();
        let existing = self
            .database()
            .await?
            .call(move |s| s.source(&path_owned))
            .await?;
        let metadata = tokio::fs::metadata(path).await.map_err(|e| e.to_string())?;
        let modified = modified_ms(&metadata);
        if let Some(existing) = existing {
            if existing["status"] == "indexed"
                && existing["byteSize"].as_u64() == Some(metadata.len())
                && existing["modifiedAt"].as_f64().unwrap_or(0.).round() == modified.round()
                && existing["indexingProfile"] == profile
            {
                return Ok(());
            }
        }
        self.idle(cancel).await?;
        let path_buf = PathBuf::from(path);
        let condition = cancel.clone();
        let ctx = self.inner.ctx.clone();
        let fingerprint=tokio::task::spawn_blocking(move||{use sha2::{Digest,Sha256};use std::io::Read;let mut file=std::fs::File::open(path_buf).map_err(|e|e.to_string())?;let meta=file.metadata().map_err(|e|e.to_string())?;let mut hash=Sha256::new();let mut block=[0u8;65536];loop{condition.check()?;while ctx.foreground_active(){condition.check()?;std::thread::sleep(std::time::Duration::from_millis(50));}let n=file.read(&mut block).map_err(|e|e.to_string())?;if n==0{break;}hash.update(&block[..n]);}Ok::<_,String>(json!({"contentHash":format!("{:x}",hash.finalize()),"byteSize":meta.len(),"modifiedAt":modified_ms(&meta)}))}).await.map_err(|e|e.to_string())??;
        cancel.check()?;
        let id = text::hash(path);
        let (source, root_id, p, fp, key, condition) = (
            id.clone(),
            string(root, "id").to_owned(),
            path.to_owned(),
            fingerprint.clone(),
            profile.clone(),
            cancel.clone(),
        );
        self.database()
            .await?
            .call(move |s| {
                condition.check()?;
                s.pending(&source, &root_id, &p, &fp, &key)
            })
            .await?;
        if root["processingBackend"] == "cloud" {
            return Err("Tezbar Cloud indexing is not available yet.".into());
        }
        let (hash, source, key) = (
            string(&fingerprint, "contentHash").to_owned(),
            id.clone(),
            profile,
        );
        let reusable = self
            .database()
            .await?
            .call(move |s| s.reusable(&hash, &source, &key))
            .await?;
        let result = if let Some(result) = reusable {
            result
        } else {
            let extension = extract::extension(Path::new(path));
            let heavy_ocr = selected_depth != "basic"
                && [
                    "pdf", "png", "jpg", "jpeg", "heic", "heif", "tif", "tiff", "bmp", "gif",
                    "webp",
                ]
                .contains(&extension.as_str());
            let _permit = if heavy_ocr {
                Some(ocr.acquire_owned().await.map_err(|e| e.to_string())?)
            } else {
                None
            };
            cancel.check()?;
            let mut result =
                extract::extract(&self.inner.ctx, Path::new(path), selected_depth, cancel).await?;
            drop(_permit);
            self.idle(cancel).await?;
            let pages = result["pages"].as_array().cloned().unwrap_or_default();
            let source = id.clone();
            let embed = selected_depth != "basic";
            let condition = cancel.clone();
            let ctx = self.inner.ctx.clone();
            let chunks = tokio::task::spawn_blocking(move || {
                text::chunks_checked(&source, &pages, embed, || {
                    condition.check()?;
                    while ctx.foreground_active() {
                        condition.check()?;
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Ok(())
                })
            })
            .await
            .map_err(|e| e.to_string())??;
            result["chunks"] = json!(chunks);
            let has_ocr = result["pages"].as_array().into_iter().flatten().any(|p| {
                p["ocr"]["text"]
                    .as_str()
                    .map(|s| !s.is_empty())
                    .unwrap_or(false)
            });
            result["completedCapabilities"] = if embed {
                json!(["extracted-text", "chunks", "text-embeddings"])
            } else {
                json!(["extracted-text", "chunks"])
            };
            if has_ocr {
                result["completedCapabilities"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("ocr"));
            }
            result
        };
        cancel.check()?;
        let after = tokio::fs::metadata(path).await.map_err(|e| e.to_string())?;
        if after.len() != metadata.len() || modified_ms(&after) != modified {
            self.inner.dirty.store(now_ms(), Ordering::SeqCst);
            return Err("File changed during indexing; queued for rescan".into());
        }
        let condition = cancel.clone();
        self.database()
            .await?
            .call(move |s| {
                condition.check()?;
                s.save_result(&id, &result)
            })
            .await
    }
    async fn search(&self, query: String, limit: usize) -> CoreResult {
        self.inner.ctx.note_foreground();
        let ids = self.root_ids();
        let db = self.database().await?;
        if self.settings()["embeddingModel"] == "embeddinggemma-2" {
            let learned_ids = self.learned_root_ids();
            let have = learned_ids.clone();
            let have = db.call(move |s| s.gemma_has(&have)).await?;
            if have {
                let settings = self.settings();
                let cancel = self.cancel();
                let vector = self
                    .inner
                    .runtime
                    .embed(
                        vec![format!("task: search result | query: {}", query.trim())],
                        string(&settings, "indexingSpeed"),
                        &cancel,
                    )
                    .await;
                if let Ok(mut vector) = vector {
                    if self.settings()["embeddingModel"] == "embeddinggemma-2" {
                        let current = self.root_ids();
                        let learned = self.learned_root_ids();
                        let vector = vector.remove(0);
                        let q = query.clone();
                        let hits = db
                            .call(move |s| {
                                s.search(&q, limit, &current, false, Some((&vector, &learned)))
                            })
                            .await?;
                        return Ok(json!(hits));
                    }
                }
            }
            return db
                .call(move |s| Ok(json!(s.search(&query, limit, &ids, true, None)?)))
                .await;
        }
        db.call(move |s| Ok(json!(s.search(&query, limit, &ids, false, None)?)))
            .await
    }
    pub async fn handle(&self, channel: &str, args: &[Value]) -> HandlerResult {
        if !CHANNELS.contains(&channel) {
            return None;
        }
        if channel == "core:knowledge:background" {
            return Some(Ok(self.background()));
        }
        Some(self.dispatch(channel, args).await)
    }
    async fn dispatch(&self, channel: &str, args: &[Value]) -> CoreResult {
        if channel == "core:knowledge:shutdown" {
            self.shutdown().await?;
            return Ok(Value::Null);
        }
        self.start().await?;
        match channel {
            "knowledge:snapshot" => {
                self.refresh().await?;
                return Ok(self.snapshot());
            }
            "knowledge:sources:list" => {
                let input = args.first().cloned().unwrap_or(json!({}));
                if !input.is_object() && !input.is_null() {
                    return Err("Knowledge source query must be an object".into());
                }
                return self
                    .database()
                    .await?
                    .call(move |s| s.list_sources(input))
                    .await;
            }
            "core:knowledge:search-metadata" => {
                self.inner.ctx.note_foreground();
                let query = required(args, 0, "A metadata query is required")?.to_owned();
                let limit = integer(args.get(1), 20, 0, 500)?;
                let roots = self.root_ids();
                return self
                    .database()
                    .await?
                    .call(move |s| s.search_metadata(&query, limit, &roots))
                    .await;
            }
            "core:knowledge:search" => {
                let query = required(args, 0, "A knowledge search query is required")?.to_owned();
                let limit = integer(args.get(1), 12, 0, 500)?;
                return self.search(query, limit).await;
            }
            "core:knowledge:read" => {
                self.inner.ctx.note_foreground();
                let id = required(args, 0, "A knowledge result id is required")?.to_owned();
                let max = integer(args.get(1), 12000, 0, 1000000)?;
                let roots = self.root_ids();
                return self
                    .database()
                    .await?
                    .call(move |s| s.read(&id, max, &roots))
                    .await;
            }
            "knowledge:choose-folder" => {
                use tauri_plugin_dialog::DialogExt;
                let app=self.inner.ctx.app.as_ref().ok_or("Folder selection requires a desktop application; pass an explicit root path in headless mode")?;
                let (tx, rx) = tokio::sync::oneshot::channel();
                app.dialog()
                    .file()
                    .set_title("Choose a Knowledge folder")
                    .pick_folder(move |path| {
                        let _ = tx.send(path.and_then(|p| p.into_path().ok()));
                    });
                return Ok(rx
                    .await
                    .map_err(|e| e.to_string())?
                    .map(|p| json!(p))
                    .unwrap_or(Value::Null));
            }
            "knowledge:embedding:download" => {
                self.inner.runtime.download().await?;
                return Ok(self.snapshot());
            }
            "knowledge:embedding:cancel-download" => {
                self.inner.runtime.cancel_download().await?;
                return Ok(self.snapshot());
            }
            _ => {}
        }
        let _mutation = self.inner.mutation.lock().await;
        // Validate all payloads before interrupting an active job.
        match channel {
            "knowledge:root:add" => {
                required(args, 0, "A folder path is required")?;
            }
            "knowledge:root:remove" => {
                required(args, 0, "A knowledge folder id is required")?;
            }
            "knowledge:depth:set" => {
                choice(
                    args.first(),
                    &["off", "basic", "smart", "deep"],
                    "Invalid Knowledge Depth",
                )?;
            }
            "knowledge:speed:set" => {
                choice(
                    args.first(),
                    &["slow", "medium", "fast"],
                    "Invalid indexing speed",
                )?;
            }
            "knowledge:embedding:set" => {
                let model = choice(
                    args.first(),
                    &["built-in", "embeddinggemma-2"],
                    "Invalid embedding model",
                )?;
                if model == "embeddinggemma-2"
                    && self.inner.runtime.snapshot()["modelState"] != "ready"
                {
                    return Err("Download EmbeddingGemma before enabling it".into());
                }
            }
            "knowledge:root:set-enabled" => {
                let p = args.first().ok_or("Invalid knowledge folder update")?;
                if p["rootId"].as_str().is_none() || p["enabled"].as_bool().is_none() {
                    return Err("Invalid knowledge folder update".into());
                }
            }
            "knowledge:root:set-depth" => {
                let p = args.first().ok_or("Invalid folder Knowledge Depth")?;
                if p["rootId"].as_str().is_none() {
                    return Err("Invalid folder Knowledge Depth".into());
                }
                choice(
                    p.get("depth"),
                    &["inherit", "off", "basic", "smart", "deep"],
                    "Invalid folder Knowledge Depth",
                )?;
            }
            _ => {}
        }
        let was_active = matches!(
            self.inner.cache.lock().unwrap().status["state"].as_str(),
            Some("scanning" | "indexing")
        );
        self.inner.generation.fetch_add(1, Ordering::SeqCst);
        let _run = self.inner.run.lock().await;
        let db = self.database().await?;
        let mut rescan = was_active;
        let mut watchers = false;
        match channel {
            "knowledge:index:pause" => {
                self.inner.paused.store(true, Ordering::SeqCst);
                self.status(json!({"state":"paused","detail":"Knowledge indexing paused"}));
                self.inner.runtime.patch(json!({"indexing":false}));
                self.inner.runtime.stop().await;
            }
            "knowledge:index:start" | "knowledge:index:resume" => {
                self.inner.paused.store(false, Ordering::SeqCst);
                rescan = true;
            }
            "knowledge:root:add" => {
                let path = PathBuf::from(required(args, 0, "A folder path is required")?);
                db.call(move |s| {
                    s.root_add(&path)?;
                    s.clear_checkpoint()
                })
                .await?;
                rescan = true;
                watchers = true;
            }
            "knowledge:roots:add-major" => {
                let home = self.inner.ctx.paths.home_dir.clone();
                db.call(move |s| {
                    for name in ["Desktop", "Documents", "Downloads", "Pictures"] {
                        let path = home.join(name);
                        if path.is_dir() {
                            s.root_add(&path)?;
                            s.db.execute(
                                "UPDATE knowledge_roots SET enabled=1,updated_at=? WHERE path=?",
                                rusqlite::params![
                                    now_ms(),
                                    db::normalize_root(&path)?.to_string_lossy()
                                ],
                            )
                            .map_err(|e| e.to_string())?;
                        }
                    }
                    s.clear_checkpoint()
                })
                .await?;
                rescan = true;
                watchers = true;
            }
            "knowledge:root:remove" => {
                let id = required(args, 0, "A knowledge folder id is required")?.to_owned();
                db.call(move |s| s.root_remove(&id)).await?;
                db.call(|s| s.clear_checkpoint()).await?;
                rescan = true;
                watchers = true;
            }
            "knowledge:root:set-enabled" | "knowledge:root:set-depth" => {
                let p = args[0].clone();
                let enabled = channel == "knowledge:root:set-enabled";
                db.call(move |s| {
                    let rows = if enabled {
                        s.db.execute(
                            "UPDATE knowledge_roots SET enabled=?,updated_at=? WHERE id=?",
                            rusqlite::params![
                                p["enabled"].as_bool().unwrap(),
                                now_ms(),
                                string(&p, "rootId")
                            ],
                        )
                    } else {
                        s.db.execute(
                            "UPDATE knowledge_roots SET depth=?,updated_at=? WHERE id=?",
                            rusqlite::params![string(&p, "depth"), now_ms(), string(&p, "rootId")],
                        )
                    }
                    .map_err(|e| e.to_string())?;
                    if rows == 0 {
                        return Err("Knowledge folder was not found".into());
                    }
                    s.clear_checkpoint()
                })
                .await?;
                rescan = true;
                watchers = true;
            }
            "knowledge:depth:set" | "knowledge:speed:set" | "knowledge:embedding:set" => {
                let key = match channel {
                    "knowledge:depth:set" => "depth",
                    "knowledge:speed:set" => "indexingSpeed",
                    _ => "embeddingModel",
                };
                let value = args[0].clone();
                db.call(move |s| {
                    let mut settings = s.settings()?;
                    settings[key] = value;
                    s.set_meta("settings", &settings)?;
                    if key == "depth" {
                        s.clear_checkpoint()?;
                    }
                    Ok(())
                })
                .await?;
                rescan |= key == "depth";
                watchers = key == "depth";
                if key != "depth" {
                    self.inner.runtime.stop().await;
                }
            }
            "knowledge:embedding:remove" => {
                db.call(|s| {
                    let mut settings = s.settings()?;
                    settings["embeddingModel"] = json!("built-in");
                    s.set_meta("settings", &settings)?;
                    s.gemma_clear()
                })
                .await?;
                self.inner.runtime.remove().await?;
            }
            _ => return Err(format!("Unhandled owned knowledge channel: {channel}")),
        }
        self.refresh().await?;
        if watchers {
            self.sync_watchers().await?;
        }
        if rescan && !self.inner.paused.load(Ordering::SeqCst) {
            self.request_index();
        }
        self.persist().await?;
        Ok(self.snapshot())
    }
}
fn depth<'a>(root: &'a Value, settings: &'a Value) -> &'a str {
    let d = string(root, "depth");
    if d == "inherit" || d == "text" || d == "text-and-images" {
        string(settings, "depth")
    } else {
        d
    }
}
fn required<'a>(args: &'a [Value], index: usize, message: &str) -> Result<&'a str, String> {
    args.get(index)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or(message.to_owned())
}
fn choice<'a>(
    value: Option<&'a Value>,
    options: &[&str],
    message: &str,
) -> Result<&'a str, String> {
    let v = value.and_then(Value::as_str).ok_or(message.to_owned())?;
    if !options.contains(&v) {
        return Err(message.into());
    }
    Ok(v)
}
fn integer(value: Option<&Value>, default: usize, min: usize, max: usize) -> Result<usize, String> {
    match value {
        None => Ok(default),
        Some(v) => {
            let n = v.as_f64().ok_or("Expected a finite numeric limit")?;
            if !n.is_finite() {
                return Err("Expected a finite numeric limit".into());
            }
            Ok(n.round().clamp(min as f64, max as f64) as usize)
        }
    }
}
fn modified_ms(meta: &std::fs::Metadata) -> f64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|t| t.as_secs_f64() * 1000.)
        .unwrap_or(0.)
}
fn scan(
    path: &Path,
    cancel: &Cancellation,
    ctx: &CoreContext,
) -> Result<(Vec<String>, bool), String> {
    let mut paths = vec![];
    let mut complete = true;
    let mut visited = 0;
    let iter = walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            e.depth() == 0
                || !extract::skip(&e.file_name().to_string_lossy(), e.file_type().is_dir())
        });
    for entry in iter {
        cancel.check()?;
        while ctx.foreground_active() {
            cancel.check()?;
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        visited += 1;
        if visited > 75000 {
            complete = false;
            break;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(_) => {
                complete = false;
                continue;
            }
        };
        if !entry.file_type().is_file() || !extract::indexable(entry.path()) {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => {
                complete = false;
                continue;
            }
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if extract::extension(entry.path()).is_empty() && meta.permissions().mode() & 0o111 != 0
            {
                continue;
            }
        }
        if meta.len() <= extract::max_bytes(entry.path()) {
            paths.push(entry.path().to_string_lossy().into_owned());
        }
    }
    Ok((paths, complete))
}
