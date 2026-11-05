//! Native status items and hotkeys. GUI objects stay on the application's main thread.
//! The host supplies dispatch and pumps menu/hotkey events from its own event loop.
use global_hotkey::{hotkey::HotKey, GlobalHotKeyManager};
use std::str::FromStr;
use std::{cell::RefCell, collections::HashMap, sync::Arc, time::Duration};
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
}
impl NativeDesktop {
    pub fn new(execute: MainThreadExecutor) -> Self {
        Self { execute }
    }
    fn call<T: Send + 'static>(
        &self,
        op: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        (self.execute)(Box::new(move || {
            let _ = tx.send(op());
        }))?;
        rx.recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?
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
    tray: tray_icon::TrayIcon,
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
                if spec.items.len() > 128 {
                    return Err("Too many tray items".into());
                }
                for item in spec.items {
                    let action = item["id"].as_str().ok_or("Tray action id required")?;
                    let os_id = format!("{id}-{action}");
                    if actions.insert(os_id.clone(), action.to_owned()).is_some() {
                        return Err("Duplicate tray action".into());
                    }
                    let title = item["title"].as_str().ok_or("Tray title required")?;
                    let enabled = item["enabled"].as_bool().unwrap_or(true);
                    if let Some(checked) = item["checked"].as_bool() {
                        menu.append(&muda::CheckMenuItem::with_id(
                            os_id, title, enabled, checked, None,
                        ))
                        .map_err(|e| e.to_string())?
                    } else {
                        menu.append(&muda::MenuItem::with_id(os_id, title, enabled, None))
                            .map_err(|e| e.to_string())?
                    }
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
                        tray,
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
        let s = parse_shortcut("Alt+F19").unwrap();
        assert_eq!(d.register(&s).unwrap_err(), "fixture executor closed");
        assert_eq!(d.remove("fixture").unwrap_err(), "fixture executor closed");
    }
}
