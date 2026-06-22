use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

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
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCreateRequest {
    cwd: Option<String>,
    initial_command: Option<String>,
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
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
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

fn login_shell() -> String {
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

#[tauri::command]
pub fn native_terminal_create(
    app: AppHandle,
    state: State<'_, NativeTerminalState>,
    request: TerminalCreateRequest,
) -> Result<TerminalCreateResult, String> {
    let cwd = working_directory(request.cwd.as_deref());
    let shell = login_shell();
    let pair = native_pty_system()
        .openpty(pty_size(request.cols, request.rows))
        .map_err(|error| format!("failed to open terminal: {error}"))?;
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("failed to open terminal output: {error}"))?;
    let mut writer = pair
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

    if let Some(initial_command) = request.initial_command.filter(|value| !value.is_empty()) {
        if let Err(error) = writer
            .write_all(format!("{initial_command}\r").as_bytes())
            .and_then(|_| writer.flush())
        {
            let _ = child.kill();
            return Err(format!("failed to run initial command: {error}"));
        }
    }

    let session_id = format!(
        "native-terminal-{}",
        state.next_id.fetch_add(1, Ordering::Relaxed)
    );
    let killer = child.clone_killer();
    state.sessions.lock().unwrap().insert(
        session_id.clone(),
        NativeTerminalSession {
            master: pair.master,
            writer,
            killer,
        },
    );

    let output_app = app.clone();
    let output_session_id = session_id.clone();
    std::thread::spawn(move || {
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    let _ = output_app.emit(
                        "terminal:data",
                        TerminalDataEvent {
                            session_id: output_session_id.clone(),
                            data: String::from_utf8_lossy(&buffer[..count]).into_owned(),
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
