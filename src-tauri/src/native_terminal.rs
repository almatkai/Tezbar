use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

const OUTPUT_REPLAY_LIMIT_BYTES: usize = 512 * 1024;

pub struct NativeTerminalState {
    sessions: Mutex<HashMap<String, NativeTerminalSession>>,
    next_id: AtomicU64,
}

impl Default for NativeTerminalState {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }
}

struct NativeTerminalSession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    shell: String,
    cwd: String,
    pid: Option<u32>,
    history_path: PathBuf,
    output_chunks: VecDeque<String>,
    output_bytes: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCreateRequest {
    cwd: Option<String>,
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

fn working_directory(requested: Option<&str>) -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) else {
        return home;
    };
    let candidate = if requested == "~" {
        home.clone()
    } else if let Some(tail) = requested.strip_prefix("~/") {
        home.join(tail)
    } else {
        PathBuf::from(requested)
    };
    if candidate.is_dir() {
        candidate
    } else {
        home
    }
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
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to locate terminal history: {error}"))?
        .join("terminal-history");
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
    Ok(terminal_history_dir(app)?.join(format!("{session_id}.log")))
}

fn read_persisted_history(path: &PathBuf) -> Option<String> {
    fs::read_to_string(path).ok()
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

#[tauri::command]
pub fn native_terminal_create(
    app: AppHandle,
    state: State<'_, NativeTerminalState>,
    request: TerminalCreateRequest,
) -> Result<TerminalCreateResult, String> {
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
    if state.sessions.lock().unwrap().contains_key(&session_id) {
        return Err("terminal session is already running".to_string());
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

    let initial_command = request.initial_command.filter(|v| !v.is_empty());

    let pid = child.process_id();
    let killer = child.clone_killer();
    state.sessions.lock().unwrap().insert(
        session_id.clone(),
        NativeTerminalSession {
            master: pair.master,
            writer,
            killer,
            shell: shell.clone(),
            cwd: cwd.to_string_lossy().into_owned(),
            pid,
            history_path,
            output_chunks: VecDeque::new(),
            output_bytes: 0,
        },
    );

    let output_app = app.clone();
    let output_session_id = session_id.clone();
    std::thread::spawn(move || {
        let mut history_file = history_file;
        let mut buffer = [0_u8; 16 * 1024];
        let mut first_chunk = true;
        loop {
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

                    let data = String::from_utf8_lossy(&buffer[..count]).into_owned();
                    // Keep the complete stream on disk. The in-memory replay
                    // buffer remains bounded, but saved sessions can always
                    // recover their full terminal history after an app restart.
                    let _ = history_file
                        .write_all(data.as_bytes())
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
    });

    let exit_app = app.clone();
    let exit_session_id = session_id.clone();
    std::thread::spawn(move || {
        let (exit_code, signal) = match child.wait() {
            Ok(status) => (status.exit_code(), status.signal().map(|_| 1)),
            Err(_) => (1, None),
        };
        exit_app
            .state::<NativeTerminalState>()
            .sessions
            .lock()
            .unwrap()
            .remove(&exit_session_id);
        let _ = exit_app.emit(
            "terminal:exit",
            TerminalExitEvent {
                session_id: exit_session_id,
                exit_code,
                signal,
            },
        );
    });

    Ok(TerminalCreateResult {
        session_id,
        shell,
        cwd: cwd.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub fn native_terminal_attach(
    state: State<'_, NativeTerminalState>,
    request: TerminalAttachRequest,
) -> Result<Option<TerminalAttachResult>, String> {
    let mut sessions = state.sessions.lock().unwrap();
    let Some(session) = sessions.get_mut(&request.session_id) else {
        return Ok(None);
    };
    session
        .master
        .resize(pty_size(request.cols, request.rows))
        .map_err(|error| format!("failed to resize terminal: {error}"))?;
    Ok(Some(TerminalAttachResult {
        session_id: request.session_id,
        shell: session.shell.clone(),
        cwd: refresh_session_working_directory(session),
        recent_output: read_persisted_history(&session.history_path)
            .unwrap_or_else(|| recent_output(session)),
    }))
}

#[tauri::command]
pub fn native_terminal_cwd(
    state: State<'_, NativeTerminalState>,
    session_id: String,
) -> Result<Option<String>, String> {
    let mut sessions = state.sessions.lock().unwrap();
    let Some(session) = sessions.get_mut(&session_id) else {
        return Ok(None);
    };
    Ok(Some(refresh_session_working_directory(session)))
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
    let keep: HashSet<String> = session_ids
        .into_iter()
        .filter(|session_id| valid_history_session_id(session_id))
        .collect();
    let active: HashSet<String> = state.sessions.lock().unwrap().keys().cloned().collect();
    let mut removed = 0;
    for entry in fs::read_dir(terminal_history_dir(&app)?)
        .map_err(|error| format!("failed to read terminal history directory: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("failed to read terminal history entry: {error}"))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("log") {
            continue;
        }
        let Some(session_id) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if keep.contains(session_id) || active.contains(session_id) {
            continue;
        }
        if fs::remove_file(path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

#[tauri::command]
pub fn native_terminal_detach(
    state: State<'_, NativeTerminalState>,
    session_id: String,
) -> Result<bool, String> {
    Ok(state.sessions.lock().unwrap().contains_key(&session_id))
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
    let Some(mut session) = state.sessions.lock().unwrap().remove(&session_id) else {
        return Ok(false);
    };
    session
        .killer
        .kill()
        .map_err(|error| format!("failed to stop terminal: {error}"))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

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
