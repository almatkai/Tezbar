// src-tauri/src/lib.rs
mod native_input;
mod native_terminal;

use serde_json::json;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State, WebviewWindow};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tokio::sync::oneshot;

struct BackendState {
    writer: Arc<Mutex<Option<TcpStream>>>,
    pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<serde_json::Value>>>>,
    request_counter: Arc<Mutex<u64>>,
}

#[derive(Default)]
struct WindowBehaviorState {
    suppress_blur_hide: Mutex<bool>,
    backend_hidden_windows: Mutex<Vec<String>>,
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

fn place_window(window: &WebviewWindow) -> Result<(), String> {
    let monitor = window
        .primary_monitor()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No monitor found".to_string())?;

    let scale_factor = monitor.scale_factor();
    let size = monitor.size();

    let monitor_width = size.width as f64 / scale_factor;
    let monitor_height = size.height as f64 / scale_factor;

    let win_width = 760.0;
    let current_size = window.outer_size().map_err(|e| e.to_string())?;
    let _win_height = current_size.height as f64 / scale_factor;

    let x = (monitor_width - win_width) / 2.0;
    let y = monitor_height * 0.12;

    window
        .set_position(LogicalPosition::new(x, y))
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn open_settings_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }

    let _settings = tauri::WebviewWindowBuilder::new(
        &app,
        "settings",
        tauri::WebviewUrl::App("index.html?window=settings".into()),
    )
    .title("Tezbar Settings")
    .inner_size(920.0, 680.0)
    .resizable(true)
    .decorations(true)
    .build()
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn open_settings_window_cmd(app: AppHandle) -> Result<(), String> {
    open_settings_window(app)
}

#[tauri::command]
fn toggle_window(window: WebviewWindow) -> Result<(), String> {
    if window.is_visible().map_err(|e| e.to_string())? {
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
    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
fn show_window(window: WebviewWindow) -> Result<(), String> {
    place_window(&window)?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    let _ = window.emit("window-shown", json!({ "resetUi": false }));
    Ok(())
}

#[tauri::command]
fn close_current_window(window: WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        window.hide().map_err(|e| e.to_string())
    } else {
        window.close().map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn start_window_snap_drag(
    window: WebviewWindow,
    state: State<'_, WindowBehaviorState>,
) -> Result<(), String> {
    *state.suppress_blur_hide.lock().unwrap() = true;
    if let Err(error) = window.start_dragging() {
        *state.suppress_blur_hide.lock().unwrap() = false;
        return Err(error.to_string());
    }
    Ok(())
}

#[tauri::command]
fn end_window_snap_drag(state: State<'_, WindowBehaviorState>) {
    *state.suppress_blur_hide.lock().unwrap() = false;
}

#[tauri::command]
fn set_suppress_blur_hide(state: State<'_, WindowBehaviorState>, value: bool) {
    *state.suppress_blur_hide.lock().unwrap() = value;
}

#[tauri::command]
fn window_set_content_height(
    window: WebviewWindow,
    height: f64,
    zoom_factor: f64,
) -> Result<(), String> {
    let actual_height = height * zoom_factor;
    let clamped_height = actual_height.clamp(120.0, 640.0);
    window
        .set_size(LogicalSize::new(760.0, clamped_height))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn update_raymes_shortcut(app: AppHandle, shortcut_str: String) -> Result<(), String> {
    let _ = app.global_shortcut().unregister_all();
    let clean_shortcut = shortcut_str
        .replace("Option", "Alt")
        .replace("CommandOrControl", "Super")
        .replace("CmdOrCtrl", "Super")
        .replace("Cmd", "Super");
    let shortcut = Shortcut::from_str(&clean_shortcut)
        .map_err(|e| format!("Invalid shortcut format: {:?}", e))?;
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
    let id = {
        let mut counter = state.request_counter.lock().unwrap();
        *counter += 1;
        *counter
    };

    let (tx, rx) = oneshot::channel();
    {
        let mut pending = state.pending_requests.lock().unwrap();
        pending.insert(id, tx);
    }

    let msg = json!({
      "type": "invoke",
      "id": id,
      "channel": channel,
      "payload": payload
    })
    .to_string();

    {
        let mut backend_writer = state.writer.lock().unwrap();
        if let Some(writer) = backend_writer.as_mut() {
            if writeln!(writer, "{}", msg).is_err() || writer.flush().is_err() {
                state.pending_requests.lock().unwrap().remove(&id);
                *backend_writer = None;
                return Err("Failed to write to backend runner process".to_string());
            }
            log::debug!("wrote backend request: id={} channel={}", id, channel);
        } else {
            state.pending_requests.lock().unwrap().remove(&id);
            return Err("Backend runner process is not running".to_string());
        }
    }

    match tokio::time::timeout(Duration::from_secs(30), rx).await {
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
            let message = "Backend request timed out after 30 seconds".to_string();
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
    .plugin(tauri_plugin_log::Builder::default().build())
    .plugin(tauri_plugin_global_shortcut::Builder::new()
      .with_handler(move |app, _shortcut, event| {
        if event.state() == ShortcutState::Pressed {
          if let Some(win) = app.get_webview_window("main") {
            let _ = toggle_window(win);
          }
        }
      })
      .build())
    .plugin(tauri_plugin_shell::init())
        .manage(BackendState {
      writer: backend_writer,
      pending_requests,
            request_counter,
        })
        .manage(WindowBehaviorState::default())
        .manage(native_terminal::NativeTerminalState::default())
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::Focused(false) = event {
                let state = window.state::<WindowBehaviorState>();
                if !*state.suppress_blur_hide.lock().unwrap() {
                    let _ = window.hide();
                }
            }
        })
    .invoke_handler(tauri::generate_handler![
      call_backend,
      open_settings_window_cmd,
      toggle_window,
      hide_window,
      show_window,
      close_current_window,
      quit_app,
            start_window_snap_drag,
            end_window_snap_drag,
            set_suppress_blur_hide,
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
      native_terminal::native_terminal_write,
      native_terminal::native_terminal_resize,
      native_terminal::native_terminal_kill
    ])
    .setup(move |app| {
      let handle = app.handle().clone();
      // Spawn Background Bun process
      let app_local_data = handle.path().app_local_data_dir().unwrap_or_default();
      let bun_cached_path = app_local_data.join("bun").join("bun");

      let home_bun_path = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .map(|home| home.join(".bun").join("bin").join("bun"));
      let bun_command = if bun_cached_path.exists() {
        Some(bun_cached_path)
      } else if home_bun_path.as_ref().is_some_and(|path| path.exists()) {
        home_bun_path
      } else if Command::new("bun").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok() {
        Some(std::path::PathBuf::from("bun"))
      } else {
        None
      };

      let Some(bun_command) = bun_command else {
        return Err("Bun is required to run the Tauri backend. Install Bun or place it in the app data bun directory.".into());
      };
      let mut cmd = Command::new(bun_command);

      cmd.env("APPDATA_DIR", app_local_data.to_string_lossy().to_string());
      cmd.env("TEMP_DIR", handle.path().temp_dir().unwrap_or_default().to_string_lossy().to_string());
      cmd.env("APP_VERSION", handle.package_info().version.to_string());
      cmd.env("IS_TAURI", "true");

      if let Ok(resource_dir) = handle.path().resource_dir() {
        cmd.env("AXHELPER_PATH", resource_dir.join("native").join("axhelper").join("axhelper"));
        cmd.env("SCREENOCR_HELPER_PATH", resource_dir.join("native").join("screenocr").join("screenocr-helper"));
        cmd.env("ESBUILD_BINARY_PATH", resource_dir.join("bin").join("esbuild"));
      }

      let script_path = if cfg!(debug_assertions) {
        // Tauri copies resources into target/debug only during a Rust build.
        // The backend bundler runs independently, so that copy quickly becomes
        // stale during development. Always execute the live workspace bundle.
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
          .join("../dist-backend/main.js")
      } else {
        handle
          .path()
          .resource_dir()
          .map(|dir| dir.join("dist-backend").join("main.js"))
          .unwrap_or_else(|_| std::path::PathBuf::from("dist-backend/main.js"))
      };
      log::info!("launching backend sidecar: {}", script_path.display());
      cmd.arg(script_path);

      let ipc_listener = TcpListener::bind("127.0.0.1:0")?;
      let ipc_port = ipc_listener.local_addr()?.port();
      ipc_listener.set_nonblocking(true)?;
      cmd.env("BACKEND_IPC_PORT", ipc_port.to_string());
      cmd.stdin(Stdio::null())
         .stdout(Stdio::piped())
         .stderr(Stdio::piped());

      let app_handle_clone = handle.clone();
      let pending_requests_thread = pending_requests_app.clone();
      let backend_writer_shutdown = backend_writer_app.clone();

      let mut child = cmd
        .spawn()
        .map_err(|error| format!("Failed to spawn background runner process: {error}"))?;
      let stdout = child.stdout.take().ok_or("Failed to open backend stdout")?;
      let stderr = child.stderr.take().ok_or("Failed to open backend stderr")?;

      let connect_deadline = Instant::now() + Duration::from_secs(5);
      let backend_stream = loop {
        match ipc_listener.accept() {
          Ok((stream, _)) => break stream,
          Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            if Instant::now() >= connect_deadline {
              let _ = child.kill();
              return Err("Backend runner did not connect to its IPC socket".into());
            }
            std::thread::sleep(Duration::from_millis(10));
          }
          Err(error) => {
            let _ = child.kill();
            return Err(format!("Failed to accept backend IPC connection: {error}").into());
          }
        }
      };
      *backend_writer_app.lock().unwrap() = Some(backend_stream);
      log::info!("backend sidecar IPC connected on localhost");

      std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
          log::info!("backend sidecar: {}", line);
        }
        log::error!("backend stderr reader stopped");
      });

      std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line_res in reader.lines() {
          let line = match line_res {
            Ok(l) => l,
            Err(_) => break,
          };
          if line.trim().is_empty() {
            continue;
          }

          if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
            log::debug!("received backend sidecar message");
            if let Some(msg_type) = val.get("type").and_then(|t| t.as_str()) {
              if msg_type == "reply" {
                if let Some(id) = val.get("id").and_then(|i| i.as_u64()) {
                  let mut pending = pending_requests_thread.lock().unwrap();
                  if let Some(tx) = pending.remove(&id) {
                    let reply_val = if let Some(err) = val.get("error") {
                      json!({ "error": err })
                    } else {
                      json!({ "result": val.get("result").unwrap_or(&serde_json::Value::Null) })
                    };
                    let _ = tx.send(reply_val);
                  }
                }
              } else if msg_type == "event" {
                if let Some(channel) = val.get("channel").and_then(|c| c.as_str()) {
                  let payload = val.get("payload").unwrap_or(&serde_json::Value::Null);
                  let _ = app_handle_clone.emit(channel, payload);
                }
              } else if msg_type == "dialog" {
                println!("[Tauri Dialog] Dialog options: {:?}", val.get("options"));
                            } else if msg_type == "app_quit" {
                                app_handle_clone.exit(0);
                            } else if msg_type == "window_suppress_blur" {
                                if let Some(value) = val.get("value").and_then(|value| value.as_bool()) {
                                    let state = app_handle_clone.state::<WindowBehaviorState>();
                                    *state.suppress_blur_hide.lock().unwrap() = value;
                                }
                            } else if msg_type == "app_visibility" {
                                if let Some(value) = val.get("visible").and_then(|value| value.as_bool()) {
                                    set_backend_app_visibility(&app_handle_clone, value);
              }
            }
          } else {
            log::error!("backend sidecar emitted invalid JSON on stdout: {}", line);
          }
        }
        log::error!("backend stdout reader stopped");
        }

        let mut pending = pending_requests_thread.lock().unwrap();
        for (_, sender) in pending.drain() {
          let _ = sender.send(json!({ "error": "Backend runner stopped" }));
        }
        *backend_writer_shutdown.lock().unwrap() = None;
        let _ = child.kill();
      });

      // System Tray Menu Setup
      use tauri::menu::{Menu, MenuItem};
      use tauri::tray::{TrayIconBuilder, TrayIconEvent};

      let show_item = MenuItem::with_id(&handle, "show", "Show Tezbar", true, None::<&str>)?;
      let settings_item = MenuItem::with_id(&handle, "settings", "Settings", true, None::<&str>)?;
      let quit_item = MenuItem::with_id(&handle, "quit", "Quit Tezbar", true, None::<&str>)?;

      let menu = Menu::with_items(&handle, &[&show_item, &settings_item, &quit_item])?;

      let handle_tray = handle.clone();
      let tray_icon = {
        let resource_path = handle.path().resource_dir()
          .expect("failed to resolve resource dir")
          .join("trayIconTemplate@2x.png");
        let try_load_png = |path: &std::path::Path| -> Option<tauri::image::Image<'static>> {
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
        .on_menu_event(move |app, event| {
          match event.id.as_ref() {
            "show" => {
              if let Some(win) = app.get_webview_window("main") {
                let _ = show_window(win);
              }
            }
            "settings" => {
              let _ = open_settings_window(app.clone());
            }
            "quit" => {
              app.exit(0);
            }
            _ => {}
          }
        })
        .on_tray_icon_event(move |tray, event| {
          if let TrayIconEvent::Click { .. } = event {
            let app = tray.app_handle();
            if let Some(win) = app.get_webview_window("main") {
              let _ = toggle_window(win);
            }
          }
        })
        .build(&handle_tray)?;



      let default_shortcut = Shortcut::new(
        Some(tauri_plugin_global_shortcut::Modifiers::ALT),
        tauri_plugin_global_shortcut::Code::Space,
      );
      let _ = handle.global_shortcut().register(default_shortcut);

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
