//! Surface navigation and inactivity, driven solely by injected monotonic milliseconds.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Surface {
    Command,
    AiChat,
    Terminal,
    ExtensionRuntime,
    Settings(String),
    Permissions,
    Extensions,
    SystemMonitor,
    Indexing,
    Clipboard,
    Snippets,
    Notes,
    Vault,
    Emoji,
    ReleaseNotes,
    Onboarding,
}
#[derive(Clone, Debug, PartialEq)]
pub enum NewItem {
    Chat,
    Snippet,
    Vault,
    Note,
}
pub fn new_item(surface: &Surface) -> NewItem {
    match surface {
        Surface::AiChat => NewItem::Chat,
        Surface::Snippets => NewItem::Snippet,
        Surface::Vault => NewItem::Vault,
        _ => NewItem::Note,
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Navigate(Surface),
    NewItem(NewItem),
    Hide,
    /// Resolve the requested action against the current updater status at execution time.
    UpdateAction,
    QuitConfirm,
    ActivateDeepSearch(String),
    RecordUsage(String),
}
#[derive(Debug)]
pub struct Router {
    pub surface: Surface,
    pub ai_mounted: bool,
    pub visible: bool,
    pub epoch: u64,
    deadline: Option<u64>,
    pub extension_timeout: u64,
    pub ai_timeout: u64,
    pub terminal_timeout: u64,
}
impl Default for Router {
    fn default() -> Self {
        Self {
            surface: Surface::Command,
            ai_mounted: false,
            visible: true,
            epoch: 0,
            deadline: None,
            extension_timeout: 300_000,
            ai_timeout: 300_000,
            terminal_timeout: 300_000,
        }
    }
}
impl Router {
    pub fn navigate(&mut self, s: Surface, now: u64) -> Vec<Effect> {
        self.epoch += 1;
        self.ai_mounted |= s == Surface::AiChat;
        self.surface = s.clone();
        self.activity(now);
        vec![Effect::Navigate(s)]
    }
    pub fn activity(&mut self, now: u64) {
        let timeout = match self.surface {
            Surface::ExtensionRuntime => self.extension_timeout,
            Surface::AiChat => self.ai_timeout,
            Surface::Terminal => self.terminal_timeout,
            _ => 0,
        };
        self.deadline = (timeout > 0).then(|| now.saturating_add(timeout));
    }
    pub fn tick(&mut self, now: u64) -> Vec<Effect> {
        if self.deadline.is_some_and(|t| now >= t) {
            self.navigate(Surface::Command, now)
        } else {
            vec![]
        }
    }
    pub fn hide(&mut self) -> Vec<Effect> {
        self.visible = false;
        vec![Effect::Hide]
    }
    pub fn invoke(&mut self, id: &str, query: &str, now: u64) -> Option<Vec<Effect>> {
        let s = match id {
            "open-settings" => Surface::Settings("general".into()),
            "open-providers" => Surface::Settings("ai".into()),
            "open-settings-updates" => Surface::Settings("general".into()),
            "open-extensions-settings" => Surface::Settings("extensions".into()),
            "open-extensions" => Surface::Extensions,
            "open-snippets" => Surface::Snippets,
            "open-vault" => Surface::Vault,
            "open-notes" | "open-quick-notes" => Surface::Notes,
            "open-emoji-picker" => Surface::Emoji,
            "open-indexing" => Surface::Indexing,
            "start-onboarding" => Surface::Onboarding,
            "show-updated-version" => Surface::ReleaseNotes,
            "show-system-monitor"
            | "show-cpu-info"
            | "show-macos-version"
            | "show-memory-info"
            | "show-disk-usage"
            | "show-battery-status" => Surface::SystemMonitor,
            "open-clipboard-history" => Surface::Clipboard,
            "download-app-update" | "restart-app-update" | "check-for-updates" => {
                return Some(vec![Effect::UpdateAction]);
            }
            "quit-tezbar" => return Some(vec![Effect::QuitConfirm]),
            "activate-deep-search" => return Some(vec![Effect::ActivateDeepSearch(query.into())]),
            _ => return None,
        };
        Some(self.navigate(s, now))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parent_review_settings_command_selects_general() {
        let mut r = Router::default();
        assert_eq!(
            r.invoke("open-settings", "", 0),
            Some(vec![Effect::Navigate(Surface::Settings("general".into()))])
        );
        assert_eq!(
            r.invoke("open-providers", "", 0),
            Some(vec![Effect::Navigate(Surface::Settings("ai".into()))])
        );
    }
    #[test]
    fn parent_review_update_commands_share_current_status_dispatch() {
        for command in [
            "check-for-updates",
            "download-app-update",
            "restart-app-update",
        ] {
            assert_eq!(
                Router::default().invoke(command, "", 0),
                Some(vec![Effect::UpdateAction])
            );
        }
    }
    #[test]
    fn new_item_routing() {
        assert_eq!(new_item(&Surface::Vault), NewItem::Vault);
        for (s, item) in [
            (Surface::AiChat, NewItem::Chat),
            (Surface::Snippets, NewItem::Snippet),
            (Surface::Command, NewItem::Note),
            (Surface::Clipboard, NewItem::Note),
        ] {
            assert_eq!(new_item(&s), item);
        }
    }
    #[test]
    fn timeout_resets_on_activity_and_zero_disables() {
        let mut r = Router::default();
        r.ai_timeout = 100;
        r.navigate(Surface::AiChat, 0);
        r.activity(99);
        assert!(r.tick(100).is_empty());
        assert_eq!(r.tick(199), vec![Effect::Navigate(Surface::Command)]);
        assert!(r.ai_mounted);
        r.ai_timeout = 0;
        r.navigate(Surface::AiChat, 200);
        assert!(r.tick(u64::MAX).is_empty());
    }
}
