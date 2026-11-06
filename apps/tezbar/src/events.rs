//! Native callbacks enqueue owned data. The receiver is awaited by GPUI: no recurring frame timer.
use std::sync::{Arc, Mutex, Weak};
use tezbar_core::host::{DialogReply, DialogRequest, SurfaceOp};
use tokio::sync::oneshot;
#[derive(Clone, Debug)]
pub enum CoreEvent {
    Backend(serde_json::Value),
    OpenSurface(serde_json::Value),
    Invalidated {
        text: Option<String>,
    },
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
            "launcher:invalidated" => Self::Invalidated {
                text: payload["text"].as_str().map(str::to_owned),
            },
            _ => Self::Other {
                channel: channel.into(),
                payload,
            },
        }
    }
}
pub enum Event {
    SettingsEffect(gpui::EntityId, crate::surfaces::settings::SettingsEffect),
    OnboardingFinished(gpui::EntityId),
    Navigate(crate::surfaces::command_bar::navigation::Navigation),
    Reveal(String),
    Update(crate::surfaces::command_bar::navigation::UpdateAction),
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
const CORE_BYTE_BUDGET: usize = 512 * 1024;
struct Queues {
    control: std::collections::VecDeque<Event>,
    core: std::collections::VecDeque<(CoreEvent, usize)>,
    bytes: usize,
    open: bool,
    core_enabled: bool,
    core_open: bool,
    leases: usize,
    channels: std::collections::BTreeMap<String, (usize, usize)>,
    dialog_releases: usize,
    capture_releases: std::collections::VecDeque<CaptureSnapshot>,
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
                if q.dialog_releases > 0 {
                    q.dialog_releases -= 1;
                    q.leases = q.leases.saturating_sub(1);
                    return Some(Event::DialogClosed);
                }
                if let Some(snapshot) = q.capture_releases.pop_front() {
                    q.leases = q.leases.saturating_sub(1);
                    return Some(Event::RestoreCapture(snapshot));
                }
                if let Some(event) = q.control.pop_front() {
                    self.0.space.notify_all();
                    return Some(event);
                }
                if let Some((event, bytes)) = if q.core_enabled {
                    q.core
                        .iter()
                        .position(|(e, _)| {
                            builtin(e.channel())
                                || q.channels
                                    .get(e.channel())
                                    .is_some_and(|(_, listeners)| *listeners > 0)
                        })
                        .and_then(|i| q.core.remove(i))
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
                core_open: true,
                leases: 0,
                channels: Default::default(),
                dialog_releases: 0,
                capture_releases: Default::default(),
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
        // Releases are bounded by admitted live leases, independent of ordinary control capacity.
        if matches!(&event, Event::DialogClosed | Event::RestoreCapture(_))
            && q.dialog_releases + q.capture_releases.len() >= q.leases
        {
            return Err("No outstanding native lease to release".into());
        }
        match event {
            Event::DialogClosed => {
                q.dialog_releases += 1;
                drop(q);
                self.0.wake.notify_one();
                return Ok(());
            }
            Event::RestoreCapture(snapshot) => {
                q.capture_releases.push_back(snapshot);
                drop(q);
                self.0.wake.notify_one();
                return Ok(());
            }
            _ => {}
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
        if matches!(
            &event,
            Event::Dialog(..) | Event::Capture(_) | Event::SavePng { .. }
        ) {
            if q.leases >= 128 {
                return Err("Native interaction ownership limit reached".into());
            }
            q.leases += 1;
        }
        q.control.push_back(event);
        drop(q);
        self.0.wake.notify_one();
        Ok(())
    }
    /// Host::emit is synchronous. On a multi-thread Tokio worker block_in_place hands
    /// scheduling back to Tokio before waiting. The application caps replacement/blocking
    /// workers at 16; there is no task-per-event or detached producer queue. Each caller
    /// retains exactly its current payload until admission or owner shutdown.
    pub fn send_core(&self, event: CoreEvent) -> Result<(), String> {
        if tokio::runtime::Handle::try_current()
            .is_ok_and(|h| h.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread)
            && std::thread::current().id() != self.0.gui
        {
            tokio::task::block_in_place(|| self.enqueue_core(event))
        } else {
            self.enqueue_core(event)
        }
    }
    fn enqueue_core(&self, event: CoreEvent) -> Result<(), String> {
        let bytes = event.bytes();
        let channel = event.channel().to_owned();
        let mut q = self.0.queues.lock().map_err(|e| e.to_string())?;
        loop {
            if !q.open || !q.core_open {
                return Err("Core event owner closed during backpressure".into());
            }
            if !builtin(&channel) && !q.channels.contains_key(&channel) {
                return Ok(());
            }
            if (builtin(&channel) && channel != "app:open-surface")
                || channel.ends_with("status")
                || channel.ends_with("-changed")
            {
                if let Some(i) = q.core.iter().position(|(e, _)| e.channel() == channel) {
                    let (_, old) = q.core.remove(i).unwrap();
                    q.bytes -= old;
                }
            }
            let (count, size) = q
                .core
                .iter()
                .filter(|(e, _)| e.channel() == channel)
                .fold((0, 0usize), |(n, b), (_, s)| (n + 1, b + s));
            if count == 0 || (count < 256 && size.saturating_add(bytes) <= CORE_BYTE_BUDGET) {
                break;
            }
            if std::thread::current().id() == self.0.gui {
                return Err("GUI must not synchronously produce a full stream".into());
            }
            q = self.0.space.wait(q).map_err(|e| e.to_string())?;
        }
        q.bytes += bytes;
        q.core.push_back((event, bytes));
        drop(q);
        self.0.wake.notify_one();
        Ok(())
    }
    fn own_channel(&self, channel: &str, listening: bool) -> Result<ChannelOwner, String> {
        let mut q = self.0.queues.lock().unwrap();
        if !q.open || (!q.channels.contains_key(channel) && q.channels.len() >= 32) {
            return Err("Core channel ownership limit reached or closed".into());
        }
        let entry = q.channels.entry(channel.into()).or_default();
        entry.0 += 1;
        entry.1 += usize::from(listening);
        drop(q);
        self.0.wake.notify_one();
        Ok(ChannelOwner {
            relay: self.clone(),
            channel: channel.into(),
            listening,
        })
    }
    pub fn enable_core(&self, enabled: bool) {
        self.0.queues.lock().unwrap().core_enabled = enabled;
        self.0.wake.notify_one();
    }
    pub fn close_core(&self) {
        let mut q = self.0.queues.lock().unwrap();
        q.core_open = false;
        q.core.clear();
        q.bytes = 0;
        q.channels.clear();
        drop(q);
        self.0.space.notify_all();
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
        let _owner = r.own_channel("terminal:data", true).unwrap();
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

/// Subscribe before starting optional work. Hidden stream retention requires an explicit channel
/// subscription that remains live while its surface is hidden. Unknown output is dropped;
/// independent channel budgets never pause unrelated channels. No owner may reserve a queue
/// without a consumer: stopping a producer and dropping its subscription releases backpressure.
#[derive(Clone)]
pub struct CoreEvents {
    state: std::rc::Rc<std::cell::RefCell<HubState>>,
    relay: Relay,
}
impl gpui::Global for CoreEvents {}
struct HubState {
    next: u64,
    listeners: std::collections::BTreeMap<u64, Listener>,
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
    _channels: Vec<ChannelOwner>,
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
            Self::Invalidated { .. } => "launcher:invalidated",
            Self::Other { channel, .. } => channel,
        }
    }
    fn bytes(&self) -> usize {
        match self {
            Self::Backend(v) | Self::OpenSurface(v) => v.to_string().len(),
            Self::Invalidated { text } => text.as_ref().map_or(0, String::len),
            Self::Other { channel, payload } => channel.len() + payload.to_string().len(),
        }
    }
}
impl CoreEvents {
    pub fn new(relay: Relay) -> Self {
        Self {
            state: std::rc::Rc::new(std::cell::RefCell::new(HubState {
                next: 0,
                listeners: Default::default(),
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
        _cx: &mut gpui::App,
    ) -> EventSubscription {
        let owned_channels = channels
            .iter()
            .map(|channel| {
                self.relay
                    .own_channel(channel, true)
                    .expect("subscribe before producer start; at most 32 owned channels")
            })
            .collect();
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
        EventSubscription {
            state: std::rc::Rc::downgrade(&self.state),
            id,
            _channels: owned_channels,
        }
    }
    pub fn dispose(&self) {
        let mut state = self.state.borrow_mut();
        state.closed = true;
        state.listeners.clear();
        state.active.clear();
        self.relay.close_core();
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
        let _ = delivered;
    }
}
#[cfg(test)]
mod capture_lease_tests {
    use super::*;
    #[tokio::test]
    async fn cancellation_after_capture_reply_send_still_restores() {
        let (relay, mut receiver) = Relay::new();
        relay.0.queues.lock().unwrap().leases = 1;
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

fn builtin(channel: &str) -> bool {
    matches!(
        channel,
        "backend-status"
            | "app:open-surface"
            | "launcher:invalidated"
            | "launcher-viewport-changed"
            | "launcher-layout-changed"
    )
}
/// Hold before starting a producer. Drop only after stopping it, or explicitly on shutdown.
struct ChannelOwner {
    relay: Relay,
    channel: String,
    listening: bool,
}
impl Drop for ChannelOwner {
    fn drop(&mut self) {
        let mut q = self.relay.0.queues.lock().unwrap();
        if let Some(entry) = q.channels.get_mut(&self.channel) {
            entry.0 -= 1;
            entry.1 = entry.1.saturating_sub(usize::from(self.listening));
            if entry.0 == 0 {
                q.channels.remove(&self.channel);
                let mut bytes = 0;
                q.core.retain(|(event, size)| {
                    if event.channel() == self.channel {
                        bytes += size;
                        false
                    } else {
                        true
                    }
                });
                q.bytes -= bytes;
            }
        }
        drop(q);
        self.relay.0.space.notify_all();
        self.relay.0.wake.notify_one();
    }
}

#[cfg(test)]
mod repair_tests {
    use super::*;
    fn output(channel: &str, n: usize) -> CoreEvent {
        CoreEvent::Other {
            channel: channel.into(),
            payload: serde_json::json!({"n": n,"bytes":"x".repeat(16384)}),
        }
    }
    #[test]
    fn two_worker_two_producer_flood_control_and_quit_remain_live() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .max_blocking_threads(8)
            .enable_all()
            .build()
            .unwrap();
        let (relay, mut rx) = Relay::new();
        let _a = relay.own_channel("agent:event", true).unwrap();
        let _b = relay.own_channel("terminal:data", true).unwrap();
        // Watchdog releases blocked callers even against the original broken implementation.
        let stop = relay.clone();
        let (done, watch) = std::sync::mpsc::channel();
        let watchdog = std::thread::spawn(move || {
            if watch
                .recv_timeout(std::time::Duration::from_secs(5))
                .is_err()
            {
                stop.close();
            }
        });
        rt.block_on(async {
            let mut producers = vec![];
            for channel in ["agent:event", "terminal:data"] {
                let r = relay.clone();
                producers.push(tokio::spawn(async move {
                    for n in 0..600 {
                        r.send_core(output(channel, n)).unwrap();
                    }
                }));
            }
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            relay.send(Event::Quit).unwrap();
            assert!(matches!(rx.recv().await, Some(Event::Quit)));
            let mut counts = [0, 0];
            for _ in 0..1200 {
                match rx.recv().await {
                    Some(Event::Core(CoreEvent::Other { channel, payload })) => {
                        let index = usize::from(channel == "terminal:data");
                        assert_eq!(payload["n"], counts[index]);
                        counts[index] += 1;
                    }
                    _ => panic!("lost output or Tokio starvation"),
                }
            }
            assert_eq!(counts, [600, 600]);
            for producer in producers {
                producer.await.unwrap();
            }
            relay.close_core();
        });
        done.send(()).unwrap();
        watchdog.join().unwrap();
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn hidden_owned_channel_is_independent_and_drop_releases_producer() {
        let (relay, mut rx) = Relay::new();
        let owner = relay.own_channel("agent:event", false).unwrap();
        for n in 0..256 {
            relay
                .send_core(CoreEvent::Other {
                    channel: "agent:event".into(),
                    payload: serde_json::json!(n),
                })
                .unwrap();
        }
        let r = relay.clone();
        let blocked = tokio::spawn(async move { r.send_core(output("agent:event", 256)) });
        relay
            .send_core(CoreEvent::Backend(serde_json::json!("ready")))
            .unwrap();
        assert!(matches!(
            rx.recv().await,
            Some(Event::Core(CoreEvent::Backend(_)))
        ));
        relay.send_core(output("unknown:channel", 0)).unwrap();
        assert_eq!(relay.0.queues.lock().unwrap().core.len(), 256);
        drop(owner);
        tokio::time::timeout(std::time::Duration::from_secs(2), blocked)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(relay.0.queues.lock().unwrap().core.is_empty());
    }
    #[tokio::test]
    async fn lease_release_bypasses_full_control_queue_with_bounded_ownership() {
        let (relay, mut rx) = Relay::new();
        relay.0.queues.lock().unwrap().leases = 2;
        for _ in 0..CONTROL_CAPACITY {
            relay.send(Event::Toggle).unwrap();
        }
        drop(DialogLease(relay.clone()));
        drop(CaptureLease::new(
            relay.clone(),
            CaptureSnapshot {
                windows: vec![],
                generation: 7,
            },
        ));
        assert!(matches!(rx.recv().await, Some(Event::DialogClosed)));
        assert!(matches!(
            rx.recv().await,
            Some(Event::RestoreCapture(CaptureSnapshot { generation: 7, .. }))
        ));
        assert_eq!(
            relay.0.queues.lock().unwrap().control.len(),
            CONTROL_CAPACITY
        );
    }
    #[gpui::test]
    fn hidden_live_subscriber_receives_and_unknown_output_is_not_retained(
        cx: &mut gpui::TestAppContext,
    ) {
        use gpui::AppContext;
        let (relay, _rx) = Relay::new();
        let hub = CoreEvents::new(relay.clone());
        cx.update(|cx| {
            let owner = cx.new(|_| 0usize);
            let subscription = hub.subscribe(
                &owner,
                vec!["agent:event".into()],
                |owner, _, cx| owner.update(cx, |n, _| *n += 1),
                cx,
            );
            hub.publish(output("agent:event", 0), cx);
            assert_eq!(*owner.read(cx), 1);
            drop(subscription);
            relay.send_core(output("agent:event", 1)).unwrap();
            assert!(relay.0.queues.lock().unwrap().core.is_empty());
            hub.dispose();
            assert!(relay.send_core(output("agent:event", 2)).is_err());
        });
    }
}

#[cfg(test)]
mod shutdown_queue_repair_tests {
    use super::*;
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn closing_owned_queues_releases_two_flooding_workers_before_shutdown_drain() {
        let (relay, mut rx) = Relay::new();
        let _owner = relay.own_channel("terminal:data", true).unwrap();
        let mut producers = vec![];
        for _ in 0..2 {
            let relay = relay.clone();
            producers.push(tokio::spawn(async move {
                loop {
                    if relay
                        .send_core(CoreEvent::Other {
                            channel: "terminal:data".into(),
                            payload: serde_json::json!("x".repeat(65536)),
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        relay.send(Event::Quit).unwrap();
        assert!(matches!(rx.recv().await, Some(Event::Quit)));
        relay.close_core();
        for producer in producers {
            tokio::time::timeout(std::time::Duration::from_secs(1), producer)
                .await
                .unwrap()
                .unwrap();
        }
        assert_eq!(relay.0.queues.lock().unwrap().bytes, 0);
    }
}

#[cfg(test)]
mod lease_admission_repair_tests {
    use super::*;
    #[tokio::test]
    async fn live_lease_admission_and_reserved_releases_are_both_bounded() {
        let (relay, mut receiver) = Relay::new();
        let mut replies = vec![];
        for _ in 0..128 {
            let (tx, rx) = oneshot::channel();
            replies.push(rx);
            relay
                .send(Event::Dialog(DialogRequest::ChooseFolder, tx))
                .unwrap();
        }
        let (tx, _) = oneshot::channel();
        assert!(
            relay
                .send(Event::Dialog(DialogRequest::ChooseFolder, tx))
                .is_err()
        );
        for _ in 0..128 {
            relay.send(Event::DialogClosed).unwrap();
        }
        assert!(relay.send(Event::DialogClosed).is_err());
        for _ in 0..128 {
            assert!(matches!(receiver.recv().await, Some(Event::DialogClosed)));
        }
        assert_eq!(relay.0.queues.lock().unwrap().leases, 0);
    }
}
