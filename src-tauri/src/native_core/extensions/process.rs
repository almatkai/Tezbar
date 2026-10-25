//! Revision-1 independent bidirectional pipes and supervised process ownership.
#[cfg(test)]
#[path = "../../child_supervisor.rs"]
mod supervisor;
use super::{process_host::Host, process_install::Package};
use crate::native_core::common::{CoreContext, CoreResult};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex, Weak,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{mpsc, oneshot, watch, Notify},
};

pub(super) const MAX_FRAME: usize = 16 * 1024 * 1024;
const MAX_PENDING: usize = 128;
fn child_environment(
    command: &mut tokio::process::Command,
    lookup: impl Fn(&str) -> Option<std::ffi::OsString>,
) {
    // Only system basics and an explicitly public OAuth client identifier. Never
    // inherit application/provider API keys, access tokens or client secrets.
    command.env_clear();
    for key in [
        "PATH",
        "HOME",
        "USERPROFILE",
        "SYSTEMROOT",
        "WINDIR",
        "TEMP",
        "TMP",
        "TMPDIR",
        "LANG",
        "LC_ALL",
        "TEZBAR_GOOGLE_OAUTH_CLIENT_ID",
    ] {
        if let Some(value) = lookup(key) {
            command.env(key, value);
        }
    }
}
pub(super) struct Process {
    pub package: Package,
    pub host: Host,
    writer: mpsc::Sender<Value>,
    pending: Mutex<HashMap<String, oneshot::Sender<CoreResult>>>,
    dead: watch::Sender<bool>,
    initialized: AtomicBool,
    launcher_enabled: AtomicBool,
    pub launcher_revision: std::sync::atomic::AtomicU64,
    pub launcher_queries: Mutex<HashMap<String, (String, String, u64)>>,
    cleaned: watch::Sender<bool>,
    cancelled: Mutex<std::collections::HashSet<String>>,
    inflight: AtomicUsize,
    pub sessions: Mutex<HashMap<String, u64>>,
    pub changed: Notify,
    pub generation: String,
}
impl Process {
    pub async fn spawn(
        ctx: &CoreContext,
        package: Package,
        reason: &str,
    ) -> Result<Arc<Self>, String> {
        #[cfg(not(test))]
        let supervisor_exe = std::env::current_exe().map_err(|e| e.to_string())?;
        #[cfg(test)]
        let supervisor_exe = tokio::task::spawn_blocking(supervisor::tests::fixture_executable)
            .await
            .map_err(|e| e.to_string())?;
        let mut command = tokio::process::Command::new(supervisor_exe);
        command
            .arg("--tezbar-private-extension-supervisor-v1")
            .arg(&package.executable);
        command
            .current_dir(&package.dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        // Do not leak application/provider credentials to publisher code through
        // inherited environment. Use only the explicit allowlist; no shell.
        child_environment(&mut command, |key| std::env::var_os(key));
        #[cfg(unix)]
        command.process_group(0);
        #[cfg(windows)]
        // Start suspended so helpers cannot escape before job assignment.
        command.creation_flags(0x08000000 | 0x00000004);
        let mut child = command
            .spawn()
            .map_err(|e| format!("Cannot launch extension executable: {e}"))?;
        let pid = child.id().ok_or("Missing extension PID")?;
        let mut tree = ProcessTree::new(pid)?;
        let mut stdin = child.stdin.take().ok_or("Missing stdin")?;
        let stdout = child.stdout.take().ok_or("Missing stdout")?;
        let stderr = child.stderr.take().ok_or("Missing stderr")?;
        let (writer, mut messages) = mpsc::channel::<Value>(64);
        let (dead, _) = watch::channel(false);
        let process = Arc::new(Self {
            host: Host::new(ctx.clone(), &package.manifest)?,
            package,
            writer,
            pending: Mutex::new(HashMap::new()),
            dead,
            initialized: AtomicBool::new(false),
            launcher_enabled: AtomicBool::new(false),
            launcher_revision: std::sync::atomic::AtomicU64::new(0),
            launcher_queries: Mutex::new(HashMap::new()),
            cleaned: watch::channel(false).0,
            cancelled: Mutex::new(std::collections::HashSet::new()),
            inflight: AtomicUsize::new(0),
            sessions: Mutex::new(HashMap::new()),
            changed: Notify::new(),
            generation: uuid::Uuid::new_v4().to_string(),
        });
        let weak = Arc::downgrade(&process);
        let mut stop = process.dead.subscribe();
        tokio::spawn(async move {
            loop {
                tokio::select! { biased; _=stop.changed()=>break, message=messages.recv()=> {
                    let Some(message)=message else {break};
                    let mut bytes=match serde_json::to_vec(&message) {Ok(b)=>b,Err(_)=>break};
                    if bytes.len()>MAX_FRAME {break;} bytes.push(b'\n');
                    let write=async {stdin.write_all(&bytes).await?;stdin.flush().await};
                    let result=tokio::select!{biased;_=stop.changed()=>break,r=tokio::time::timeout(Duration::from_secs(10),write)=>r};
                    if !matches!(result,Ok(Ok(()))) {break;}
                } }
            }
            if let Some(p) = weak.upgrade() {
                p.fail("Extension pipe writer closed");
            }
        });
        let weak = Arc::downgrade(&process);
        let mut stop = process.dead.subscribe();
        let (hello_tx, hello_rx) = oneshot::channel();
        tokio::spawn(async move {
            read_loop(weak, stdout, hello_tx, &mut stop).await;
        });
        // Stderr is drained with constant memory even without LF. Never send logs
        // (which may contain OAuth tokens) into renderer events or host RPC logs.
        let mut stop = process.dead.subscribe();
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut stderr = stderr;
            let mut b = [0u8; 8192];
            loop {
                tokio::select! {_=stop.changed()=>break,r=stderr.read(&mut b)=>if !matches!(r,Ok(n) if n>0){break;}}
            }
        });
        let weak = Arc::downgrade(&process);
        let mut stop = process.dead.subscribe();
        tokio::spawn(async move {
            // No polling: Notify wakes on ownership changes; the only timer is
            // the one-shot idle grace. Child exit wakes immediately.
            loop {
                let Some(p) = weak.upgrade() else { break };
                let idle = p.idle();
                let notified = p.changed.notified();
                tokio::select! { biased;
                    _=stop.changed()=>break,
                    _=child.wait()=>break,
                    _=notified=>{},
                    _=tokio::time::sleep(Duration::from_secs(15)), if idle=>{if p.idle(){break;}}
                }
            }
            if let Some(p) = weak.upgrade() {
                // Idle exit asks the SDK to dispose tracked work first. Failure
                // paths immediately fence all requests and force tree cleanup.
                if p.alive() {
                    let _ = p.call("shutdown", json!({}), Duration::from_secs(5)).await;
                }
                p.fail("Extension process stopped");
            }
            // Closing the protocol lease lets the independent supervisor kill
            // its owned extension group, including helpers, on every host exit.
            if tokio::time::timeout(Duration::from_secs(2), child.wait())
                .await
                .is_err()
            {
                tree.terminate();
                let _ = child.start_kill();
                let _ = child.wait().await;
            }
            tree.armed = false;
            drop(tree);
            if let Some(p) = weak.upgrade() {
                let _ = tokio::time::timeout(Duration::from_secs(5), p.host.cleanup()).await;
                p.cleaned.send_replace(true);
            }
        });
        let mut guard = StopOnDrop(Some(process.clone()));
        let hello = tokio::time::timeout(Duration::from_secs(15), hello_rx)
            .await
            .map_err(|_| "Extension hello timed out")?
            .map_err(|_| "Extension closed before hello")??;
        if hello["type"] != "hello"
            || hello["protocol"] != 1
            || hello["id"] != process.package.manifest["id"]
            || hello["version"] != process.package.manifest["version"]
            || !hello["sdkVersion"]
                .as_str()
                .and_then(|v| semver::Version::parse(v).ok())
                .is_some_and(|v| v.major == 0 && v.minor == 1)
        {
            return Err("Extension hello identity/version/protocol mismatch".into());
        }
        let paths = process.host.paths()?;
        let result=process.call("initialize",json!({"protocol":1,"extensionId":process.package.manifest["id"],"platform":super::process_install::target(),"packageDir":process.package.dir,"dataDir":paths.0,"tempDir":paths.1,"grantedCapabilities":process.package.manifest["capabilities"],"reason":reason,"migrationPaths":process.host.migration_paths(),"hostFeatures":[super::launcher::FEATURE]}),Duration::from_secs(20)).await?;
        if result["ready"] != true {
            return Err("Extension initialization did not acknowledge ready".into());
        }
        process.initialized.store(true, Ordering::Release);
        let negotiated = [&hello["features"], &result["features"]].iter().all(|v| {
            v.as_array()
                .is_some_and(|a| a.iter().any(|v| v == super::launcher::FEATURE))
        });
        process
            .launcher_enabled
            .store(negotiated, Ordering::Release);
        if !negotiated
            && super::launcher::descriptors(&process.package.manifest)
                .next()
                .is_some()
        {
            process.host.ctx.emit("extension:launcher-unavailable",json!({"extensionId":process.package.manifest["id"],"message":"Installed executable did not negotiate launcher.v1; command sessions remain available"}));
        }
        if negotiated {
            for d in super::launcher::descriptors(&process.package.manifest)
                .filter(|d| d["index"] == true)
            {
                if let Err(error) = process
                    .call(
                        "launcher.activate",
                        json!({"contributionId":d["id"]}),
                        Duration::from_secs(3),
                    )
                    .await
                {
                    // Optional index coverage must not make ordinary command
                    // sessions unavailable (for example, a repairable index limit).
                    log::warn!("Launcher index activation unavailable: {error}");
                    process.host.ctx.emit("extension:launcher-unavailable",json!({"extensionId":process.package.manifest["id"],"contributionId":d["id"],"message":error}));
                }
            }
        }
        guard.0 = None;
        Ok(process)
    }
    #[cfg(test)]
    pub fn pending_count_for_test(&self) -> usize {
        self.pending.lock().unwrap().len()
    }
    pub fn alive(&self) -> bool {
        !*self.dead.borrow()
    }
    pub fn launcher_enabled(&self) -> bool {
        self.launcher_enabled.load(Ordering::Acquire)
    }
    pub fn ready(&self) -> bool {
        self.initialized.load(Ordering::Acquire)
    }
    fn idle(&self) -> bool {
        self.inflight.load(Ordering::Acquire) == 0
            && self
                .sessions
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_empty()
            && !self.host.retained()
    }
    pub fn fail(&self, reason: &str) {
        self.dead.send_replace(true);
        for (_, sender) in self
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain()
        {
            let _ = sender.send(Err(reason.to_string()));
        }
        self.changed.notify_one();
    }
    pub fn stop(&self) {
        self.fail("Extension stopped by host");
    }
    pub async fn stop_and_wait(self: &Arc<Self>) {
        // Generation/session fences are applied by the caller before this point.
        if self.alive() {
            let _ = self
                .call("shutdown", json!({}), Duration::from_secs(5))
                .await;
        }
        let mut cleaned = self.cleaned.subscribe();
        self.stop();
        if !*cleaned.borrow() {
            let _ = tokio::time::timeout(Duration::from_secs(8), cleaned.wait_for(|v| *v)).await;
        }
    }
    pub async fn send(&self, message: Value) -> Result<(), String> {
        if !self.alive() {
            return Err("Extension is not running".into());
        }
        tokio::time::timeout(Duration::from_secs(5), self.writer.send(message))
            .await
            .map_err(|_| "Extension queue deadline exceeded")?
            .map_err(|_| "Extension pipe closed".into())
    }
    pub async fn call(
        self: &Arc<Self>,
        method: &str,
        params: Value,
        deadline: Duration,
    ) -> CoreResult {
        if !self.alive() {
            return Err("Extension is not running".into());
        }
        let id = format!("h:{}", uuid::Uuid::new_v4());
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().map_err(|e| e.to_string())?;
            if pending.len() >= MAX_PENDING {
                return Err("Too many extension requests".into());
            }
            pending.insert(id.clone(), tx);
        }
        self.inflight.fetch_add(1, Ordering::AcqRel);
        self.changed.notify_one();
        let mut guard = CallGuard {
            process: self.clone(),
            id: id.clone(),
            complete: false,
        };
        let expires = tokio::time::Instant::now() + deadline;
        tokio::time::timeout_at(
            expires,
            self.send(json!({"type":"request","id":id,"method":method,"params":params})),
        )
        .await
        .map_err(|_| "Extension request deadline exceeded")??;
        let result = tokio::time::timeout_at(expires, rx)
            .await
            .map_err(|_| "Extension request deadline exceeded")?
            .map_err(|_| "Extension response channel closed")?;
        guard.complete = true;
        result
    }
    pub async fn background_event(self: &Arc<Self>, params: Value) -> CoreResult {
        // A background callback can perform the same interactive host work as a
        // foreground command. Keep its default budget compatible with commands
        // and reverse RPC, without relying on any extension-specific policy.
        let result = self
            .call("background.event", params, Duration::from_secs(120))
            .await;
        if result.is_err() {
            self.fail("Background event failed");
        }
        result
    }
    pub async fn dispose(self: &Arc<Self>, id: &str) {
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        self.changed.notify_one();
        if self.alive()
            && self
                .call(
                    "session.dispose",
                    json!({"sessionId":id}),
                    Duration::from_secs(3),
                )
                .await
                .is_err()
        {
            self.fail("Extension session cleanup failed");
        }
        if self
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
        {
            let _ = self.host.release_view_resources().await;
        }
    }
}
struct StopOnDrop(Option<Arc<Process>>);
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            p.stop();
        }
    }
}
struct CallGuard {
    process: Arc<Process>,
    id: String,
    complete: bool,
}
impl Drop for CallGuard {
    fn drop(&mut self) {
        self.process
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.id);
        self.process.inflight.fetch_sub(1, Ordering::AcqRel);
        self.process.changed.notify_one();
        if !self.complete && self.process.alive() {
            let mut cancelled = self
                .process
                .cancelled
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if cancelled.len() >= 65536 {
                self.process.fail("Cancelled request tracking exhausted");
                return;
            }
            cancelled.insert(self.id.clone());
            drop(cancelled);
            if self.process.writer.try_send(json!({"type":"cancel","requestId":self.id,"reason":"host-cancelled-or-deadline"})).is_err(){self.process.fail("Cancellation queue unavailable");}
        }
    }
}
async fn read_loop(
    weak: Weak<Process>,
    stdout: tokio::process::ChildStdout,
    hello: oneshot::Sender<CoreResult>,
    stop: &mut watch::Receiver<bool>,
) {
    let mut reader = BufReader::new(stdout);
    let mut hello = Some(hello);
    let mut tasks = tokio::task::JoinSet::new();
    let mut reverse = HashMap::new();
    let mut seen = std::collections::HashSet::new();
    let result:Result<(),String>=async {loop{tokio::select! { biased;
        _=stop.changed()=>break,
        Some(completed)=tasks.join_next(),if !tasks.is_empty()=>{if let Ok(id)=completed{reverse.remove(&id);}},
        frame=read_frame(&mut reader)=>{
            let frame=frame?.ok_or("Extension stdout EOF")?;
            let value:Value=serde_json::from_slice(&frame).map_err(|_|"Malformed extension JSON")?;
            if let Some(tx)=hello.take(){let _=tx.send(Ok(value));continue;}
            let p=weak.upgrade().ok_or("Host process owner gone")?;
            match value["type"].as_str(){
                Some("response")=>{
                    let id=value["id"].as_str().filter(|id|id.starts_with("h:")).ok_or("Invalid response id")?;
                    let has_result=value.get("result").is_some();let has_error=value.get("error").is_some();
                    if has_result==has_error{return Err("Response must have exactly one result/error".into());}
                    let result=if has_result{Ok(value["result"].clone())}else{
                        if !value["error"]["code"].is_string() || !value["error"]["message"].is_string(){return Err("Malformed RPC error".into());}
                        Err(value["error"]["message"].as_str().unwrap_or("Extension error").to_string())
                    };
                    let sender=p.pending.lock().map_err(|e|e.to_string())?.remove(id);
                    if let Some(tx)=sender{let _=tx.send(result);}
                    else if !p.cancelled.lock().map_err(|e|e.to_string())?.remove(id){return Err("Unknown or duplicate extension response id".into());}
                },
                Some("request")=>{
                    let id=value["id"].as_str().filter(|id|id.strip_prefix("e:").is_some_and(|s|uuid::Uuid::parse_str(s).is_ok())).ok_or("Invalid reverse request id")?.to_string();
                    // Bounded replay tracking. An exhausted connection must be
                    // reopened rather than accepting a duplicate side effect.
                    if reverse.len()>=32 || seen.len()>=65536 || !seen.insert(id.clone()){return Err("Reverse RPC limit/duplicate id".into());}
                    let method=value["method"].as_str().ok_or("Missing reverse method")?.to_string();let params=value["params"].clone();
                    let task_id=id.clone();
                    let abort=tasks.spawn(async move {
                        let result=tokio::time::timeout(Duration::from_secs(120),p.host.request(&p,&method,params)).await.unwrap_or_else(|_|Err("Host capability deadline exceeded".into()));
                        let response=match result{Ok(result)=>json!({"type":"response","id":task_id,"result":result}),Err(message)=>json!({"type":"response","id":task_id,"error":{"code":"host-error","message":message}})};
                        if p.send(response).await.is_err(){p.fail("Reverse RPC reply failed");}task_id
                    });reverse.insert(id,abort);
                },
                Some("cancel")=>{let id=value["requestId"].as_str().ok_or("Missing cancelled id")?;if let Some(task)=reverse.remove(id){task.abort();}},
                Some("event")=>{
                    if value["event"]=="launcher.invalidated" {
                        if !p.launcher_enabled() { return Err("Launcher event not negotiated".into()); }
                        let d=&value["data"];
                        let key=d["contributionId"].as_str().ok_or("Missing contribution ID")?;
                        let revision=d["revision"].as_u64().ok_or("Missing launcher revision")?;
                        let mut queries=p.launcher_queries.lock().map_err(|e|e.to_string())?;
                        if let Some((query_id,text,old))=queries.get_mut(key) {
                            if d["queryId"]==*query_id && revision>*old {
                                *old=revision;p.launcher_revision.fetch_add(1,Ordering::AcqRel);
                                p.host.ctx.emit("launcher:invalidated",json!({"text":text,"queryId":query_id}));
                            }
                        }
                        continue;
                    }
                    if value["event"]!="session.invalidated" {return Err("Unsupported extension event".into());}
                    let id=value["sessionId"].as_str().ok_or("Missing event session")?;let revision=value["revision"].as_u64().ok_or("Missing event revision")?;
                    let mut sessions=p.sessions.lock().map_err(|e|e.to_string())?;
                    if let Some(previous)=sessions.get_mut(id){if revision>*previous{*previous=revision;p.host.ctx.emit("extension:session-invalidated",json!({"sessionId":id,"revision":revision,"generation":p.generation}));}}
                },
                _=>return Err("Unsupported extension envelope".into())
            }
        }
    }}Ok(())}.await;
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    if let Some(p) = weak.upgrade() {
        p.fail(
            result
                .as_ref()
                .err()
                .map(String::as_str)
                .unwrap_or("Extension reader stopped"),
        );
    }
    if let Some(tx) = hello {
        let _ = tx.send(Err(result
            .err()
            .unwrap_or("Extension closed before hello".into())));
    }
}
pub(super) async fn read_frame<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Option<Vec<u8>>, String> {
    let mut frame = Vec::new();
    loop {
        let b = reader.fill_buf().await.map_err(|e| e.to_string())?;
        if b.is_empty() {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Err("Truncated extension frame".into())
            };
        }
        let end = b.iter().position(|b| *b == b'\n');
        let n = end.map_or(b.len(), |i| i + 1);
        let data_len = if end.is_some() { n - 1 } else { n };
        if frame.len() + data_len > MAX_FRAME {
            return Err("Extension frame exceeds 16MiB".into());
        }
        frame.extend_from_slice(&b[..n]);
        reader.consume(n);
        if end.is_some() {
            frame.pop();
            if frame.contains(&b'\r') {
                return Err("Protocol requires LF, not CRLF".into());
            }
            return Ok(Some(frame));
        }
    }
}
struct ProcessTree {
    pid: u32,
    armed: bool,
    #[cfg(windows)]
    job: windows_sys::Win32::Foundation::HANDLE,
}
// The job handle is owned exclusively by the supervisor task and closed on drop.
#[cfg(windows)]
unsafe impl Send for ProcessTree {}
#[cfg(windows)]
#[link(name = "ntdll")]
extern "system" {
    fn NtResumeProcess(process: windows_sys::Win32::Foundation::HANDLE) -> i32;
}

impl ProcessTree {
    fn new(pid: u32) -> Result<Self, String> {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::{
                Foundation::*,
                System::{JobObjects::*, Threading::*},
            };
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err("Cannot create extension job object".into());
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let process = OpenProcess(
                PROCESS_SET_QUOTA | PROCESS_TERMINATE | PROCESS_SUSPEND_RESUME,
                false as i32,
                pid,
            );
            if process.is_null()
                || SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const _,
                    std::mem::size_of_val(&info) as u32,
                ) == 0
                || AssignProcessToJobObject(job, process) == 0
            {
                if !process.is_null() {
                    TerminateProcess(process, 1);
                    CloseHandle(process);
                }
                CloseHandle(job);
                return Err("Cannot contain extension process tree in Windows Job Object".into());
            }
            if NtResumeProcess(process) < 0 {
                TerminateProcess(process, 1);
                CloseHandle(process);
                CloseHandle(job);
                return Err("Cannot resume contained extension process".into());
            }
            CloseHandle(process);
            return Ok(Self {
                pid,
                job,
                armed: true,
            });
        }
        #[cfg(not(windows))]
        Ok(Self { pid, armed: true })
    }
    fn terminate(&mut self) {
        if !self.armed {
            return;
        }
        self.armed = false;
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.pid as i32), libc::SIGKILL);
        }
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job, 1);
        }
    }
}
impl Drop for ProcessTree {
    fn drop(&mut self) {
        self.terminate();
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.job);
        }
    }
}

#[cfg(test)]
mod environment_tests {
    use super::*;

    #[test]
    fn child_environment_keeps_public_client_id_and_filters_credentials() {
        let mut command = tokio::process::Command::new("fixture");
        command.env("OPENAI_API_KEY", "synthetic-not-a-key");
        let values = HashMap::from([
            ("PATH", "/fixture/bin"),
            (
                "TEZBAR_GOOGLE_OAUTH_CLIENT_ID",
                "fixture.apps.googleusercontent.com",
            ),
            (
                "TEZBAR_GOOGLE_OAUTH_CLIENT_SECRET",
                "synthetic-not-a-secret",
            ),
            ("OPENAI_API_KEY", "synthetic-not-a-key"),
            ("GITHUB_TOKEN", "synthetic-not-a-token"),
        ]);
        child_environment(&mut command, |key| {
            values.get(key).map(std::ffi::OsString::from)
        });
        let actual = command
            .as_std()
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_str().unwrap().to_string(),
                    value.unwrap().to_str().unwrap().to_string(),
                )
            })
            .collect::<HashMap<_, _>>();
        assert_eq!(
            actual,
            HashMap::from([
                ("PATH".to_string(), "/fixture/bin".to_string()),
                (
                    "TEZBAR_GOOGLE_OAUTH_CLIENT_ID".to_string(),
                    "fixture.apps.googleusercontent.com".to_string()
                ),
            ])
        );
    }
}
