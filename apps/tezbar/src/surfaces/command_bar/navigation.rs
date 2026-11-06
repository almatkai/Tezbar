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
    pending: Option<Navigation>,
}
impl gpui::Global for NavigationInbox {}
impl NavigationInbox {
    pub fn put(&mut self, intent: Navigation) {
        self.pending = Some(intent);
    }
    pub fn pending(&self) -> Option<&Navigation> {
        self.pending.as_ref()
    }
    pub fn take(&mut self, surface: &Surface) -> Option<Navigation> {
        if self.pending.as_ref().is_some_and(|n| &n.surface == surface) {
            self.pending.take()
        } else {
            None
        }
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
