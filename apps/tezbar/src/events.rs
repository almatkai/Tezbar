//! Native callbacks enqueue owned data. The receiver is awaited by GPUI: no recurring frame timer.
use std::sync::{Arc, Mutex, Weak};
use tezbar_core::host::{DialogReply, DialogRequest, SurfaceOp};
use tokio::sync::oneshot;
#[derive(Clone, Debug)]
pub enum CoreEvent {
    Backend(serde_json::Value),
    OpenSurface(serde_json::Value),
    Invalidated,
    Other {
        channel: String,
        payload: serde_json::Value,
    },
}
impl CoreEvent {
    pub fn decode(channel: &str, payload: serde_json::Value) -> Self {
        match channel {
            "backend-status" => Self::Backend(payload),
            "app:open-surface" => Self::OpenSurface(payload),
            "launcher:invalidated" => Self::Invalidated,
            _ => Self::Other {
                channel: channel.into(),
                payload,
            },
        }
    }
}
pub enum Event {
    #[cfg(feature = "bench-control")]
    Control(
        serde_json::Value,
        std::sync::mpsc::SyncSender<Result<serde_json::Value, String>>,
    ),
    Main(Box<dyn FnOnce() + Send>),
    Surface(SurfaceOp, oneshot::Sender<Result<(), String>>),
    Dialog(DialogRequest, oneshot::Sender<Result<DialogReply, String>>),
    Core(CoreEvent),
    SavePng {
        directory: std::path::PathBuf,
        name: String,
        png: Vec<u8>,
        reply: oneshot::Sender<Result<Option<std::path::PathBuf>, String>>,
    },
    Activate,
    Hide,
    Toggle,
    Settings,
    SettingsFocused,
    ThemeChanged(String),
    Quit,
    RetryCore,
    Bounds,
    CloseSettings,
    DialogClosed,
    SnapBegin,
    SnapEnd,
    SnapDwell(u64),
    Zoom(f32),
    PanelShow(Vec<(String, String)>),
    PanelHide,
    PanelFocused(bool),
    Menu(String),
    Hotkey(u32),
    Blur(u64),
    Idle(u64),
    SettingsClose(u64),
    SettingsAck(u64),
    SettingsFinish(u64, Result<(), String>),
    Capture(oneshot::Sender<CaptureLease>),
    RestoreCapture(CaptureSnapshot),
}
#[derive(Clone)]
pub struct CaptureSnapshot {
    pub windows: Vec<gpui::WindowId>,
    pub generation: u64,
}
pub struct CaptureLease {
    pub snapshot: CaptureSnapshot,
    relay: Relay,
}
impl CaptureLease {
    pub fn new(relay: Relay, snapshot: CaptureSnapshot) -> Self {
        Self { relay, snapshot }
    }
}
impl Drop for CaptureLease {
    fn drop(&mut self) {
        let _ = self.relay.send(Event::RestoreCapture(CaptureSnapshot {
            windows: std::mem::take(&mut self.snapshot.windows),
            generation: self.snapshot.generation,
        }));
    }
}
pub struct DialogLease(pub Relay);
impl Drop for DialogLease {
    fn drop(&mut self) {
        let _ = self.0.send(Event::DialogClosed);
    }
}
const CONTROL_CAPACITY: usize = 256;
const CORE_BYTE_BUDGET: usize = 4 * 1024 * 1024;
struct Queues {
    control: std::collections::VecDeque<Event>,
    core: std::collections::VecDeque<(CoreEvent, usize)>,
    bytes: usize,
    open: bool,
    core_enabled: bool,
}
pub struct Mailbox {
    queues: Mutex<Queues>,
    space: std::sync::Condvar,
    wake: tokio::sync::Notify,
    gui: std::thread::ThreadId,
}
#[derive(Clone)]
pub struct Relay(Arc<Mailbox>);
pub struct Receiver(Arc<Mailbox>);
impl Receiver {
    pub async fn recv(&mut self) -> Option<Event> {
        loop {
            let wake = self.0.wake.notified();
            {
                let mut q = self.0.queues.lock().unwrap();
                if let Some(event) = q.control.pop_front() {
                    self.0.space.notify_all();
                    return Some(event);
                }
                if let Some((event, bytes)) = if q.core_enabled {
                    q.core.pop_front()
                } else {
                    None
                } {
                    q.bytes -= bytes;
                    self.0.space.notify_all();
                    return Some(Event::Core(event));
                }
                if !q.open {
                    return None;
                }
            }
            wake.await;
        }
    }
}
impl Relay {
    pub(crate) fn from_mailbox(mailbox: Arc<Mailbox>) -> Self {
        Self(mailbox)
    }
    pub fn new() -> (Self, Receiver) {
        let mailbox = Arc::new(Mailbox {
            queues: Mutex::new(Queues {
                control: Default::default(),
                core: Default::default(),
                bytes: 0,
                open: true,
                core_enabled: true,
            }),
            space: std::sync::Condvar::new(),
            wake: tokio::sync::Notify::new(),
            gui: std::thread::current().id(),
        });
        (Self(mailbox.clone()), Receiver(mailbox))
    }
    pub fn send(&self, event: Event) -> Result<(), String> {
        if let Event::Core(event) = event {
            return self.send_core(event);
        }
        let mut q = self.0.queues.lock().map_err(|e| e.to_string())?;
        if !q.open {
            return Err("Application is shutting down".into());
        }
        // Activation, visibility and layout are state signals and coalesce while the UI is busy.
        let equivalent = |queued: &Event| {
            matches!(
                (&event, queued),
                (Event::Activate, Event::Activate) | (Event::Bounds, Event::Bounds)
            )
        };
        if q.control.iter().any(equivalent) {
            return Ok(());
        }
        if q.control.len() >= CONTROL_CAPACITY {
            return Err("Native control queue is full; operation was not executed".into());
        }
        q.control.push_back(event);
        drop(q);
        self.0.wake.notify_one();
        Ok(())
    }
    pub fn send_core(&self, event: CoreEvent) -> Result<(), String> {
        let bytes = match &event {
            CoreEvent::Other { channel, payload } => channel.len() + payload.to_string().len(),
            CoreEvent::Backend(value) | CoreEvent::OpenSurface(value) => value.to_string().len(),
            CoreEvent::Invalidated => 0,
        };
        let key = |event: &CoreEvent| match event {
            CoreEvent::Backend(_) => Some("backend-status"),
            CoreEvent::Invalidated => Some("launcher:invalidated"),
            CoreEvent::Other { channel, .. } if channel == "launcher-viewport-changed" => {
                Some("launcher-viewport-changed")
            }
            CoreEvent::Other { channel, .. } if channel == "launcher-layout-changed" => {
                Some("launcher-layout-changed")
            }
            _ => None,
        };
        let mut q = self.0.queues.lock().map_err(|e| e.to_string())?;
        if let Some(key) = key(&event) {
            if let Some(index) = q.core.iter().position(|(queued, _)| match queued {
                CoreEvent::Backend(_) => key == "backend-status",
                CoreEvent::Invalidated => key == "launcher:invalidated",
                CoreEvent::Other { channel, .. } => channel == key,
                _ => false,
            }) {
                let (_, old) = q.core.remove(index).unwrap();
                q.bytes -= old;
            }
        }
        // Lossless backpressure for stream/terminal/tool bytes. One oversized core-owned event may occupy
        // an otherwise empty queue; it can never accumulate beside other output events.
        while q.open
            && (!q.core.is_empty()
                && (q.bytes.saturating_add(bytes) > CORE_BYTE_BUDGET || q.core.len() >= 256))
        {
            if std::thread::current().id() == self.0.gui {
                return Err("GUI thread must not emit high-volume core output".into());
            }
            q = self.0.space.wait(q).map_err(|e| e.to_string())?;
        }
        if !q.open {
            return Err("Core event owner closed during backpressure".into());
        }
        q.bytes += bytes;
        q.core.push_back((event, bytes));
        drop(q);
        self.0.wake.notify_one();
        Ok(())
    }
    pub fn enable_core(&self, enabled: bool) {
        self.0.queues.lock().unwrap().core_enabled = enabled;
        self.0.wake.notify_one();
    }
    pub fn close(&self) {
        let mut q = self.0.queues.lock().unwrap();
        q.open = false;
        q.control.clear();
        q.core.clear();
        q.bytes = 0;
        drop(q);
        self.0.space.notify_all();
        self.0.wake.notify_one();
    }
    pub fn downgrade(&self) -> Weak<Mailbox> {
        Arc::downgrade(&self.0)
    }
}
pub fn install_native_handlers(relay: &Relay) {
    let weak = relay.downgrade();
    global_hotkey::GlobalHotKeyEvent::set_event_handler(Some(
        move |e: global_hotkey::GlobalHotKeyEvent| {
            if e.state == global_hotkey::HotKeyState::Pressed {
                send_weak(&weak, Event::Hotkey(e.id));
            }
        },
    ));
    let weak = relay.downgrade();
    muda::MenuEvent::set_event_handler(Some(move |e: muda::MenuEvent| {
        send_weak(&weak, Event::Menu(e.id.0))
    }));
    let weak = relay.downgrade();
    tray_icon::TrayIconEvent::set_event_handler(Some(move |e: tray_icon::TrayIconEvent| {
        if let tray_icon::TrayIconEvent::Click {
            id,
            button: tray_icon::MouseButton::Left,
            button_state: tray_icon::MouseButtonState::Up,
            ..
        } = e
        {
            if id.0 == "tezbar-preview-primary" {
                send_weak(&weak, Event::Toggle);
            }
        }
    }));
}
fn send_weak(weak: &Weak<Mailbox>, event: Event) {
    if let Some(mailbox) = weak.upgrade() {
        let _ = Relay(mailbox).send(event);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn closed_relay_cannot_wake_or_replay_effect() {
        let (r, mut rx) = Relay::new();
        r.send(Event::Toggle).unwrap();
        assert!(matches!(rx.recv().await, Some(Event::Toggle)));
        let weak = r.downgrade();
        r.close();
        send_weak(&weak, Event::Toggle);
        assert!(rx.recv().await.is_none());
        assert!(r.send(Event::Toggle).is_err());
    }
    #[tokio::test]
    async fn output_backpressure_preserves_bytes_and_wakes_on_drain() {
        let (r, mut rx) = Relay::new();
        let payload = serde_json::json!({"data":"x".repeat(CORE_BYTE_BUDGET)});
        r.send_core(CoreEvent::Other {
            channel: "terminal:data".into(),
            payload: payload.clone(),
        })
        .unwrap();
        let producer = r.clone();
        let thread = std::thread::spawn(move || {
            producer.send_core(CoreEvent::Other {
                channel: "terminal:data".into(),
                payload,
            })
        });
        assert!(matches!(
            rx.recv().await,
            Some(Event::Core(CoreEvent::Other { .. }))
        ));
        assert!(thread.join().unwrap().is_ok());
        assert!(matches!(
            rx.recv().await,
            Some(Event::Core(CoreEvent::Other { .. }))
        ));
        r.close();
    }
    #[tokio::test]
    async fn activation_and_status_are_coalesced() {
        let (r, mut rx) = Relay::new();
        for _ in 0..1000 {
            r.send(Event::Activate).unwrap();
            r.send_core(CoreEvent::Backend(serde_json::json!("ready")))
                .unwrap();
        }
        assert!(matches!(rx.recv().await, Some(Event::Activate)));
        assert!(matches!(
            rx.recv().await,
            Some(Event::Core(CoreEvent::Backend(_)))
        ));
        r.close();
        assert!(rx.recv().await.is_none());
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        Relay(self.0.clone()).close();
    }
}

/// Subscribe before starting an optional core owner. Critical output without a live subscriber is
/// retained; reaching the budget pauses core dequeueing while native controls remain responsive.
#[derive(Clone)]
pub struct CoreEvents {
    state: std::rc::Rc<std::cell::RefCell<HubState>>,
    relay: Relay,
}
impl gpui::Global for CoreEvents {}
struct HubState {
    next: u64,
    listeners: std::collections::BTreeMap<u64, Listener>,
    pending: std::collections::VecDeque<CoreEvent>,
    bytes: usize,
    closed: bool,
    active: std::collections::BTreeSet<u64>,
}
struct Listener {
    channels: Vec<String>,
    callback: Box<dyn FnMut(&CoreEvent, &mut gpui::App) -> bool>,
}
pub struct EventSubscription {
    state: std::rc::Weak<std::cell::RefCell<HubState>>,
    id: u64,
}
impl Drop for EventSubscription {
    fn drop(&mut self) {
        if let Some(state) = self.state.upgrade() {
            let mut state = state.borrow_mut();
            state.listeners.remove(&self.id);
            state.active.remove(&self.id);
        }
    }
}
impl CoreEvent {
    pub fn channel(&self) -> &str {
        match self {
            Self::Backend(_) => "backend-status",
            Self::OpenSurface(_) => "app:open-surface",
            Self::Invalidated => "launcher:invalidated",
            Self::Other { channel, .. } => channel,
        }
    }
    fn bytes(&self) -> usize {
        match self {
            Self::Backend(v) | Self::OpenSurface(v) => v.to_string().len(),
            Self::Invalidated => 0,
            Self::Other { channel, payload } => channel.len() + payload.to_string().len(),
        }
    }
    fn reliable(&self) -> bool {
        matches!(self,Self::Other{channel,..}if !channel.starts_with("window-")&&!channel.starts_with("launcher-"))
    }
}
impl CoreEvents {
    pub fn new(relay: Relay) -> Self {
        Self {
            state: std::rc::Rc::new(std::cell::RefCell::new(HubState {
                next: 0,
                listeners: Default::default(),
                pending: Default::default(),
                bytes: 0,
                closed: false,
                active: Default::default(),
            })),
            relay,
        }
    }
    pub fn subscribe<V: 'static>(
        &self,
        owner: &gpui::Entity<V>,
        channels: Vec<String>,
        mut listener: impl FnMut(&gpui::Entity<V>, &CoreEvent, &mut gpui::App) + 'static,
        cx: &mut gpui::App,
    ) -> EventSubscription {
        let owner = owner.downgrade();
        let mut state = self.state.borrow_mut();
        state.next += 1;
        let id = state.next;
        state.active.insert(id);
        state.listeners.insert(
            id,
            Listener {
                channels,
                callback: Box::new(move |event, cx| {
                    if let Some(owner) = owner.upgrade() {
                        listener(&owner, event, cx);
                        true
                    } else {
                        false
                    }
                }),
            },
        );
        drop(state);
        let pending = std::mem::take(&mut self.state.borrow_mut().pending);
        self.state.borrow_mut().bytes = 0;
        for event in pending {
            self.publish(event, cx);
        }
        self.relay
            .enable_core(self.state.borrow().bytes < CORE_BYTE_BUDGET);
        EventSubscription {
            state: std::rc::Rc::downgrade(&self.state),
            id,
        }
    }
    pub fn dispose(&self) {
        let mut state = self.state.borrow_mut();
        state.closed = true;
        state.pending.clear();
        state.bytes = 0;
        state.listeners.clear();
        state.active.clear();
        self.relay.enable_core(true);
    }
    pub fn publish(&self, event: CoreEvent, cx: &mut gpui::App) {
        if self.state.borrow().closed {
            return;
        }
        // Move callbacks out while invoking them: subscribers may drop ownership or add another subscriber.
        let ids = self
            .state
            .borrow()
            .listeners
            .iter()
            .filter(|(_, l)| l.channels.iter().any(|s| s == event.channel()))
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        let mut delivered = false;
        for id in ids {
            let slot = self.state.borrow_mut().listeners.remove(&id);
            if let Some(mut slot) = slot {
                if (slot.callback)(&event, cx) {
                    delivered = true;
                    if self.state.borrow().active.contains(&id) {
                        self.state.borrow_mut().listeners.insert(id, slot);
                    }
                }
            }
        }
        if !delivered && event.reliable() {
            let mut state = self.state.borrow_mut();
            state.bytes = state.bytes.saturating_add(event.bytes());
            state.pending.push_back(event);
            if state.bytes >= CORE_BYTE_BUDGET {
                self.relay.enable_core(false);
            }
        }
    }
}
#[cfg(test)]
mod hub_tests {
    use super::*;
    use gpui::AppContext;
    #[gpui::test]
    fn buffered_output_is_owned_until_subscribed_and_unsubscribe_prevents_callbacks(
        cx: &mut gpui::TestAppContext,
    ) {
        let (relay, _rx) = Relay::new();
        let hub = CoreEvents::new(relay);
        let seen = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        cx.update(|cx| {
            let event = |text: &str| CoreEvent::Other {
                channel: "terminal:data".into(),
                payload: serde_json::json!({"data":text}),
            };
            hub.publish(event("a"), cx);
            assert_eq!(hub.state.borrow().pending.len(), 1);
            let owner = cx.new(|_| ());
            let sink = seen.clone();
            let subscription = hub.subscribe(
                &owner,
                vec!["terminal:data".into()],
                move |_, event, _| {
                    if let CoreEvent::Other { payload, .. } = event {
                        sink.lock()
                            .unwrap()
                            .push(payload["data"].as_str().unwrap().to_owned());
                    }
                },
                cx,
            );
            assert_eq!(*seen.lock().unwrap(), ["a"]);
            drop(subscription);
            hub.publish(event("b"), cx);
            assert_eq!(*seen.lock().unwrap(), ["a"]);
            assert_eq!(hub.state.borrow().pending.len(), 1);
            hub.dispose();
            assert!(hub.state.borrow().pending.is_empty());
        });
    }
}
#[cfg(test)]
mod capture_lease_tests {
    use super::*;
    #[tokio::test]
    async fn cancellation_after_capture_reply_send_still_restores() {
        let (relay, mut receiver) = Relay::new();
        let (tx, rx) = oneshot::channel();
        assert!(
            tx.send(CaptureLease::new(
                relay,
                CaptureSnapshot {
                    windows: vec![],
                    generation: 9
                }
            ))
            .is_ok()
        );
        drop(rx);
        assert!(matches!(
            receiver.recv().await,
            Some(Event::RestoreCapture(CaptureSnapshot { generation: 9, .. }))
        ));
    }
}
