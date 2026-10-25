//! Demand-launched JavaScript compatibility process. Never dispatch core IPC here.
use super::common::{CoreContext, CoreResult};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
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

struct PendingRequest {
    id: String,
    pending: Pending,
}
impl Drop for PendingRequest {
    fn drop(&mut self) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&self.id);
        }
    }
}

#[derive(Clone)]
pub struct ExtensionWorker {
    ctx: CoreContext,
    process: Arc<tokio::sync::Mutex<Option<Process>>>,
}

impl ExtensionWorker {
    pub fn new(ctx: CoreContext) -> Self {
        Self {
            ctx,
            process: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    pub async fn request(&self, channel: &str, args: Vec<Value>) -> CoreResult {
        if !CHANNELS.contains(&channel) {
            return Err(format!("Channel is not an extension operation: {channel}"));
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
            pending.insert(id.clone(), reply);
        }
        let _request = PendingRequest {
            id: id.clone(),
            pending,
        };
        let timeout = if channel.ends_with("install") {
            Duration::from_secs(900)
        } else {
            Duration::from_secs(120)
        };
        tokio::time::timeout(timeout, async {
            sender
                .send(json!({"type":"invoke", "id":id, "channel":channel, "payload":args}))
                .await
                .map_err(|_| "Extension Worker disconnected".to_string())?;
            result
                .await
                .map_err(|_| "Extension Worker stopped before replying".to_string())?
        })
        .await
        .map_err(|_| format!("Extension operation timed out: {channel}"))?
    }

    async fn launch(&self) -> Result<Process, String> {
        self.ctx
            .emit("extension-worker-status", json!({"phase":"starting"}));
        let data = self.ctx.paths.data_dir.clone();
        // Runtime discovery/download happens only in this extension-only path.
        let bun = tokio::task::spawn_blocking(move || crate::locate_bun(&data))
            .await
            .map_err(|e| e.to_string())??;
        let directory = if cfg!(debug_assertions) {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dist-backend")
        } else {
            self.ctx.paths.resource_dir.join("dist-backend")
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
                        host_requests.spawn(async move {
                            let channel = message["channel"].as_str().unwrap_or("");
                            let result = if HOST_CHANNELS.contains(&channel) {
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
                            if let Some(app)=ctx.app.as_ref(){crate::timer_notifications::setup(app);}
                            crate::timer_notifications::deliver(name, path);
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

    pub async fn shutdown(&self) {
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
