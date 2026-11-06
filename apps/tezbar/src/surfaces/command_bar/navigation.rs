//! Lossless handoff to M6–M10. The app owns one inbox; a destination takes its matching intent.
//! Nonces are monotonic per launcher owner. Navigation is emitted by activation, never render.
use serde_json::Value;
use tezbar_ui_model::router::{NewItem, Surface};
#[derive(Clone, Debug, PartialEq)]
pub enum Payload {
    None,
    Ai {
        prompt: Option<String>,
        cwd: Option<String>,
        resume: Option<String>,
        capture: bool,
    },
    Terminal {
        command: Option<String>,
        cwd: Option<String>,
        session: Option<String>,
        defaults: tezbar_ui_model::ui_state::TerminalDefaults,
    },
    Vault {
        item_id: String,
    },
    Note {
        created_at: u64,
    },
    Extension {
        result: Value,
    },
    New(NewItem),
    Settings {
        tab: String,
    },
    Update(UpdateAction),
}
#[derive(Clone, Debug, PartialEq)]
pub enum UpdateAction {
    Check,
    Download,
    Restart,
    Installer(String),
}
pub fn surface_title(surface: &Surface) -> &'static str {
    match surface {
        Surface::Command => "Commands",
        Surface::AiChat => "AI Chat",
        Surface::Terminal => "Terminal",
        Surface::ExtensionRuntime => "Extension",
        Surface::Settings(_) => "Settings",
        Surface::Permissions => "Permissions",
        Surface::Extensions => "Extensions",
        Surface::SystemMonitor => "System Monitor",
        Surface::Indexing => "Indexing",
        Surface::Clipboard => "Clipboard History",
        Surface::Snippets => "Snippets",
        Surface::Notes => "Notes",
        Surface::Vault => "Vault",
        Surface::Emoji => "Emoji",
        Surface::ReleaseNotes => "Release Notes",
        Surface::Onboarding => "Welcome to Tezbar",
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Navigation {
    pub nonce: u64,
    pub surface: Surface,
    pub payload: Payload,
}
#[derive(Default)]
pub struct NavigationInbox {
    pending: std::collections::VecDeque<Navigation>,
}
impl gpui::Global for NavigationInbox {}
impl NavigationInbox {
    /// Every admitted intent remains until its matching surface consumes/acknowledges it.
    /// A full inbox reports backpressure instead of silently replacing an older intent.
    pub fn put(&mut self, intent: Navigation) -> Result<(), String> {
        if self.pending.contains(&intent) {
            return Ok(());
        }
        if self.pending.len() >= 16 {
            return Err("That screen is still opening. Try again shortly.".into());
        }
        self.pending.push_back(intent);
        Ok(())
    }
    pub fn pending(&self) -> Option<&Navigation> {
        self.pending.front()
    }
    pub fn take(&mut self, surface: &Surface) -> Option<Navigation> {
        let index = self
            .pending
            .iter()
            .position(|intent| &intent.surface == surface)?;
        self.pending.remove(index)
    }
    pub fn acknowledge(&mut self, intent: &Navigation) -> bool {
        self.pending
            .iter()
            .position(|pending| pending == intent)
            .and_then(|index| self.pending.remove(index))
            .is_some()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UpdateIntent {
    pub nonce: u64,
    pub action: UpdateAction,
}
/// Updater work has its own typed owner; it can never be consumed as a Settings route.
#[derive(Default)]
pub struct UpdateInbox {
    next_nonce: u64,
    pending: std::collections::VecDeque<UpdateIntent>,
}
impl gpui::Global for UpdateInbox {}
impl UpdateInbox {
    pub fn put(&mut self, action: UpdateAction) -> Result<u64, String> {
        if self.pending.len() >= 16 {
            return Err("Update requests are still pending. Try again shortly.".into());
        }
        let nonce = self
            .next_nonce
            .checked_add(1)
            .ok_or("Update request limit reached")?;
        self.next_nonce = nonce;
        self.pending.push_back(UpdateIntent { nonce, action });
        Ok(nonce)
    }
    pub fn take(&mut self) -> Option<UpdateIntent> {
        self.pending.pop_front()
    }
}

impl Navigation {
    /// Canonical app:open-surface strings emitted by NativeCore command shortcuts.
    pub fn from_core(value: &Value, nonce: u64) -> Option<Self> {
        let name = value.as_str().or_else(|| value["surface"].as_str())?;
        let (surface, payload) = match name {
            "command" => (Surface::Command, Payload::None),
            "ai-chat" => (Surface::AiChat, Payload::None),
            "terminal" => (Surface::Terminal, Payload::None),
            "extension-runtime" => (
                Surface::ExtensionRuntime,
                Payload::Extension {
                    result: value.clone(),
                },
            ),
            "settings" => (
                Surface::Settings("general".into()),
                Payload::Settings {
                    tab: "general".into(),
                },
            ),
            "settings-ai" => (
                Surface::Settings("ai".into()),
                Payload::Settings { tab: "ai".into() },
            ),
            "settings-extensions" => (
                Surface::Settings("extensions".into()),
                Payload::Settings {
                    tab: "extensions".into(),
                },
            ),
            "permissions" => (Surface::Permissions, Payload::None),
            "extensions" => (Surface::Extensions, Payload::None),
            "system-monitor" => (Surface::SystemMonitor, Payload::None),
            "indexing" => (Surface::Indexing, Payload::None),
            "clipboard" => (Surface::Clipboard, Payload::None),
            "snippets" => (Surface::Snippets, Payload::None),
            "notes" => (Surface::Notes, Payload::None),
            "vault" => (Surface::Vault, Payload::None),
            "emoji-picker" => (Surface::Emoji, Payload::None),
            "release-notes" => (Surface::ReleaseNotes, Payload::None),
            "onboarding" => (Surface::Onboarding, Payload::None),
            _ => return None,
        };
        Some(Self {
            nonce,
            surface,
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn intent(nonce: u64, surface: Surface) -> Navigation {
        Navigation {
            nonce,
            surface,
            payload: Payload::None,
        }
    }
    #[test]
    fn pending_routes_are_preserved_per_surface_and_in_order() {
        let mut inbox = NavigationInbox::default();
        inbox.put(intent(1, Surface::Notes)).unwrap();
        inbox
            .put(intent(2, Surface::Settings("ai".into())))
            .unwrap();
        inbox.put(intent(3, Surface::Notes)).unwrap();
        assert_eq!(inbox.take(&Surface::Notes).unwrap().nonce, 1);
        assert_eq!(inbox.take(&Surface::Notes).unwrap().nonce, 3);
        assert_eq!(
            inbox.take(&Surface::Settings("ai".into())).unwrap().nonce,
            2
        );
        assert!(inbox.pending().is_none());
    }
    #[test]
    fn full_route_inbox_rejects_new_work_without_dropping_admitted_routes() {
        let mut inbox = NavigationInbox::default();
        for nonce in 1..=16 {
            inbox.put(intent(nonce, Surface::Notes)).unwrap();
        }
        assert!(inbox.put(intent(17, Surface::Notes)).is_err());
        for nonce in 1..=16 {
            assert_eq!(inbox.take(&Surface::Notes).unwrap().nonce, nonce);
        }
        let route = intent(18, Surface::Notes);
        inbox.put(route.clone()).unwrap();
        assert!(inbox.acknowledge(&route));
        assert!(!inbox.acknowledge(&route));
    }
    #[test]
    fn update_actions_have_a_separate_ordered_monotonic_owner() {
        let mut navigation = NavigationInbox::default();
        let mut updates = UpdateInbox::default();
        let settings = Surface::Settings("general".into());
        updates.put(UpdateAction::Download).unwrap();
        navigation.put(intent(10, settings.clone())).unwrap();
        updates.put(UpdateAction::Restart).unwrap();
        assert_eq!(navigation.take(&settings).unwrap().nonce, 10);
        assert!(navigation.take(&settings).is_none());
        assert_eq!(
            updates.take(),
            Some(UpdateIntent {
                nonce: 1,
                action: UpdateAction::Download
            })
        );
        assert_eq!(
            updates.take(),
            Some(UpdateIntent {
                nonce: 2,
                action: UpdateAction::Restart
            })
        );
        assert!(updates.take().is_none());
        assert_eq!(updates.put(UpdateAction::Check).unwrap(), 3);
    }
}
