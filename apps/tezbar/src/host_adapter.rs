//! Core-to-GPUI adapter. OS objects are created on, and dispatched only to, the GUI thread.
use crate::events::{CoreEvent, Event, Relay};
use std::sync::{Arc, OnceLock, Weak};
use tezbar_core::host::*;
use tezbar_host::desktop::NativeDesktop;
pub struct GpuiHost {
    pub desktop: NativeDesktop,
    pub relay: Relay,
    registry: PreviewShortcuts,
    launcher_hidden: std::sync::atomic::AtomicBool,
    production: bool,
    /// Preview opt-in for clipboard/app/file observers (production always has them).
    services: bool,
    status_core: OnceLock<Weak<tezbar_core::NativeCore>>,
}
impl GpuiHost {
    #[cfg(test)]
    pub(crate) fn fixture(relay: Relay) -> Self {
        let desktop = NativeDesktop::new(Arc::new(|_| {
            Err("Desktop operations forbidden in fixture".into())
        }));
        Self {
            registry: PreviewShortcuts(desktop.clone(), false),
            launcher_hidden: true.into(),
            production: false,
            services: false,
            status_core: OnceLock::new(),
            desktop,
            relay,
        }
    }

    pub(crate) fn set_launcher_hidden(&self, hidden: bool) {
        self.launcher_hidden.store(hidden, std::sync::atomic::Ordering::Release);
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
            launcher_hidden: true.into(),
            production: false,
            services: false,
            status_core: OnceLock::new(),
            desktop,
            relay,
        }
    }
    /// Explicit preview opt-in (see `Preview::allow_desktop_services`).
    pub fn with_desktop_services(mut self, allow: bool) -> Self {
        self.services = allow;
        self
    }
    pub(crate) fn production(relay: Relay) -> Self {
        let mut host = Self::with_desktop_integration(relay, true);
        host.production = true;
        host
    }
    /// Non-cyclic capability: NativeCore owns this host, never vice versa.
    pub(crate) fn bind_status_assets(&self, core: &Arc<tezbar_core::NativeCore>) {
        assert!(
            self.status_core.set(Arc::downgrade(core)).is_ok(),
            "Status assets already bound"
        );
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
    fn launcher_hidden(&self) -> bool {
        self.production && self.launcher_hidden.load(std::sync::atomic::Ordering::Acquire)
    }
    fn sdk_status_items(&self) -> bool {
        true
    }
    fn resolve_sdk_status_icon(
        &self,
        extension: String,
        icon: serde_json::Value,
    ) -> HostFuture<StatusIcon> {
        crate::asset_transport::resolve_status_icon(
            self.status_core.get().cloned().unwrap_or_default(),
            extension,
            icon,
        )
    }
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
        self.production || self.services
    }
    fn native_terminal(&self) -> bool {
        // Preview is a native desktop application too, with its own CorePaths.
        // Only this local PTY capability is shared with production.
        true
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
    fn capture_targets(&self, target: String) -> HostFuture<Vec<Vec<u8>>> {
        let relay = self.relay.clone();
        Box::pin(async move {
            #[cfg(target_os = "windows")]
            {
                use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
                struct Cancel(Arc<AtomicBool>);
                impl Drop for Cancel { fn drop(&mut self) { self.0.store(true, Ordering::Release); } }
                let cancel = Cancel(Arc::new(AtomicBool::new(false)));
                let (tx, rx) = tokio::sync::oneshot::channel();
                relay.send(Event::Capture(tx))?;
                let lease = rx.await.map_err(|_| "Capture owner closed")?;
                tezbar_host::input::capture_targets_protected(target, cancel.0.clone(), move || {
                    let mut lease = Some(lease);
                    let restore: Box<dyn FnMut() -> Result<(), String> + Send> = Box::new(move || { drop(lease.take()); Ok(()) });
                    Ok(vec![tezbar_host::input::CaptureGuard::new(restore)])
                }).await
            }
            #[cfg(not(target_os = "windows"))]
            { let _ = (relay, target); Err("Negotiated capture unavailable on this platform".into()) }
        })
    }
    fn notify(&self, spec: NotificationSpec) -> HostFuture<()> {
        Box::pin(async move {
            match spec {
                NotificationSpec::ExtensionFeedback(body) => {
                    tezbar_host::notifications::show_if_authorized("tezbar-extension-feedback", "Tezbar", &body).await
                }
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
pub(crate) const PRIMARY_TRAY_ID: &str = "tezbar-primary";
pub(crate) const PREVIEW_TRAY_ID: &str = "tezbar-preview-primary";
/// Owns the primary tray separately from extension trays. Dropping it removes the native item.
pub struct PrimaryTray {
    _tray: tray_icon::TrayIcon,
    pub shortcut: Option<Shortcut>,
}
impl PrimaryTray {
    pub fn new(desktop: &NativeDesktop, resources: &std::path::Path) -> Result<Self, String> {
        Self::build(desktop, resources, false, None)
    }
    /// Only this explicit launch option may claim the user's launcher key in preview.
    /// Core's general preview shortcut registry remains restricted.
    pub(crate) fn with_shortcut(
        desktop: &NativeDesktop,
        resources: &std::path::Path,
        accelerator: &str,
    ) -> Result<Self, String> {
        Self::build(desktop, resources, false, Some(accelerator))
    }
    pub(crate) fn production(
        desktop: &NativeDesktop,
        resources: &std::path::Path,
    ) -> Result<Self, String> {
        Self::build(desktop, resources, true, None)
    }
    fn build(
        desktop: &NativeDesktop,
        resources: &std::path::Path,
        launcher_hidden: std::sync::atomic::AtomicBool,
    production: bool,
        requested_shortcut: Option<&str>,
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
                PRIMARY_TRAY_ID
            } else {
                PREVIEW_TRAY_ID
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
            let result = desktop
                .parse(requested_shortcut.unwrap_or("Alt+F19"))
                .and_then(|shortcut| {
                    desktop.register(&shortcut)?;
                    log::info!(
                        "Preview launcher hotkey registered: {}",
                        shortcut.accelerator
                    );
                    Ok(shortcut)
                });
            if requested_shortcut.is_some() {
                Some(result?)
            } else {
                result
                    .map_err(|e| log::warn!("Preview hotkey unavailable; tray retained: {e}"))
                    .ok()
            }
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
    use super::*;
    #[tokio::test]
    async fn status_icons_gpui_binding_is_weak_and_unbound_capability_is_explicit() {
        let (relay, _receiver) = crate::events::Relay::new();
        let host = Arc::new(GpuiHost::fixture(relay));
        assert!(
            host.resolve_sdk_status_icon("raycast.fixture".into(), serde_json::json!("Gear"))
                .await
                .is_err()
        );
        let dir = tempfile::tempdir().unwrap();
        let ctx = tezbar_core::common::CoreContext::new(
            tezbar_core::common::CorePaths {
                data_dir: dir.path().join("data"),
                config_dir: dir.path().join("config"),
                resource_dir: dir.path().join("resources"),
                temp_dir: dir.path().join("tmp"),
                home_dir: dir.path().join("home"),
            },
            Some(host.clone()),
        );
        let core = tezbar_core::NativeCore::new(ctx).unwrap();
        let owners = Arc::strong_count(&core);
        host.bind_status_assets(&core);
        assert_eq!(
            Arc::strong_count(&core),
            owners,
            "host created an ownership cycle"
        );
        let weak = Arc::downgrade(&core);
        drop(core);
        assert!(weak.upgrade().is_none());
        assert!(
            host.resolve_sdk_status_icon("raycast.fixture".into(), serde_json::json!("Gear"))
                .await
                .is_err()
        );
    }
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
