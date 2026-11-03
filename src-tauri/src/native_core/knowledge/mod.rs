//! Native, profile-scoped knowledge service. SQLite is confined to one bounded
//! dedicated thread; extraction and inference never execute JavaScript workers.
mod db;
mod embedding_schema;
mod extract;
mod runtime;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod performance_tests;
#[cfg(test)]
mod lifecycle_tests;
mod text;
use crate::native_core::common::{now_ms, CoreContext, CoreResult, HandlerResult};
use db::{number, string, Database};
use db::semantic_work::{CountObservation, Scope};
use extract::Cancellation;
use notify::{RecursiveMode, Watcher};
use runtime::{EmbeddingVariantId, ForegroundEmbedding, Runtime};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
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
    #[cfg(test)]
    observer_ticks: [AtomicU64; 2],
    #[cfg(test)]
    progress_events: AtomicU64,
    #[cfg(test)]
    pacing_delays: AtomicU64,
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
    pending: Arc<Mutex<PendingReconcile>>,
    profile_roots: Arc<Mutex<HashSet<PathBuf>>>,
    wake: Arc<Notify>,
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
    wake: &'a Notify,
}
impl Drop for SemanticChange<'_> {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.changing.store(false, Ordering::SeqCst);
        self.wake.notify_waiters();
    }
}
#[derive(Debug, PartialEq, Eq)]
enum IndexOutcome {
    Unchanged,
    Indexed,
}
const FULL_RECONCILE_MS: u64 = 300_000;
const RECONCILE_DEBOUNCE_MS: u64 = 5_000;
const RECONCILE_MAX_WAIT_MS: u64 = 30_000;
const MAX_PENDING_PATHS: usize = 2_000;
const CHURN_BASE_MS: u64 = 60_000;
const CHURN_MAX_MS: u64 = 30 * 60_000;
const MAX_CHURN_PATHS: usize = 4_000;
const COMPLETION_BATCH: usize = 128;

#[derive(Debug)]
struct PathChurn {
    last_indexed: u64,
    last_changed: u64,
    interval: u64,
}
#[derive(Default, Debug)]
struct PendingReconcile {
    paths: HashSet<PathBuf>,
    deferred: HashMap<PathBuf, u64>,
    churn: HashMap<PathBuf, PathChurn>,
    full: bool,
    first: u64,
    last: u64,
}
impl PendingReconcile {
    fn record(
        &mut self,
        event: &notify::Result<notify::Event>,
        roots: &[PathBuf],
        profiles: &HashSet<PathBuf>,
        now: u64,
    ) -> bool {
        if !watcher_requests_reconcile(event, roots) {
            return false;
        }
        let full = match event {
            Ok(event) => event.need_rescan() || event.paths.is_empty(),
            Err(_) => true,
        };
        let paths: Vec<_> = event
            .as_ref()
            .ok()
            .into_iter()
            .flat_map(|e| &e.paths)
            .filter_map(|path| {
                // Resolve lexical parent components before applying exclusions;
                // deleted paths cannot be canonicalized with a filesystem stat.
                let path = if path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    db::normalize_root(path).ok()?
                } else {
                    path.clone()
                };
                (roots.iter().any(|r| path.starts_with(r))
                    && watcher_path_reconcile(&path, roots)
                    && !profiles.iter().any(|r| path.starts_with(r)))
                .then_some(path)
            })
            .collect();
        if !full && paths.is_empty() {
            return false;
        }
        let mut queued = full;
        let mut batch = full;
        for path in paths {
            let already_deferred = self.deferred.contains_key(&path);
            if self.defer(&path, now) {
                queued |= !already_deferred;
            } else if !self.full && !full {
                self.paths.insert(path);
                queued = true;
                batch = true;
            }
        }
        if batch {
            if self.first == 0 {
                self.first = now;
            }
            self.last = now;
        }
        self.full |= full;
        self.bound_pending(now);
        if self.full {
            self.paths.clear();
        }
        queued
    }
    fn batch_deadline(&self, last_full: u64) -> Option<u64> {
        if self.first == 0 || (!self.full && self.paths.is_empty()) {
            return None;
        }
        let due = self
            .last
            .saturating_add(RECONCILE_DEBOUNCE_MS)
            .min(self.first.saturating_add(RECONCILE_MAX_WAIT_MS));
        Some(if self.full {
            due.max(last_full.saturating_add(FULL_RECONCILE_MS))
        } else {
            due
        })
    }
    fn deadline(&self, last_full: u64) -> Option<u64> {
        self.batch_deadline(last_full)
            .into_iter()
            .chain(self.deferred.values().copied())
            .min()
    }
    fn take_due(&mut self, now: u64, last_full: u64) -> Option<Self> {
        if !self.deadline(last_full).is_some_and(|due| now >= due) {
            return None;
        }
        let mut work = Self::default();
        if self.batch_deadline(last_full).is_some_and(|due| now >= due) {
            work.paths = std::mem::take(&mut self.paths);
            work.full = std::mem::take(&mut self.full);
            work.first = std::mem::take(&mut self.first);
            work.last = std::mem::take(&mut self.last);
        }
        self.deferred.retain(|path, due| {
            if now >= *due {
                work.paths.insert(path.clone());
                false
            } else {
                true
            }
        });
        if work.first == 0 {
            work.first = now;
            work.last = now;
        }
        Some(work)
    }
    fn bound_pending(&mut self, now: u64) {
        if self.paths.len() + self.deferred.len() > MAX_PENDING_PATHS {
            self.full = true;
            self.paths.clear();
            self.deferred.clear();
            if self.first == 0 {
                self.first = now;
            }
            self.last = now;
        }
    }
    // A deferred window has a fixed deadline. Further appends coalesce into
    // that window instead of doubling on every event and starving the file.
    fn defer(&mut self, path: &Path, now: u64) -> bool {
        let Some(churn) = self.churn.get_mut(path) else {
            return false;
        };
        if now.saturating_sub(churn.last_changed) > churn.interval {
            churn.interval = CHURN_BASE_MS;
        }
        churn.last_changed = now;
        if self.deferred.get(path).is_some_and(|due| now < *due) {
            return true;
        }
        self.deferred.remove(path);
        if now < churn.last_indexed.saturating_add(churn.interval) {
            churn.interval = churn.interval.saturating_mul(2).min(CHURN_MAX_MS);
            self.deferred.insert(
                path.to_path_buf(),
                churn.last_indexed.saturating_add(churn.interval),
            );
            self.paths.remove(path);
            if self.paths.is_empty() && !self.full {
                self.first = 0;
                self.last = 0;
            }
            self.bound_pending(now);
            return true;
        }
        false
    }
    fn indexed(&mut self, path: &Path, now: u64) {
        let churn = self.churn.entry(path.to_path_buf()).or_insert(PathChurn {
            last_indexed: now,
            last_changed: now,
            interval: CHURN_BASE_MS,
        });
        if now.saturating_sub(churn.last_changed) > churn.interval {
            churn.interval = CHURN_BASE_MS;
        }
        churn.last_indexed = now;
        churn.last_changed = now;
        if self.churn.len() > MAX_CHURN_PATHS {
            if let Some(oldest) = self
                .churn
                .iter()
                .filter(|(path, _)| !self.deferred.contains_key(*path))
                .min_by_key(|(_, churn)| churn.last_changed)
                .map(|(path, _)| path.clone())
            {
                self.churn.remove(&oldest);
            }
        }
    }
}
struct Cache {
    settings: Value,
    status: Value,
    roots: Vec<Value>,
    storage: u64,
    semantic_counts: Option<PublishedCounts>,
}
struct PublishedCounts {
    scope: Scope,
    observation: CountObservation,
}
fn semantic_counts_ready(settings: &Value, runtime: &Value) -> bool {
    settings["embeddingModel"] == "embeddinggemma-2"
        && runtime["modelState"] == "ready"
        && runtime["integrityState"] == "verified"
}
impl KnowledgeCore {
    /// Instructs SQLite to release as much in-memory page and cache memory as possible.
    pub async fn shrink_memory(&self) -> Result<(), String> {
        if let Some(db) = self.inner.database.get() {
            let _ = db
                .call(|store| {
                    let _ = store.db.execute_batch("PRAGMA shrink_memory;");
                    Ok(())
                })
                .await;
        }
        Ok(())
    }

    pub fn new(ctx: CoreContext) -> Self {
        let runtimes = EmbeddingVariantId::ALL.map(|v| Runtime::new_for_variant(ctx.clone(), v));
        let wake = Arc::new(Notify::new());
        for runtime in &runtimes {
            runtime.observe(wake.clone());
        }
        Self {
            inner: Arc::new(Inner {
                ctx,
                #[cfg(test)]
                observer_ticks: [AtomicU64::new(0), AtomicU64::new(0)],
                #[cfg(test)]
                progress_events: AtomicU64::new(0),
                #[cfg(test)]
                pacing_delays: AtomicU64::new(0),
                database: OnceCell::new(),
                cache: Mutex::new(Cache {
                    settings: db::default_settings(),
                    status: db::default_status(),
                    roots: vec![],
                    storage: 0,
                    semantic_counts: None,
                }),
                runtimes,
                semantic_generation: Arc::default(),
                semantic_changing: AtomicBool::new(false),
                generation: Arc::new(AtomicU64::new(0)),
                search_generations: [Arc::default(), Arc::default()],
                closed: AtomicBool::new(false),
                paused: AtomicBool::new(false),
                requested: AtomicBool::new(false),
                pending: Arc::default(),
                profile_roots: Arc::default(),
                wake,
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
            runtime.start_idle_supervisor();
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
            wake: &self.inner.wake,
        }
    }
    fn semantic_cancel(&self) -> Cancellation {
        Cancellation {
            generation: self.inner.semantic_generation.clone(),
            expected: self.inner.semantic_generation.load(Ordering::SeqCst),
        }
    }
    // Accept borrowed inputs so callers already holding cache never relock it.
    fn semantic_scope_for(&self, settings: &Value, roots: &[Value]) -> (EmbeddingVariantId, Scope) {
        let variant =
            serde_json::from_value(settings["embeddingVariant"].clone()).unwrap_or_default();
        (variant, Scope::new(settings, roots, self.runtime(variant).model_key()))
    }
    fn semantic_snapshot(&self, cache: &Cache) -> Value {
        let settings = &cache.settings;
        let (variant, scope) = self.semantic_scope_for(settings, &cache.roots);
        let mut status = self.runtime(variant).snapshot();
        let published = cache.semantic_counts.as_ref().filter(|c| c.scope == scope);
        let counts = published
            .and_then(|c| c.observation.value)
            .unwrap_or((0, 0));
        let fresh = published.is_some_and(|c| c.observation.fresh)
            && semantic_counts_ready(settings, &status)
            && !self.inner.semantic_changing.load(Ordering::SeqCst)
            && !self.inner.paused.load(Ordering::SeqCst)
            && !self.inner.closed.load(Ordering::SeqCst);
        status["indexedChunks"] = json!(counts.0);
        status["totalChunks"] = json!(counts.1);
        status["countsStale"] = json!(!fresh);
        // Existing renderer checks indexVariant for count identity. Do not
        // imply completion for an unknown/stale scope; keep numeric API fields.
        if fresh {
            status["indexVariant"] = json!(variant);
        } else if let Some(fields) = status.as_object_mut() {
            fields.remove("indexVariant");
        }
        status["variants"] = json!(EmbeddingVariantId::ALL.map(|v| {
            let mut snapshot = self.runtime(v).snapshot_variant();
            snapshot["countsStale"] = json!(v != variant || !fresh);
            snapshot
        }));
        status
    }
    fn background(&self) -> Value {
        let cache = self.inner.cache.lock().unwrap();
        json!({"status":cache.status,"settings":{"embeddingModel":cache.settings["embeddingModel"],"embeddingVariant":cache.settings["embeddingVariant"]},"semanticSearch":self.semantic_snapshot(&cache)})
    }
    fn snapshot(&self) -> Value {
        let c = self.inner.cache.lock().unwrap();
        json!({"roots":c.roots,"status":c.status,"localBackendAvailable":true,"cloudBackendAvailable":false,"settings":c.settings,"storageBytes":c.storage,"semanticSearch":self.semantic_snapshot(&c)})
    }
    fn semantic_scope(&self) -> Scope {
        let cache = self.inner.cache.lock().unwrap();
        self.semantic_scope_for(&cache.settings, &cache.roots).1
    }
    async fn refresh_semantic_counts(&self) -> Result<(), String> {
        // Only the background owner calls this. UI/settings refreshes hydrate
        // small snapshots, never enqueue COUNT. One selected scope at a time.
        if self.inner.paused.load(Ordering::SeqCst)
            || self.inner.closed.load(Ordering::SeqCst)
            || self.inner.semantic_changing.load(Ordering::SeqCst)
            || self.inner.ctx.foreground_active()
        {
            return Ok(());
        }
        let cancel = self.semantic_cancel();
        let scope = {
            let cache = self.inner.cache.lock().unwrap();
            let (variant, scope) = self.semantic_scope_for(&cache.settings, &cache.roots);
            let runtime = self.runtime(variant);
            if !semantic_counts_ready(&cache.settings, &runtime.snapshot()) {
                return Ok(());
            }
            scope
        };
        let requested = scope.clone();
        if let Some(c) = self.inner.cache.lock().unwrap().semantic_counts.as_mut() {
            c.observation.fresh = false;
        }
        let result = self
            .database()
            .await?
            .call_search(db::SearchToken::background(cancel.clone()), move |s| {
                let observation = s.semantic_counts_if_due(&requested, std::time::Instant::now())?;
                let storage = observation.reconciled.then(|| s.storage());
                Ok((observation, storage))
            })
            .await;
        cancel.check()?;
        let (observation, storage) = match result {
            // External root/settings writes require rehydrating the cheap cache.
            Err(e) if e == "Knowledge semantic scope changed" => {
                self.refresh().await?;
                return Err(e);
            }
            result => result?,
        };
        if self.inner.paused.load(Ordering::SeqCst)
            || self.inner.closed.load(Ordering::SeqCst)
            || self.inner.semantic_changing.load(Ordering::SeqCst)
        {
            return Ok(());
        }
        let mut cache = self.inner.cache.lock().unwrap();
        cancel.check()?;
        let (variant, current_scope) = self.semantic_scope_for(&cache.settings, &cache.roots);
        if current_scope == scope
            && semantic_counts_ready(&cache.settings, &self.runtime(variant).snapshot())
        {
            if let Some(storage) = storage {
                cache.storage = storage;
            }
            cache.semantic_counts = Some(PublishedCounts { scope, observation });
        }
        Ok(())
    }
    fn status(&self, patch: Value) {
        #[cfg(test)]
        self.inner.progress_events.fetch_add(1, Ordering::SeqCst);
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
            if self.semantic_scope_for(&c.settings, &c.roots).1
                != self.semantic_scope_for(&settings, &roots).1
                && !self.inner.semantic_changing.load(Ordering::SeqCst)
            {
                // External scope changes also wake/reset learned-work retries.
                self.inner
                    .semantic_generation
                    .fetch_add(1, Ordering::SeqCst);
            }
            c.roots = roots;
            c.settings = settings;
            c.storage = storage;
        }
        self.refresh_counts().await?;
        // Counts are coalesced by the background owner, not settings polling.
        self.inner.wake.notify_waiters();
        Ok(())
    }
    fn request_index(&self) {
        self.inner.requested.store(true, Ordering::SeqCst);
        self.inner.wake.notify_waiters();
    }
    async fn sync_watchers(&self) -> Result<(), String> {
        let roots = self.roots();
        let pending = self.inner.pending.clone();
        let profiles = self.inner.profile_roots.clone();
        let wake = self.inner.wake.clone();
        let watched_paths: Vec<_> = roots.iter().map(|r| PathBuf::from(string(r, "path"))).collect();
        let watcher = tokio::task::spawn_blocking(move || {
            // FSEvents may return /private/var/... for a configured /var/...
            // root. Keep stored source paths in the configured namespace, even
            // for removed paths which can no longer be canonicalized.
            let mut aliases: Vec<_> = watched_paths
                .iter()
                .filter_map(|root| {
                    root.canonicalize()
                        .ok()
                        .filter(|p| p != root)
                        .map(|p| (p, root.clone()))
                })
                .collect();
            aliases.sort_by_key(|(path, _)| std::cmp::Reverse(path.components().count()));
            let mut watcher = notify::recommended_watcher(
                move |mut event: notify::Result<notify::Event>| {
                    if let Ok(event) = &mut event {
                        for path in &mut event.paths {
                            if let Some((_, root, relative)) =
                                aliases.iter().find_map(|(alias, root)| {
                                    path.strip_prefix(alias)
                                        .ok()
                                        .map(|relative| (alias, root, relative.to_path_buf()))
                                })
                            {
                                *path = root.join(relative);
                            }
                        }
                    }
                    if let Err(error) = &event {
                        log::warn!("Knowledge watcher: {error}");
                    }
                    if pending.lock().unwrap().record(
                        &event,
                        &watched_paths,
                        &profiles.lock().unwrap(),
                        now_ms(),
                    ) {
                        wake.notify_waiters();
                    }
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
        loop {
            // Register before observing state: completion during an await must
            // remain visible, including while the extraction owner is busy.
            let wake = self.inner.wake.notified();
            tokio::pin!(wake);
            wake.as_mut().enable();
            if self.inner.closed.load(Ordering::SeqCst) { break; }
            #[cfg(test)]
            self.inner.observer_ticks[0].fetch_add(1, Ordering::Relaxed);
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
            wake.await;
        }
    }
    async fn background_loop(&self) {
        let startup = now_ms() + 8000;
        let mut started = false;
        let mut backfill = true;
        let mut last_vector = 0;
        let mut last_rescan = now_ms();
        let mut last_semantic = Value::Null;
        let mut retry_learned_after = 0;
        let mut retry_counts_after = 0;
        let mut last_variant = self.selected_variant();
        let mut last_semantic_epoch = self.inner.semantic_generation.load(Ordering::SeqCst);
        while !self.inner.closed.load(Ordering::SeqCst) {
            let wake = self.inner.wake.notified();
            tokio::pin!(wake);
            wake.as_mut().enable();
            #[cfg(test)]
            self.inner.observer_ticks[1].fetch_add(1, Ordering::Relaxed);
            let variant = self.selected_variant();
            let epoch = self.inner.semantic_generation.load(Ordering::SeqCst);
            if epoch != last_semantic_epoch {
                retry_learned_after = 0;
                retry_counts_after = 0;
                last_semantic_epoch = epoch;
            }
            if variant != last_variant {
                last_variant = variant;
                retry_learned_after = 0;
                retry_counts_after = 0;
                backfill = true;
            }
            let runtime = self.runtime(variant);
            if !started && now_ms() >= startup {
                started = true;
                if !self.inner.paused.load(Ordering::SeqCst) {
                    self.request_index();
                }
            }
            // Explicit requests (startup/settings/user) bypass the full-pass
            // interval. Watcher recovery and periodic work share the same limit.
            let paused = self.inner.paused.load(Ordering::SeqCst);
            let explicit = !paused && self.inner.requested.swap(false, Ordering::SeqCst);
            let periodic = now_ms().saturating_sub(last_rescan) >= FULL_RECONCILE_MS;
            let pending = if !paused {
                self.inner
                    .pending
                    .lock()
                    .unwrap()
                    .take_due(now_ms(), last_rescan)
            } else {
                None
            };
            if explicit || (!paused && periodic) || pending.is_some() {
                let _run = self.inner.run.lock().await;
                let cancel = self.cancel();
                if !self.inner.paused.load(Ordering::SeqCst)
                    && !self.inner.closed.load(Ordering::SeqCst)
                {
                    let full = explicit || periodic || pending.as_ref().is_some_and(|p| p.full);
                    let result = if full {
                        self.run_index(&cancel).await
                    } else {
                        self.run_incremental(&pending.as_ref().unwrap().paths, &cancel)
                            .await
                    };
                    if let Err(error) = result {
                        if cancel.check().is_ok() {
                            self.status(json!({"state":"failed","error":error}));
                            let _ = self.persist().await;
                        }
                        // Incremental work has no durable queue. Retain its paths
                        // on cancellation/error; a full checkpoint resumes normally.
                        if !full {
                            let mut queued = self.inner.pending.lock().unwrap();
                            let retry = pending.unwrap();
                            queued.paths.extend(retry.paths);
                            if queued.first == 0 {
                                queued.first = now_ms();
                            }
                            queued.last = now_ms();
                            queued.bound_pending(now_ms());
                        }
                    }
                    if full {
                        last_rescan = now_ms();
                    }
                } else if !self.inner.closed.load(Ordering::SeqCst) {
                    if explicit {
                        self.request_index();
                    }
                    if let Some(retry) = pending {
                        let mut queued = self.inner.pending.lock().unwrap();
                        queued.paths.extend(retry.paths);
                        queued.full |= retry.full;
                        queued.bound_pending(now_ms());
                        if queued.full {
                            queued.paths.clear();
                        }
                        if queued.first == 0 {
                            queued.first = retry.first;
                        }
                        queued.last = queued.last.max(retry.last);
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
            // This is a cheap generation/scope probe on unchanged ticks. The
            // worker coalesces dirty generations for 10s since COUNT completion.
            if now_ms() >= retry_counts_after && self.refresh_semantic_counts().await.is_err() {
                // Keep the scope dirty on failure without repeatedly running a
                // failing large query. Selection/root/pause epochs reset this.
                retry_counts_after = now_ms() + 30000;
            }
            let semantic = self.semantic_snapshot(&self.inner.cache.lock().unwrap());
            if semantic != last_semantic {
                last_semantic = semantic;
                self.status(json!({}));
            }
            // Sleep to actual service deadlines when quiescent. Selected
            // semantic indexes retain their original 250ms responsiveness for
            // pending work, external writers and counts. Filesystem/runtime/
            // settings changes and shutdown wake both observers immediately.
            let now = now_ms();
            let mut next = if self.inner.paused.load(Ordering::SeqCst) { now.saturating_add(FULL_RECONCILE_MS) } else { last_rescan.saturating_add(FULL_RECONCILE_MS) };
            if !started { next = next.min(startup); }
            if !self.inner.paused.load(Ordering::SeqCst) {
                if let Some(due) = self.inner.pending.lock().unwrap().deadline(last_rescan) {
                    next = next.min(due);
                }
            }
            if !self.inner.paused.load(Ordering::SeqCst) {
                if backfill {
                    let vector_due = if self.inner.ctx.foreground_active() {
                        now.saturating_add(250)
                    } else {
                        startup.max(last_vector.saturating_add(251))
                    };
                    next = next.min(vector_due);
                }
            }
            if (!self.inner.paused.load(Ordering::SeqCst)
                && self.settings()["embeddingModel"] == "embeddinggemma-2")
                || self.inner.runtimes.iter().any(Runtime::downloading)
            {
                // Download progress is independent of indexing pause/selection.
                // Its byte counter changes under the status mutex; retain the
                // old observation cadence only while that work is active.
                next = next.min(now.saturating_add(250));
            }
            tokio::select! {_=&mut wake=>{},_=tokio::time::sleep(std::time::Duration::from_millis(next.saturating_sub(now).max(1)))=>{}}
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
        let scope = self.semantic_scope();
        let chunks = self
            .database()
            .await?
            .call_search(db::SearchToken::background(cancel.clone()), move |s| {
                s.semantic_pending(&scope, count, std::time::Instant::now())
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
            if let Some(c) = self.inner.cache.lock().unwrap().semantic_counts.as_mut() {
                c.observation.fresh = false;
            }
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
    fn queue_changed_path(&self, path: PathBuf) {
        let roots = self
            .roots()
            .iter()
            .map(|r| PathBuf::from(string(r, "path")))
            .collect::<Vec<_>>();
        let now = now_ms();
        let mut pending = self.inner.pending.lock().unwrap();
        // Also cover unstable files encountered by a full pass. An attempt
        // which could not commit still paid hashing/extraction costs.
        pending.indexed(&path, now);
        let event = Ok(notify::Event::new(notify::EventKind::Any).add_path(path));
        if pending.record(
            &event,
            &roots,
            &self.inner.profile_roots.lock().unwrap(),
            now,
        ) {
            self.inner.wake.notify_waiters();
        }
    }
    async fn run_incremental(
        &self,
        changed: &HashSet<PathBuf>,
        cancel: &Cancellation,
    ) -> Result<(), String> {
        self.run_incremental_with_clock(changed, cancel, now_ms)
            .await
    }
    async fn run_incremental_with_clock(
        &self,
        changed: &HashSet<PathBuf>,
        cancel: &Cancellation,
        clock: impl Fn() -> u64,
    ) -> Result<(), String> {
        let db = self.database().await?;
        let roots = self.roots();
        let watched: Vec<_> = roots
            .iter()
            .map(|r| PathBuf::from(string(r, "path")))
            .collect();
        let mut changed: Vec<_> = changed.iter().collect();
        changed.sort_by_key(|p| p.components().count());
        let mut subtrees: Vec<&PathBuf> = vec![];
        for path in changed {
            if !subtrees.iter().any(|ancestor| path.starts_with(ancestor)) {
                subtrees.push(path);
            }
        }
        self.status(
            json!({"state":"indexing","error":null,"detail":"Updating changed knowledge files"}),
        );
        let ocr = Arc::new(tokio::sync::Semaphore::new(
            self.settings()["maxConcurrentOcrJobs"]
                .as_u64()
                .unwrap_or(1)
                .clamp(1, 4) as usize,
        ));
        for path in subtrees {
            self.idle(cancel).await?;
            if !watcher_path_reconcile(path, &watched) {
                continue;
            }
            // Detect profiles created since the last full walk as well. This
            // runs on the worker, never in the synchronous notify callback.
            let scan_path = path.clone();
            let condition = cancel.clone();
            let ctx = self.inner.ctx.clone();
            let (paths, complete, profiles) = tokio::task::spawn_blocking(move || {
                let profiles: HashSet<_> = scan_path
                    .ancestors()
                    .filter(|p| extract::browser_profile(p))
                    .map(Path::to_path_buf)
                    .collect();
                if !profiles.is_empty() {
                    return Ok((vec![], true, profiles));
                }
                match std::fs::symlink_metadata(&scan_path) {
                    Ok(_) => scan(&scan_path, &condition, &ctx),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        Ok((vec![], true, profiles))
                    }
                    Err(e) => Err(e.to_string()),
                }
            })
            .await
            .map_err(|e| e.to_string())??;
            self.inner.profile_roots.lock().unwrap().extend(profiles);
            for root in roots.iter().filter(|r| path.starts_with(string(r, "path"))) {
                if complete {
                    let present = paths.iter().cloned().collect();
                    let id = string(root, "id").to_owned();
                    let subtree = path.clone();
                    let condition = cancel.clone();
                    db.call(move |s| {
                        condition.check()?;
                        s.prune_path(&id, &subtree, &present)
                    })
                    .await?;
                }
                for file in &paths {
                    {
                        let mut pending = self.inner.pending.lock().unwrap();
                        let now = clock();
                        // Gate the discovered files too: a directory event must
                        // not bypass backoff for its continuously changing child.
                        if pending.defer(Path::new(file), now) {
                            continue;
                        }
                        pending.indexed(Path::new(file), now);
                    }
                    let result = self.index_one(root, file, cancel, ocr.clone()).await;
                    cancel.check()?;
                    if let Err(error) = result {
                        let id = text::hash(file);
                        let condition = cancel.clone();
                        db.call(move |s| {
                            condition.check()?;
                            s.failed(&id, &error)
                        })
                        .await?;
                    }
                }
            }
        }
        cancel.check()?;
        self.refresh().await?;
        self.status(json!({"state":"completed","detail":"Knowledge index is up to date","lastCompletedAt":now_ms()}));
        self.persist().await
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
            let mut profiles = HashSet::new();
            for root in roots {
                self.idle(cancel).await?;
                let root_path = PathBuf::from(string(&root, "path"));
                let path = root_path.clone();
                let cancellation = cancel.clone();
                let ctx = self.inner.ctx.clone();
                let (paths, complete, discovered) =
                    tokio::task::spawn_blocking(move || scan(&path, &cancellation, &ctx))
                        .await
                        .map_err(|e| e.to_string())??;
                cancel.check()?;
                profiles.extend(discovered);
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
            *self.inner.profile_roots.lock().unwrap() = profiles;
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
        let mut completed = Vec::with_capacity(COMPLETION_BATCH);
        let mut unpublished = 0usize;
        loop {
            self.idle(cancel).await?;
            let jobs = db.call(move |s| s.next_batch(COMPLETION_BATCH)).await?;
            if jobs.is_empty() {
                break;
            }
            use futures_util::{stream, StreamExt};
            let mut futures = stream::iter(jobs).map(|(job, position, root_id, path)| {
                let root = self
                    .roots()
                    .into_iter()
                    .find(|r| string(r, "id") == root_id);
                let ocr = ocr.clone();
                async move {
                    let result = if let Some(root) = root {
                        self.index_one(&root, &path, cancel, ocr).await
                    } else {
                        Ok(IndexOutcome::Unchanged)
                    };
                    (job, position, path, result)
                }
            }).buffer_unordered(concurrency);
            while let Some((job, position, path, result)) = futures.next().await {
                cancel.check()?;
                let paced = result != Ok(IndexOutcome::Unchanged);
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
                completed.push((job, position, failed));
                let condition = cancel.clone();
                unpublished += 1;
                let publish = paced || unpublished >= COMPLETION_BATCH;
                // Flush before progress publication, and once at the end of each
                // fetched batch. Dropped tails stay in the checkpoint on cancel.
                let durable = if publish {
                    std::mem::take(&mut completed)
                } else {
                    vec![]
                };
                let checkpoint = if publish {
                    db.call(move |s| {
                        condition.check()?;
                        s.complete_batch(&durable)?;
                        s.checkpoint()
                    })
                    .await?
                } else {
                    None
                };
                if let Some(checkpoint) = checkpoint {
                    unpublished = 0;
                    self.refresh_counts().await?;
                    let processed = number(&checkpoint, "processedSources");
                    let total = number(&checkpoint, "totalSources");
                    self.status(json!({"state":"indexing","jobId":checkpoint["jobId"],"processedSources":processed,"failedSources":checkpoint["failedSources"],"queuedSources":(total-processed).max(0),"progress":if total>0{processed as f64/total as f64}else{1.},"detail":format!("Processed {}",db::title(&path))}));
                    self.persist().await?;
                }
                if paced {
                    let settings = self.settings();
                    let delay = match string(&settings, "indexingSpeed") {
                        "slow" => 120,
                        "fast" => 0,
                        _ => 40,
                    };
                    #[cfg(test)]
                    self.inner.pacing_delays.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }
            }
            let durable = std::mem::take(&mut completed);
            if !durable.is_empty() {
                let condition = cancel.clone();
                db.call(move |s| {
                    condition.check()?;
                    s.complete_batch(&durable)
                })
                .await?;
            }
        }
        cancel.check()?;
        // Publish the durable tail before deleting the checkpoint. On cancellation
        // the checkpoint remains authoritative and resume reads its exact counts.
        if let Some(checkpoint) = db.call(|s| s.checkpoint()).await? {
            self.status(json!({"processedSources":checkpoint["processedSources"],"failedSources":checkpoint["failedSources"]}));
        }
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
    ) -> Result<IndexOutcome, String> {
        let result = self.index_existing(root, path, cancel, ocr).await;
        if result.is_err()
            && tokio::fs::metadata(path)
                .await
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        {
            let condition = cancel.clone();
            let id = text::hash(path);
            self.database()
                .await?
                .call(move |s| {
                    condition.check()?;
                    s.remove_source(&id)
                })
                .await?;
            return Ok(IndexOutcome::Unchanged);
        }
        result
    }
    async fn index_existing(
        &self,
        root: &Value,
        path: &str,
        cancel: &Cancellation,
        ocr: Arc<tokio::sync::Semaphore>,
    ) -> Result<IndexOutcome, String> {
        let settings = self.settings();
        let selected_depth = depth(root, &settings);
        if selected_depth == "off" {
            return Ok(IndexOutcome::Unchanged);
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
            if (existing["status"] == "indexed"
                || (existing["status"] == "failed"
                    && existing["error"] != "File changed during indexing; queued for rescan"))
                && existing["byteSize"].as_u64() == Some(metadata.len())
                && existing["modifiedAt"].as_f64().unwrap_or(0.).round() == modified.round()
                && existing["indexingProfile"] == profile
            {
                return Ok(IndexOutcome::Unchanged);
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
            self.queue_changed_path(PathBuf::from(path));
            return Err("File changed during indexing; queued for rescan".into());
        }
        let condition = cancel.clone();
        self.database()
            .await?
            .call(move |s| {
                condition.check()?;
                s.save_result(&id, &result)
            })
            .await?;
        Ok(IndexOutcome::Indexed)
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
            self.inner.wake.notify_waiters();
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
        self.inner.wake.notify_waiters();
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
        // Fence counts/pending reads throughout asynchronous scope mutations,
        // not just at admission. Drop fences errors and dropped IPC futures too.
        let _change = self.semantic_change();
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
                // Coalesced UI updates may trail durable completion. A pause
                // publishes the authoritative tail after the indexing owner exits.
                let checkpoint = db.call(|s| s.checkpoint()).await?;
                if let Some(checkpoint) = checkpoint {
                    let processed = number(&checkpoint, "processedSources");
                    let total = number(&checkpoint, "totalSources");
                    self.status(json!({"processedSources":processed,"failedSources":checkpoint["failedSources"],"queuedSources":(total-processed).max(0),"progress":if total>0{processed as f64/total as f64}else{1.}}));
                }
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
        drop(_change);
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
) -> Result<(Vec<String>, bool, HashSet<PathBuf>), String> {
    let mut paths = vec![];
    let mut complete = true;
    let mut visited = 0;
    let mut profiles = HashSet::new();
    let iter = walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir() && extract::browser_profile(e.path()) {
                profiles.insert(e.path().to_path_buf());
                return false;
            }
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
    Ok((paths, complete, profiles))
}

// Filter paths hidden by scanner exclusions without stat calls. Removed and
// renamed eligible directories must still reconcile their children.
fn watcher_reconcile(event: &notify::Event, roots: &[PathBuf]) -> bool {
    if event.need_rescan() {
        return true;
    }
    if matches!(event.kind, notify::EventKind::Access(_)) {
        return false;
    }
    if event.paths.is_empty() {
        return true;
    }
    event
        .paths
        .iter()
        .any(|path| watcher_path_reconcile(path, roots))
}

fn watcher_path_reconcile(path: &Path, roots: &[PathBuf]) -> bool {
    let containing: Vec<_> = roots
        .iter()
        .filter_map(|root| path.strip_prefix(root).ok())
        .collect();
    if containing.is_empty() {
        return true;
    }
    // A nested explicit root overrides the parent's name exclusions (depth 0).
    containing.iter().any(|relative| {
        let components: Vec<_> = relative.components().collect();
        if components
            .iter()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return true;
        }
        !components.iter().enumerate().any(|(i, component)| match component {
            std::path::Component::Normal(name) => {
                // No stat here: removed directories still reconcile children.
                // File exclusions apply to leaves, directory rules to ancestors.
                extract::skip(&name.to_string_lossy(), i + 1 < components.len())
                    || extract::skip(&name.to_string_lossy(), true)
            }
            _ => false,
        })
    })
}

fn watcher_requests_reconcile(event: &notify::Result<notify::Event>, roots: &[PathBuf]) -> bool {
    match event {
        Ok(event) => watcher_reconcile(event, roots),
        Err(_) => true,
    }
}
