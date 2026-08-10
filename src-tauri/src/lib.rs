// src-tauri/src/lib.rs
mod native_input;
mod native_terminal;
#[cfg(target_os = "macos")]
mod timer_notifications;
mod updater;

#[cfg(target_os = "macos")]
use core_foundation::base::{kCFAllocatorDefault, CFType, TCFType};
#[cfg(target_os = "macos")]
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
#[cfg(target_os = "macos")]
use core_foundation::number::CFNumber;
#[cfg(target_os = "macos")]
use core_foundation::string::{CFString, CFStringRef};
#[cfg(target_os = "macos")]
use core_foundation::uuid::{CFUUIDCreateString, CFUUIDRef, CFUUID};
#[cfg(target_os = "macos")]
use core_graphics::display::{CGDirectDisplayID, CGDisplay};
#[cfg(target_os = "macos")]
use core_graphics::window::{
    copy_window_info, kCGWindowAlpha, kCGWindowBounds, kCGWindowLayer,
    kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly, kCGWindowOwnerName,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
#[cfg(target_os = "macos")]
use tauri::LogicalPosition;
#[cfg(not(target_os = "macos"))]
use tauri::PhysicalSize;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, Monitor, PhysicalPosition, State, WebviewWindow,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tokio::sync::oneshot;

const WINDOW_WIDTH: f64 = 760.0;
const WINDOW_MIN_HEIGHT: f64 = 120.0;
const WINDOW_MAX_HEIGHT: f64 = 700.0;
const TERMINAL_SESSIONS_LABEL: &str = "terminal-sessions";
const TERMINAL_SESSIONS_WIDTH: f64 = 300.0;
// The sessions sidebar sits to the LEFT of the main window. A positive overhang
// means the sidebar extends that many logical pixels beyond the left edge of main.
const TERMINAL_SESSIONS_LEFT_OVERHANG: f64 = 266.0;
const TERMINAL_SESSIONS_TOP_OFFSET: f64 = 96.0;
const TERMINAL_SESSIONS_BOTTOM_OFFSET: f64 = 86.0;
const TERMINAL_SESSIONS_MIN_HEIGHT: f64 = 260.0;
const SNAP_OVERLAY_LABEL: &str = "window-snap-overlay";
const SNAP_THRESHOLD: f64 = 32.0;
const SNAP_RELEASE_THRESHOLD: f64 = 32.0;
const SNAP_SLOW_SPEED_THRESHOLD: f64 = 420.0;
const SNAP_DWELL_DURATION: Duration = Duration::from_millis(130);
const WINDOW_POSITIONS_BY_MONITOR_KEY: &str = "windowPositionsByMonitorV2";
const DEFAULT_BACKEND_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const EXTENSION_INSTALL_REQUEST_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const EXTENSION_RUNTIME_REQUEST_TIMEOUT: Duration = Duration::from_secs(2 * 60);

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGDisplayCreateUUIDFromDisplayID(display: CGDirectDisplayID) -> CFUUIDRef;
}

struct BackendState {
    writer: Arc<Mutex<Option<TcpStream>>>,
    pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<serde_json::Value>>>>,
    request_counter: Arc<Mutex<u64>>,
}

#[derive(Clone)]
struct BackendLaunchConfig {
    executable: PathBuf,
    script_path: PathBuf,
    env: Vec<(String, String)>,
}

fn backend_request_timeout(channel: &str) -> Duration {
    match channel {
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
    backend_hidden_windows: Mutex<Vec<String>>,
    // Windows creates the hidden launcher at an OS-chosen position and emits
    // Moved before Tezbar has selected its real placement. Ignore that event
    // so it cannot overwrite the user's saved location with (typically) 130,130.
    #[cfg(target_os = "windows")]
    main_window_placed: Mutex<bool>,
    // SetPosition is asynchronous on Windows. A generation keeps an older
    // mixed-DPI finalizer from racing a newer launcher invocation.
    #[cfg(target_os = "windows")]
    window_placement_generation: Mutex<u64>,
    // The renderer can report a content height immediately after show. Hold
    // the newest value until mixed-DPI placement has finished so neither
    // operation can overwrite the other.
    #[cfg(target_os = "windows")]
    pending_content_height: Mutex<Option<f64>>,
    snap_drag_active: Mutex<bool>,
    snap_locked: Mutex<SnapLockState>,
    snap_guides: Mutex<SnapGuidesState>,
    snap_motion: Mutex<SnapMotionState>,
    snap_drag_generation: Mutex<u64>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
struct PersistedWindowPosition {
    x: f64,
    y: f64,
}

#[derive(Clone, Copy, Debug)]
struct MonitorGeometry {
    bounds: (f64, f64, f64, f64),
    work_area: (f64, f64, f64, f64),
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
struct SnapLockState {
    x: bool,
    y: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapGuidesState {
    visible: bool,
    snap_x: bool,
    snap_y: bool,
    centered: bool,
    target_rect: Option<SnapTargetRect>,
}

#[derive(Debug)]
struct SnapMotionState {
    generation: u64,
    last_raw_position: Option<PersistedWindowPosition>,
    last_raw_at: Option<Instant>,
    slow_x_since: Option<Instant>,
    slow_y_since: Option<Instant>,
    programmatic_position: Option<(PersistedWindowPosition, Instant)>,
    settle_timer_active: bool,
    snap_anchor: PersistedWindowPosition,
    snap_escape: PersistedWindowPosition,
}

impl Default for SnapMotionState {
    fn default() -> Self {
        Self {
            generation: 0,
            last_raw_position: None,
            last_raw_at: None,
            slow_x_since: None,
            slow_y_since: None,
            programmatic_position: None,
            settle_timer_active: false,
            snap_anchor: PersistedWindowPosition { x: 0.0, y: 0.0 },
            snap_escape: PersistedWindowPosition { x: 0.0, y: 0.0 },
        }
    }
}

struct SnapAcquireContext {
    position: PersistedWindowPosition,
    window_width: f64,
    window_height: f64,
    monitor: MonitorGeometry,
    locked: SnapLockState,
    now: Instant,
    from_timer: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapTargetRect {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
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

/// Close the sidecar connection before leaving the Tauri event loop. The
/// backend treats EOF on this socket as its shutdown signal, which gives it a
/// chance to stop its own workers instead of leaving a Bun process behind.
fn close_backend_connection(app: &AppHandle) {
    let Some(state) = app.try_state::<BackendState>() else {
        return;
    };
    state.writer.lock().unwrap().take();
    let mut pending = state.pending_requests.lock().unwrap();
    for (_, sender) in pending.drain() {
        let _ = sender.send(json!({ "error": "Tezbar is shutting down" }));
    }
}

fn quit_app_now(app: &AppHandle) {
    // Release the global shortcut and sidecar IPC before asking Tauri to exit.
    // This makes tray/menu quits behave the same as the Settings quit action.
    let _ = app.global_shortcut().unregister_all();
    close_backend_connection(app);
    app.exit(0);
}

fn handle_backend_message(
    app: &AppHandle,
    pending_requests: &Arc<Mutex<HashMap<u64, oneshot::Sender<serde_json::Value>>>>,
    line: &str,
) {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else {
        log::error!("backend sidecar emitted invalid JSON on stdout: {}", line);
        return;
    };

    log::debug!("received backend sidecar message");
    let Some(msg_type) = val.get("type").and_then(|value| value.as_str()) else {
        return;
    };
    match msg_type {
        "reply" => {
            let Some(id) = val.get("id").and_then(|value| value.as_u64()) else {
                return;
            };
            let mut pending = pending_requests.lock().unwrap();
            if let Some(tx) = pending.remove(&id) {
                let reply = if let Some(error) = val.get("error") {
                    json!({ "error": error })
                } else {
                    json!({ "result": val.get("result").unwrap_or(&serde_json::Value::Null) })
                };
                let _ = tx.send(reply);
            }
        }
        "event" => {
            if let Some(channel) = val.get("channel").and_then(|value| value.as_str()) {
                let payload = val.get("payload").unwrap_or(&serde_json::Value::Null);
                let _ = app.emit(channel, payload);
            }
        }
        "dialog" => println!("[Tauri Dialog] Dialog options: {:?}", val.get("options")),
        "app_quit" => quit_app_now(app),
        "window_suppress_blur" => {
            if let Some(value) = val.get("value").and_then(|value| value.as_bool()) {
                let state = app.state::<WindowBehaviorState>();
                *state.suppress_blur_hide.lock().unwrap() = value;
            }
        }
        "app_visibility" => {
            if let Some(value) = val.get("visible").and_then(|value| value.as_bool()) {
                set_backend_app_visibility(app, value);
            }
        }
        #[cfg(target_os = "macos")]
        "timer_notification" => {
            let timer_file = val.get("timerFile").and_then(|value| value.as_str());
            let name = val.get("name").and_then(|value| value.as_str());
            if let (Some(timer_file), Some(name)) = (timer_file, name) {
                timer_notifications::deliver(name, timer_file);
            }
        }
        _ => {}
    }
}

fn run_backend_generation(
    app: &AppHandle,
    config: &BackendLaunchConfig,
    writer: &Arc<Mutex<Option<TcpStream>>>,
    pending_requests: &Arc<Mutex<HashMap<u64, oneshot::Sender<serde_json::Value>>>>,
) -> Result<Duration, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
    let ipc_port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;

    let mut command = Command::new(&config.executable);
    command
        .arg(&config.script_path)
        .envs(config.env.iter().map(|(key, value)| (key, value)))
        .env("BACKEND_IPC_PORT", ipc_port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        // Bun is a console executable. Without this flag Windows may surface
        // it through the user's default terminal every time the supervised
        // backend starts or restarts.
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }

    log::info!(
        "launching backend sidecar: {}",
        config.script_path.display()
    );
    let mut child = command
        .spawn()
        .map_err(|error| format!("Failed to spawn background runner process: {error}"))?;
    let stdout = child.stdout.take().ok_or("Failed to open backend stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to open backend stderr")?;

    // Start draining stderr before waiting for the IPC handshake. A startup
    // failure used to be hidden until after the five-second timeout, leaving
    // only the misleading "still recovering" message in the UI.
    let stderr_thread = std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            log::info!("backend sidecar: {}", line);
        }
        log::info!("backend stderr reader stopped");
    });

    let connect_deadline = Instant::now() + Duration::from_secs(5);
    let backend_stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= connect_deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stderr_thread.join();
                    return Err("Backend runner did not connect to its IPC socket".to_string());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stderr_thread.join();
                return Err(format!("Failed to accept backend IPC connection: {error}"));
            }
        }
    };
    *writer.lock().unwrap() = Some(backend_stream);
    let connected_at = Instant::now();
    log::info!("backend sidecar IPC connected on localhost");

    for line_result in BufReader::new(stdout).lines() {
        match line_result {
            Ok(line) if !line.trim().is_empty() => {
                handle_backend_message(app, pending_requests, &line)
            }
            Ok(_) => {}
            Err(error) => {
                log::error!("failed reading backend stdout: {}", error);
                break;
            }
        }
    }

    *writer.lock().unwrap() = None;
    let mut pending = pending_requests.lock().unwrap();
    for (_, sender) in pending.drain() {
        let _ = sender.send(json!({ "error": "Backend runner stopped; restarting" }));
    }
    drop(pending);

    if child.try_wait().ok().flatten().is_none() {
        let _ = child.kill();
    }
    let status = child.wait().ok();
    let _ = stderr_thread.join();
    log::error!("backend sidecar stopped with status {:?}", status);
    Ok(connected_at.elapsed())
}

fn supervise_backend(
    app: AppHandle,
    config: BackendLaunchConfig,
    writer: Arc<Mutex<Option<TcpStream>>>,
    pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<serde_json::Value>>>>,
) {
    let mut consecutive_failures = 0_u32;
    loop {
        match run_backend_generation(&app, &config, &writer, &pending_requests) {
            Ok(uptime) if uptime >= Duration::from_secs(30) => consecutive_failures = 0,
            Ok(_) => consecutive_failures = consecutive_failures.saturating_add(1),
            Err(error) => {
                consecutive_failures = consecutive_failures.saturating_add(1);
                log::error!("backend sidecar launch failed: {}", error);
            }
        }

        // A crash-loop used to keep respawning the whole Bun sidecar at full
        // speed (max 8s). With infinite retries and a 8s cap this is still
        // resilient, but soaks the machine if the bundle is broken.
        // Cap the backoff much higher and give up after 4 failures so we
        // stop the worst-case churn and show the user a stable error.
        if consecutive_failures >= 4 {
            log::error!(
                "backend sidecar kept failing ({} consecutive); sleeping 60s before retry",
                consecutive_failures
            );
            std::thread::sleep(Duration::from_secs(60));
            // Try one more time; if it still fails, keep the 60s sleep loop
            // without the boot-time churn. The user can restart the app.
            continue;
        }
        let exponent = consecutive_failures.min(5);
        let delay_ms = (250_u64 * (1_u64 << exponent)).min(8_000);
        log::info!("restarting backend sidecar in {}ms", delay_ms);
        std::thread::sleep(Duration::from_millis(delay_ms));
    }
}

fn openray_config_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|home| PathBuf::from(home).join(".openray").join("config.json"))
}

fn bun_executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "bun.exe"
    } else {
        "bun"
    }
}

fn bun_in_user_profile() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .map(|home| home.join(".bun").join("bin").join(bun_executable_name()))
}

#[cfg(target_os = "windows")]
fn install_windows_bun(app_local_data: &std::path::Path) -> Result<PathBuf, String> {
    let architecture = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        other => {
            return Err(format!(
                "Bun does not provide a supported Windows build for {other}"
            ))
        }
    };
    let destination_dir = app_local_data.join("bun");
    let destination = destination_dir.join("bun.exe");
    fs::create_dir_all(&destination_dir)
        .map_err(|error| format!("failed to create Bun runtime directory: {error}"))?;
    let url = format!(
        "https://github.com/oven-sh/bun/releases/download/bun-v1.2.5/bun-windows-{architecture}.zip"
    );
    let script = r#"$ErrorActionPreference='Stop'; $zip=Join-Path $env:TEMP ('tezbar-bun-'+[guid]::NewGuid().ToString()+'.zip'); $extract=Join-Path $env:TEMP ('tezbar-bun-'+[guid]::NewGuid().ToString()); try { Invoke-WebRequest -UseBasicParsing -Uri $env:TEZBAR_BUN_URL -OutFile $zip; Expand-Archive -LiteralPath $zip -DestinationPath $extract -Force; $bun=Get-ChildItem -LiteralPath $extract -Filter bun.exe -Recurse | Select-Object -First 1; if (-not $bun) { throw 'bun.exe was not present in the downloaded archive' }; Copy-Item -LiteralPath $bun.FullName -Destination $env:TEZBAR_BUN_DEST -Force } finally { Remove-Item -LiteralPath $zip -Force -ErrorAction SilentlyContinue; Remove-Item -LiteralPath $extract -Recurse -Force -ErrorAction SilentlyContinue }"#;
    let status = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .env("TEZBAR_BUN_URL", url)
        .env("TEZBAR_BUN_DEST", &destination)
        .status()
        .map_err(|error| format!("failed to start the Windows Bun installer: {error}"))?;
    if status.success() && destination.is_file() {
        Ok(destination)
    } else {
        Err("failed to download the Bun runtime required by Tezbar".to_string())
    }
}

fn locate_bun(app_local_data: &std::path::Path) -> Result<PathBuf, String> {
    let cached = app_local_data.join("bun").join(bun_executable_name());
    if cached.is_file() {
        return Ok(cached);
    }
    if let Some(user_bun) = bun_in_user_profile().filter(|path| path.is_file()) {
        return Ok(user_bun);
    }
    if Command::new(bun_executable_name())
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
    {
        return Ok(PathBuf::from(bun_executable_name()));
    }
    #[cfg(target_os = "windows")]
    {
        return install_windows_bun(app_local_data);
    }
    #[cfg(not(target_os = "windows"))]
    Err("Bun is required to run the Tauri backend. Install Bun or place it in the app data bun directory.".to_string())
}

fn copy_backend_directory(source: &std::path::Path, destination: &std::path::Path) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|error| format!("failed to create backend directory {}: {error}", destination.display()))?;
    for entry in fs::read_dir(source)
        .map_err(|error| format!("failed to read backend resources {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| format!("failed to enumerate backend resources: {error}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to inspect backend resource {}: {error}", source_path.display()))?;
        if file_type.is_dir() {
            copy_backend_directory(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path).map_err(|error| {
                format!(
                    "failed to stage backend resource {} -> {}: {error}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn stage_backend_bundle(
    source_dir: &std::path::Path,
    app_local_data: &std::path::Path,
) -> Result<PathBuf, String> {
    let destination_dir = app_local_data.join("backend");
    copy_backend_directory(source_dir, &destination_dir)?;

    // The bundle keeps esbuild external because it includes a native binary.
    // Stage the two packaged module trees beside the relocated JS so Bun's
    // normal CommonJS resolution still works from the writable directory.
    let source_node_modules = source_dir
        .parent()
        .map(|parent| parent.join("node_modules"))
        .ok_or("could not resolve packaged node_modules directory")?;
    for module in ["esbuild", "@esbuild"] {
        let source_module = source_node_modules.join(module);
        if source_module.is_dir() {
            copy_backend_directory(&source_module, &destination_dir.join("node_modules").join(module))?;
        }
    }

    for required in ["main.js", "knowledge-worker.js"] {
        if !destination_dir.join(required).is_file() {
            return Err(format!(
                "packaged backend resource is missing after staging: {}",
                destination_dir.join(required).display()
            ));
        }
    }
    Ok(destination_dir.join("main.js"))
}

fn read_openray_config() -> serde_json::Value {
    let Some(path) = openray_config_path() else {
        return json!({});
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return json!({});
    };
    serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .filter(|value| value.is_object())
        .unwrap_or_else(|| json!({}))
}

fn position_from_config_value(
    value: Option<&serde_json::Value>,
) -> Option<PersistedWindowPosition> {
    serde_json::from_value(value?.clone()).ok()
}

fn monitor_positions_from_config(
    config: &serde_json::Value,
) -> BTreeMap<String, PersistedWindowPosition> {
    config
        .get(WINDOW_POSITIONS_BY_MONITOR_KEY)
        .and_then(serde_json::Value::as_object)
        .map(|positions| {
            positions
                .iter()
                .filter_map(|(monitor_id, value)| {
                    position_from_config_value(Some(value))
                        .map(|position| (monitor_id.clone(), position))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn window_position_config_needs_reset(config: &serde_json::Value) -> bool {
    [
        "tauriWindowPosition",
        "tauriWindowPositionsByDisplay",
        "windowPositionsByMonitor",
        "windowPosition",
        "windowPositionsByDisplay",
        "tezbarWindowPlacementInitialized",
    ]
    .iter()
    .any(|key| config.get(key).is_some())
}

#[derive(Clone, Copy, Debug, Default)]
struct StoredWindowPosition {
    position: Option<PersistedWindowPosition>,
    needs_reset: bool,
}

fn stored_window_position_for_monitor(monitor_id: &str) -> StoredWindowPosition {
    let config = read_openray_config();
    StoredWindowPosition {
        position: monitor_positions_from_config(&config)
            .get(monitor_id)
            .copied(),
        needs_reset: window_position_config_needs_reset(&config),
    }
}

fn update_window_position_config(
    config: &mut serde_json::Value,
    monitor_id: &str,
    position: PersistedWindowPosition,
) {
    let mut positions = monitor_positions_from_config(config);
    positions.insert(monitor_id.to_string(), position);

    let Some(config_object) = config.as_object_mut() else {
        return;
    };
    for obsolete_key in [
        "tauriWindowPosition",
        "tauriWindowPositionsByDisplay",
        "windowPositionsByMonitor",
        "windowPosition",
        "windowPositionsByDisplay",
        "tezbarWindowPlacementInitialized",
    ] {
        config_object.remove(obsolete_key);
    }
    config_object.insert(
        WINDOW_POSITIONS_BY_MONITOR_KEY.to_string(),
        json!(positions),
    );
}

fn set_persisted_window_position_for_monitor(
    window: &WebviewWindow,
    monitor: &Monitor,
    position: PersistedWindowPosition,
) {
    let Some(path) = openray_config_path() else {
        return;
    };
    let Some(monitor_id) = monitor_id(window, monitor) else {
        return;
    };
    let mut config = read_openray_config();
    update_window_position_config(&mut config, &monitor_id, position);

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(serialized) = serde_json::to_string_pretty(&config) {
        let _ = fs::write(path, format!("{serialized}\n"));
    }
}

#[cfg(any(not(target_os = "macos"), test))]
fn monitor_id_from_name(name: Option<&str>) -> Option<String> {
    name.map(str::to_string)
}

#[cfg(target_os = "macos")]
fn monitors_match(left: &Monitor, right: &Monitor) -> bool {
    left.name() == right.name()
        && left.position() == right.position()
        && left.size() == right.size()
        && left.scale_factor() == right.scale_factor()
}

#[cfg(target_os = "macos")]
fn macos_display_uuid(display_id: CGDirectDisplayID) -> Option<String> {
    let uuid_ref = unsafe { CGDisplayCreateUUIDFromDisplayID(display_id) };
    if uuid_ref.is_null() {
        return None;
    }
    let uuid = unsafe { CFUUID::wrap_under_create_rule(uuid_ref) };
    let string_ref = unsafe {
        CFUUIDCreateString(kCFAllocatorDefault, uuid.as_concrete_TypeRef())
    };
    if string_ref.is_null() {
        return None;
    }
    let uuid_string = unsafe { CFString::wrap_under_create_rule(string_ref) };
    Some(format!("macos:{}", uuid_string))
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
            return macos_display_uuid(display_id);
        }
    }
    None
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

fn position_is_in_bounds(position: PersistedWindowPosition, bounds: (f64, f64, f64, f64)) -> bool {
    let (x, y, width, height) = bounds;
    position.x >= x && position.x < x + width && position.y >= y && position.y < y + height
}

fn clamp_position_to_work_area(
    position: PersistedWindowPosition,
    work_area: (f64, f64, f64, f64),
    window_width: f64,
    window_height: f64,
) -> PersistedWindowPosition {
    let (work_x, work_y, work_width, work_height) = work_area;
    let max_x = work_x + (work_width - window_width).max(0.0);
    let max_y = work_y + (work_height - window_height).max(0.0);
    PersistedWindowPosition {
        x: position.x.round().clamp(work_x, max_x),
        y: position.y.round().clamp(work_y, max_y),
    }
}

fn plan_window_position(
    active_monitor: MonitorGeometry,
    window_width: f64,
    window_height: f64,
    saved_for_active_monitor: Option<PersistedWindowPosition>,
) -> PersistedWindowPosition {
    if let Some(position) = saved_for_active_monitor {
        return clamp_position_to_work_area(
            position,
            active_monitor.work_area,
            window_width,
            window_height,
        );
    }

    let (work_x, work_y, work_width, work_height) = active_monitor.work_area;
    let position = PersistedWindowPosition {
        x: work_x + ((work_width - window_width) / 2.0).max(0.0),
        y: work_y + ((work_height - window_height) / 2.0).max(0.0),
    };
    debug_assert!(position_is_in_bounds(position, active_monitor.bounds));
    position
}

fn snap_axis(
    window_start: f64,
    window_extent: f64,
    target: f64,
    distance: f64,
    locked: bool,
    acquire: bool,
) -> (f64, bool, bool) {
    let still_locked = locked && distance <= SNAP_RELEASE_THRESHOLD;
    let should_lock = !locked && acquire && distance <= SNAP_THRESHOLD;
    if !still_locked && !should_lock {
        return (window_start, false, false);
    }
    (target - window_extent / 2.0, true, true)
}

#[cfg(any(target_os = "windows", test))]
fn window_position_for_cursor_drag(
    initial_window_position: PersistedWindowPosition,
    initial_cursor_position: PersistedWindowPosition,
    cursor_position: PersistedWindowPosition,
) -> PersistedWindowPosition {
    PersistedWindowPosition {
        x: initial_window_position.x + cursor_position.x - initial_cursor_position.x,
        y: initial_window_position.y + cursor_position.y - initial_cursor_position.y,
    }
}

fn snap_window_position(
    position: PersistedWindowPosition,
    window_width: f64,
    window_height: f64,
    monitor: MonitorGeometry,
    locked: SnapLockState,
    acquire: SnapLockState,
) -> (PersistedWindowPosition, SnapLockState, bool) {
    let (work_x, work_y, work_width, work_height) = monitor.work_area;
    let window_center_x = position.x + window_width / 2.0;
    let window_center_y = position.y + window_height / 2.0;
    let monitor_center_x = work_x + work_width / 2.0;
    let monitor_center_y = work_y + work_height / 2.0;
    let dx = (window_center_x - monitor_center_x).abs();
    let dy = (window_center_y - monitor_center_y).abs();
    let (x, x_locked, x_active) = snap_axis(
        position.x,
        window_width,
        monitor_center_x,
        dx,
        locked.x,
        acquire.x,
    );
    let (y, y_locked, y_active) = snap_axis(
        position.y,
        window_height,
        monitor_center_y,
        dy,
        locked.y,
        acquire.y,
    );

    (
        PersistedWindowPosition { x, y },
        SnapLockState {
            x: x_locked,
            y: y_locked,
        },
        x_active || y_active,
    )
}

fn snap_target_rect(
    window_width: f64,
    window_height: f64,
    monitor: MonitorGeometry,
) -> SnapTargetRect {
    let (work_x, work_y, work_width, work_height) = monitor.work_area;
    let target_x = work_x + (work_width - window_width) / 2.0;
    let target_y = work_y + (work_height - window_height) / 2.0;
    SnapTargetRect {
        left: ((target_x - work_x) / work_width * 100.0).clamp(0.0, 100.0),
        top: ((target_y - work_y) / work_height * 100.0).clamp(0.0, 100.0),
        right: ((target_x + window_width - work_x) / work_width * 100.0).clamp(0.0, 100.0),
        bottom: ((target_y + window_height - work_y) / work_height * 100.0).clamp(0.0, 100.0),
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

fn update_snap_candidate(
    since: &mut Option<Instant>,
    eligible: bool,
    now: Instant,
) -> (bool, bool) {
    if !eligible {
        *since = None;
        return (false, false);
    }
    let started = since.is_none();
    let started_at = *since.get_or_insert(now);
    (
        now.duration_since(started_at) >= SNAP_DWELL_DURATION,
        started,
    )
}

fn snap_acquire_state(
    motion: &mut SnapMotionState,
    context: SnapAcquireContext,
) -> (SnapLockState, bool, bool) {
    let SnapAcquireContext {
        position,
        window_width,
        window_height,
        monitor,
        locked,
        now,
        from_timer,
    } = context;
    let speed = if from_timer {
        0.0
    } else {
        match (motion.last_raw_position, motion.last_raw_at) {
            (Some(previous), Some(previous_at)) => {
                let elapsed = now.duration_since(previous_at).as_secs_f64();
                if elapsed > 0.0 {
                    ((position.x - previous.x).hypot(position.y - previous.y)) / elapsed
                } else {
                    f64::INFINITY
                }
            }
            _ => f64::INFINITY,
        }
    };
    if !from_timer {
        motion.last_raw_position = Some(position);
        motion.last_raw_at = Some(now);
    }

    let (work_x, work_y, work_width, work_height) = monitor.work_area;
    let dx = (position.x + window_width / 2.0 - (work_x + work_width / 2.0)).abs();
    let dy = (position.y + window_height / 2.0 - (work_y + work_height / 2.0)).abs();
    let stationary = from_timer
        && motion
            .last_raw_at
            .map(|last| now.duration_since(last) >= SNAP_DWELL_DURATION)
            .unwrap_or(false);
    let moving_slowly = speed <= SNAP_SLOW_SPEED_THRESHOLD || stationary;
    let near_x = !locked.x && dx <= SNAP_THRESHOLD;
    let near_y = !locked.y && dy <= SNAP_THRESHOLD;
    let should_schedule_timer = if from_timer {
        motion.settle_timer_active = false;
        false
    } else if (near_x || near_y) && !motion.settle_timer_active {
        motion.settle_timer_active = true;
        true
    } else {
        false
    };
    if stationary {
        if near_x && motion.slow_x_since.is_none() {
            motion.slow_x_since = Some(now - SNAP_DWELL_DURATION);
        }
        if near_y && motion.slow_y_since.is_none() {
            motion.slow_y_since = Some(now - SNAP_DWELL_DURATION);
        }
    }
    let x_eligible = !locked.x
        && near_x
        && moving_slowly
        && (!from_timer || motion.slow_x_since.is_some() || stationary);
    let y_eligible = !locked.y
        && near_y
        && moving_slowly
        && (!from_timer || motion.slow_y_since.is_some() || stationary);
    let (acquire_x, started_x) = update_snap_candidate(&mut motion.slow_x_since, x_eligible, now);
    let (acquire_y, started_y) = update_snap_candidate(&mut motion.slow_y_since, y_eligible, now);

    (
        SnapLockState {
            x: acquire_x,
            y: acquire_y,
        },
        started_x || started_y,
        should_schedule_timer,
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

#[cfg(target_os = "macos")]
fn cf_type_for_static_key(key: CFStringRef) -> CFType {
    unsafe { CFString::wrap_under_get_rule(key).as_CFType() }
}

#[cfg(target_os = "macos")]
fn cf_type_for_string_key(key: &str) -> CFType {
    CFString::new(key).as_CFType()
}

#[cfg(target_os = "macos")]
fn dictionary_value_for_key(
    dictionary: &CFDictionary<CFType, CFType>,
    key: &CFType,
) -> Option<CFType> {
    dictionary.find(key).map(|value| (*value).clone())
}

#[cfg(target_os = "macos")]
fn dictionary_number_for_key(
    dictionary: &CFDictionary<CFType, CFType>,
    key: &CFType,
) -> Option<f64> {
    dictionary_value_for_key(dictionary, key)?
        .downcast::<CFNumber>()?
        .to_f64()
}

#[cfg(target_os = "macos")]
fn dictionary_string_for_key(
    dictionary: &CFDictionary<CFType, CFType>,
    key: &CFType,
) -> Option<String> {
    dictionary_value_for_key(dictionary, key)?
        .downcast::<CFString>()
        .map(|value| value.to_string())
}

#[cfg(target_os = "macos")]
fn dictionary_for_key(
    dictionary: &CFDictionary<CFType, CFType>,
    key: &CFType,
) -> Option<CFDictionary<CFType, CFType>> {
    let value = dictionary_value_for_key(dictionary, key)?;
    Some(unsafe {
        CFDictionary::<CFType, CFType>::wrap_under_get_rule(value.as_CFTypeRef() as CFDictionaryRef)
    })
}

#[cfg(target_os = "macos")]
fn frontmost_window_monitor(window: &WebviewWindow) -> Option<Monitor> {
    let window_infos = copy_window_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        0,
    )?;
    let layer_key = cf_type_for_static_key(unsafe { kCGWindowLayer });
    let alpha_key = cf_type_for_static_key(unsafe { kCGWindowAlpha });
    let owner_key = cf_type_for_static_key(unsafe { kCGWindowOwnerName });
    let bounds_key = cf_type_for_static_key(unsafe { kCGWindowBounds });
    let x_key = cf_type_for_string_key("X");
    let y_key = cf_type_for_string_key("Y");
    let width_key = cf_type_for_string_key("Width");
    let height_key = cf_type_for_string_key("Height");

    for item in window_infos.iter() {
        let dictionary = unsafe {
            CFDictionary::<CFType, CFType>::wrap_under_get_rule(*item as CFDictionaryRef)
        };
        let Some(layer) = dictionary_number_for_key(&dictionary, &layer_key) else {
            continue;
        };
        if layer.round() as i32 != 0 {
            continue;
        }
        if dictionary_number_for_key(&dictionary, &alpha_key).unwrap_or(0.0) <= 0.0 {
            continue;
        }
        let owner = dictionary_string_for_key(&dictionary, &owner_key).unwrap_or_default();
        if owner.is_empty()
            || owner == "Tezbar"
            || owner == "Raymes"
            || owner == "Dock"
            || owner == "Window Server"
        {
            continue;
        }
        let Some(bounds) = dictionary_for_key(&dictionary, &bounds_key) else {
            continue;
        };
        let Some(x) = dictionary_number_for_key(&bounds, &x_key) else {
            continue;
        };
        let Some(y) = dictionary_number_for_key(&bounds, &y_key) else {
            continue;
        };
        let Some(width) = dictionary_number_for_key(&bounds, &width_key) else {
            continue;
        };
        let Some(height) = dictionary_number_for_key(&bounds, &height_key) else {
            continue;
        };
        if width < 40.0 || height < 40.0 {
            continue;
        }
        if let Ok(Some(monitor)) = window.monitor_from_point(x + width / 2.0, y + height / 2.0) {
            return Some(monitor);
        }
    }
    None
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn frontmost_window_monitor(_window: &WebviewWindow) -> Option<Monitor> {
    None
}

#[cfg(target_os = "windows")]
fn frontmost_window_monitor(window: &WebviewWindow) -> Option<Monitor> {
    use std::mem::size_of;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let foreground_window = unsafe { GetForegroundWindow() };
    if foreground_window.is_null() {
        return None;
    }

    let monitor_handle =
        unsafe { MonitorFromWindow(foreground_window, MONITOR_DEFAULTTONEAREST) };
    if monitor_handle.is_null() {
        return None;
    }

    let mut monitor_info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        rcMonitor: unsafe { std::mem::zeroed() },
        rcWork: unsafe { std::mem::zeroed() },
        dwFlags: 0,
    };
    if unsafe { GetMonitorInfoW(monitor_handle, &mut monitor_info) } == 0 {
        return None;
    }

    // Tauri's monitor_from_point uses the same physical desktop coordinate
    // space as the Win32 monitor rectangles. Use the foreground monitor's
    // center to map the native handle back to Tauri's Monitor value.
    let center_x =
        (i64::from(monitor_info.rcMonitor.left) + i64::from(monitor_info.rcMonitor.right)) as f64
            / 2.0;
    let center_y =
        (i64::from(monitor_info.rcMonitor.top) + i64::from(monitor_info.rcMonitor.bottom)) as f64
            / 2.0;
    window.monitor_from_point(center_x, center_y).ok().flatten()
}

#[cfg(target_os = "macos")]
fn cursor_monitor(window: &WebviewWindow) -> Option<Monitor> {
    use core_graphics::event::CGEvent;
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    // Tauri scales the macOS cursor position by the primary monitor's scale
    // factor before passing it to CoreGraphics' logical-coordinate monitor
    // lookup. That misses mixed-DPI secondary monitors. Read the cursor in
    // CoreGraphics' native global coordinate space instead.
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).ok()?;
    let position = CGEvent::new(source).ok()?.location();
    window
        .monitor_from_point(position.x, position.y)
        .ok()
        .flatten()
}

#[cfg(target_os = "windows")]
fn cursor_monitor(window: &WebviewWindow) -> Option<Monitor> {
    // Keep cursor and monitor lookup in Win32's physical virtual-desktop
    // coordinate space, including negative coordinates and mixed DPI.
    let position = windows_cursor_position().ok()?;
    window
        .monitor_from_point(position.x, position.y)
        .ok()
        .flatten()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn cursor_monitor(window: &WebviewWindow) -> Option<Monitor> {
    let position = window.cursor_position().ok()?;
    window
        .monitor_from_point(position.x, position.y)
        .ok()
        .flatten()
}

fn select_active_monitor<T>(
    cursor: Option<T>,
    frontmost: Option<T>,
    current: impl FnOnce() -> Option<T>,
    primary: impl FnOnce() -> Option<T>,
) -> Option<T> {
    cursor.or(frontmost).or_else(current).or_else(primary)
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

fn window_size_for_target_monitor(
    current_size: (f64, f64),
    current_scale_factor: f64,
    target_scale_factor: f64,
) -> (f64, f64) {
    let current_scale_factor = valid_scale_factor(current_scale_factor);
    let target_scale_factor = valid_scale_factor(target_scale_factor);
    let logical_height = (current_size.1 / current_scale_factor)
        .clamp(WINDOW_MIN_HEIGHT, WINDOW_MAX_HEIGHT);
    (
        WINDOW_WIDTH * target_scale_factor,
        logical_height * target_scale_factor,
    )
}

fn valid_scale_factor(scale_factor: f64) -> f64 {
    if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    }
}

#[cfg(any(target_os = "macos", test))]
fn logical_position_for_monitor(
    position: PersistedWindowPosition,
    target_scale_factor: f64,
) -> PersistedWindowPosition {
    let target_scale_factor = valid_scale_factor(target_scale_factor);
    PersistedWindowPosition {
        x: position.x / target_scale_factor,
        y: position.y / target_scale_factor,
    }
}

fn window_size_for_monitor(window: &WebviewWindow, monitor: &Monitor) -> (f64, f64) {
    let target_scale_factor = monitor.scale_factor();
    match window.outer_size() {
        Ok(current_size) => window_size_for_target_monitor(
            (current_size.width as f64, current_size.height as f64),
            window.scale_factor().unwrap_or(target_scale_factor),
            target_scale_factor,
        ),
        Err(_) => (
            WINDOW_WIDTH * target_scale_factor,
            WINDOW_MAX_HEIGHT * target_scale_factor,
        ),
    }
}

#[cfg(any(target_os = "macos", test))]
fn logical_window_center(
    position: PersistedWindowPosition,
    size: (f64, f64),
    scale_factor: f64,
) -> PersistedWindowPosition {
    let scale_factor = valid_scale_factor(scale_factor);
    PersistedWindowPosition {
        x: (position.x + size.0 / 2.0) / scale_factor,
        y: (position.y + size.1 / 2.0) / scale_factor,
    }
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
    let center = logical_window_center(
        position,
        size,
        window.scale_factor().unwrap_or(1.0),
    );
    #[cfg(not(target_os = "macos"))]
    let center = PersistedWindowPosition {
        x: position.x + size.0 / 2.0,
        y: position.y + size.1 / 2.0,
    };
    window
        .monitor_from_point(center.x, center.y)
        .ok()
        .flatten()
}

fn persist_window_position_at(window: &WebviewWindow, position: PersistedWindowPosition) {
    let Ok(size) = window.outer_size() else {
        return;
    };
    let Some(monitor) = monitor_for_window_position(
        window,
        position,
        (size.width as f64, size.height as f64),
    ) else {
        return;
    };
    set_persisted_window_position_for_monitor(window, &monitor, position);
}

fn persist_current_window_position(window: &WebviewWindow) {
    {
        #[cfg(target_os = "windows")]
        if window.label() == "main"
            && !*window
                .state::<WindowBehaviorState>()
                .main_window_placed
                .lock()
                .unwrap()
        {
            return;
        }
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
        // Coordinate with a content-height command that may have arrived
        // while placement was in progress. Both paths lock `main_window_placed`
        // before `pending_content_height`, so no update can fall between the
        // final pending read and releasing the placement guard.
        let pending_content_height = {
            let state = window.state::<WindowBehaviorState>();
            let mut placed = state.main_window_placed.lock().unwrap();
            let pending = state.pending_content_height.lock().unwrap().take();
            *placed = true;
            pending
        };
        if let Some(height) = pending_content_height {
            match apply_window_content_height(&window, height) {
                Ok(()) if persist_position => {
                    // Position updates are asynchronous on Windows. Persist the
                    // post-resize coordinate after that final move can land.
                    tokio::time::sleep(Duration::from_millis(16)).await;
                    if placement_is_current() {
                        persist_current_window_position(&window);
                    }
                }
                Ok(()) => {}
                Err(error) => {
                    log::warn!("failed to apply deferred Windows content height: {error}");
                }
            }
        } else if target_reached && placement_settled && persist_position {
            persist_current_window_position(&window);
        }
    });
}

fn place_window(window: &WebviewWindow) -> Result<(), String> {
    let monitor = active_monitor(window)?;
    let (window_width, window_height) = window_size_for_monitor(window, &monitor);
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
        {
            let mut placed = state.main_window_placed.lock().unwrap();
            let mut pending = state.pending_content_height.lock().unwrap();
            *placed = false;
            *pending = None;
        }
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
        set_window_position_on_monitor(window, position, &monitor)?;
        if persist_position {
            // Old placement formats used the wrong coordinate space. Reset
            // once and keep only monitor ID + coordinates.
            set_persisted_window_position_for_monitor(window, &monitor, position);
        }
        Ok(())
    }
}

#[cfg(test)]
mod window_placement_tests {
    use super::*;

    #[test]
    fn monitor_storage_uses_only_the_monitor_id() {
        assert_eq!(
            monitor_id_from_name(Some(r"\\.\DISPLAY2")).as_deref(),
            Some(r"\\.\DISPLAY2")
        );
    }

    #[test]
    fn cursor_monitor_is_the_active_monitor() {
        let selected = select_active_monitor(
            Some("cursor"),
            Some("frontmost"),
            || panic!("current monitor lookup must be short-circuited"),
            || panic!("primary monitor lookup must be short-circuited"),
        );

        assert_eq!(selected, Some("cursor"));
    }

    #[test]
    fn frontmost_monitor_is_used_when_cursor_detection_is_unavailable() {
        let selected = select_active_monitor(
            None,
            Some("frontmost"),
            || panic!("current monitor lookup must be short-circuited"),
            || panic!("primary monitor lookup must be short-circuited"),
        );

        assert_eq!(selected, Some("frontmost"));
    }

    #[test]
    fn persistence_keeps_one_coordinate_pair_per_monitor_id() {
        let mut config = json!({
            "unrelatedSetting": true,
            "windowPositionsByMonitorV2": {
                "Monitor B": { "x": 2100.0, "y": 150.0 }
            }
        });

        update_window_position_config(
            &mut config,
            "Monitor A",
            PersistedWindowPosition { x: 30.0, y: 40.0 },
        );

        assert_eq!(
            config,
            json!({
                "unrelatedSetting": true,
                "windowPositionsByMonitorV2": {
                    "Monitor A": { "x": 30.0, "y": 40.0 },
                    "Monitor B": { "x": 2100.0, "y": 150.0 }
                }
            })
        );
    }

    #[test]
    fn broken_legacy_coordinates_are_reset_instead_of_restored() {
        let mut config = json!({
            "tauriWindowPosition": { "x": 1.0, "y": 2.0 },
            "windowPositionsByMonitor": {
                "macos:broken": { "x": 52.0, "y": 568.0 }
            },
            "windowPosition": { "x": 0.5, "y": 1.0 },
            "windowPositionsByDisplay": {
                "Monitor A:1920:1080:2": { "x": 15.0, "y": 25.0 }
            },
            "tauriWindowPositionsByDisplay": {
                "Monitor A:0:0:1920:1080:2": { "x": 30.0, "y": 50.0 }
            }
        });

        assert!(window_position_config_needs_reset(&config));
        update_window_position_config(
            &mut config,
            "Monitor B",
            PersistedWindowPosition { x: 200.0, y: 300.0 },
        );

        assert_eq!(
            config,
            json!({
                "windowPositionsByMonitorV2": {
                    "Monitor B": { "x": 200.0, "y": 300.0 }
                }
            })
        );
    }

    #[test]
    fn missing_position_centers_on_the_active_monitor() {
        let external = MonitorGeometry {
            bounds: (1920.0, -180.0, 2560.0, 1440.0),
            work_area: (1920.0, -156.0, 2560.0, 1416.0),
        };

        let position = plan_window_position(external, 760.0, 640.0, None);

        assert!(
            position_is_in_bounds(position, external.bounds),
            "launcher must be placed on the active external monitor, got {position:?}"
        );
        assert_eq!(
            position,
            PersistedWindowPosition {
                x: 2820.0,
                y: 232.0
            }
        );
    }

    #[test]
    fn mixed_dpi_monitor_uses_its_own_scale_and_center() {
        let external = MonitorGeometry {
            bounds: (1800.0, 0.0, 1920.0, 1080.0),
            work_area: (1800.0, 0.0, 1920.0, 1080.0),
        };
        let (window_width, window_height) =
            window_size_for_target_monitor((1520.0, 1280.0), 2.0, 1.0);

        assert_eq!((window_width, window_height), (760.0, 640.0));
        let physical_position = plan_window_position(
            external,
            window_width,
            window_height,
            None,
        );
        assert_eq!(
            logical_position_for_monitor(physical_position, 1.0),
            PersistedWindowPosition { x: 2380.0, y: 220.0 }
        );
        assert!(position_is_in_bounds(physical_position, external.bounds));

        // This is the old Tauri PhysicalPosition behavior: Tao divides by the
        // hidden window's current scale (2), putting x=1190 back on the Retina
        // monitor instead of the active external monitor.
        let old_effective_position = PersistedWindowPosition {
            x: physical_position.x / 2.0,
            y: physical_position.y / 2.0,
        };
        assert!(!position_is_in_bounds(old_effective_position, external.bounds));
    }

    #[test]
    fn windows_mixed_dpi_centers_from_200_percent_to_100_percent() {
        let target = MonitorGeometry {
            bounds: (0.0, 0.0, 1920.0, 1080.0),
            work_area: (0.0, 0.0, 1920.0, 1040.0),
        };
        let (window_width, window_height) =
            window_size_for_target_monitor((1520.0, 1280.0), 2.0, 1.0);

        assert_eq!((window_width, window_height), (760.0, 640.0));
        assert_eq!(
            plan_window_position(target, window_width, window_height, None),
            PersistedWindowPosition { x: 580.0, y: 200.0 }
        );
    }

    #[test]
    fn windows_mixed_dpi_centers_on_negative_200_percent_monitor() {
        let target = MonitorGeometry {
            bounds: (-3840.0, 0.0, 3840.0, 2160.0),
            work_area: (-3840.0, 0.0, 3840.0, 2080.0),
        };
        let (window_width, window_height) =
            window_size_for_target_monitor((760.0, 640.0), 1.0, 2.0);

        assert_eq!((window_width, window_height), (1520.0, 1280.0));
        assert_eq!(
            plan_window_position(target, window_width, window_height, None),
            PersistedWindowPosition { x: -2680.0, y: 400.0 }
        );
    }

    #[test]
    fn mixed_dpi_monitor_scales_up_from_external_to_retina() {
        let retina = MonitorGeometry {
            bounds: (0.0, 0.0, 3600.0, 2338.0),
            work_area: (0.0, 0.0, 3600.0, 2338.0),
        };
        let (window_width, window_height) =
            window_size_for_target_monitor((760.0, 640.0), 1.0, 2.0);

        assert_eq!((window_width, window_height), (1520.0, 1280.0));
        let physical_position = plan_window_position(
            retina,
            window_width,
            window_height,
            None,
        );
        assert_eq!(
            physical_position,
            PersistedWindowPosition { x: 1040.0, y: 529.0 }
        );
        assert_eq!(
            logical_position_for_monitor(physical_position, 2.0),
            PersistedWindowPosition { x: 520.0, y: 264.5 }
        );
    }

    #[test]
    fn active_monitor_restores_and_clamps_its_own_saved_position() {
        let external = MonitorGeometry {
            bounds: (-2560.0, -180.0, 2560.0, 1440.0),
            work_area: (-2560.0, -156.0, 2560.0, 1416.0),
        };

        let position = plan_window_position(
            external,
            760.0,
            640.0,
            Some(PersistedWindowPosition {
                x: -3000.0,
                y: -500.0,
            }),
        );

        assert_eq!(
            position,
            PersistedWindowPosition {
                x: -2560.0,
                y: -156.0,
            }
        );
    }

    #[test]
    fn cursor_drag_preserves_the_initial_grab_offset() {
        let position = window_position_for_cursor_drag(
            PersistedWindowPosition { x: 180.0, y: 120.0 },
            PersistedWindowPosition { x: 184.0, y: 124.0 },
            PersistedWindowPosition { x: 598.0, y: 214.0 },
        );

        assert_eq!(position, PersistedWindowPosition { x: 594.0, y: 210.0 });
    }

    #[test]
    fn window_snaps_to_monitor_center_when_both_axes_are_near() {
        let monitor = MonitorGeometry {
            bounds: (0.0, 0.0, 1920.0, 1080.0),
            work_area: (0.0, 24.0, 1920.0, 1056.0),
        };
        let (position, locked, active) = snap_window_position(
            PersistedWindowPosition { x: 572.0, y: 208.0 },
            760.0,
            640.0,
            monitor,
            SnapLockState::default(),
            SnapLockState { x: true, y: true },
        );

        assert_eq!(position, PersistedWindowPosition { x: 580.0, y: 232.0 });
        assert!(locked.x);
        assert!(locked.y);
        assert!(active);
    }

    #[test]
    fn window_snaps_each_axis_independently() {
        let monitor = MonitorGeometry {
            bounds: (0.0, 0.0, 1920.0, 1080.0),
            work_area: (0.0, 24.0, 1920.0, 1056.0),
        };
        let (position, locked, active) = snap_window_position(
            PersistedWindowPosition { x: 572.0, y: 320.0 },
            760.0,
            640.0,
            monitor,
            SnapLockState::default(),
            SnapLockState { x: true, y: true },
        );

        assert_eq!(position.x, 580.0);
        assert_eq!(position.y, 320.0);
        assert!(locked.x);
        assert!(!locked.y);
        assert!(active);
    }

    #[test]
    fn window_snap_waits_for_a_slow_dwell_before_acquiring() {
        let monitor = MonitorGeometry {
            bounds: (0.0, 0.0, 1920.0, 1080.0),
            work_area: (0.0, 24.0, 1920.0, 1056.0),
        };
        let position = PersistedWindowPosition { x: 580.0, y: 320.0 };
        let now = Instant::now();
        let mut motion = SnapMotionState {
            last_raw_position: Some(position),
            last_raw_at: Some(now),
            ..SnapMotionState::default()
        };

        let (acquire, started, _) = snap_acquire_state(
            &mut motion,
            SnapAcquireContext {
                position,
                window_width: 760.0,
                window_height: 640.0,
                monitor,
                locked: SnapLockState::default(),
                now: now + Duration::from_millis(1),
                from_timer: false,
            },
        );
        assert!(started);
        assert!(!acquire.x);

        let (acquire, _, _) = snap_acquire_state(
            &mut motion,
            SnapAcquireContext {
                position,
                window_width: 760.0,
                window_height: 640.0,
                monitor,
                locked: SnapLockState::default(),
                now: now + SNAP_DWELL_DURATION + Duration::from_millis(1),
                from_timer: true,
            },
        );
        assert!(acquire.x);
    }

    #[test]
    fn window_does_not_snap_just_outside_the_soft_capture_radius() {
        let monitor = MonitorGeometry {
            bounds: (0.0, 0.0, 1920.0, 1080.0),
            work_area: (0.0, 24.0, 1920.0, 1056.0),
        };
        let (position, locked, active) = snap_window_position(
            PersistedWindowPosition { x: 616.0, y: 268.0 },
            760.0,
            640.0,
            monitor,
            SnapLockState::default(),
            SnapLockState { x: true, y: true },
        );

        assert_eq!(position, PersistedWindowPosition { x: 616.0, y: 268.0 });
        assert!(!locked.x);
        assert!(!locked.y);
        assert!(!active);
    }

    #[test]
    fn snap_guides_describe_a_fixed_centered_target_frame() {
        let monitor = MonitorGeometry {
            bounds: (-1920.0, 0.0, 1920.0, 1080.0),
            work_area: (-1920.0, 24.0, 1920.0, 1056.0),
        };
        let target = snap_target_rect(760.0, 640.0, monitor);

        assert!((target.left - 30.208_333).abs() < 0.000_01);
        assert!((target.right - 69.791_667).abs() < 0.000_01);
        assert!((target.top - 19.696_970).abs() < 0.000_01);
        assert!((target.bottom - 80.303_030).abs() < 0.000_01);
    }

    #[test]
    fn window_snap_uses_hysteresis_before_releasing() {
        let monitor = MonitorGeometry {
            bounds: (0.0, 0.0, 1920.0, 1080.0),
            work_area: (0.0, 24.0, 1920.0, 1056.0),
        };

        let (_, locked, active) = snap_window_position(
            PersistedWindowPosition { x: 600.0, y: 250.0 },
            760.0,
            640.0,
            monitor,
            SnapLockState { x: true, y: true },
            SnapLockState::default(),
        );
        assert!(locked.x);
        assert!(locked.y);
        assert!(active);

        let (_, locked, active) = snap_window_position(
            PersistedWindowPosition { x: 700.0, y: 340.0 },
            760.0,
            640.0,
            monitor,
            SnapLockState { x: true, y: true },
            SnapLockState::default(),
        );
        assert!(!locked.x);
        assert!(!locked.y);
        assert!(!active);
    }
}

fn schedule_drag_position_persistence(window: WebviewWindow) {
    tauri::async_runtime::spawn(async move {
        for _ in 0..12 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if !window.is_visible().unwrap_or(false) {
                break;
            }
            persist_current_window_position(&window);
        }
    });
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

fn finish_window_snap_drag(
    window: &WebviewWindow,
    app: &AppHandle,
    state: &WindowBehaviorState,
) {
    persist_current_window_position(window);
    *state.snap_drag_active.lock().unwrap() = false;
    *state.snap_locked.lock().unwrap() = SnapLockState::default();
    *state.snap_motion.lock().unwrap() = SnapMotionState::default();
    hide_snap_overlay(app, Some(window));
    *state.suppress_blur_hide.lock().unwrap() = false;
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
            match update_window_snap_state(
                &window,
                &app,
                &state,
                raw_position,
                false,
                true,
            ) {
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
    main_window.hide().map_err(|error| error.to_string())
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
    settings_window
        .show()
        .map_err(|error| error.to_string())?;
    settings_window
        .set_focus()
        .map_err(|error| error.to_string())?;
    Ok(true)
}

fn open_settings_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("settings") {
        // The Settings WebView is predeclared so WebView2 has already started
        // its renderer. Do only the minimum native work here: blocking while a
        // secondary renderer initializes can make Windows mark the launcher as
        // unresponsive.
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
    .title("Tezbar Settings")
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
    main_window.show().map_err(|e| e.to_string())?;
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
fn open_settings_window_cmd(app: AppHandle) -> Result<(), String> {
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
    window.show().map_err(|e| e.to_string())?;
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
        window.hide().map_err(|e| e.to_string())?;
    } else {
        place_window(&window)?;
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        let _ = window.emit("window-shown", json!({ "resetUi": false }));
    }
    Ok(())
}

#[tauri::command]
fn hide_window(window: WebviewWindow) -> Result<(), String> {
    persist_current_window_position(&window);
    hide_terminal_sessions(window.app_handle());
    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
fn show_window(window: WebviewWindow) -> Result<(), String> {
    if window.label() == "main" && focus_settings_window_if_visible(window.app_handle())? {
        return Ok(());
    }
    place_window(&window)?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    let _ = window.emit("window-shown", json!({ "resetUi": false }));
    Ok(())
}

#[tauri::command]
fn close_current_window(window: WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        persist_current_window_position(&window);
        window.hide().map_err(|e| e.to_string())
    } else if window.label() == "settings" {
        // Settings is declared in tauri.conf.json, so hide it instead of
        // destroying its WebView. Reusing the initialized WebView avoids the
        // blank-page race that occurs when WebView2 is created on demand.
        window.hide().map_err(|e| e.to_string())?;
        restore_main_window(window.app_handle())
    } else {
        window.close().map_err(|e| e.to_string())
    }
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
        // A visible user drag owns the window immediately. Cancel any delayed
        // placement writes and apply the latest deferred content height first.
        {
            let mut generation = state.window_placement_generation.lock().unwrap();
            *generation = generation.wrapping_add(1);
        }
        let pending_content_height = {
            let mut placed = state.main_window_placed.lock().unwrap();
            let pending = state.pending_content_height.lock().unwrap().take();
            *placed = true;
            pending
        };
        if let Some(height) = pending_content_height {
            apply_window_content_height(&window, height)?;
        }
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

fn apply_window_content_height(
    window: &WebviewWindow,
    clamped_height: f64,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let previous_center = if window.is_visible().unwrap_or(false) {
        window
            .outer_position()
            .ok()
            .zip(window.outer_size().ok())
            .map(|(position, size)| {
                (
                    position.x as f64 + size.width as f64 / 2.0,
                    position.y as f64 + size.height as f64 / 2.0,
                )
            })
    } else {
        None
    };

    window
        .set_size(LogicalSize::new(WINDOW_WIDTH, clamped_height))
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    if let (Some((center_x, center_y)), Ok(size)) = (previous_center, window.outer_size()) {
        // Resizing a borderless WebView keeps its top-left corner fixed. Move
        // it back by half the size delta so a content-height update cannot
        // pull the launcher away from the monitor center.
        set_window_position(
            &window,
            PersistedWindowPosition {
                x: center_x - size.width as f64 / 2.0,
                y: center_y - size.height as f64 / 2.0,
            },
        )?;
    }

    let _ = sync_terminal_sessions_window(window.app_handle());
    Ok(())
}

#[tauri::command]
fn window_set_content_height(
    window: WebviewWindow,
    height: f64,
    zoom_factor: f64,
) -> Result<(), String> {
    // `height` is already in physical pixels (css_height * zoom_factor computed
    // on the JS side). Convert back to logical pixels by dividing once.
    let zoom = if zoom_factor > 0.0 { zoom_factor } else { 1.0 };
    let logical_height = height / zoom;
    let clamped_height = logical_height.clamp(WINDOW_MIN_HEIGHT, WINDOW_MAX_HEIGHT);

    #[cfg(target_os = "windows")]
    if window.label() == "main" {
        let state = window.state::<WindowBehaviorState>();
        let placed = state.main_window_placed.lock().unwrap();
        if !*placed {
            *state.pending_content_height.lock().unwrap() = Some(clamped_height);
            return Ok(());
        }
    }

    apply_window_content_height(&window, clamped_height)
}

#[tauri::command]
fn update_raymes_shortcut(app: AppHandle, shortcut_str: String) -> Result<(), String> {
    let clean_shortcut = shortcut_str
        .replace("Option", "Alt")
        .replace("CommandOrControl", "Super")
        .replace("CmdOrCtrl", "Super")
        .replace("Cmd", "Super")
        .replace("Ctrl", "Control");
    // Alt+Space is reserved by Windows for the native window menu. It was
    // previously the cross-platform default, so make old installations
    // usable instead of silently leaving them without a launcher shortcut.
    let clean_shortcut = if cfg!(target_os = "windows")
        && clean_shortcut.eq_ignore_ascii_case("Alt+Space")
    {
        "Control+Space".to_string()
    } else {
        clean_shortcut
    };
    let shortcut = Shortcut::from_str(&clean_shortcut)
        .map_err(|e| format!("Invalid shortcut format: {:?}", e))?;
    let _ = app.global_shortcut().unregister_all();
    app.global_shortcut()
        .register(shortcut)
        .map_err(|e| format!("Failed to register shortcut: {:?}", e))?;
    Ok(())
}

#[tauri::command]
async fn call_backend(
    state: State<'_, BackendState>,
    channel: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let started_at = Instant::now();
    let request_timeout = backend_request_timeout(&channel);
    let id = {
        let mut counter = state.request_counter.lock().unwrap();
        *counter += 1;
        *counter
    };

    let (tx, rx) = oneshot::channel();
    let mut response_sender = Some(tx);

    let msg = json!({
      "type": "invoke",
      "id": id,
      "channel": channel,
      "payload": payload
    })
    .to_string();

    // Startup and crash recovery briefly leave the writer empty. Give the
    // supervisor a chance to reconnect instead of immediately surfacing a
    // permanent-looking error to every feature that uses the backend.
    let writer_deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let write_result = {
            let mut backend_writer = state.writer.lock().unwrap();
            backend_writer.as_mut().map(|writer| {
                // Register only after a live generation is available. This
                // prevents the previous generation's shutdown drain from
                // consuming requests that are waiting for the replacement.
                state
                    .pending_requests
                    .lock()
                    .unwrap()
                    .insert(id, response_sender.take().unwrap());
                writeln!(writer, "{}", msg).and_then(|_| writer.flush())
            })
        };

        match write_result {
            Some(Ok(())) => {
                log::debug!("wrote backend request: id={} channel={}", id, channel);
                break;
            }
            Some(Err(_)) => {
                state.pending_requests.lock().unwrap().remove(&id);
                *state.writer.lock().unwrap() = None;
                return Err(
                    "Failed to write to backend runner process; recovery is in progress"
                        .to_string(),
                );
            }
            None if Instant::now() < writer_deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            None => {
                return Err("Backend runner is still recovering".to_string());
            }
        }
    }

    match tokio::time::timeout(request_timeout, rx).await {
        Ok(Ok(res)) => {
            if let Some(err) = res.get("error") {
                let message = err.as_str().unwrap_or("Unknown backend error").to_string();
                log::error!(
                    "backend request failed: id={} channel={} elapsed_ms={} error={}",
                    id,
                    channel,
                    started_at.elapsed().as_millis(),
                    message
                );
                Err(message)
            } else {
                log::debug!(
                    "backend request completed: id={} channel={} elapsed_ms={}",
                    id,
                    channel,
                    started_at.elapsed().as_millis()
                );
                Ok(res
                    .get("result")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null))
            }
        }
        Ok(Err(_)) => {
            let message = "Backend runner stopped before replying".to_string();
            log::error!("{}: id={} channel={}", message, id, channel);
            Err(message)
        }
        Err(_) => {
            state.pending_requests.lock().unwrap().remove(&id);
            let message = format!(
                "Backend request for {channel} timed out after {} seconds",
                request_timeout.as_secs()
            );
            log::error!("{}: id={} channel={}", message, id, channel);
            Err(message)
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let pending_requests = Arc::new(Mutex::new(
        HashMap::<u64, oneshot::Sender<serde_json::Value>>::new(),
    ));
    let request_counter = Arc::new(Mutex::new(0));
    let backend_writer = Arc::new(Mutex::new(None::<TcpStream>));

    let pending_requests_app = pending_requests.clone();
    let backend_writer_app = backend_writer.clone();
    tauri::Builder::default()
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
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = toggle_window(win);
                        }
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(updater::UpdaterState::default())
        .manage(BackendState {
            writer: backend_writer,
            pending_requests,
            request_counter,
        })
        .manage(WindowBehaviorState::default())
        .manage(native_terminal::NativeTerminalState::default())
        .on_window_event(|window, event| {
            if window.label() == "settings" {
                match event {
                    tauri::WindowEvent::Focused(true) => {
                        if let Err(error) =
                            hide_main_window_for_settings(&window.app_handle())
                        {
                            log::debug!("failed to hide launcher behind Settings: {error}");
                        }
                    }
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        // Treat the title-bar X like the in-app Back action. Keep
                        // the predeclared WebView alive and return focus to the
                        // launcher instead of leaving the whole app hidden.
                        api.prevent_close();
                        let _ = window.hide();
                        if let Err(error) = restore_main_window(&window.app_handle()) {
                            log::debug!(
                                "failed to restore launcher after closing settings: {error}"
                            );
                        }
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
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(80));
                        if let Some(main_window) = app.get_webview_window("main") {
                            let main_focused = main_window.is_focused().unwrap_or(false);
                            let sidebar_focused = app
                                .get_webview_window(TERMINAL_SESSIONS_LABEL)
                                .map(|w| w.is_focused().unwrap_or(false))
                                .unwrap_or(false);
                            if !main_focused && !sidebar_focused {
                                persist_current_window_position(&main_window);
                                hide_terminal_sessions(&app);
                                let _ = main_window.hide();
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
                tauri::WindowEvent::Moved(position) => {
                    #[cfg(target_os = "windows")]
                    let _ = position;
                    if let Some(main_window) = window.app_handle().get_webview_window("main") {
                        let state = main_window.state::<WindowBehaviorState>();
                        #[cfg(target_os = "windows")]
                        if !*state.main_window_placed.lock().unwrap() {
                            // The hidden predeclared window receives a move event at
                            // Windows' default location before `place_window` runs.
                            // It is not a user move and must not be persisted.
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
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(120));
                        let Some(main_window) = app.get_webview_window("main") else {
                            return;
                        };
                        let state = main_window.state::<WindowBehaviorState>();
                        #[cfg(target_os = "windows")]
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
                        let _ = main_window.hide();
                    });
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            call_backend,
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
            quit_app,
            start_window_snap_drag,
            end_window_snap_drag,
            get_window_snap_guides,
            set_suppress_blur_hide,
            set_quick_look_window_state,
            window_set_content_height,
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
            updater::check_for_updates,
            updater::download_and_install_update,
            updater::restart_app,
            updater::open_release_page
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            #[cfg(target_os = "macos")]
            timer_notifications::setup(&handle);
            let app_local_data = handle.path().app_local_data_dir().unwrap_or_default();
            let mut backend_env = vec![
                (
                    "APPDATA_DIR".to_string(),
                    app_local_data.to_string_lossy().into_owned(),
                ),
                (
                    "TEMP_DIR".to_string(),
                    handle
                        .path()
                        .temp_dir()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                ),
                (
                    "APP_VERSION".to_string(),
                    handle.package_info().version.to_string(),
                ),
                ("IS_TAURI".to_string(), "true".to_string()),
            ];

            #[cfg(target_os = "macos")]
            if let Ok(resource_dir) = handle.path().resource_dir() {
                backend_env.extend([
                    (
                        "AXHELPER_PATH".to_string(),
                        resource_dir
                            .join("native")
                            .join("axhelper")
                            .join("axhelper")
                            .to_string_lossy()
                            .into_owned(),
                    ),
                    (
                        "SCREENOCR_HELPER_PATH".to_string(),
                        resource_dir
                            .join("native")
                            .join("screenocr")
                            .join("screenocr-helper")
                            .to_string_lossy()
                            .into_owned(),
                    ),
                    (
                        "COLOR_PICKER_HELPER_PATH".to_string(),
                        resource_dir
                            .join("native")
                            .join("color-picker")
                            .join("color-picker-helper")
                            .to_string_lossy()
                        .into_owned(),
                    ),
                ]);
            }

            #[cfg(target_os = "windows")]
            {
                let helper_path = if cfg!(debug_assertions) {
                    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("../native/color-picker/windows.ps1")
                } else {
                    handle
                        .path()
                        .resource_dir()
                        .unwrap_or_default()
                        .join("native")
                        .join("color-picker")
                        .join("windows.ps1")
                };
                backend_env.push((
                    "COLOR_PICKER_HELPER_PATH".to_string(),
                    helper_path.to_string_lossy().into_owned(),
                ));
                let image_colors_helper_path = if cfg!(debug_assertions) {
                    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("../native/image-colors/windows.ps1")
                } else {
                    handle
                        .path()
                        .resource_dir()
                        .unwrap_or_default()
                        .join("native")
                        .join("image-colors")
                        .join("windows.ps1")
                };
                backend_env.push((
                    "IMAGE_COLORS_HELPER_PATH".to_string(),
                    image_colors_helper_path.to_string_lossy().into_owned(),
                ));
            }

            let script_path = if cfg!(debug_assertions) {
                // Tauri copies resources into target/debug only during a Rust build.
                // The backend bundler runs independently, so that copy quickly becomes
                // stale during development. Always execute the live workspace bundle.
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dist-backend/main.js")
            } else {
                handle
                    .path()
                    .resource_dir()
                    .map(|dir| dir.join("dist-backend").join("main.js"))
                    .unwrap_or_else(|_| std::path::PathBuf::from("dist-backend/main.js"))
            };
            let supervisor_handle = handle.clone();
            let supervisor_pending = pending_requests_app.clone();
            let supervisor_writer = backend_writer_app.clone();
            std::thread::spawn(move || {
                let script_path = if cfg!(debug_assertions) {
                    script_path
                } else {
                    let source_dir = script_path
                        .parent()
                        .map(std::path::Path::to_path_buf)
                        .unwrap_or_else(|| std::path::PathBuf::from("dist-backend"));
                    loop {
                        match stage_backend_bundle(&source_dir, &app_local_data) {
                            Ok(path) => break path,
                            Err(error) => {
                                log::error!("backend bundle staging failed: {}", error);
                                std::thread::sleep(Duration::from_secs(10));
                            }
                        }
                    }
                };
                let mut backend_env = backend_env;
                let backend_root = script_path
                    .parent()
                    .map(std::path::Path::to_path_buf)
                    .unwrap_or_else(|| app_local_data.join("backend"));
                let esbuild_platform = if cfg!(target_os = "windows") {
                    if cfg!(target_arch = "aarch64") {
                        "win32-arm64"
                    } else {
                        "win32-x64"
                    }
                } else if cfg!(target_arch = "aarch64") {
                    "darwin-arm64"
                } else {
                    "darwin-x64"
                };
                let esbuild_relative = if cfg!(target_os = "windows") {
                    std::path::PathBuf::from("esbuild.exe")
                } else {
                    std::path::PathBuf::from("bin").join("esbuild")
                };
                backend_env.push((
                    "ESBUILD_BINARY_PATH".to_string(),
                    backend_root
                        .join("node_modules")
                        .join("@esbuild")
                        .join(esbuild_platform)
                        .join(esbuild_relative)
                        .to_string_lossy()
                        .into_owned(),
                ));
                if !cfg!(debug_assertions) {
                    backend_env.push((
                        "NODE_PATH".to_string(),
                        backend_root.join("node_modules").to_string_lossy().into_owned(),
                    ));
                    backend_env.push((
                        "BUN_RUNTIME_TRANSPILER_CACHE_PATH".to_string(),
                        backend_root.join("bun-cache").to_string_lossy().into_owned(),
                    ));
                }

                // Bun bootstrap/download can involve the network and may take
                // seconds. Never perform it on Tauri's setup thread: doing so
                // blocks the Windows message loop and produces a frozen,
                // apparently non-responsive launcher window.
                let mut failures = 0_u32;
                loop {
                    let bun_command = match locate_bun(&app_local_data) {
                        Ok(path) => {
                            failures = 0;
                            path
                        }
                        Err(error) => {
                            failures = failures.saturating_add(1);
                            log::error!("backend runtime unavailable: {}", error);
                            let exponent = failures.min(5);
                            let delay_ms = (500_u64 * (1_u64 << exponent)).min(15_000);
                            log::info!(
                                "retrying backend runtime discovery in {}ms",
                                delay_ms
                            );
                            std::thread::sleep(Duration::from_millis(delay_ms));
                            continue;
                        }
                    };

                    let backend_config = BackendLaunchConfig {
                        executable: bun_command,
                        script_path: script_path.clone(),
                        env: backend_env.clone(),
                    };
                    supervise_backend(
                        supervisor_handle.clone(),
                        backend_config,
                        supervisor_writer.clone(),
                        supervisor_pending.clone(),
                    );
                }
            });

            // System Tray Menu Setup
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

            let show_item = MenuItem::with_id(&handle, "show", "Show Tezbar", true, None::<&str>)?;
            let settings_item =
                MenuItem::with_id(&handle, "settings", "Settings", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(&handle, "quit", "Quit Tezbar", true, None::<&str>)?;

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
                        let _ = open_settings_window(app.clone());
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

            let default_shortcut = Shortcut::new(
                if cfg!(target_os = "windows") {
                    Some(tauri_plugin_global_shortcut::Modifiers::CONTROL)
                } else {
                    Some(tauri_plugin_global_shortcut::Modifiers::ALT)
                },
                tauri_plugin_global_shortcut::Code::Space,
            );
            if let Err(error) = handle.global_shortcut().register(default_shortcut) {
                log::error!("failed to register default launcher shortcut: {:?}", error);
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod backend_timeout_tests {
    use super::*;

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
