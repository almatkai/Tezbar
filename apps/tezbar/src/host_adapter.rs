//! Core-to-GPUI adapter. OS objects are created on, and dispatched only to, the GUI thread.
use crate::events::{CoreEvent, Event, Relay};
use std::sync::Arc;
use tezbar_core::host::*;
use tezbar_host::desktop::NativeDesktop;
pub struct GpuiHost {
    pub desktop: NativeDesktop,
    pub relay: Relay,
    registry: PreviewShortcuts,
    production: bool,
}
impl GpuiHost {
    #[cfg(test)]
    pub(crate) fn fixture(relay: Relay) -> Self {
        let desktop = NativeDesktop::new(Arc::new(|_| {
            Err("Desktop operations forbidden in fixture".into())
        }));
        Self {
            registry: PreviewShortcuts(desktop.clone(), false),
            production: false,
            desktop,
            relay,
        }
    }

    pub fn new(relay: Relay) -> Self {
        Self::with_desktop_integration(relay, false)
    }
    pub fn with_desktop_integration(relay: Relay, allow: bool) -> Self {
        #[cfg(target_os = "macos")]
        assert!(
            objc2::MainThreadMarker::new().is_some(),
            "GpuiHost requires the actual GUI main thread"
        );
        let queue = relay.clone();
        let desktop = NativeDesktop::new(Arc::new(move |op| queue.send(Event::Main(op))));
        Self {
            registry: PreviewShortcuts(desktop.clone(), allow),
            production: false,
            desktop,
            relay,
        }
    }
    pub(crate) fn production(relay: Relay) -> Self {
        let mut host = Self::with_desktop_integration(relay, true);
        host.production = true;
        host
    }
    /// QR/image surfaces call this explicit effect after the user chooses Save.
    pub fn save_png(
        &self,
        directory: std::path::PathBuf,
        name: String,
        png: Vec<u8>,
    ) -> HostFuture<Option<std::path::PathBuf>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let sent = if png.len() > 16 * 1024 * 1024 || !png.starts_with(b"\x89PNG\r\n\x1a\n") {
            Err("A PNG of at most 16 MiB is required".into())
        } else {
            self.relay.send(Event::SavePng {
                directory,
                name,
                png,
                reply: tx,
            })
        };
        Box::pin(async move {
            sent?;
            rx.await
                .map_err(|_| "Save dialog owner closed".to_string())?
        })
    }
}
impl Host for GpuiHost {
    fn extension_control(
        &self,
        session: String,
        source: u64,
        control: ExtensionControl,
    ) -> HostFuture<()> {
        let (kind, message) = match control {
            ExtensionControl::Close => ("close", None),
            ExtensionControl::Preferences => ("preferences", None),
            ExtensionControl::ClearSearch => ("clear-search", None),
            ExtensionControl::Hud(message) => ("hud", Some(message)),
        };
        let result=self.relay.send_core(CoreEvent::Other{channel:"extension:ui".into(),payload:serde_json::json!({"sessionId":session,"source":source,"kind":kind,"message":message})});
        Box::pin(async { result })
    }

    fn emit(&self, channel: &str, payload: serde_json::Value) {
        if let Err(error) = self.relay.send_core(CoreEvent::decode(channel, payload)) {
            eprintln!("Core event delivery failed for {channel}: {error}");
        }
    }
    fn surface(&self, op: SurfaceOp) -> HostFuture<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let sent = self.relay.send(Event::Surface(op, tx));
        Box::pin(async move {
            sent?;
            rx.await.map_err(|_| "Surface owner closed".to_string())?
        })
    }
    fn dialog(&self, req: DialogRequest) -> HostFuture<DialogReply> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let sent = self.relay.send(Event::Dialog(req, tx));
        Box::pin(async move {
            sent?;
            rx.await.map_err(|_| "Dialog owner closed".to_string())?
        })
    }
    fn shortcuts(&self) -> &dyn ShortcutRegistry {
        if self.production {
            &self.desktop
        } else {
            &self.registry
        }
    }
    fn status_items(&self) -> &dyn StatusItems {
        &self.desktop
    }
    fn desktop_services(&self) -> bool {
        self.production
    }
    fn capture_active_display(&self) -> HostFuture<Vec<u8>> {
        let relay = self.relay.clone();
        Box::pin(async move {
            let (tx, rx) = tokio::sync::oneshot::channel();
            relay.send(Event::Capture(tx))?;
            // The lease itself crosses the oneshot: cancellation after send but before recv
            // polling still drops the lease and restores/suppression exactly once.
            let lease = rx.await.map_err(|_| "Capture owner closed")?;
            // Move the lease into the existing blocking capture guard. Cancelling the await
            // must not restore Tezbar before the actual capture operation has finished.
            tezbar_host::input::capture_protected(true, move || {
                let mut lease = Some(lease);
                let restore: Box<dyn FnMut() -> Result<(), String> + Send> = Box::new(move || {
                    drop(lease.take());
                    Ok(())
                });
                Ok(vec![tezbar_host::input::CaptureGuard::new(restore)])
            })
            .await
        })
    }
    fn notify(&self, spec: NotificationSpec) -> HostFuture<()> {
        Box::pin(async move {
            match spec {
                NotificationSpec::Show { id, title, body } => {
                    tezbar_host::notifications::show(&id, &title, &body).await
                }
                NotificationSpec::Remove { id } => tezbar_host::notifications::remove(&id).await,
                NotificationSpec::Timer { name, path } => {
                    #[cfg(target_os = "macos")]
                    tezbar_host::timer_notifications::deliver(&name, &path);
                    #[cfg(not(target_os = "macos"))]
                    tezbar_host::notifications::show(&path, &name, "Timer completed").await?;
                    Ok(())
                }
            }
        })
    }
}
/// Owns the primary tray separately from extension trays. Dropping it removes the native item.
pub struct PrimaryTray {
    _tray: tray_icon::TrayIcon,
    pub shortcut: Option<Shortcut>,
}
impl PrimaryTray {
    pub fn new(desktop: &NativeDesktop, resources: &std::path::Path) -> Result<Self, String> {
        Self::build(desktop, resources, false)
    }
    pub(crate) fn production(
        desktop: &NativeDesktop,
        resources: &std::path::Path,
    ) -> Result<Self, String> {
        Self::build(desktop, resources, true)
    }
    fn build(
        desktop: &NativeDesktop,
        resources: &std::path::Path,
        production: bool,
    ) -> Result<Self, String> {
        let menu = muda::Menu::new();
        for (id, title) in [
            ("preview-show", "Show Tezbar"),
            ("preview-settings", "Settings"),
            ("preview-quit", "Quit Tezbar"),
        ] {
            menu.append(&muda::MenuItem::with_id(id, title, true, None))
                .map_err(|e| e.to_string())?;
        }
        let icon_path = resources.join("trayIconTemplate@2x.png");
        let bytes = std::fs::read(&icon_path)
            .map_err(|e| format!("Tray template {}: {e}", icon_path.display()))?;
        let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?
            .into_rgba8();
        let (w, h) = image.dimensions();
        let icon = tray_icon::Icon::from_rgba(image.into_raw(), w, h).map_err(|e| e.to_string())?;
        let tray = tray_icon::TrayIconBuilder::new()
            .with_id(if production {
                "tezbar-primary"
            } else {
                "tezbar-preview-primary"
            })
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_icon(icon)
            .with_icon_as_template(true)
            .with_tooltip(if production {
                "Tezbar"
            } else {
                "Tezbar native preview"
            })
            .build()
            .map_err(|e| e.to_string())?;
        let shortcut = if production {
            None
        } else {
            desktop
                .parse("Alt+F19")
                .and_then(|shortcut| {
                    desktop.register(&shortcut)?;
                    Ok(shortcut)
                })
                .map_err(|e| log::warn!("Preview hotkey unavailable; tray retained: {e}"))
                .ok()
        };
        Ok(Self {
            _tray: tray,
            shortcut,
        })
    }
}

struct PreviewShortcuts(NativeDesktop, bool);
fn preview_shortcut_allowed(accelerator: &str) -> bool {
    let Ok(key) = tezbar_host::desktop::parse_shortcut(accelerator) else {
        return false;
    };
    ["Alt+Space", "Control+Space"].into_iter().all(|blocked| {
        tezbar_host::desktop::parse_shortcut(blocked).is_ok_and(|blocked| blocked.id != key.id)
    })
}
impl ShortcutRegistry for PreviewShortcuts {
    fn available(&self) -> bool {
        self.1
    }
    fn parse(&self, raw: &str) -> Result<Shortcut, String> {
        if !preview_shortcut_allowed(raw) {
            return Err("Production launcher shortcuts are excluded from isolated preview".into());
        }
        self.0.parse(raw)
    }
    fn register(&self, shortcut: &Shortcut) -> Result<(), String> {
        if !self.1 {
            return Err("Preview desktop integration requires --allow-desktop-integration".into());
        }
        if !preview_shortcut_allowed(&shortcut.accelerator) {
            return Err("Production launcher shortcut excluded from preview".into());
        }
        self.0.register(shortcut)
    }
    fn unregister(&self, shortcut: &Shortcut) -> Result<(), String> {
        self.0.unregister(shortcut)
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn preview_cannot_register_production_launcher_accelerators() {
        assert!(!super::preview_shortcut_allowed("Alt+Space"));
        assert!(!super::preview_shortcut_allowed("Control+Space"));
        assert!(!super::preview_shortcut_allowed("Option+Space"));
        assert!(!super::preview_shortcut_allowed("Alt + Space"));
        assert!(!super::preview_shortcut_allowed("Space+Alt"));
        #[cfg(target_os = "windows")]
        assert!(!super::preview_shortcut_allowed("CmdOrCtrl+Space"));
        assert!(super::preview_shortcut_allowed("Alt+F19"));
    }
}
