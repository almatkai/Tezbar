use super::common::{CoreContext, CoreResult, HandlerResult};
use base64::Engine;
use serde_json::{json, Value};
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use std::path::Path;
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

pub const CHANNELS: &[&str] = &[
    "shell:open",
    "open-external-url",
    "window-set-content-height",
    "app:confirm-quit",
    "app:open-extensions",
    "core:platform:open-app",
    "core:platform:open-file",
    "core:platform:open-url",
    "core:platform:open-with-app",
    "core:platform:reveal-file",
    "core:platform:selected-text",
    "core:platform:screenshot",
    "core:platform:paste-text",
    "core:platform:paste",
    "core:platform:clipboard-write",
    "core:platform:confirm",
    "core:platform:visibility",
    "core:platform:suppress-blur",
    "core:platform:update-launcher-hotkey",
    "core:platform:clear-webview-cache",
    "core:platform:open-settings",
    "core:shortcut:update-command",
    "core:platform:quit",
    "core:platform:open-surface",
    "window:show",
    "window:hide",
    "window:close-current",
    "window:snap-drag-start",
    "window:snap-drag-end",
    "window:suppress-blur-hide",
];

#[derive(Default)]
struct Shortcuts {
    launcher: Option<Shortcut>,
    commands: HashMap<String, Shortcut>,
}

#[derive(Clone)]
pub struct PlatformCore {
    ctx: CoreContext,
    shortcuts: Arc<Mutex<Shortcuts>>,
    shortcut_updates: Arc<Mutex<()>>,
}

impl PlatformCore {
    pub fn new(ctx: CoreContext) -> Self {
        Self {
            ctx,
            shortcuts: Arc::new(Mutex::new(Shortcuts::default())),
            shortcut_updates: Arc::new(Mutex::new(())),
        }
    }

    pub fn initialize_shortcuts(&self) -> Result<(), String> {
        if self.ctx.app.is_none() {
            return Ok(());
        }
        let config = self.ctx.config.read()?;
        let default = if cfg!(target_os = "windows") {
            "Control+Space"
        } else {
            "Alt+Space"
        };
        let requested = config["raymesHotkey"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(default);
        self.register_shortcut(None, requested)?;
        if let Some(commands) = config["commandHotkeys"].as_object() {
            for (id, key) in commands {
                if let Some(key) = key.as_str() {
                    if let Err(error) = self.register_shortcut(Some(id), key) {
                        log::warn!("Shortcut {id}: {error}");
                    }
                }
            }
        }
        Ok(())
    }

    pub fn command_for_shortcut(&self, id: u32) -> Option<String> {
        self.shortcuts
            .lock()
            .ok()?
            .commands
            .iter()
            .find(|(_, shortcut)| shortcut.id() == id)
            .map(|(id, _)| id.clone())
    }

    pub fn is_launcher_shortcut(&self, id: u32) -> bool {
        self.shortcuts
            .lock()
            .ok()
            .and_then(|s| s.launcher)
            .is_some_and(|s| s.id() == id)
    }

    /// Register first, then release the previous binding; a conflict must not
    /// remove the user's only working launcher shortcut.
    pub fn register_shortcut(&self, command: Option<&str>, raw: &str) -> Result<String, String> {
        let _update = self.shortcut_updates.lock().map_err(|e| e.to_string())?;
        self.register_shortcut_inner(command, raw)
    }

    fn register_shortcut_inner(&self, command: Option<&str>, raw: &str) -> Result<String, String> {
        let app = self
            .ctx
            .app
            .as_ref()
            .ok_or("Global shortcuts require a desktop session")?;
        let normalized = normalize_shortcut(raw);
        let next = if normalized.is_empty() {
            None
        } else {
            Some(Shortcut::from_str(&normalized).map_err(|e| format!("Invalid shortcut: {e}"))?)
        };
        if command.is_none() && next.is_none() {
            return Err("Launcher shortcut cannot be empty".into());
        }
        let mut shortcuts = self.shortcuts.lock().map_err(|e| e.to_string())?;
        let old = command
            .and_then(|id| shortcuts.commands.get(id).copied())
            .or_else(|| {
                if command.is_none() {
                    shortcuts.launcher
                } else {
                    None
                }
            });
        if old == next {
            return Ok(normalized);
        }
        if let Some(next) = next {
            if shortcuts.launcher == Some(next) || shortcuts.commands.values().any(|s| *s == next) {
                return Err("This shortcut is already assigned to another Tezbar command".into());
            }
            app.global_shortcut()
                .register(next)
                .map_err(|e| format!("Could not register shortcut: {e}"))?;
        }
        if let Some(old) = old {
            if let Err(error) = app.global_shortcut().unregister(old) {
                if let Some(next) = next {
                    if let Err(cleanup) = app.global_shortcut().unregister(next) {
                        return Err(format!("Could not replace shortcut: {error}; could not remove new binding: {cleanup}"));
                    }
                }
                return Err(format!("Could not replace shortcut: {error}"));
            }
        }
        if let Some(id) = command {
            if let Some(next) = next {
                shortcuts.commands.insert(id.into(), next);
            } else {
                shortcuts.commands.remove(id);
            }
        } else {
            shortcuts.launcher = next;
        }
        Ok(normalized)
    }

    // Called with shortcut_updates held. An initial registration may have failed,
    // so rollback must also support returning to an unregistered launcher.
    fn restore_launcher_shortcut(&self, previous: Option<Shortcut>) -> Result<(), String> {
        if let Some(previous) = previous {
            return self
                .register_shortcut_inner(None, &previous.to_string())
                .map(|_| ());
        }
        let mut shortcuts = self.shortcuts.lock().map_err(|e| e.to_string())?;
        if let Some(current) = shortcuts.launcher {
            self.app()?
                .global_shortcut()
                .unregister(current)
                .map_err(|e| e.to_string())?;
            shortcuts.launcher = None;
        }
        Ok(())
    }

    pub async fn handle(&self, channel: &str, args: &[Value]) -> HandlerResult {
        if !CHANNELS.contains(&channel) {
            return None;
        }
        Some(self.dispatch(channel, args).await)
    }

    async fn dispatch(&self, channel: &str, args: &[Value]) -> CoreResult {
        let value = args.first().cloned().unwrap_or(Value::Null);
        let text = value.as_str().unwrap_or("");
        match channel {
            "window-set-content-height" => Ok(Value::Null), // User-resized native layout owns height.
            "window:show"
            | "window:hide"
            | "window:close-current"
            | "window:snap-drag-start"
            | "window:snap-drag-end" => {
                let app = self.app()?;
                let window = app
                    .get_webview_window("main")
                    .ok_or("Main window is unavailable")?;
                match channel {
                    "window:show" => crate::show_window(window)?,
                    "window:hide" => crate::hide_window(window)?,
                    "window:close-current" => crate::close_current_window(window)?,
                    "window:snap-drag-start" => crate::start_window_snap_drag(
                        window,
                        app.clone(),
                        app.state::<crate::WindowBehaviorState>(),
                    )?,
                    _ => crate::end_window_snap_drag(
                        window,
                        app.clone(),
                        app.state::<crate::WindowBehaviorState>(),
                    ),
                }
                Ok(Value::Null)
            }
            "core:platform:update-launcher-hotkey" => {
                let platform = self.clone();
                let requested = text.to_string();
                // Complete registration, persistence and any rollback even if the IPC
                // caller is cancelled, and serialize against other registrations.
                tokio::task::spawn_blocking(move || {
                    let _update = platform
                        .shortcut_updates
                        .lock()
                        .map_err(|e| e.to_string())?;
                    let previous = platform
                        .shortcuts
                        .lock()
                        .map_err(|e| e.to_string())?
                        .launcher;
                    match platform.register_shortcut_inner(None, &requested) {
                        Ok(accelerator) => {
                            persist_or_rollback(
                                || {
                                    platform
                                        .ctx
                                        .config
                                        .patch(&json!({"raymesHotkey":accelerator}))
                                        .map(|_| ())
                                },
                                || platform.restore_launcher_shortcut(previous),
                            )?;
                            Ok(json!({"ok":true,"accelerator":accelerator}))
                        }
                        Err(error) => Ok(json!({"ok":false,"accelerator":requested,"error":error})),
                    }
                })
                .await
                .map_err(|e| e.to_string())?
            }
            "core:shortcut:update-command" => {
                let key = args.get(1).and_then(Value::as_str).unwrap_or("");
                let _update = self.shortcut_updates.lock().map_err(|e| e.to_string())?;
                let previous = self
                    .shortcuts
                    .lock()
                    .map_err(|e| e.to_string())?
                    .commands
                    .get(text)
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                match self.register_shortcut_inner(Some(text), key) {
                    Ok(accelerator) => Ok(
                        json!({"ok":true,"accelerator":accelerator,"previousAccelerator":previous}),
                    ),
                    Err(error) => Ok(json!({"ok":false,"accelerator":key,"error":error})),
                }
            }
            "core:platform:visibility" => {
                crate::set_backend_app_visibility(self.app()?, value.as_bool().unwrap_or(false));
                Ok(Value::Null)
            }
            "core:platform:suppress-blur" | "window:suppress-blur-hide" => {
                let state = self.app()?.state::<crate::WindowBehaviorState>();
                *state.suppress_blur_hide.lock().map_err(|e| e.to_string())? =
                    value.as_bool().unwrap_or(false);
                Ok(Value::Null)
            }
            "app:open-extensions" | "core:platform:open-surface" => {
                let surface = if channel == "app:open-extensions" {
                    "extensions"
                } else {
                    text
                };
                if ![
                    "command",
                    "settings",
                    "settings-extensions",
                    "clipboard",
                    "extensions",
                    "indexing",
                    "snippets",
                    "notes",
                    "emoji-picker",
                    "open-ports",
                    "system-monitor",
                    "onboarding",
                ]
                .contains(&surface)
                {
                    return Err("Unknown app surface".into());
                }
                crate::set_backend_app_visibility(self.app()?, true);
                self.ctx.emit("app:open-surface", json!(surface));
                Ok(json!(true))
            }
            "core:platform:open-settings" => {
                let app = self.app()?.clone();
                tokio::task::spawn_blocking(move || crate::open_settings_window(app))
                    .await
                    .map_err(|e| e.to_string())??;
                // The launcher listener writes the shared Settings tab storage and
                // activates the native Settings window, including an already-open one.
                if let Some(surface) = settings_surface(text) {
                    self.ctx.emit("app:open-surface", json!(surface));
                }
                Ok(json!({"ok":true}))
            }
            "core:platform:quit" => {
                crate::quit_app_now(self.app()?);
                Ok(json!({"ok":true}))
            }
            "app:confirm-quit" => {
                if self.confirm(&json!({"title":"Quit Tezbar?","message":"Quit Tezbar?","detail":"Background tasks and active terminal sessions will stop.","buttons":["Cancel","Quit"]})).await?{
                    crate::quit_app_now(self.app()?);
                }
                Ok(Value::Null)
            }
            "core:platform:confirm" => {
                Ok(json!({"response":if self.confirm(&value).await?{1}else{0}}))
            }
            "core:platform:clear-webview-cache" => {
                for (_, window) in self.app()?.webview_windows() {
                    window
                        .clear_all_browsing_data()
                        .map_err(|e| e.to_string())?;
                }
                Ok(json!({"ok":true}))
            }
            "core:platform:selected-text" => selected_text().await.map(Value::String),
            "core:platform:screenshot" => {
                let window = self
                    .app()?
                    .get_webview_window("main")
                    .ok_or("Main window is unavailable")?;
                let bytes = crate::native_input::active_capture(window).await?;
                let max = value["maxWidth"].as_u64().unwrap_or(1600).clamp(320, 4096) as u32;
                tokio::task::spawn_blocking(move||{
                    let mut image=image::load_from_memory(&bytes).map_err(|e|e.to_string())?;
                    if image.width()>max {image=image.resize(max,u32::MAX,image::imageops::FilterType::Triangle);}
                    let mut out=std::io::Cursor::new(Vec::new());image.write_to(&mut out,image::ImageFormat::Png).map_err(|e|e.to_string())?;
                    Ok(json!({"type":"image","data":base64::engine::general_purpose::STANDARD.encode(out.into_inner()),"mimeType":"image/png","width":image.width(),"height":image.height()}))
                }).await.map_err(|e|e.to_string())?
            }
            "core:platform:paste" | "core:platform:paste-text" => {
                if channel == "core:platform:paste-text" {
                    self.ctx
                        .request("clipboard:write", vec![json!(text)])
                        .await?;
                }
                crate::set_backend_app_visibility(self.app()?, false);
                // Hiding NSWindows alone leaves Tezbar active and can swallow Cmd+V.
                #[cfg(target_os = "macos")]
                self.app()?.hide().map_err(|e| e.to_string())?;
                tokio::time::sleep(Duration::from_millis(80)).await;
                crate::native_input::press_key(
                    "v".into(),
                    vec![if cfg!(target_os = "macos") {
                        "cmd".into()
                    } else {
                        "ctrl".into()
                    }],
                )?;
                Ok(json!({"ok":true,"message":"Pasted text"}))
            }
            "core:platform:clipboard-write" => tokio::task::spawn_blocking(move || {
                let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
                if let Some(file) = value["file"].as_str() {
                    let image = image::open(file).map_err(|e| e.to_string())?.into_rgba8();
                    clipboard
                        .set_image(arboard::ImageData {
                            width: image.width() as usize,
                            height: image.height() as usize,
                            bytes: std::borrow::Cow::Owned(image.into_raw()),
                        })
                        .map_err(|e| e.to_string())?;
                } else if let Some(html) = value["html"].as_str() {
                    clipboard
                        .set_html(html, value["text"].as_str())
                        .map_err(|e| e.to_string())?;
                } else {
                    clipboard
                        .set_text(value["text"].as_str().unwrap_or(""))
                        .map_err(|e| e.to_string())?;
                }
                Ok(json!({"ok":true}))
            })
            .await
            .map_err(|e| e.to_string())?,
            "open-external-url" => {
                let url = reqwest::Url::parse(text).map_err(|e| e.to_string())?;
                let host = url.host_str().unwrap_or("");
                if url.scheme() != "https"
                    || !(host == "github.com" || host.ends_with(".github.com"))
                {
                    return Err("Only GitHub HTTPS URLs are supported here".into());
                }
                open_target(text).await
            }
            "shell:open" | "core:platform:open-file" | "core:platform:open-url" => {
                open_target(text).await
            }
            "core:platform:open-app" => {
                let target = application_target(&value)?;
                #[cfg(target_os = "macos")]
                output("/usr/bin/open", &["-a", target], 10).await?;
                #[cfg(target_os = "windows")]
                if target.starts_with("shell:AppsFolder\\") {
                    tokio::process::Command::new("explorer.exe")
                        .arg(target)
                        .creation_flags(0x08000000)
                        .spawn()
                        .map_err(|e| e.to_string())?;
                } else {
                    open_target(target).await?;
                }
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                open_target(target).await?;
                Ok(json!({"ok":true,"message":"Opened application"}))
            }
            "core:platform:open-with-app" => {
                let path = value["path"].as_str().ok_or("File path required")?;
                let app = value["appPath"]
                    .as_str()
                    .ok_or("Application path required")?;
                #[cfg(target_os = "macos")]
                output("/usr/bin/open", &["-a", app, path], 10).await?;
                #[cfg(target_os = "windows")]
                {
                    let mut command = windows_open_with_command(app, path);
                    let output = tokio::time::timeout(Duration::from_secs(10), command.output())
                        .await
                        .map_err(|_| "Starting application timed out".to_string())?
                        .map_err(|e| e.to_string())?;
                    if !output.status.success() {
                        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
                    }
                }
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                {
                    tokio::process::Command::new(app)
                        .arg(path)
                        .spawn()
                        .map_err(|e| e.to_string())?;
                }
                Ok(json!({"ok":true,"message":"Opened file"}))
            }
            "core:platform:reveal-file" => {
                #[cfg(target_os = "macos")]
                output("/usr/bin/open", &["-R", text], 10).await?;
                #[cfg(target_os = "windows")]
                {
                    tokio::process::Command::new("explorer.exe")
                        .arg(format!("/select,{text}"))
                        .creation_flags(0x08000000)
                        .spawn()
                        .map_err(|e| e.to_string())?;
                }
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                open_target(
                    Path::new(text)
                        .parent()
                        .and_then(|p| p.to_str())
                        .unwrap_or(text),
                )
                .await?;
                Ok(json!({"ok":true}))
            }
            _ => Err(format!("Unknown platform operation: {channel}")),
        }
    }

    fn app(&self) -> Result<&tauri::AppHandle, String> {
        self.ctx
            .app
            .as_ref()
            .ok_or_else(|| "This operation requires a desktop session".into())
    }

    async fn confirm(&self, options: &Value) -> Result<bool, String> {
        let app = self.app()?;
        let title = options["title"].as_str().unwrap_or("Confirm");
        let message = options["message"].as_str().unwrap_or(title);
        let detail = options["detail"].as_str().unwrap_or("");
        let cancel = options["buttons"][0]
            .as_str()
            .unwrap_or("Cancel")
            .to_string();
        let accept = options["buttons"][1].as_str().unwrap_or("OK").to_string();
        let (send, receive) = tokio::sync::oneshot::channel();
        let state = app.state::<crate::WindowBehaviorState>();
        let previous = std::mem::replace(
            &mut *state.suppress_blur_hide.lock().map_err(|e| e.to_string())?,
            true,
        );
        let dialog_app = app.clone();
        app.dialog()
            .message(if detail.is_empty() {
                message.to_string()
            } else {
                format!("{message}\n\n{detail}")
            })
            .title(title)
            .buttons(MessageDialogButtons::OkCancelCustom(accept, cancel))
            .kind(if options["type"] == "warning" {
                MessageDialogKind::Warning
            } else {
                MessageDialogKind::Info
            })
            .show(move |accepted| {
                if let Ok(mut suppress) = dialog_app
                    .state::<crate::WindowBehaviorState>()
                    .suppress_blur_hide
                    .lock()
                {
                    *suppress = previous;
                }
                let _ = send.send(accepted);
            });
        receive
            .await
            .map_err(|_| "Confirmation dialog closed".to_string())
    }
}

fn normalize_shortcut(raw: &str) -> String {
    let control = if cfg!(target_os = "macos") {
        "Super"
    } else {
        "Control"
    };
    let normalized = raw
        .trim()
        .replace("CommandOrControl", control)
        .replace("CmdOrCtrl", control)
        .replace("Option", "Alt")
        .replace("Command", "Super")
        .replace("Cmd", "Super")
        .replace("Ctrl", "Control");
    if cfg!(target_os = "windows") && normalized.eq_ignore_ascii_case("Alt+Space") {
        "Control+Space".into()
    } else {
        normalized
    }
}

pub async fn output(program: &str, args: &[&str], timeout: u64) -> Result<String, String> {
    let mut command = tokio::process::Command::new(program);
    command.args(args).kill_on_drop(true);
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(Duration::from_secs(timeout), command.output())
        .await
        .map_err(|_| format!("{program} timed out"))?
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    String::from_utf8(output.stdout).map_err(|e| e.to_string())
}

async fn open_target(target: &str) -> CoreResult {
    if target.trim().is_empty() || target.contains('\0') {
        return Err("An open target is required".into());
    }
    let target = target.to_string();
    tokio::task::spawn_blocking(move || open::that_detached(target).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??;
    Ok(json!({"ok":true,"message":"Opened"}))
}

async fn selected_text() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    let selected = output(
        "/usr/bin/osascript",
        &[
            "-e",
            r#"tell application "System Events"
  set frontmostProcess to first application process whose frontmost is true
  tell frontmostProcess
    set focusedElement to value of attribute "AXFocusedUIElement"
    set selectedValue to value of attribute "AXSelectedText" of focusedElement
    if selectedValue is missing value then return ""
    return selectedValue as text
  end tell
end tell"#,
        ],
        3,
    )
    .await?;
    #[cfg(target_os = "macos")]
    let selected = {
        let line = selected.strip_suffix('\n').unwrap_or(&selected);
        line.strip_suffix('\r').unwrap_or(line).to_string()
    };
    #[cfg(target_os = "windows")]
    let selected = {
        let source = r#"Add-Type -AssemblyName UIAutomationClient; Add-Type -AssemblyName UIAutomationTypes
$focused=[System.Windows.Automation.AutomationElement]::FocusedElement
if($null -eq $focused){throw 'No focused text element'}
$pattern=$null
if(-not $focused.TryGetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern,[ref]$pattern)){throw 'The focused application does not expose text selection'}
$text=(@($pattern.GetSelection() | ForEach-Object {$_.GetText(-1)}) -join '')
[Console]::Write([Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($text)))"#;
        let encoded = output(
            "powershell.exe",
            &[
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                source,
            ],
            3,
        )
        .await?;
        String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(encoded.trim())
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?
    };
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let selected: String =
        return Err("Reading selected text is supported on macOS and Windows".into());
    if selected.is_empty() {
        Err("No text is selected in the focused application".into())
    } else {
        Ok(selected)
    }
}

fn application_target(value: &Value) -> Result<&str, String> {
    value["appPath"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| value["appName"].as_str().filter(|s| !s.trim().is_empty()))
        .filter(|s| !s.contains('\0'))
        .ok_or_else(|| "Application path or name required".into())
}

fn settings_surface(tab: &str) -> Option<&'static str> {
    match tab {
        "extensions" => Some("settings-extensions"),
        "ai" => Some("settings-ai"),
        "voice" => Some("settings-voice"),
        "knowledge" => Some("settings-knowledge"),
        "permissions" => Some("settings-permissions"),
        "storage" => Some("settings-storage"),
        "advanced" => Some("settings-advanced"),
        "" | "general" => Some("settings"),
        _ => None,
    }
}

fn persist_or_rollback(
    save: impl FnOnce() -> Result<(), String>,
    rollback: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    match save() {
        Ok(()) => Ok(()),
        Err(error) => match rollback() {
            Ok(()) => Err(format!(
                "Could not persist shortcut; previous shortcut restored: {error}"
            )),
            Err(restore) => Err(format!(
                "Could not persist shortcut: {error}; rollback failed: {restore}"
            )),
        },
    }
}

#[cfg(any(target_os = "windows", test))]
fn windows_open_with_command(app: &str, path: &str) -> tokio::process::Command {
    let mut command = tokio::process::Command::new("powershell.exe");
    command.args(["-NoLogo","-NoProfile","-NonInteractive","-Command",
        r#"$ErrorActionPreference='Stop'; Start-Process -FilePath $env:TEZBAR_APP_PATH -ArgumentList (, ('"' + $env:TEZBAR_TARGET_PATH + '"'))"#])
        .env("TEZBAR_APP_PATH",app).env("TEZBAR_TARGET_PATH",path).kill_on_drop(true);
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);
    command
}

#[cfg(test)]
#[path = "platform_tests.rs"]
mod tests;
