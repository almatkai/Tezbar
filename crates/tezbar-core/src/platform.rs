use super::common::{CoreContext, CoreResult, HandlerResult};
use crate::host::{DialogReply, DialogRequest, Shortcut, SurfaceOp};
use base64::Engine;
use serde_json::{json, Value};
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use std::path::Path;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

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
    "core:platform:ocr-images",
    "core:platform:paste-text",
    "core:platform:paste",
    "core:platform:clipboard-write",
    "core:platform:confirm",
    "core:platform:visibility",
    "core:platform:suppress-blur",
    "core:platform:update-launcher-hotkey",
    "core:platform:clear-webview-cache",
    "core:platform:trim-memory",
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
pub(crate) struct Shortcuts {
    initialized: bool,
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
            shortcuts: ctx.shortcuts.clone(),
            shortcut_updates: ctx.shortcut_updates.clone(),
            ctx,
        }
    }

    pub fn initialize_shortcuts(&self) -> Result<(), String> {
        if !self.ctx.host.shortcuts().available() {
            return Ok(());
        }
        let _update = self.shortcut_updates.lock().map_err(|e| e.to_string())?;
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
        self.register_shortcut_inner(None, requested)?;
        if let Some(commands) = config["commandHotkeys"].as_object() {
            for (id, key) in commands {
                if let Some(key) = key.as_str() {
                    if let Err(error) = self.set_binding(Some(id), key, &[]) {
                        log::warn!("Shortcut {id}: {error}");
                    }
                }
            }
        }
        self.shortcuts.lock().map_err(|e| e.to_string())?.initialized = true;
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
            .and_then(|s| s.launcher.clone())
            .is_some_and(|s| s.id() == id)
    }

    /// Register first, then release the previous binding; a conflict must not
    /// remove the user's only working launcher shortcut.
    pub fn register_shortcut(&self, command: Option<&str>, raw: &str) -> Result<String, String> {
        let _update = self.shortcut_updates.lock().map_err(|e| e.to_string())?;
        self.register_shortcut_inner(command, raw)
    }

    fn register_shortcut_inner(&self, command: Option<&str>, raw: &str) -> Result<String, String> {
        let aliases = command.map(crate::extensions::command_hotkey_aliases).unwrap_or_default();
        self.set_binding(command, raw, &aliases)
    }

    fn set_binding(&self, command: Option<&str>, raw: &str, aliases: &[String]) -> Result<String, String> {
        let normalized = normalize_shortcut(raw);
        let next = if normalized.is_empty() {
            None
        } else {
            Some(
                self.ctx
                    .host
                    .shortcuts()
                    .parse(&normalized)
                    .map_err(|e| format!("Invalid shortcut: {e}"))?,
            )
        };
        if command.is_none() && next.is_none() {
            return Err("Launcher shortcut cannot be empty".into());
        }
        let mut shortcuts = self.shortcuts.lock().map_err(|e| e.to_string())?;
        if let Some(id) = command {
            if !aliases.is_empty() {
                return self.replace_migrated_command(id, aliases, next, normalized, &mut shortcuts);
            }
        }
        let old = command
            .and_then(|id| shortcuts.commands.get(id).cloned())
            .or_else(|| {
                if command.is_none() {
                    shortcuts.launcher.clone()
                } else {
                    None
                }
            });
        if old == next {
            return Ok(normalized);
        }
        if let Some(next) = &next {
            if shortcuts.launcher.as_ref() == Some(next)
                || shortcuts.commands.values().any(|s| s == next)
            {
                return Err("This shortcut is already assigned to another Tezbar command".into());
            }
            self.ctx
                .host
                .shortcuts()
                .register(next)
                .map_err(|e| format!("Could not register shortcut: {e}"))?;
        }
        if let Some(old) = old {
            if let Err(error) = self.ctx.host.shortcuts().unregister(&old) {
                if let Some(next) = &next {
                    if let Err(cleanup) = self.ctx.host.shortcuts().unregister(next) {
                        return Err(format!("Could not replace shortcut: {error}; could not remove new binding: {cleanup}"));
                    }
                }
                return Err(format!("Could not replace shortcut: {error}"));
            }
        }
        if let Some(id) = command {
            if let Some(next) = &next {
                shortcuts.commands.insert(id.into(), next.clone());
            } else {
                shortcuts.commands.remove(id);
            }
        } else {
            shortcuts.launcher = next;
        }
        Ok(normalized)
    }

    fn replace_migrated_command(
        &self, id: &str, aliases: &[String], next: Option<Shortcut>,
        normalized: String, shortcuts: &mut Shortcuts,
    ) -> Result<String, String> {
        let owns = |key: &str| key == id || aliases.iter().any(|alias| alias == key);
        let mut previous = Vec::new();
        for key in std::iter::once(id).chain(aliases.iter().map(String::as_str)) {
            if let Some(binding) = shortcuts.commands.get(key) {
                if !previous.contains(binding) { previous.push(binding.clone()); }
            }
        }
        if let Some(binding) = &next {
            if shortcuts.launcher.as_ref() == Some(binding)
                || shortcuts.commands.iter().any(|(key, value)| !owns(key) && value == binding)
            {
                return Err("This shortcut is already assigned to another Tezbar command".into());
            }
        }
        let newly_registered = next.as_ref().is_some_and(|binding| !previous.contains(binding));
        if newly_registered {
            self.ctx.host.shortcuts().register(next.as_ref().unwrap())
                .map_err(|error| format!("Could not register shortcut: {error}"))?;
        }
        let mut removed = Vec::new();
        for binding in previous.iter().filter(|binding| Some(*binding) != next.as_ref()) {
            if let Err(error) = self.ctx.host.shortcuts().unregister(binding) {
                let mut rollback = Vec::new();
                for old in removed.iter().rev() {
                    if let Err(error) = self.ctx.host.shortcuts().register(old) { rollback.push(error.to_string()); }
                }
                if newly_registered {
                    if let Err(error) = self.ctx.host.shortcuts().unregister(next.as_ref().unwrap()) { rollback.push(error.to_string()); }
                }
                return Err(if rollback.is_empty() {
                    format!("Could not replace shortcut: {error}")
                } else { format!("Could not replace shortcut: {error}; rollback failed: {}", rollback.join("; ")) });
            }
            removed.push(binding.clone());
        }
        shortcuts.commands.retain(|key, _| !owns(key));
        if let Some(binding) = next { shortcuts.commands.insert(id.into(), binding); }
        Ok(normalized)
    }

    // Serialize OS changes with the durable write and compensate every changed owner.
    pub(crate) fn command_transaction<T>(&self, update: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let _guard = self.shortcut_updates.lock().map_err(|e| e.to_string())?;
        let before = self.shortcuts.lock().map_err(|e| e.to_string())?.commands.clone();
        let error = match update() {
            Ok(result) => return Ok(result),
            Err(error) => error,
        };
        let after = self.shortcuts.lock().map_err(|e| e.to_string())?.commands.clone();
        let mut failures = Vec::new();
        for (id, binding) in &after {
            if before.get(id) != Some(binding) {
                if let Err(e) = self.set_binding(Some(id), "", &[]) { failures.push(e); }
            }
        }
        for (id, binding) in &before {
            if after.get(id) != Some(binding) {
                if let Err(e) = self.set_binding(Some(id), &binding.to_string(), &[]) { failures.push(e); }
            }
        }
        if failures.is_empty() { Err(error) } else { Err(format!("{error}; rollback failed: {}", failures.join("; "))) }
    }

    // Caller holds the command transaction lock. Before initialization the
    // startup config read itself will pick up the migrated owners.
    pub(crate) fn reconcile_command(&self, id: &str, accelerator: &str) -> Result<(), String> {
        if self.shortcuts.lock().map_err(|e| e.to_string())?.initialized {
            self.register_shortcut_inner(Some(id), accelerator)?;
        }
        Ok(())
    }

    pub(crate) fn clean_obsolete_canonical(&self, canonical: &str, effective_alias: Option<(&str, &str)>) -> Result<(), String> {
        if self.shortcuts.lock().map_err(|e| e.to_string())?.initialized {
            self.set_binding(Some(canonical), "", &[])?;
            if let Some((alias, key)) = effective_alias {
                self.set_binding(Some(alias), key, &[])?;
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn restore_legacy_owner(&self, canonical: &str, alias: &str, key: &str) -> Result<(), String> {
        self.clean_obsolete_canonical(canonical, Some((alias, key)))
    }

    fn save_command_hotkey(&self, requested: &str, key: &str) -> CoreResult {
        self.command_transaction(|| {
            let (id, pending_target) = crate::extensions::hotkey_edit_owner(&self.ctx, requested);
            let aliases = crate::extensions::command_hotkey_aliases(&id);
            let previous = {
                let shortcuts = self.shortcuts.lock().map_err(|e| e.to_string())?;
                std::iter::once(id.as_str()).chain(aliases.iter().map(String::as_str))
                    .find_map(|key| shortcuts.commands.get(key)).map(ToString::to_string).unwrap_or_default()
            };
            let accelerator = match self.set_binding(Some(&id), key, &aliases) {
                Ok(accelerator) => accelerator,
                Err(error) => return Ok(json!({"ok":false,"accelerator":key,"error":error})),
            };
            self.ctx.config.update(|v| {
                if v.get("commandHotkeys").is_none() { v["commandHotkeys"] = json!({}); }
                crate::extensions::retire_legacy_command_hotkeys(v, &id)?;
                let keys = v["commandHotkeys"].as_object_mut().ok_or("Invalid commandHotkeys")?;
                if accelerator.is_empty() { keys.remove(&id); }
                else { keys.insert(id.clone(), json!(accelerator)); }
                if let Some(target) = pending_target {
                    v[format!("identityMigrationHotkey:{target}")] = json!(id);
                } else if !aliases.is_empty() {
                    v[format!("identityMigrationHotkey:{id}")] = json!(true);
                }
                Ok(())
            })?;
            Ok(json!({"ok":true,"accelerator":accelerator,"previousAccelerator":previous}))
        })
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
        if let Some(current) = &shortcuts.launcher {
            self.ctx
                .host
                .shortcuts()
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
                let op = match channel {
                    "window:show" => SurfaceOp::ShowLauncher,
                    "window:hide" => SurfaceOp::HideLauncher,
                    "window:close-current" => SurfaceOp::CloseCurrent,
                    "window:snap-drag-start" => SurfaceOp::SnapDragStart,
                    _ => SurfaceOp::SnapDragEnd,
                };
                self.ctx.host.surface(op).await?;
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
                        .launcher
                        .clone();
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
                let key = args.get(1).and_then(Value::as_str).unwrap_or("").to_owned();
                let id = text.to_owned();
                let platform = self.clone();
                tokio::task::spawn_blocking(move || platform.save_command_hotkey(&id, &key))
                    .await.map_err(|e| e.to_string())?
            }
            "core:platform:visibility" => {
                self.ctx
                    .host
                    .surface(SurfaceOp::SetAppVisible(value.as_bool().unwrap_or(false)))
                    .await?;
                Ok(Value::Null)
            }
            "core:platform:suppress-blur" | "window:suppress-blur-hide" => {
                self.ctx
                    .host
                    .surface(SurfaceOp::SuppressBlurHide(
                        value.as_bool().unwrap_or(false),
                    ))
                    .await?;
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
                    "vault",
                    "notes",
                    "emoji-picker",
                    "system-monitor",
                    "onboarding",
                ]
                .contains(&surface)
                {
                    return Err("Unknown app surface".into());
                }
                self.ctx
                    .host
                    .surface(SurfaceOp::SetAppVisible(true))
                    .await?;
                self.ctx.emit("app:open-surface", json!(surface));
                Ok(json!(true))
            }
            "core:platform:open-settings" => {
                self.ctx.host.surface(SurfaceOp::OpenSettings).await?;
                // The launcher listener writes the shared Settings tab storage and
                // activates the native Settings window, including an already-open one.
                if let Some(surface) = settings_surface(text) {
                    self.ctx.emit("app:open-surface", json!(surface));
                }
                Ok(json!({"ok":true}))
            }
            "core:platform:quit" => {
                self.ctx.host.surface(SurfaceOp::Quit).await?;
                Ok(json!({"ok":true}))
            }
            "app:confirm-quit" => {
                if self.confirm(&json!({"title":"Quit Tezbar?","message":"Quit Tezbar?","detail":"Background tasks and active terminal sessions will stop.","buttons":["Cancel","Quit"]})).await?{
                    self.ctx.host.surface(SurfaceOp::Quit).await?;
                }
                Ok(Value::Null)
            }
            "core:platform:confirm" => {
                Ok(json!({"response":if self.confirm(&value).await?{1}else{0}}))
            }
            "core:platform:clear-webview-cache" => {
                self.ctx
                    .host
                    .surface(SurfaceOp::ClearLegacyWebviewCache)
                    .await?;
                Ok(json!({ "ok": true }))
            }
            "core:platform:trim-memory" => {
                crate::trim_os_process_memory();
                Ok(json!({ "ok": true }))
            }
            "core:platform:selected-text" => {
                if let Some(result) = self
                    .ctx
                    .host
                    .desktop_effect(crate::host::DesktopEffect::SelectedText)
                {
                    return result;
                }
                selected_text().await.map(Value::String)
            }
            "core:platform:ocr-images" => {
                let images = value.as_array().ok_or("Invalid OCR images")?.clone();
                super::ai::ocr_images_local(&self.ctx.paths, &images)
                    .await
                    .map(Value::String)
            }
            "core:platform:screenshot" => {
                let bytes = self.ctx.host.capture_active_display().await?;
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
                self.ctx.host.surface(SurfaceOp::HideForPaste).await?;
                tokio::time::sleep(Duration::from_millis(80)).await;
                self.ctx
                    .host
                    .surface(SurfaceOp::PressKey {
                        key: "v".into(),
                        modifiers: vec![if cfg!(target_os = "macos") {
                            "cmd".into()
                        } else {
                            "ctrl".into()
                        }],
                    })
                    .await?;
                Ok(json!({"ok":true,"message":"Pasted text"}))
            }
            "core:platform:clipboard-write" => {
                if !value.is_object()
                    || ["text", "html", "file"]
                        .iter()
                        .any(|k| value.get(*k).is_some_and(|v| !v.is_string()))
                {
                    return Err("Invalid clipboard data".into());
                }
                let host = self.ctx.host.clone();
                tokio::task::spawn_blocking(move || {
                    if let Some(file) = value["file"].as_str() {
                        let image = image::open(file).map_err(|e| e.to_string())?.into_rgba8();
                        if let Some(result) =
                            host.desktop_effect(crate::host::DesktopEffect::WriteImage {
                                bytes: image.as_raw().clone(),
                                width: image.width(),
                                height: image.height(),
                            })
                        {
                            return result;
                        }
                        arboard::Clipboard::new()
                            .map_err(|e| e.to_string())?
                            .set_image(arboard::ImageData {
                                width: image.width() as usize,
                                height: image.height() as usize,
                                bytes: std::borrow::Cow::Owned(image.into_raw()),
                            })
                            .map_err(|e| e.to_string())?;
                    } else if let Some(html) = value["html"].as_str() {
                        if let Some(result) =
                            host.desktop_effect(crate::host::DesktopEffect::WriteHtml {
                                html: html.into(),
                                text: value["text"].as_str().map(str::to_owned),
                            })
                        {
                            return result;
                        }
                        arboard::Clipboard::new()
                            .map_err(|e| e.to_string())?
                            .set_html(html, value["text"].as_str())
                            .map_err(|e| e.to_string())?;
                    } else {
                        let text = value["text"].as_str().ok_or("Clipboard text required")?;
                        if let Some(result) =
                            host.desktop_effect(crate::host::DesktopEffect::WriteText(text.into()))
                        {
                            return result;
                        }
                        arboard::Clipboard::new()
                            .map_err(|e| e.to_string())?
                            .set_text(value["text"].as_str().unwrap_or(""))
                            .map_err(|e| e.to_string())?;
                    }
                    Ok(json!({"ok":true}))
                })
                .await
                .map_err(|e| e.to_string())?
            }
            "open-external-url" => {
                let url = reqwest::Url::parse(text).map_err(|e| e.to_string())?;
                let host = url.host_str().unwrap_or("");
                if url.scheme() != "https"
                    || !(host == "github.com" || host.ends_with(".github.com"))
                {
                    return Err("Only GitHub HTTPS URLs are supported here".into());
                }
                if text.trim().is_empty() || text.contains('\0') {
                    return Err("An open target is required".into());
                }
                if let Some(result) = self
                    .ctx
                    .host
                    .desktop_effect(crate::host::DesktopEffect::Open(text.into()))
                {
                    return result;
                }
                open_target(text).await
            }
            "shell:open" | "core:platform:open-file" | "core:platform:open-url" => {
                if text.trim().is_empty() || text.contains('\0') {
                    return Err("An open target is required".into());
                }
                if let Some(result) = self
                    .ctx
                    .host
                    .desktop_effect(crate::host::DesktopEffect::Open(text.into()))
                {
                    return result;
                }
                open_target(text).await
            }
            "core:platform:open-app" => {
                let target = application_target(&value)?;
                if let Some(r) = self
                    .ctx
                    .host
                    .desktop_effect(crate::host::DesktopEffect::OpenApp(target.into()))
                {
                    return r;
                }
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
                Ok(json!({"ok":true}))
            }
            "core:platform:open-with-app" => {
                let path = value["path"].as_str().ok_or("File path required")?;
                let app = value["appPath"]
                    .as_str()
                    .ok_or("Application path required")?;
                if path.trim().is_empty()
                    || app.trim().is_empty()
                    || path.contains('\0')
                    || app.contains('\0')
                {
                    return Err("Invalid application/file target".into());
                }
                if let Some(r) =
                    self.ctx
                        .host
                        .desktop_effect(crate::host::DesktopEffect::OpenWith {
                            path: path.into(),
                            app: app.into(),
                        })
                {
                    return r;
                }
                #[cfg(target_os = "macos")]
                {
                    let app_path = std::path::Path::new(app);
                    let cli = app_path.join("Contents/Resources/app/bin/code");
                    let spawned_cli = if cli.is_file() {
                        tokio::process::Command::new(&cli)
                            .arg(path)
                            .spawn()
                            .is_ok()
                    } else {
                        false
                    };
                    if !spawned_cli {
                        output("/usr/bin/open", &["-a", app, path], 10).await?;
                    }
                }
                // ShellExecute resolves .lnk shortcuts and registered apps like Start-Process did.
                #[cfg(target_os = "windows")]
                {
                    let (app, path) = (app.to_owned(), path.to_owned());
                    tokio::task::spawn_blocking(move || crate::windows_shell::open_with(&app, &path))
                        .await
                        .map_err(|e| e.to_string())??;
                }
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                {
                    tokio::process::Command::new(app)
                        .arg(path)
                        .spawn()
                        .map_err(|e| e.to_string())?;
                }
                Ok(json!({"ok":true}))
            }
            "core:platform:reveal-file" => {
                if text.is_empty() || text.contains('\0') {
                    return Err("Invalid reveal path".into());
                }
                if let Some(result) = self
                    .ctx
                    .host
                    .desktop_effect(crate::host::DesktopEffect::Reveal(text.into()))
                {
                    return result;
                }

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

    async fn confirm(&self, options: &Value) -> Result<bool, String> {
        match self
            .ctx
            .host
            .dialog(DialogRequest::Confirm(options.clone()))
            .await?
        {
            DialogReply::Confirm(value) => Ok(value),
            _ => Err("Invalid confirmation dialog reply".into()),
        }
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
    // UI Automation text selection of the focused control, in-process.
    #[cfg(target_os = "windows")]
    let selected = tokio::task::spawn_blocking(crate::windows_shell::selected_text)
        .await
        .map_err(|e| e.to_string())??;
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

#[cfg(test)]
#[path = "platform_tests.rs"]
mod tests;
