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
    Arc<std::sync::Mutex<Option<tokio::runtime::Handle>>>,
    Arc<tokio::sync::Semaphore>,
    Arc<ExtensionAdmission>,
    tezbar_core::StartupOptions,
);
struct ExtensionAdmission {
    normal: Arc<tokio::sync::Semaphore>,
    installs: std::sync::atomic::AtomicUsize,
    commands: std::sync::atomic::AtomicUsize,
    closing: std::sync::atomic::AtomicBool,
}
struct InstallLease(Arc<ExtensionAdmission>);
struct CommandLease(Arc<ExtensionAdmission>);
impl Drop for CommandLease {
    fn drop(&mut self) {
        self.0
            .commands
            .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}
impl Drop for InstallLease {
    fn drop(&mut self) {
        self.0
            .installs
            .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}
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
    Extension(crate::surfaces::extensions_store::request::ExtensionRequest),
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
            Self::Extension(request) => request.wire(),
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
        if let Request::ExtensionRun(value) = &request {
            use crate::surfaces::extensions_store::request::{
                ExtensionRequest, ExtensionTransport,
            };
            let parsed: Result<ExtensionRequest, &'static str> = (|| {
                Ok(ExtensionRequest::Run {
                    extension: value["extensionId"]
                        .as_str()
                        .ok_or("Missing extension")?
                        .into(),
                    command: value["commandName"]
                        .as_str()
                        .ok_or("Missing command")?
                        .into(),
                    arguments: serde_json::from_value(
                        value
                            .get("argumentValues")
                            .cloned()
                            .unwrap_or_else(|| json!({})),
                    )
                    .map_err(|_| "Invalid command arguments")?,
                })
            })();
            return match parsed {
                Ok(request) => ExtensionTransport::send(self, request),
                Err(error) => Box::pin(async move { Err(error.to_string()) }),
            };
        }
        if let Request::Extension(request) = request {
            return crate::surfaces::extensions_store::request::ExtensionTransport::send(
                self, request,
            );
        }
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
    pub(crate) fn bind_extension_runtime(&self, handle: tokio::runtime::Handle) {
        #[cfg(feature = "bench-control")]
        let owner = &self.5;
        #[cfg(not(feature = "bench-control"))]
        let owner = &self.4;
        *owner.lock().unwrap() = Some(handle);
    }
    #[cfg(test)]
    pub(crate) fn hold_extension_install_for_test(&self) -> impl Drop {
        #[cfg(feature = "bench-control")]
        let admission = &self.7;
        #[cfg(not(feature = "bench-control"))]
        let admission = &self.6;
        admission
            .installs
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        InstallLease(admission.clone())
    }
    pub(crate) fn resume_extensions(&self) {
        #[cfg(feature = "bench-control")]
        let admission = &self.7;
        #[cfg(not(feature = "bench-control"))]
        let admission = &self.6;
        admission
            .closing
            .store(false, std::sync::atomic::Ordering::Release);
    }
    pub fn new(core: Arc<tezbar_core::NativeCore>) -> Self {
        Self::with_startup_options(core, tezbar_core::StartupOptions::ISOLATED_PREVIEW)
    }
    pub(crate) fn for_launch(core: Arc<tezbar_core::NativeCore>, production: bool) -> Self {
        // Keep optional background startup unchanged; only production owns configured shortcuts.
        Self::with_startup_options(
            core,
            tezbar_core::StartupOptions {
                initialize_shortcuts: production,
                ..tezbar_core::StartupOptions::ISOLATED_PREVIEW
            },
        )
    }
    fn with_startup_options(
        core: Arc<tezbar_core::NativeCore>,
        options: tezbar_core::StartupOptions,
    ) -> Self {
        Self(
            core,
            Arc::default(),
            Arc::default(),
            #[cfg(feature = "bench-control")]
            Arc::default(),
            Arc::new(tokio::sync::Semaphore::new(16)),
            Arc::new(std::sync::Mutex::new(
                tokio::runtime::Handle::try_current().ok(),
            )),
            Arc::new(tokio::sync::Semaphore::new(16)),
            Arc::new(ExtensionAdmission {
                normal: Arc::new(tokio::sync::Semaphore::new(16)),
                installs: std::sync::atomic::AtomicUsize::new(0),
                commands: std::sync::atomic::AtomicUsize::new(0),
                closing: std::sync::atomic::AtomicBool::new(false),
            }),
            options,
        )
    }
    pub async fn request(&self, request: Request) -> Result<Value, String> {
        if let Request::Extension(request) = request {
            return crate::surfaces::extensions_store::request::ExtensionTransport::send(
                self, request,
            )
            .await;
        }
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
    fn startup_options(&self) -> tezbar_core::StartupOptions {
        #[cfg(feature = "bench-control")]
        {
            self.8
        }
        #[cfg(not(feature = "bench-control"))]
        {
            self.7
        }
    }
    /// Called on the GUI thread before migration/core readiness can delay optional services.
    /// Registration is idempotent and reads the existing configured accelerator; no fallback.
    pub(crate) fn initialize_shortcuts(&self) -> Result<(), String> {
        if self.startup_options().initialize_shortcuts {
            self.0.platform.initialize_shortcuts()
        } else {
            Ok(())
        }
    }
    pub async fn start(&self) -> Result<(), String> {
        let mut owner = self.2.lock().await;
        if owner.ready {
            return Ok(());
        }
        if owner.task.is_none() {
            let core = self.0.clone();
            let options = self.startup_options();
            owner.task = Some(tokio::spawn(async move {
                let result = core.start_with_options(options).await;
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

impl crate::surfaces::extensions_store::request::ExtensionTransport for CoreClient {
    fn send(
        &self,
        request: crate::surfaces::extensions_store::request::ExtensionRequest,
    ) -> crate::surfaces::extensions_store::request::ExtensionFuture {
        #[cfg(feature = "bench-control")]
        let (admission, runtime, disposal) = (&self.7, &self.5, &self.6);
        #[cfg(not(feature = "bench-control"))]
        let (admission, runtime, disposal) = (&self.6, &self.4, &self.5);
        if admission.closing.load(std::sync::atomic::Ordering::Acquire)
            && !matches!(
                request,
                crate::surfaces::extensions_store::request::ExtensionRequest::Dispose(_)
            )
        {
            return Box::pin(async {
                Err("Tezbar is closing. Cancel quit before running an extension.".into())
            });
        }
        let slots = if matches!(
            request,
            crate::surfaces::extensions_store::request::ExtensionRequest::Dispose(_)
        ) {
            disposal
        } else {
            &admission.normal
        };
        let permit = match slots.clone().try_acquire_owned() {
            Ok(v) => v,
            Err(_) => {
                return Box::pin(async {
                    Err("Native operations are still finishing; retry".into())
                });
            }
        };
        let handle = runtime
            .lock()
            .unwrap()
            .clone()
            .or_else(|| tokio::runtime::Handle::try_current().ok());
        let Some(handle) = handle else {
            return Box::pin(async { Err("Core runtime unavailable".into()) });
        };
        let install = if matches!(
            request,
            crate::surfaces::extensions_store::request::ExtensionRequest::Install(_)
        ) {
            admission
                .installs
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            Some(InstallLease(admission.clone()))
        } else {
            None
        };
        use crate::surfaces::extensions_store::request::ExtensionRequest;
        let command = if matches!(
            &request,
            ExtensionRequest::Run { .. }
                | ExtensionRequest::Invoke { .. }
                | ExtensionRequest::InvokeOwned { .. }
                | ExtensionRequest::SearchText { .. }
                | ExtensionRequest::LoadMore(_)
        ) {
            admission
                .commands
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            Some(CommandLease(admission.clone()))
        } else {
            None
        };
        let (channel, args) = request.wire();
        let core = self.0.clone();
        let task = handle.spawn(async move {
            let _permit = permit;
            let _install = install;
            let _command = command;
            core.request(channel, &args).await
        });
        Box::pin(async move {
            task.await
                .map_err(|_| "Core operation interrupted".to_string())?
        })
    }
    fn drain(&self) -> crate::surfaces::extensions_store::request::ExtensionFuture {
        #[cfg(feature = "bench-control")]
        let (admission, disposal) = (self.7.clone(), self.6.clone());
        #[cfg(not(feature = "bench-control"))]
        let (admission, disposal) = (self.6.clone(), self.5.clone());
        Box::pin(async move {
            admission
                .closing
                .store(true, std::sync::atomic::Ordering::Release);
            if admission
                .installs
                .load(std::sync::atomic::Ordering::Acquire)
                != 0
            {
                return Err(
                    "Extension install in progress. Wait for it to finish, then quit.".into(),
                );
            }
            if admission
                .commands
                .load(std::sync::atomic::Ordering::Acquire)
                != 0
            {
                return Err(
                    "Extension command still running. Wait for it to finish, then quit.".into(),
                );
            }
            let _all = admission
                .normal
                .clone()
                .acquire_many_owned(16)
                .await
                .map_err(|_| "Core admission closed")?;
            let _dispose = disposal
                .acquire_many_owned(16)
                .await
                .map_err(|_| "Disposal admission closed")?;
            Ok(json!(true))
        })
    }
}

#[cfg(test)]
mod m9_admission_tests {
    use super::*;
    use crate::surfaces::extensions_store::request::{ExtensionRequest, ExtensionTransport};
    #[tokio::test]
    async fn extension_admission_precedes_poll_and_drain_owns_dropped_waiters() {
        let root = tempfile::tempdir().unwrap();
        let host = Arc::new(tezbar_core::host::TestHost {
            allow_interaction: true,
            ..Default::default()
        });
        let ctx = tezbar_core::common::CoreContext::new(
            tezbar_core::common::CorePaths {
                data_dir: root.path().join("data"),
                config_dir: root.path().join("config"),
                resource_dir: root.path().join("resources"),
                home_dir: root.path().join("home"),
                temp_dir: root.path().join("tmp"),
            },
            Some(host.clone()),
        );
        let core = tezbar_core::NativeCore::new(ctx).unwrap();
        let client = CoreClient::new(core.clone());
        for _ in 0..16 {
            let future =
                ExtensionTransport::send(&client, ExtensionRequest::Copy("fixture".into()));
            drop(future);
        }
        assert!(
            ExtensionTransport::send(&client, ExtensionRequest::Copy("excess".into()))
                .await
                .is_err()
        );
        let disposal =
            ExtensionTransport::send(&client, ExtensionRequest::Dispose("missing-session".into()));
        drop(disposal);
        core.start_with_options(tezbar_core::StartupOptions::ISOLATED_PREVIEW)
            .await
            .unwrap();
        ExtensionTransport::drain(&client).await.unwrap();
        assert!(
            !host
                .events
                .lock()
                .unwrap()
                .iter()
                .any(|(channel, _)| channel == "extension-worker-status")
        );
        assert_eq!(host.desktop_effects.lock().unwrap().len(), 16);
        assert!(
            ExtensionTransport::send(
                &client,
                ExtensionRequest::Copy("blocked while closing".into())
            )
            .await
            .is_err()
        );
        client.resume_extensions();
        #[cfg(feature = "bench-control")]
        let global = client.4.clone();
        #[cfg(not(feature = "bench-control"))]
        let global = client.3.clone();
        let _unrelated = global.acquire_many_owned(16).await.unwrap();
        ExtensionTransport::send(&client, ExtensionRequest::Copy("independent".into()))
            .await
            .unwrap();
        ExtensionTransport::drain(&client).await.unwrap();
        assert_eq!(host.desktop_effects.lock().unwrap().len(), 17);
        client.resume_extensions();
        core.shutdown().await;
    }
}

#[cfg(test)]
#[path = "core_client_command_tests.rs"]
mod command_tests;

#[cfg(all(test, unix))]
#[path = "preview_terminal_tests.rs"]
mod preview_terminal_tests;
