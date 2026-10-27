//! Native, profile-scoped knowledge service. SQLite is confined to one bounded
//! dedicated thread; extraction and inference never execute JavaScript workers.
mod db;
mod embedding_schema;
mod extract;
mod runtime;
#[cfg(test)]
mod tests;
mod text;
use crate::native_core::common::{now_ms, CoreContext, CoreResult, HandlerResult};
use db::{number, string, Database};
use extract::Cancellation;
use notify::{RecursiveMode, Watcher};
use runtime::{EmbeddingVariantId, ForegroundEmbedding, Runtime};
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
    "core:knowledge:search-sources",
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
    runtimes: [Runtime; 2],
    semantic_generation: Arc<AtomicU64>,
    semantic_changing: AtomicBool,
    generation: Arc<AtomicU64>,
    search_generations: [Arc<AtomicU64>; 2],
    closed: AtomicBool,
    paused: AtomicBool,
    requested: AtomicBool,
    dirty: Arc<AtomicU64>,
    wake: Notify,
    task: AsyncMutex<Option<tokio::task::JoinHandle<()>>>,
    prewarm_task: AsyncMutex<Option<tokio::task::JoinHandle<()>>>,
    run: AsyncMutex<()>,
    mutation: AsyncMutex<()>,
    refresh_lock: AsyncMutex<()>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
}
// Fence requests that begin during a selection's asynchronous persistence.
// Drop also fences the error path and futures dropped by an IPC caller.
struct SemanticChange<'a> {
    generation: &'a AtomicU64,
    changing: &'a AtomicBool,
}
impl Drop for SemanticChange<'_> {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.changing.store(false, Ordering::SeqCst);
    }
}
struct Cache {
    settings: Value,
    status: Value,
    roots: Vec<Value>,
    storage: u64,
}
impl KnowledgeCore {
    pub fn new(ctx: CoreContext) -> Self {
        let runtimes = EmbeddingVariantId::ALL.map(|v| Runtime::new_for_variant(ctx.clone(), v));
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
                runtimes,
                semantic_generation: Arc::default(),
                semantic_changing: AtomicBool::new(false),
                generation: Arc::new(AtomicU64::new(0)),
                search_generations: [Arc::default(), Arc::default()],
                closed: AtomicBool::new(false),
                paused: AtomicBool::new(false),
                requested: AtomicBool::new(false),
                dirty: Arc::new(AtomicU64::new(0)),
                wake: Notify::new(),
                task: AsyncMutex::new(None),
                prewarm_task: AsyncMutex::new(None),
                run: AsyncMutex::new(()),
                mutation: AsyncMutex::new(()),
                refresh_lock: AsyncMutex::new(()),
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
        db.pause_search_cache(paused);
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
        for runtime in &self.inner.runtimes {
            runtime.initialize().await?;
        }
        // Large learned-index counts belong to the background loop. Settings
        // must become readable before a million-chunk scan finishes.
        self.prewarm_sources().await;
        self.sync_watchers().await?;
        let this = self.clone();
        *self.inner.prewarm_task.lock().await = Some(tokio::spawn(async move {
            this.prewarm_loop().await;
        }));
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
        self.inner
            .semantic_generation
            .fetch_add(1, Ordering::SeqCst);
        for generation in &self.inner.search_generations {
            generation.fetch_add(1, Ordering::SeqCst);
        }
        if let Some(db) = self.inner.database.get() {
            db.pause_search_cache(true);
        }
        self.inner.wake.notify_waiters();
        self.inner.watcher.lock().unwrap().take();
        for runtime in &self.inner.runtimes {
            runtime.shutdown().await;
        }
        if let Some(task) = self.inner.prewarm_task.lock().await.take() {
            task.await.map_err(|e| e.to_string())?;
        }
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
    fn selected_variant(&self) -> EmbeddingVariantId {
        serde_json::from_value(self.settings()["embeddingVariant"].clone()).unwrap_or_default()
    }
    fn runtime(&self, variant: EmbeddingVariantId) -> &Runtime {
        &self.inner.runtimes[match variant {
            EmbeddingVariantId::Q8_0 => 0,
            EmbeddingVariantId::Q4KM => 1,
        }]
    }
    fn selected_runtime(&self) -> &Runtime {
        self.runtime(self.selected_variant())
    }
    fn semantic_change(&self) -> SemanticChange<'_> {
        self.inner.semantic_changing.store(true, Ordering::SeqCst);
        self.inner
            .semantic_generation
            .fetch_add(1, Ordering::SeqCst);
        SemanticChange {
            generation: &self.inner.semantic_generation,
            changing: &self.inner.semantic_changing,
        }
    }
    fn semantic_cancel(&self) -> Cancellation {
        Cancellation {
            generation: self.inner.semantic_generation.clone(),
            expected: self.inner.semantic_generation.load(Ordering::SeqCst),
        }
    }
    fn semantic_snapshot(&self, settings: &Value) -> Value {
        let variant =
            serde_json::from_value(settings["embeddingVariant"].clone()).unwrap_or_default();
        let mut status = self.runtime(variant).snapshot();
        status["variants"] =
            json!(EmbeddingVariantId::ALL.map(|v| self.runtime(v).snapshot_variant()));
        status["indexVariant"] = json!(variant);
        status
    }
    fn background(&self) -> Value {
        let cache = self.inner.cache.lock().unwrap();
        json!({"status":cache.status,"settings":{"embeddingModel":cache.settings["embeddingModel"],"embeddingVariant":cache.settings["embeddingVariant"]},"semanticSearch":self.semantic_snapshot(&cache.settings)})
    }
    fn snapshot(&self) -> Value {
        let c = self.inner.cache.lock().unwrap();
        json!({"roots":c.roots,"status":c.status,"localBackendAvailable":true,"cloudBackendAvailable":false,"settings":c.settings,"storageBytes":c.storage,"semanticSearch":self.semantic_snapshot(&c.settings)})
    }
    async fn refresh_semantic_counts(&self) -> Result<(), String> {
        let ids = self.learned_root_ids();
        let (counts, storage) = self
            .database()
            .await?
            .call(move |s| {
                let counts = EmbeddingVariantId::ALL
                    .map(|v| s.gemma_counts_for_model(&ids, v.descriptor().model_key))
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((counts, s.storage()))
            })
            .await?;
        // Snapshot polling no longer reads SQLite. Keep disk usage current as
        // background extraction and embedding add data to the profile.
        self.inner.cache.lock().unwrap().storage = storage;
        for (v, (indexed, total)) in EmbeddingVariantId::ALL.into_iter().zip(counts) {
            self.runtime(v)
                .patch(json!({"indexedChunks":indexed,"totalChunks":total}));
        }
        Ok(())
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
        // Serialize the DB read AND cache publication. A delayed old snapshot
        // must finish before a mutation's authoritative refresh can complete.
        let _refresh = self.inner.refresh_lock.lock().await;
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
        self.refresh_semantic_counts().await?;
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
    // Verification may finish while the extraction loop is processing a long
    // source queue. One service-owned observer warms the selected installation
    // independently; query supersession never owns or restarts this task.
    async fn prewarm_loop(&self) {
        let mut prewarmed = None;
        while !self.inner.closed.load(Ordering::SeqCst) {
            let settings = self.settings();
            let variant =
                serde_json::from_value(settings["embeddingVariant"].clone()).unwrap_or_default();
            let runtime = self.runtime(variant);
            let state = runtime.snapshot();
            if settings["embeddingModel"] == "embeddinggemma-2"
                && state["modelState"] == "ready"
                && state["integrityState"] == "verified"
                && !self.inner.paused.load(Ordering::SeqCst)
            {
                if prewarmed != Some(variant)
                    && state["error"].is_null()
                    && !self.inner.semantic_changing.load(Ordering::SeqCst)
                {
                    let _ = runtime.prewarm(string(&settings, "indexingSpeed")).await;
                    self.prewarm_sources().await;
                    prewarmed = Some(variant);
                }
            } else {
                prewarmed = None;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
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
        let mut last_variant = self.selected_variant();
        let mut last_semantic_epoch = self.inner.semantic_generation.load(Ordering::SeqCst);
        while !self.inner.closed.load(Ordering::SeqCst) {
            let variant = self.selected_variant();
            let epoch = self.inner.semantic_generation.load(Ordering::SeqCst);
            if epoch != last_semantic_epoch {
                retry_learned_after = 0;
                last_semantic_epoch = epoch;
            }
            if variant != last_variant {
                last_variant = variant;
                retry_learned_after = 0;
                last_counts = 0;
                backfill = true;
            }
            let runtime = self.runtime(variant);
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
                        let key = runtime.model_key();
                        match db.call(move |s| s.vector_backfill_for_model(key)).await {
                            Ok(more) => backfill = more,
                            Err(e) => {
                                log::warn!("Knowledge vector backfill: {e}");
                                backfill = false;
                            }
                        }
                    }
                }
                if self.settings()["embeddingModel"] == "embeddinggemma-2"
                    && runtime.snapshot()["modelState"] == "ready"
                    && runtime.snapshot()["error"].is_null()
                    && !self.inner.semantic_changing.load(Ordering::SeqCst)
                    && now_ms() >= retry_learned_after
                {
                    let _run = self.inner.run.lock().await;
                    let cancel = self.semantic_cancel();
                    if let Err(error) = self.learned_batch(&cancel).await {
                        if cancel.check().is_ok() {
                            retry_learned_after = now_ms() + 30000;
                            log::warn!("Knowledge semantic indexing: {error}");
                        }
                    }
                }
            }
            if now_ms().saturating_sub(last_counts) > 10000 {
                last_counts = now_ms();
                let _ = self.refresh_semantic_counts().await;
            }
            for runtime in &self.inner.runtimes {
                runtime.stop_idle().await;
            }
            let semantic = self.semantic_snapshot(&self.settings());
            if semantic != last_semantic {
                last_semantic = semantic;
                self.status(json!({}));
            }
            tokio::select! {_=self.inner.wake.notified()=>{},_=tokio::time::sleep(std::time::Duration::from_millis(250))=>{}}
        }
    }
    async fn learned_batch(&self, cancel: &Cancellation) -> Result<(), String> {
        // Capture identity and service epoch before the first await. A switch
        // cancels only this domain; extracted-source jobs keep their checkpoint.
        cancel.check()?;
        if self.inner.semantic_changing.load(Ordering::SeqCst) {
            return Err("Knowledge model selection is changing".into());
        }
        let settings = self.settings();
        let variant =
            serde_json::from_value(settings["embeddingVariant"].clone()).unwrap_or_default();
        let runtime = self.runtime(variant);
        let key = runtime.model_key();
        if settings["embeddingModel"] != "embeddinggemma-2" {
            return Ok(());
        }
        let speed = string(&settings, "indexingSpeed");
        let count = match speed {
            "slow" => 4,
            "fast" => 16,
            _ => 8,
        };
        let ids = self.learned_root_ids();
        let condition = cancel.clone();
        let chunks = self
            .database()
            .await?
            .call(move |s| {
                condition.check()?;
                s.gemma_pending_for_model(&ids, count, key)
            })
            .await?;
        cancel.check()?;
        if chunks.is_empty() {
            runtime.patch(json!({"indexing":false}));
            return Ok(());
        }
        runtime.patch(json!({"indexing":true}));
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
        // Runtime owns runtime-generation/error attribution (including Repair).
        // Do not republish a late inference error over a newer install/retry.
        let vectors = match runtime.embed(inputs, speed, cancel).await {
            Ok(vectors) => vectors,
            Err(error) => {
                if cancel.check().is_ok() {
                    runtime.patch(json!({"indexing":false}));
                }
                return Err(error);
            }
        };
        let result = async {
            cancel.check()?;
            let condition = cancel.clone();
            self.database()
                .await?
                .call(move |s| {
                    condition.check()?;
                    s.gemma_save_for_model(&chunks, &vectors, key)
                })
                .await?;
            cancel.check()?;
            self.refresh_semantic_counts().await?;
            Ok::<_, String>(())
        }
        .await;
        // Never publish an obsolete completion after a selection change.
        if cancel.check().is_ok() {
            if let Err(error) = &result {
                runtime.patch(json!({"indexing":false,"error":error,"errorSource":"runtime"}));
            } else {
                runtime.patch(json!({"indexing":false}));
            }
        }
        result?;
        let delay = match speed {
            "slow" => 350,
            "fast" => 0,
            _ => 75,
        };
        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        cancel.check()
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
    async fn prewarm_sources(&self) {
        // Queue context only, including on scope/model changes. Never allocate
        // a dense lane or scan authoritative vectors on the async/service task.
        if self.inner.paused.load(Ordering::SeqCst) {
            return;
        }
        let roots = self.root_ids();
        let learned = self.learned_root_ids();
        let condition = self.semantic_cancel();
        if self.inner.semantic_changing.load(Ordering::SeqCst) {
            return;
        }
        let settings = self.settings();
        let variant =
            serde_json::from_value(settings["embeddingVariant"].clone()).unwrap_or_default();
        let runtime = self.runtime(variant);
        let key = runtime.model_key();
        let state = runtime.snapshot();
        let use_learned = settings["embeddingModel"] == "embeddinggemma-2"
            && state["modelState"] == "ready"
            && state["integrityState"] == "verified"
            && state["error"].is_null();
        if let Some(db) = self.inner.database.get() {
            if let Err(error) = db
                .call(move |s| {
                    condition.check()?;
                    if use_learned && s.gemma_has_for_model(&learned, key)? {
                        s.prewarm_source_cache(&roots, &learned, Some(key), 256)
                    } else {
                        s.prewarm_source_cache(&roots, &roots, None, 384)
                    }
                })
                .await
            {
                log::debug!("Knowledge prewarm context: {error}");
            }
        }
    }
    async fn search(&self, query: String, limit: usize, sources: bool) -> CoreResult {
        let request =
            db::SearchGuard::new(self.inner.search_generations[usize::from(sources)].clone());
        let lifecycle = self.cancel();
        let semantic = self.semantic_cancel();
        if self.inner.semantic_changing.load(Ordering::SeqCst) {
            return Err("Knowledge model selection is changing".into());
        }
        let settings = self.settings();
        let variant =
            serde_json::from_value(settings["embeddingVariant"].clone()).unwrap_or_default();
        let runtime = self.runtime(variant);
        let key = runtime.model_key();
        self.inner.ctx.note_foreground();
        let ids = self.root_ids();
        let db = self.database().await?;
        if settings["embeddingModel"] == "embeddinggemma-2" {
            let learned = self.learned_root_ids();
            let have = learned.clone();
            let have = db
                .call_search(request.0.clone(), move |s| {
                    s.gemma_has_for_model(&have, key)
                })
                .await?;
            request.0.check()?;
            lifecycle.check()?;
            semantic.check()?;
            if have {
                let cancel = request.0.cancellation();
                let vector = runtime
                    .foreground_embed(
                        vec![format!("task: search result | query: {}", query.trim())],
                        string(&settings, "indexingSpeed"),
                        &cancel,
                    )
                    .await;
                request.0.check()?;
                lifecycle.check()?;
                semantic.check()?;
                if let Ok(ForegroundEmbedding::Ready(mut vectors)) = vector {
                    if let Some(vector) = vectors.pop() {
                        let q = query.clone();
                        let current = ids.clone();
                        let condition = semantic.clone();
                        let hits = db
                            .call_search(request.0.clone(), move |s| {
                                condition.check()?;
                                if sources {
                                    s.search_sources_with_model(
                                        &q,
                                        limit,
                                        &current,
                                        false,
                                        Some((&vector, &learned, key)),
                                    )
                                } else {
                                    s.search_with_model(
                                        &q,
                                        limit,
                                        &current,
                                        false,
                                        Some((&vector, &learned, key)),
                                    )
                                }
                            })
                            .await?;
                        request.0.check()?;
                        lifecycle.check()?;
                        semantic.check()?;
                        return Ok(json!(hits));
                    }
                }
            }
            // Warming, Busy, absent vectors and visible runtime failures all
            // retain the independent built-in hybrid lane.
        }
        request.0.check()?;
        lifecycle.check()?;
        semantic.check()?;
        let condition = semantic.clone();
        let hits = db
            .call_search(request.0.clone(), move |s| {
                condition.check()?;
                Ok(json!(if sources {
                    s.search_sources(&query, limit, &ids, false, None)?
                } else {
                    s.search(&query, limit, &ids, false, None)?
                }))
            })
            .await?;
        request.0.check()?;
        lifecycle.check()?;
        semantic.check()?;
        Ok(hits)
    }
    async fn change_embedding(
        &self,
        channel: &str,
        args: &[Value],
        variant: EmbeddingVariantId,
    ) -> CoreResult {
        let _mutation = self.inner.mutation.lock().await;
        let settings = self.settings();
        let previous = self.selected_variant();
        let runtime = self.runtime(variant);
        let removing = channel == "knowledge:embedding:remove";
        let model = if removing {
            "built-in"
        } else {
            choice(
                args.first(),
                &["built-in", "embeddinggemma-2"],
                "Invalid embedding model",
            )?
        };
        if !removing && model == "embeddinggemma-2" && runtime.snapshot()["modelState"] != "ready" {
            return Err("Download EmbeddingGemma before enabling it".into());
        }
        let active_remove =
            removing && previous == variant && settings["embeddingModel"] == "embeddinggemma-2";
        let same = !removing && settings["embeddingModel"] == model && previous == variant;
        if same {
            // Retry start is explicit; healthy prewarm is itself idempotent.
            let _retry = (model == "embeddinggemma-2" && runtime.snapshot()["error"].is_string())
                .then(|| self.semantic_change());
            if model == "embeddinggemma-2" {
                runtime.prewarm(string(&settings, "indexingSpeed")).await?;
            }
            self.refresh_semantic_counts().await?;
            self.inner.wake.notify_one();
            return Ok(self.snapshot());
        }
        let _change = (!removing || active_remove).then(|| self.semantic_change());
        if !removing || active_remove {
            for g in &self.inner.search_generations {
                g.fetch_add(1, Ordering::SeqCst);
            }
            self.runtime(previous).patch(json!({"indexing":false}));
            self.runtime(previous).stop().await;
        }
        let key = runtime.model_key();
        let model = model.to_owned();
        self.database()
            .await?
            .call(move |s| {
                if !removing || active_remove {
                    let mut settings = s.settings()?;
                    settings["embeddingModel"] = json!(model);
                    if !removing {
                        settings["embeddingVariant"] = json!(variant);
                    }
                    s.set_meta("settings", &settings)?;
                }
                if removing {
                    s.gemma_clear_for_model(key)?;
                }
                Ok(())
            })
            .await?;
        if removing {
            runtime.remove().await?;
        }
        self.refresh().await?;
        if !removing
            && self.settings()["embeddingModel"] == "embeddinggemma-2"
            && !self.inner.paused.load(Ordering::SeqCst)
        {
            runtime
                .prewarm(string(&self.settings(), "indexingSpeed"))
                .await?;
        }
        drop(_change);
        self.prewarm_sources().await;
        self.inner.wake.notify_one();
        Ok(self.snapshot())
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
        // Reject invalid/unknown optional variants before startup or cancellation.
        let variant = match channel {
            "knowledge:embedding:set" => {
                choice(
                    args.first(),
                    &["built-in", "embeddinggemma-2"],
                    "Invalid embedding model",
                )?;
                Some(ipc_variant(args.get(1))?)
            }
            "knowledge:embedding:download"
            | "knowledge:embedding:cancel-download"
            | "knowledge:embedding:remove" => Some(ipc_variant(args.first())?),
            _ => None,
        };
        self.start().await?;
        if matches!(
            channel,
            "knowledge:embedding:set" | "knowledge:embedding:remove"
        ) {
            return self.change_embedding(channel, args, variant.unwrap()).await;
        }
        match channel {
            "knowledge:snapshot" => {
                // start() hydrated persisted settings/roots and mutations publish
                // their authoritative refresh before returning. Never queue UI
                // polling behind indexing or repeat full learned-index counts.
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
            "core:knowledge:search" | "core:knowledge:search-sources" => {
                let query = required(args, 0, "A knowledge search query is required")?.to_owned();
                let limit = integer(args.get(1), 12, 0, 500)?;
                return self
                    .search(query, limit, channel == "core:knowledge:search-sources")
                    .await;
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
                self.runtime(variant.unwrap()).download().await?;
                return Ok(self.snapshot());
            }
            "knowledge:embedding:cancel-download" => {
                self.runtime(variant.unwrap()).cancel_download().await?;
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
        self.inner
            .semantic_generation
            .fetch_add(1, Ordering::SeqCst);
        for generation in &self.inner.search_generations {
            generation.fetch_add(1, Ordering::SeqCst);
        }
        let _run = self.inner.run.lock().await;
        // The obsolete semantic batch has now exited. Source mutations keep
        // their existing lifecycle while its old progress flag is cleared.
        for runtime in &self.inner.runtimes {
            runtime.patch(json!({"indexing":false}));
        }
        let db = self.database().await?;
        let mut rescan = was_active;
        let mut watchers = false;
        match channel {
            "knowledge:index:pause" => {
                self.inner.paused.store(true, Ordering::SeqCst);
                self.database().await?.pause_search_cache(true);
                self.status(json!({"state":"paused","detail":"Knowledge indexing paused"}));
                for runtime in &self.inner.runtimes {
                    runtime.patch(json!({"indexing":false}));
                    runtime.stop().await;
                }
            }
            "knowledge:index:start" | "knowledge:index:resume" => {
                self.inner.paused.store(false, Ordering::SeqCst);
                self.database().await?.pause_search_cache(false);
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
            "knowledge:depth:set" | "knowledge:speed:set" => {
                let key = match channel {
                    "knowledge:depth:set" => "depth",
                    "knowledge:speed:set" => "indexingSpeed",
                    _ => "indexingSpeed",
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
                    self.selected_runtime().stop().await;
                }
            }
            _ => return Err(format!("Unhandled owned knowledge channel: {channel}")),
        }
        self.refresh().await?;
        self.prewarm_sources().await;
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
fn ipc_variant(value: Option<&Value>) -> Result<EmbeddingVariantId, String> {
    match value {
        None => Ok(EmbeddingVariantId::Q8_0),
        Some(v) => {
            serde_json::from_value(v.clone()).map_err(|_| "Invalid embedding variant".into())
        }
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
