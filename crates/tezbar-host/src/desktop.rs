//! Native status items and hotkeys. GUI objects stay on the application's main thread.
//! The host supplies dispatch and pumps menu/hotkey events from its own event loop.
use global_hotkey::{hotkey::HotKey, GlobalHotKeyManager};
use std::str::FromStr;
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    time::Duration,
};
use tezbar_core::host::*;
pub fn parse_shortcut(raw: &str) -> Result<Shortcut, String> {
    let key = HotKey::from_str(raw).map_err(|e| e.to_string())?;
    Ok(Shortcut {
        id: key.id(),
        accelerator: key.to_string(),
    })
}
pub type MainThreadExecutor =
    Arc<dyn Fn(Box<dyn FnOnce() + Send>) -> Result<(), String> + Send + Sync>;
#[derive(Clone)]
pub struct NativeDesktop {
    execute: MainThreadExecutor,
    main_thread: std::thread::ThreadId,
}
impl NativeDesktop {
    /// Construct on the application's main thread, before handing clones to core.
    pub fn new(execute: MainThreadExecutor) -> Self {
        Self {
            execute,
            main_thread: std::thread::current().id(),
        }
    }
    fn call<T: Send + 'static>(
        &self,
        op: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        self.call_with_timeout(op, Duration::from_secs(5))
    }
    fn call_with_timeout<T: Send + 'static>(
        &self,
        op: impl FnOnce() -> Result<T, String> + Send + 'static,
        timeout: Duration,
    ) -> Result<T, String> {
        if std::thread::current().id() == self.main_thread {
            return op();
        }
        const QUEUED: u8 = 0;
        const RUNNING: u8 = 1;
        const CANCELLED: u8 = 2;
        let state = Arc::new(AtomicU8::new(QUEUED));
        let queued_state = state.clone();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let dispatched = (self.execute)(Box::new(move || {
            if queued_state
                .compare_exchange(QUEUED, RUNNING, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return;
            }
            let _ = tx.send(op());
        }));
        if let Err(error) = dispatched {
            if state
                .compare_exchange(QUEUED, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Err(error);
            }
            // Dispatch may already have started the operation. Observe its real
            // outcome instead of reporting a failure followed by a late effect.
            return rx.recv().map_err(|e| e.to_string())?;
        }
        match rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(error) => {
                if state
                    .compare_exchange(QUEUED, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    Err(error.to_string())
                } else {
                    rx.recv().map_err(|e| e.to_string())?
                }
            }
        }
    }
    /// Called on the main thread by the owning run loop. Returns whether an item handled it.
    pub fn dispatch_menu_event(id: &str) -> bool {
        let callback = STATE.with(|state| {
            state.borrow().items.values().find_map(|item| {
                item.actions
                    .get(id)
                    .map(|action| (item.callback.clone(), action.clone()))
            })
        });
        if let Some((callback, action)) = callback {
            callback(action);
            true
        } else {
            false
        }
    }
}
struct Item {
    _tray: tray_icon::TrayIcon,
    actions: HashMap<String, String>,
    callback: StatusCallback,
}
#[derive(Default)]
struct State {
    hotkeys: Option<GlobalHotKeyManager>,
    items: HashMap<String, Item>,
}
thread_local! {static STATE:RefCell<State>=RefCell::new(State::default());}
impl ShortcutRegistry for NativeDesktop {
    fn available(&self) -> bool {
        true
    }
    fn parse(&self, raw: &str) -> Result<Shortcut, String> {
        parse_shortcut(raw)
    }
    fn register(&self, s: &Shortcut) -> Result<(), String> {
        let s = s.clone();
        self.call(move || {
            STATE.with(|state| {
                let mut state = state.borrow_mut();
                if state.hotkeys.is_none() {
                    state.hotkeys = Some(GlobalHotKeyManager::new().map_err(|e| e.to_string())?)
                }
                state
                    .hotkeys
                    .as_ref()
                    .unwrap()
                    .register(HotKey::from_str(&s.accelerator).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())
            })
        })
    }
    fn unregister(&self, s: &Shortcut) -> Result<(), String> {
        let s = s.clone();
        self.call(move || {
            STATE.with(|state| {
                let state = state.borrow();
                state
                    .hotkeys
                    .as_ref()
                    .ok_or("Hotkey manager unavailable")?
                    .unregister(HotKey::from_str(&s.accelerator).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())
            })
        })
    }
}
impl StatusItems for NativeDesktop {
    fn set(&self, id: &str, spec: StatusItem, callback: StatusCallback) -> Result<(), String> {
        let id = id.to_owned();
        self.call(move || {
            STATE.with(|state| {
                let mut state = state.borrow_mut();
                let menu = muda::Menu::new();
                let mut actions = HashMap::new();
                if state.items.len() >= 128 && !state.items.contains_key(&id) {
                    return Err("Too many status items".into());
                }
                fn build(
                    items: &[serde_json::Value],
                    scope: &str,
                    actions: &mut HashMap<String, String>,
                    depth: usize,
                    count: &mut usize,
                ) -> Result<Vec<Box<dyn muda::IsMenuItem>>, String> {
                    if depth > 8 {
                        return Err("Tray nesting limit".into());
                    }
                    let mut out: Vec<Box<dyn muda::IsMenuItem>> = Vec::new();
                    for item in items {
                        *count += 1;
                        if *count > 128 {
                            return Err("Too many tray items".into());
                        }
                        let action = item["id"].as_str().ok_or("Tray action id required")?;
                        let os_id = format!("{scope}-{action}");
                        let title = item["title"].as_str().ok_or("Tray title required")?;
                        let enabled = item["enabled"].as_bool().unwrap_or(true);
                        match item["kind"].as_str() {
                            Some("separator") => {
                                out.push(Box::new(muda::PredefinedMenuItem::separator()))
                            }
                            Some("section") => out
                                .push(Box::new(muda::MenuItem::with_id(os_id, title, false, None))),
                            Some("submenu") => {
                                let sub = muda::Submenu::with_id(os_id, title, enabled);
                                for child in build(
                                    item["items"].as_array().ok_or("Submenu items required")?,
                                    scope,
                                    actions,
                                    depth + 1,
                                    count,
                                )? {
                                    sub.append(child.as_ref()).map_err(|e| e.to_string())?;
                                }
                                out.push(Box::new(sub));
                            }
                            None => {
                                if actions.insert(os_id.clone(), action.into()).is_some() {
                                    return Err("Duplicate tray action".into());
                                }
                                if let Some(checked) = item["checked"].as_bool() {
                                    out.push(Box::new(muda::CheckMenuItem::with_id(
                                        os_id, title, enabled, checked, None,
                                    )));
                                } else {
                                    out.push(Box::new(muda::MenuItem::with_id(
                                        os_id, title, enabled, None,
                                    )));
                                }
                            }
                            _ => return Err("Unknown tray item kind".into()),
                        }
                    }
                    Ok(out)
                }
                for item in build(&spec.items, &id, &mut actions, 0, &mut 0)? {
                    menu.append(item.as_ref()).map_err(|e| e.to_string())?;
                }
                let mut builder = tray_icon::TrayIconBuilder::new()
                    .with_id(&id)
                    .with_menu(Box::new(menu))
                    .with_title(spec.title)
                    .with_tooltip(spec.tooltip);
                if let Some((bytes, w, h)) = spec.icon {
                    if w > 512 || h > 512 {
                        return Err("Tray image too large".into());
                    }
                    builder = builder.with_icon(
                        tray_icon::Icon::from_rgba(bytes, w, h).map_err(|e| e.to_string())?,
                    );
                }
                let tray = builder.build().map_err(|e| e.to_string())?;
                state.items.insert(
                    id,
                    Item {
                        _tray: tray,
                        actions,
                        callback,
                    },
                );
                Ok(())
            })
        })
    }
    fn remove(&self, id: &str) -> Result<(), String> {
        let id = id.to_owned();
        self.call(move || {
            STATE.with(|state| state.borrow_mut().items.remove(&id));
            Ok(())
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parsing_matches_existing_backend_without_registering() {
        let s = parse_shortcut("Alt+Space").unwrap();
        assert_eq!(s.id, HotKey::from_str("Alt+Space").unwrap().id());
        assert!(parse_shortcut("invalid++thing").is_err());
    }
    #[test]
    fn dispatcher_failures_return_without_creating_os_objects() {
        let d = NativeDesktop::new(Arc::new(|_| Err("fixture executor closed".into())));
        std::thread::spawn(move || {
            let s = parse_shortcut("Alt+F19").unwrap();
            assert_eq!(d.register(&s).unwrap_err(), "fixture executor closed");
            assert_eq!(d.remove("fixture").unwrap_err(), "fixture executor closed");
        })
        .join()
        .unwrap();
    }
    #[test]
    fn main_thread_calls_execute_inline_without_queueing() {
        let d = NativeDesktop::new(Arc::new(|_| panic!("main-thread call must not queue")));
        assert_eq!(d.call(|| Ok(42)).unwrap(), 42);
    }
    #[test]
    fn timed_out_queued_operations_have_no_late_effects() {
        let queued = Arc::new(std::sync::Mutex::new(None));
        let sink = queued.clone();
        let d = NativeDesktop::new(Arc::new(move |op| {
            *sink.lock().unwrap() = Some(op);
            Ok(())
        }));
        let effects = Arc::new(AtomicU8::new(0));
        let counter = effects.clone();
        let error = std::thread::spawn(move || {
            d.call_with_timeout(
                move || {
                    counter.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                },
                Duration::from_millis(10),
            )
        })
        .join()
        .unwrap();
        assert!(error.is_err());
        queued.lock().unwrap().take().unwrap()();
        assert_eq!(effects.load(Ordering::Relaxed), 0);
    }
    #[test]
    fn started_operation_returns_its_outcome_instead_of_a_false_timeout() {
        let started = Arc::new(std::sync::Barrier::new(2));
        let dispatch_started = started.clone();
        let d = NativeDesktop::new(Arc::new(move |op| {
            std::thread::spawn(op);
            dispatch_started.wait();
            Ok(())
        }));
        let caller = std::thread::spawn(move || {
            d.call_with_timeout(
                move || {
                    started.wait();
                    std::thread::sleep(Duration::from_millis(20));
                    Ok(42)
                },
                Duration::from_millis(5),
            )
        });
        assert_eq!(caller.join().unwrap().unwrap(), 42);
    }
}
