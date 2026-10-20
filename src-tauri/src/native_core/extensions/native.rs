//! Own native sessions while retaining the compatibility worker's wire protocol.
use super::{perplexity, registry};
use crate::native_core::common::{CoreContext, CoreResult, HandlerResult};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

const PREFIX: &str = "native-extension:";
const MAX_SESSIONS: usize = 128;

struct Entry {
    command: perplexity::Session,
    active: AtomicBool,
    busy: AtomicBool,
}

// Release on cancellation as well as on success/error. No mutex spans an await.
struct Flight(Arc<Entry>);
impl Drop for Flight {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::Release);
    }
}

#[derive(Default)]
struct State {
    generation: u64,
    sessions: HashMap<String, Arc<Entry>>,
}

#[derive(Clone, Default)]
pub(super) struct NativeSessions(Arc<Mutex<State>>);

impl NativeSessions {
    pub fn clear(&self) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.generation = state.generation.wrapping_add(1);
        for (_, entry) in state.sessions.drain() {
            entry.active.store(false, Ordering::Release);
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
        let launch = channel == "extension:run-command"
            && matches!(
                body["extensionId"].as_str(),
                Some("raycast.perplexity" | "perplexity")
            )
            && body["commandName"] == perplexity::COMMAND;
        let id = body["sessionId"].as_str().unwrap_or("");
        if !launch && !id.starts_with(PREFIX) {
            return None;
        }
        let result = if launch {
            self.launch(ctx, files, body).await
        } else {
            self.dispatch(ctx, files, channel, body, id).await.map(Some)
        };
        match result {
            Ok(None) => None, // Not installed: let the worker handle compatibility/errors.
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
        let generation = self.0.lock().map_err(|e| e.to_string())?.generation;
        let Some(preferences) = installed(ctx, files, true).await? else {
            return Ok(None);
        };
        let query = match body.get("argumentValues") {
            None => "",
            Some(values) if values.is_object() => match values.get("query") {
                None => "",
                Some(Value::String(query)) => query,
                _ => return Err("query argument must be text".into()),
            },
            _ => return Err("argumentValues must be an object".into()),
        };
        let command = perplexity::Session {
            use_app: preferences["perplexityApp"].as_bool().unwrap_or(false),
        };
        let id = format!("{PREFIX}{}", uuid::Uuid::new_v4());
        if !query.is_empty() {
            if self.0.lock().map_err(|e| e.to_string())?.generation != generation {
                return Err("Native extension sessions were cleared; reopen the extension".into());
            }
            // No renderer-visible session exists; cancellation cannot leak capacity.
            return command.open(ctx, query).await.map(Some);
        }
        let entry = Arc::new(Entry {
            command,
            active: AtomicBool::new(true),
            busy: AtomicBool::new(false),
        });
        {
            let mut state = self.0.lock().map_err(|e| e.to_string())?;
            if state.generation != generation {
                return Err("Native extension sessions were cleared; reopen the extension".into());
            }
            if state.sessions.len() >= MAX_SESSIONS {
                return Err("Too many native extension sessions; close an extension first".into());
            }
            state.sessions.insert(id.clone(), entry.clone());
        }
        Ok(Some(entry.command.view(&id)))
    }

    fn remove(&self, id: &str) -> Result<bool, String> {
        let entry = self
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .sessions
            .remove(id);
        if let Some(entry) = &entry {
            entry.active.store(false, Ordering::Release);
        }
        Ok(entry.is_some())
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
            return self.remove(id).map(|removed| json!(removed));
        }
        let entry = self
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .sessions
            .get(id)
            .cloned()
            .ok_or("Extension session not found.")?;
        if channel == "extension:refresh-session" {
            return Ok(json!({"ok":true,"mode":"unchanged"}));
        }
        if installed(ctx, files, false).await?.is_none() {
            self.remove(id)?;
            return Err("Extension command is no longer installed.".into());
        }
        if !entry.active.load(Ordering::Acquire) {
            return Err("Extension session not found.".into());
        }
        match channel {
            "extension:invoke-action" => {
                if body["actionId"] != perplexity::SUBMIT {
                    return Err("Action is no longer available in this session.".into());
                }
                let query = match body.get("formValues") {
                    None => "",
                    Some(values) if values.is_object() => match values.get("query") {
                        None => "",
                        Some(Value::String(query)) => query,
                        _ => return Err("query form value must be text".into()),
                    },
                    _ => return Err("formValues must be an object".into()),
                };
                if query.is_empty() {
                    return Err("Query cannot be empty".into());
                }
                if entry.busy.swap(true, Ordering::AcqRel) {
                    return Err("An extension action is already in progress.".into());
                }
                let _flight = Flight(entry.clone());
                entry.command.open(ctx, query).await
            }
            _ => Err(format!("Unsupported native extension operation: {channel}")),
        }
    }
}

// Registry IO stays on its existing serialized blocking lane, including on launch.
async fn installed(
    ctx: &CoreContext,
    files: &Arc<Mutex<()>>,
    preferences: bool,
) -> Result<Option<Value>, String> {
    let ctx = ctx.clone();
    let files = files.clone();
    tokio::task::spawn_blocking(move || {
        let _guard = files.lock().map_err(|e| e.to_string())?;
        registry::installed_command(&ctx, perplexity::EXTENSION, perplexity::COMMAND)
            .map(|extension| {
                if preferences {
                    registry::preferences(&extension, Some(perplexity::COMMAND))
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
