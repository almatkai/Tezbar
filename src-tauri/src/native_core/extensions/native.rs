//! Bounded process sessions; canonical failures never fall through to Bun.
use super::{process::Process, process_install, registry, v2_runtime};
use crate::native_core::common::{CoreContext, CoreResult, HandlerResult};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::{watch, Mutex as AsyncMutex};
const PREFIX: &str = "native-extension:";
const MAX_SESSIONS: usize = 128;
struct Slot {
    process: AsyncMutex<Option<Arc<Process>>>,
    generation: watch::Sender<u64>,
    launcher_hint: Mutex<Vec<Value>>,
}
impl Default for Slot {
    fn default() -> Self {
        Self {
            process: AsyncMutex::new(None),
            generation: watch::channel(0).0,
            launcher_hint: Mutex::new(Vec::new()),
        }
    }
}
struct Entry {
    process: Arc<Process>,
    canonical: String,
    command: String,
    policy: Value,
    active: AtomicBool,
    cancel: watch::Sender<bool>,
    search: Mutex<watch::Sender<()>>,
    serial: AsyncMutex<()>,
}
struct LauncherResult {
    rows: Value,
    at: std::time::Instant,
    // None also caches failures before a process can be started. A restarted
    // process cannot reuse another process's revision, even if both are zero.
    scope: Option<(String, u64)>,
    failed: bool,
}
struct LauncherQuery {
    key: String,
    id: String,
    cancel: watch::Sender<()>,
    results: HashMap<String, LauncherResult>,
    pending: std::collections::HashSet<String>,
}
struct State {
    sessions: HashMap<String, Arc<Entry>>,
    slots: HashMap<String, Arc<Slot>>,
    startup_epoch: watch::Sender<u64>,
    launcher_queries: HashMap<String, LauncherQuery>,
    launcher_packages: Arc<AsyncMutex<Option<Vec<process_install::Package>>>>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            sessions: HashMap::new(),
            slots: HashMap::new(),
            startup_epoch: watch::channel(0).0,
            launcher_queries: HashMap::new(),
            launcher_packages: Arc::default(),
        }
    }
}
#[derive(Clone, Default)]
pub(super) struct NativeSessions(Arc<Mutex<State>>);
impl NativeSessions {
    fn slot(&self, id: &str) -> Arc<Slot> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .slots
            .entry(id.into())
            .or_default()
            .clone()
    }
    pub async fn clear(&self) {
        self.clear_matching(None).await;
    }
    pub async fn clear_slug(&self, slug: &str) {
        self.clear_matching(Some(&v2_runtime::canonical_id(slug)))
            .await;
    }
    async fn clear_matching(&self, id: Option<&str>) {
        let (slots, entries) = {
            let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
            if id.is_none() {
                state.startup_epoch.send_modify(|g| *g = g.wrapping_add(1));
            }
            if let Some(id) = id {
                let prefix = format!("{id}:");
                for query in state.launcher_queries.values_mut() {
                    query.results.retain(|key, _| !key.starts_with(&prefix));
                    query.pending.retain(|key| !key.starts_with(&prefix));
                }
            } else {
                for query in state.launcher_queries.values() {
                    query.cancel.send_replace(());
                }
                state.launcher_queries.clear();
            }
            let slots = state
                .slots
                .iter()
                .filter(|(key, _)| id.is_none_or(|id| id == key.as_str()))
                .map(|(_, s)| s.clone())
                .collect::<Vec<_>>();
            for slot in &slots {
                slot.launcher_hint
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clear();
                slot.generation.send_modify(|g| *g = g.wrapping_add(1));
            }
            let ids = state
                .sessions
                .iter()
                .filter(|(_, e)| id.is_none_or(|id| id == e.canonical))
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();
            let entries = ids
                .into_iter()
                .filter_map(|key| state.sessions.remove(&key))
                .collect::<Vec<_>>();
            (slots, entries)
        };
        let packages = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .launcher_packages
            .clone();
        *packages.lock().await = None;
        for e in entries {
            e.active.store(false, Ordering::Release);
            e.cancel.send_replace(true);
        }
        for slot in slots {
            if let Some(p) = slot.process.lock().await.take() {
                p.sessions.lock().unwrap_or_else(|e| e.into_inner()).clear();
                p.stop_and_wait().await;
            }
        }
    }
    pub fn start(&self, ctx: &CoreContext) -> Result<(), String> {
        self.bootstrap_launcher(ctx, None)?;
        self.restore(ctx, None)
    }
    pub fn restore_id(&self, ctx: &CoreContext, id: &str) -> Result<(), String> {
        self.bootstrap_launcher(ctx, Some(id))?;
        self.restore(ctx, Some(id))
    }
    fn restore(&self, ctx: &CoreContext, target: Option<&str>) -> Result<(), String> {
        // Reading an empty registry starts no process. No slug/store-file probes.
        let mut startup_epoch = self
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .startup_epoch
            .subscribe();
        let mut records = super::process_host::background_records(ctx)?;
        let entries = records
            .as_object_mut()
            .ok_or("Invalid background database")?;
        entries.retain(|id, _| target.is_none_or(|target| target == id));
        let mut probes = super::process_host::startup_probes(ctx)?;
        probes.retain(|(id, _, _)| target.is_none_or(|target| target == id));
        let this = self.clone();
        let ctx = ctx.clone();
        if !probes.is_empty() || records.as_object().is_some_and(|r| !r.is_empty()) {
            tokio::spawn(async move {
                tokio::select! {biased;_=startup_epoch.changed()=>{},_=async{
                let mut restored = std::collections::HashSet::new();
                for (id, keys) in records.as_object().into_iter().flatten() {
                    if !id.starts_with("com.tezbar.native.") {
                        continue;
                    }
                    let slot = this.slot(id);
                    let generation = *slot.generation.borrow();
                    match this
                        .process(&ctx, id, &slot, generation, "background-restore")
                        .await
                    {
                        Ok(p) => {
                            for (key, data) in keys.as_object().into_iter().flatten() {
                                if let Err(error) = p
                                    .call(
                                        "background.activate",
                                        json!({"key":key,"data":data}),
                                        Duration::from_secs(30),
                                    )
                                    .await
                                {
                                    log::warn!("Background activation for {id} failed: {error}");
                                    p.stop();
                                    break;
                                }
                                restored.insert((id.clone(), key.clone()));
                            }
                        }
                        Err(error) => log::warn!("Background restoration for {id} failed: {error}"),
                    }
                }
                for (id, probe, fingerprint) in probes {
                    let key = probe["key"].as_str().unwrap_or("");
                    // A restored durable lease already owns migration restoration.
                    if restored.contains(&(id.clone(), key.to_string())) {
                        let _ = super::process_host::acknowledge_probe(&ctx, &id, &fingerprint);
                        continue;
                    }
                    let slot = this.slot(&id);
                    let generation = *slot.generation.borrow();
                    match this
                        .process(&ctx, &id, &slot, generation, "startup-migration-probe")
                        .await
                    {
                        Ok(p) => match p
                            .call(
                                "background.activate",
                                json!({"key":key,"data":probe["data"]}),
                                Duration::from_secs(30),
                            )
                            .await
                        {
                            Ok(_) => {
                                if let Err(e) =
                                    super::process_host::acknowledge_probe(&ctx, &id, &fingerprint)
                                {
                                    log::warn!("Startup probe persistence failed: {e}");
                                }
                            }
                            Err(e) => {
                                log::warn!("Startup probe for {id} failed: {e}");
                                p.stop();
                            }
                        },
                        Err(e) => log::warn!("Startup probe for {id} failed: {e}"),
                    }
                }
                }=>{}}
            });
        }
        Ok(())
    }
    async fn process(
        &self,
        ctx: &CoreContext,
        id: &str,
        slot: &Slot,
        generation: u64,
        reason: &str,
    ) -> Result<Arc<Process>, String> {
        let mut cancel = slot.generation.subscribe();
        tokio::select! {biased;_=cancel.changed()=>Err("Extension generation changed".into()),result=async{
            let mut current=slot.process.lock().await;
            if *slot.generation.borrow()!=generation{return Err("Extension generation changed".into());}
            if let Some(p)=current.as_ref().filter(|p|p.alive()){return Ok(p.clone());}
            let resolve_ctx=ctx.clone();let id=id.to_string();
            let package=tokio::task::spawn_blocking(move||process_install::resolve(&resolve_ctx,&id)).await.map_err(|e|e.to_string())??;
            let p=Process::spawn(ctx,package,reason).await?;
            if *slot.generation.borrow()!=generation{p.stop();return Err("Extension generation changed".into());}
            *current=Some(p.clone());Ok(p)
        }=>result}
    }
    async fn launcher_packages(&self, ctx: &CoreContext) -> Vec<process_install::Package> {
        let cache = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .launcher_packages
            .clone();
        let mut cache = cache.lock().await;
        if cache.is_none() {
            let ctx = ctx.clone();
            *cache = Some(
                tokio::task::spawn_blocking(move || super::launcher::packages(&ctx))
                    .await
                    .unwrap_or_default(),
            );
        }
        cache.clone().unwrap_or_default()
    }
    /// Immediately return the lightweight index/current snapshot. Child launch,
    /// enumeration and refresh always happen in bounded cancellable work.
    async fn launcher_search(&self, ctx: &CoreContext, body: &Value) -> CoreResult {
        let text = body["text"]
            .as_str()
            .filter(|s| s.len() <= 1024)
            .ok_or("Invalid launcher query")?;
        let surface = body["surface"].as_str().unwrap_or("query");
        if !["query", "argument"].contains(&surface) {
            return Err("Invalid launcher surface".into());
        }
        let key = serde_json::to_string(body).map_err(|e| e.to_string())?;
        let (query_id, cancelled) = {
            let mut state = self.0.lock().map_err(|e| e.to_string())?;
            let changed = state
                .launcher_queries
                .get(surface)
                .is_none_or(|q| q.key != key);
            if changed {
                for slot in state.slots.values() {
                    if let Ok(process) = slot.process.try_lock() {
                        if let Some(p) = process.as_ref() {
                            p.launcher_queries
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .clear();
                        }
                    }
                }
            }
            let q = state
                .launcher_queries
                .entry(surface.into())
                .or_insert_with(|| LauncherQuery {
                    key: String::new(),
                    id: String::new(),
                    cancel: watch::channel(()).0,
                    results: HashMap::new(),
                    pending: Default::default(),
                });
            if q.key != key {
                q.cancel.send_replace(());
                q.key = key;
                q.id = uuid::Uuid::new_v4().to_string();
                q.results.clear();
                q.pending.clear();
            }
            (q.id.clone(), q.cancel.subscribe())
        };
        if text.trim().is_empty() || body["deep"] == true {
            return Ok(json!([]));
        }
        let packages = self.launcher_packages(ctx).await;
        let mut out = Vec::new();
        let mut selected_count = 0;
        for package in packages {
            if !super::launcher::current(ctx, &package) {
                continue;
            }
            let m = &package.manifest;
            let id = m["id"].as_str().unwrap_or("");
            for d in super::launcher::descriptors(m) {
                let command = d["commandName"].as_str().unwrap_or("");
                if !super::launcher::enabled(ctx, m, command)
                    || !d["surfaces"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|s| s == surface))
                {
                    continue;
                }
                if surface == "argument"
                    && (body["extensionId"] != id
                        || body["commandName"] != command
                        || body["argumentName"] != d["argumentName"])
                {
                    continue;
                }
                let index = super::launcher::index_rows(ctx, m, d, text).unwrap_or_default();
                let existing = self
                    .0
                    .lock()
                    .map_err(|e| e.to_string())?
                    .slots
                    .get(id)
                    .cloned();
                let warm_match = existing.as_ref().is_some_and(|slot| {
                    let active = slot
                        .process
                        .try_lock()
                        .ok()
                        .and_then(|p| p.clone())
                        .is_some_and(|p| p.alive());
                    active
                        && slot
                            .launcher_hint
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .iter()
                            .any(|r| {
                                format!(
                                    "{} {}",
                                    r["title"].as_str().unwrap_or(""),
                                    r["subtitle"].as_str().unwrap_or("")
                                )
                                .to_lowercase()
                                .contains(&text.trim().to_lowercase())
                            })
                });
                let selected = surface == "argument"
                    || super::launcher::selected(d, text)
                    || !index.is_empty()
                    || warm_match;
                out.extend(index);
                if !selected || selected_count >= 8 {
                    continue;
                }
                selected_count += 1;
                let contribution = d["id"].as_str().unwrap_or("").to_string();
                let result_key = format!("{id}:{contribution}");
                let slot = self.slot(id);
                let process = slot.process.try_lock().ok().and_then(|p| p.clone());
                let scope = process.as_ref().filter(|p| p.alive()).map(|p| {
                    (
                        p.generation.clone(),
                        p.launcher_revision.load(Ordering::Acquire),
                    )
                });
                let should_start = {
                    let mut state = self.0.lock().map_err(|e| e.to_string())?;
                    let Some(q) = state
                        .launcher_queries
                        .get_mut(surface)
                        .filter(|q| q.id == query_id)
                    else {
                        return Ok(json!([]));
                    };
                    if let Some(entry) = q.results.get(&result_key) {
                        if entry.scope == scope {
                            out.extend(entry.rows.as_array().into_iter().flatten().cloned());
                        }
                    }
                    // Failures retry only on later demand after a one-second
                    // backoff, or immediately for a new query/process/revision.
                    // No retry timer or failure invalidation drives another call.
                    let fresh = q.results.get(&result_key).is_some_and(|entry| {
                        let ttl = if entry.failed { 1000 } else { 500 };
                        entry.at.elapsed() < Duration::from_millis(ttl) && entry.scope == scope
                    });
                    !fresh && q.pending.insert(result_key.clone())
                };
                if !should_start {
                    continue;
                }
                let this = self.clone();
                let ctx = ctx.clone();
                let id = id.to_owned();
                let d = d.clone();
                let m = m.clone();
                let text = text.to_owned();
                let surface = surface.to_owned();
                let body = body.clone();
                let query_id = query_id.clone();
                let mut cancelled = cancelled.clone();
                let generation = *slot.generation.borrow();
                let mut generation_changed = slot.generation.subscribe();
                tokio::spawn(async move {
                    let mut observed_scope = scope;
                    let work = async {
                        let p = this
                            .process(&ctx, &id, &slot, generation, "launcher-query")
                            .await?;
                        observed_scope = Some((
                            p.generation.clone(),
                            p.launcher_revision.load(Ordering::Acquire),
                        ));
                        if !p.launcher_enabled() {
                            return Err(
                                "Installed executable does not negotiate launcher.v1".into()
                            );
                        }
                        p.launcher_queries
                            .lock()
                            .map_err(|e| e.to_string())?
                            .insert(contribution.clone(), (query_id.clone(), text.clone(), 0));
                        let value=p.call("launcher.search",json!({"contributionId":contribution,"queryId":query_id,"text":text,"surface":surface,"commandName":body["commandName"],"argumentName":body["argumentName"],"limit":d["limit"]}),Duration::from_millis(d["timeoutMs"].as_u64().unwrap_or(1000))).await?;
                        let rows = super::launcher::rows(&m, &d, &value["rows"], false)?;
                        if rows.len() > d["limit"].as_u64().unwrap_or(12) as usize {
                            return Err("Launcher result exceeds declared limit".into());
                        }
                        *slot.launcher_hint.lock().map_err(|e| e.to_string())? = rows.clone();
                        Ok::<_, String>(json!(rows))
                    };
                    let result = tokio::select! {biased;_=cancelled.changed()=>return,_=generation_changed.changed()=>return,result=work=>result};
                    if *slot.generation.borrow() != generation {
                        return;
                    }
                    let mut state = this.0.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(q) = state
                        .launcher_queries
                        .get_mut(&surface)
                        .filter(|q| q.id == query_id)
                    {
                        q.pending.remove(&result_key);
                        let failed = result.is_err();
                        let rows = match result {
                            Ok(rows) => rows,
                            Err(error) => {
                                log::debug!("Launcher contribution failed: {error}");
                                q.results
                                    .remove(&result_key)
                                    .filter(|entry| entry.scope == observed_scope)
                                    .map(|entry| entry.rows)
                                    .unwrap_or_else(|| json!([]))
                            }
                        };
                        q.results.insert(
                            result_key,
                            LauncherResult {
                                rows,
                                at: std::time::Instant::now(),
                                scope: observed_scope,
                                failed,
                            },
                        );
                        if !failed {
                            ctx.emit(
                                "launcher:invalidated",
                                json!({"text":text,"queryId":query_id}),
                            );
                        }
                    }
                });
            }
        }
        if !self
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .launcher_queries
            .get(surface)
            .is_some_and(|q| q.id == query_id)
        {
            return Ok(json!([]));
        }
        let mut seen = std::collections::HashSet::new();
        out.reverse();
        out.retain(|r| seen.insert(r["id"].to_string()));
        out.sort_by(|a, b| {
            b["score"]
                .as_i64()
                .unwrap_or(0)
                .cmp(&a["score"].as_i64().unwrap_or(0))
        });
        out.truncate(80);
        Ok(json!(out))
    }
    fn bootstrap_launcher(&self, ctx: &CoreContext, target: Option<&str>) -> Result<(), String> {
        let packages = super::launcher::packages(ctx);
        let this = self.clone();
        let ctx = ctx.clone();
        let target = target.map(str::to_owned);
        let selected: Vec<_> = packages
            .into_iter()
            .filter(|p| {
                target.as_ref().is_none_or(|id| p.manifest["id"] == *id)
                    && super::launcher::needs_bootstrap(&ctx, p, target.is_some())
            })
            .take(8)
            .collect();
        let selected: Vec<_> = selected
            .into_iter()
            .map(|p| {
                let slot = this.slot(p.manifest["id"].as_str().unwrap_or(""));
                let generation = *slot.generation.borrow();
                (p, slot, generation)
            })
            .collect();
        let mut epoch = this
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .startup_epoch
            .subscribe();
        if !selected.is_empty() {
            tokio::spawn(async move {
                tokio::select! {biased;_=epoch.changed()=>{},_=async {
                    for (package,slot,generation) in selected {
                        let id=package.manifest["id"].as_str().unwrap_or("");
                        if *slot.generation.borrow()!=generation || !super::launcher::current(&ctx,&package){continue;}
                        if let Err(error)=this.process(&ctx,id,&slot,generation,"launcher-index-bootstrap").await {log::warn!("Launcher index bootstrap failed: {error}");}
                    }
                }=>{}}
            });
        }
        Ok(())
    }
    #[cfg(test)]
    pub(super) async fn test_launcher(&self, ctx: &CoreContext, body: Value) -> Value {
        self.launcher_search(ctx, &body).await.unwrap()
    }
    #[cfg(test)]
    pub(super) fn test_process_count(&self) -> usize {
        self.0.lock().unwrap().slots.len()
    }
    #[cfg(test)]
    pub(super) async fn test_process(&self, id: &str) -> Option<Arc<Process>> {
        self.slot(id).process.lock().await.clone()
    }
    pub async fn handle(
        &self,
        ctx: &CoreContext,
        files: &Arc<Mutex<()>>,
        channel: &str,
        args: &[Value],
    ) -> HandlerResult {
        let body = args.first().unwrap_or(&Value::Null);
        if matches!(
            channel,
            "core:extensions:launcher-search" | "extension:launcher-search"
        ) {
            return Some(self.launcher_search(ctx, body).await);
        }
        let id = body["sessionId"].as_str().unwrap_or("");
        let result = if id.starts_with(PREFIX) {
            self.dispatch(channel, body, id).await.map(Some)
        } else if channel == "extension:run-command" {
            let requested = body["extensionId"].as_str().unwrap_or("");
            if requested.starts_with("com.tezbar.native.") || v2_runtime::slug(requested).is_some()
            {
                self.launch(ctx, files, body).await
            } else {
                return None;
            }
        } else {
            return None;
        };
        match result {
            Ok(Some(v)) => Some(Ok(v)),
            Ok(None) => None,
            Err(message) => Some(Ok(json!({"ok":false,"message":message}))),
        }
    }
    async fn launch(
        &self,
        ctx: &CoreContext,
        files: &Arc<Mutex<()>>,
        body: &Value,
    ) -> Result<Option<Value>, String> {
        if body.get("argumentValues").is_some_and(|v| !v.is_object()) {
            return Err("argumentValues must be an object".into());
        }
        let requested = body["extensionId"].as_str().ok_or("Missing extensionId")?;
        let command = body["commandName"].as_str().ok_or("Missing commandName")?;
        let canonical = if requested.starts_with("com.tezbar.native.") {
            requested.to_string()
        } else {
            v2_runtime::canonical_id(v2_runtime::slug(requested).ok_or("Unknown extension")?)
        };
        let slot = self.slot(&canonical);
        let mut generation = slot.generation.subscribe();
        let selected_generation = *generation.borrow();
        let preferences = {
            let ctx = ctx.clone();
            let files = files.clone();
            let id = canonical.clone();
            let command = command.to_string();
            tokio::task::spawn_blocking(move || {
                let _guard = files.lock().map_err(|e| e.to_string())?;
                registry::installed_command(&ctx, &id, &command)
                    .map(|e| registry::preferences(&e, Some(&command)))
                    .transpose()
            })
            .await
            .map_err(|e| e.to_string())??
        };
        let Some(preferences) = preferences else {
            if requested.starts_with("raycast.") {
                return Ok(None);
            }
            // Surface schema-2/tamper/target errors, not a false legacy launch.
            process_install::resolve(ctx, &canonical)?;
            return Err("Extension command unavailable on this platform".into());
        };
        let p = self
            .process(ctx, &canonical, &slot, selected_generation, "command")
            .await?;
        let policy = p.package.manifest["commands"]
            .as_array()
            .and_then(|c| c.iter().find(|c| c["name"] == command))
            .cloned()
            .ok_or("Command not declared by installed executable")?;
        let id = format!("{PREFIX}{}", uuid::Uuid::new_v4());
        let entry = Arc::new(Entry {
            process: p.clone(),
            canonical,
            command: command.into(),
            policy,
            active: AtomicBool::new(true),
            cancel: watch::channel(false).0,
            search: Mutex::new(watch::channel(()).0),
            serial: AsyncMutex::new(()),
        });
        {
            let mut state = self.0.lock().map_err(|e| e.to_string())?;
            if *slot.generation.borrow() != selected_generation {
                return Err("Extension generation changed".into());
            }
            state
                .sessions
                .retain(|_, e| e.active.load(Ordering::Acquire) && e.process.alive());
            if state.sessions.len() >= MAX_SESSIONS {
                return Err("Too many extension sessions; close an extension first".into());
            }
            state.sessions.insert(id.clone(), entry.clone());
            p.sessions
                .lock()
                .map_err(|e| e.to_string())?
                .insert(id.clone(), 0);
            p.changed.notify_one();
        }
        let mut guard = LaunchCleanup {
            sessions: self.clone(),
            id: id.clone(),
            entry: Some(entry.clone()),
        };
        let mut cancelled = entry.cancel.subscribe();
        let value = tokio::select! {biased;_=generation.changed()=>Err("Extension generation changed".into()),_=cancelled.changed()=>Err("Extension session disposed".into()),result=p.call("session.open",json!({"sessionId":id,"commandName":command,"preferences":preferences,"argumentValues":body["argumentValues"].as_object().cloned().unwrap_or_default(),"searchText":body["searchText"],"launchContext":body["launchContext"]}),deadline(&entry))=>result}?;
        if *slot.generation.borrow() != selected_generation || !entry.active.load(Ordering::Acquire)
        {
            return Err("Extension session no longer active".into());
        }
        let mut value = annotate(&entry, &id, value)?;
        if value["mode"] != "view" {
            self.remove(&id);
            p.dispose(&id).await;
        } else {
            value["refreshMs"] = entry
                .policy
                .get("refreshMs")
                .cloned()
                .unwrap_or(Value::Null);
        }
        guard.entry = None;
        Ok(Some(value))
    }
    fn remove(&self, id: &str) -> Option<Arc<Entry>> {
        let entry = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .sessions
            .remove(id);
        if let Some(e) = &entry {
            e.active.store(false, Ordering::Release);
            e.cancel.send_replace(true);
            e.process
                .sessions
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(id);
            e.process.changed.notify_one();
        }
        entry
    }
    async fn dispatch(&self, channel: &str, body: &Value, id: &str) -> CoreResult {
        if channel == "extension:dispose-session" {
            let e = self.remove(id);
            if let Some(e) = &e {
                e.process.dispose(id).await;
            }
            return Ok(json!(e.is_some()));
        }
        if !matches!(
            channel,
            "extension:invoke-action"
                | "extension:search-text-changed"
                | "extension:load-more"
                | "extension:refresh-session"
        ) {
            return Err("Unsupported process session operation".into());
        }
        let entry = self
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .sessions
            .get(id)
            .cloned()
            .ok_or("Extension session not found")?;
        let mut cancel = entry.cancel.subscribe();
        let mut search = if channel == "extension:search-text-changed" {
            let tx = entry.search.lock().map_err(|e| e.to_string())?;
            tx.send_replace(());
            Some(tx.subscribe())
        } else {
            None
        };
        tokio::select! {biased;_=cancel.changed()=>Err("Extension session disposed".into()),_=async{if let Some(s)=&mut search{let _=s.changed().await;}else{std::future::pending::<()>().await;}}=>Err("Search superseded by a newer query".into()),result=async{
            let _serial=if channel=="extension:refresh-session"{match entry.serial.try_lock(){Ok(g)=>g,Err(_)=>return Ok(json!({"ok":true,"mode":"unchanged"}))}}
            else if entry.policy["concurrency"]=="reject-while-busy"{entry.serial.try_lock().map_err(|_|"An extension action is already in progress")?}else{entry.serial.lock().await};
            if !entry.active.load(Ordering::Acquire){return Err("Extension session disposed".into());}
            let value=entry.process.call("session.call",json!({"sessionId":id,"operation":channel,"payload":body}),deadline(&entry)).await?;
            if !entry.active.load(Ordering::Acquire){return Err("Extension session disposed".into());}
            annotate(&entry,id,value)
        }=>result}
    }
}
fn deadline(entry: &Entry) -> Duration {
    Duration::from_millis(
        entry.policy["timeoutMs"]
            .as_u64()
            .unwrap_or(120000)
            .clamp(1, 300000),
    )
}
fn annotate(e: &Entry, id: &str, mut value: Value) -> CoreResult {
    if !value["ok"].is_boolean() {
        return Err("Invalid extension result".into());
    }
    if value["ok"] == false {
        return Ok(value);
    }
    match value["mode"].as_str() {
        Some("view") => {
            value["sessionId"] = json!(id);
            value["extensionId"] = json!(e.canonical);
            value["commandName"] = json!(e.command);
            value["generation"] = json!(e.process.generation);
            value["refreshMs"] = e.policy.get("refreshMs").cloned().unwrap_or(Value::Null);
        }
        Some("no-view" | "unchanged") => {}
        _ => return Err("Invalid extension view mode".into()),
    }
    Ok(value)
}
struct LaunchCleanup {
    sessions: NativeSessions,
    id: String,
    entry: Option<Arc<Entry>>,
}
impl Drop for LaunchCleanup {
    fn drop(&mut self) {
        if let Some(entry) = self.entry.take() {
            self.sessions.remove(&self.id);
            let id = self.id.clone();
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    entry.process.dispose(&id).await;
                });
            } else {
                entry.process.stop();
            }
        }
    }
}
#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
