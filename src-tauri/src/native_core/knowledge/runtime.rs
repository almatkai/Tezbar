// The binary owns helper dispatch. Include privately here for its protocol and
// isolated fixture tests; lib.rs needs no new public export.
#[allow(dead_code)]
#[path = "../../child_supervisor.rs"]
mod child_supervisor;
use super::extract::{self, Cancellation};
use crate::native_core::common::{now_ms, read_json, write_json_atomic, CoreContext};
use futures_util::StreamExt;
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
    sync::Mutex as AsyncMutex,
};
const REVISION: &str = "bfcd298762cc34d0357ece5ebdd31791a3a374d8";
const MODEL_FILE: &str = "embeddinggemma-2-Q8_0.gguf";
const MODEL_BYTES: u64 = 309855456;
const MODEL_SHA: &str = "2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135";
pub const MODEL_KEY: &str =
    "google/embeddinggemma-2:bfcd298762cc34d0357ece5ebdd31791a3a374d8:q8:256:search-v1";
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
#[derive(Clone)]
pub struct Runtime {
    inner: Arc<Inner>,
}
struct Inner {
    directory: PathBuf,
    #[cfg(test)]
    supervisor: Mutex<Option<PathBuf>>,
    status: Mutex<Value>,
    download_gen: Arc<AtomicU64>,
    download: AsyncMutex<Option<tokio::task::JoinHandle<()>>>,
    process: AsyncMutex<Option<Process>>,
    closed: AtomicBool,
    removing: AtomicBool,
    client: reqwest::Client,
}
struct Process {
    child: Child,
    endpoint: String,
    token: String,
    last_used: u64,
    threads: usize,
}
impl Runtime {
    pub fn new(ctx: CoreContext) -> Self {
        let total = MODEL_BYTES + platform().map(|p| p.bytes).unwrap_or(0);
        let directory = ctx.paths.data_dir.join("knowledge/models/embeddinggemma-2");
        Self {
            inner: Arc::new(Inner {
                directory,
                #[cfg(test)]
                supervisor: Mutex::new(None),
                status: Mutex::new(
                    json!({"supported":platform().is_some(),"modelState":"not-downloaded","downloadProgress":0,"downloadBytes":0,"downloadTotalBytes":total,"indexing":false,"indexedChunks":0,"totalChunks":0}),
                ),
                download_gen: Arc::new(AtomicU64::new(0)),
                download: AsyncMutex::new(None),
                process: AsyncMutex::new(None),
                closed: AtomicBool::new(false),
                removing: AtomicBool::new(false),
                client: reqwest::Client::new(),
            }),
        }
    }
    pub fn snapshot(&self) -> Value {
        self.inner.status.lock().unwrap().clone()
    }
    pub fn patch(&self, patch: Value) {
        let mut status = self.inner.status.lock().unwrap();
        if let Some(fields) = patch.as_object() {
            for (k, v) in fields {
                status[k] = v.clone();
            }
        }
        drop(status);
    }
    pub async fn initialize(&self) -> Result<(), String> {
        let directory = self.inner.directory.clone();
        let ready = tokio::task::spawn_blocking(move || {
            let manifest = read_json(&directory.join("installed.json")).unwrap_or(Value::Null);
            platform()
                .map(|p| {
                    manifest["revision"] == REVISION
                        && manifest["runtime"] == p.file
                        && directory.join(MODEL_FILE).is_file()
                        && server(&directory.join("runtime")).is_some()
                })
                .unwrap_or(false)
        })
        .await
        .map_err(|e| e.to_string())?;
        if ready {
            let total = self.snapshot()["downloadTotalBytes"].as_u64().unwrap_or(0);
            self.patch(json!({"modelState":"ready","downloadProgress":1,"downloadBytes":total}));
        }
        Ok(())
    }
    pub async fn download(&self) -> Result<(), String> {
        if self.inner.closed.load(Ordering::SeqCst) || self.inner.removing.load(Ordering::SeqCst) {
            return Err("Embedding runtime is shutting down".into());
        }
        let mut task = self.inner.download.lock().await;
        if self.snapshot()["modelState"] == "ready"
            || task.as_ref().map(|t| !t.is_finished()).unwrap_or(false)
        {
            return Ok(());
        }
        let manifest = platform().ok_or("EmbeddingGemma is not available for this platform yet")?;
        let expected = self.inner.download_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let cancel = Cancellation {
            generation: self.inner.download_gen.clone(),
            expected,
        };
        self.patch(
            json!({"modelState":"downloading","downloadBytes":0,"downloadProgress":0,"error":null}),
        );
        let this = self.clone();
        *task = Some(tokio::spawn(async move {
            let result = this.install(manifest, &cancel).await;
            if let Err(error) = result {
                let cancelled = cancel.check().is_err();
                this.patch(json!({"modelState":if cancelled{"not-downloaded"}else{"error"},"error":if cancelled{Value::Null}else{json!(error)}}));
            }
        }));
        Ok(())
    }
    async fn install(&self, manifest: Manifest, cancel: &Cancellation) -> Result<(), String> {
        let directory = &self.inner.directory;
        tokio::fs::create_dir_all(directory)
            .await
            .map_err(|e| e.to_string())?;
        let archive = directory.join(manifest.file);
        self.download_file(
            &format!(
                "https://github.com/ggml-org/llama.cpp/releases/download/b11474/{}",
                manifest.file
            ),
            &archive,
            manifest.sha,
            manifest.bytes,
            cancel,
        )
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
        self.download_file(&format!("https://huggingface.co/ggml-org/embeddinggemma-2-GGUF/resolve/{REVISION}/{MODEL_FILE}"),&directory.join(MODEL_FILE),MODEL_SHA,MODEL_BYTES,cancel).await?;
        cancel.check()?;
        let path = directory.join("installed.json");
        let value = json!({"revision":REVISION,"runtime":manifest.file});
        tokio::task::spawn_blocking(move || write_json_atomic(&path, &value))
            .await
            .map_err(|e| e.to_string())??;
        self.patch(json!({"modelState":"ready","downloadProgress":1,"error":null}));
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
        self.inner.download_gen.fetch_add(1, Ordering::SeqCst);
        let task = self.inner.download.lock().await.take();
        let was_downloading = task.as_ref().map(|t| !t.is_finished()).unwrap_or(false);
        if let Some(task) = task {
            task.await.map_err(|e| e.to_string())?;
        }
        if was_downloading && self.snapshot()["modelState"] != "ready" {
            if tokio::fs::try_exists(&self.inner.directory)
                .await
                .map_err(|e| e.to_string())?
            {
                tokio::fs::remove_dir_all(&self.inner.directory)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            self.patch(
                json!({"modelState":"not-downloaded","downloadBytes":0,"downloadProgress":0}),
            );
        }
        Ok(())
    }
    pub async fn remove(&self) -> Result<(), String> {
        self.inner.removing.store(true, Ordering::SeqCst);
        let result=async{self.cancel_download().await?;self.stop().await;if tokio::fs::try_exists(&self.inner.directory).await.map_err(|e|e.to_string())?{tokio::fs::remove_dir_all(&self.inner.directory).await.map_err(|e|e.to_string())?;}self.patch(json!({"modelState":"not-downloaded","downloadBytes":0,"downloadProgress":0,"indexedChunks":0,"totalChunks":0,"error":null}));Ok(())}.await;
        self.inner.removing.store(false, Ordering::SeqCst);
        result
    }
    pub async fn stop(&self) {
        if let Some(mut p) = self.inner.process.lock().await.take() {
            // Closing the lease lets the supervisor kill and reap the entire tree.
            drop(p.child.stdin.take());
            let _ = p.child.wait().await;
        }
    }
    pub async fn stop_idle(&self) {
        let mut process = self.inner.process.lock().await;
        if process
            .as_ref()
            .map(|p| now_ms().saturating_sub(p.last_used) > 120000)
            .unwrap_or(false)
        {
            if let Some(mut p) = process.take() {
                // Closing the lease lets the supervisor kill and reap the entire tree.
                drop(p.child.stdin.take());
                let _ = p.child.wait().await;
            }
        }
    }
    pub async fn shutdown(&self) {
        self.inner.closed.store(true, Ordering::SeqCst);
        let _ = self.cancel_download().await;
        self.stop().await;
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
        if self.inner.closed.load(Ordering::SeqCst) || self.inner.removing.load(Ordering::SeqCst) {
            return Err("Embedding runtime is stopping".into());
        }
        let mut guard = self.inner.process.lock().await;
        cancel.check()?;
        let available = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let threads = available.min(match speed {
            "slow" => 2,
            "fast" => 8,
            _ => 4,
        });
        if let Some(p) = guard.as_mut() {
            if p.threads != threads || p.child.try_wait().map_err(|e| e.to_string())?.is_some() {
                let mut p = guard.take().unwrap();
                // Closing the lease lets the supervisor kill and reap the entire tree.
                drop(p.child.stdin.take());
                let _ = p.child.wait().await;
            }
        }
        if guard.is_none() {
            *guard = Some(self.launch(threads, cancel).await?);
        }
        let p = guard.as_mut().unwrap();
        p.last_used = now_ms();
        let request = async {
            let response = self
                .inner
                .client
                .post(format!("{}/v1/embeddings", p.endpoint))
                .bearer_auth(&p.token)
                .json(&json!({"input":inputs,"encoding_format":"float"}))
                .timeout(std::time::Duration::from_secs(60))
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
        let cancelled = async {
            loop {
                cancel.check()?;
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        };
        let result = tokio::select! {r=request=>r,r=cancelled=>r};
        p.last_used = now_ms();
        if let Err(e) = &result {
            self.patch(json!({"error":e}));
            if let Some(mut p) = guard.take() {
                // Closing the lease lets the supervisor kill and reap the entire tree.
                drop(p.child.stdin.take());
                let _ = p.child.wait().await;
            }
        } else {
            self.patch(json!({"error":null}));
        }
        result
    }
    async fn launch(&self, threads: usize, cancel: &Cancellation) -> Result<Process, String> {
        if self.snapshot()["modelState"] != "ready" {
            return Err("Download EmbeddingGemma before enabling it".into());
        }
        let directory = self.inner.directory.join("runtime");
        let executable = tokio::task::spawn_blocking(move || server(&directory))
            .await
            .map_err(|e| e.to_string())?
            .ok_or("Downloaded runtime does not contain llama-server")?;
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
            .arg(self.inner.directory.join(MODEL_FILE))
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
        if let Some(mut pipe) = child.stderr.take() {
            let output = stderr.clone();
            tokio::spawn(async move {
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
            });
        }
        let deadline = now_ms() + 60000;
        while now_ms() < deadline {
            cancel.check()?;
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                let error = stderr.lock().unwrap().iter().copied().collect::<Vec<_>>();
                return Err(format!(
                    "Local embedding runtime exited ({status}): {}",
                    String::from_utf8_lossy(&error)
                ));
            }
            if let Ok(response) = self
                .inner
                .client
                .get(format!("{endpoint}/health"))
                .bearer_auth(&token)
                .timeout(std::time::Duration::from_secs(1))
                .send()
                .await
            {
                if response.status().is_success() {
                    return Ok(Process {
                        child,
                        endpoint,
                        token,
                        last_used: now_ms(),
                        threads,
                    });
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        Err("Local embedding runtime took too long to start".into())
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
    let magnitude = truncated.iter().map(|v| v * v).sum::<f64>().sqrt();
    if magnitude == 0. {
        return Err("EmbeddingGemma returned an empty vector".into());
    }
    for v in &mut truncated {
        *v /= magnitude;
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
            let runtime = Runtime::new(context(temp.path()));
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
        std::fs::write(&executable,r#"#!/usr/bin/env python3
import sys,json,os,subprocess,time
from pathlib import Path
from http.server import BaseHTTPRequestHandler,HTTPServer
args=sys.argv[1:];port=int(args[args.index('--port')+1]);token=args[args.index('--api-key')+1]
assert '--embedding' in args and args[args.index('--pooling')+1]=='mean'
directory=Path(args[args.index('--model')+1]).parent
leaf=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)'])
(directory/'pids').write_text(str(os.getpid())+' '+str(leaf.pid))
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args): pass
 def do_GET(self):
  if self.headers.get('Authorization')!='Bearer '+token:self.send_error(403);return
  self.send_response(200);self.end_headers();self.wfile.write(b'{}')
 def do_POST(self):
  if self.headers.get('Authorization')!='Bearer '+token:self.send_error(403);return
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
  if body['input']==['hold']:
   (directory/'holding').write_text('ready')
   time.sleep(60)
  data=[{'index':i,'embedding':[1.0]*768} for i,_ in enumerate(body['input'])]
  self.send_response(200);self.send_header('Content-Type','application/json');self.end_headers();self.wfile.write(json.dumps({'data':data}).encode())
HTTPServer(('127.0.0.1',port),Handler).serve_forever()
"#).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let runtime = Runtime::new(ctx);
        *runtime.inner.supervisor.lock().unwrap() =
            Some(child_supervisor::tests::fixture_executable());
        runtime.initialize().await.unwrap();
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

        let runtime = Runtime::new(context(temp.path()));
        *runtime.inner.supervisor.lock().unwrap() =
            Some(child_supervisor::tests::fixture_executable());
        runtime.initialize().await.unwrap();
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
