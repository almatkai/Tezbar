//! Demand-launched JavaScript compatibility process. Never dispatch core IPC here.
use super::common::{CoreContext, CoreResult};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

pub const CHANNELS: &[&str] = &[
    "extension:run-command",
    "extension:invoke-action",
    "extension:search-text-changed",
    "extension:refresh-session",
    "extension:dispose-session",
    "extension:load-more",
    "extension:install",
    "extensions:install",
    "extensions:reinstall",
];

const HOST_CHANNELS: &[&str] = &[
    "core:apps:list",
    "core:ai:extension-ask",
    "core:platform:selected-text",
    "core:platform:suppress-blur",
    "core:platform:visibility",
    "core:platform:open-file",
    "core:platform:reveal-file",
    "core:platform:confirm",
    "core:platform:clipboard-write",
    "core:platform:paste",
    "clipboard:read",
    "clipboard:write",
    "clipboard:write-png",
    "shell:open",
];
const MAX_FRAME: usize = 16 * 1024 * 1024;
type Pending = Arc<Mutex<HashMap<String, oneshot::Sender<CoreResult>>>>;

struct Process {
    sender: mpsc::Sender<Value>,
    pending: Pending,
    alive: Arc<AtomicBool>,
    stop: Option<oneshot::Sender<()>>,
    finished: tokio::task::JoinHandle<()>,
}

struct RunGrant {
    extension: String,
    preferences: bool,
}
struct PendingRequest {
    id: String,
    pending: Pending,
    runs: Arc<Mutex<HashMap<String, RunGrant>>>,
}
impl Drop for PendingRequest {
    fn drop(&mut self) {
        if let Ok(mut runs) = self.runs.lock() {
            runs.remove(&self.id);
        }
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&self.id);
        }
    }
}

#[derive(Clone)]
pub struct WorkerLaunch {
    bun: PathBuf,
    directory: PathBuf,
}
impl WorkerLaunch {
    /// Explicit host construction capability, not an IPC/config/environment override.
    /// Isolated fixtures can use the real worker without replacing the shared bundle.
    pub fn new(bun: PathBuf, directory: PathBuf) -> Result<Self, String> {
        let bun =
            std::fs::canonicalize(bun).map_err(|_| "Extension worker executable is unavailable")?;
        let directory = std::fs::canonicalize(directory)
            .map_err(|_| "Extension worker directory is unavailable")?;
        if !bun.is_file() || !directory.join("extension-worker.js").is_file() {
            return Err(
                "Extension worker requires an executable file and a built worker bundle".into(),
            );
        }
        Ok(Self { bun, directory })
    }
}
#[derive(Clone)]
pub struct ExtensionWorker {
    ctx: CoreContext,
    process: Arc<tokio::sync::Mutex<Option<Process>>>,
    launch: Option<WorkerLaunch>,
    sessions: Arc<Mutex<HashMap<String, (String, String, String)>>>,
    status_signatures: Arc<Mutex<HashMap<String, Option<(u64, u64)>>>>,
    status_revision: Arc<AtomicU64>,
    // One refresh owner per session; true coalesces invalidations received while
    // that owner's bounded icon resolution is awaiting admission or decoding.
    status_refreshing: Arc<Mutex<HashMap<String, bool>>>,
    in_flight_runs: Arc<Mutex<HashMap<String, RunGrant>>>,
}

impl ExtensionWorker {
    pub fn new(ctx: CoreContext) -> Self {
        Self {
            ctx,
            process: Arc::new(tokio::sync::Mutex::new(None)),
            launch: None,
            sessions: Default::default(),
            status_signatures: Default::default(),
            status_revision: Default::default(),
            status_refreshing: Default::default(),
            in_flight_runs: Default::default(),
        }
    }
    pub fn with_launch(ctx: CoreContext, launch: WorkerLaunch) -> Self {
        Self {
            launch: Some(launch),
            ..Self::new(ctx)
        }
    }

    pub async fn request(&self, channel: &str, args: Vec<Value>) -> CoreResult {
        if !CHANNELS.contains(&channel) && channel != "core:extension-status-action" {
            return Err(format!("Channel is not an extension operation: {channel}"));
        }
        if channel == "extension:dispose-session" {
            let id = args
                .first()
                .and_then(|v| v["sessionId"].as_str())
                .ok_or("Session identity required")?;
            let exists = self
                .sessions
                .lock()
                .map(|s| s.contains_key(id))
                .unwrap_or(false);
            self.remove_sdk_session(id);
            if !exists {
                return Ok(json!(false));
            }
        }
        let (sender, pending, alive) = {
            let mut process = self.process.lock().await;
            if !process
                .as_ref()
                .is_some_and(|p| p.alive.load(Ordering::Acquire))
            {
                if let Some(previous) = process.take() {
                    let _ = previous.finished.await;
                }
                *process = Some(self.launch().await?);
            }
            let process = process.as_ref().ok_or("Extension Worker did not start")?;
            (
                process.sender.clone(),
                process.pending.clone(),
                process.alive.clone(),
            )
        };
        let id = uuid::Uuid::new_v4().to_string();
        let (reply, result) = oneshot::channel();
        {
            let mut pending = pending.lock().map_err(|e| e.to_string())?;
            // Serialize registration with the supervisor's final drain. A caller
            // can hold an old sender when the process dies between lock scopes.
            if !alive.load(Ordering::Acquire) {
                return Err("Extension Worker stopped before request registration".into());
            }
            if pending.len()
                >= if channel == "extension:dispose-session" {
                    144
                } else {
                    128
                }
            {
                return Err("Extension operation capacity reached".into());
            }
            pending.insert(id.clone(), reply);
        }
        if channel == "extension:run-command" {
            if let Some(extension) = args.first().and_then(|a| a["extensionId"].as_str()) {
                self.in_flight_runs
                    .lock()
                    .map_err(|_| "SDK run owner unavailable")?
                    .insert(
                        id.clone(),
                        RunGrant {
                            extension: extension.into(),
                            preferences: false,
                        },
                    );
            }
        }
        let _request = PendingRequest {
            id: id.clone(),
            pending,
            runs: self.in_flight_runs.clone(),
        };
        let timeout = if channel.ends_with("install") {
            Duration::from_secs(900)
        } else {
            Duration::from_secs(120)
        };
        let request_args = args.clone();
        let mut response = tokio::time::timeout(timeout, async {
            sender
                .send(json!({"type":"invoke", "id":id, "channel":channel, "payload":args}))
                .await
                .map_err(|_| "Extension Worker disconnected".to_string())?;
            result
                .await
                .map_err(|_| "Extension Worker stopped before replying".to_string())?
        })
        .await
        .map_err(|_| format!("Extension operation timed out: {channel}"))??;
        if channel == "extension:run-command" {
            let preferences = self
                .in_flight_runs
                .lock()
                .map(|runs| runs.get(&id).is_some_and(|run| run.preferences))
                .unwrap_or(false);
            if preferences {
                if let Some(reply) = response.as_object_mut() {
                    reply.insert("preferencesRequested".into(), json!(true));
                }
            }
            let extension = request_args[0]["extensionId"].as_str().unwrap_or("");
            let command = request_args[0]["commandName"].as_str().unwrap_or("");
            let old = self
                .sessions
                .lock()
                .map(|sessions| {
                    sessions
                        .iter()
                        .filter(|(id, (e, c, _))| {
                            e == extension
                                && c == command
                                && Some(id.as_str()) != response["sessionId"].as_str()
                        })
                        .map(|(id, _)| id.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            for id in old {
                self.remove_sdk_session(&id);
            }
        }
        if channel == "extension:run-command"
            && response["ok"] == true
            && response["mode"] == "view"
        {
            let extension = request_args[0]["extensionId"]
                .as_str()
                .ok_or("Missing extension")?;
            let session = response["sessionId"].as_str().ok_or("Missing session")?;
            if response["extensionId"] != extension {
                return Err("Worker identity mismatch".into());
            }
            let mut sessions = self
                .sessions
                .lock()
                .map_err(|_| "Session owner unavailable")?;
            if sessions.len() >= 128 && !sessions.contains_key(session) {
                return Err("Too many SDK sessions".into());
            }
            sessions.insert(
                session.into(),
                (
                    extension.into(),
                    request_args[0]["commandName"].as_str().unwrap_or("").into(),
                    format!("sdk-{}", uuid::Uuid::new_v4()),
                ),
            );
        }
        if channel == "extension:dispose-session" {
            self.remove_sdk_session(request_args[0]["sessionId"].as_str().unwrap_or(""));
        }
        self.sync_sdk_status(&response).await?;
        Ok(response)
    }

    async fn launch(&self) -> Result<Process, String> {
        self.ctx
            .emit("extension-worker-status", json!({"phase":"starting"}));
        let (bun, directory) = if let Some(launch) = &self.launch {
            (launch.bun.clone(), launch.directory.clone())
        } else {
            let data = self.ctx.paths.data_dir.clone();
            // Discovery remains lazy and exclusive to extension execution.
            let bun = tokio::task::spawn_blocking(move || crate::bun::locate_bun(&data))
                .await
                .map_err(|e| e.to_string())??;
            let directory = if cfg!(debug_assertions) {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dist-backend")
            } else {
                self.ctx.paths.resource_dir.join("dist-backend")
            };
            (bun, directory)
        };
        let entry = directory.join("extension-worker.js");
        if !entry.is_file() {
            return Err(format!(
                "Extension Worker bundle is missing: {}",
                entry.display()
            ));
        }
        let mut command = tokio::process::Command::new(&bun);
        command
            .arg(&entry)
            .current_dir(&directory)
            .env("APPDATA_DIR", &self.ctx.paths.data_dir)
            .env("TEZBAR_CONFIG_DIR", &self.ctx.paths.config_dir)
            .env("TEZBAR_RESOURCE_DIR", &self.ctx.paths.resource_dir)
            .env("TEMP_DIR", &self.ctx.paths.temp_dir)
            .env("APP_VERSION", env!("CARGO_PKG_VERSION"))
            .env("IS_TAURI", "true")
            .env("TEZBAR_EXTENSION_WORKER", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let platform = if cfg!(target_os = "windows") {
            "win32"
        } else if cfg!(target_os = "macos") {
            "darwin"
        } else {
            "linux"
        };
        let arch = if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            "x64"
        };
        let esbuild = directory
            .join("node_modules/@esbuild")
            .join(format!("{platform}-{arch}"))
            .join(if cfg!(target_os = "windows") {
                "esbuild.exe"
            } else {
                "bin/esbuild"
            });
        if esbuild.is_file() {
            command.env("ESBUILD_BINARY_PATH", esbuild);
        }
        #[cfg(unix)]
        command.process_group(0);
        #[cfg(target_os = "windows")]
        command.creation_flags(0x08000000);
        // Bun 1.2.5 does not wake stdin stream reads on an open Unix pipe.
        // Match Node's spawn transport: a socketpair remains interactive without
        // closing stdin after each frame (which would terminate the worker).
        #[cfg(unix)]
        let (stdin, worker_stdin) = std::os::unix::net::UnixStream::pair()
            .map_err(|e| format!("Could not create Extension Worker transport: {e}"))?;
        #[cfg(unix)]
        command.stdin(Stdio::from(std::os::fd::OwnedFd::from(worker_stdin)));
        let mut child = command
            .spawn()
            .map_err(|e| format!("Could not start Extension Worker: {e}"))?;
        // Command retains its configured Stdio; close our copy of the child end.
        drop(command);
        let pid = child.id();
        #[cfg(unix)]
        stdin.set_nonblocking(true).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        let mut stdin = tokio::net::UnixStream::from_std(stdin).map_err(|e| e.to_string())?;
        #[cfg(not(unix))]
        let mut stdin = child
            .stdin
            .take()
            .ok_or("Extension Worker stdin unavailable")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Extension Worker stdout unavailable")?;
        let stderr = child
            .stderr
            .take()
            .ok_or("Extension Worker stderr unavailable")?;
        let (sender, mut messages) = mpsc::channel::<Value>(64);
        let (stop, stopped) = oneshot::channel::<()>();
        let (ready, readiness) = oneshot::channel();
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        let output_pending = pending.clone();
        let output_sender = sender.clone();
        let ctx = self.ctx.clone();
        let (cancel_output, mut output_cancelled) = oneshot::channel::<()>();
        let ui_sessions = self.sessions.clone();
        let in_flight_runs = self.in_flight_runs.clone();
        let status_signatures = self.status_signatures.clone();
        let status_refreshing = self.status_refreshing.clone();
        let mut output = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut ready = Some(ready);
            let mut host_requests = tokio::task::JoinSet::new();
            loop {
                let frame = tokio::select! {
                    _ = &mut output_cancelled => break,
                    frame = read_frame(&mut reader) => match frame {
                        Ok(Some(frame)) => frame,
                        _ => break,
                    },
                };
                while host_requests.try_join_next().is_some() {}
                let Ok(message) = serde_json::from_slice::<Value>(&frame) else {
                    continue;
                };
                match message["type"].as_str() {
                    Some("ready") if message["protocol"] == 1 => {
                        if let Some(ready) = ready.take() {
                            let _ = ready.send(());
                        }
                    }
                    Some("reply") => {
                        if let Some(id) = message["id"].as_str() {
                            if let Ok(mut pending) = output_pending.lock() {
                                if let Some(reply) = pending.remove(id) {
                                    let result = match message.get("error") {
                                        Some(error) => Err(error
                                            .as_str()
                                            .unwrap_or("Extension failed")
                                            .to_string()),
                                        None => Ok(message["result"].clone()),
                                    };
                                    let _ = reply.send(result);
                                }
                            }
                        }
                    }
                    Some("core-request") => {
                        let sender = output_sender.clone();
                        let ctx = ctx.clone();
                        let ui_sessions = ui_sessions.clone();
                        let runs = in_flight_runs.clone();
                        if host_requests.len() >= 128 {
                            let _=sender.send(json!({"type":"core-reply","id":message["id"],"error":"Extension host capacity reached"})).await;
                            continue;
                        }
                        host_requests.spawn(async move {
                            let channel = message["channel"].as_str().unwrap_or("");
                            let result = if channel == "core:extension-ui" {
                                let body = &message["payload"][0];
                                let valid = ui_sessions
                                    .lock()
                                    .map(|sessions| {
                                        sessions
                                            .contains_key(body["sessionId"].as_str().unwrap_or(""))
                                    })
                                    .unwrap_or(false)
                                    || body["runRequest"]
                                        .as_str()
                                        .zip(body["extensionId"].as_str())
                                        .is_some_and(|(request, extension)| {
                                            runs.lock()
                                                .map(|runs| {
                                                    runs.get(request).is_some_and(|owner| {
                                                        owner.extension == extension
                                                    })
                                                })
                                                .unwrap_or(false)
                                        });
                                if !valid {
                                    Err("Unknown SDK source".into())
                                } else {
                                    match body["kind"].as_str() {
                                        Some(
                                            kind @ ("close" | "preferences" | "clear-search"
                                            | "hud"),
                                        ) => {
                                            use crate::host::ExtensionControl;
                                            let control = match kind {
                                                "close" => ExtensionControl::Close,
                                                "preferences" => ExtensionControl::Preferences,
                                                "clear-search" => ExtensionControl::ClearSearch,
                                                _ => ExtensionControl::Hud(
                                                    body["message"]
                                                        .as_str()
                                                        .filter(|s| s.len() <= 4096)
                                                        .unwrap_or("")
                                                        .into(),
                                                ),
                                            };
                                            let accepted = ctx
                                                .host
                                                .extension_control(
                                                    body["sessionId"].as_str().unwrap_or("").into(),
                                                    body["source"].as_u64().unwrap_or(0),
                                                    control,
                                                )
                                                .await;
                                            // Retain this admitted request as a reply intent. The
                                            // no-view run has no mounted native source yet; its
                                            // caller applies the intent after checking visibility
                                            // and request ownership, using the caller's extension.
                                            if accepted.is_ok()
                                                && kind == "preferences"
                                                && body["source"].as_u64().unwrap_or(0) == 0
                                            {
                                                if let Some(request) = body["runRequest"].as_str() {
                                                    if let Ok(mut runs) = runs.lock() {
                                                        if let Some(run) =
                                                            runs.get_mut(request).filter(|run| {
                                                                Some(run.extension.as_str())
                                                                    == body["extensionId"].as_str()
                                                            })
                                                        {
                                                            run.preferences = true;
                                                        }
                                                    }
                                                }
                                            }
                                            accepted.map(|_| json!(true))
                                        }
                                        _ => Err("Unknown SDK control".into()),
                                    }
                                }
                            } else if HOST_CHANNELS.contains(&channel) {
                                ctx.request(
                                    channel,
                                    message["payload"].as_array().cloned().unwrap_or_default(),
                                )
                                .await
                            } else {
                                Err(format!(
                                    "Host channel is not allowed for extensions: {channel}"
                                ))
                            };
                            let mut reply = json!({"type":"core-reply", "id":message["id"]});
                            match result {
                                Ok(value) => reply["result"] = value,
                                Err(error) => reply["error"] = json!(error),
                            }
                            let _ = sender.send(reply).await;
                        });
                    }
                    Some("event") => {
                        if let Some(channel) = message["channel"].as_str().filter(|c| {
                            c.starts_with("extension:") || *c == "extension-install-status"
                        }) {
                            if channel == "extension:session-disposed" {
                                if let Some(id) = message["payload"]["sessionId"].as_str() {
                                    if let Some((_, _, scope)) =
                                        ui_sessions.lock().ok().and_then(|mut s| s.remove(id))
                                    {
                                        let _ = ctx.host.status_items().remove(&scope);
                                    }
                                    if let Ok(mut s) = status_signatures.lock() {
                                        s.remove(id);
                                    }
                                    if let Ok(mut s) = status_refreshing.lock() {
                                        s.remove(id);
                                    }
                                }
                            }
                            if channel == "extension:session-invalidated" {
                                let id = message["payload"]["sessionId"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string();
                                let tracked = status_signatures
                                    .lock()
                                    .map(|mut s| {
                                        if let Some(signature) = s.get_mut(&id) {
                                            // Invalidate any bitmap currently awaiting the host,
                                            // even if another refresh already owns this session.
                                            *signature = None;
                                            true
                                        } else {
                                            false
                                        }
                                    })
                                    .unwrap_or(false);
                                let start = tracked
                                    && status_refreshing
                                        .lock()
                                        .map(|mut s| {
                                            if let Some(dirty) = s.get_mut(&id) {
                                                *dirty = true;
                                                false
                                            } else if host_requests.len() < 128 {
                                                s.insert(id.clone(), false);
                                                true
                                            } else {
                                                false
                                            }
                                        })
                                        .unwrap_or(false);
                                if start {
                                    let ctx = ctx.clone();
                                    let pending = status_refreshing.clone();
                                    host_requests.spawn(async move {
                                        struct Release {
                                            pending: Arc<Mutex<HashMap<String, bool>>>,
                                            id: Option<String>,
                                        }
                                        impl Drop for Release {
                                            fn drop(&mut self) {
                                                if let (Some(id), Ok(mut pending)) =
                                                    (&self.id, self.pending.lock())
                                                {
                                                    pending.remove(id);
                                                }
                                            }
                                        }
                                        let mut lease = Release {
                                            pending,
                                            id: Some(id.clone()),
                                        };
                                        loop {
                                            let _ = ctx
                                                .request(
                                                    "core:extension-status-refresh",
                                                    vec![json!({"sessionId":id})],
                                                )
                                                .await;
                                            let again = lease
                                                .pending
                                                .lock()
                                                .map(|mut pending| {
                                                    if pending.get(&id) == Some(&true) {
                                                        pending.insert(id.clone(), false);
                                                        true
                                                    } else {
                                                        pending.remove(&id);
                                                        false
                                                    }
                                                })
                                                .unwrap_or(false);
                                            if !again {
                                                // Do not remove a new owner admitted after
                                                // the previous map entry was released.
                                                lease.id = None;
                                                break;
                                            }
                                        }
                                    });
                                }
                            }
                            ctx.emit(channel, message["payload"].clone());
                        }
                    }
                    #[cfg(target_os = "macos")]
                    Some("timer_notification") => {
                        if let (Some(name), Some(path)) =
                            (message["name"].as_str(), message["timerFile"].as_str())
                        {
                            // Legacy JS notification compatibility initializes only
                            // when actually used; external processes use generic APIs.
                            let _ = ctx
                                .host
                                .notify(crate::host::NotificationSpec::Timer {
                                    name: name.into(),
                                    path: path.into(),
                                })
                                .await;
                        }
                    }
                    _ => {}
                }
            }
            // Cancellation is complete only after every host future is dropped.
            host_requests.abort_all();
            while host_requests.join_next().await.is_some() {}
        });
        let mut writer = tokio::spawn(async move {
            while let Some(message) = messages.recv().await {
                let Ok(mut frame) = serde_json::to_vec(&message) else {
                    continue;
                };
                if frame.len() >= MAX_FRAME {
                    break;
                }
                frame.push(b'\n');
                if stdin.write_all(&frame).await.is_err() {
                    break;
                }
            }
        });
        let errors = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            while let Ok(Some(line)) = read_frame(&mut reader).await {
                log::debug!("Extension Worker: {}", String::from_utf8_lossy(&line));
                #[cfg(test)]
                eprintln!(
                    "Extension Worker stderr: {}",
                    String::from_utf8_lossy(&line)
                );
            }
        });
        let process_alive = alive.clone();
        let process_pending = pending.clone();
        let ctx = self.ctx.clone();
        let sdk_sessions = self.sessions.clone();
        let sdk_signatures = self.status_signatures.clone();
        let sdk_refreshing = self.status_refreshing.clone();
        let finished = tokio::spawn(async move {
            let mut output_joined = false;
            let mut writer_joined = false;
            tokio::select! {
                _ = stopped => {}
                _ = child.wait() => {}
                _ = &mut output => { output_joined = true; }
                _ = &mut writer => { writer_joined = true; }
            }
            process_alive.store(false, Ordering::Release);
            if let Some(pid) = pid {
                kill_process_tree(pid).await;
            }
            // Kill the tree while its root is still addressable (taskkill /T on Windows).
            let _ = child.kill().await;
            let _ = child.wait().await;
            if let Ok(mut pending) = process_pending.lock() {
                for (_, reply) in pending.drain() {
                    let _ =
                        reply.send(Err("Extension Worker stopped; reopen the extension".into()));
                }
            }
            let _ = cancel_output.send(());
            if !output_joined {
                let _ = output.await;
            }
            writer.abort();
            if !writer_joined {
                let _ = writer.await;
            }
            errors.abort();
            let _ = errors.await;
            if let Ok(mut signatures) = sdk_signatures.lock() {
                signatures.clear();
            }
            if let Ok(mut pending) = sdk_refreshing.lock() {
                pending.clear();
            }
            if let Ok(mut sessions) = sdk_sessions.lock() {
                for (_, (_, _, scope)) in sessions.drain() {
                    let _ = ctx.host.status_items().remove(&scope);
                }
            }
            ctx.emit("extension-worker-status", json!({"phase":"stopped"}));
        });
        if tokio::time::timeout(Duration::from_secs(20), readiness)
            .await
            .ok()
            .and_then(Result::ok)
            .is_none()
        {
            let _ = stop.send(());
            let _ = finished.await;
            return Err("Extension Worker did not become ready".into());
        }
        self.ctx
            .emit("extension-worker-status", json!({"phase":"ready"}));
        Ok(Process {
            sender,
            pending,
            alive,
            stop: Some(stop),
            finished,
        })
    }

    pub(crate) async fn retire_sdk(&self, extension: &str, command: Option<&str>) {
        let ids = self
            .sessions
            .lock()
            .map(|sessions| {
                sessions
                    .iter()
                    .filter(|(_, (e, c, _))| {
                        e == extension && command.is_none_or(|command| command == c)
                    })
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for id in ids {
            let _ = self
                .request("extension:dispose-session", vec![json!({"sessionId":id})])
                .await;
        }
    }
    fn remove_sdk_session(&self, id: &str) {
        if let Ok(mut signatures) = self.status_signatures.lock() {
            signatures.remove(id);
        }
        if let Ok(mut pending) = self.status_refreshing.lock() {
            pending.remove(id);
        }
        if let Ok(mut sessions) = self.sessions.lock() {
            if let Some((_, _, scope)) = sessions.remove(id) {
                let _ = self.ctx.host.status_items().remove(&scope);
                self.ctx
                    .emit("extension:session-disposed", json!({"sessionId":id}));
            }
        }
    }
    async fn sync_sdk_status(&self, reply: &Value) -> Result<(), String> {
        if !self.ctx.host.sdk_status_items() {
            return Ok(());
        }
        let Some(id) = reply["sessionId"].as_str() else {
            return Ok(());
        };
        let Some((extension, command, scope)) = self
            .sessions
            .lock()
            .map_err(|_| "SDK resources unavailable")?
            .get(id)
            .cloned()
        else {
            return Ok(());
        };
        if reply["root"]["type"] != "MenuBarExtra" {
            // Retain background ownership while the extension temporarily returns
            // no status item. Later worker invalidations must still refresh it.
            let sessions = self
                .sessions
                .lock()
                .map_err(|_| "SDK resources unavailable")?;
            if sessions.get(id) != Some(&(extension, command, scope.clone())) {
                return Ok(());
            }
            if let Some(signature) = self
                .status_signatures
                .lock()
                .map_err(|_| "Status owner unavailable")?
                .get_mut(id)
            {
                *signature = None;
            }
            let _ = self.ctx.host.status_items().remove(&scope);
            return Ok(());
        }
        let root = &reply["root"];
        fn items(node: &Value, out: &mut Vec<Value>, counter: &mut usize) {
            *counter += 1;
            let group = format!("group-{}", counter);
            match node["type"].as_str() {
                Some("MenuBarExtra.Item") => {
                    if let Some(action) = node["props"]["actionIds"][0].as_str() {
                        out.push(json!({"id":action,"title":node["props"]["title"],"enabled":node["props"]["isEnabled"].as_bool().unwrap_or(true),"checked":node["props"]["isSelected"]}));
                    } else if let Some(title) = node["props"]["title"].as_str() {
                        out.push(json!({"id":group,"title":title,"enabled":false}));
                    }
                }
                Some("MenuBarExtra.Submenu") => {
                    let mut nested = Vec::new();
                    if let Some(children) = node["children"].as_array() {
                        for child in children {
                            items(child, &mut nested, counter);
                        }
                    }
                    out.push(json!({"id":group,"kind":"submenu","title":node["props"]["title"],"items":nested}));
                    return;
                }
                Some("MenuBarExtra.Section") => {
                    if let Some(title) = node["props"]["title"].as_str() {
                        out.push(
                            json!({"id":group,"kind":"section","title":title,"enabled":false}),
                        );
                    }
                }
                Some("MenuBarExtra.Separator") => {
                    out.push(json!({"id":group,"kind":"separator","title":""}));
                    return;
                }
                _ => {}
            }
            if let Some(children) = node["children"].as_array() {
                for child in children {
                    items(child, out, counter);
                }
            }
        }
        let mut menu = Vec::new();
        items(root, &mut menu, &mut 0);
        let this = self.clone();
        let session = id.to_string();
        let runtime = tokio::runtime::Handle::current();
        let callback = Arc::new(move |action: String| {
            let this = this.clone();
            let session = session.clone();
            runtime.spawn(async move {
                // Reject callbacks after dispose/disable/EOF, including queued OS clicks.
                if !this
                    .sessions
                    .lock()
                    .map(|s| s.contains_key(&session))
                    .unwrap_or(false)
                {
                    return;
                }
                let _ = Box::pin(this.request(
                    "core:extension-status-action",
                    vec![json!({"sessionId":session,"actionId":action})],
                ))
                .await;
            });
        });
        let title = root["props"]["title"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                if root["props"]["icon"].is_null() {
                    command.replace(['-', '_'], " ")
                } else {
                    String::new()
                }
            });
        let tooltip = root["props"]["tooltip"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| command.replace(['-', '_'], " "));
        let mut params =
            json!({"items":menu,"title":title,"tooltip":tooltip,"icon":root["props"]["icon"]});
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        params.to_string().hash(&mut hash);
        let signature = hash.finish();
        let revision = {
            let sessions = self
                .sessions
                .lock()
                .map_err(|_| "SDK resources unavailable")?;
            if sessions.get(id) != Some(&(extension.clone(), command.clone(), scope.clone())) {
                return Ok(());
            }
            let mut signatures = self
                .status_signatures
                .lock()
                .map_err(|_| "Status owner unavailable")?;
            if signatures.get(id).and_then(|s| *s).map(|s| s.0) == Some(signature) {
                return Ok(());
            }
            let revision = self
                .status_revision
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |r| r.checked_add(1))
                .map_err(|_| "Status revision exhausted")?;
            signatures.insert(id.into(), Some((signature, revision)));
            revision
        };
        use crate::extensions::process_host::{sdk_status_icon_fallback, SdkStatusIconFallback};
        let windows = cfg!(target_os = "windows");
        let icon = if root["props"]["icon"].is_null() {
            if windows {
                params["tooltip"] = json!(format!("{tooltip} (no icon declared)"));
            }
            sdk_status_icon_fallback(windows, SdkStatusIconFallback::Absent)
        } else {
            match self
                .ctx
                .host
                .resolve_sdk_status_icon(extension.clone(), root["props"]["icon"].clone())
                .await
            {
                Ok(icon) => Some(icon),
                Err(error) => {
                    // An unsupported/refused source is not a gear. Keep honest
                    // text on macOS and a visible warning marker on Windows.
                    log::warn!("SDK status icon unavailable: {error}");
                    if title.is_empty() {
                        params["title"] = json!(command.replace(['-', '_'], " "));
                    }
                    params["tooltip"] = json!(format!("{tooltip} (icon unavailable)"));
                    sdk_status_icon_fallback(windows, SdkStatusIconFallback::Unavailable)
                }
            }
        };
        // Serialize final publication with disposal and newer revisions. In
        // particular A -> B -> A must not admit the first A's late completion.
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| "SDK resources unavailable")?;
        let mut signatures = self
            .status_signatures
            .lock()
            .map_err(|_| "Status owner unavailable")?;
        if sessions.get(id) != Some(&(extension, command, scope.clone()))
            || signatures.get(id) != Some(&Some((signature, revision)))
        {
            return Ok(());
        }
        if let Err(error) = crate::extensions::process_host::set_sdk_status_item(
            &self.ctx, &scope, &params, callback, icon,
        ) {
            signatures.insert(id.into(), None);
            return Err(error);
        }
        Ok(())
    }
    pub async fn shutdown(&self) {
        if let Ok(mut signatures) = self.status_signatures.lock() {
            signatures.clear();
        }
        if let Ok(mut pending) = self.status_refreshing.lock() {
            pending.clear();
        }
        if let Some(mut process) = self.process.lock().await.take() {
            if let Some(stop) = process.stop.take() {
                let _ = stop.send(());
            }
            let _ = process.finished.await;
        }
    }

    #[cfg(test)]
    pub async fn is_running(&self) -> bool {
        self.process
            .lock()
            .await
            .as_ref()
            .is_some_and(|process| process.alive.load(Ordering::Acquire))
    }
}

async fn kill_process_tree(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    #[cfg(target_os = "windows")]
    {
        let _ = tokio::process::Command::new("taskkill.exe")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x08000000)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
}

/// Bound allocations and split only on LF, preserving UTF-8 and U+2028/U+2029.
async fn read_frame<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Option<Vec<u8>>, String> {
    let mut frame = Vec::new();
    loop {
        let bytes = reader.fill_buf().await.map_err(|e| e.to_string())?;
        if bytes.is_empty() {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Err("Truncated extension protocol frame".into())
            };
        }
        let count = bytes
            .iter()
            .position(|b| *b == b'\n')
            .map(|n| n + 1)
            .unwrap_or(bytes.len());
        if frame.len() + count > MAX_FRAME {
            return Err("Extension protocol frame too large".into());
        }
        let complete = bytes[count - 1] == b'\n';
        frame.extend_from_slice(&bytes[..count]);
        reader.consume(count);
        if complete {
            frame.pop();
            return Ok(Some(frame));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn framing_preserves_unicode_and_rejects_truncation() {
        let payload = "{\"text\":\"Қазақша\u{2028}日本語\"}\n";
        let mut reader = BufReader::with_capacity(1, payload.as_bytes());
        let value: Value =
            serde_json::from_slice(&read_frame(&mut reader).await.unwrap().unwrap()).unwrap();
        assert_eq!(value["text"], "Қазақша\u{2028}日本語");
        assert!(read_frame(&mut reader).await.unwrap().is_none());
        assert!(read_frame(&mut BufReader::new(&b"{partial"[..]))
            .await
            .is_err());
    }
    #[tokio::test]
    async fn framing_bounds_unterminated_input_and_allows_multiple_records() {
        let mut oversized = BufReader::new(std::io::Cursor::new(vec![b'x'; MAX_FRAME + 1]));
        assert!(read_frame(&mut oversized)
            .await
            .unwrap_err()
            .contains("too large"));
        let mut records = BufReader::with_capacity(2, &b"first\n\nlast\n"[..]);
        assert_eq!(read_frame(&mut records).await.unwrap().unwrap(), b"first");
        assert!(read_frame(&mut records).await.unwrap().unwrap().is_empty());
        assert_eq!(read_frame(&mut records).await.unwrap().unwrap(), b"last");
        assert!(read_frame(&mut records).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn constructing_worker_does_not_launch_runtime_and_rejects_core_channels() {
        let dir = tempfile::tempdir().unwrap();
        let paths = super::super::common::CorePaths {
            data_dir: dir.path().join("data"),
            config_dir: dir.path().join("config"),
            resource_dir: dir.path().join("resources"),
            temp_dir: dir.path().join("temp"),
            home_dir: dir.path().to_owned(),
        };
        let worker = ExtensionWorker::new(CoreContext::headless(paths));
        assert!(worker.process.lock().await.is_none());
        assert!(worker.request("search", vec![]).await.is_err());
        assert!(worker.process.lock().await.is_none());
        assert!(!HOST_CHANNELS.contains(&"llm-config-get"));
    }
}

#[cfg(test)]
#[path = "extension_worker_tests.rs"]
mod integration_tests;

#[cfg(test)]
#[path = "extension_worker_status_icon_tests.rs"]
mod status_icon_tests;

#[cfg(test)]
mod launch_path_tests {
    use super::*;
    #[test]
    fn explicit_worker_launch_validates_owned_paths_without_launching() {
        let root = tempfile::tempdir().unwrap();
        let bun = root.path().join("fixture-bun");
        let dir = root.path().join("bundle");
        std::fs::create_dir(&dir).unwrap();
        assert!(WorkerLaunch::new(bun.clone(), dir.clone()).is_err());
        std::fs::write(&bun, b"not invoked by this test").unwrap();
        assert!(WorkerLaunch::new(bun.clone(), dir.clone()).is_err());
        std::fs::write(dir.join("extension-worker.js"), b"fixture bundle").unwrap();
        let launch = WorkerLaunch::new(bun.clone(), dir.clone()).unwrap();
        assert_eq!(launch.bun, std::fs::canonicalize(bun).unwrap());
        assert_eq!(launch.directory, std::fs::canonicalize(dir).unwrap());
    }
}
