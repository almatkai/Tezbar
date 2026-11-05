//! Temporary adapter; all GUI objects stay in the legacy executable until M11.
use std::{collections::HashMap, str::FromStr, sync::Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut as NativeShortcut};
use tezbar_core::host::*;

pub struct TauriHost {
    app: AppHandle,
    trays: Mutex<HashMap<String, tauri::tray::TrayIcon>>,
}
impl TauriHost {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            trays: Mutex::new(HashMap::new()),
        }
    }
}
impl Host for TauriHost {
    fn emit(&self, channel: &str, value: serde_json::Value) {
        if let Err(error) = self.app.emit(channel, value) {
            log::warn!("Could not emit {channel}: {error}");
        }
    }
    fn surface(&self, op: SurfaceOp) -> HostFuture<()> {
        let app = self.app.clone();
        Box::pin(async move {
            match op {
                SurfaceOp::ShowLauncher
                | SurfaceOp::HideLauncher
                | SurfaceOp::CloseCurrent
                | SurfaceOp::SnapDragStart
                | SurfaceOp::SnapDragEnd => {
                    let window = app
                        .get_webview_window("main")
                        .ok_or("Main window is unavailable")?;
                    match op {
                        SurfaceOp::ShowLauncher => crate::show_window(window)?,
                        SurfaceOp::HideLauncher => crate::hide_window(window)?,
                        SurfaceOp::CloseCurrent => crate::close_current_window(window)?,
                        SurfaceOp::SnapDragStart => {
                            crate::start_window_snap_drag(window, app.clone(), app.state())?
                        }
                        _ => crate::end_window_snap_drag(window, app.clone(), app.state()),
                    }
                }
                SurfaceOp::SetAppVisible(visible) => {
                    crate::set_backend_app_visibility(&app, visible)
                }
                SurfaceOp::SuppressBlurHide(value) => {
                    *app.state::<crate::WindowBehaviorState>()
                        .suppress_blur_hide
                        .lock()
                        .map_err(|e| e.to_string())? = value
                }
                SurfaceOp::OpenSettings => {
                    tokio::task::spawn_blocking(move || crate::open_settings_window(app))
                        .await
                        .map_err(|e| e.to_string())??
                }
                SurfaceOp::HideForPaste => {
                    crate::set_backend_app_visibility(&app, false);
                    #[cfg(target_os = "macos")]
                    app.hide().map_err(|e| e.to_string())?;
                }
                SurfaceOp::ShowAfterAuth => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = crate::show_main_window_native(&window);
                        let _ = window.set_focus();
                        let _ = window.emit("window-shown", serde_json::json!({"resetUi":false}));
                    }
                }
                SurfaceOp::ClearLegacyWebviewCache => {
                    for (_, window) in app.webview_windows() {
                        window
                            .clear_all_browsing_data()
                            .map_err(|e| e.to_string())?;
                    }
                }
                SurfaceOp::Quit => crate::quit_app_now(&app),
                SurfaceOp::PressKey { key, modifiers } => {
                    crate::native_input::press_key(key, modifiers)?
                }
            }
            Ok(())
        })
    }
    fn dialog(&self, request: DialogRequest) -> HostFuture<DialogReply> {
        let app = self.app.clone();
        Box::pin(async move {
            match request {
                DialogRequest::ChooseFolder => {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    app.dialog()
                        .file()
                        .set_title("Choose a Knowledge folder")
                        .pick_folder(move |path| {
                            let _ = tx.send(path.and_then(|p| p.into_path().ok()));
                        });
                    Ok(DialogReply::Path(rx.await.map_err(|e| e.to_string())?))
                }
                DialogRequest::Confirm(options) => {
                    let title = options["title"].as_str().unwrap_or("Confirm");
                    let message = options["message"].as_str().unwrap_or(title);
                    let detail = options["detail"].as_str().unwrap_or("");
                    let cancel = options["buttons"][0]
                        .as_str()
                        .unwrap_or("Cancel")
                        .to_string();
                    let accept = options["buttons"][1].as_str().unwrap_or("OK").to_string();
                    let (tx, rx) = tokio::sync::oneshot::channel();
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
                            let _ = tx.send(accepted);
                        });
                    Ok(DialogReply::Confirm(
                        rx.await
                            .map_err(|_| "Confirmation dialog closed".to_string())?,
                    ))
                }
            }
        })
    }
    fn shortcuts(&self) -> &dyn ShortcutRegistry {
        self
    }
    fn status_items(&self) -> &dyn StatusItems {
        self
    }
    fn desktop_services(&self) -> bool {
        true
    }
    fn capture_active_display(&self) -> HostFuture<Vec<u8>> {
        let app = self.app.clone();
        Box::pin(async move {
            crate::native_input::active_capture(
                app.get_webview_window("main")
                    .ok_or("Main window is unavailable")?,
            )
            .await
        })
    }
    fn notify(&self, spec: NotificationSpec) -> HostFuture<()> {
        let app = self.app.clone();
        Box::pin(async move {
            match spec {
                NotificationSpec::Timer { name, path } => {
                    #[cfg(target_os = "macos")]
                    {
                        crate::timer_notifications::setup(&app);
                        crate::timer_notifications::deliver(&name, &path);
                    }
                    #[cfg(not(target_os = "macos"))]
                    let _ = (app, name, path);
                    Ok(())
                }
                NotificationSpec::Show { id, title, body } => {
                    crate::process_notifications::show(&id, &title, &body).await
                }
                NotificationSpec::Remove { id } => crate::process_notifications::remove(&id).await,
            }
        })
    }
}
impl ShortcutRegistry for TauriHost {
    fn available(&self) -> bool {
        true
    }
    fn parse(&self, raw: &str) -> Result<Shortcut, String> {
        tezbar_host::desktop::parse_shortcut(raw)
    }
    fn register(&self, shortcut: &Shortcut) -> Result<(), String> {
        self.app
            .global_shortcut()
            .register(NativeShortcut::from_str(&shortcut.accelerator).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
    fn unregister(&self, shortcut: &Shortcut) -> Result<(), String> {
        self.app
            .global_shortcut()
            .unregister(NativeShortcut::from_str(&shortcut.accelerator).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
}
impl StatusItems for TauriHost {
    fn set(&self, scoped: &str, spec: StatusItem, callback: StatusCallback) -> Result<(), String> {
        use tauri::{
            menu::{CheckMenuItem, Menu, MenuItem},
            tray::TrayIconBuilder,
        };
        let app = &self.app;
        let menu = Menu::new(app).map_err(|e| e.to_string())?;
        let mut actions = HashMap::new();
        for item in spec.items {
            let id = item["id"].as_str().ok_or("Tray action id required")?;
            let os_id = format!("{scoped}-{id}");
            actions.insert(os_id.clone(), id.to_string());
            let title = item["title"].as_str().ok_or("Tray title required")?;
            let enabled = item["enabled"].as_bool().unwrap_or(true);
            if let Some(checked) = item["checked"].as_bool() {
                menu.append(
                    &CheckMenuItem::with_id(app, os_id, title, enabled, checked, None::<&str>)
                        .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
            } else {
                menu.append(
                    &MenuItem::with_id(app, os_id, title, enabled, None::<&str>)
                        .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
            }
        }
        let mut builder = TrayIconBuilder::with_id(scoped)
            .menu(&menu)
            .title(spec.title)
            .tooltip(spec.tooltip);
        if let Some((bytes, w, h)) = spec.icon {
            builder = builder.icon(tauri::image::Image::new_owned(bytes, w, h));
        }
        let tray = builder
            .on_menu_event(move |_, event| {
                if let Some(action) = actions.get(event.id().as_ref()) {
                    callback(action.clone());
                }
            })
            .build(app)
            .map_err(|e| e.to_string())?;
        self.trays
            .lock()
            .map_err(|e| e.to_string())?
            .insert(scoped.into(), tray);
        Ok(())
    }
    fn remove(&self, scoped: &str) -> Result<(), String> {
        self.trays.lock().map_err(|e| e.to_string())?.remove(scoped);
        Ok(())
    }
}
