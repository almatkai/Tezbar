//! One typed request seam; views retain the task and fence replies by owner revision.
use serde_json::{Value, json};
use std::sync::Arc;
#[derive(Clone)]
pub struct CoreClient(
    pub Arc<tezbar_core::NativeCore>,
    Arc<tokio::sync::Mutex<ShutdownOwner>>,
    Arc<tokio::sync::Mutex<StartOwner>>,
    #[cfg(feature = "bench-control")] Arc<crate::bench_control::ShutdownGate>,
    Arc<tokio::sync::Semaphore>,
);
#[derive(Default)]
struct ShutdownOwner {
    task: Option<tokio::task::JoinHandle<()>>,
    complete: bool,
}
#[derive(Default)]
struct StartOwner {
    task: Option<tokio::task::JoinHandle<Result<(), String>>>,
    ready: bool,
}
impl ShutdownOwner {
    async fn wait(&mut self, bound: std::time::Duration) -> Result<(), String> {
        let result = tokio::time::timeout(bound, self.task.as_mut().unwrap()).await;
        match result {
            Ok(result) => {
                self.task.take(); // A completed JoinHandle must never be polled twice, even after panic.
                self.complete = true;
                result.map_err(|e| {
                    format!("Core shutdown failed; exiting with supervisor cleanup: {e}")
                })
            }
            Err(_) => Err(
                "Core shutdown exceeded eight seconds; proceeding with supervisor cleanup".into(),
            ),
        }
    }
}
pub type SearchRow = tezbar_ui_model::launcher::Row;
#[derive(Clone, Debug)]
pub enum Request {
    Terminal(crate::surfaces::terminal::TerminalRequest),
    Chat(crate::surfaces::ai_chat::ChatRequest),
    Settings(crate::surfaces::settings::SettingsRequest),
    Utility(crate::surfaces::utilities::UtilityRequest),
    Search(String),
    Execute {
        action: Value,
        query: String,
        rank: usize,
    },
    Activate {
        action: Value,
        query: String,
        rank: usize,
        result_id: String,
    },
    Record {
        action: Value,
        query: String,
        rank: usize,
        result_id: String,
    },
    CompletePath(String),
    AssetIcon {
        kind: String,
        path: String,
    },
    ResolveDirectory(String),
    DirectoryVisit(String),
    Chats,
    TerminalSessions,
    TerminalPrompt,
    ExtensionSearch(Value),
    ExtensionRun(Value),
    Query(String),
    QueryOwned {
        query: String,
        owner: String,
    },
    QuickLook(Vec<String>),
    VaultCopy(String),
    Speak(String),
    Currency(String),
    LlmConfig,
    LlmSet(Value),
    VaultStatus,
    VaultUnlock,
    TerminalStop(String),
    TerminalDelete(String),
    Open(String),
    ConfirmQuit,
    Config,
}
impl Request {
    /// Canonical public NativeCore channels. Internal core:* routing stays inside the core.
    pub fn wire(self) -> (&'static str, Vec<Value>) {
        let context = |action, query, rank, id: Option<String>| json!({"action":action,"context":{"query":query,"rank":rank,"resultId":id}});
        match self {
            Self::Terminal(request) => request.wire(),
            Self::Chat(request) => request.wire(),
            Self::Utility(request) => request.wire(),
            Self::Settings(request) => request.wire(),
            Self::Search(q) => ("search:all", vec![json!(q)]),
            Self::Execute {
                action,
                query,
                rank,
            } => ("search:execute", vec![context(action, query, rank, None)]),
            Self::Activate {
                action,
                query,
                rank,
                result_id,
            } => (
                "search:execute",
                vec![context(action, query, rank, Some(result_id))],
            ),
            Self::Record {
                action,
                query,
                rank,
                result_id,
            } => (
                "search:record-usage",
                vec![context(action, query, rank, Some(result_id))],
            ),
            Self::CompletePath(q) => ("path:complete", vec![json!(q)]),
            Self::AssetIcon { kind, path } => (
                "asset-icon:data-url",
                vec![json!({"kind":kind,"path":path})],
            ),
            Self::ResolveDirectory(q) => ("path:resolve-directory", vec![json!(q)]),
            Self::DirectoryVisit(p) => ("directory-visit:record", vec![json!(p)]),
            Self::Chats => ("chat:list", vec![json!(100)]),
            Self::TerminalSessions => ("terminal:list", vec![]),
            Self::TerminalPrompt => ("terminal:get-prompt-info", vec![]),
            Self::ExtensionSearch(p) => ("extension:launcher-search", vec![p]),
            Self::ExtensionRun(p) => ("extension:run-command", vec![p]),
            Self::Query(q) => ("query", vec![json!(q)]),
            Self::QueryOwned { query, owner } => ("query", vec![json!(query), json!(owner)]),
            Self::QuickLook(paths) => ("file:quick-look", vec![json!(paths)]),
            Self::VaultCopy(id) => ("vault:copy", vec![json!(id)]),
            Self::LlmConfig => ("llm-config-get", vec![]),
            Self::LlmSet(patch) => ("llm-config-set", vec![patch]),
            Self::Currency(base) => ("currency:frankfurter-latest", vec![json!(base)]),
            Self::VaultStatus => ("vault:status", vec![]),
            Self::VaultUnlock => ("vault:unlock", vec![]),
            Self::TerminalStop(id) => ("terminal:kill", vec![json!({"sessionId":id})]),
            Self::TerminalDelete(id) => ("terminal:delete", vec![json!({"sessionId":id})]),
            Self::Speak(text) => ("voice:tts:speak", vec![json!({"text":text})]),
            Self::Open(target) => ("shell:open", vec![json!(target)]),
            Self::ConfirmQuit => ("app:confirm-quit", vec![]),
            Self::Config => ("config:get", vec![]),
        }
    }
}
/// Injectable transport: GPUI integration tests use synthetic responses, never system actions.
pub trait RequestAdapter: Send + Sync + 'static {
    fn send(
        &self,
        request: Request,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>>;
}
impl RequestAdapter for CoreClient {
    fn send(
        &self,
        request: Request,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>> {
        let client = self.clone();
        Box::pin(async move { client.request(request).await })
    }
}
impl CoreClient {
    /// Read-only HTTP stays cancellable, while sharing the app's request admission bound.
    /// Unlike effectful core work, dropping this future must not detach a network task.
    pub(crate) async fn release_notes(
        &self,
        url: String,
        accept: &'static str,
    ) -> Result<String, String> {
        #[cfg(feature = "bench-control")]
        let admissions = self.4.clone();
        #[cfg(not(feature = "bench-control"))]
        let admissions = self.3.clone();
        let _permit = admissions
            .try_acquire_owned()
            .map_err(|_| "Native operations are still finishing; try again".to_owned())?;
        tezbar_update::notes::http::fetch(&url, accept).await
    }
    pub fn new(core: Arc<tezbar_core::NativeCore>) -> Self {
        Self(
            core,
            Arc::default(),
            Arc::default(),
            #[cfg(feature = "bench-control")]
            Arc::default(),
            Arc::new(tokio::sync::Semaphore::new(16)),
        )
    }
    pub async fn request(&self, request: Request) -> Result<Value, String> {
        let (channel, args) = request.wire();
        let core = self.0.clone();
        #[cfg(feature = "bench-control")]
        let admissions = self.4.clone();
        #[cfg(not(feature = "bench-control"))]
        let admissions = self.3.clone();
        owned_request(admissions, async move {
            if channel == "file:quick-look" {
                use tezbar_core::host::SurfaceOp;
                core.ctx
                    .host
                    .surface(SurfaceOp::SuppressBlurHide(true))
                    .await?;
                let result = core.request(channel, &args).await;
                let restored = core
                    .ctx
                    .host
                    .surface(SurfaceOp::SuppressBlurHide(false))
                    .await;
                result.and_then(|value| restored.map(|_| value))
            } else {
                if channel.starts_with("core:terminal:") {
                    // Finite TerminalRequest is the only caller; generic NativeCore::request
                    // continues rejecting every core:* renderer/extension route.
                    core.ctx.request(channel, args).await
                } else {
                    core.request(channel, &args).await
                }
            }
        })
        .await
    }
    pub async fn search(&self, q: String) -> Result<Vec<SearchRow>, String> {
        serde_json::from_value(self.request(Request::Search(q)).await?).map_err(|e| e.to_string())
    }
    pub async fn start(&self) -> Result<(), String> {
        let mut owner = self.2.lock().await;
        if owner.ready {
            return Ok(());
        }
        if owner.task.is_none() {
            let core = self.0.clone();
            owner.task = Some(tokio::spawn(async move {
                let result = core
                    .start_with_options(tezbar_core::StartupOptions::ISOLATED_PREVIEW)
                    .await;
                log::info!("Native core start completed: {result:?}");
                result
            }));
        }
        let result = owner
            .task
            .as_mut()
            .unwrap()
            .await
            .map_err(|e| e.to_string())
            .and_then(|v| v);
        owner.task.take();
        owner.ready = result.is_ok();
        result
    }
    #[cfg(feature = "bench-control")]
    pub fn hold_shutdown_for_fixture(&self) {
        self.3.hold();
    }
    pub async fn shutdown(&self) -> Result<(), String> {
        self.shutdown_with_budget(std::time::Duration::from_secs(8))
            .await
    }
    pub(crate) async fn shutdown_with_budget(
        &self,
        budget: std::time::Duration,
    ) -> Result<(), String> {
        let mut owner = self.1.lock().await;
        if owner.complete {
            return Ok(());
        }
        if owner.task.is_none() {
            let client = self.clone();
            owner.task = Some(tokio::spawn(async move {
                // Start is independently owned. Dropping its GPUI await never aborts it.
                let mut start = client.2.lock().await;
                if let Some(task) = start.task.as_mut() {
                    let _ = task.await;
                    start.task.take();
                }
                drop(start);
                #[cfg(feature = "bench-control")]
                client.3.wait().await;
                client.0.shutdown().await;
                log::info!("Native core shutdown completed");
            }));
        }
        owner.wait(budget).await
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

#[cfg(test)]
mod shutdown_repair_tests {
    use super::*;
    #[tokio::test]
    async fn shutdown_consumes_join_error_and_timeout_keeps_same_owner() {
        let mut owner = ShutdownOwner {
            task: Some(tokio::spawn(async { panic!("fixture shutdown panic") })),
            complete: false,
        };
        assert!(owner.wait(std::time::Duration::from_secs(1)).await.is_err());
        assert!(owner.complete);
        assert!(owner.task.is_none());
        let mut owner = ShutdownOwner {
            task: Some(tokio::spawn(std::future::pending())),
            complete: false,
        };
        let id = owner.task.as_ref().unwrap().id();
        assert!(
            owner
                .wait(std::time::Duration::from_millis(20))
                .await
                .is_err()
        );
        assert_eq!(owner.task.as_ref().unwrap().id(), id);
        owner.task.take().unwrap().abort();
    }
}

/// Admission belongs to core work, not its UI waiter. Dropping the JoinHandle detaches the already
/// admitted operation; it never cancels a command halfway through a write or a host effect.
async fn owned_request(
    admissions: Arc<tokio::sync::Semaphore>,
    work: impl std::future::Future<Output = Result<Value, String>> + Send + 'static,
) -> Result<Value, String> {
    let permit = admissions
        .try_acquire_owned()
        .map_err(|_| "Native operations are still finishing; try again".to_owned())?;
    tokio::spawn(async move {
        let _permit = permit;
        let result = work.await;
        if result.is_ok() {
            log::debug!("Native request completed successfully");
        } else {
            log::warn!("Native request completed with error");
        }
        result
    })
    .await
    .map_err(|e| format!("Native request task failed: {e}"))?
}
#[cfg(test)]
mod request_owner_tests {
    use super::*;
    #[tokio::test]
    async fn cancelling_waiter_keeps_admitted_core_work_owned_and_bounded() {
        let admissions = Arc::new(tokio::sync::Semaphore::new(1));
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (finish_tx, finish_rx) = tokio::sync::oneshot::channel();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let owner = admissions.clone();
        let waiter = tokio::spawn(async move {
            owned_request(owner, async move {
                let _ = started_tx.send(());
                finish_rx.await.map_err(|e| e.to_string())?;
                let _ = done_tx.send(());
                Ok(Value::Null)
            })
            .await
        });
        started_rx.await.unwrap();
        waiter.abort();
        let _ = waiter.await;
        assert_eq!(admissions.available_permits(), 0);
        assert!(
            owned_request(admissions.clone(), async { Ok(Value::Null) })
                .await
                .is_err()
        );
        finish_tx.send(()).unwrap();
        done_rx.await.unwrap();
        tokio::task::yield_now().await;
        assert_eq!(admissions.available_permits(), 1);
    }
}

impl crate::surfaces::terminal::TerminalTransport for CoreClient {
    fn send(
        &self,
        request: crate::surfaces::terminal::TerminalRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>> {
        let client = self.clone();
        Box::pin(async move { client.request(Request::Terminal(request)).await })
    }
}
