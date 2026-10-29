// The binary owns helper dispatch. Include privately here for its protocol and
// isolated fixture tests; lib.rs needs no new public export.
#[allow(dead_code)]
#[path = "../../child_supervisor.rs"]
mod child_supervisor;
#[path = "runtime_integrity.rs"]
mod runtime_integrity;
#[path = "runtime_models.rs"]
mod runtime_models;
use super::extract::{self, Cancellation};
use crate::native_core::common::{now_ms, write_json_atomic, CoreContext};
use futures_util::StreamExt;
use runtime_integrity::{ModelIdentity, VerifiedInstall};
pub use runtime_models::{EmbeddingVariantId, ModelDescriptor};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
    sync::{Mutex as AsyncMutex, Notify},
};
// Q8 compatibility constants: existing databases retain this exact identity.
const REVISION: &str = runtime_models::Q8.revision;
const MODEL_FILE: &str = runtime_models::Q8.file;
const MODEL_SHA: &str = runtime_models::Q8.sha256;
pub const MODEL_KEY: &str = runtime_models::Q8.model_key;
#[derive(Clone)]
struct Manifest {
    file: &'static str,
    bytes: u64,
    sha: &'static str,
}
fn platform() -> Option<Manifest> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some(Manifest {
            file: "llama-b11474-bin-macos-arm64.tar.gz",
            bytes: 12011955,
            sha: "ae7f002f2b477d14fea8ac4d93438684344cd18ec10d2b0ee9d5518b2a74def5",
        }),
        ("macos", "x86_64") => Some(Manifest {
            file: "llama-b11474-bin-macos-x64.tar.gz",
            bytes: 11530937,
            sha: "46381da99834f84ead41c78d091c9137755bac27d9423e56ea1fd0bc51104d60",
        }),
        ("windows", "x86_64") => Some(Manifest {
            file: "llama-b11474-bin-win-cpu-x64.zip",
            bytes: 19440991,
            sha: "5a39102a1b27d75ab8a9d777c6cf2ce373c468265dc1797a2566dec085501a95",
        }),
        ("windows", "aarch64") => Some(Manifest {
            file: "llama-b11474-bin-win-cpu-arm64.zip",
            bytes: 12269479,
            sha: "2874f37ef48f12edfab2346f3857d7c866f4d423b2e9da045baf14312201db71",
        }),
        ("linux", "x86_64") => Some(Manifest {
            file: "llama-b11474-bin-ubuntu-x64.tar.gz",
            bytes: 17737147,
            sha: "98abe0fac169db55712ce96b0e30adf99fa47fa5386fddef8fef84fe21f4a690",
        }),
        _ => None,
    }
}
fn server(directory: &Path) -> Option<PathBuf> {
    walkdir::WalkDir::new(directory)
        .max_depth(8)
        .into_iter()
        .filter_map(Result::ok)
        .find(|e| {
            e.file_type().is_file()
                && e.file_name()
                    == if cfg!(windows) {
                        "llama-server.exe"
                    } else {
                        "llama-server"
                    }
        })
        .map(|e| e.into_path())
}
#[derive(Debug, PartialEq)]
pub enum ForegroundEmbedding {
    Ready(Vec<Vec<f64>>),
    Warming,
    Busy,
}

#[derive(Clone)]
pub struct Runtime {
    inner: Arc<Inner>,
    // Background jobs share Inner but never retain this external-owner lease.
    _owner: Option<Arc<RuntimeOwner>>,
}
struct RuntimeOwner {
    closed: Arc<AtomicBool>,
    download_gen: Arc<AtomicU64>,
    runtime_gen: Arc<AtomicU64>,
    startup_task: Arc<Mutex<Option<OwnedTask<()>>>>,
    idle_task: Arc<Mutex<Option<OwnedTask<()>>>>,
}
impl Drop for RuntimeOwner {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::SeqCst);
        self.download_gen.fetch_add(1, Ordering::SeqCst);
        self.runtime_gen.fetch_add(1, Ordering::SeqCst);
        // Abort closes an unpublished supervisor lease as well as an idle one.
        self.startup_task.lock().unwrap().take();
        self.idle_task.lock().unwrap().take();
    }
}
struct Inner {
    state_changed: Notify,
    observer: Mutex<Option<Arc<Notify>>>,
    directory: PathBuf,
    #[cfg(test)]
    supervisor: Mutex<Option<PathBuf>>,
    #[cfg(test)]
    install_source: Mutex<Option<TestInstallSource>>,
    status: Mutex<Value>,
    download_gen: Arc<AtomicU64>,
    download: AsyncMutex<Option<tokio::task::JoinHandle<()>>>,
    process: AsyncMutex<Option<Process>>,
    startup_task: Arc<Mutex<Option<OwnedTask<()>>>>,
    idle_task: Arc<Mutex<Option<OwnedTask<()>>>>,
    runtime_gen: Arc<AtomicU64>,
    stopping: AtomicBool,
    foreground_timeout: std::time::Duration,
    verification: AsyncMutex<()>,
    verification_task: AsyncMutex<Option<tokio::task::JoinHandle<()>>>,
    verified: Mutex<Option<VerifiedInstall>>,
    model: ModelIdentity,
    ready_timeout: std::time::Duration,
    request_timeout: std::time::Duration,
    closed: Arc<AtomicBool>,
    removing: AtomicBool,
    client: reqwest::Client,
}
// Status cleanup on dropped/aborted inference futures; process ownership is
// separately enforced by the supervisor lease, never by a status flag.
struct RuntimeActivity<'a>(Option<&'a Runtime>);
impl RuntimeActivity<'_> {
    fn complete(&mut self) {
        self.0 = None;
    }
}
impl Drop for RuntimeActivity<'_> {
    fn drop(&mut self) {
        if let Some(runtime) = self.0 {
            if runtime.snapshot()["runtimeState"] != "error" {
                runtime.patch(json!({"runtimeState":"stopped"}));
            }
        }
    }
}
// Only unit tests route downloads to loopback archives; production always uses
// pinned publisher URLs/checksums and the platform runtime catalog.
#[cfg(test)]
#[derive(Clone)]
struct TestInstallSource {
    runtime: Manifest,
    runtime_url: String,
    model_url: String,
}
struct Process {
    child: Child,
    endpoint: String,
    token: String,
    last_used: u64,
    threads: usize,
    _stderr_task: Option<OwnedTask<()>>,
}
// The handle, including the stderr drainer, belongs to a runtime operation.
// Dropping/aborting its owner cannot detach an unbounded task.
struct OwnedTask<T>(Option<tokio::task::JoinHandle<T>>);
impl<T> OwnedTask<T> {
    fn finished(&self) -> bool {
        self.0.as_ref().map(|t| t.is_finished()).unwrap_or(true)
    }
    fn abort(mut self) -> tokio::task::JoinHandle<T> {
        let task = self.0.take().unwrap();
        task.abort();
        task
    }
}
impl<T> Drop for OwnedTask<T> {
    fn drop(&mut self) {
        if let Some(task) = &self.0 {
            task.abort();
        }
    }
}
struct Stopping<'a>(&'a AtomicBool);
impl Drop for Stopping<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
fn threads(speed: &str) -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(match speed {
            "slow" => 2,
            "fast" => 8,
            _ => 4,
        })
}
impl Runtime {
    pub fn new(ctx: CoreContext) -> Self {
        Self::new_for_variant(ctx, EmbeddingVariantId::default())
    }
    pub fn new_for_variant(ctx: CoreContext, variant: EmbeddingVariantId) -> Self {
        let descriptor = variant.descriptor();
        let total = descriptor.bytes + platform().map(|p| p.bytes).unwrap_or(0);
        let directory = ctx
            .paths
            .data_dir
            .join("knowledge/models")
            .join(descriptor.directory);
        let closed = Arc::new(AtomicBool::new(false));
        let download_gen = Arc::new(AtomicU64::new(0));
        let runtime_gen = Arc::new(AtomicU64::new(0));
        let startup_task = Arc::new(Mutex::new(None));
        let idle_task = Arc::new(Mutex::new(None));
        let owner = RuntimeOwner {
            closed: closed.clone(),
            download_gen: download_gen.clone(),
            runtime_gen: runtime_gen.clone(),
            startup_task: startup_task.clone(),
            idle_task: idle_task.clone(),
        };
        Self {
            _owner: Some(Arc::new(owner)),
            inner: Arc::new(Inner {
                state_changed: Notify::new(),
                observer: Mutex::new(None),
                directory,
                #[cfg(test)]
                supervisor: Mutex::new(None),
                #[cfg(test)]
                install_source: Mutex::new(None),
                status: Mutex::new(
                    json!({"weightBytes":descriptor.bytes,"supported":platform().is_some(),"modelState":"not-downloaded","integrityState":"unchecked","runtimeState":"stopped","downloadProgress":0,"downloadBytes":0,"downloadTotalBytes":total,"indexing":false,"indexedChunks":0,"totalChunks":0,"error":null}),
                ),
                download_gen,
                download: AsyncMutex::new(None),
                process: AsyncMutex::new(None),
                startup_task,
                idle_task,
                runtime_gen,
                stopping: AtomicBool::new(false),
                foreground_timeout: std::time::Duration::from_secs(1),
                verification: AsyncMutex::new(()),
                verification_task: AsyncMutex::new(None),
                verified: Mutex::new(None),
                model: ModelIdentity {
                    descriptor,
                    bytes: descriptor.bytes,
                    sha: descriptor.sha256,
                },
                ready_timeout: std::time::Duration::from_secs(60),
                request_timeout: std::time::Duration::from_secs(60),
                closed,
                removing: AtomicBool::new(false),
                client: reqwest::Client::new(),
            }),
        }
    }
    fn worker(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            _owner: None,
        }
    }
    pub fn variant_id(&self) -> EmbeddingVariantId {
        self.inner.model.descriptor.id
    }
    pub fn model_key(&self) -> &'static str {
        self.inner.model.descriptor.model_key
    }
    pub fn weight_bytes(&self) -> u64 {
        self.inner.model.descriptor.bytes
    }
    /// Additive shared SemanticVariantStatus; legacy snapshot() remains available.
    pub fn snapshot_variant(&self) -> Value {
        let status = self.snapshot();
        let state = if status["integrityState"] == "verifying" {
            json!("verifying")
        } else {
            status["modelState"].clone()
        };
        let mut result = json!({"id":self.variant_id(),"weightBytes":self.weight_bytes(),"state":state,
            "downloadProgress":status["downloadProgress"],"downloadBytes":status["downloadBytes"],"downloadTotalBytes":status["downloadTotalBytes"]});
        if let Some(message) = status["error"].as_str() {
            let stage = match status["errorSource"].as_str() {
                Some("download") => "download",
                Some("integrity") => "integrity",
                _ => "startup",
            };
            result["failure"] = json!({"stage":stage,"message":message});
        }
        result
    }
    /// Schedules one bounded, owned startup, independent of query cancellation.
    /// Explicitly call this again to retry a published startup error.
    pub async fn prewarm(&self, speed: &str) -> Result<(), String> {
        if self.unavailable() {
            return Err("Embedding runtime is stopping".into());
        }
        let status = self.snapshot();
        match status["integrityState"].as_str() {
            Some("unchecked") => {
                self.initialize().await?;
                return Ok(());
            }
            Some("verifying") => return Ok(()),
            Some("verified") if status["modelState"] == "ready" => {}
            _ => {
                return Err(status["error"]
                    .as_str()
                    .unwrap_or("Download EmbeddingGemma before enabling it")
                    .into())
            }
        }
        let mut task = self.inner.startup_task.lock().unwrap();
        if self.unavailable() {
            return Err("Embedding runtime is stopping".into());
        }
        if task.as_ref().map(|t| !t.finished()).unwrap_or(false) {
            return Ok(());
        }
        let expected = self.inner.runtime_gen.load(Ordering::SeqCst);
        let cancel = Cancellation {
            generation: self.inner.runtime_gen.clone(),
            expected,
        };
        if status["runtimeState"] != "ready" {
            self.patch(json!({"runtimeState":"starting"}));
        }
        let this = self.worker();
        let speed = speed.to_owned();
        *task = Some(OwnedTask(Some(tokio::spawn(async move {
            let run = async {
                let mut guard = tokio::select! {
                    guard = this.inner.process.lock() => guard,
                    error = this.interrupted(&cancel, expected) => return Err(error),
                };
                let mut activity = RuntimeActivity(Some(&this));
                let result = tokio::select! {
                    r = this.ensure_process(&mut guard, &speed, &cancel, expected) => r,
                    error = this.interrupted(&cancel, expected) => Err(error),
                };
                if result.is_ok() {
                    activity.complete();
                }
                result
            };
            let result = tokio::time::timeout(this.inner.ready_timeout, run)
                .await
                .unwrap_or_else(|_| Err("Local embedding runtime took too long to start".into()));
            this.publish_runtime_result(&result, &cancel, expected);
        }))));
        Ok(())
    }
    /// Foreground search can fall back immediately while cold or while indexing
    /// owns the process. Only an already-warm request spends the one-second budget.
    pub async fn foreground_embed(
        &self,
        inputs: Vec<String>,
        speed: &str,
        cancel: &Cancellation,
    ) -> Result<ForegroundEmbedding, String> {
        cancel.check()?;
        if self.unavailable() {
            return Err("Embedding runtime is stopping".into());
        }
        if inputs.is_empty() {
            return Ok(ForegroundEmbedding::Ready(vec![]));
        }
        let expected = self.inner.runtime_gen.load(Ordering::SeqCst);
        let result = tokio::time::timeout(
            self.inner.foreground_timeout,
            self.foreground_inner(inputs, speed, cancel, expected),
        )
        .await
        .unwrap_or_else(|_| Err("Foreground embedding request timed out".into()));
        // Warming/Busy are ordinary keyword fallback, never startup failures.
        if matches!(&result, Ok(ForegroundEmbedding::Ready(_)))
            || (result.is_err() && self.snapshot()["modelState"] == "ready")
        {
            self.publish_runtime_result(&result, cancel, expected);
        }
        result
    }
    async fn foreground_inner(
        &self,
        inputs: Vec<String>,
        speed: &str,
        cancel: &Cancellation,
        expected: u64,
    ) -> Result<ForegroundEmbedding, String> {
        let status = self.snapshot();
        if status["integrityState"] == "verifying" {
            return Ok(ForegroundEmbedding::Warming);
        }
        if status["integrityState"] == "unchecked" {
            self.initialize().await?;
            return Ok(ForegroundEmbedding::Warming);
        }
        if status["modelState"] == "downloading" {
            return Ok(ForegroundEmbedding::Busy);
        }
        if status["modelState"] != "ready" {
            return Err(status["error"]
                .as_str()
                .unwrap_or("Download EmbeddingGemma before enabling it")
                .into());
        }
        if status["runtimeState"] == "error" {
            return Err(status["error"]
                .as_str()
                .unwrap_or("Embedding runtime needs a startup retry")
                .into());
        }
        let mut guard = match self.inner.process.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                return Ok(if status["runtimeState"] == "starting" {
                    ForegroundEmbedding::Warming
                } else {
                    ForegroundEmbedding::Busy
                })
            }
        };
        if guard
            .as_ref()
            .map(|p| p.threads != threads(speed))
            .unwrap_or(true)
        {
            drop(guard);
            self.prewarm(speed).await?;
            return Ok(ForegroundEmbedding::Warming);
        }
        let mut activity = RuntimeActivity(Some(self));
        // Metadata-only cache check: changed bytes require background verification,
        // never a foreground 300 MB checksum or an unauthorized stale launch.
        let inner = self.inner.clone();
        let cached = inner.verified.lock().unwrap().clone();
        let valid = tokio::task::spawn_blocking(move || {
            platform().and_then(|p| {
                runtime_integrity::cached_install(
                    &inner.directory,
                    &p,
                    inner.model,
                    cached.as_ref(),
                )
            })
        })
        .await
        .map_err(|e| e.to_string())?;
        if valid.is_none() {
            drop(guard.take()); // Closing the supervisor lease cleans the old tree.
            drop(guard);
            self.initialize().await?;
            return Ok(ForegroundEmbedding::Warming);
        }
        self.check_runtime(cancel, expected)?;
        let exited = match guard.as_mut().unwrap().child.try_wait() {
            Ok(status) => status.is_some(),
            Err(error) => {
                drop(guard.take());
                return Err(format!("Cannot inspect owned embedding runtime: {error}"));
            }
        };
        if exited {
            drop(guard.take());
            return Err("Local embedding runtime exited; retry startup".into());
        }
        let result = self.request_with_process(
            &mut guard,
            inputs,
            cancel,
            expected,
            self.inner.foreground_timeout,
        )
        .await
        .map(ForegroundEmbedding::Ready);
        if result.is_ok() {
            activity.complete();
        }
        result
    }
    fn unavailable(&self) -> bool {
        self.inner.closed.load(Ordering::SeqCst)
            || self.inner.removing.load(Ordering::SeqCst)
            || self.inner.stopping.load(Ordering::SeqCst)
    }
    fn check_runtime(&self, cancel: &Cancellation, expected: u64) -> Result<(), String> {
        cancel.check()?;
        if self.unavailable() || self.inner.runtime_gen.load(Ordering::SeqCst) != expected {
            return Err("Embedding runtime is stopping".into());
        }
        Ok(())
    }
    fn publish_runtime_result<T>(
        &self,
        result: &Result<T, String>,
        cancel: &Cancellation,
        expected: u64,
    ) {
        if self.check_runtime(cancel, expected).is_err() {
            return;
        }
        match result {
            Ok(_) => self.patch(json!({"error":null,"errorSource":null,"runtimeState":"ready"})),
            Err(error) => {
                let source = if self.snapshot()["integrityState"] == "invalid" {
                    "integrity"
                } else {
                    "runtime"
                };
                self.patch(json!({"error":error,"errorSource":source,"runtimeState":"error"}));
            }
        }
    }
    pub fn snapshot(&self) -> Value {
        self.inner.status.lock().unwrap().clone()
    }
    pub fn downloading(&self) -> bool {
        self.inner.status.lock().unwrap()["modelState"] == "downloading"
    }
    pub fn observe(&self, observer: Arc<Notify>) {
        *self.inner.observer.lock().unwrap() = Some(observer);
    }
    pub fn patch(&self, patch: Value) {
        let mut status = self.inner.status.lock().unwrap();
        let mut changed = false;
        if let Some(fields) = patch.as_object() {
            for (k, v) in fields {
                if status.get(k) != Some(v) {
                    status[k] = v.clone();
                    changed = true;
                }
            }
        }
        drop(status);
        if changed {
            self.inner.state_changed.notify_waiters();
            if let Some(observer) = self.inner.observer.lock().unwrap().as_ref() {
                observer.notify_waiters();
            }
        }
    }
    pub async fn initialize(&self) -> Result<(), String> {
        // Snapshot/Settings initialization must never wait for a disk checksum.
        // Until verification completes no caller is allowed to launch a model.
        let mut task = self.inner.verification_task.lock().await;
        if task.as_ref().map(|t| !t.is_finished()).unwrap_or(false) {
            return Ok(());
        }
        if self.inner.closed.load(Ordering::SeqCst) {
            return Ok(());
        }
        if self.snapshot()["modelState"] == "downloading" {
            return Ok(());
        }
        self.patch(json!({"integrityState":"verifying"}));
        let expected = self.inner.download_gen.load(Ordering::SeqCst);
        let this = self.worker();
        *task = Some(tokio::spawn(async move {
            let _ = this.verify_installed(expected).await;
        }));
        Ok(())
    }
    async fn verify_installed(&self, expected: u64) -> Result<VerifiedInstall, String> {
        self.verify_installed_checked(expected, None).await
    }
    async fn verify_installed_checked(
        &self,
        expected: u64,
        runtime_epoch: Option<u64>,
    ) -> Result<VerifiedInstall, String> {
        let _verification = self.inner.verification.lock().await;
        let inner = self.inner.clone();
        let cached = inner.verified.lock().unwrap().clone();
        let result = tokio::task::spawn_blocking(move || {
            let check = || {
                if inner.closed.load(Ordering::SeqCst)
                    || inner.download_gen.load(Ordering::SeqCst) != expected
                    || runtime_epoch
                        .map(|epoch| inner.runtime_gen.load(Ordering::SeqCst) != epoch)
                        .unwrap_or(false)
                {
                    Err("Embedding verification cancelled".to_string())
                } else {
                    Ok(())
                }
            };
            match platform() {
                Some(runtime) => runtime_integrity::verify(
                    &inner.directory,
                    &runtime,
                    inner.model,
                    cached.as_ref(),
                    &check,
                ),
                None => Ok(None),
            }
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|v| v);
        if self.inner.closed.load(Ordering::SeqCst)
            || self.inner.download_gen.load(Ordering::SeqCst) != expected
            || runtime_epoch
                .map(|epoch| self.inner.runtime_gen.load(Ordering::SeqCst) != epoch)
                .unwrap_or(false)
        {
            return Err("Embedding verification cancelled".into());
        }
        match result {
            Ok(Some(verified)) => {
                *self.inner.verified.lock().unwrap() = Some(verified.clone());
                let total = self.snapshot()["downloadTotalBytes"].as_u64().unwrap_or(0);
                let repaired = self.snapshot()["errorSource"] == "integrity";
                let downloading = self.snapshot()["modelState"] == "downloading";
                self.patch(json!({"modelState":if downloading {"downloading"} else {"ready"},"integrityState":"verified",
                    "runtimeIntegrity":if verified.legacy {"legacy-local-baseline"} else {"installed-file-sha256"},
                    "modelFile":self.inner.model.descriptor.file,"modelSha256":self.inner.model.sha,"modelKey":self.model_key(),
                    "downloadProgress":1,"downloadBytes":total}));
                if repaired {
                    self.patch(json!({"error":null,"errorSource":null,"runtimeState":"stopped"}));
                }
                Ok(verified)
            }
            Ok(None) => {
                self.patch(json!({"modelState":"not-downloaded","integrityState":"missing","runtimeState":"stopped","error":null,"errorSource":null,"downloadProgress":0,"downloadBytes":0}));
                Err("Download EmbeddingGemma before enabling it".into())
            }
            Err(error) => {
                let error = format!("EmbeddingGemma installation is damaged or incompatible: {error}. Download again to repair.");
                let downloading = self.snapshot()["modelState"] == "downloading";
                self.patch(json!({"modelState":if downloading {"downloading"} else {"error"},"integrityState":"invalid","runtimeState":"error","error":error,"errorSource":"integrity"}));
                Err(error)
            }
        }
    }
    pub async fn download(&self) -> Result<(), String> {
        if self.inner.closed.load(Ordering::SeqCst) || self.inner.removing.load(Ordering::SeqCst) {
            return Err("Embedding runtime is shutting down".into());
        }
        let mut task = self.inner.download.lock().await;
        if self.inner.closed.load(Ordering::SeqCst) || self.inner.removing.load(Ordering::SeqCst) {
            return Err("Embedding runtime is shutting down".into());
        }
        if task.as_ref().map(|t| !t.is_finished()).unwrap_or(false) {
            return Ok(());
        }
        let status = self.snapshot();
        // An explicit Download/Repair cannot replace an independent startup file
        // check. Wait for that check to publish ready/error before deciding.
        if status["integrityState"] == "verifying" {
            return Ok(());
        }
        // Repair uses this same API. A published startup failure deliberately
        // bypasses the healthy-ready no-op and stages a fresh pinned installation.
        if status["modelState"] == "ready"
            && status["errorSource"] != "runtime"
            && self
                .verify_installed(self.inner.download_gen.load(Ordering::SeqCst))
                .await
                .is_ok()
        {
            return Ok(());
        }
        let manifest = platform().ok_or("EmbeddingGemma is not available for this platform yet")?;
        if self.inner.closed.load(Ordering::SeqCst) || self.inner.removing.load(Ordering::SeqCst) {
            return Err("Embedding runtime is shutting down".into());
        }
        let expected = self.inner.download_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let cancel = Cancellation {
            generation: self.inner.download_gen.clone(),
            expected,
        };
        self.patch(
            json!({"modelState":"downloading","integrityState":"unchecked","downloadBytes":0,"downloadProgress":0,"error":null,"errorSource":null}),
        );
        let this = self.worker();
        *task = Some(tokio::spawn(async move {
            let result = this.install(manifest, &cancel).await;
            if let Err(error) = result {
                let cancelled = cancel.check().is_err();
                let integrity = matches!(
                    this.snapshot()["integrityState"].as_str(),
                    Some("verifying" | "invalid")
                );
                this.patch(json!({"modelState":if cancelled{"not-downloaded"}else{"error"},"integrityState":if cancelled {"unchecked"} else if integrity {"invalid"} else {"unchecked"},"error":if cancelled{Value::Null}else{json!(error)},"errorSource":if cancelled{Value::Null}else if integrity {json!("integrity")} else {json!("download")}}));
            }
        }));
        Ok(())
    }
    async fn install(&self, manifest: Manifest, cancel: &Cancellation) -> Result<(), String> {
        #[cfg(test)]
        {
            let source = self.inner.install_source.lock().unwrap().clone();
            if let Some(source) = source {
                return self
                    .install_from(
                        source.runtime,
                        &source.runtime_url,
                        &source.model_url,
                        cancel,
                    )
                    .await;
            }
        }
        let runtime_url = format!(
            "https://github.com/ggml-org/llama.cpp/releases/download/b11474/{}",
            manifest.file
        );
        let model_url = self.inner.model.descriptor.url();
        self.install_from(manifest, &runtime_url, &model_url, cancel)
            .await
    }
    async fn install_from(
        &self,
        manifest: Manifest,
        runtime_url: &str,
        model_url: &str,
        cancel: &Cancellation,
    ) -> Result<(), String> {
        let parent = self
            .inner
            .directory
            .parent()
            .ok_or("Model directory has no parent")?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
        // Never extract or download over a usable installation. TempDir also
        // removes staging on cancellation, panic, or an aborted installer future.
        let staging = tempfile::Builder::new()
            .prefix(".embeddinggemma-install-")
            .tempdir_in(parent)
            .map_err(|e| e.to_string())?;
        let directory = staging.path();
        let archive = directory.join(manifest.file);
        self.download_file(runtime_url, &archive, manifest.sha, manifest.bytes, cancel)
            .await?;
        let runtime = directory.join("runtime");
        tokio::fs::create_dir_all(&runtime)
            .await
            .map_err(|e| e.to_string())?;
        let cmd = if manifest.file.ends_with(".zip") {
            let mut c = Command::new("powershell.exe");
            c.args(["-NoLogo","-NoProfile","-NonInteractive","-Command","Expand-Archive -LiteralPath $env:TEZBAR_GEMMA_ARCHIVE -DestinationPath $env:TEZBAR_GEMMA_DESTINATION -Force"]).env("TEZBAR_GEMMA_ARCHIVE",&archive).env("TEZBAR_GEMMA_DESTINATION",&runtime);
            c
        } else {
            let mut c = Command::new("tar");
            c.arg("-xzf").arg(&archive).arg("-C").arg(&runtime);
            c
        };
        // The archive was checksum verified before extraction; no JS supervisor participates.
        extract::command(cmd, cancel).await?;
        let search = runtime.clone();
        let executable = tokio::task::spawn_blocking(move || server(&search))
            .await
            .map_err(|e| e.to_string())?
            .ok_or("Downloaded runtime does not contain llama-server")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755))
                .await
                .map_err(|e| e.to_string())?;
        }
        let _ = tokio::fs::remove_file(&archive).await;
        self.download_file(
            model_url,
            &directory.join(self.inner.model.descriptor.file),
            self.inner.model.sha,
            self.inner.model.bytes,
            cancel,
        )
        .await?;
        cancel.check()?;
        self.patch(json!({"integrityState":"verifying"}));
        let path = directory.join("installed.json");
        let runtime_dir = runtime.clone();
        let cancel_hash = cancel.clone();
        let files = tokio::task::spawn_blocking(move || {
            runtime_integrity::runtime_files(&runtime_dir, &|| cancel_hash.check())
        })
        .await
        .map_err(|e| e.to_string())??;
        let value = runtime_integrity::manifest(&manifest, self.inner.model, files);
        tokio::task::spawn_blocking(move || write_json_atomic(&path, &value))
            .await
            .map_err(|e| e.to_string())??;
        self.publish_install(directory, cancel).await?;
        self.verify_installed(cancel.expected).await?;
        self.patch(json!({"modelState":"ready","downloadProgress":1,"error":null,"errorSource":null,"runtimeState":"stopped"}));
        Ok(())
    }
    async fn publish_install(&self, staging: &Path, cancel: &Cancellation) -> Result<(), String> {
        self.stop_process().await?;
        // Same lock order as inference: process, then verification. No active
        // server may retain libraries while the directory is swapped (Windows).
        let mut process = self.inner.process.lock().await;
        let _verification = self.inner.verification.lock().await;
        if let Some(mut process) = process.take() {
            self.close_child(&mut process.child).await?;
        }
        cancel.check()?;
        let directory = self.inner.directory.clone();
        let staging = staging.to_owned();
        let model = self.inner.model;
        let cancel = cancel.clone();
        tokio::task::spawn_blocking(move || {
            let manifest = platform().ok_or("Unsupported runtime")?;
            runtime_integrity::verify(&staging, &manifest, model, None, &|| cancel.check())?
                .ok_or("Staged installation is missing")?;
            cancel.check()?;
            let backup = directory.with_extension(format!("backup-{}", uuid::Uuid::new_v4()));
            let had_old = directory.exists();
            if had_old {
                std::fs::rename(&directory, &backup).map_err(|e| e.to_string())?;
            }
            if let Err(error) = std::fs::rename(&staging, &directory) {
                if had_old {
                    std::fs::rename(&backup, &directory).map_err(|restore| {
                        format!(
                            "Install failed: {error}; restoring {} failed: {restore}",
                            backup.display()
                        )
                    })?;
                }
                return Err(error.to_string());
            }
            if had_old {
                let _ = std::fs::remove_dir_all(backup);
            }
            Ok::<_, String>(())
        })
        .await
        .map_err(|e| e.to_string())??;
        *self.inner.verified.lock().unwrap() = None;
        Ok(())
    }
    async fn download_file(
        &self,
        url: &str,
        path: &Path,
        sha: &str,
        expected: u64,
        cancel: &Cancellation,
    ) -> Result<(), String> {
        let part = path.with_extension(format!(
            "{}.part",
            path.extension().unwrap_or_default().to_string_lossy()
        ));
        let run = async {
            let response = self
                .inner
                .client
                .get(url)
                .timeout(std::time::Duration::from_secs(1800))
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?;
            let mut stream = response.bytes_stream();
            let mut file = tokio::fs::File::create(&part)
                .await
                .map_err(|e| e.to_string())?;
            let mut size = 0;
            let mut hash = Sha256::new();
            while let Some(bytes) = stream.next().await {
                cancel.check()?;
                let bytes = bytes.map_err(|e| e.to_string())?;
                size += bytes.len() as u64;
                if size > expected {
                    return Err("Downloaded file exceeds its expected size".into());
                }
                hash.update(&bytes);
                file.write_all(&bytes).await.map_err(|e| e.to_string())?;
                let mut status = self.inner.status.lock().unwrap();
                let downloaded = status["downloadBytes"].as_u64().unwrap_or(0) + bytes.len() as u64;
                status["downloadBytes"] = json!(downloaded);
                status["downloadProgress"] =
                    json!(downloaded as f64 / status["downloadTotalBytes"].as_f64().unwrap_or(1.));
            }
            if size != expected || format!("{:x}", hash.finalize()) != sha {
                return Err("Downloaded file failed checksum verification".into());
            }
            file.sync_all().await.map_err(|e| e.to_string())?;
            drop(file);
            cancel.check()?;
            tokio::fs::rename(&part, path)
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        };
        let cancelled = async {
            loop {
                cancel.check()?;
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        };
        let result = tokio::select! {r=run=>r,r=cancelled=>r};
        let _ = tokio::fs::remove_file(&part).await;
        result
    }
    pub async fn cancel_download(&self) -> Result<(), String> {
        // Serialize cancellation/restoration with a following re-download; an
        // old task must not overwrite the next operation's snapshot.
        let mut download = self.inner.download.lock().await;
        let task = download.take();
        let was_downloading = task.as_ref().map(|t| !t.is_finished()).unwrap_or(false);
        // Reopen verification is independent from an installer. Cancel download
        // neither cancels that check nor invents a missing/ready installation.
        if was_downloading {
            self.inner.download_gen.fetch_add(1, Ordering::SeqCst);
        }
        if let Some(task) = task {
            task.await.map_err(|e| e.to_string())?;
        }
        if was_downloading {
            // A cancelled staged install must not delete the previous model.
            if self.snapshot()["modelState"] == "downloading" {
                self.patch(json!({"modelState":"not-downloaded"}));
            }
            let _ = self
                .verify_installed(self.inner.download_gen.load(Ordering::SeqCst))
                .await;
            if self.snapshot()["integrityState"] != "invalid" {
                self.patch(json!({"error":null}));
            }
        }
        Ok(())
    }
    pub async fn remove(&self) -> Result<(), String> {
        self.inner.removing.store(true, Ordering::SeqCst);
        self.inner.download_gen.fetch_add(1, Ordering::SeqCst);
        let result = async {
            self.cancel_download().await?;
            self.stop_process().await?;
            let _verification = self.inner.verification.lock().await;
            if tokio::fs::try_exists(&self.inner.directory).await.map_err(|e| e.to_string())? {
                tokio::fs::remove_dir_all(&self.inner.directory).await.map_err(|e| e.to_string())?;
            }
            *self.inner.verified.lock().unwrap() = None;
            self.patch(json!({"modelState":"not-downloaded","integrityState":"missing","runtimeState":"stopped","downloadBytes":0,"downloadProgress":0,"indexedChunks":0,"totalChunks":0,"error":null,"errorSource":null}));
            Ok(())
        }.await;
        self.inner.removing.store(false, Ordering::SeqCst);
        result
    }
    async fn close_child(&self, child: &mut Child) -> Result<(), String> {
        // Do not kill the supervisor: it owns descendant cleanup. Closing its
        // lease works even if this wait or its caller is subsequently aborted.
        drop(child.stdin.take());
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait())
            .await
            .map_err(|_| "Owned embedding runtime did not stop in time".to_string())
            .and_then(|r| {
                r.map(|_| ())
                    .map_err(|e| format!("Cannot reap owned embedding runtime: {e}"))
            });
        if let Err(error) = &result {
            self.patch(json!({"error":error,"errorSource":"runtime","runtimeState":"error"}));
        }
        result
    }
    async fn stop_process(&self) -> Result<(), String> {
        self.inner.stopping.store(true, Ordering::SeqCst);
        let _stopping = Stopping(&self.inner.stopping);
        self.inner.runtime_gen.fetch_add(1, Ordering::SeqCst);
        let task = self.inner.startup_task.lock().unwrap().take();
        if let Some(task) = task {
            let _ = task.abort().await;
        }
        if let Some(mut p) = self.inner.process.lock().await.take() {
            self.close_child(&mut p.child).await?;
        }
        self.patch(json!({"runtimeState":"stopped"}));
        Ok(())
    }
    pub async fn stop(&self) {
        let _ = self.stop_process().await;
    }
    pub fn start_idle_supervisor(&self) {
        let mut task = self.inner.idle_task.lock().unwrap();
        if self.inner.closed.load(Ordering::SeqCst) || task.is_some() {
            return;
        }
        // No external-owner lease: last-owner drop aborts this task. Shutdown
        // also aborts and joins it, including while child cleanup is awaited.
        let this = self.worker();
        *task = Some(OwnedTask(Some(tokio::spawn(async move {
            loop {
                let changed = this.inner.state_changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if this.inner.closed.load(Ordering::SeqCst) {
                    break;
                }
                this.stop_idle().await;
                let delay = match this.inner.process.try_lock() {
                    Ok(process) => process.as_ref().map(|p| {
                        // Keep the existing 120s policy. Inference updates
                        // last_used and owns this mutex, so no active request
                        // can be unloaded by this observer.
                        std::time::Duration::from_millis(120001u64.saturating_sub(now_ms().saturating_sub(p.last_used)))
                    }),
                    Err(_) => Some(std::time::Duration::from_secs(1)),
                };
                match delay {
                    Some(delay) => tokio::select! { _ = &mut changed => {}, _ = tokio::time::sleep(delay) => {} },
                    None => changed.await,
                }
            }
        }))));
    }
    pub async fn stop_idle(&self) {
        // Startup and inference own this mutex, with the process unpublished
        // during requests. Never queue an idle decision behind that activity.
        let Ok(mut process) = self.inner.process.try_lock() else {
            return;
        };
        if process
            .as_ref()
            .map(|p| now_ms().saturating_sub(p.last_used) > 120000)
            .unwrap_or(false)
        {
            if let Some(mut p) = process.take() {
                if self.close_child(&mut p.child).await.is_ok() {
                    self.patch(json!({"runtimeState":"stopped"}));
                }
            }
        }
    }
    pub async fn shutdown(&self) {
        self.inner.closed.store(true, Ordering::SeqCst);
        let idle_task = self.inner.idle_task.lock().unwrap().take();
        // Abort synchronously, but do not yield to join before closing the
        // cached process lease: this shutdown future itself may be dropped.
        let idle_join = idle_task.map(OwnedTask::abort);
        self.inner.download_gen.fetch_add(1, Ordering::SeqCst);
        // Release an idle lease before waiting for any installer. If this future
        // is aborted, active inference/verification sees closed and releases its
        // own lease; an already-taken Process drops its closed pipe on abort.
        self.stop().await;
        if let Some(task) = idle_join {
            let _ = task.await;
        }
        let _ = self.cancel_download().await;
    }
    pub async fn embed(
        &self,
        inputs: Vec<String>,
        speed: &str,
        cancel: &Cancellation,
    ) -> Result<Vec<Vec<f64>>, String> {
        if inputs.is_empty() {
            return Ok(vec![]);
        }
        if self.snapshot()["integrityState"] == "verifying" {
            // Core search already has a keyword fallback. Do not queue it behind
            // a startup checksum; verification publishes its own result/error.
            return Err(
                "EmbeddingGemma integrity verification is in progress; keyword search is available"
                    .into(),
            );
        }
        let expected = self.inner.runtime_gen.load(Ordering::SeqCst);
        let result = self.embed_inner(inputs, speed, cancel, expected).await;
        self.publish_runtime_result(&result, cancel, expected);
        result
    }
    async fn interrupted(&self, cancel: &Cancellation, expected: u64) -> String {
        loop {
            if let Err(error) = self.check_runtime(cancel, expected) {
                return error;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }
    async fn embed_inner(
        &self,
        inputs: Vec<String>,
        speed: &str,
        cancel: &Cancellation,
        expected: u64,
    ) -> Result<Vec<Vec<f64>>, String> {
        self.check_runtime(cancel, expected)?;
        let mut guard = tokio::select! {
            guard = self.inner.process.lock() => guard,
            error = self.interrupted(cancel, expected) => return Err(error),
        };
        let mut activity = RuntimeActivity(Some(self));
        tokio::select! {
            r = self.ensure_process(&mut guard, speed, cancel, expected) => r?,
            error = self.interrupted(cancel, expected) => return Err(error),
        }
        let result = self.request_with_process(
            &mut guard,
            inputs,
            cancel,
            expected,
            self.inner.request_timeout,
        )
        .await;
        if result.is_ok() {
            activity.complete();
        }
        result
    }
    async fn ensure_process(
        &self,
        guard: &mut Option<Process>,
        speed: &str,
        cancel: &Cancellation,
        expected: u64,
    ) -> Result<(), String> {
        // Unpublish the cached lease while preparing: cancellation/timeout/drop
        // must close it even when re-verification or a profile change is pending.
        let mut owned = guard.take();
        self.prepare_process(&mut owned, speed, cancel, expected)
            .await?;
        *guard = owned;
        Ok(())
    }
    async fn prepare_process(
        &self,
        guard: &mut Option<Process>,
        speed: &str,
        cancel: &Cancellation,
        expected: u64,
    ) -> Result<(), String> {
        self.check_runtime(cancel, expected)?;
        // Re-stat on use, hash only if changed. Even a same-length replacement
        // cannot inherit a previously verified identity from another file.
        let verified = match self
            .verify_installed_checked(
                self.inner.download_gen.load(Ordering::SeqCst),
                Some(expected),
            )
            .await
        {
            Ok(verified) => verified,
            Err(error) => {
                if let Some(mut p) = guard.take() {
                    if let Err(cleanup) = self.close_child(&mut p.child).await {
                        return Err(format!("{error}; {cleanup}"));
                    }
                }
                return Err(error);
            }
        };
        self.check_runtime(cancel, expected)?;
        let threads = threads(speed);
        if let Some(p) = guard.as_mut() {
            let exited = match p.child.try_wait() {
                Ok(status) => status.is_some(),
                Err(error) => {
                    let mut p = guard.take().unwrap();
                    let _ = self.close_child(&mut p.child).await;
                    return Err(format!("Cannot inspect owned embedding runtime: {error}"));
                }
            };
            if p.threads != threads || exited {
                let mut p = guard.take().unwrap();
                self.close_child(&mut p.child).await?;
            }
        }
        if guard.is_none() {
            self.patch(json!({"runtimeState":"starting"}));
            *guard = Some(
                self.launch(threads, &verified.executable, cancel, expected)
                    .await?,
            );
        }
        self.patch(json!({"runtimeState":"ready"}));
        Ok(())
    }
    async fn request_with_process(
        &self,
        guard: &mut Option<Process>,
        inputs: Vec<String>,
        cancel: &Cancellation,
        expected: u64,
        timeout: std::time::Duration,
    ) -> Result<Vec<Vec<f64>>, String> {
        self.check_runtime(cancel, expected)?;
        // A dropped foreground/indexing future closes only its own active lease.
        let mut p = guard.take().unwrap();
        p.last_used = now_ms();
        let request = async {
            let response = self
                .inner
                .client
                .post(format!("{}/v1/embeddings", p.endpoint))
                .bearer_auth(&p.token)
                .json(&json!({"input":inputs,"encoding_format":"float"}))
                .timeout(timeout)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?;
            let result: Value = response.json().await.map_err(|e| e.to_string())?;
            let mut data = result["data"]
                .as_array()
                .ok_or("Local embedding response is incomplete")?
                .clone();
            data.sort_by_key(|v| v["index"].as_u64().unwrap_or(u64::MAX));
            if data.len() != inputs.len()
                || data
                    .iter()
                    .enumerate()
                    .any(|(i, v)| v["index"].as_u64() != Some(i as u64))
            {
                return Err("Local embedding response is incomplete".into());
            }
            data.iter()
                .map(|v| normalize(&v["embedding"]))
                .collect::<Result<Vec<_>, String>>()
        };
        let result =
            tokio::select! {r=request=>r,error=self.interrupted(cancel, expected)=>Err(error)};
        p.last_used = now_ms();
        if let Err(error) = &result {
            if let Err(cleanup) = self.close_child(&mut p.child).await {
                return Err(format!("{error}; {cleanup}"));
            }
        } else {
            *guard = Some(p);
        }
        result
    }
    async fn launch(
        &self,
        threads: usize,
        executable: &Path,
        cancel: &Cancellation,
        expected: u64,
    ) -> Result<Process, String> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        drop(listener);
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let endpoint = format!("http://127.0.0.1:{port}");
        cancel.check()?;
        // Tests inject a minimal native executable: a libtest binary is not the GUI
        // entrypoint and must never be recursively launched as the supervisor.
        #[cfg(not(test))]
        let supervisor = std::env::current_exe().map_err(|e| e.to_string())?;
        #[cfg(test)]
        let supervisor = self
            .inner
            .supervisor
            .lock()
            .unwrap()
            .clone()
            .ok_or("Test must inject a native supervisor fixture")?;
        let mut cmd = Command::new(supervisor);
        cmd.arg(child_supervisor::MODE).arg(executable);
        cmd.arg("--model")
            .arg(self.inner.directory.join(self.inner.model.descriptor.file))
            .args([
                "--embedding",
                "--pooling",
                "mean",
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--api-key",
                &token,
                "--ctx-size",
                "8192",
                "--batch-size",
                "8192",
                "--ubatch-size",
                "8192",
                "--parallel",
                "1",
                "--threads",
                &threads.to_string(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            // Never kill the supervisor on drop: dropping its stdin signals it to
            // terminate descendants first, including on aborted launch futures.
            .kill_on_drop(false);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        let mut child = cmd.spawn().map_err(|e| e.to_string())?;
        let stderr = Arc::new(Mutex::new(std::collections::VecDeque::<u8>::new()));
        let stderr_task = if let Some(mut pipe) = child.stderr.take() {
            let output = stderr.clone();
            Some(OwnedTask(Some(tokio::spawn(async move {
                let mut block = [0; 1024];
                loop {
                    match pipe.read(&mut block).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let mut tail = output.lock().unwrap();
                            tail.extend(&block[..n]);
                            while tail.len() > 4000 {
                                tail.pop_front();
                            }
                        }
                    }
                }
            }))))
        } else {
            None
        };
        #[cfg(test)]
        let mut last_health_probe = String::from("not attempted");
        let ready = tokio::time::timeout(self.inner.ready_timeout, async {
            loop {
                self.check_runtime(cancel, expected)?;
                if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                    let error = stderr.lock().unwrap().iter().copied().collect::<Vec<_>>();
                    return Err(format!(
                        "Local embedding runtime exited ({status}): {}",
                        String::from_utf8_lossy(&error)
                    ));
                }
                let health = self
                    .inner
                    .client
                    .get(format!("{endpoint}/health"))
                    .bearer_auth(&token)
                    .timeout(std::time::Duration::from_secs(1))
                    .send()
                    .await;
                #[cfg(test)]
                {
                    last_health_probe = match &health {
                        Ok(response) => format!("HTTP {}", response.status()),
                        Err(error) => format!("{error:?}"),
                    };
                }
                if let Ok(response) = health {
                    if response.status().is_success() {
                        return Ok(());
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        })
        .await
        .unwrap_or_else(|_| Err("Local embedding runtime took too long to start".into()));
        if let Err(error) = ready {
            if let Err(cleanup) = self.close_child(&mut child).await {
                return Err(format!("{error}; {cleanup}"));
            }
            // Keep native CI failures actionable without changing runtime errors,
            // readiness deadlines, or lease cleanup. No auth headers are printed.
            #[cfg(test)]
            eprintln!(
                "[embedding-protocol-fixture] {error}; last health probe: {last_health_probe}; child stderr: {}",
                String::from_utf8_lossy(&stderr.lock().unwrap().iter().copied().collect::<Vec<_>>())
            );
            return Err(error);
        }
        Ok(Process {
            child,
            endpoint,
            token,
            last_used: now_ms(),
            threads,
            _stderr_task: stderr_task,
        })
    }
}
pub fn normalize(value: &Value) -> Result<Vec<f64>, String> {
    let values = value
        .as_array()
        .ok_or("EmbeddingGemma returned an invalid vector")?;
    if values.len() != 768
        || values
            .iter()
            .any(|v| v.as_f64().map(|v| !v.is_finite()).unwrap_or(true))
    {
        return Err("EmbeddingGemma returned an invalid vector".into());
    }
    let mut truncated: Vec<f64> = values[..256].iter().map(|v| v.as_f64().unwrap()).collect();
    // Scale first: squaring finite values can overflow (or underflow to zero).
    // This preserves first-256 Matryoshka truncation and ordinary unit vectors.
    let scale = truncated.iter().map(|v| v.abs()).fold(0., f64::max);
    if !scale.is_finite() || scale <= 0. {
        return Err("EmbeddingGemma returned an empty vector".into());
    }
    let magnitude = truncated
        .iter()
        .map(|v| (v / scale).powi(2))
        .sum::<f64>()
        .sqrt();
    if !magnitude.is_finite() || magnitude <= 0. {
        return Err("EmbeddingGemma returned an invalid vector norm".into());
    }
    for v in &mut truncated {
        *v = (*v / scale) / magnitude;
    }
    if truncated.iter().any(|v| !v.is_finite()) || !truncated.iter().any(|v| *v != 0.) {
        return Err("EmbeddingGemma returned an invalid normalized vector".into());
    }
    Ok(truncated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_core::common::CorePaths;
    fn context(path: &Path) -> CoreContext {
        CoreContext::headless(CorePaths {
            data_dir: path.join("data"),
            config_dir: path.join("config"),
            resource_dir: path.join("resources"),
            temp_dir: path.join("tmp"),
            home_dir: path.join("home"),
        })
    }
    // Small protocol fixtures use the production verifier, with an explicit test
    // identity, never an integrity bypass or a duplicate model-verification algorithm.
    fn fixture_runtime(ctx: CoreContext) -> Runtime {
        fixture_runtime_for(ctx, EmbeddingVariantId::Q8_0)
    }
    fn fixture_runtime_for(ctx: CoreContext, variant: EmbeddingVariantId) -> Runtime {
        let mut runtime = Runtime::new_for_variant(ctx, variant);
        Arc::get_mut(&mut runtime.inner).unwrap().model = ModelIdentity {
            descriptor: variant.descriptor(),
            bytes: 15,
            sha: "0984e3ef689f99ea5dc2d75420b1f4a61d0280a6383626c691b4403650ad08c1",
        };
        runtime
    }
    async fn initialized(runtime: &Runtime) {
        runtime.initialize().await.unwrap();
        if let Some(task) = runtime.inner.verification_task.lock().await.take() {
            task.await.unwrap();
        }
    }
    fn installed_fixture(runtime: &Runtime) {
        std::fs::create_dir_all(runtime.inner.directory.join("runtime")).unwrap();
        std::fs::write(
            runtime
                .inner
                .directory
                .join(runtime.inner.model.descriptor.file),
            b"test-only-model",
        )
        .unwrap();
        write_json_atomic(
            &runtime.inner.directory.join("installed.json"),
            &json!({"revision":runtime.inner.model.descriptor.revision,"runtime":platform().unwrap().file}),
        )
        .unwrap();
    }
    #[test]
    fn variants_have_independent_install_paths_and_legacy_q8_identity() {
        let temp = tempfile::tempdir().unwrap();
        let q8 = Runtime::new(context(temp.path()));
        let q4 = Runtime::new_for_variant(context(temp.path()), EmbeddingVariantId::Q4KM);
        assert_eq!(
            q8.inner.directory,
            context(temp.path())
                .paths
                .data_dir
                .join("knowledge/models/embeddinggemma-2")
        );
        assert_eq!(MODEL_KEY, runtime_models::Q8.model_key);
        assert_ne!(
            q4.inner.directory, q8.inner.directory,
            "Q4 must never inherit the Q8 installation"
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn first_foreground_query_returns_while_cold_runtime_is_warming() {
        let (_temp, runtime) = protocol_fixture("warming");
        initialized(&runtime).await;
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            runtime.foreground_embed(vec!["fixture".into()], "slow", &cancellation()),
        )
        .await;
        runtime.shutdown().await;
        assert!(result.is_ok(), "foreground query blocked on a cold runtime");
        assert_eq!(result.unwrap().unwrap(), ForegroundEmbedding::Warming);
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn foreground_does_not_queue_behind_indexing_mutex() {
        let (_temp, runtime) = protocol_fixture("ok");
        initialized(&runtime).await;
        let hold = runtime.inner.process.lock().await;
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            runtime.foreground_embed(vec!["fixture".into()], "slow", &cancellation()),
        )
        .await;
        drop(hold);
        assert!(
            result.is_ok(),
            "foreground query queued behind background indexing"
        );
        assert_eq!(result.unwrap().unwrap(), ForegroundEmbedding::Busy);
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn dropping_last_runtime_owner_aborts_warmup_promptly() {
        let (_temp, runtime) = protocol_fixture("unhealthy");
        initialized(&runtime).await;
        runtime.prewarm("slow").await.unwrap();
        wait_for_file(&runtime, "pids").await;
        let owned = pids(&runtime);
        drop(runtime);
        tokio::time::timeout(std::time::Duration::from_millis(600), wait_dead(owned))
            .await
            .expect("last runtime owner left a detached warmup running");
    }
    #[cfg(unix)]
    async fn wait_for_file(runtime: &Runtime, name: &str) {
        tokio::time::timeout(std::time::Duration::from_secs(4), async {
            while !runtime.inner.directory.join(name).exists() {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn cancel_download_preserves_independent_initial_and_reopen_verification() {
        let (_temp, runtime) = protocol_fixture("ok");
        for reopen in [false, true] {
            let hold = runtime.inner.verification.lock().await;
            runtime.initialize().await.unwrap();
            assert_eq!(runtime.snapshot_variant()["state"], "verifying");
            let generation = runtime.inner.download_gen.load(Ordering::SeqCst);
            tokio::time::timeout(
                std::time::Duration::from_millis(200),
                runtime.cancel_download(),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(
                runtime.snapshot_variant()["state"],
                "verifying",
                "Cancel download replaced an independent file check with fake missing"
            );
            assert_eq!(
                runtime.inner.download_gen.load(Ordering::SeqCst),
                generation
            );
            assert!(runtime.inner.directory.join(MODEL_FILE).exists());
            if reopen {
                assert_eq!(runtime.snapshot()["modelState"], "ready");
            }
            drop(hold);
            initialized(&runtime).await;
            assert_eq!(runtime.snapshot_variant()["state"], "ready");
        }
    }
    #[test]
    fn registry_artifacts_urls_keys_and_wire_ids_are_exact() {
        assert_eq!(EmbeddingVariantId::default(), EmbeddingVariantId::Q8_0);
        assert_eq!(
            serde_json::to_value(EmbeddingVariantId::ALL).unwrap(),
            json!(["q8_0", "q4_k_m"])
        );
        assert_eq!(
            serde_json::from_value::<EmbeddingVariantId>(json!("q4_k_m")).unwrap(),
            EmbeddingVariantId::Q4KM
        );
        assert!(serde_json::from_value::<EmbeddingVariantId>(json!("q4")).is_err());
        let q8 = EmbeddingVariantId::Q8_0.descriptor();
        let q4 = EmbeddingVariantId::Q4KM.descriptor();
        assert_eq!(q8.bytes, 309855456);
        assert_eq!(q4.bytes, 181695104);
        assert_eq!(
            q8.sha256,
            "2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135"
        );
        assert_eq!(
            q4.sha256,
            "ea25550b050fa8826707c5e44659a7c18259606dcc6ba449f771e3c75cf4e663"
        );
        assert_eq!(q8.url(), "https://huggingface.co/ggml-org/embeddinggemma-2-GGUF/resolve/bfcd298762cc34d0357ece5ebdd31791a3a374d8/embeddinggemma-2-Q8_0.gguf");
        assert_eq!(q4.url(), "https://huggingface.co/ngquocvinh/EmbeddingGemma-2-GGUF/resolve/8e83fab7c71a90d8f97ae7cacf6a488f654cb5b6/EmbeddingGemma-2-Q4_K_M.gguf");
        assert_eq!(
            q8.model_key,
            "google/embeddinggemma-2:bfcd298762cc34d0357ece5ebdd31791a3a374d8:q8:256:search-v1"
        );
        assert_eq!(q4.model_key, "ngquocvinh/EmbeddingGemma-2-GGUF:8e83fab7c71a90d8f97ae7cacf6a488f654cb5b6:ea25550b050fa8826707c5e44659a7c18259606dcc6ba449f771e3c75cf4e663:q4_k_m:256:search-v1");
    }
    #[test]
    fn variant_snapshot_uses_weights_progress_and_attributed_failures() {
        let temp = tempfile::tempdir().unwrap();
        for variant in EmbeddingVariantId::ALL {
            let runtime = Runtime::new_for_variant(context(temp.path()), variant);
            assert_eq!(runtime.variant_id(), variant);
            assert_eq!(runtime.model_key(), variant.descriptor().model_key);
            let snap = runtime.snapshot_variant();
            assert_eq!(snap["weightBytes"], variant.descriptor().bytes);
            assert_eq!(
                runtime.snapshot()["weightBytes"],
                variant.descriptor().bytes
            );
            assert_eq!(
                snap["downloadTotalBytes"],
                variant.descriptor().bytes + platform().unwrap().bytes
            );
            assert_eq!(snap["state"], "not-downloaded");
            runtime.patch(
                json!({"modelState":"downloading","downloadBytes":17,"downloadProgress":0.1}),
            );
            let snap = runtime.snapshot_variant();
            assert_eq!(snap["state"], "downloading");
            assert_eq!(snap["downloadBytes"], 17);
            assert_eq!(snap["downloadProgress"], 0.1);
            runtime.patch(json!({"integrityState":"verifying"}));
            assert_eq!(runtime.snapshot_variant()["state"], "verifying");
            for (source, stage) in [
                ("download", "download"),
                ("integrity", "integrity"),
                ("runtime", "startup"),
            ] {
                runtime.patch(json!({"integrityState":"verified","modelState":if source=="runtime" {"ready"} else {"error"},"errorSource":source,"error":"fixture failure"}));
                let snap = runtime.snapshot_variant();
                assert_eq!(
                    snap["failure"],
                    json!({"stage":stage,"message":"fixture failure"})
                );
                if source == "runtime" {
                    assert_eq!(snap["state"], "ready");
                }
            }
        }
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn q4_manifest_requires_source_and_cannot_reuse_q8_cache_or_identity() {
        let temp = tempfile::tempdir().unwrap();
        let q8 = protocol_variant(context(temp.path()), EmbeddingVariantId::Q8_0, "ok");
        let q4 = protocol_variant(context(temp.path()), EmbeddingVariantId::Q4KM, "ok");
        initialized(&q8).await;
        initialized(&q4).await;
        assert_eq!(q4.snapshot_variant()["state"], "ready");
        assert_eq!(q4.snapshot()["modelFile"], runtime_models::Q4.file);
        assert_eq!(q4.snapshot()["modelKey"], q4.model_key());
        let cache = q8.inner.verified.lock().unwrap().clone().unwrap();
        assert!(runtime_integrity::cached_install(
            &q8.inner.directory,
            &platform().unwrap(),
            q4.inner.model,
            Some(&cache)
        )
        .is_none());
        assert!(runtime_integrity::verify(
            &q8.inner.directory,
            &platform().unwrap(),
            q4.inner.model,
            Some(&cache),
            &|| Ok(())
        )
        .is_err());
        let path = q4.inner.directory.join("installed.json");
        let valid: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        for (key, value) in [
            ("variantId", json!("q8_0")),
            ("sourceRepository", json!(runtime_models::Q8.repository)),
            ("modelKey", json!(MODEL_KEY)),
            ("modelFile", json!(MODEL_FILE)),
            ("revision", json!(REVISION)),
            ("modelSha256", json!(MODEL_SHA)),
        ] {
            let mut bad = valid.clone();
            bad[key] = value;
            write_json_atomic(&path, &bad).unwrap();
            initialized(&q4).await;
            assert_eq!(
                q4.snapshot_variant()["state"],
                "error",
                "accepted cross-variant {key}"
            );
            assert_eq!(q8.snapshot_variant()["state"], "ready");
        }
        for key in ["variantId", "sourceRepository", "schema"] {
            let mut bad = valid.clone();
            bad.as_object_mut().unwrap().remove(key);
            write_json_atomic(&path, &bad).unwrap();
            initialized(&q4).await;
            assert_eq!(
                q4.snapshot_variant()["state"],
                "error",
                "accepted missing {key}"
            );
        }
        write_json_atomic(&path, &valid).unwrap();
        initialized(&q4).await;
        assert_eq!(q4.snapshot_variant()["state"], "ready");
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn q4_q8_stop_cancel_remove_and_failures_are_independent() {
        for removed in EmbeddingVariantId::ALL {
            let temp = tempfile::tempdir().unwrap();
            let q8 = protocol_variant(context(temp.path()), EmbeddingVariantId::Q8_0, "ok");
            let q4 = protocol_variant(context(temp.path()), EmbeddingVariantId::Q4KM, "ok");
            initialized(&q8).await;
            initialized(&q4).await;
            infer(&q8, "fixture").await.unwrap();
            infer(&q4, "fixture").await.unwrap();
            let (remove, keep) = if removed == EmbeddingVariantId::Q8_0 {
                (&q8, &q4)
            } else {
                (&q4, &q8)
            };
            let keep_pids = pids(keep);
            let removed_pids = pids(remove);
            remove.cancel_download().await.unwrap();
            assert_eq!(keep.inner.download_gen.load(Ordering::SeqCst), 0);
            remove.remove().await.unwrap();
            wait_dead(removed_pids).await;
            assert!(!remove.inner.directory.exists());
            assert!(keep
                .inner
                .directory
                .join(keep.inner.model.descriptor.file)
                .exists());
            assert!(keep_pids.iter().all(|p| child_supervisor::tests::alive(*p)));
            infer(keep, "fixture").await.unwrap();
            assert_eq!(pids(keep), keep_pids);
            keep.shutdown().await;
            wait_dead(keep_pids).await;
        }
    }
    #[cfg(unix)]
    async fn wait_warm(runtime: &Runtime) {
        tokio::time::timeout(std::time::Duration::from_secs(6), async {
            while runtime.snapshot()["runtimeState"] != "ready" {
                assert_ne!(
                    runtime.snapshot()["runtimeState"],
                    "error",
                    "{}",
                    runtime.snapshot()
                );
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn rapid_cancelled_queries_do_not_relaunch_or_cancel_owned_warmup() {
        let (_temp, runtime) = protocol_fixture("warming");
        initialized(&runtime).await;
        assert_eq!(
            runtime
                .foreground_embed(vec!["fixture".into()], "slow", &cancellation())
                .await
                .unwrap(),
            ForegroundEmbedding::Warming
        );
        wait_for_file(&runtime, "pids").await;
        for _ in 0..20 {
            assert_eq!(
                runtime
                    .foreground_embed(vec!["fixture".into()], "slow", &cancellation())
                    .await
                    .unwrap(),
                ForegroundEmbedding::Warming
            );
            let cancel = cancellation();
            cancel.generation.fetch_add(1, Ordering::SeqCst);
            assert!(runtime
                .foreground_embed(vec!["stale".into()], "fast", &cancel)
                .await
                .is_err());
            runtime.prewarm("slow").await.unwrap();
        }
        wait_warm(&runtime).await;
        let expected = ForegroundEmbedding::Ready(vec![vec![0.0625; 256]]);
        assert_eq!(
            runtime
                .foreground_embed(vec!["fixture".into()], "slow", &cancellation())
                .await
                .unwrap(),
            expected
        );
        assert_eq!(
            std::fs::read_to_string(runtime.inner.directory.join("launches"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        assert!(runtime.snapshot()["error"].is_null());
        let owned = pids(&runtime);
        runtime.shutdown().await;
        wait_dead(owned).await;
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn warm_foreground_timeout_is_bounded_visible_cleans_lease_and_recovers() {
        let (_temp, runtime) = protocol_fixture("ok");
        initialized(&runtime).await;
        runtime.prewarm("slow").await.unwrap();
        wait_warm(&runtime).await;
        let owned = pids(&runtime);
        let start = std::time::Instant::now();
        let error = runtime
            .foreground_embed(vec!["hold".into()], "slow", &cancellation())
            .await
            .unwrap_err();
        assert!(start.elapsed() < std::time::Duration::from_millis(1300));
        assert!(error.contains("timed out"), "{error}");
        assert_eq!(
            runtime.snapshot_variant()["failure"],
            json!({"stage":"startup","message":error})
        );
        assert_eq!(runtime.snapshot_variant()["state"], "ready");
        assert!(runtime.inner.process.lock().await.is_none());
        wait_dead(owned).await;
        // Keystrokes cannot turn a persistent failure into an automatic retry loop.
        assert!(runtime
            .foreground_embed(vec!["fixture".into()], "slow", &cancellation())
            .await
            .is_err());
        assert_eq!(
            std::fs::read_to_string(runtime.inner.directory.join("launches"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        runtime.prewarm("slow").await.unwrap();
        wait_warm(&runtime).await;
        assert!(runtime.snapshot()["error"].is_null());
        assert!(matches!(
            runtime
                .foreground_embed(vec!["fixture".into()], "slow", &cancellation())
                .await
                .unwrap(),
            ForegroundEmbedding::Ready(_)
        ));
        let owned = pids(&runtime);
        runtime.shutdown().await;
        wait_dead(owned).await;
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn foreground_busy_does_not_cancel_background_indexing_request() {
        let (_temp, runtime) = protocol_fixture("ok");
        initialized(&runtime).await;
        let this = runtime.clone();
        let cancel = cancellation();
        let generation = cancel.generation.clone();
        let task =
            tokio::spawn(async move { this.embed(vec!["hold".into()], "slow", &cancel).await });
        wait_for_file(&runtime, "holding").await;
        let owned = pids(&runtime);
        assert_eq!(
            runtime
                .foreground_embed(vec!["fixture".into()], "slow", &cancellation())
                .await
                .unwrap(),
            ForegroundEmbedding::Busy
        );
        assert!(owned.iter().all(|p| child_supervisor::tests::alive(*p)));
        generation.fetch_add(1, Ordering::SeqCst);
        assert!(task.await.unwrap().is_err());
        wait_dead(owned).await;
        assert!(runtime.snapshot()["error"].is_null());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn stop_remove_shutdown_abort_and_reap_owned_startup() {
        for operation in ["stop", "remove", "shutdown"] {
            let (_temp, runtime) = protocol_fixture("unhealthy");
            initialized(&runtime).await;
            runtime.prewarm("slow").await.unwrap();
            wait_for_file(&runtime, "pids").await;
            let owned = pids(&runtime);
            match operation {
                "stop" => runtime.stop().await,
                "remove" => runtime.remove().await.unwrap(),
                _ => runtime.shutdown().await,
            }
            wait_dead(owned).await;
            assert!(runtime.inner.startup_task.lock().unwrap().is_none());
            assert!(runtime.inner.process.lock().await.is_none());
            assert_eq!(runtime.snapshot()["runtimeState"], "stopped");
            assert!(
                runtime.snapshot()["error"].is_null(),
                "{}",
                runtime.snapshot()
            );
        }
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn aborted_foreground_future_drops_its_active_lease() {
        let (_temp, runtime) = protocol_fixture("ok");
        initialized(&runtime).await;
        runtime.prewarm("slow").await.unwrap();
        wait_warm(&runtime).await;
        let this = runtime.clone();
        let task = tokio::spawn(async move {
            this.foreground_embed(vec!["hold".into()], "slow", &cancellation())
                .await
        });
        wait_for_file(&runtime, "holding").await;
        let owned = pids(&runtime);
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        wait_dead(owned).await;
        assert!(runtime.inner.process.lock().await.is_none());
        assert!(runtime.snapshot()["error"].is_null());
        assert_eq!(runtime.snapshot()["runtimeState"], "stopped");
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn foreground_changed_model_schedules_verification_without_using_cached_process() {
        let (_temp, runtime) = protocol_fixture("ok");
        initialized(&runtime).await;
        runtime.prewarm("slow").await.unwrap();
        wait_warm(&runtime).await;
        let owned = pids(&runtime);
        std::fs::write(runtime.inner.directory.join(MODEL_FILE), b"bad!-only-model").unwrap();
        let hold = runtime.inner.verification.lock().await;
        assert_eq!(
            runtime
                .foreground_embed(vec!["fixture".into()], "slow", &cancellation())
                .await
                .unwrap(),
            ForegroundEmbedding::Warming
        );
        assert_eq!(runtime.snapshot()["integrityState"], "verifying");
        wait_dead(owned).await;
        drop(hold);
        initialized(&runtime).await;
        assert_eq!(runtime.snapshot_variant()["failure"]["stage"], "integrity");
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn ordinary_indexing_still_awaits_cold_start_and_prewarm_changes_profile_safely() {
        let (_temp, runtime) = protocol_fixture("warming");
        initialized(&runtime).await;
        let start = std::time::Instant::now();
        assert_eq!(
            infer(&runtime, "fixture").await.unwrap(),
            vec![vec![0.0625; 256]]
        );
        assert!(start.elapsed() >= std::time::Duration::from_millis(600));
        let old = pids(&runtime);
        runtime.prewarm("fast").await.unwrap();
        let task = runtime.inner.startup_task.lock().unwrap().take().unwrap();
        // Await without detaching; the handle remains in this owned wrapper.
        let mut task = task;
        task.0.take().unwrap().await.unwrap();
        assert_eq!(
            runtime.inner.process.lock().await.as_ref().unwrap().threads,
            threads("fast")
        );
        if threads("slow") != threads("fast") {
            wait_dead(old).await;
        }
        let owned = pids(&runtime);
        runtime.shutdown().await;
        wait_dead(owned).await;
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn failed_owned_prewarm_publishes_error_and_explicit_retry_recovers() {
        let (_temp, runtime) = protocol_fixture("unhealthy");
        initialized(&runtime).await;
        runtime.prewarm("slow").await.unwrap();
        wait_for_file(&runtime, "pids").await;
        let owned = pids(&runtime);
        tokio::time::timeout(std::time::Duration::from_secs(4), async {
            while runtime.snapshot()["runtimeState"] != "error" {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(runtime.snapshot_variant()["state"], "ready");
        assert_eq!(runtime.snapshot_variant()["failure"]["stage"], "startup");
        wait_dead(owned).await;
        for _ in 0..3 {
            assert!(runtime
                .foreground_embed(vec!["fixture".into()], "slow", &cancellation())
                .await
                .is_err());
        }
        assert_eq!(
            std::fs::read_to_string(runtime.inner.directory.join("launches"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        std::fs::write(runtime.inner.directory.join("mode"), "ok").unwrap();
        runtime.prewarm("slow").await.unwrap();
        wait_warm(&runtime).await;
        assert!(runtime.snapshot()["error"].is_null());
        let owned = pids(&runtime);
        runtime.shutdown().await;
        wait_dead(owned).await;
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn cancelled_owned_shutdown_future_still_closes_startup_lease() {
        let (_temp, runtime) = protocol_fixture("unhealthy");
        initialized(&runtime).await;
        runtime.prewarm("slow").await.unwrap();
        wait_for_file(&runtime, "pids").await;
        let owned = pids(&runtime);
        let this = runtime.clone();
        let task = tokio::spawn(async move { this.shutdown().await });
        while !runtime.inner.closed.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
        task.abort();
        let _ = task.await;
        wait_dead(owned).await;
        assert!(runtime.inner.process.lock().await.is_none());
        assert!(runtime.inner.startup_task.lock().unwrap().is_none());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn cancelling_one_active_download_preserves_the_other_download() {
        let temp = tempfile::tempdir().unwrap();
        let q8 = protocol_variant(context(temp.path()), EmbeddingVariantId::Q8_0, "ok");
        let q4 = protocol_variant(context(temp.path()), EmbeddingVariantId::Q4KM, "ok");
        initialized(&q8).await;
        initialized(&q4).await;
        let mut servers = Vec::new();
        for runtime in [&q8, &q4] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}/fixture", listener.local_addr().unwrap());
            let (accepted, observed) = tokio::sync::oneshot::channel();
            servers.push(tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0; 2048];
                let _ = socket.read(&mut request).await;
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100000\r\n\r\nx")
                    .await
                    .unwrap();
                let _ = accepted.send(());
                std::future::pending::<()>().await;
            }));
            let cancel = Cancellation {
                generation: runtime.inner.download_gen.clone(),
                expected: runtime.inner.download_gen.load(Ordering::SeqCst),
            };
            let this = runtime.worker();
            runtime.patch(json!({"modelState":"downloading"}));
            *runtime.inner.download.lock().await = Some(tokio::spawn(async move {
                assert!(this
                    .install_from(
                        platform().unwrap(),
                        &url,
                        "http://127.0.0.1:1/unused",
                        &cancel
                    )
                    .await
                    .is_err());
            }));
            observed.await.unwrap();
        }
        q4.cancel_download().await.unwrap();
        assert_eq!(q4.snapshot_variant()["state"], "ready");
        assert_eq!(q8.snapshot_variant()["state"], "downloading");
        assert_eq!(q8.inner.download_gen.load(Ordering::SeqCst), 0);
        assert!(!q8
            .inner
            .download
            .lock()
            .await
            .as_ref()
            .unwrap()
            .is_finished());
        q8.cancel_download().await.unwrap();
        assert_eq!(q8.snapshot_variant()["state"], "ready");
        for server in servers {
            server.abort();
            let _ = server.await;
        }
        assert_eq!(
            std::fs::read_dir(q8.inner.directory.parent().unwrap())
                .unwrap()
                .count(),
            2
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn explicit_repair_redownloads_startup_failed_install_and_healthy_download_is_idempotent()
    {
        for variant in EmbeddingVariantId::ALL {
            let temp = tempfile::tempdir().unwrap();
            let runtime = protocol_variant(context(temp.path()), variant, "ok");
            initialized(&runtime).await;
            runtime.download().await.unwrap();
            assert!(
                runtime.inner.download.lock().await.is_none(),
                "healthy ready download must stay idempotent"
            );
            assert_eq!(runtime.inner.download_gen.load(Ordering::SeqCst), 0);
            *runtime.inner.supervisor.lock().unwrap() =
                Some(temp.path().join("missing-supervisor"));
            assert!(infer(&runtime, "fixture").await.is_err());
            assert_eq!(runtime.snapshot()["modelState"], "ready");
            assert_eq!(runtime.snapshot()["errorSource"], "runtime");
            let archive = temp.path().join("fixture.tar.gz");
            assert!(std::process::Command::new("tar")
                .arg("-czf")
                .arg(&archive)
                .arg("-C")
                .arg(runtime.inner.directory.join("runtime"))
                .arg("llama-server")
                .status()
                .unwrap()
                .success());
            let bytes = std::fs::read(archive).unwrap();
            let digest = Box::leak(format!("{:x}", Sha256::digest(&bytes)).into_boxed_str());
            let (runtime_url, archive_task) = serve(bytes.clone()).await;
            let (model_url, model_task) = serve(b"test-only-model".to_vec()).await;
            *runtime.inner.install_source.lock().unwrap() = Some(TestInstallSource {
                runtime: Manifest {
                    file: platform().unwrap().file,
                    bytes: bytes.len() as u64,
                    sha: digest,
                },
                runtime_url,
                model_url,
            });
            runtime.download().await.unwrap();
            let task = runtime.inner.download.lock().await.take();
            if let Some(task) = task {
                tokio::time::timeout(std::time::Duration::from_secs(5), task)
                    .await
                    .unwrap()
                    .unwrap();
                archive_task.await.unwrap();
                model_task.await.unwrap();
            } else {
                archive_task.abort();
                model_task.abort();
                let _ = archive_task.await;
                let _ = model_task.await;
                panic!("explicit Repair did not schedule installation after startup failure");
            }
            assert_eq!(runtime.snapshot_variant()["state"], "ready");
            assert!(runtime.snapshot()["error"].is_null());
            assert_eq!(runtime.inner.download_gen.load(Ordering::SeqCst), 1);
            runtime.download().await.unwrap();
            assert!(runtime.inner.download.lock().await.is_none());
            assert_eq!(runtime.inner.download_gen.load(Ordering::SeqCst), 1);
        }
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn repeated_download_during_reopen_check_cannot_cancel_or_replace_installed_model() {
        let (_temp, runtime) = protocol_fixture("ok");
        let hold = runtime.inner.verification.lock().await;
        runtime.initialize().await.unwrap();
        // No network source exists: this operation must preserve the owned check.
        runtime.download().await.unwrap();
        let task = runtime.inner.download.lock().await.take();
        let scheduled = task.is_some();
        if let Some(task) = task {
            task.abort();
            let _ = task.await;
        }
        drop(hold);
        assert!(
            !scheduled,
            "Download during independent verification scheduled a replacement"
        );
        assert_eq!(runtime.inner.download_gen.load(Ordering::SeqCst), 0);
        initialized(&runtime).await;
        assert_eq!(runtime.snapshot_variant()["state"], "ready");
    }
    #[test]
    fn normalization_handles_finite_overflow_and_underflow_without_zero_vectors() {
        for value in [1e308, 1e-308] {
            let vector = normalize(&json!(vec![value; 768])).unwrap();
            assert_eq!(vector, vec![0.0625; 256]);
        }
        let ordinary: Vec<f64> = (0..768).map(|i| (i as f64 * 0.37).sin()).collect();
        let norm = ordinary[..256].iter().map(|v| v * v).sum::<f64>().sqrt();
        let vector = normalize(&json!(ordinary)).unwrap();
        assert!(vector
            .iter()
            .zip(&ordinary)
            .all(|(a, b)| (a - b / norm).abs() < 1e-15));
        assert!((vector.iter().map(|v| v * v).sum::<f64>() - 1.).abs() < 1e-14);
        for value in [
            json!(vec![0.; 768]),
            json!(vec![1.; 767]),
            json!(vec![Value::Null; 768]),
        ] {
            assert!(normalize(&value).is_err());
        }
    }
    #[tokio::test]
    async fn corrupt_installed_model_is_not_ready_and_reports_repairable_error() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = Runtime::new(context(temp.path()));
        let directory = &runtime.inner.directory;
        std::fs::create_dir_all(directory.join("runtime")).unwrap();
        std::fs::write(directory.join(MODEL_FILE), b"corrupt").unwrap();
        std::fs::write(directory.join("runtime/llama-server"), b"not executable").unwrap();
        write_json_atomic(
            &directory.join("installed.json"),
            &json!({"revision":REVISION,"runtime":platform().unwrap().file}),
        )
        .unwrap();
        initialized(&runtime).await;
        assert_ne!(runtime.snapshot()["modelState"], "ready");
        assert!(runtime.snapshot()["error"].is_string());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn spawn_failure_is_published_before_lexical_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = fixture_runtime(context(temp.path()));
        installed_fixture(&runtime);
        std::fs::write(
            runtime.inner.directory.join("runtime/llama-server"),
            b"fixture",
        )
        .unwrap();
        initialized(&runtime).await;
        *runtime.inner.supervisor.lock().unwrap() = Some(temp.path().join("missing-supervisor"));
        runtime.patch(json!({"modelState":"ready"}));
        let cancel = Cancellation {
            generation: Arc::new(AtomicU64::new(0)),
            expected: 0,
        };
        let error = runtime
            .embed(vec!["fixture".into()], "slow", &cancel)
            .await
            .unwrap_err();
        assert_eq!(runtime.snapshot()["error"], error);
    }
    fn cancellation() -> Cancellation {
        Cancellation {
            generation: Arc::new(AtomicU64::new(0)),
            expected: 0,
        }
    }
    #[cfg(unix)]
    fn protocol_fixture(mode: &str) -> (tempfile::TempDir, Runtime) {
        let temp = tempfile::tempdir().unwrap();
        let runtime = protocol_variant(context(temp.path()), EmbeddingVariantId::Q8_0, mode);
        (temp, runtime)
    }
    #[cfg(unix)]
    fn protocol_variant(ctx: CoreContext, variant: EmbeddingVariantId, mode: &str) -> Runtime {
        use std::os::unix::fs::PermissionsExt;
        let mut runtime = fixture_runtime_for(ctx, variant);
        let inner = Arc::get_mut(&mut runtime.inner).unwrap();
        inner.ready_timeout =
            std::time::Duration::from_secs(if mode == "unhealthy" { 2 } else { 5 });
        inner.request_timeout = std::time::Duration::from_millis(300);
        installed_fixture(&runtime);
        let executable = runtime.inner.directory.join("runtime/llama-server");
        std::fs::write(&executable, PROTOCOL_SERVER).unwrap();
        std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(runtime.inner.directory.join("mode"), mode).unwrap();
        *runtime.inner.supervisor.lock().unwrap() =
            Some(child_supervisor::tests::fixture_executable());
        if variant != EmbeddingVariantId::Q8_0 {
            let files =
                runtime_integrity::runtime_files(&runtime.inner.directory.join("runtime"), &|| {
                    Ok(())
                })
                .unwrap();
            write_json_atomic(
                &runtime.inner.directory.join("installed.json"),
                &runtime_integrity::manifest(&platform().unwrap(), runtime.inner.model, files),
            )
            .unwrap();
        }
        runtime
    }
    #[cfg(unix)]
    const PROTOCOL_SERVER: &str = r#"#!/usr/bin/env python3
import sys,faulthandler
# Bound diagnostics independently of the host's existing readiness deadline.
# A stalled import, subprocess launch, or hostname lookup leaves a Python stack.
faulthandler.dump_traceback_later(2)
def startup(stage): print('[embedding-protocol-fixture] '+stage,file=sys.stderr,flush=True)
startup('python '+sys.version.split()[0])
import json,os,subprocess,time
from pathlib import Path
from http.server import BaseHTTPRequestHandler,HTTPServer
from socketserver import TCPServer
startup('imports complete')
args=sys.argv[1:];port=int(args[args.index('--port')+1]);token=args[args.index('--api-key')+1]
assert '--embedding' in args and args[args.index('--pooling')+1]=='mean'
directory=Path(args[args.index('--model')+1]).parent
leaf=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)'])
startup('leaf spawned')
(directory/'pids').write_text(str(os.getpid())+' '+str(leaf.pid))
with (directory/'launches').open('a') as f: f.write(str(os.getpid())+'\n')
mode=(directory/'mode').read_text() if (directory/'mode').exists() else 'ok'
if mode=='exit': sys.exit(23)
if mode=='warming': time.sleep(0.7)
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args): pass
 def do_GET(self):
  if self.headers.get('Authorization')!='Bearer '+token:self.send_error(403);return
  self.send_response(503 if mode=='unhealthy' else 200);self.end_headers();self.wfile.write(b'{}')
 def do_POST(self):
  if self.headers.get('Authorization')!='Bearer '+token:self.send_error(403);return
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
  value=body['input'][0]
  if value=='hold':
   (directory/'holding').write_text('ready')
   time.sleep(60)
  if value=='http500':self.send_error(500);return
  data=[{'index':i,'embedding':[1.0]*768} for i,_ in enumerate(body['input'])]
  if value=='missing': data=[]
  if value=='duplicate': data=[data[0],data[0]]
  if value=='missingindex': del data[0]['index']
  if value=='wrongdim': data[0]['embedding']=[1.0]*767
  if value=='zero': data[0]['embedding']=[0.0]*768
  if value=='null': data[0]['embedding']=[None]*768
  if value=='nan': data[0]['embedding']=[float('nan')]*768
  if value=='overflow': data[0]['embedding']=[1e308]*768
  self.send_response(200);self.send_header('Content-Type','application/json');self.end_headers();self.wfile.write(json.dumps({'data':list(reversed(data))}).encode())
class LoopbackHTTPServer(HTTPServer):
 def server_bind(self):
  # HTTPServer resolves getfqdn here; this numeric-loopback fixture needs no DNS.
  TCPServer.server_bind(self)
  self.server_name,self.server_port=self.server_address
startup('binding HTTPServer')
server=LoopbackHTTPServer(('127.0.0.1',port),Handler)
startup('HTTPServer ready')
faulthandler.cancel_dump_traceback_later()
server.serve_forever()
"#;
    #[cfg(unix)]
    fn pids(runtime: &Runtime) -> Vec<u32> {
        std::fs::read_to_string(runtime.inner.directory.join("pids"))
            .unwrap()
            .split_whitespace()
            .map(|p| p.parse().unwrap())
            .collect()
    }
    #[cfg(unix)]
    async fn wait_dead(pids: Vec<u32>) {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while pids.iter().any(|pid| child_supervisor::tests::alive(*pid)) {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
    }
    async fn infer(runtime: &Runtime, text: &str) -> Result<Vec<Vec<f64>>, String> {
        runtime
            .embed(vec![text.into()], "slow", &cancellation())
            .await
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn successful_warm_inference_does_not_publish_false_stopped_transitions() {
        let (_temp, runtime) = protocol_fixture("ok");
        initialized(&runtime).await;
        infer(&runtime, "warmup").await.unwrap();
        let observer = Arc::new(Notify::new());
        runtime.observe(observer.clone());
        let mut spurious = Vec::new();
        for foreground in [false, true] {
            let changed = observer.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if foreground {
                assert!(matches!(
                    runtime
                        .foreground_embed(vec!["fixture".into()], "slow", &cancellation())
                        .await
                        .unwrap(),
                    ForegroundEmbedding::Ready(_)
                ));
            } else {
                infer(&runtime, "fixture").await.unwrap();
            }
            spurious.push(
                tokio::time::timeout(std::time::Duration::from_millis(20), changed)
                    .await
                    .is_ok(),
            );
        }
        let owned = pids(&runtime);
        runtime.shutdown().await;
        wait_dead(owned).await;
        assert_eq!(spurious, [false, false], "a successful request retains the ready process; only cancellation/failure should publish stopped");
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn protocol_fixture_serves_authenticated_inference_without_hostname_resolution() {
        let (_temp, runtime) = protocol_fixture("ok");
        // Inject only into this owned child, before running the actual shared
        // fixture. No process-global environment or host DNS state is changed.
        let guarded = [
            r#"#!/usr/bin/env python3
import socket
from pathlib import Path
guard_directory=Path(__file__).parent.parent
(guard_directory/'dns-guard-armed').write_text('ready')
def reject_hostname_resolution(*args,**kwargs):
 (guard_directory/'dns-lookup-attempted').write_text('getfqdn')
 raise RuntimeError('loopback protocol fixture must not resolve hostnames')
socket.getfqdn=reject_hostname_resolution
"#,
            PROTOCOL_SERVER,
        ]
        .concat();
        std::fs::write(
            runtime.inner.directory.join("runtime/llama-server"),
            guarded,
        )
        .unwrap();
        initialized(&runtime).await;
        let inference = infer(&runtime, "fixture").await;
        // Also reap both descendants on the pre-fix red path, before asserting.
        if inference.is_err() {
            runtime.shutdown().await;
            wait_dead(pids(&runtime)).await;
        }
        assert_eq!(inference.unwrap()[0], vec![0.0625; 256]);
        let endpoint = runtime
            .inner
            .process
            .lock()
            .await
            .as_ref()
            .unwrap()
            .endpoint
            .clone();
        let mut forbidden = Vec::new();
        for (method, path) in [
            (reqwest::Method::GET, "/health"),
            (reqwest::Method::POST, "/v1/embeddings"),
        ] {
            for authorization in [None, Some("Bearer wrong-token")] {
                let mut request = runtime
                    .inner
                    .client
                    .request(method.clone(), format!("{endpoint}{path}"))
                    .json(&json!({"input":["fixture"]}))
                    .timeout(runtime.inner.request_timeout);
                if let Some(value) = authorization {
                    request = request.header(reqwest::header::AUTHORIZATION, value);
                }
                forbidden.push(request.send().await.map(|response| response.status()));
            }
        }
        let owned = pids(&runtime);
        runtime.shutdown().await;
        wait_dead(owned).await;
        assert!(runtime.inner.process.lock().await.is_none());
        assert!(runtime.inner.directory.join("dns-guard-armed").is_file());
        assert!(!runtime
            .inner
            .directory
            .join("dns-lookup-attempted")
            .exists());
        for response in forbidden {
            assert_eq!(response.unwrap(), reqwest::StatusCode::FORBIDDEN);
        }
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn integrity_checks_same_length_replacement_missing_model_and_repaired_reinitialize() {
        let (temp, runtime) = protocol_fixture("ok");
        initialized(&runtime).await;
        assert_eq!(
            runtime.snapshot()["runtimeIntegrity"],
            "legacy-local-baseline"
        );
        let model = runtime.inner.directory.join(MODEL_FILE);
        // Same length and restored mtime, but a different file identity. Neither
        // a size-only check nor an mtime cache may authorize this replacement.
        let modified = std::fs::metadata(&model).unwrap().modified().unwrap();
        let replacement = runtime.inner.directory.join("replacement.gguf");
        std::fs::write(&replacement, b"bad!-only-model").unwrap();
        std::fs::File::open(&replacement)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        std::fs::rename(replacement, &model).unwrap();
        let error = infer(&runtime, "fixture").await.unwrap_err();
        assert!(error.contains("SHA-256"), "{error}");
        assert_eq!(runtime.snapshot()["integrityState"], "invalid");
        assert!(!runtime.inner.directory.join("pids").exists());
        // A fresh process also hashes bytes, never trusting the persisted manifest.
        let reopened = fixture_runtime(context(temp.path()));
        initialized(&reopened).await;
        assert_eq!(reopened.snapshot()["modelState"], "error");
        std::fs::remove_file(&model).unwrap();
        initialized(&runtime).await;
        assert_eq!(runtime.snapshot()["modelState"], "error");
        std::fs::write(&model, b"test-only-model").unwrap();
        initialized(&runtime).await;
        assert_eq!(runtime.snapshot()["modelState"], "ready");
        assert!(runtime.snapshot()["error"].is_null());
        assert_eq!(
            infer(&runtime, "fixture").await.unwrap()[0],
            vec![0.0625; 256]
        );
        runtime.shutdown().await;
        wait_dead(pids(&runtime)).await;
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn manifest_identity_and_runtime_file_digests_are_enforced() {
        let (_temp, runtime) = protocol_fixture("ok");
        let path = runtime.inner.directory.join("installed.json");
        let files =
            runtime_integrity::runtime_files(&runtime.inner.directory.join("runtime"), &|| Ok(()))
                .unwrap();
        let valid = runtime_integrity::manifest(&platform().unwrap(), runtime.inner.model, files);
        for (key, value) in [
            ("revision", json!("another-revision")),
            ("runtime", json!("other-runtime")),
            ("modelFile", json!("Q4.gguf")),
            ("modelBytes", json!(1)),
            ("modelSha256", json!("wrong")),
            ("modelKey", json!("wrong-space")),
            ("schema", json!(99)),
            ("runtimeFiles", json!({})),
        ] {
            let mut invalid = valid.clone();
            invalid[key] = value;
            write_json_atomic(&path, &invalid).unwrap();
            initialized(&runtime).await;
            assert_eq!(runtime.snapshot()["modelState"], "error", "accepted {key}");
        }
        write_json_atomic(&path, &valid).unwrap();
        initialized(&runtime).await;
        assert_eq!(runtime.snapshot()["modelState"], "ready");
        let binary = runtime.inner.directory.join("runtime/llama-server");
        let original = std::fs::read(&binary).unwrap();
        let mut changed = original.clone();
        changed[0] = b'!';
        std::fs::write(&binary, changed).unwrap();
        assert!(infer(&runtime, "fixture")
            .await
            .unwrap_err()
            .contains("runtime failed"));
        std::fs::write(&binary, original).unwrap();
        initialized(&runtime).await;
        assert_eq!(runtime.snapshot()["modelState"], "ready");
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn launch_exit_and_readiness_timeout_publish_errors_cleanup_and_retry() {
        for mode in ["exit", "unhealthy"] {
            let (_temp, runtime) = protocol_fixture(mode);
            initialized(&runtime).await;
            let error = infer(&runtime, "fixture").await.unwrap_err();
            assert!(
                error.contains(if mode == "exit" { "23" } else { "too long" }),
                "{error}"
            );
            assert_eq!(runtime.snapshot()["error"], error);
            assert_eq!(runtime.snapshot()["modelState"], "ready");
            assert_eq!(runtime.snapshot()["runtimeState"], "error");
            assert!(runtime.inner.process.lock().await.is_none());
            wait_dead(pids(&runtime)).await;
            std::fs::write(runtime.inner.directory.join("mode"), "ok").unwrap();
            assert_eq!(
                infer(&runtime, "fixture").await.unwrap()[0],
                vec![0.0625; 256]
            );
            assert!(runtime.snapshot()["error"].is_null());
            assert_eq!(runtime.snapshot()["runtimeState"], "ready");
            runtime.shutdown().await;
            wait_dead(pids(&runtime)).await;
        }
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn protocol_errors_and_request_deadline_cleanup_and_retry() {
        let (_temp, runtime) = protocol_fixture("ok");
        initialized(&runtime).await;
        for text in [
            "http500",
            "missing",
            "duplicate",
            "missingindex",
            "wrongdim",
            "zero",
            "null",
            "nan",
            "hold",
        ] {
            let error = infer(&runtime, text).await.unwrap_err();
            assert_eq!(runtime.snapshot()["error"], error, "{text}");
            assert_eq!(runtime.snapshot()["runtimeState"], "error");
            assert!(runtime.inner.process.lock().await.is_none());
            wait_dead(pids(&runtime)).await;
            assert_eq!(
                infer(&runtime, "fixture").await.unwrap()[0],
                vec![0.0625; 256]
            );
            assert!(runtime.snapshot()["error"].is_null());
        }
        assert_eq!(
            infer(&runtime, "overflow").await.unwrap()[0],
            vec![0.0625; 256]
        );
        let duplicate = runtime
            .embed(
                vec!["duplicate".into(), "second".into()],
                "slow",
                &cancellation(),
            )
            .await
            .unwrap_err();
        assert!(duplicate.contains("incomplete"));
        wait_dead(pids(&runtime)).await;
        let batch = runtime
            .embed(
                vec!["first".into(), "second".into()],
                "slow",
                &cancellation(),
            )
            .await
            .unwrap();
        assert_eq!(batch.len(), 2);
        runtime.shutdown().await;
        wait_dead(pids(&runtime)).await;
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn aborted_active_request_and_shutdown_release_the_owned_lease() {
        for shutdown in [false, true] {
            let (_temp, runtime) = protocol_fixture("ok");
            initialized(&runtime).await;
            let this = runtime.clone();
            let request = tokio::spawn(async move { infer(&this, "hold").await });
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                while !runtime.inner.directory.join("holding").exists() {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let owned = pids(&runtime);
            if shutdown {
                tokio::time::timeout(std::time::Duration::from_secs(3), runtime.shutdown())
                    .await
                    .unwrap();
                assert!(request.await.unwrap().is_err());
            } else {
                request.abort();
                assert!(request.await.unwrap_err().is_cancelled());
            }
            wait_dead(owned).await;
            assert!(runtime.inner.process.lock().await.is_none());
            assert!(runtime.snapshot()["error"].is_null());
            assert_eq!(runtime.snapshot()["runtimeState"], "stopped");
        }
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn aborted_shutdown_future_still_closes_an_idle_owned_lease() {
        let (_temp, runtime) = protocol_fixture("ok");
        initialized(&runtime).await;
        infer(&runtime, "fixture").await.unwrap();
        let owned = pids(&runtime);
        let this = runtime.clone();
        let task = tokio::spawn(async move { this.shutdown().await });
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while !runtime.inner.closed.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        task.abort();
        let _ = task.await;
        wait_dead(owned).await;
        assert!(runtime.inner.process.lock().await.is_none());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn staged_install_rejects_bad_or_cancelled_replacement_and_can_repair() {
        let (_temp, runtime) = protocol_fixture("ok");
        initialized(&runtime).await;
        let old_model = std::fs::read(runtime.inner.directory.join(MODEL_FILE)).unwrap();
        let staging = tempfile::tempdir_in(runtime.inner.directory.parent().unwrap()).unwrap();
        let destination = staging.path();
        std::fs::create_dir_all(destination.join("runtime")).unwrap();
        std::fs::copy(
            runtime.inner.directory.join("runtime/llama-server"),
            destination.join("runtime/llama-server"),
        )
        .unwrap();
        std::fs::write(destination.join(MODEL_FILE), b"bad!-only-model").unwrap();
        let files =
            runtime_integrity::runtime_files(&destination.join("runtime"), &|| Ok(())).unwrap();
        write_json_atomic(
            &destination.join("installed.json"),
            &runtime_integrity::manifest(&platform().unwrap(), runtime.inner.model, files),
        )
        .unwrap();
        assert!(runtime
            .publish_install(destination, &cancellation())
            .await
            .is_err());
        assert_eq!(
            std::fs::read(runtime.inner.directory.join(MODEL_FILE)).unwrap(),
            old_model
        );
        std::fs::write(destination.join(MODEL_FILE), &old_model).unwrap();
        let cancel = cancellation();
        cancel.generation.fetch_add(1, Ordering::SeqCst);
        assert!(runtime.publish_install(destination, &cancel).await.is_err());
        assert_eq!(
            std::fs::read(runtime.inner.directory.join(MODEL_FILE)).unwrap(),
            old_model
        );
        // The real install's publication seam repairs a damaged installed artifact.
        std::fs::write(runtime.inner.directory.join(MODEL_FILE), b"corrupt").unwrap();
        runtime
            .publish_install(destination, &cancellation())
            .await
            .unwrap();
        initialized(&runtime).await;
        assert_eq!(
            runtime.snapshot()["runtimeIntegrity"],
            "installed-file-sha256"
        );
        assert_eq!(runtime.snapshot()["modelState"], "ready");
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn installer_verifies_archive_and_model_and_redownload_repairs_without_destroying_old() {
        let (_temp, runtime) = protocol_fixture("ok");
        initialized(&runtime).await;
        let original = std::fs::read(runtime.inner.directory.join(MODEL_FILE)).unwrap();
        let archive_temp = tempfile::tempdir().unwrap();
        let archive = archive_temp.path().join("runtime.tar.gz");
        let output = std::process::Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(runtime.inner.directory.join("runtime"))
            .arg("llama-server")
            .output()
            .unwrap();
        assert!(output.status.success());
        let bytes = std::fs::read(archive).unwrap();
        let digest: &'static str =
            Box::leak(format!("{:x}", Sha256::digest(&bytes)).into_boxed_str());
        let manifest = || Manifest {
            file: platform().unwrap().file,
            bytes: bytes.len() as u64,
            sha: digest,
        };
        for bad_archive in [true, false] {
            let (runtime_url, archive_task) = serve(if bad_archive {
                b"bad archive".to_vec()
            } else {
                bytes.clone()
            })
            .await;
            let (model_url, model_task) = serve(b"bad!-only-model".to_vec()).await;
            assert!(runtime
                .install_from(manifest(), &runtime_url, &model_url, &cancellation())
                .await
                .is_err());
            archive_task.await.unwrap();
            model_task.abort();
            let _ = model_task.await;
            assert_eq!(
                std::fs::read(runtime.inner.directory.join(MODEL_FILE)).unwrap(),
                original
            );
        }
        std::fs::write(runtime.inner.directory.join(MODEL_FILE), b"corrupt").unwrap();
        initialized(&runtime).await;
        assert_eq!(runtime.snapshot()["modelState"], "error");
        let (runtime_url, archive_task) = serve(bytes.clone()).await;
        let (model_url, model_task) = serve(original).await;
        runtime.patch(json!({"modelState":"downloading"}));
        runtime
            .install_from(manifest(), &runtime_url, &model_url, &cancellation())
            .await
            .unwrap();
        archive_task.await.unwrap();
        model_task.await.unwrap();
        assert_eq!(runtime.snapshot()["modelState"], "ready");
        assert!(runtime.snapshot()["error"].is_null());
        // No .part, staging directory or backup remains after real installation.
        assert_eq!(
            std::fs::read_dir(runtime.inner.directory.parent().unwrap())
                .unwrap()
                .count(),
            1
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn cancelled_download_keeps_previous_install_and_removes_partial_staging() {
        let (_temp, runtime) = protocol_fixture("ok");
        initialized(&runtime).await;
        let original = std::fs::read(runtime.inner.directory.join(MODEL_FILE)).unwrap();
        // An open HTTP response that never sends its body, cancelled through the
        // public API and the actual installer (not an installer mirror).
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/fixture", listener.local_addr().unwrap());
        let (accepted, observed) = tokio::sync::oneshot::channel();
        let server_task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut input = [0; 2048];
            let _ = socket.read(&mut input).await;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100000\r\n\r\nx")
                .await
                .unwrap();
            let _ = accepted.send(());
            std::future::pending::<()>().await;
        });
        let expected = runtime.inner.download_gen.load(Ordering::SeqCst);
        let cancel = Cancellation {
            generation: runtime.inner.download_gen.clone(),
            expected,
        };
        let this = runtime.clone();
        runtime.patch(json!({"modelState":"downloading"}));
        *runtime.inner.download.lock().await = Some(tokio::spawn(async move {
            assert!(this
                .install_from(
                    platform().unwrap(),
                    &url,
                    "http://127.0.0.1:1/unused",
                    &cancel
                )
                .await
                .is_err());
        }));
        observed.await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(3), runtime.cancel_download())
            .await
            .unwrap()
            .unwrap();
        server_task.abort();
        let _ = server_task.await;
        assert_eq!(
            std::fs::read(runtime.inner.directory.join(MODEL_FILE)).unwrap(),
            original
        );
        assert_eq!(runtime.snapshot()["modelState"], "ready");
        assert!(runtime.snapshot()["error"].is_null());
        assert_eq!(
            std::fs::read_dir(runtime.inner.directory.parent().unwrap())
                .unwrap()
                .count(),
            1
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn initialization_and_keyword_fallback_do_not_wait_for_background_hash() {
        let (_temp, runtime) = protocol_fixture("ok");
        let hold = runtime.inner.verification.lock().await;
        tokio::time::timeout(std::time::Duration::from_millis(100), runtime.initialize())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(runtime.snapshot()["integrityState"], "verifying");
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            infer(&runtime, "fixture"),
        )
        .await;
        drop(hold);
        assert!(
            result.is_ok(),
            "keyword fallback waited for checksum completion"
        );
        assert!(result.unwrap().is_err());
        assert!(runtime.snapshot()["error"].is_null());
        initialized(&runtime).await;
        assert_eq!(runtime.snapshot()["modelState"], "ready");
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn cancellation_while_waiting_for_another_request_is_prompt_and_not_an_error() {
        let (_temp, runtime) = protocol_fixture("ok");
        initialized(&runtime).await;
        let hold = runtime.inner.process.lock().await;
        let cancel = cancellation();
        let generation = cancel.generation.clone();
        let this = runtime.clone();
        let mut task =
            tokio::spawn(async move { this.embed(vec!["fixture".into()], "slow", &cancel).await });
        tokio::task::yield_now().await;
        generation.fetch_add(1, Ordering::SeqCst);
        let result = tokio::time::timeout(std::time::Duration::from_millis(200), &mut task).await;
        drop(hold);
        if result.is_err() {
            task.abort();
            let _ = task.await;
        }
        assert!(
            result.is_ok(),
            "cancelled query waited for a different inference"
        );
        assert!(result.unwrap().unwrap().is_err());
        assert!(runtime.snapshot()["error"].is_null());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn verified_cache_is_process_local_and_does_not_rehash_unchanged_files() {
        use std::cell::Cell;
        let (_temp, runtime) = protocol_fixture("ok");
        let calls = Cell::new(0);
        let check = || {
            calls.set(calls.get() + 1);
            Ok(())
        };
        let manifest = platform().unwrap();
        let verified = runtime_integrity::verify(
            &runtime.inner.directory,
            &manifest,
            runtime.inner.model,
            None,
            &check,
        )
        .unwrap()
        .unwrap();
        assert!(calls.get() > 1); // actual model + runtime hashing
        calls.set(0);
        runtime_integrity::verify(
            &runtime.inner.directory,
            &manifest,
            runtime.inner.model,
            Some(&verified),
            &check,
        )
        .unwrap()
        .unwrap();
        assert_eq!(calls.get(), 1); // metadata only; no hash/read loop
        calls.set(0);
        runtime_integrity::verify(
            &runtime.inner.directory,
            &manifest,
            runtime.inner.model,
            None,
            &check,
        )
        .unwrap()
        .unwrap();
        assert!(calls.get() > 1); // new app start must hash again
        let other = ModelIdentity {
            descriptor: &runtime_models::Q8,
            bytes: runtime.inner.model.bytes,
            sha: "another-model-with-the-same-length",
        };
        assert!(runtime_integrity::verify(
            &runtime.inner.directory,
            &manifest,
            other,
            Some(&verified),
            &check
        )
        .is_err());
    }
    async fn serve(bytes: Vec<u8>) -> (String, tokio::task::JoinHandle<()>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 2048];
            let _ = socket.read(&mut request).await;
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            );
            socket.write_all(headers.as_bytes()).await.unwrap();
            socket.write_all(&bytes).await.unwrap();
        });
        (format!("http://{address}/fixture"), task)
    }
    #[tokio::test]
    async fn downloads_require_exact_sha_and_size_and_leave_no_part_file() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = Runtime::new(context(temp.path()));
        let cancel = Cancellation {
            generation: Arc::new(AtomicU64::new(0)),
            expected: 0,
        };
        let bytes = b"verified native model fixture".to_vec();
        let path = temp.path().join("model.gguf");
        let (url, task) = serve(bytes.clone()).await;
        runtime
            .download_file(
                &url,
                &path,
                &format!("{:x}", Sha256::digest(&bytes)),
                bytes.len() as u64,
                &cancel,
            )
            .await
            .unwrap();
        task.await.unwrap();
        assert_eq!(tokio::fs::read(&path).await.unwrap(), bytes);
        let (url, task) = serve(bytes.clone()).await;
        assert!(runtime
            .download_file(
                &url,
                &path,
                "incorrect checksum",
                bytes.len() as u64,
                &cancel
            )
            .await
            .unwrap_err()
            .contains("checksum"));
        task.await.unwrap();
        assert_eq!(tokio::fs::read(&path).await.unwrap(), bytes);
        assert!(!path.with_extension("gguf.part").exists());
        let (url, task) = serve(bytes).await;
        assert!(runtime
            .download_file(&url, &path, "anything", 1, &cancel)
            .await
            .unwrap_err()
            .contains("expected size"));
        task.await.unwrap();
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn readiness_cancellation_and_aborted_future_terminate_the_server_tree() {
        use std::os::unix::fs::PermissionsExt;
        for abort_future in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let runtime = fixture_runtime(context(temp.path()));
            installed_fixture(&runtime);
            let fixture = child_supervisor::tests::fixture_executable();
            *runtime.inner.supervisor.lock().unwrap() = Some(fixture.clone());
            let runtime_dir = runtime.inner.directory.join("runtime");
            std::fs::create_dir_all(&runtime_dir).unwrap();
            let executable = runtime_dir.join("llama-server");
            // Launch an inert native tree which never serves health, so cancellation
            // occurs while startup owns an unpublished Process rather than the cache.
            std::fs::write(
                &executable,
                format!(
                    "#!/usr/bin/env python3\nimport os\nos.execv({0},[{0},'tree',{1}])\n",
                    serde_json::to_string(&fixture).unwrap(),
                    serde_json::to_string(temp.path()).unwrap(),
                ),
            )
            .unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
            runtime.patch(json!({"modelState":"ready"}));
            let generation = Arc::new(AtomicU64::new(0));
            let cancel = Cancellation {
                generation: generation.clone(),
                expected: 0,
            };
            let this = runtime.clone();
            let task =
                tokio::spawn(
                    async move { this.embed(vec!["fixture".into()], "slow", &cancel).await },
                );
            let read_pid = |name: &str| {
                std::fs::read_to_string(temp.path().join(name))
                    .ok()
                    .and_then(|s| s.parse::<u32>().ok())
            };
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                while read_pid("root").is_none() || read_pid("leaf").is_none() {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            })
            .await
            .unwrap();
            let root = read_pid("root").unwrap();
            let leaf = read_pid("leaf").unwrap();
            if abort_future {
                task.abort();
                assert!(task.await.unwrap_err().is_cancelled());
            } else {
                generation.fetch_add(1, Ordering::SeqCst);
                assert!(
                    tokio::time::timeout(std::time::Duration::from_secs(5), task)
                        .await
                        .unwrap()
                        .unwrap()
                        .is_err()
                );
            }
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                while child_supervisor::tests::alive(root) || child_supervisor::tests::alive(leaf) {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            })
            .await
            .unwrap();
            assert!(runtime.inner.process.lock().await.is_none());
        }
    }

    #[tokio::test]
    async fn service_start_owns_one_idle_supervisor_per_runtime_and_shutdown_joins() {
        let temp = tempfile::tempdir().unwrap();
        let core = super::super::KnowledgeCore::new(context(temp.path()));
        core.start().await.unwrap();
        let ids: Vec<_> = core
            .inner
            .runtimes
            .iter()
            .map(|runtime| {
                runtime
                    .inner
                    .idle_task
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .0
                    .as_ref()
                    .unwrap()
                    .id()
            })
            .collect();
        core.start().await.unwrap();
        for (runtime, id) in core.inner.runtimes.iter().zip(ids) {
            assert_eq!(
                runtime
                    .inner
                    .idle_task
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .0
                    .as_ref()
                    .unwrap()
                    .id(),
                id
            );
        }
        core.shutdown().await.unwrap();
        for runtime in &core.inner.runtimes {
            assert!(runtime.inner.idle_task.lock().unwrap().is_none());
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn idle_supervisor_does_not_retain_last_runtime_owner() {
        let (_temp, runtime) = protocol_fixture("ok");
        initialized(&runtime).await;
        infer(&runtime, "fixture").await.unwrap();
        runtime.start_idle_supervisor();
        let owned = pids(&runtime);
        let inner = Arc::downgrade(&runtime.inner);
        drop(runtime);
        wait_dead(owned).await;
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while inner.upgrade().is_some() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("idle supervisor must not retain the last owner");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn idle_supervisor_is_independent_owned_and_protects_active_leases() {
        let (_temp, runtime) = protocol_fixture("warming");
        let core = super::super::KnowledgeCore::new(context(_temp.path()));
        let indexing_owner = core.inner.run.lock().await;
        initialized(&runtime).await;
        runtime.start_idle_supervisor();
        let task_id = runtime
            .inner
            .idle_task
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .0
            .as_ref()
            .unwrap()
            .id();
        runtime.start_idle_supervisor();
        assert_eq!(
            runtime
                .inner
                .idle_task
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .0
                .as_ref()
                .unwrap()
                .id(),
            task_id
        );
        runtime.prewarm("slow").await.unwrap();
        wait_for_file(&runtime, "pids").await;
        let owned = pids(&runtime);
        // The startup owns the mutex. An idle check must return without
        // waiting for startup or taking/closing its unpublished process.
        tokio::time::timeout(std::time::Duration::from_millis(100), runtime.stop_idle())
            .await
            .unwrap();
        assert!(owned.iter().all(|pid| child_supervisor::tests::alive(*pid)));
        wait_warm(&runtime).await;
        let this = runtime.clone();
        let cancel = cancellation();
        let generation = cancel.generation.clone();
        let request =
            tokio::spawn(async move { this.embed(vec!["hold".into()], "slow", &cancel).await });
        wait_for_file(&runtime, "holding").await;
        tokio::time::timeout(std::time::Duration::from_millis(100), runtime.stop_idle())
            .await
            .unwrap();
        assert!(!request.is_finished());
        assert!(owned.iter().all(|pid| child_supervisor::tests::alive(*pid)));
        generation.fetch_add(1, Ordering::SeqCst);
        assert!(request.await.unwrap().is_err());
        wait_dead(owned).await;
        infer(&runtime, "fixture").await.unwrap();
        let owned = pids(&runtime);
        // A fresh last-used timestamp survives a check; expiry is injected
        // rather than waiting two minutes or changing the idle policy.
        runtime.stop_idle().await;
        assert!(runtime.inner.process.lock().await.is_some());
        runtime
            .inner
            .process
            .lock()
            .await
            .as_mut()
            .unwrap()
            .last_used = now_ms().saturating_sub(120001);
        let stopped = tokio::time::timeout(std::time::Duration::from_secs(4), async {
            loop {
                if runtime.inner.process.lock().await.is_none() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await;
        // Always clean fixture children before asserting the red verdict.
        runtime.shutdown().await;
        wait_dead(owned).await;
        assert!(runtime.inner.idle_task.lock().unwrap().is_none());
        runtime.start_idle_supervisor();
        assert!(runtime.inner.idle_task.lock().unwrap().is_none());
        drop(indexing_owner);
        stopped.expect("idle termination must run while source indexing owns run");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn native_inference_uses_auth_normalizes_768_to_256_and_stops_on_idle() {
        use std::os::unix::fs::PermissionsExt;
        if platform().is_none() {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let directory = ctx.paths.data_dir.join("knowledge/models/embeddinggemma-2");
        let runtime_dir = directory.join("runtime");
        std::fs::create_dir_all(&runtime_dir).unwrap();
        std::fs::write(directory.join(MODEL_FILE), b"test-only-model").unwrap();
        write_json_atomic(
            &directory.join("installed.json"),
            &json!({"revision":REVISION,"runtime":platform().unwrap().file}),
        )
        .unwrap();
        let executable = runtime_dir.join("llama-server");
        std::fs::write(&executable, PROTOCOL_SERVER).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let runtime = fixture_runtime(ctx);
        *runtime.inner.supervisor.lock().unwrap() =
            Some(child_supervisor::tests::fixture_executable());
        initialized(&runtime).await;
        assert_eq!(runtime.snapshot()["modelState"], "ready");
        let cancel = Cancellation {
            generation: Arc::new(AtomicU64::new(0)),
            expected: 0,
        };
        let vector = runtime
            .embed(
                vec!["task: search result | query: fixture".into()],
                "slow",
                &cancel,
            )
            .await
            .unwrap();
        assert_eq!(vector[0], vec![0.0625; 256]);
        let pids = || {
            std::fs::read_to_string(directory.join("pids"))
                .unwrap()
                .split_whitespace()
                .map(|s| s.parse::<u32>().unwrap())
                .collect::<Vec<_>>()
        };
        let wait_dead = |pids: Vec<u32>| async move {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                while pids.iter().any(|pid| child_supervisor::tests::alive(*pid)) {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            })
            .await
            .unwrap();
        };
        let idle_pids = pids();
        {
            let mut p = runtime.inner.process.lock().await;
            p.as_mut().unwrap().last_used = now_ms().saturating_sub(120001);
        }
        runtime.stop_idle().await;
        assert!(runtime.inner.process.lock().await.is_none());
        wait_dead(idle_pids).await;

        // Cancel an active HTTP inference after readiness, rather than only startup.
        let generation = Arc::new(AtomicU64::new(0));
        let request_cancel = Cancellation {
            generation: generation.clone(),
            expected: 0,
        };
        let this = runtime.clone();
        let task = tokio::spawn(async move {
            this.embed(vec!["hold".into()], "slow", &request_cancel)
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while !directory.join("holding").exists() {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let cancelled_pids = pids();
        generation.fetch_add(1, Ordering::SeqCst);
        assert!(task.await.unwrap().is_err());
        wait_dead(cancelled_pids).await;
        assert!(runtime.inner.process.lock().await.is_none());

        runtime
            .embed(vec!["fixture".into()], "slow", &cancel)
            .await
            .unwrap();
        let shutdown_pids = pids();
        runtime.shutdown().await;
        wait_dead(shutdown_pids).await;
        assert!(directory.join(MODEL_FILE).is_file());

        let runtime = fixture_runtime(context(temp.path()));
        *runtime.inner.supervisor.lock().unwrap() =
            Some(child_supervisor::tests::fixture_executable());
        initialized(&runtime).await;
        runtime
            .embed(vec!["fixture".into()], "slow", &cancel)
            .await
            .unwrap();
        let removed_pids = pids();
        runtime.remove().await.unwrap();
        wait_dead(removed_pids).await;
        assert!(!directory.exists());
    }
}
