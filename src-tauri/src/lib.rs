use tezbar_host::placement::*;
// src-tauri/src/lib.rs
mod backend_health;
#[cfg(feature = "desktop-bench")]
mod desktop_bench;
use tezbar_core as native_core;
mod process_notifications;
mod tauri_host;
#[cfg(feature = "native-launcher-facade")]
pub use tezbar_core::launcher_facade;
pub use tezbar_core::native_v2_cli::dispatch as dispatch_native_v2_cli;
mod native_input;
mod window_layout;
use backend_health::BackendStatus;
mod manual_releases;
mod native_terminal;
mod qr_download;
mod release_channels;
use tezbar_core::resource_directory;
#[cfg(target_os = "macos")]
mod timer_notifications;
mod updater;
#[cfg(target_os = "macos")]
mod updater_temp;

#[cfg(target_os = "macos")]
use core_graphics::display::CGDisplay;
use serde_json::json;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
#[cfg(target_os = "macos")]
use tauri::LogicalPosition;
#[cfg(not(target_os = "macos"))]
use tauri::PhysicalSize;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, Monitor, PhysicalPosition, State, WebviewWindow,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

const TERMINAL_SESSIONS_LABEL: &str = "terminal-sessions";
const TERMINAL_SESSIONS_WIDTH: f64 = 300.0;
// The sessions sidebar sits to the LEFT of the main window. A positive overhang
// means the sidebar extends that many logical pixels beyond the left edge of main.
const TERMINAL_SESSIONS_LEFT_OVERHANG: f64 = 266.0;
const TERMINAL_SESSIONS_TOP_OFFSET: f64 = 96.0;
const TERMINAL_SESSIONS_BOTTOM_OFFSET: f64 = 86.0;
const TERMINAL_SESSIONS_MIN_HEIGHT: f64 = 260.0;
const SNAP_OVERLAY_LABEL: &str = "window-snap-overlay";
const DEFAULT_BACKEND_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const EXTENSION_INSTALL_REQUEST_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const EXTENSION_RUNTIME_REQUEST_TIMEOUT: Duration = Duration::from_secs(2 * 60);

struct BackendState {
    status: Mutex<BackendStatus>,
    shutting_down: AtomicBool,
}

fn set_backend_status(app: &AppHandle, phase: &str, detail: &str) {
    let state = app.state::<BackendState>();
    let mut status = state.status.lock().unwrap();
    if status.phase == phase && status.detail == detail {
        return;
    }
    if phase == "ready" && status.phase != "ready" {
        status.generation += 1;
    }
    status.phase = phase.into();
    status.detail = detail.into();
    status.revision += 1;
    log::info!("Backend {phase}: {detail}");
    let _ = app.emit("backend-status", status.clone());
}

#[tauri::command]
fn get_backend_status(state: State<'_, BackendState>) -> BackendStatus {
    state.status.lock().unwrap().clone()
}

#[tauri::command]
async fn restart_backend(
    app: AppHandle,
    core: State<'_, Arc<native_core::NativeCore>>,
) -> Result<(), String> {
    core.extensions.restart_worker().await;
    match core.start().await {
        Ok(()) => {
            set_backend_status(&app, "ready", "Rust core ready");
            Ok(())
        }
        Err(error) => {
            set_backend_status(&app, "failed", &error);
            Err(error)
        }
    }
}

fn backend_request_timeout(channel: &str) -> Duration {
    match channel {
        "file:quick-look" => Duration::from_secs(24 * 60 * 60),
        "app:confirm-quit" => Duration::from_secs(10 * 60),
        "ai:action" => Duration::from_secs(16 * 60),
        "chat:generate-meta" => Duration::from_secs(30),
        "voice:stt:transcribe" => Duration::from_secs(15 * 60),
        "pi:install" => Duration::from_secs(4 * 60),
        "llm-list-models" => Duration::from_secs(70),
        "voice:models:list" | "voice:stt:modes" | "voice:model:download" => Duration::from_secs(60),
        // Deep search can cold-start the native embedding runtime before inference.
        "search:all" => Duration::from_secs(150),
        // Installing can include a runtime download, source download,
        // dependency installation, and an esbuild pass. Each stage has its own
        // bounded timeout, so the bridge must not abandon the request first.
        "extension:install" | "extensions:install" | "extensions:reinstall" => {
            EXTENSION_INSTALL_REQUEST_TIMEOUT
        }
        // Extension commands can legitimately wait on their own network
        // request. Keep the general RPC timeout short while allowing these
        // explicitly long-running operations to finish.
        "extension:run-command"
        | "extension:invoke-action"
        | "extension:refresh-session"
        | "extension:search-text-changed"
        | "extension:load-more"
        | "search:benchmark:run" => EXTENSION_RUNTIME_REQUEST_TIMEOUT,
        _ => DEFAULT_BACKEND_REQUEST_TIMEOUT,
    }
}

#[derive(Default)]
struct WindowBehaviorState {
    suppress_blur_hide: Mutex<bool>,
    settings_close: Mutex<(u64, bool)>,
    backend_hidden_windows: Mutex<Vec<String>>,
    // Desktop window managers can position the hidden predeclared launcher and
    // emit Moved before Tezbar has selected its real placement. Ignore those
    // startup events so they cannot overwrite the user's saved location.
    main_window_placed: Mutex<bool>,
    // SetPosition is asynchronous on Windows. A generation keeps an older
    // mixed-DPI finalizer from racing a newer launcher invocation.
    #[cfg(target_os = "windows")]
    window_placement_generation: Mutex<u64>,
    snap_drag_active: Mutex<bool>,
    snap_locked: Mutex<SnapLockState>,
    snap_guides: Mutex<SnapGuidesState>,
    snap_motion: Mutex<SnapMotionState>,
    snap_drag_generation: Mutex<u64>,
}

static IDLE_TRIM_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn emit_main_window_visibility(window: &WebviewWindow, visible: bool) {
    if window.label() == "main" {
        let _ = window.emit("window-visibility", visible);
        if visible {
            IDLE_TRIM_GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        } else {
            let _ = window.emit("window-hidden", ());
            schedule_idle_memory_trim(window);
        }
    }
}

fn schedule_idle_memory_trim(window: &WebviewWindow) {
    let generation = IDLE_TRIM_GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    let window_clone = window.clone();
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn(async move {
        // Wait 5 seconds of sustained idle state before reclaiming memory
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        if IDLE_TRIM_GENERATION.load(std::sync::atomic::Ordering::SeqCst) != generation {
            return;
        }
        if window_clone.is_visible().unwrap_or(false) {
            return;
        }
        // Tell Webview renderer to run JS garbage collection & trim client caches
        let _ = window_clone.eval(
            "window.dispatchEvent(new CustomEvent('tezbar-idle-trim')); if (window.gc) { window.gc(); }",
        );
        // Trim NativeCore (SQLite cache, search cache)
        if let Some(core) = app.try_state::<std::sync::Arc<crate::native_core::NativeCore>>() {
            core.trim_idle_memory().await;
        } else {
            crate::native_core::trim_os_process_memory();
        }
    });
}

pub(crate) fn show_main_window_native(window: &WebviewWindow) -> Result<(), String> {
    window.show().map_err(|error| error.to_string())?;
    emit_main_window_visibility(window, true);
    Ok(())
}

fn hide_main_window_native(window: &WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|error| error.to_string())?;
    emit_main_window_visibility(window, false);
    Ok(())
}

fn set_backend_app_visibility(app: &AppHandle, visible: bool) {
    let state = app.state::<WindowBehaviorState>();
    if visible {
        let labels = {
            let mut hidden = state.backend_hidden_windows.lock().unwrap();
            std::mem::take(&mut *hidden)
        };
        for label in labels {
            if let Some(window) = app.get_webview_window(&label) {
                let _ = window.show();
                emit_main_window_visibility(&window, true);
            }
        }
        return;
    }

    let mut hidden = state.backend_hidden_windows.lock().unwrap();
    hidden.clear();
    for (label, window) in app.webview_windows() {
        if window.is_visible().unwrap_or(false) {
            hidden.push(label);
            let _ = window.hide();
            emit_main_window_visibility(&window, false);
        }
    }
}

fn emit_snap_guides(
    window: &WebviewWindow,
    app: &AppHandle,
    visible: bool,
    locked: SnapLockState,
    target_rect: Option<SnapTargetRect>,
) {
    let guides = SnapGuidesState {
        visible,
        snap_x: locked.x,
        snap_y: locked.y,
        centered: locked.x && locked.y,
        target_rect,
    };
    *window
        .state::<WindowBehaviorState>()
        .snap_guides
        .lock()
        .unwrap() = guides;
    let payload = serde_json::to_value(guides).unwrap_or_default();
    let _ = window.emit("window:snap-guides", &payload);
    if let Some(overlay) = app.get_webview_window(SNAP_OVERLAY_LABEL) {
        let _ = overlay.emit("window:snap-guides", payload);
    }
}

fn ensure_snap_overlay_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(SNAP_OVERLAY_LABEL) {
        return Ok(window);
    }

    let builder = tauri::WebviewWindowBuilder::new(
        app,
        SNAP_OVERLAY_LABEL,
        tauri::WebviewUrl::App("index.html?window=snap-overlay".into()),
    )
    .title("Tezbar Snap Guides")
    .resizable(false)
    .decorations(false)
    .always_on_top(true)
    .transparent(true)
    .skip_taskbar(true)
    .visible(false)
    .focusable(false)
    .shadow(false);
    let window = builder.build().map_err(|error| error.to_string())?;
    window
        .set_ignore_cursor_events(true)
        .map_err(|error| error.to_string())?;
    Ok(window)
}

fn sync_snap_overlay(
    app: &AppHandle,
    main_window: &WebviewWindow,
    monitor: &Monitor,
    window_width: f64,
    window_height: f64,
    locked: SnapLockState,
    create_if_missing: bool,
) -> Result<(), String> {
    let overlay = match app.get_webview_window(SNAP_OVERLAY_LABEL) {
        Some(overlay) => overlay,
        None if create_if_missing => ensure_snap_overlay_window(app)?,
        None => return Ok(()),
    };
    let work_area = monitor.work_area();
    let position = work_area.position;
    let size = work_area.size;
    let needs_position_update = overlay
        .outer_position()
        .map(|current| current.x != position.x || current.y != position.y)
        .unwrap_or(true);
    let needs_size_update = overlay
        .outer_size()
        .map(|current| current.width != size.width || current.height != size.height)
        .unwrap_or(true);
    if needs_position_update {
        #[cfg(target_os = "macos")]
        {
            let logical = logical_position_for_monitor(
                PersistedWindowPosition {
                    x: position.x as f64,
                    y: position.y as f64,
                },
                monitor.scale_factor(),
            );
            overlay
                .set_position(LogicalPosition::new(logical.x, logical.y))
                .map_err(|error| error.to_string())?;
        }
        #[cfg(not(target_os = "macos"))]
        overlay
            .set_position(PhysicalPosition::new(position.x, position.y))
            .map_err(|error| error.to_string())?;
    }
    if needs_size_update {
        #[cfg(target_os = "macos")]
        overlay
            .set_size(LogicalSize::new(
                size.width as f64 / valid_scale_factor(monitor.scale_factor()),
                size.height as f64 / valid_scale_factor(monitor.scale_factor()),
            ))
            .map_err(|error| error.to_string())?;
        #[cfg(not(target_os = "macos"))]
        overlay
            .set_size(PhysicalSize::new(size.width, size.height))
            .map_err(|error| error.to_string())?;
    }
    if !overlay.is_visible().unwrap_or(false) {
        overlay.show().map_err(|error| error.to_string())?;
    }
    let target_rect = snap_target_rect(window_width, window_height, monitor_geometry(monitor));
    emit_snap_guides(main_window, app, true, locked, Some(target_rect));
    Ok(())
}

fn hide_snap_overlay(app: &AppHandle, main_window: Option<&WebviewWindow>) {
    if let Some(overlay) = app.get_webview_window(SNAP_OVERLAY_LABEL) {
        let _ = overlay.hide();
    }
    if let Some(window) = main_window {
        emit_snap_guides(window, app, false, SnapLockState::default(), None);
    }
}

fn quit_app_now(app: &AppHandle) {
    let state = app.state::<BackendState>();
    if state.shutting_down.swap(true, Ordering::AcqRel) {
        return;
    }
    let _ = app.global_shortcut().unregister_all();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        native_terminal::shutdown_all(&app);
        if let Some(core) = app.try_state::<Arc<native_core::NativeCore>>() {
            let _ = tokio::time::timeout(Duration::from_secs(8), core.shutdown()).await;
        }
        let _ = tauri::async_runtime::spawn_blocking(|| flush_window_positions(None)).await;
        app.exit(0);
    });
}

fn openray_config_path() -> Option<PathBuf> {
    if let Some(path) = PROFILE_CONFIG_PATH.get() {
        return Some(path.clone());
    }
    tezbar_host::paths::config_path(
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from),
        std::env::var_os("TEZBAR_CONFIG_DIR").map(PathBuf::from),
    )
}

static PROFILE_CONFIG_PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
static PROFILE_STORE: std::sync::OnceLock<Arc<native_core::common::ConfigStore>> =
    std::sync::OnceLock::new();

#[cfg(test)]
use tezbar_core::bun::bun_executable_name;
#[cfg(test)]
use tezbar_core::bun::locate_bun;
fn read_openray_config() -> serde_json::Value {
    if let Some(config) = PROFILE_STORE.get() {
        return config.read().unwrap_or_else(|error| {
            log::error!("Profile: {error}");
            json!({})
        });
    }
    openray_config_path()
        .and_then(|path| native_core::common::ConfigStore::new(path).read().ok())
        .unwrap_or_else(|| json!({}))
}

static WINDOW_POSITIONS: std::sync::LazyLock<Mutex<PendingWindowPositions>> =
    std::sync::LazyLock::new(|| Mutex::new(PendingWindowPositions::default()));

// Only background writers hold this lock; UI updates never wait for disk I/O.
static WINDOW_POSITION_WRITER: Mutex<()> = Mutex::new(());

fn flush_window_positions(generation: Option<u64>) {
    let _writer = WINDOW_POSITION_WRITER.lock().unwrap();
    let (positions, snapshot_generation) = {
        let pending = WINDOW_POSITIONS.lock().unwrap();
        if !pending.should_flush(generation) {
            return;
        }
        (pending.positions.clone(), pending.generation)
    };
    let result = (|| -> Result<(), String> {
        let path = openray_config_path().ok_or("Configuration path unavailable")?;
        let fallback = Arc::new(native_core::common::ConfigStore::new(path));
        PROFILE_STORE.get().unwrap_or(&fallback).update(|value| {
            for (monitor_id, position) in &positions {
                update_window_position_config(value, monitor_id, *position);
            }
            Ok(())
        })?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            let mut pending = WINDOW_POSITIONS.lock().unwrap();
            if pending.generation == snapshot_generation {
                pending.dirty = false;
            }
        }
        Err(error) => log::warn!("Could not save window position: {error}"),
    }
}

fn stored_window_position_for_monitor(monitor_id: &str) -> StoredWindowPosition {
    if let Some(position) = WINDOW_POSITIONS
        .lock()
        .unwrap()
        .positions
        .get(monitor_id)
        .copied()
    {
        return StoredWindowPosition {
            position: Some(position),
            needs_reset: false,
        };
    }
    let config = read_openray_config();
    StoredWindowPosition {
        position: monitor_positions_from_config(&config)
            .get(monitor_id)
            .copied(),
        needs_reset: window_position_config_needs_reset(&config),
    }
}

fn set_persisted_window_position_for_monitor(
    window: &WebviewWindow,
    monitor: &Monitor,
    position: PersistedWindowPosition,
) {
    let Some(monitor_id) = monitor_id(window, monitor) else {
        return;
    };
    let generation = WINDOW_POSITIONS
        .lock()
        .unwrap()
        .remember(monitor_id, position);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        let should_flush = WINDOW_POSITIONS
            .lock()
            .unwrap()
            .should_flush(Some(generation));
        if should_flush {
            let _ = tauri::async_runtime::spawn_blocking(move || {
                flush_window_positions(Some(generation))
            })
            .await;
        }
    });
}

#[cfg(target_os = "macos")]
fn monitors_match(left: &Monitor, right: &Monitor) -> bool {
    left.name() == right.name()
        && left.position() == right.position()
        && left.size() == right.size()
        && left.scale_factor() == right.scale_factor()
}

#[cfg(target_os = "macos")]
fn monitor_id(window: &WebviewWindow, monitor: &Monitor) -> Option<String> {
    for display_id in CGDisplay::active_displays().ok()? {
        let bounds = CGDisplay::new(display_id).bounds();
        let candidate = window
            .monitor_from_point(
                bounds.origin.x + bounds.size.width / 2.0,
                bounds.origin.y + bounds.size.height / 2.0,
            )
            .ok()
            .flatten();
        if candidate
            .as_ref()
            .is_some_and(|candidate| monitors_match(candidate, monitor))
        {
            if let Some(uuid) = tezbar_host::monitors::display_uuid(display_id) {
                return Some(uuid);
            }
            break;
        }
    }

    // UUID matching should normally succeed, but persistence must not silently
    // stop if CoreGraphics and Tao briefly disagree while displays are changing.
    // This geometry-based ID remains stable for the usual monitor arrangement.
    let position = monitor.position();
    let size = monitor.size();
    Some(format!(
        "macos-fallback:{}:{}:{}:{}:{}:{:.3}",
        monitor.name().map(String::as_str).unwrap_or("unknown"),
        position.x,
        position.y,
        size.width,
        size.height,
        monitor.scale_factor(),
    ))
}

#[cfg(not(target_os = "macos"))]
fn monitor_id(_window: &WebviewWindow, monitor: &Monitor) -> Option<String> {
    monitor_id_from_name(monitor.name().map(String::as_str))
}

fn physical_monitor_work_area(monitor: &Monitor) -> (f64, f64, f64, f64) {
    let work_area = monitor.work_area();
    (
        work_area.position.x as f64,
        work_area.position.y as f64,
        work_area.size.width as f64,
        work_area.size.height as f64,
    )
}

fn monitor_geometry(monitor: &Monitor) -> MonitorGeometry {
    let position = monitor.position();
    let size = monitor.size();
    MonitorGeometry {
        bounds: (
            position.x as f64,
            position.y as f64,
            size.width as f64,
            size.height as f64,
        ),
        work_area: physical_monitor_work_area(monitor),
    }
}

fn monitor_for_position(
    window: &WebviewWindow,
    position: PersistedWindowPosition,
    window_width: f64,
    window_height: f64,
) -> Result<Monitor, String> {
    #[cfg(target_os = "macos")]
    let center = logical_window_center(
        position,
        (window_width, window_height),
        window.scale_factor().unwrap_or(1.0),
    );
    #[cfg(not(target_os = "macos"))]
    let center = PersistedWindowPosition {
        x: position.x + window_width / 2.0,
        y: position.y + window_height / 2.0,
    };
    window
        .monitor_from_point(center.x, center.y)
        .map_err(|error| error.to_string())?
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten())
        .ok_or_else(|| "No monitor found for window".to_string())
}

fn monitor_for_window(window: &WebviewWindow) -> Result<Monitor, String> {
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.outer_size().map_err(|error| error.to_string())?;
    monitor_for_position(
        window,
        PersistedWindowPosition {
            x: position.x as f64,
            y: position.y as f64,
        },
        size.width as f64,
        size.height as f64,
    )
}

fn update_window_snap_state(
    window: &WebviewWindow,
    app: &AppHandle,
    state: &WindowBehaviorState,
    raw_position: PersistedWindowPosition,
    from_timer: bool,
    move_window: bool,
) -> Result<bool, String> {
    if !*state.snap_drag_active.lock().unwrap() {
        return Ok(false);
    }

    let now = Instant::now();
    if !from_timer && !move_window {
        let mut motion = state.snap_motion.lock().unwrap();
        if let Some((programmatic, set_at)) = motion.programmatic_position {
            if now.duration_since(set_at) <= Duration::from_millis(100)
                && (raw_position.x - programmatic.x).abs() <= 1.0
                && (raw_position.y - programmatic.y).abs() <= 1.0
            {
                motion.programmatic_position = None;
                return Ok(false);
            }
            if now.duration_since(set_at) > Duration::from_millis(100) {
                motion.programmatic_position = None;
            }
        }
    }

    let size = window.outer_size().map_err(|error| error.to_string())?;
    let window_width = size.width as f64;
    let window_height = size.height as f64;
    let monitor = monitor_for_position(window, raw_position, window_width, window_height)?;
    let monitor_geometry = monitor_geometry(&monitor);
    let locked = *state.snap_locked.lock().unwrap();
    let (acquire, started_candidate, should_schedule_timer, previous_raw_position) = {
        let mut motion = state.snap_motion.lock().unwrap();
        let previous_raw_position = motion.last_raw_position;
        let (acquire, started_candidate, should_schedule_timer) = snap_acquire_state(
            &mut motion,
            SnapAcquireContext {
                position: raw_position,
                window_width,
                window_height,
                monitor: monitor_geometry,
                locked,
                now,
                from_timer,
            },
        );
        (
            acquire,
            started_candidate,
            should_schedule_timer,
            previous_raw_position,
        )
    };
    let (snapped_position, snapped_locked, _active) = snap_window_position(
        raw_position,
        window_width,
        window_height,
        monitor_geometry,
        locked,
        acquire,
    );
    let mut next_position = snapped_position;
    let mut next_locked = snapped_locked;
    {
        let mut motion = state.snap_motion.lock().unwrap();
        if locked.x {
            if let Some(previous_raw_position) = previous_raw_position {
                motion.snap_escape.x += raw_position.x - previous_raw_position.x;
            }
        } else {
            motion.snap_escape.x = 0.0;
        }
        if locked.y {
            if let Some(previous_raw_position) = previous_raw_position {
                motion.snap_escape.y += raw_position.y - previous_raw_position.y;
            }
        } else {
            motion.snap_escape.y = 0.0;
        }
        if !locked.x && snapped_locked.x {
            motion.snap_anchor.x = snapped_position.x;
            motion.snap_escape.x = 0.0;
        }
        if !locked.y && snapped_locked.y {
            motion.snap_anchor.y = snapped_position.y;
            motion.snap_escape.y = 0.0;
        }
        if locked.x && motion.snap_escape.x.abs() >= SNAP_RELEASE_THRESHOLD {
            next_locked.x = false;
            next_position.x = motion.snap_anchor.x + motion.snap_escape.x;
            motion.snap_escape.x = 0.0;
        }
        if locked.y && motion.snap_escape.y.abs() >= SNAP_RELEASE_THRESHOLD {
            next_locked.y = false;
            next_position.y = motion.snap_anchor.y + motion.snap_escape.y;
            motion.snap_escape.y = 0.0;
        }
        if next_locked.x {
            motion.slow_x_since = None;
        } else {
            motion.snap_escape.x = 0.0;
        }
        if next_locked.y {
            motion.slow_y_since = None;
        } else {
            motion.snap_escape.y = 0.0;
        }
    }
    *state.snap_locked.lock().unwrap() = next_locked;

    if move_window || next_position != raw_position {
        state.snap_motion.lock().unwrap().programmatic_position = Some((
            PersistedWindowPosition {
                x: next_position.x.round(),
                y: next_position.y.round(),
            },
            now,
        ));
        set_window_position_on_monitor(window, next_position, &monitor)?;
    }
    sync_snap_overlay(
        app,
        window,
        &monitor,
        window_width,
        window_height,
        next_locked,
        !cfg!(target_os = "windows"),
    )?;
    Ok(started_candidate || should_schedule_timer)
}

fn schedule_snap_dwell(window: WebviewWindow, generation: u64) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SNAP_DWELL_DURATION).await;
        let app = window.app_handle().clone();
        let state = app.state::<WindowBehaviorState>();
        if !*state.snap_drag_active.lock().unwrap() {
            return;
        }
        if state.snap_motion.lock().unwrap().generation != generation {
            return;
        }
        let raw_position = state.snap_motion.lock().unwrap().last_raw_position;
        if let Some(raw_position) = raw_position {
            if let Err(error) =
                update_window_snap_state(&window, &app, &state, raw_position, true, false)
            {
                log::debug!("delayed window snap update failed: {error}");
            }
        }
    });
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn cursor_monitor(window: &WebviewWindow) -> Option<Monitor> {
    let (x, y) = tezbar_host::monitors::cursor_point()?;
    window.monitor_from_point(x, y).ok().flatten()
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn cursor_monitor(window: &WebviewWindow) -> Option<Monitor> {
    let position = window.cursor_position().ok()?;
    window
        .monitor_from_point(position.x, position.y)
        .ok()
        .flatten()
}

fn frontmost_window_monitor(window: &WebviewWindow) -> Option<Monitor> {
    tezbar_host::monitors::frontmost(|x, y| window.monitor_from_point(x, y).ok().flatten())
}

fn active_monitor(window: &WebviewWindow) -> Result<Monitor, String> {
    select_active_monitor(
        cursor_monitor(window),
        frontmost_window_monitor(window),
        || window.current_monitor().ok().flatten(),
        || window.primary_monitor().ok().flatten(),
    )
    .ok_or_else(|| "No monitor found".to_string())
}

fn monitor_for_window_position(
    window: &WebviewWindow,
    position: PersistedWindowPosition,
    size: (f64, f64),
) -> Option<Monitor> {
    if let Some(monitor) = window.current_monitor().ok().flatten() {
        return Some(monitor);
    }

    #[cfg(target_os = "macos")]
    let center = logical_window_center(position, size, window.scale_factor().unwrap_or(1.0));
    #[cfg(not(target_os = "macos"))]
    let center = PersistedWindowPosition {
        x: position.x + size.0 / 2.0,
        y: position.y + size.1 / 2.0,
    };
    window.monitor_from_point(center.x, center.y).ok().flatten()
}

fn persist_window_position_at(window: &WebviewWindow, position: PersistedWindowPosition) {
    let Ok(size) = window.outer_size() else {
        return;
    };
    let Some(monitor) =
        monitor_for_window_position(window, position, (size.width as f64, size.height as f64))
    else {
        return;
    };
    set_persisted_window_position_for_monitor(window, &monitor, position);
}

fn persist_current_window_position(window: &WebviewWindow) {
    if window.label() == "main"
        && !*window
            .state::<WindowBehaviorState>()
            .main_window_placed
            .lock()
            .unwrap()
    {
        return;
    }

    let Ok(position) = window.outer_position() else {
        return;
    };
    persist_window_position_at(
        window,
        PersistedWindowPosition {
            x: position.x as f64,
            y: position.y as f64,
        },
    );
}

fn set_window_position(
    window: &WebviewWindow,
    position: PersistedWindowPosition,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
        };

        let hwnd = window.hwnd().map_err(|error| error.to_string())?;
        let success = unsafe {
            // Tauri/Tao normally queues a move with SWP_ASYNCWINDOWPOS, which
            // lets Show overtake it when a hidden launcher is reopened. Apply
            // this move synchronously so the window never becomes visible on
            // its previous monitor.
            SetWindowPos(
                hwnd.0 as _,
                std::ptr::null_mut(),
                position.x.round() as i32,
                position.y.round() as i32,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
            )
        };
        if success == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    window
        .set_position(PhysicalPosition::new(
            position.x.round() as i32,
            position.y.round() as i32,
        ))
        .map_err(|e| e.to_string())
}

fn set_window_position_on_monitor(
    window: &WebviewWindow,
    position: PersistedWindowPosition,
    _monitor: &Monitor,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use objc2::runtime::AnyObject;
        use objc2_foundation::NSPoint;

        let logical = logical_position_for_monitor(position, _monitor.scale_factor());
        let point = NSPoint::new(
            logical.x,
            CGDisplay::main().pixels_high() as f64 - logical.y,
        );
        let ns_window = window.ns_window().map_err(|error| error.to_string())? as usize;

        // Tao's macOS set_position queues setFrameTopLeftPoint on GCD, while
        // show() is synchronous. That lets the old monitor flash before the
        // queued move runs. Put a synchronous AppKit move on Tauri's main-thread
        // queue; the subsequent Show message cannot overtake it.
        window
            .run_on_main_thread(move || unsafe {
                let ns_window = &*(ns_window as *const AnyObject);
                let _: () = objc2::msg_send![ns_window, setFrameTopLeftPoint: point];
            })
            .map_err(|error| error.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        set_window_position(window, position)
    }
}

#[cfg(target_os = "windows")]
fn schedule_windows_window_placement_finalization(
    window: WebviewWindow,
    target_monitor_id: String,
    target_position: PersistedWindowPosition,
    target_size: (f64, f64),
    generation: u64,
    persist_position: bool,
) {
    tauri::async_runtime::spawn(async move {
        let placement_is_current = || {
            *window
                .state::<WindowBehaviorState>()
                .window_placement_generation
                .lock()
                .unwrap()
                == generation
        };
        let current_monitor_is_target = || {
            window
                .current_monitor()
                .ok()
                .flatten()
                .and_then(|monitor| monitor_id(&window, &monitor))
                .as_deref()
                == Some(target_monitor_id.as_str())
        };

        // Wait until Win32 has moved the hidden window onto the target monitor
        // and delivered its DPI transition. The initial move is synchronous;
        // this loop handles the later DPI resize and any compositor lag.
        let mut target_reached = false;
        for _ in 0..30 {
            tokio::time::sleep(Duration::from_millis(16)).await;
            if !placement_is_current() {
                return;
            }
            if current_monitor_is_target() {
                target_reached = true;
                break;
            }
            let _ = set_window_position(&window, target_position);
        }

        let mut placement_settled = false;
        if target_reached {
            // Min/max constraints are logical values. Reapply after the DPI
            // transition so the old monitor's DPI cannot clamp the saved size.
            if let Some(monitor) = window.current_monitor().ok().flatten() {
                let _ = window_layout::prepare(&window, &monitor);
            }
            let desired_size = PhysicalSize::new(
                target_size.0.round().max(1.0) as u32,
                target_size.1.round().max(1.0) as u32,
            );
            // Normally WM_DPICHANGED preserves logical dimensions. Windows can
            // intentionally skip that resize when "Show window contents while
            // dragging" is disabled, so finalize the physical size ourselves
            // and then reapply the physical top-left to keep exact centering.
            for _ in 0..4 {
                if !placement_is_current() {
                    return;
                }
                let _ = window.set_size(desired_size);
                let _ = set_window_position(&window, target_position);
                tokio::time::sleep(Duration::from_millis(16)).await;

                let size_matches = window.outer_size().ok() == Some(desired_size);
                let position_matches = window.outer_position().ok().is_some_and(|position| {
                    position.x == target_position.x.round() as i32
                        && position.y == target_position.y.round() as i32
                });
                if size_matches && position_matches && current_monitor_is_target() {
                    placement_settled = true;
                    break;
                }
            }
            if !placement_settled {
                log::warn!(
                    "Windows launcher reached target monitor {target_monitor_id} but did not settle at the requested size and position"
                );
            }
        } else {
            log::warn!(
                "Windows launcher did not reach target monitor {target_monitor_id} before placement finalization timed out"
            );
        }

        if !placement_is_current() {
            return;
        }
        *window
            .state::<WindowBehaviorState>()
            .main_window_placed
            .lock()
            .unwrap() = true;
        if target_reached && placement_settled && persist_position {
            persist_current_window_position(&window);
        }
    });
}

fn place_window(window: &WebviewWindow) -> Result<(), String> {
    let monitor = active_monitor(window)?;
    let profile = window_layout::prepare(window, &monitor)?;
    let (window_width, window_height) = profile.physical_size(monitor.scale_factor());
    let target_monitor_id = monitor_id(window, &monitor);
    let stored = target_monitor_id
        .as_deref()
        .map(stored_window_position_for_monitor)
        .unwrap_or_default();
    let position = plan_window_position(
        monitor_geometry(&monitor),
        window_width,
        window_height,
        stored.position,
    );
    let persist_position = stored.needs_reset || stored.position != Some(position);

    #[cfg(target_os = "windows")]
    {
        let state = window.state::<WindowBehaviorState>();
        *state.main_window_placed.lock().unwrap() = false;
        let generation = {
            let mut generation = state.window_placement_generation.lock().unwrap();
            *generation = generation.wrapping_add(1);
            *generation
        };
        // Queue the target physical size before the synchronous Win32 move.
        // The hidden window is then already sized for the destination DPI when
        // the caller sends its subsequent Show message.
        window
            .set_size(PhysicalSize::new(
                window_width.round().max(1.0) as u32,
                window_height.round().max(1.0) as u32,
            ))
            .map_err(|error| error.to_string())?;
        set_window_position_on_monitor(window, position, &monitor)?;
        if let Some(target_monitor_id) = target_monitor_id {
            schedule_windows_window_placement_finalization(
                window.clone(),
                target_monitor_id,
                position,
                (window_width, window_height),
                generation,
                persist_position,
            );
        } else {
            *state.main_window_placed.lock().unwrap() = true;
        }
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        window
            .set_size(LogicalSize::new(profile.width, profile.height))
            .map_err(|error| error.to_string())?;
        set_window_position_on_monitor(window, position, &monitor)?;
        *window
            .state::<WindowBehaviorState>()
            .main_window_placed
            .lock()
            .unwrap() = true;
        if persist_position {
            // Old placement formats used the wrong coordinate space. Reset
            // once and keep only monitor ID + coordinates.
            set_persisted_window_position_for_monitor(window, &monitor, position);
        }
        Ok(())
    }
}

fn schedule_drag_position_persistence(window: WebviewWindow) {
    persist_current_window_position(&window);
}

#[cfg(target_os = "windows")]
fn windows_cursor_position() -> Result<PersistedWindowPosition, String> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut point) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(PersistedWindowPosition {
        x: point.x as f64,
        y: point.y as f64,
    })
}

#[cfg(target_os = "windows")]
fn windows_left_mouse_button_is_down() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};

    unsafe { GetAsyncKeyState(VK_LBUTTON as i32) < 0 }
}

fn finish_window_snap_drag(window: &WebviewWindow, app: &AppHandle, state: &WindowBehaviorState) {
    persist_current_window_position(window);
    *state.snap_drag_active.lock().unwrap() = false;
    *state.snap_locked.lock().unwrap() = SnapLockState::default();
    *state.snap_motion.lock().unwrap() = SnapMotionState::default();
    hide_snap_overlay(app, Some(window));
    *state.suppress_blur_hide.lock().unwrap() = false;
    window_layout::schedule_capture(window.clone());
}

#[cfg(target_os = "windows")]
fn schedule_windows_snap_overlay(window: WebviewWindow, app: AppHandle, generation: u64) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<WindowBehaviorState>();
        if !*state.snap_drag_active.lock().unwrap()
            || state.snap_motion.lock().unwrap().generation != generation
        {
            return;
        }
        let result = monitor_for_window(&window).and_then(|monitor| {
            let size = window.outer_size().map_err(|error| error.to_string())?;
            let locked = *state.snap_locked.lock().unwrap();
            sync_snap_overlay(
                &app,
                &window,
                &monitor,
                size.width as f64,
                size.height as f64,
                locked,
                true,
            )
        });
        if let Err(error) = result {
            log::warn!("failed to show snap overlay: {error}");
        }
        if !*state.snap_drag_active.lock().unwrap()
            || state.snap_motion.lock().unwrap().generation != generation
        {
            hide_snap_overlay(&app, Some(&window));
        }
    });
}

#[cfg(target_os = "windows")]
fn schedule_windows_snap_drag(
    window: WebviewWindow,
    initial_window_position: PersistedWindowPosition,
    initial_cursor_position: PersistedWindowPosition,
    generation: u64,
) {
    tauri::async_runtime::spawn(async move {
        let app = window.app_handle().clone();
        let mut last_cursor_position = initial_cursor_position;
        loop {
            // 33ms (~30Hz) is still fast enough for flicker-free window
            // dragging but avoids waking the runtime every 8ms while the
            // user decides where to drop. The Tauri shell also hoists an
            // async worker per iteration, so tighter is not free.
            tokio::time::sleep(Duration::from_millis(33)).await;
            let state = app.state::<WindowBehaviorState>();
            if !*state.snap_drag_active.lock().unwrap()
                || state.snap_motion.lock().unwrap().generation != generation
            {
                return;
            }
            if !windows_left_mouse_button_is_down() {
                finish_window_snap_drag(&window, &app, &state);
                return;
            }
            let Ok(cursor_position) = windows_cursor_position() else {
                continue;
            };
            if cursor_position == last_cursor_position {
                continue;
            }
            last_cursor_position = cursor_position;
            let raw_position = window_position_for_cursor_drag(
                initial_window_position,
                initial_cursor_position,
                cursor_position,
            );
            match update_window_snap_state(&window, &app, &state, raw_position, false, true) {
                Ok(true) => schedule_snap_dwell(window.clone(), generation),
                Ok(false) => {}
                Err(error) => {
                    log::debug!("manual Windows window snap update failed: {error}");
                    let _ = set_window_position(&window, raw_position);
                }
            }
        }
    });
}

fn hide_main_window_for_settings(app: &AppHandle) -> Result<(), String> {
    let Some(main_window) = app.get_webview_window("main") else {
        return Ok(());
    };
    persist_current_window_position(&main_window);
    hide_terminal_sessions(app);
    hide_main_window_native(&main_window)
}

fn focus_settings_window_if_visible(app: &AppHandle) -> Result<bool, String> {
    let Some(settings_window) = app.get_webview_window("settings") else {
        return Ok(false);
    };
    if !settings_window
        .is_visible()
        .map_err(|error| error.to_string())?
    {
        return Ok(false);
    }

    // Settings is a mutually exclusive surface. Any attempt to activate the
    // launcher while Settings is open must keep the launcher hidden and bring
    // the existing Settings window forward instead.
    hide_main_window_for_settings(app)?;
    settings_window
        .unminimize()
        .map_err(|error| error.to_string())?;
    settings_window.show().map_err(|error| error.to_string())?;
    settings_window
        .set_focus()
        .map_err(|error| error.to_string())?;
    Ok(true)
}

fn open_settings_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("settings") {
        // Reuse Settings while it is open; closing releases the WebView.
        win.unminimize().map_err(|error| error.to_string())?;
        win.show().map_err(|error| error.to_string())?;
        win.set_focus().map_err(|error| error.to_string())?;
        hide_main_window_for_settings(&app)?;
        return Ok(());
    }

    let settings = tauri::WebviewWindowBuilder::new(
        &app,
        "settings",
        // Keep the resource URL identical to the configured main window. The
        // explicit initialization script selects the surface without relying
        // on a query-string URL being resolved by the dev/custom protocol.
        tauri::WebviewUrl::App("index.html?window=settings".into()),
    )
    .initialization_script("window.__TEZBAR_WINDOW_LABEL__ = 'settings';")
    .title(format!("{} Settings", app.package_info().name))
    .inner_size(920.0, 680.0)
    .resizable(true)
    .decorations(true)
    .build()
    .map_err(|e| e.to_string())?;

    settings.set_focus().map_err(|error| error.to_string())?;
    hide_main_window_for_settings(&app)?;
    Ok(())
}

fn restore_main_window(app: &AppHandle) -> Result<(), String> {
    if focus_settings_window_if_visible(app)? {
        return Ok(());
    }
    let Some(main_window) = app.get_webview_window("main") else {
        return Ok(());
    };
    place_window(&main_window)?;
    show_main_window_native(&main_window)?;
    main_window.set_focus().map_err(|e| e.to_string())?;
    let _ = main_window.emit("window-shown", json!({ "resetUi": false }));
    Ok(())
}

fn terminal_sessions_layout(
    main_window: &WebviewWindow,
) -> Result<(PersistedWindowPosition, f64), String> {
    let position = main_window.outer_position().map_err(|e| e.to_string())?;
    let size = main_window.outer_size().map_err(|e| e.to_string())?;
    let scale_factor = main_window
        .current_monitor()
        .map_err(|e| e.to_string())?
        .map(|monitor| monitor.scale_factor())
        .unwrap_or(1.0);

    let main_height = size.height as f64 / scale_factor;
    let sidebar_height =
        (main_height - TERMINAL_SESSIONS_TOP_OFFSET - TERMINAL_SESSIONS_BOTTOM_OFFSET)
            .max(TERMINAL_SESSIONS_MIN_HEIGHT);
    let x = position.x as f64 - TERMINAL_SESSIONS_LEFT_OVERHANG * scale_factor;
    let y = position.y as f64 + TERMINAL_SESSIONS_TOP_OFFSET * scale_factor;

    Ok((PersistedWindowPosition { x, y }, sidebar_height))
}

fn ensure_terminal_sessions_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(TERMINAL_SESSIONS_LABEL) {
        return Ok(window);
    }

    let mut builder = tauri::WebviewWindowBuilder::new(
        app,
        TERMINAL_SESSIONS_LABEL,
        tauri::WebviewUrl::App("index.html?window=terminal-sessions".into()),
    )
    .title("Terminal Sessions")
    .inner_size(TERMINAL_SESSIONS_WIDTH, TERMINAL_SESSIONS_MIN_HEIGHT)
    .resizable(false)
    .decorations(false)
    .always_on_top(true)
    .transparent(true)
    .skip_taskbar(true)
    .visible(false)
    .shadow(false);

    if let Some(main_window) = app.get_webview_window("main") {
        builder = builder.parent(&main_window).map_err(|e| e.to_string())?;
    }

    let sidebar = builder.build().map_err(|e| e.to_string())?;
    Ok(sidebar)
}

fn sync_terminal_sessions_window(app: &AppHandle) -> Result<(), String> {
    let Some(main_window) = app.get_webview_window("main") else {
        return Ok(());
    };
    let Some(sidebar) = app.get_webview_window(TERMINAL_SESSIONS_LABEL) else {
        return Ok(());
    };
    let (position, height) = terminal_sessions_layout(&main_window)?;
    sidebar
        .set_size(LogicalSize::new(TERMINAL_SESSIONS_WIDTH, height))
        .map_err(|e| e.to_string())?;
    set_window_position(&sidebar, position)
}

fn hide_terminal_sessions(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(TERMINAL_SESSIONS_LABEL) {
        let _ = window.hide();
    }
}

#[tauri::command]
async fn open_settings_window_cmd(app: AppHandle) -> Result<(), String> {
    open_settings_window(app)
}

#[tauri::command]
fn hide_launcher_for_settings(window: WebviewWindow) -> Result<(), String> {
    // The native open path is authoritative. This renderer callback is an
    // additional first-paint safeguard for startup and WebView reloads.
    if window.label() != "settings" {
        return Ok(());
    }
    hide_main_window_for_settings(window.app_handle())
}

#[tauri::command]
fn terminal_sessions_show(app: AppHandle) -> Result<(), String> {
    let main_window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window not found".to_string())?;
    let sidebar = ensure_terminal_sessions_window(&app)?;
    let (position, height) = terminal_sessions_layout(&main_window)?;
    sidebar
        .set_size(LogicalSize::new(TERMINAL_SESSIONS_WIDTH, height))
        .map_err(|e| e.to_string())?;
    set_window_position(&sidebar, position)?;
    sidebar.show().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn terminal_sessions_hide(app: AppHandle) -> Result<(), String> {
    hide_terminal_sessions(&app);
    Ok(())
}

#[tauri::command]
fn terminal_sessions_sync(app: AppHandle) -> Result<(), String> {
    sync_terminal_sessions_window(&app)
}

#[tauri::command]
fn open_extensions_window(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window not found".to_string())?;
    place_window(&window)?;
    show_main_window_native(&window)?;
    window.set_focus().map_err(|e| e.to_string())?;
    let _ = window.emit("window-shown", json!({ "resetUi": false }));
    let _ = window.emit("app:open-surface", "extensions");
    Ok(())
}

#[tauri::command]
fn toggle_window(window: WebviewWindow) -> Result<(), String> {
    if window.label() == "main" && focus_settings_window_if_visible(window.app_handle())? {
        return Ok(());
    }
    if window.is_visible().map_err(|e| e.to_string())? {
        persist_current_window_position(&window);
        hide_main_window_native(&window)?;
    } else {
        place_window(&window)?;
        show_main_window_native(&window)?;
        window.set_focus().map_err(|e| e.to_string())?;
        let _ = window.emit("window-shown", json!({ "resetUi": false }));
    }
    Ok(())
}

#[tauri::command]
fn hide_window(window: WebviewWindow) -> Result<(), String> {
    persist_current_window_position(&window);
    hide_terminal_sessions(window.app_handle());
    if window.label() == "main" {
        hide_main_window_native(&window)
    } else {
        window.hide().map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn show_window(window: WebviewWindow) -> Result<(), String> {
    if window.label() == "main" && focus_settings_window_if_visible(window.app_handle())? {
        return Ok(());
    }
    place_window(&window)?;
    show_main_window_native(&window)?;
    window.set_focus().map_err(|e| e.to_string())?;
    let _ = window.emit("window-shown", json!({ "resetUi": false }));
    Ok(())
}

#[tauri::command]
fn close_current_window(window: WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        persist_current_window_position(&window);
        hide_main_window_native(&window)
    } else if window.label() == "settings" {
        // All close routes enter the same asynchronous renderer save handshake.
        window.close().map_err(|e| e.to_string())
    } else {
        window.close().map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn finish_settings_close(window: WebviewWindow) -> Result<(), String> {
    if window.label() != "settings" {
        return Err("Not a Settings window".into());
    }
    // destroy bypasses CloseRequested; close would recursively request a save.
    let app = window.app_handle().clone();
    {
        let state = window.state::<WindowBehaviorState>();
        let mut close = state.settings_close.lock().unwrap();
        close.0 += 1;
        close.1 = true;
    }
    window.hide().map_err(|e| e.to_string())?;
    let restored = restore_main_window(&app);
    // Even a launcher restoration failure must not retain a hidden webview.
    window.destroy().map_err(|e| e.to_string())?;
    restored
}

#[tauri::command]
fn ack_settings_close(window: WebviewWindow, generation: u64) -> Result<(), String> {
    if window.label() != "settings" {
        return Err("Not a Settings window".into());
    }
    let state = window.state::<WindowBehaviorState>();
    let mut close = state.settings_close.lock().unwrap();
    if close.0 == generation {
        close.1 = true;
    }
    Ok(())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    quit_app_now(&app);
}

#[tauri::command]
fn start_window_snap_drag(
    window: WebviewWindow,
    app: AppHandle,
    state: State<'_, WindowBehaviorState>,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // A visible user drag owns the window immediately. Cancel delayed
        // mixed-DPI placement writes so they cannot undo the user's move.
        {
            let mut generation = state.window_placement_generation.lock().unwrap();
            *generation = generation.wrapping_add(1);
        }
        *state.main_window_placed.lock().unwrap() = true;
    }

    let initial_position = window.outer_position().map_err(|error| error.to_string())?;
    let initial_window_position = PersistedWindowPosition {
        x: initial_position.x as f64,
        y: initial_position.y as f64,
    };
    #[cfg(target_os = "windows")]
    let initial_cursor_position = windows_cursor_position()?;
    *state.suppress_blur_hide.lock().unwrap() = true;
    *state.snap_drag_active.lock().unwrap() = true;
    *state.snap_locked.lock().unwrap() = SnapLockState::default();
    let generation = {
        let mut generation = state.snap_drag_generation.lock().unwrap();
        *generation = generation.wrapping_add(1);
        *generation
    };
    *state.snap_motion.lock().unwrap() = SnapMotionState {
        generation,
        last_raw_position: Some(initial_window_position),
        last_raw_at: Some(Instant::now()),
        ..SnapMotionState::default()
    };
    #[cfg(target_os = "windows")]
    {
        schedule_windows_snap_drag(
            window.clone(),
            initial_window_position,
            initial_cursor_position,
            generation,
        );
        schedule_windows_snap_overlay(window.clone(), app.clone(), generation);
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(monitor) = monitor_for_window(&window) {
            if let Ok(size) = window.outer_size() {
                if let Err(error) = sync_snap_overlay(
                    &app,
                    &window,
                    &monitor,
                    size.width as f64,
                    size.height as f64,
                    SnapLockState::default(),
                    true,
                ) {
                    log::warn!("failed to show snap overlay: {error}");
                }
            }
        }
        if let Err(error) = window.start_dragging() {
            finish_window_snap_drag(&window, &app, &state);
            return Err(error.to_string());
        }
    }
    schedule_drag_position_persistence(window);
    Ok(())
}

#[tauri::command]
fn end_window_snap_drag(
    window: WebviewWindow,
    app: AppHandle,
    state: State<'_, WindowBehaviorState>,
) {
    finish_window_snap_drag(&window, &app, &state);
}

#[tauri::command]
fn get_window_snap_guides(state: State<'_, WindowBehaviorState>) -> SnapGuidesState {
    *state.snap_guides.lock().unwrap()
}

#[tauri::command]
fn set_suppress_blur_hide(state: State<'_, WindowBehaviorState>, value: bool) {
    *state.suppress_blur_hide.lock().unwrap() = value;
}

#[tauri::command]
fn set_quick_look_window_state(window: WebviewWindow, previewing: bool) -> Result<(), String> {
    window
        .set_always_on_top(!previewing)
        .map_err(|e| e.to_string())?;
    if !previewing {
        window.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn window_set_content_height(
    window: WebviewWindow,
    height: f64,
    zoom_factor: f64,
) -> Result<(), String> {
    // Legacy renderers may still report content height. The user's per-display
    // dimensions, not the current surface or UI zoom, now own native sizing.
    let _ = (window, height, zoom_factor);
    Ok(())
}

#[tauri::command]
fn update_raymes_shortcut(app: AppHandle, shortcut_str: String) -> Result<(), String> {
    let core = app.state::<Arc<native_core::NativeCore>>();
    core.platform
        .register_shortcut(None, &shortcut_str)
        .map(|_| ())
}

#[tauri::command]
async fn call_backend(
    core: State<'_, Arc<native_core::NativeCore>>,
    channel: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    #[cfg(feature = "desktop-bench")]
    if channel == "__desktop_bench_reply" {
        desktop_bench::reply(
            core.ctx.app.as_ref().unwrap(),
            payload[0].as_u64().unwrap(),
            payload[1].clone(),
        );
        return Ok(serde_json::Value::Null);
    }
    let args = payload
        .as_array()
        .ok_or("IPC payload must be an argument array")?;
    let timeout = backend_request_timeout(&channel);
    tokio::time::timeout(timeout, core.request(&channel, args))
        .await
        .map_err(|_| format!("Native request timed out: {channel}"))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();
    let builder = tauri::Builder::default();
    #[cfg(feature = "desktop-bench")]
    let (context, builder) = {
        let mut context = context;
        desktop_bench::validate_isolation();
        context.config_mut().identifier = "com.tezbar.performance".into();
        (context, builder.plugin(desktop_bench::plugin()))
    };
    builder
        // This must stay before every other plugin. A second launch should
        // wake the existing UI and exit before it creates another tray icon,
        // backend supervisor, or global shortcut registration.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            match focus_settings_window_if_visible(app) {
                Ok(true) => return,
                Ok(false) => {}
                Err(error) => {
                    log::debug!("failed to focus existing Settings window: {error}");
                    return;
                }
            }
            if let Err(error) = restore_main_window(app) {
                log::debug!("failed to focus existing Tezbar instance: {error}");
            }
        }))
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .max_file_size(5_000_000)
                .build(),
        )
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let Some(core) = app.try_state::<Arc<native_core::NativeCore>>() else {
                        return;
                    };
                    if let Some(command) = core.platform.command_for_shortcut(shortcut.id()) {
                        let core = core.inner().clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(error) = core.execute_command_shortcut(&command).await {
                                core.ctx.emit("command-error", json!({"message":error}));
                            }
                        });
                    } else if core.platform.is_launcher_shortcut(shortcut.id()) {
                        if let Some(win) = app.get_webview_window("main") {
                            if let Err(error) = toggle_window(win) {
                                log::error!("Launcher shortcut: {error}");
                            }
                        }
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_updater::Builder::new()
                .header(
                    "User-Agent",
                    if cfg!(target_os = "windows") {
                        "Tezbar-App/0.2.0 (Windows; Windows NT)"
                    } else if cfg!(target_os = "macos") {
                        "Tezbar-App/0.2.0 (Macintosh; Intel Mac OS X)"
                    } else {
                        "Tezbar-App/0.2.0 (Linux; X11)"
                    },
                )
                .expect("valid user agent header")
                .build(),
        )
        .plugin(tauri_plugin_process::init())
        .manage(updater::UpdaterState::default())
        .manage(BackendState {
            status: Mutex::new(BackendStatus::default()),
            shutting_down: AtomicBool::new(false),
        })
        .manage(WindowBehaviorState::default())
        .manage(window_layout::WindowLayoutState::default())
        .on_window_event(|window, event| {
            if window.label() == "settings" {
                match event {
                    tauri::WindowEvent::Focused(true) => {
                        if let Err(error) = hide_main_window_for_settings(&window.app_handle()) {
                            log::debug!("failed to hide launcher behind Settings: {error}");
                        }
                    }
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        // Prevent teardown even while a config IPC write is in flight.
                        // The renderer acknowledges only after its save queue drains.
                        api.prevent_close();
                        let generation = {
                            let state = window.state::<WindowBehaviorState>();
                            let mut close = state.settings_close.lock().unwrap();
                            close.0 += 1;
                            close.1 = false;
                            close.0
                        };
                        let _ = window.emit("settings-close-requested", generation);
                        let app = window.app_handle().clone();
                        tauri::async_runtime::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                            let state = app.state::<WindowBehaviorState>();
                            if *state.settings_close.lock().unwrap() != (generation, false) {
                                return;
                            }
                            // No renderer acknowledged this request (failed bootstrap or
                            // content process). Acknowledged drains have no deadline.
                            if let Some(window) = app.get_webview_window("settings") {
                                let _ = finish_settings_close(window);
                            }
                        });
                    }
                    _ => {}
                }
                return;
            }

            if window.label() == TERMINAL_SESSIONS_LABEL {
                if let tauri::WindowEvent::Focused(false) = event {
                    // Delay the check so the main window has time to receive focus
                    // before we decide whether to hide everything. Without this
                    // small grace period, clicking from the sessions sidebar back
                    // into the main terminal view hides both windows because the
                    // sidebar loses focus a few ms before main gains it.
                    let app = window.app_handle().clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(80)).await;
                        if let Some(main_window) = app.get_webview_window("main") {
                            let main_focused = main_window.is_focused().unwrap_or(false);
                            let sidebar_focused = app
                                .get_webview_window(TERMINAL_SESSIONS_LABEL)
                                .map(|w| w.is_focused().unwrap_or(false))
                                .unwrap_or(false);
                            if !main_focused && !sidebar_focused {
                                persist_current_window_position(&main_window);
                                hide_terminal_sessions(&app);
                                let _ = hide_main_window_native(&main_window);
                            }
                        }
                    });
                }
                return;
            }

            if window.label() != "main" {
                return;
            }
            match event {
                tauri::WindowEvent::Resized(size) => {
                    if let Some(main_window) = window.app_handle().get_webview_window("main") {
                        window_layout::sync_viewport(&main_window, *size);
                        window_layout::schedule_capture(main_window);
                        let _ = sync_terminal_sessions_window(window.app_handle());
                    }
                }
                tauri::WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                    if let Some(main_window) = window.app_handle().get_webview_window("main") {
                        window_layout::sync_viewport(&main_window, *new_inner_size);
                        window_layout::schedule_capture(main_window);
                        let _ = sync_terminal_sessions_window(window.app_handle());
                    }
                }
                tauri::WindowEvent::Moved(position) => {
                    #[cfg(target_os = "windows")]
                    let _ = position;
                    if let Some(main_window) = window.app_handle().get_webview_window("main") {
                        let state = main_window.state::<WindowBehaviorState>();
                        if !*state.main_window_placed.lock().unwrap() {
                            // Hidden predeclared windows can receive a startup move
                            // before `place_window` runs. It is not a user move and
                            // must not be persisted on macOS or Windows.
                            return;
                        }
                        if *state.snap_drag_active.lock().unwrap() {
                            // Windows uses the cursor-driven drag task. Its SetPosition
                            // calls also emit Moved events, but feeding those delayed
                            // programmatic events back into the snap motion model would
                            // reset the slow-dwell candidate before it can acquire.
                            #[cfg(not(target_os = "windows"))]
                            {
                                let raw_position = PersistedWindowPosition {
                                    x: position.x as f64,
                                    y: position.y as f64,
                                };
                                match update_window_snap_state(
                                    &main_window,
                                    window.app_handle(),
                                    &state,
                                    raw_position,
                                    false,
                                    false,
                                ) {
                                    Ok(true) => {
                                        let generation =
                                            state.snap_motion.lock().unwrap().generation;
                                        schedule_snap_dwell(main_window.clone(), generation);
                                    }
                                    Ok(false) => {}
                                    Err(error) => {
                                        log::debug!("window snap update failed: {error}");
                                    }
                                }
                            }
                        }
                        persist_current_window_position(&main_window);
                        window_layout::schedule_capture(main_window.clone());
                        let _ = sync_terminal_sessions_window(window.app_handle());
                    }
                }
                tauri::WindowEvent::Focused(false) => {
                    if window
                        .app_handle()
                        .get_webview_window("settings")
                        .map(|settings| settings.is_visible().unwrap_or(false))
                        .unwrap_or(false)
                    {
                        let _ = hide_main_window_for_settings(&window.app_handle());
                        return;
                    }
                    if let Some(sidebar) = window
                        .app_handle()
                        .get_webview_window(TERMINAL_SESSIONS_LABEL)
                    {
                        if sidebar.is_focused().unwrap_or(false) {
                            return;
                        }
                    }
                    // Windows can emit Focused(false) between show() and the
                    // matching activation/focus event, especially when the
                    // launcher is opened from the tray. Hiding synchronously
                    // turns a successful tray activation into a flash-and-close
                    // window. Re-check after the activation settles.
                    let app = window.app_handle().clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(120)).await;
                        let Some(main_window) = app.get_webview_window("main") else {
                            return;
                        };
                        let state = main_window.state::<WindowBehaviorState>();
                        if !*state.main_window_placed.lock().unwrap() {
                            return;
                        }
                        if *state.suppress_blur_hide.lock().unwrap() {
                            return;
                        }
                        if main_window.is_focused().unwrap_or(false) {
                            return;
                        }
                        if app
                            .get_webview_window(TERMINAL_SESSIONS_LABEL)
                            .map(|sidebar| sidebar.is_focused().unwrap_or(false))
                            .unwrap_or(false)
                        {
                            return;
                        }
                        persist_current_window_position(&main_window);
                        hide_terminal_sessions(&app);
                        let _ = hide_main_window_native(&main_window);
                    });
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            call_backend,
            get_backend_status,
            restart_backend,
            open_settings_window_cmd,
            hide_launcher_for_settings,
            terminal_sessions_show,
            terminal_sessions_hide,
            terminal_sessions_sync,
            open_extensions_window,
            toggle_window,
            hide_window,
            show_window,
            close_current_window,
            finish_settings_close,
            ack_settings_close,
            quit_app,
            start_window_snap_drag,
            end_window_snap_drag,
            get_window_snap_guides,
            set_suppress_blur_hide,
            set_quick_look_window_state,
            window_set_content_height,
            window_layout::get_launcher_layout,
            window_layout::set_launcher_layout,
            window_layout::start_launcher_resize,
            update_raymes_shortcut,
            native_input::move_mouse,
            native_input::click,
            native_input::double_click,
            native_input::press_key,
            native_input::type_text,
            native_input::scroll,
            native_input::screenshot,
            native_input::is_physical_key_down,
            native_terminal::native_terminal_create,
            native_terminal::native_terminal_attach,
            native_terminal::native_terminal_cwd,
            native_terminal::native_terminal_detach,
            native_terminal::native_terminal_write,
            native_terminal::native_terminal_resize,
            native_terminal::native_terminal_kill,
            native_terminal::native_terminal_delete_history,
            native_terminal::native_terminal_prune_history,
            updater::get_update_status,
            updater::get_update_channel,
            updater::set_update_channel,
            updater::check_for_updates,
            updater::download_and_install_update,
            updater::restart_app,
            updater::open_release_page,
            qr_download::save_qr_png_data_url
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let app_local_data = std::env::var_os("TEZBAR_DATA_DIR")
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
                .unwrap_or(handle.path().app_local_data_dir()?);
            let config_path = openray_config_path();
            if let Some(path) = &config_path {
                let _ = PROFILE_CONFIG_PATH.set(path.clone());
            }
            #[cfg(not(feature = "desktop-bench"))]
            let home_dir = handle.path().home_dir()?;
            #[cfg(feature = "desktop-bench")]
            let home_dir = PathBuf::from(std::env::var_os("TEZBAR_BENCH_HOME").unwrap());
            let executable = if cfg!(debug_assertions) {
                std::env::current_exe()?
            } else {
                PathBuf::new()
            };
            let paths = native_core::common::CorePaths {
                data_dir: app_local_data,
                config_dir: config_path
                    .as_ref()
                    .and_then(|p| p.parent())
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| home_dir.join(".openray")),
                resource_dir: resource_directory::resolve(
                    cfg!(debug_assertions),
                    tauri::is_dev(),
                    &executable,
                    &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."),
                    || handle.path().resource_dir(),
                )?,
                temp_dir: handle.path().temp_dir()?,
                home_dir,
            };
            let context = native_core::common::CoreContext::new(
                paths,
                Some(Arc::new(tauri_host::TauriHost::new(handle.clone()))),
            )
            .with_dev_mode(tauri::is_dev())
            .with_legacy_data_dir(handle.path().app_data_dir().ok());
            let _ = PROFILE_STORE.set(context.config.clone());
            let core = native_core::NativeCore::new(context).map_err(std::io::Error::other)?;
            app.manage(core.clone());
            let core_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                match core.start().await {
                    Ok(()) => set_backend_status(&core_handle, "ready", "Rust core ready"),
                    Err(error) => {
                        log::error!("Native service initialization: {error}");
                        set_backend_status(&core_handle, "failed", &error);
                    }
                }
            });

            // System Tray Menu Setup
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

            let app_name = handle.package_info().name.clone();
            let show_item = MenuItem::with_id(
                &handle,
                "show",
                format!("Show {app_name}"),
                true,
                None::<&str>,
            )?;
            let settings_item =
                MenuItem::with_id(&handle, "settings", "Settings", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(
                &handle,
                "quit",
                format!("Quit {app_name}"),
                true,
                None::<&str>,
            )?;

            let menu = Menu::with_items(&handle, &[&show_item, &settings_item, &quit_item])?;

            let handle_tray = handle.clone();
            let tray_icon = {
                let resource_path = handle
                    .path()
                    .resource_dir()
                    .expect("failed to resolve resource dir")
                    .join("trayIconTemplate@2x.png");
                let try_load_png =
                    |path: &std::path::Path| -> Option<tauri::image::Image<'static>> {
                        let img = image::open(path).ok()?.into_rgba8();
                        let (w, h) = img.dimensions();
                        Some(tauri::image::Image::new_owned(img.into_raw(), w, h))
                    };
                if let Some(icon) = try_load_png(&resource_path) {
                    icon
                } else {
                    // Fallback: try relative path (dev mode)
                    let dev_path = std::env::current_dir()
                        .unwrap_or_default()
                        .join("../resources/trayIconTemplate@2x.png");
                    try_load_png(&dev_path)
                        .unwrap_or_else(|| handle.default_window_icon().unwrap().clone())
                }
            };
            let _tray = TrayIconBuilder::new()
                .icon(tray_icon)
                .icon_as_template(true)
                .menu(&menu)
                // A left click is the launcher toggle. Keep the context menu
                // on the right click so Windows does not run both actions for
                // the same tray interaction.
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = show_window(win);
                        }
                    }
                    "settings" => {
                        let handle = app.clone();
                        // WebView2 creation must happen off the UI event loop.
                        tauri::async_runtime::spawn(async move {
                            if let Err(error) = open_settings_window(handle) {
                                log::error!("failed to open Settings: {error}");
                            }
                        });
                    }
                    "quit" => {
                        quit_app_now(app);
                    }
                    _ => {}
                })
                .on_tray_icon_event(move |tray, event| {
                    // Windows emits both button-down and button-up events for
                    // every mouse button. Only toggle on a completed left
                    // click; handling right/middle clicks here steals the
                    // tray context-menu interaction and can flash the launcher
                    // before hiding it again.
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = toggle_window(win);
                        }
                    }
                })
                .build(&handle_tray)?;

            let handle_for_updater = handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(3)).await;
                log::info!("updater: startup background check triggered");
                let _ = updater::check_for_updates(handle_for_updater).await;
            });

            Ok(())
        })
        .build(context)
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = &event {
                if !app
                    .state::<BackendState>()
                    .shutting_down
                    .load(Ordering::Acquire)
                {
                    api.prevent_exit();
                    quit_app_now(app);
                }
            }
            if let tauri::RunEvent::Ready = event {
                // Place the initially hidden launcher before showing it on
                // every launch, including the new process after an update.
                if let Err(error) = restore_main_window(app) {
                    log::error!("failed to show Tezbar on startup: {error}");
                }
            }
        });
}

#[cfg(test)]
mod backend_timeout_tests {
    use super::*;

    #[test]
    fn position_updates_coalesce_into_latest_snapshot() {
        let mut pending = PendingWindowPositions::default();
        let first = pending.remember("monitor".into(), PersistedWindowPosition { x: 1.0, y: 2.0 });
        let last = pending.remember("monitor".into(), PersistedWindowPosition { x: 3.0, y: 4.0 });
        let writes = [first, last]
            .into_iter()
            .filter(|generation| pending.should_flush(Some(*generation)))
            .count();
        assert_eq!(writes, 1);
        assert_ne!(first, pending.generation);
        assert_eq!(last, pending.generation);
        assert_eq!(pending.positions.len(), 1);
        assert_eq!(pending.positions["monitor"].x, 3.0);
        assert!(pending.dirty);
    }

    #[test]
    fn locate_bun_detects_cached_or_fails_cleanly() {
        let temp =
            std::env::temp_dir().join(format!("tezbar-test-bun-locate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        let fake_bun_dir = temp.join("bun");
        let _ = std::fs::create_dir_all(&fake_bun_dir);
        let fake_bin = fake_bun_dir.join(bun_executable_name());
        let _ = std::fs::write(&fake_bin, b"fake");
        assert_eq!(locate_bun(&temp).unwrap(), fake_bin);
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn ordinary_backend_requests_keep_the_short_failure_bound() {
        assert_eq!(
            backend_request_timeout("extension:list"),
            DEFAULT_BACKEND_REQUEST_TIMEOUT
        );
    }

    #[test]
    fn extension_installs_outlive_their_bounded_install_stages() {
        assert_eq!(
            backend_request_timeout("extension:install"),
            EXTENSION_INSTALL_REQUEST_TIMEOUT
        );
        assert_eq!(
            backend_request_timeout("extensions:reinstall"),
            EXTENSION_INSTALL_REQUEST_TIMEOUT
        );
    }
}
