//! One typed request seam; views retain the task and fence replies by owner revision.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
#[derive(Clone)]
pub struct CoreClient(
    pub Arc<tezbar_core::NativeCore>,
    Arc<tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
);
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SearchRow {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    pub action: Value,
}
pub enum Request {
    Search(String),
    Execute {
        action: Value,
        query: String,
        rank: usize,
    },
    ConfirmQuit,
    Config,
}
impl CoreClient {
    pub fn new(core: Arc<tezbar_core::NativeCore>) -> Self {
        Self(core, Arc::default())
    }
    pub async fn request(&self, request: Request) -> Result<Value, String> {
        let (channel, args) = match request {
            Request::Search(q) => ("search:all", vec![json!(q)]),
            Request::Execute {
                action,
                query,
                rank,
            } => (
                "search:execute",
                vec![action, json!({"query":query,"rank":rank})],
            ),
            Request::ConfirmQuit => ("app:confirm-quit", vec![]),
            Request::Config => ("config:get", vec![]),
        };
        self.0.request(channel, &args).await
    }
    pub async fn search(&self, q: String) -> Result<Vec<SearchRow>, String> {
        serde_json::from_value(self.request(Request::Search(q)).await?).map_err(|e| e.to_string())
    }
    pub async fn start(&self) -> Result<(), String> {
        self.0
            .start_with_options(tezbar_core::StartupOptions::ISOLATED_PREVIEW)
            .await
    }
    pub async fn shutdown(&self) -> Result<(), String> {
        let mut owner = self.1.lock().await;
        if owner.is_none() {
            let core = self.0.clone();
            *owner = Some(tokio::spawn(async move {
                core.shutdown().await;
            }));
        }
        // Retain a timed-out shutdown task. Cancelling NativeCore::shutdown halfway through would leave
        // its closing flag set and let a second call falsely report that owned children had drained.
        match tokio::time::timeout(std::time::Duration::from_secs(8), owner.as_mut().unwrap()).await
        {
            Ok(Ok(())) => {
                owner.take();
                Ok(())
            }
            Ok(Err(e)) => Err(e.to_string()),
            Err(_) => {
                Err("Core is still stopping after eight seconds; quit remains deferred".into())
            }
        }
    }
}
#[derive(Default, Debug)]
pub struct Generation {
    value: u64,
    alive: bool,
}
impl Generation {
    pub fn next(&mut self) -> u64 {
        self.alive = true;
        self.value = self.value.wrapping_add(1);
        self.value
    }
    pub fn accepts(&self, value: u64) -> bool {
        self.alive && self.value == value
    }
    pub fn cancel(&mut self) {
        self.alive = false;
        self.value = self.value.wrapping_add(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn owner_cancellation_and_stale_replies() {
        let mut g = Generation::default();
        let a = g.next();
        let b = g.next();
        assert!(!g.accepts(a));
        assert!(g.accepts(b));
        g.cancel();
        assert!(!g.accepts(b));
    }
}
