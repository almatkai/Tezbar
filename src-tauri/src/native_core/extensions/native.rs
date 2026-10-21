//! Bounded native sessions, serialized adapter state and cancellation-aware lifecycle.
#[cfg(test)]
use super::perplexity;
use super::{registry, v2_runtime};
use crate::native_core::common::{CoreContext, CoreResult, HandlerResult};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::sync::{watch, Mutex as AsyncMutex};

const PREFIX: &str = "native-extension:";
const MAX_SESSIONS: usize = 128;
struct Entry {
    ctx: CoreContext,
    slug: String,
    extension_id: String,
    command: String,
    preferences: Value,
    state: AsyncMutex<Value>,
    active: AtomicBool,
    cancel: watch::Sender<bool>,
    search: Mutex<watch::Sender<()>>,
    #[cfg(test)]
    adapter: Mutex<Option<TestAdapter>>,
}
struct State {
    generation: u64,
    sessions: HashMap<String, Arc<Entry>>,
    cancel_launches: watch::Sender<u64>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            generation: 0,
            sessions: HashMap::new(),
            cancel_launches: watch::channel(0).0,
        }
    }
}
// A cancelled launch has no renderer to dispose its adapter resources.
struct LaunchCleanup(Option<Arc<Entry>>);
impl Drop for LaunchCleanup {
    fn drop(&mut self) {
        if let Some(entry) = self.0.take() {
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    cleanup(&entry).await;
                });
            }
        }
    }
}
#[derive(Clone, Default)]
pub(super) struct NativeSessions(Arc<Mutex<State>>);

impl NativeSessions {
    pub async fn clear(&self) {
        self.clear_matching(None).await;
    }
    pub async fn clear_slug(&self, slug: &str) {
        self.clear_matching(Some(slug)).await;
    }
    async fn clear_matching(&self, slug: Option<&str>) {
        let entries = {
            let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
            state.generation = state.generation.wrapping_add(1);
            state.cancel_launches.send_replace(state.generation);
            let ids: Vec<_> = state
                .sessions
                .iter()
                .filter(|(_, e)| slug.is_none_or(|s| s == e.slug))
                .map(|(id, _)| id.clone())
                .collect();
            ids.into_iter()
                .filter_map(|id| state.sessions.remove(&id))
                .collect::<Vec<_>>()
        };
        // Signal every in-flight call before awaiting any adapter cleanup.
        for entry in &entries {
            entry.active.store(false, Ordering::Release);
            entry.cancel.send_replace(true);
        }
        // Detached cleanup continues even if the shutdown/uninstall caller is cancelled.
        let cleanup_tasks: Vec<_> = entries
            .into_iter()
            .map(|entry| {
                tokio::spawn(async move {
                    cleanup(&entry).await;
                })
            })
            .collect();
        for task in cleanup_tasks {
            let _ = task.await;
        }
    }
    pub async fn handle(
        &self,
        ctx: &CoreContext,
        files: &Arc<Mutex<()>>,
        channel: &str,
        args: &[Value],
    ) -> HandlerResult {
        let body = args.first().unwrap_or(&Value::Null);
        let id = body["sessionId"].as_str().unwrap_or("");
        // Even a run-command carrying a stale native ID must never reach the worker.
        let result = if id.starts_with(PREFIX) {
            self.dispatch(ctx, files, channel, body, id).await.map(Some)
        } else if channel == "extension:run-command"
            && v2_runtime::supports(
                body["extensionId"].as_str().unwrap_or(""),
                body["commandName"].as_str().unwrap_or(""),
            )
        {
            self.launch(ctx, files, body).await
        } else if channel == "extension:run-command"
            && body["extensionId"]
                .as_str()
                .is_some_and(|id| id.starts_with("com.tezbar.native."))
        {
            Err("Native extension command is not available on this platform.".into())
        } else {
            return None;
        };
        match result {
            Ok(None) => None,
            Ok(Some(value)) => Some(Ok(value)),
            Err(message) => Some(Ok(json!({"ok":false,"message":message}))),
        }
    }
    async fn launch(
        &self,
        ctx: &CoreContext,
        files: &Arc<Mutex<()>>,
        body: &Value,
    ) -> Result<Option<Value>, String> {
        let (generation, mut cancel) = {
            let state = self.0.lock().map_err(|e| e.to_string())?;
            (state.generation, state.cancel_launches.subscribe())
        };
        let extension_id = body["extensionId"].as_str().ok_or("Missing extensionId")?;
        let slug = v2_runtime::slug(extension_id).ok_or("Unknown native extension")?;
        let command = body["commandName"].as_str().ok_or("Missing commandName")?;
        let Some(preferences) = installed(ctx, files, extension_id, command, true).await? else {
            // Canonical IDs never name legacy worker packages.
            if extension_id.starts_with("com.tezbar.native.") {
                return Err("Native extension command is not installed.".into());
            }
            return Ok(None);
        };
        let entry = Arc::new(Entry {
            ctx: ctx.clone(),
            slug: slug.into(),
            extension_id: extension_id.into(),
            command: command.into(),
            preferences,
            state: AsyncMutex::new(json!({})),
            active: AtomicBool::new(true),
            cancel: watch::channel(false).0,
            search: Mutex::new(watch::channel(()).0),
            #[cfg(test)]
            adapter: Mutex::new(None),
        });
        let mut cancellation_cleanup = LaunchCleanup(Some(entry.clone()));
        let payload = payload(&entry, body);
        let mut state = entry.state.lock().await;
        let result = tokio::select! {
            biased;
            _ = cancel.changed() => Err("Native extension sessions were cleared; reopen the extension".into()),
            result = v2_runtime::handle(ctx, slug, command, "extension:run-command", &payload, &mut state) => result,
        };
        drop(state);
        cancellation_cleanup.0 = None;
        let mut value = match result {
            Ok(v) => v,
            Err(e) => {
                cleanup(&entry).await;
                return Err(e);
            }
        };
        if value["mode"] != "view" {
            return Ok(Some(value));
        }
        let id = format!("{PREFIX}{}", uuid::Uuid::new_v4());
        let registration = {
            let mut state = self.0.lock().map_err(|e| e.to_string())?;
            if state.generation != generation {
                Err("Native extension sessions were cleared; reopen the extension")
            } else if state.sessions.len() >= MAX_SESSIONS {
                Err("Too many native extension sessions; close an extension first")
            } else {
                state.sessions.insert(id.clone(), entry.clone());
                Ok(())
            }
        };
        if let Err(error) = registration {
            cleanup(&entry).await;
            return Err(error.into());
        }
        annotate(&entry, &id, &mut value);
        Ok(Some(value))
    }
    async fn dispatch(
        &self,
        ctx: &CoreContext,
        files: &Arc<Mutex<()>>,
        channel: &str,
        body: &Value,
        id: &str,
    ) -> CoreResult {
        if channel == "extension:dispose-session" {
            let entry = self
                .0
                .lock()
                .map_err(|e| e.to_string())?
                .sessions
                .remove(id);
            if let Some(entry) = &entry {
                entry.active.store(false, Ordering::Release);
                entry.cancel.send_replace(true);
                let cleanup_entry = entry.clone();
                let _ = tokio::spawn(async move {
                    cleanup(&cleanup_entry).await;
                })
                .await;
            }
            return Ok(json!(entry.is_some()));
        }
        let entry = self
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .sessions
            .get(id)
            .cloned()
            .ok_or("Extension session not found.")?;
        if !matches!(
            channel,
            "extension:invoke-action"
                | "extension:search-text-changed"
                | "extension:load-more"
                | "extension:refresh-session"
        ) {
            return Err(format!("Unsupported native extension operation: {channel}"));
        }
        // Register before waiting for state: a new query cancels both an HTTP request
        // already running and an older query still waiting behind an action.
        let mut search_cancel = if channel == "extension:search-text-changed" {
            let sender = entry.search.lock().map_err(|e| e.to_string())?;
            sender.send_replace(());
            Some(sender.subscribe())
        } else {
            None
        };
        let search_version = search_cancel.clone();
        let mut cancel = entry.cancel.subscribe();
        if !entry.active.load(Ordering::Acquire) {
            return Err("Extension session not found.".into());
        }
        tokio::select! {
            biased;
            _ = cancel.changed() => Err("Extension session not found.".into()),
            _ = async {
                match &mut search_cancel {
                    Some(cancel) => { let _ = cancel.changed().await; }
                    None => std::future::pending::<()>().await,
                }
            } => Err("Search superseded by a newer query.".into()),
            result = async {
                let mut state = if channel == "extension:refresh-session" {
                    match entry.state.try_lock() {
                        Ok(state) => state,
                        Err(_) => return Ok(super::v2_sdk::unchanged()),
                    }
                } else if entry.slug == "perplexity" {
                    entry.state.try_lock().map_err(|_| "An extension action is already in progress.".to_string())?
                } else { entry.state.lock().await };
                if !entry.active.load(Ordering::Acquire) { return Err("Extension session not found.".into()); }
                if channel != "extension:refresh-session" && installed(ctx, files, &entry.extension_id, &entry.command, false).await?.is_none() {
                    entry.active.store(false, Ordering::Release);
                    entry.cancel.send_replace(true);
                    self.0.lock().map_err(|e| e.to_string())?.sessions.remove(id);
                    drop(state);
                    let cleanup_entry = entry.clone();
                    tokio::spawn(async move { cleanup(&cleanup_entry).await; });
                    return Err("Extension command is no longer installed.".into());
                }
                // A dropped search future must not leave partially updated query/results.
                let mut staged = state.clone();
                let target = if channel == "extension:search-text-changed" { &mut staged } else { &mut *state };
                let mut value = run_adapter(&entry, ctx, channel, &payload(&entry, body), target).await?;
                if !entry.active.load(Ordering::Acquire) { return Err("Extension session not found.".into()); }
                if channel == "extension:search-text-changed" {
                    let _registration = entry.search.lock().map_err(|e| e.to_string())?;
                    if search_version.as_ref().is_some_and(|version| version.has_changed().unwrap_or(true)) {
                        return Err("Search superseded by a newer query.".into());
                    }
                    *state = staged;
                }
                annotate(&entry, id, &mut value);
                Ok(value)
            } => result,
        }
    }
}
// Test-only adapter seam exercises the public IPC lifecycle without external services.
#[cfg(test)]
type TestAdapter =
    for<'a> fn(
        &str,
        &Value,
        &'a mut Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = CoreResult> + Send + 'a>>;
async fn run_adapter(
    entry: &Entry,
    ctx: &CoreContext,
    operation: &str,
    payload: &Value,
    state: &mut Value,
) -> CoreResult {
    #[cfg(test)]
    {
        let adapter = *entry.adapter.lock().unwrap();
        if let Some(adapter) = adapter {
            return adapter(operation, payload, state).await;
        }
    }
    v2_runtime::handle(ctx, &entry.slug, &entry.command, operation, payload, state).await
}
fn payload(entry: &Entry, body: &Value) -> Value {
    let mut body = body.clone();
    if !body.is_object() {
        body = json!({});
    }
    body["preferences"] = entry.preferences.clone();
    body
}
fn annotate(entry: &Entry, id: &str, value: &mut Value) {
    if value["mode"] == "view" {
        value["sessionId"] = json!(id);
        value["extensionId"] = json!(entry.extension_id);
        value["commandName"] = json!(entry.command);
    }
}
async fn cleanup(entry: &Entry) {
    let mut state = entry.state.lock().await;
    if let Err(error) = v2_runtime::handle(
        &entry.ctx,
        &entry.slug,
        &entry.command,
        "extension:dispose-session",
        &payload(entry, &json!({})),
        &mut state,
    )
    .await
    {
        log::warn!("Native extension {} cleanup failed: {error}", entry.slug);
    }
}
async fn installed(
    ctx: &CoreContext,
    files: &Arc<Mutex<()>>,
    extension_id: &str,
    command: &str,
    preferences: bool,
) -> Result<Option<Value>, String> {
    let (ctx, files, extension_id, command) = (
        ctx.clone(),
        files.clone(),
        extension_id.to_owned(),
        command.to_owned(),
    );
    tokio::task::spawn_blocking(move || {
        let _guard = files.lock().map_err(|e| e.to_string())?;
        registry::installed_command(&ctx, &extension_id, &command)
            .map(|extension| {
                if preferences {
                    registry::preferences(&extension, Some(&command))
                } else {
                    Ok(Value::Null)
                }
            })
            .transpose()
    })
    .await
    .map_err(|e| e.to_string())?
}
#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;
