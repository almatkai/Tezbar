use crate::native_core::common::CoreContext;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

const OUTPUT_REPLAY_LIMIT_BYTES: usize = 512 * 1024;

pub struct NativeTerminalState {
    sessions: Mutex<HashMap<String, NativeTerminalSession>>,
    next_id: AtomicU64,
    waiters: Mutex<Vec<std::thread::JoinHandle<()>>>,
    stopped: Condvar,
    closing: AtomicBool,
}

impl Default for NativeTerminalState {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            waiters: Mutex::new(Vec::new()),
            stopped: Condvar::new(),
            closing: AtomicBool::new(false),
        }
    }
}

impl NativeTerminalState {
    pub fn wait_stopped(&self, id: &str) -> bool {
        let guard = self.sessions.lock().unwrap();
        let (guard, _) = self
            .stopped
            .wait_timeout_while(guard, std::time::Duration::from_secs(5), |sessions| {
                sessions.contains_key(id)
            })
            .unwrap();
        !guard.contains_key(id)
    }
}

pub fn shutdown_all(app: &AppHandle) {
    if let Some(state) = app.try_state::<NativeTerminalState>() {
        state.closing.store(true, Ordering::Release);
        if let Ok(mut sessions) = state.sessions.lock() {
            for session in sessions.values_mut() {
                let _ = stop_session(session);
            }
        }
        // Wait threads own Child and always reap it, including during app shutdown.
        let waiters = std::mem::take(&mut *state.waiters.lock().unwrap());
        for waiter in waiters {
            let _ = waiter.join();
        }
    }
}

fn core_context(app: &AppHandle) -> Result<CoreContext, String> {
    app.try_state::<std::sync::Arc<crate::native_core::NativeCore>>()
        .map(|core| core.ctx.clone())
        .ok_or_else(|| "Native core is unavailable".into())
}

fn metadata(ctx: &CoreContext, channel: &str, payload: Value) -> Result<Value, String> {
    let launcher = ctx
        .app
        .as_ref()
        .and_then(|app| {
            app.try_state::<std::sync::Arc<crate::native_core::NativeCore>>()
                .map(|core| core.launcher.clone())
        })
        .ok_or("Native launcher is unavailable")?;
    tauri::async_runtime::block_on(launcher.handle(channel, &[payload]))
        .unwrap_or_else(|| Err(format!("Terminal metadata channel unavailable: {channel}")))
}

struct NativeTerminalSession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    shell: String,
    cwd: String,
    pid: Option<u32>,
    exited: std::sync::Arc<AtomicBool>,
    history_path: PathBuf,
    legacy_history_path: Option<PathBuf>,
    output_chunks: VecDeque<String>,
    output_bytes: usize,
}

fn stop_session(session: &mut NativeTerminalSession) -> Result<(), String> {
    if session.exited.load(Ordering::Acquire) {
        return Ok(());
    }
    #[cfg(unix)]
    if let Some(pid) = session.pid {
        // portable-pty only sends SIGHUP, which interactive shells can ignore.
        // Include background job groups attached to this isolated PTY. The
        // explicit PID/group signals below remain the fallback on minimal hosts.
        if let Some(tty) = session.master.tty_name() {
            if let Some(name) = tty.to_str().and_then(|name| name.strip_prefix("/dev/")) {
                let _ = ProcessCommand::new("/usr/bin/pkill")
                    .args(["-KILL", "-t", name])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        }
        if session.exited.load(Ordering::Acquire) {
            return Ok(());
        }
        if let Some(group) = session.master.process_group_leader() {
            if group > 1 && group != unsafe { libc::getpgrp() } {
                unsafe {
                    libc::kill(-group, libc::SIGKILL);
                }
            }
        }
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
        return Ok(());
    }
    #[cfg(windows)]
    if let Some(pid) = session.pid {
        let status = ProcessCommand::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            return Ok(());
        }
    }
    session.killer.kill().map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCreateRequest {
    cwd: Option<String>,
    name: Option<String>,
    save_for: Option<String>,
    keep_alive_for: Option<String>,
    initial_command: Option<String>,
    restore_session_id: Option<String>,
    restore_command: Option<String>,
    cols: u16,
    rows: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCreateResult {
    session_id: String,
    shell: String,
    cwd: String,
    summary: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalAttachRequest {
    session_id: String,
    cols: u16,
    rows: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalAttachResult {
    session_id: String,
    shell: String,
    cwd: String,
    recent_output: String,
    summary: Value,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalDataEvent {
    session_id: String,
    data: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalExitEvent {
    session_id: String,
    exit_code: u32,
    signal: Option<u32>,
}

fn working_directory_for_home(requested: Option<&str>, home: &Path) -> PathBuf {
    let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) else {
        return home.to_path_buf();
    };
    let absolute_prefixes = [
        "/Users/",
        "/Volumes/",
        "/private/",
        "/tmp/",
        "/var/",
        "/System/",
        "/Library/",
    ];
    let is_filesystem_absolute = absolute_prefixes
        .iter()
        .any(|prefix| requested.starts_with(prefix))
        || requested == "/Users"
        || requested == "/Volumes";
    let candidate = if requested == "~" {
        home.to_path_buf()
    } else if let Some(tail) = requested.strip_prefix("~/") {
        home.join(tail)
    } else if requested.starts_with('/') && !is_filesystem_absolute {
        // The launcher displays paths below the home directory as `/Desktop/…`
        // to keep its input compact. Resolve that syntax before spawning the
        // shell; treating it as a filesystem-root path would silently fall
        // back to the home directory.
        home.join(requested.trim_start_matches('/'))
    } else {
        PathBuf::from(requested)
    };
    if candidate.is_dir() {
        candidate
    } else {
        home.to_path_buf()
    }
}

fn working_directory(requested: Option<&str>) -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    working_directory_for_home(requested, &home)
}

fn parse_lsof_working_directory(output: &str) -> Option<String> {
    output
        .lines()
        .find_map(|line| line.strip_prefix('n'))
        .filter(|path| PathBuf::from(path).is_dir())
        .map(str::to_string)
}

#[cfg(target_os = "macos")]
fn process_working_directory(pid: u32) -> Option<String> {
    let output = ProcessCommand::new("/usr/sbin/lsof")
        .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
        .and_then(|stdout| parse_lsof_working_directory(&stdout))
}

#[cfg(target_os = "linux")]
fn process_working_directory(pid: u32) -> Option<String> {
    fs::read_link(format!("/proc/{pid}/cwd"))
        .ok()
        .filter(|path| path.is_dir())
        .map(|path| path.to_string_lossy().into_owned())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn process_working_directory(_pid: u32) -> Option<String> {
    None
}

fn refresh_session_working_directory(session: &mut NativeTerminalSession) -> String {
    if let Some(cwd) = session.pid.and_then(process_working_directory) {
        session.cwd = cwd;
    }
    session.cwd.clone()
}

fn login_shell() -> String {
    #[cfg(target_os = "windows")]
    {
        return std::env::var("TEZBAR_TERMINAL_SHELL")
            .ok()
            .filter(|shell| PathBuf::from(shell).is_file())
            .unwrap_or_else(|| "powershell.exe".to_string());
    }
    #[cfg(not(target_os = "windows"))]
    std::env::var("SHELL")
        .ok()
        .filter(|shell| PathBuf::from(shell).is_file())
        .unwrap_or_else(|| "/bin/zsh".to_string())
}

fn pty_size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        cols: cols.clamp(2, 500),
        rows: rows.clamp(2, 300),
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn valid_history_session_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id.len() <= 200
        && session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn terminal_history_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = core_context(app)?.paths.config_dir.join("terminal-history");
    fs::create_dir_all(&dir)
        .map_err(|error| format!("failed to create terminal history directory: {error}"))?;
    #[cfg(unix)]
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("failed to secure terminal history directory: {error}"))?;
    Ok(dir)
}

fn terminal_history_path(app: &AppHandle, session_id: &str) -> Result<PathBuf, String> {
    if !valid_history_session_id(session_id) {
        return Err("invalid terminal session id".to_string());
    }
    let path = terminal_history_dir(app)?.join(format!("{session_id}.log"));
    let legacy = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("terminal-history")
        .join(format!("{session_id}.log"));
    // Copy, never move or delete the existing native history. If both versions
    // have logs, retain both and use the canonical profile log going forward.
    if !path.exists() && legacy.is_file() && legacy != path {
        let mut source = fs::File::open(legacy).map_err(|e| e.to_string())?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        match options.open(&path) {
            Ok(mut target) => {
                std::io::copy(&mut source, &mut target).map_err(|e| e.to_string())?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(path)
}

fn read_persisted_history(path: &Path, legacy: Option<&Path>) -> Option<String> {
    let current = fs::read(path).ok();
    let older = legacy
        .filter(|old| *old != path)
        .and_then(|old| fs::read(old).ok());
    let bytes = match (current, older) {
        (Some(current), Some(mut older)) if !current.starts_with(&older) => {
            older.extend(current);
            older
        }
        (Some(current), _) => current,
        (None, Some(older)) => older,
        _ => return None,
    };
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn legacy_terminal_history(command: Option<&str>) -> String {
    let safe_command: String = command
        .unwrap_or_default()
        .chars()
        .filter(|character| !character.is_control())
        .take(4_096)
        .collect();
    if safe_command.trim().is_empty() {
        return String::new();
    }
    format!(
        "[Previous output was not recorded by this app version]\r\n$ {}\r\n\r\n",
        safe_command.trim()
    )
}

fn append_output(session: &mut NativeTerminalSession, data: String) {
    session.output_bytes += data.len();
    session.output_chunks.push_back(data);
    while session.output_bytes > OUTPUT_REPLAY_LIMIT_BYTES && session.output_chunks.len() > 1 {
        if let Some(removed) = session.output_chunks.pop_front() {
            session.output_bytes = session.output_bytes.saturating_sub(removed.len());
        }
    }
}

fn recent_output(session: &NativeTerminalSession) -> String {
    session.output_chunks.iter().cloned().collect()
}

// Retain incomplete codepoints across read boundaries while preserving raw disk bytes.
#[derive(Default)]
struct TerminalUtf8Decoder {
    pending: Vec<u8>,
}
impl TerminalUtf8Decoder {
    fn decode(&mut self, bytes: &[u8], eof: bool) -> String {
        self.pending.extend_from_slice(bytes);
        let mut output = String::new();
        let mut consumed = 0;
        while consumed < self.pending.len() {
            match std::str::from_utf8(&self.pending[consumed..]) {
                Ok(text) => {
                    output.push_str(text);
                    consumed = self.pending.len();
                }
                Err(error) => {
                    let end = consumed + error.valid_up_to();
                    output.push_str(std::str::from_utf8(&self.pending[consumed..end]).unwrap());
                    consumed = end;
                    match error.error_len() {
                        Some(n) => {
                            output.push('�');
                            consumed += n;
                        }
                        None if eof => {
                            output.push('�');
                            consumed = self.pending.len();
                        }
                        None => break,
                    }
                }
            }
        }
        self.pending.drain(..consumed);
        output
    }
}

#[tauri::command]
pub fn native_terminal_create(
    app: AppHandle,
    state: State<'_, NativeTerminalState>,
    request: TerminalCreateRequest,
) -> Result<TerminalCreateResult, String> {
    let ctx = core_context(&app)?;
    if request
        .initial_command
        .as_ref()
        .is_some_and(|v| v.len() > 16 * 1024)
    {
        return Err("Initial terminal command exceeds 16 KiB".into());
    }
    // Serialize create/restore through summary publication, before any exit can
    // run. Keep ids reserved until their old wait/output threads finish.
    let mut sessions = state.sessions.lock().unwrap();
    if state.closing.load(Ordering::Acquire) {
        return Err("Terminals are shutting down".into());
    }
    let restore_session_id = match request.restore_session_id {
        Some(session_id) if valid_history_session_id(&session_id) => Some(session_id),
        Some(_) => return Err("invalid terminal session id".to_string()),
        None => None,
    };
    let session_id = restore_session_id.clone().unwrap_or_else(|| {
        format!(
            "native-terminal-{}-{}",
            now_millis(),
            state.next_id.fetch_add(1, Ordering::Relaxed)
        )
    });
    if sessions.contains_key(&session_id) {
        return Err("terminal session is already running".to_string());
    }
    if restore_session_id.is_some() {
        let config = ctx.config.read()?;
        if !config["terminalSessions"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|row| row["sessionId"] == session_id)
        {
            return Err("Terminal summary to restore was not found".into());
        }
    }
    let history_path = terminal_history_path(&app, &session_id)?;
    let seed_legacy_history = restore_session_id.is_some()
        && fs::metadata(&history_path)
            .map(|metadata| metadata.len() == 0)
            .unwrap_or(true);
    let mut history_options = OpenOptions::new();
    history_options
        .create(true)
        .write(true)
        .append(restore_session_id.is_some())
        .truncate(restore_session_id.is_none());
    #[cfg(unix)]
    history_options.mode(0o600);
    let mut history_file = history_options
        .open(&history_path)
        .map_err(|error| format!("failed to open terminal history: {error}"))?;
    if seed_legacy_history {
        let history = legacy_terminal_history(request.restore_command.as_deref());
        history_file
            .write_all(history.as_bytes())
            .and_then(|_| history_file.flush())
            .map_err(|error| format!("failed to seed terminal history: {error}"))?;
    }

    let cwd = working_directory(request.cwd.as_deref());
    let shell = login_shell();
    let pair = native_pty_system()
        .openpty(pty_size(request.cols, request.rows))
        .map_err(|error| format!("failed to open terminal: {error}"))?;
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("failed to open terminal output: {error}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| format!("failed to open terminal input: {error}"))?;

    let mut command = CommandBuilder::new(&shell);
    #[cfg(not(target_os = "windows"))]
    command.arg("-l");
    command.cwd(&cwd);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("TERM_PROGRAM", "Tezbar");

    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| format!("failed to start shell: {error}"))?;
    drop(pair.slave);

    let initial_command = request.initial_command.clone().filter(|v| !v.is_empty());

    let exited = std::sync::Arc::new(AtomicBool::new(false));
    let pid = child.process_id();
    let killer = child.clone_killer();
    let summary = match metadata(
        &ctx,
        if restore_session_id.is_some() {
            "terminal:native-restore"
        } else {
            "terminal:native-create"
        },
        json!({"sessionId":session_id,"shell":shell,"cwd":cwd,"initialCommand":request.initial_command,
            "name":request.name,"saveFor":request.save_for,"keepAliveFor":request.keep_alive_for}),
    ) {
        Ok(value) if !value.is_null() => value,
        result => {
            let _ = child.kill();
            #[cfg(unix)]
            if let Some(pid) = child.process_id() {
                unsafe {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
            let _ = child.wait();
            return Err(result
                .err()
                .unwrap_or_else(|| "Terminal summary to restore was not found".into()));
        }
    };
    sessions.insert(
        session_id.clone(),
        NativeTerminalSession {
            master: pair.master,
            writer,
            killer,
            shell: shell.clone(),
            cwd: cwd.to_string_lossy().into_owned(),
            pid,
            exited: exited.clone(),
            history_path,
            legacy_history_path: app.path().app_data_dir().ok().map(|dir| {
                dir.join("terminal-history")
                    .join(format!("{session_id}.log"))
            }),
            output_chunks: VecDeque::new(),
            output_bytes: 0,
        },
    );

    let mut waiters = state.waiters.lock().unwrap();
    drop(sessions);
    #[cfg(unix)]
    let output_exited = exited.clone();
    #[cfg(unix)]
    let output_fd = state
        .sessions
        .lock()
        .unwrap()
        .get(&session_id)
        .and_then(|s| s.master.as_raw_fd());
    let output_app = app.clone();
    let output_session_id = session_id.clone();
    let output_thread = std::thread::spawn(move || {
        let mut decoder = TerminalUtf8Decoder::default();
        let mut history_file = history_file;
        let mut buffer = [0_u8; 16 * 1024];
        let mut first_chunk = true;
        #[cfg(unix)]
        let mut drain_started: Option<std::time::Instant> = None;
        loop {
            #[cfg(unix)]
            if output_exited.load(Ordering::Acquire) {
                if drain_started
                    .get_or_insert_with(std::time::Instant::now)
                    .elapsed()
                    > std::time::Duration::from_secs(1)
                {
                    break;
                }
            }
            #[cfg(unix)]
            if let Some(fd) = output_fd {
                let mut descriptor = libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                };
                let ready = unsafe { libc::poll(&mut descriptor, 1, 100) };
                if ready == 0 {
                    if output_exited.load(Ordering::Acquire) {
                        break;
                    }
                    continue;
                }
                if ready < 0 {
                    if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                        continue;
                    }
                    break;
                }
            }
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    // On the very first chunk of output the shell prompt is ready.
                    // Write the initial command now so it is not swallowed by
                    // shell initialisation (.zshrc / .zprofile loading).
                    if first_chunk {
                        first_chunk = false;
                        if let Some(ref cmd) = initial_command {
                            if let Some(session) = output_app
                                .state::<NativeTerminalState>()
                                .sessions
                                .lock()
                                .unwrap()
                                .get_mut(&output_session_id)
                            {
                                let _ = session
                                    .writer
                                    .write_all(format!("{cmd}\r").as_bytes())
                                    .and_then(|_| session.writer.flush());
                            }
                        }
                    }

                    let data = decoder.decode(&buffer[..count], false);
                    // Keep the complete stream on disk. The in-memory replay
                    // buffer remains bounded, but saved sessions can always
                    // recover their full terminal history after an app restart.
                    let _ = history_file
                        .write_all(&buffer[..count])
                        .and_then(|_| history_file.flush());
                    if let Some(session) = output_app
                        .state::<NativeTerminalState>()
                        .sessions
                        .lock()
                        .unwrap()
                        .get_mut(&output_session_id)
                    {
                        append_output(session, data.clone());
                    }
                    let _ = output_app.emit(
                        "terminal:data",
                        TerminalDataEvent {
                            session_id: output_session_id.clone(),
                            data,
                        },
                    );
                }
            }
        }
        let data = decoder.decode(&[], true);
        if !data.is_empty() {
            if let Some(session) = output_app
                .state::<NativeTerminalState>()
                .sessions
                .lock()
                .unwrap()
                .get_mut(&output_session_id)
            {
                append_output(session, data.clone());
            }
            let _ = output_app.emit(
                "terminal:data",
                TerminalDataEvent {
                    session_id: output_session_id.clone(),
                    data,
                },
            );
        }
    });

    let exit_app = app.clone();
    let exit_session_id = session_id.clone();
    let waiter = std::thread::spawn(move || {
        let (exit_code, signal) = match child.wait() {
            Ok(status) => (status.exit_code(), status.signal().map(|_| 1)),
            Err(_) => (1, None),
        };
        exited.store(true, Ordering::Release);
        // Drain the last output before publishing exit/removing the id.
        let _ = output_thread.join();
        let state = exit_app.state::<NativeTerminalState>();
        let mut sessions = state.sessions.lock().unwrap();
        if let Err(error) = metadata(
            &ctx,
            "core:terminal:exit",
            json!({"sessionId":exit_session_id,"exitCode":exit_code,"signal":signal}),
        ) {
            log::error!("Could not persist terminal exit: {error}");
        }
        sessions.remove(&exit_session_id);
        state.stopped.notify_all();
        let _ = exit_app.emit(
            "terminal:exit",
            TerminalExitEvent {
                session_id: exit_session_id,
                exit_code,
                signal,
            },
        );
    });

    waiters.retain(|waiter| !waiter.is_finished());
    waiters.push(waiter);
    Ok(TerminalCreateResult {
        session_id,
        shell,
        cwd: cwd.to_string_lossy().into_owned(),
        summary,
    })
}

#[tauri::command]
pub fn native_terminal_attach(
    app: AppHandle,
    state: State<'_, NativeTerminalState>,
    request: TerminalAttachRequest,
) -> Result<Option<TerminalAttachResult>, String> {
    let mut sessions = state.sessions.lock().unwrap();
    let Some(session) = sessions.get_mut(&request.session_id) else {
        return Ok(None);
    };
    if session.exited.load(Ordering::Acquire) {
        return Ok(None);
    }
    session
        .master
        .resize(pty_size(request.cols, request.rows))
        .map_err(|error| format!("failed to resize terminal: {error}"))?;
    let cwd = refresh_session_working_directory(session);
    metadata(
        &core_context(&app)?,
        "terminal:update",
        json!({"sessionId":request.session_id,"cwd":cwd}),
    )?;
    Ok(Some(TerminalAttachResult {
        session_id: request.session_id.clone(),
        shell: session.shell.clone(),
        cwd,
        summary: metadata(
            &core_context(&app)?,
            "terminal:native-attach",
            json!({"sessionId":request.session_id}),
        )?,
        recent_output: read_persisted_history(
            &session.history_path,
            session.legacy_history_path.as_deref(),
        )
        .unwrap_or_else(|| recent_output(session)),
    }))
}

#[tauri::command]
pub fn native_terminal_cwd(
    app: AppHandle,
    state: State<'_, NativeTerminalState>,
    session_id: String,
) -> Result<Option<String>, String> {
    let mut sessions = state.sessions.lock().unwrap();
    let Some(session) = sessions.get_mut(&session_id) else {
        return Ok(None);
    };
    let cwd = refresh_session_working_directory(session);
    metadata(
        &core_context(&app)?,
        "terminal:update",
        json!({"sessionId":session_id,"cwd":cwd}),
    )?;
    Ok(Some(cwd))
}

#[tauri::command]
pub fn native_terminal_delete_history(
    app: AppHandle,
    state: State<'_, NativeTerminalState>,
    session_id: String,
) -> Result<bool, String> {
    if state.sessions.lock().unwrap().contains_key(&session_id) {
        return Ok(false);
    }
    let path = terminal_history_path(&app, &session_id)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("failed to delete terminal history: {error}")),
    }
}

#[tauri::command]
pub fn native_terminal_prune_history(
    app: AppHandle,
    state: State<'_, NativeTerminalState>,
    session_ids: Vec<String>,
) -> Result<usize, String> {
    // Retention is driven by the Rust summary owner, never by a potentially
    // stale renderer list. Keep this old command as a harmless compatibility API.
    let _ = (app, state, session_ids);
    Ok(0)
}

#[tauri::command]
pub fn native_terminal_detach(
    app: AppHandle,
    state: State<'_, NativeTerminalState>,
    session_id: String,
) -> Result<bool, String> {
    let mut sessions = state.sessions.lock().unwrap();
    let Some(session) = sessions.get_mut(&session_id) else {
        return Ok(false);
    };
    let cwd = refresh_session_working_directory(session);
    metadata(
        &core_context(&app)?,
        "terminal:update",
        json!({"sessionId":session_id,"cwd":cwd}),
    )?;
    metadata(
        &core_context(&app)?,
        "terminal:detach",
        json!({"sessionId":session_id}),
    )?;
    Ok(true)
}

#[tauri::command]
pub fn native_terminal_write(
    state: State<'_, NativeTerminalState>,
    session_id: String,
    data: String,
) -> Result<bool, String> {
    if data.is_empty() || data.len() > 64 * 1024 {
        return Ok(false);
    }
    let mut sessions = state.sessions.lock().unwrap();
    let Some(session) = sessions.get_mut(&session_id) else {
        return Ok(false);
    };
    session
        .writer
        .write_all(data.as_bytes())
        .and_then(|_| session.writer.flush())
        .map_err(|error| format!("failed to write to terminal: {error}"))?;
    Ok(true)
}

#[tauri::command]
pub fn native_terminal_resize(
    state: State<'_, NativeTerminalState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<bool, String> {
    let sessions = state.sessions.lock().unwrap();
    let Some(session) = sessions.get(&session_id) else {
        return Ok(false);
    };
    session
        .master
        .resize(pty_size(cols, rows))
        .map_err(|error| format!("failed to resize terminal: {error}"))?;
    Ok(true)
}

#[tauri::command]
pub fn native_terminal_kill(
    state: State<'_, NativeTerminalState>,
    session_id: String,
) -> Result<bool, String> {
    let mut sessions = state.sessions.lock().unwrap();
    let Some(session) = sessions.get_mut(&session_id) else {
        return Ok(false);
    };
    stop_session(session)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn terminal_stop_reaps_a_controlled_shell_that_ignores_hangup() {
        let directory = tempfile::tempdir().unwrap();
        let pair = native_pty_system().openpty(pty_size(80, 24)).unwrap();
        let mut reader = pair.master.try_clone_reader().unwrap();
        let writer = pair.master.take_writer().unwrap();
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "trap '' HUP; printf READY; while :; do :; done"]);
        command.cwd(directory.path());
        let mut child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        let mut session = NativeTerminalSession {
            master: pair.master,
            writer,
            killer: child.clone_killer(),
            shell: "/bin/sh".into(),
            cwd: directory.path().to_string_lossy().into_owned(),
            pid: child.process_id(),
            exited: std::sync::Arc::new(AtomicBool::new(false)),
            history_path: directory.path().join("history.log"),
            legacy_history_path: None,
            output_chunks: VecDeque::new(),
            output_bytes: 0,
        };
        let mut ready = [0; 5];
        reader.read_exact(&mut ready).unwrap();
        stop_session(&mut session).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if std::time::Instant::now() >= deadline {
                unsafe {
                    libc::kill(session.pid.unwrap() as i32, libc::SIGKILL);
                }
                let _ = child.wait();
                panic!("Controlled terminal shell did not stop");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        child.wait().unwrap();
        for _ in 0..100 {
            append_output(&mut session, "🚀".repeat(4096));
        }
        let replay = recent_output(&session);
        assert!(replay.len() <= OUTPUT_REPLAY_LIMIT_BYTES);
        assert!(replay.ends_with("🚀"));
        assert_eq!(replay.len(), session.output_bytes);
    }

    #[test]
    fn terminal_utf8_preserves_every_multibyte_split_and_invalid_eof() {
        let text = "prompt Ж қазақша 🚀 é\r\n";
        for split in 0..=text.len() {
            let mut decoder = TerminalUtf8Decoder::default();
            let mut actual = decoder.decode(&text.as_bytes()[..split], false);
            actual.push_str(&decoder.decode(&text.as_bytes()[split..], false));
            actual.push_str(&decoder.decode(&[], true));
            assert_eq!(actual, text, "split {split}");
        }
        let mut decoder = TerminalUtf8Decoder::default();
        let mut actual = String::new();
        for byte in text.as_bytes() {
            actual.push_str(&decoder.decode(&[*byte], false));
        }
        assert_eq!(actual, text);
        assert_eq!(decoder.decode(&[0xff, 0xe2, 0x82], false), "�");
        assert_eq!(decoder.decode(&[], true), "�");
    }

    #[test]
    fn terminal_history_reads_both_locations_without_destroying_raw_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join("profile.log");
        let legacy = dir.path().join("native.log");
        fs::write(&current, b"new output\xff").unwrap();
        fs::write(&legacy, "old 🚀\r\n").unwrap();
        assert_eq!(
            read_persisted_history(&current, Some(&legacy)).unwrap(),
            "old 🚀\r\nnew output�"
        );
        assert_eq!(fs::read(&current).unwrap(), b"new output\xff");
        assert_eq!(fs::read_to_string(&legacy).unwrap(), "old 🚀\r\n");
        fs::write(&current, "old 🚀\r\nappended").unwrap();
        assert_eq!(
            read_persisted_history(&current, Some(&legacy)).unwrap(),
            "old 🚀\r\nappended"
        );
    }

    #[test]
    fn history_session_ids_cannot_escape_the_history_directory() {
        assert!(valid_history_session_id("native-terminal-123-4"));
        assert!(!valid_history_session_id("../config"));
        assert!(!valid_history_session_id("nested/session"));
        assert!(!valid_history_session_id(""));
    }

    #[test]
    fn parses_the_cwd_record_from_lsof_field_output() {
        assert_eq!(
            parse_lsof_working_directory("p123\nfcwd\nn/tmp\n"),
            Some("/tmp".to_string())
        );
    }

    #[test]
    fn compact_launcher_paths_use_home_as_their_root() {
        let home = std::env::temp_dir().join(format!(
            "tezbar-native-terminal-home-{}",
            std::process::id()
        ));
        let directory = home.join("Desktop/code");
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&directory).unwrap();

        assert_eq!(
            working_directory_for_home(Some("/Desktop/code/"), &home),
            directory
        );

        fs::remove_dir_all(home).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn pty_keeps_follow_up_commands_in_the_same_shell() {
        let pair = native_pty_system().openpty(pty_size(80, 24)).unwrap();
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut writer = pair.master.take_writer().unwrap();
        let mut command = CommandBuilder::new("/bin/zsh");
        command.arg("-f");
        let mut child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);

        let (output_tx, output_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buffer = [0_u8; 4096];
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                let _ = output_tx.send(String::from_utf8_lossy(&buffer[..count]).into_owned());
            }
        });

        std::thread::sleep(std::time::Duration::from_millis(100));
        writer.write_all(b"printf '__FIRST__\\n'\r").unwrap();
        writer.flush().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        writer.write_all(b"cd /tmp\r").unwrap();
        writer.flush().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        writer
            .write_all(b"printf '__CWD__:%s\\n' \"$PWD\"\r")
            .unwrap();
        writer.flush().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        writer.write_all(b"exit\r").unwrap();
        writer.flush().unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while child.try_wait().unwrap().is_none() {
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                panic!(
                    "shell did not exit; output was: {:?}",
                    output_rx.try_iter().collect::<String>()
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        drop(writer);
        drop(pair.master);

        std::thread::sleep(std::time::Duration::from_millis(100));
        let output = output_rx.try_iter().collect::<String>();
        assert!(output.contains("__FIRST__"), "output was: {output:?}");
        assert!(output.contains("__CWD__:/tmp"), "output was: {output:?}");
    }
}
