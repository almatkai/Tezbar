use tauri::{AppHandle, Manager};
pub use tezbar_core::terminal::pty::*;
fn core_context(app: &AppHandle) -> Result<tezbar_core::common::CoreContext, String> {
    app.try_state::<std::sync::Arc<tezbar_core::NativeCore>>()
        .map(|c| c.ctx.clone())
        .ok_or("Native core is unavailable".into())
}
pub fn shutdown_all(app: &AppHandle) {
    if let Ok(ctx) = core_context(app) {
        tezbar_core::terminal::pty::shutdown_all(&ctx.terminal);
    }
}
// Tauri executes non-async commands on the UI thread. Keep the synchronous
// entry points for the legacy bridge (which already uses spawn_blocking), and
// run direct invoke calls on blocking workers as well. The synchronous metadata
// transaction and PTY operations must not block a Tokio async worker.
#[tauri::command]
pub async fn native_terminal_create(
    app: AppHandle,
    request: TerminalCreateRequest,
) -> Result<TerminalCreateResult, String> {
    tokio::task::spawn_blocking(move || {
        native_terminal_create_impl(core_context(&app)?, &core_context(&app)?.terminal, request)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn native_terminal_attach(
    app: AppHandle,
    request: TerminalAttachRequest,
) -> Result<Option<TerminalAttachResult>, String> {
    tokio::task::spawn_blocking(move || {
        native_terminal_attach_impl(core_context(&app)?, &core_context(&app)?.terminal, request)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn native_terminal_cwd(
    app: AppHandle,
    session_id: String,
) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || {
        native_terminal_cwd_impl(
            core_context(&app)?,
            &core_context(&app)?.terminal,
            session_id,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn native_terminal_delete_history(
    app: AppHandle,
    session_id: String,
) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || {
        native_terminal_delete_history_impl(
            core_context(&app)?,
            &core_context(&app)?.terminal,
            session_id,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn native_terminal_prune_history(
    app: AppHandle,
    session_ids: Vec<String>,
) -> Result<usize, String> {
    tokio::task::spawn_blocking(move || {
        native_terminal_prune_history_impl(
            core_context(&app)?,
            &core_context(&app)?.terminal,
            session_ids,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn native_terminal_detach(app: AppHandle, session_id: String) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || {
        native_terminal_detach_impl(
            core_context(&app)?,
            &core_context(&app)?.terminal,
            session_id,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

// Write and resize stay synchronous on purpose: the renderer fires writes
// without awaiting them, and only the UI thread handles IPC in arrival order.
// Async commands would race keystrokes on separate workers. Both are cheap now:
// write only enqueues bytes for the session's writer thread.
#[tauri::command]
pub fn native_terminal_write(
    app: AppHandle,
    session_id: String,
    data: String,
) -> Result<bool, String> {
    native_terminal_write_impl(&core_context(&app)?.terminal, session_id, data)
}

#[tauri::command]
pub fn native_terminal_resize(
    app: AppHandle,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<bool, String> {
    native_terminal_resize_impl(&core_context(&app)?.terminal, session_id, cols, rows)
}

#[tauri::command]
pub async fn native_terminal_kill(app: AppHandle, session_id: String) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || {
        native_terminal_kill_impl(&core_context(&app)?.terminal, session_id)
    })
    .await
    .map_err(|error| error.to_string())?
}
