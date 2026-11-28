//! Typed desktop seam. Adapters own GUI objects; the core owns ordering and policy.
use serde_json::Value;
use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
};
pub type HostFuture<T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'static>>;
#[derive(Clone, Debug, PartialEq)]
pub enum SurfaceOp {
    ShowLauncher,
    HideLauncher,
    CloseCurrent,
    SnapDragStart,
    SnapDragEnd,
    SetAppVisible(bool),
    SuppressBlurHide(bool),
    OpenSettings,
    HideForPaste,
    ShowAfterAuth,
    ClearLegacyWebviewCache,
    Quit,
    PressKey { key: String, modifiers: Vec<String> },
}
#[derive(Clone, Debug, PartialEq)]
pub enum DialogRequest {
    Confirm(Value),
    ChooseFolder,
    QuickLink(Value),
}
#[derive(Clone, Debug, PartialEq)]
pub enum DialogReply {
    Confirm(bool),
    QuickLink(Option<Value>),
    Path(Option<PathBuf>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub id: u32,
    pub accelerator: String,
}
impl Shortcut {
    pub fn id(&self) -> u32 {
        self.id
    }
}
impl std::fmt::Display for Shortcut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.accelerator.fmt(f)
    }
}
pub trait ShortcutRegistry: Send + Sync {
    fn available(&self) -> bool;
    fn parse(&self, accelerator: &str) -> Result<Shortcut, String>;
    fn register(&self, shortcut: &Shortcut) -> Result<(), String>;
    fn unregister(&self, shortcut: &Shortcut) -> Result<(), String>;
}
/// Straight RGBA pixels at the desktop status boundary (not GPUI's BGRA).
pub type StatusIcon = (Vec<u8>, u32, u32);
pub type StatusIconResolver = Arc<dyn Fn(String, Value) -> HostFuture<StatusIcon> + Send + Sync>;
#[derive(Clone, Debug)]
pub struct StatusItem {
    pub title: String,
    pub tooltip: String,
    pub items: Vec<Value>,
    pub icon: Option<StatusIcon>,
}
pub type StatusCallback = Arc<dyn Fn(String) + Send + Sync>;
pub trait StatusItems: Send + Sync {
    fn set(
        &self,
        scoped_id: &str,
        spec: StatusItem,
        callback: StatusCallback,
    ) -> Result<(), String>;
    fn remove(&self, scoped_id: &str) -> Result<(), String>;
}
#[derive(Clone, Debug)]
pub enum NotificationSpec {
    ExtensionFeedback(String),
    Timer {
        name: String,
        path: String,
    },
    Show {
        id: String,
        title: String,
        body: String,
    },
    Remove {
        id: String,
    },
}
/// Finite desktop interception at the execution boundary. None preserves the
/// production native routine; headless hosts always return Some, including denial.
#[derive(Clone)]
pub enum DesktopEffect {
    ReadClipboard,
    ReadClipboardContent,
    ClipboardSequence,
    WriteText(String),
    WriteHtml {
        html: String,
        text: Option<String>,
    },
    WriteImage {
        bytes: Vec<u8>,
        width: u32,
        height: u32,
    },
    WriteFiles(Vec<PathBuf>),
    SelectedText,
    SelectedFiles,
    ReadImage,
    OpenApp(String),
    OpenWith {
        path: String,
        app: String,
    },
    Open(String),
    Reveal(String),
}
impl std::fmt::Debug for DesktopEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DesktopEffect(<redacted>)")
    }
}
#[derive(Clone)]
pub enum ExtensionControl {
    Close,
    Preferences,
    ClearSearch,
    Hud(String),
}
impl std::fmt::Debug for ExtensionControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExtensionControl(<redacted>)")
    }
}
pub trait Host: Send + Sync + 'static {
    /// SDK status sync requires the native session-aware status host. Legacy
    /// hosts preserve their existing in-list behavior until their own cutover.
    fn launcher_hidden(&self) -> bool { false }
    fn sdk_status_items(&self) -> bool {
        false
    }
    /// Native-only capability. Core never decodes SDK paths or creates an image service.
    fn resolve_sdk_status_icon(&self, _extension: String, _icon: Value) -> HostFuture<StatusIcon> {
        Box::pin(async { Err("Status icon resolver is unavailable".into()) })
    }
    fn extension_control(
        &self,
        _session: String,
        _source: u64,
        _control: ExtensionControl,
    ) -> HostFuture<()> {
        // Legacy Tauri controls were no-ops. Only an explicit fenced native
        // adapter may turn them into window or Settings effects.
        Box::pin(async { Ok(()) })
    }
    fn desktop_effect(&self, _effect: DesktopEffect) -> Option<Result<Value, String>> {
        None
    }
    fn emit(&self, channel: &str, payload: Value);
    fn surface(&self, op: SurfaceOp) -> HostFuture<()>;
    fn dialog(&self, request: DialogRequest) -> HostFuture<DialogReply>;
    fn shortcuts(&self) -> &dyn ShortcutRegistry;
    fn status_items(&self) -> &dyn StatusItems;
    fn capture_active_display(&self) -> HostFuture<Vec<u8>>;
    fn capture_targets(&self, _target: String) -> HostFuture<Vec<Vec<u8>>> {
        Box::pin(async { Err("Negotiated capture unavailable on this platform".into()) })
    }
    fn desktop_services(&self) -> bool;
    /// Explicit first-party local PTY capability. It does not authorize clipboard,
    /// global shortcuts, authentication, tray integration or background services.
    fn native_terminal(&self) -> bool {
        self.desktop_services()
    }
    fn notify(&self, _spec: NotificationSpec) -> HostFuture<()> {
        Box::pin(async { Ok(()) })
    }
}
/// Scripted headless adapter. Desktop interaction is denied unless explicitly enabled.
#[derive(Default)]
pub struct TestHost {
    pub desktop: bool,
    pub desktop_effects: Mutex<Vec<DesktopEffect>>,
    pub clipboard_text: Mutex<String>,
    pub allow_interaction: bool,
    pub calls: Mutex<Vec<String>>,
    pub events: Mutex<Vec<(String, Value)>>,
    pub dialogs: Mutex<VecDeque<Result<DialogReply, String>>>,
    pub captures: Mutex<VecDeque<Result<Vec<u8>, String>>>,
    pub shortcut_results: Mutex<VecDeque<Result<(), String>>>,
    pub status: Mutex<HashMap<String, StatusCallback>>,
    pub status_specs: Mutex<HashMap<String, StatusItem>>,
    /// Explicit fake-OS fixtures may bind the production bounded asset resolver.
    pub status_icon_resolver: Mutex<Option<StatusIconResolver>>,
}
impl TestHost {
    fn record(&self, call: String) -> Result<(), String> {
        self.calls.lock().unwrap().push(call);
        if self.allow_interaction {
            Ok(())
        } else {
            Err("This operation requires a desktop session".into())
        }
    }
}
impl Host for TestHost {
    fn sdk_status_items(&self) -> bool {
        true
    }
    fn resolve_sdk_status_icon(&self, extension: String, icon: Value) -> HostFuture<StatusIcon> {
        if let Some(resolve) = self.status_icon_resolver.lock().unwrap().clone() {
            resolve(extension, icon)
        } else {
            Box::pin(async { Err("Fixture status icon resolver is unbound".into()) })
        }
    }
    fn extension_control(
        &self,
        session: String,
        source: u64,
        control: ExtensionControl,
    ) -> HostFuture<()> {
        if let Err(e) = self.record("ExtensionControl".into()) {
            return Box::pin(async move { Err(e) });
        }
        let (kind, message) = match control {
            ExtensionControl::Close => ("close", None),
            ExtensionControl::Preferences => ("preferences", None),
            ExtensionControl::ClearSearch => ("clear-search", None),
            ExtensionControl::Hud(message) => ("hud", Some(message)),
        };
        self.emit(
            "extension:ui",
            serde_json::json!({"sessionId":session,"source":source,"kind":kind,"message":message}),
        );
        Box::pin(async { Ok(()) })
    }
    fn desktop_effect(&self, effect: DesktopEffect) -> Option<Result<Value, String>> {
        if !self.allow_interaction {
            return Some(Err("Desktop effect denied".into()));
        }
        let result = match &effect {
            DesktopEffect::ClipboardSequence => serde_json::json!(0),
            DesktopEffect::ReadClipboard => {
                serde_json::json!(self.clipboard_text.lock().unwrap().clone())
            }
            DesktopEffect::ReadClipboardContent => {
                serde_json::json!({"text": self.clipboard_text.lock().unwrap().clone(), "hasFiles": false})
            }
            DesktopEffect::WriteText(text) => {
                *self.clipboard_text.lock().unwrap() = text.clone();
                serde_json::json!({"ok":true})
            }
            DesktopEffect::SelectedText => Value::String(String::new()),
            _ => serde_json::json!({"ok":true}),
        };
        self.desktop_effects.lock().unwrap().push(effect);
        Some(Ok(result))
    }

    fn emit(&self, channel: &str, value: Value) {
        self.events.lock().unwrap().push((channel.into(), value));
    }
    fn surface(&self, op: SurfaceOp) -> HostFuture<()> {
        let result = self.record(format!("surface:{op:?}"));
        Box::pin(async { result })
    }
    fn dialog(&self, req: DialogRequest) -> HostFuture<DialogReply> {
        let result = self.record(format!("dialog:{req:?}")).and_then(|_| {
            self.dialogs
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Err("Unscripted dialog".into()))
        });
        Box::pin(async { result })
    }
    fn shortcuts(&self) -> &dyn ShortcutRegistry {
        self
    }
    fn status_items(&self) -> &dyn StatusItems {
        self
    }
    fn capture_active_display(&self) -> HostFuture<Vec<u8>> {
        let result = self.record("capture".into()).and_then(|_| {
            self.captures
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Err("Unscripted capture".into()))
        });
        Box::pin(async { result })
    }
    fn desktop_services(&self) -> bool {
        self.desktop
    }
    fn notify(&self, spec: NotificationSpec) -> HostFuture<()> {
        self.calls.lock().unwrap().push(format!("notify:{spec:?}"));
        Box::pin(async { Ok(()) })
    }
}
impl ShortcutRegistry for TestHost {
    fn available(&self) -> bool {
        self.allow_interaction
    }
    fn parse(&self, raw: &str) -> Result<Shortcut, String> {
        let id = raw
            .bytes()
            .fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
        Ok(Shortcut {
            id,
            accelerator: raw.into(),
        })
    }
    fn register(&self, shortcut: &Shortcut) -> Result<(), String> {
        self.record(format!("register:{shortcut}"))?;
        self.shortcut_results
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Ok(()))
    }
    fn unregister(&self, shortcut: &Shortcut) -> Result<(), String> {
        self.record(format!("unregister:{shortcut}"))?;
        self.shortcut_results
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Ok(()))
    }
}
impl StatusItems for TestHost {
    fn set(&self, id: &str, spec: StatusItem, callback: StatusCallback) -> Result<(), String> {
        self.record(format!("status:set:{id}"))?;
        self.status_specs.lock().unwrap().insert(id.into(), spec);
        self.status.lock().unwrap().insert(id.into(), callback);
        Ok(())
    }
    fn remove(&self, id: &str) -> Result<(), String> {
        self.record(format!("status:remove:{id}"))?;
        self.status_specs.lock().unwrap().remove(id);
        self.status.lock().unwrap().remove(id);
        Ok(())
    }
}
