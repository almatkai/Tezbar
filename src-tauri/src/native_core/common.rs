use serde_json::{json, Value};
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

pub type CoreResult = Result<Value, String>;
pub type HandlerResult = Option<CoreResult>;
pub type CoreFuture = Pin<Box<dyn Future<Output = CoreResult> + Send>>;
pub type CoreRouter = Arc<dyn Fn(String, Vec<Value>) -> CoreFuture + Send + Sync>;
pub type EventSink = Arc<dyn Fn(&str, Value) + Send + Sync>;

#[derive(Clone, Debug)]
pub struct CorePaths {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    pub resource_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub home_dir: PathBuf,
}

/// Shared profile and in-process communication. No runtime process is involved.
#[derive(Clone)]
pub struct CoreContext {
    pub paths: CorePaths,
    pub config: Arc<ConfigStore>,
    pub app: Option<AppHandle>,
    events: EventSink,
    router: Arc<OnceLock<CoreRouter>>,
    foreground_until: Arc<AtomicU64>,
}

impl CoreContext {
    pub fn new(paths: CorePaths, app: Option<AppHandle>) -> Self {
        let event_app = app.clone();
        Self {
            config: Arc::new(ConfigStore::new(paths.config_dir.join("config.json"))),
            paths,
            app,
            events: Arc::new(move |channel, payload| {
                if let Some(app) = &event_app {
                    if let Err(error) = app.emit(channel, payload) {
                        log::warn!("Could not emit {channel}: {error}");
                    }
                }
            }),
            router: Arc::new(OnceLock::new()),
            foreground_until: Arc::new(AtomicU64::new(0)),
        }
    }

    #[cfg(test)]
    pub fn headless(paths: CorePaths) -> Self {
        Self::new(paths, None)
    }

    #[cfg(test)]
    pub fn with_event_sink(mut self, sink: EventSink) -> Self {
        self.events = sink;
        self
    }

    pub fn set_router(&self, router: CoreRouter) -> Result<(), String> {
        self.router
            .set(router)
            .map_err(|_| "Native core dispatcher was already initialized".to_string())
    }

    pub fn emit(&self, channel: &str, payload: Value) {
        (self.events)(channel, payload);
    }

    pub async fn request(&self, channel: &str, args: Vec<Value>) -> CoreResult {
        let router = self
            .router
            .get()
            .ok_or_else(|| "Native core dispatcher is not initialized".to_string())?;
        router(channel.to_string(), args).await
    }

    pub fn note_foreground(&self) {
        self.foreground_until
            .store(now_ms().saturating_add(1_200), Ordering::Relaxed);
    }

    pub fn foreground_active(&self) -> bool {
        now_ms() < self.foreground_until.load(Ordering::Relaxed)
    }
}

/// One writer for the legacy profile, preserving keys owned by other modules.
pub struct ConfigStore {
    path: PathBuf,
    transaction: Mutex<()>,
}

impl ConfigStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            transaction: Mutex::new(()),
        }
    }

    #[cfg(test)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn read_unlocked(&self) -> CoreResult {
        match read_json(&self.path) {
            Ok(value) if value.is_object() => Ok(value),
            Ok(_) => Err(format!(
                "Configuration is not an object: {}",
                self.path.display()
            )),
            Err(error) if !self.path.exists() => {
                // Only a missing profile means defaults. Never overwrite malformed data.
                if self.path.try_exists().map_err(|e| e.to_string())? {
                    Err(error)
                } else {
                    Ok(json!({}))
                }
            }
            Err(error) => Err(error),
        }
    }

    pub fn read(&self) -> CoreResult {
        let _guard = self.transaction.lock().map_err(|e| e.to_string())?;
        self.read_unlocked()
    }

    pub fn patch(&self, patch: &Value) -> CoreResult {
        let patch = patch
            .as_object()
            .ok_or("Configuration patch must be an object")?;
        self.update(|current| {
            let object = current
                .as_object_mut()
                .ok_or("Configuration must be an object")?;
            for (key, value) in patch {
                object.insert(key.clone(), value.clone());
            }
            Ok(())
        })
    }

    pub fn update<F>(&self, update: F) -> CoreResult
    where
        F: FnOnce(&mut Value) -> Result<(), String>,
    {
        let _guard = self.transaction.lock().map_err(|e| e.to_string())?;
        let mut current = self.read_unlocked()?;
        update(&mut current)?;
        if !current.is_object() {
            return Err("Configuration must remain an object".to_string());
        }
        write_json_atomic(&self.path, &current)?;
        Ok(current)
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn read_json(path: &Path) -> CoreResult {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn write_json_atomic(path: &Path, value: &Value) -> Result<(), String> {
    let parent = path.parent().ok_or("JSON path has no parent directory")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut temporary, value).map_err(|e| e.to_string())?;
    temporary.write_all(b"\n").map_err(|e| e.to_string())?;
    temporary.flush().map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    temporary.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_preserves_unknown_keys_and_serializes_concurrent_updates() {
        let directory = tempfile::tempdir().unwrap();
        let config = Arc::new(ConfigStore::new(directory.path().join("config.json")));
        config
            .patch(&json!({"futureSetting": {"keep": true}, "count": 0}))
            .unwrap();
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let config = config.clone();
                std::thread::spawn(move || {
                    for _ in 0..10 {
                        config
                            .update(|value| {
                                value["count"] = json!(value["count"].as_u64().unwrap() + 1);
                                Ok(())
                            })
                            .unwrap();
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(
            config.read().unwrap(),
            json!({"futureSetting":{"keep":true},"count":40})
        );
    }

    #[test]
    fn malformed_profile_is_never_replaced_by_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        std::fs::write(&path, b"{broken").unwrap();
        let config = ConfigStore::new(path.clone());
        assert!(config.patch(&json!({"provider":"ollama"})).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"{broken");
    }
}
