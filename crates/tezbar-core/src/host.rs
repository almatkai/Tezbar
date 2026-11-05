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
}
#[derive(Clone, Debug, PartialEq)]
pub enum DialogReply {
    Confirm(bool),
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
#[derive(Clone, Debug)]
pub struct StatusItem {
    pub title: String,
    pub tooltip: String,
    pub items: Vec<Value>,
    pub icon: Option<(Vec<u8>, u32, u32)>,
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
pub trait Host: Send + Sync + 'static {
    fn emit(&self, channel: &str, payload: Value);
    fn surface(&self, op: SurfaceOp) -> HostFuture<()>;
    fn dialog(&self, request: DialogRequest) -> HostFuture<DialogReply>;
    fn shortcuts(&self) -> &dyn ShortcutRegistry;
    fn status_items(&self) -> &dyn StatusItems;
    fn capture_active_display(&self) -> HostFuture<Vec<u8>>;
    fn desktop_services(&self) -> bool;
    fn notify(&self, _spec: NotificationSpec) -> HostFuture<()> {
        Box::pin(async { Ok(()) })
    }
}
/// Scripted headless adapter. Desktop interaction is denied unless explicitly enabled.
#[derive(Default)]
pub struct TestHost {
    pub desktop: bool,
    pub allow_interaction: bool,
    pub calls: Mutex<Vec<String>>,
    pub events: Mutex<Vec<(String, Value)>>,
    pub dialogs: Mutex<VecDeque<Result<DialogReply, String>>>,
    pub captures: Mutex<VecDeque<Result<Vec<u8>, String>>>,
    pub shortcut_results: Mutex<VecDeque<Result<(), String>>>,
    pub status: Mutex<HashMap<String, StatusCallback>>,
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
    fn set(&self, id: &str, _: StatusItem, callback: StatusCallback) -> Result<(), String> {
        self.record(format!("status:set:{id}"))?;
        self.status.lock().unwrap().insert(id.into(), callback);
        Ok(())
    }
    fn remove(&self, id: &str) -> Result<(), String> {
        self.record(format!("status:remove:{id}"))?;
        self.status.lock().unwrap().remove(id);
        Ok(())
    }
}
