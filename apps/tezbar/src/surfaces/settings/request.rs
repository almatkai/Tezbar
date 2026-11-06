//! Finite Settings protocol. Diagnostic output intentionally contains no arguments.
use serde_json::{Value, json};
use tezbar_ui_model::{knowledge_settings::Operation, settings::contributions::Action};
#[derive(Clone)]
pub enum AccountAction {
    List,
    SignIn,
    Paste(String),
    Status,
    Switch(String),
    Remove(String),
    Cancel,
}
#[derive(Clone)]
pub enum VoiceAction {
    List,
    Selected,
    Modes,
    Select(String),
    Download(String),
    CancelDownload(String),
    Delete(String),
}
#[derive(Clone)]
pub enum SettingsRequest {
    CompletePath(String),
    PracticeCopy(String),
    Permissions,
    Permission(tezbar_ui_model::utilities::PermissionId),
    Config,
    Save(Value),
    Models {
        provider: String,
        base_url: Option<String>,
        key: Option<String>,
    },
    ProviderStatuses,
    PiExtensions,
    DeviceStart(Option<String>),
    DevicePoll,
    DeviceCancel,
    Account(AccountAction),
    Contribution {
        action: Action,
        args: Vec<Value>,
    },
    Voice(VoiceAction),
    KnowledgeSnapshot,
    Knowledge(Operation),
    ChooseFolder,
    Storage,
    ClipboardConfig,
    ClipboardSet(Value),
    ClearImages,
    Vacuum,
    DryRun,
    SetDryRun(bool),
    Installed,
    ExtensionSchema,
    ExtensionSettings,
    CommandEnabled(String, bool),
    CommandHotkey(String, String),
    Preferences {
        extension: String,
        command: Option<String>,
    },
    SavePreferences {
        extension: String,
        command: Option<String>,
        values: Value,
    },
    Uninstall(String),
    OpenUrl(String),
}
impl std::fmt::Debug for SettingsRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SettingsRequest(<redacted>)")
    }
}
impl SettingsRequest {
    // Replacement identity contains only finite target locators, never a credential value.
    pub(super) fn mutation_scope(&self) -> Option<String> {
        Some(match self {
            Self::Voice(VoiceAction::Select(_)) => "voice-selection".into(),
            Self::Knowledge(Operation::Switch(_)) => "knowledge-model".into(),
            Self::Knowledge(Operation::Speed(_)) => "knowledge-speed".into(),
            Self::Knowledge(Operation::Depth(_)) => "knowledge-depth".into(),
            Self::Knowledge(Operation::RootDepth { id, .. }) => {
                format!("knowledge-root-depth:{id}")
            }
            Self::Knowledge(Operation::RootEnabled { id, .. }) => {
                format!("knowledge-root-enabled:{id}")
            }
            Self::ClipboardSet(v) => {
                format!("clipboard:{:?}", v.as_object()?.keys().collect::<Vec<_>>())
            }
            Self::SetDryRun(_) => "dry-run".into(),
            Self::CommandEnabled(id, _) => format!("command-enabled:{id}"),
            Self::CommandHotkey(id, _) => format!("command-hotkey:{id}"),
            Self::SavePreferences {
                extension, command, ..
            } => format!("preferences:{extension}:{command:?}"),
            Self::Contribution {
                action: Action::ReviewProject,
                ..
            } => "project-review".into(),
            Self::Contribution {
                action: Action::Instructions,
                ..
            } => "instructions".into(),
            Self::Contribution {
                action: Action::Update,
                args,
            } => format!(
                "extension:{}:{:?}",
                args.first()?,
                args.get(1)?.as_object()?.keys().collect::<Vec<_>>()
            ),
            Self::Contribution {
                action: Action::SetEnabled,
                args,
            } => format!("extension-enabled:{}", args.first()?),
            Self::Contribution {
                action: Action::SetTool,
                args,
            } => format!("tool:{}:{}:{}", args.first()?, args.get(1)?, args.get(3)?),
            Self::Contribution {
                action: Action::SetSecret,
                args,
            } => format!("secret:{}:{}:{}", args.first()?, args.get(1)?, args.get(3)?),
            _ => return None,
        })
    }
    pub(super) fn ordered_mutation(&self) -> bool {
        self.recoverable_write()
            || matches!(
                self,
                Self::Contribution {
                    action: Action::ReviewProject,
                    ..
                }
            )
    }
    pub fn interrupt(&self) -> bool {
        matches!(
            self,
            Self::DeviceCancel
                | Self::Account(AccountAction::Cancel)
                | Self::Voice(VoiceAction::CancelDownload(_))
                | Self::Knowledge(Operation::Cancel(_))
        )
    }
    pub fn recoverable_write(&self) -> bool {
        matches!(
            self,
            Self::Save(_)
                | Self::Voice(VoiceAction::Select(_) | VoiceAction::Delete(_))
                | Self::Knowledge(
                    Operation::Switch(_)
                        | Operation::Speed(_)
                        | Operation::Depth(_)
                        | Operation::RootDepth { .. }
                        | Operation::RootEnabled { .. }
                        | Operation::AddRoot(_)
                        | Operation::AddMajor
                        | Operation::RemoveRoot(_)
                )
                | Self::ClipboardSet(_)
                | Self::SetDryRun(_)
                | Self::CommandEnabled(..)
                | Self::CommandHotkey(..)
                | Self::SavePreferences { .. }
                | Self::Uninstall(_)
                | Self::Contribution {
                    action: Action::Add
                        | Action::Update
                        | Action::Remove
                        | Action::SetEnabled
                        | Action::AcceptReview
                        | Action::SetTool
                        | Action::SetSecret
                        | Action::ResetSecrets
                        | Action::ClearAllowance
                        | Action::Instructions
                        | Action::TrustProject
                        | Action::DismissProject,
                    ..
                }
        )
    }
    pub fn durable(&self) -> bool {
        !matches!(
            self,
            Self::Config
                | Self::CompletePath(_)
                | Self::ProviderStatuses
                | Self::PiExtensions
                | Self::Models { .. }
                | Self::Voice(VoiceAction::List | VoiceAction::Selected | VoiceAction::Modes)
                | Self::KnowledgeSnapshot
                | Self::ChooseFolder
                | Self::Storage
                | Self::ClipboardConfig
                | Self::DryRun
                | Self::Installed
                | Self::ExtensionSchema
                | Self::ExtensionSettings
                | Self::Preferences { .. }
                | Self::Permissions
                | Self::Account(AccountAction::List | AccountAction::Status)
                | Self::Contribution {
                    action: Action::List
                        | Action::ParseImport
                        | Action::Logs
                        | Action::OAuthStatus
                        | Action::ComposerItems
                        | Action::PromptResolve
                        | Action::ReadResource,
                    ..
                }
        )
    }
    pub fn wire(self) -> (&'static str, Vec<Value>) {
        match self {
            Self::CompletePath(q) => ("path:complete", vec![json!(q)]),
            Self::PracticeCopy(text) => ("clipboard:write", vec![json!(text)]),
            Self::Permissions => ("permissions:snapshot", vec![]),
            Self::Permission(id) => ("permissions:request", vec![json!(id)]),
            Self::Config => ("llm-config-get", vec![]),
            Self::Save(v) => ("llm-config-set", vec![v]),
            Self::Models {
                provider,
                base_url,
                key,
            } => (
                "llm-list-models",
                vec![json!(provider), json!(base_url), json!(key)],
            ),
            Self::ProviderStatuses => ("llm-provider-statuses", vec![]),
            Self::PiExtensions => ("pi-extensions:list", vec![]),
            Self::DeviceStart(id) => ("github-device-start", vec![json!(id)]),
            Self::DevicePoll => ("github-device-poll", vec![]),
            Self::DeviceCancel => ("github-device-cancel", vec![]),
            Self::Account(a) => match a {
                AccountAction::List => ("ai-accounts:antigravity:list", vec![]),
                AccountAction::SignIn => ("ai-accounts:antigravity:sign-in", vec![]),
                AccountAction::Paste(v) => {
                    ("ai-accounts:antigravity:sign-in-paste", vec![json!(v)])
                }
                AccountAction::Status => ("ai-accounts:antigravity:status", vec![]),
                AccountAction::Switch(v) => ("ai-accounts:antigravity:switch", vec![json!(v)]),
                AccountAction::Remove(v) => ("ai-accounts:antigravity:remove", vec![json!(v)]),
                AccountAction::Cancel => ("ai-accounts:antigravity:cancel", vec![]),
            },
            Self::Contribution { action, args } => (action.channel(), args),
            Self::Voice(a) => match a {
                VoiceAction::List => ("voice:models:list", vec![]),
                VoiceAction::Selected => ("voice:model:get-selected", vec![]),
                VoiceAction::Modes => ("voice:stt:modes", vec![]),
                VoiceAction::Select(id) => {
                    ("voice:model:set-selected", vec![json!({"modelId":id})])
                }
                VoiceAction::Download(id) => ("voice:model:download", vec![json!({"modelId":id})]),
                VoiceAction::CancelDownload(id) => {
                    ("voice:model:cancel-download", vec![json!({"modelId":id})])
                }
                VoiceAction::Delete(id) => ("voice:model:delete", vec![json!({"modelId":id})]),
            },
            Self::KnowledgeSnapshot => ("knowledge:snapshot", vec![]),
            Self::Knowledge(o) => {
                let r = o.request();
                (r.channel, r.args)
            }
            Self::ChooseFolder => ("knowledge:choose-folder", vec![]),
            Self::Storage => ("storage:breakdown", vec![]),
            Self::ClipboardConfig => ("storage:clipboard-config:get", vec![]),
            Self::ClipboardSet(v) => ("storage:clipboard-config:set", vec![v]),
            Self::ClearImages => ("storage:clear-clipboard-images", vec![]),
            Self::Vacuum => ("storage:vacuum-search-db", vec![]),
            Self::DryRun => ("safety:dry-run:get", vec![]),
            Self::SetDryRun(v) => ("safety:dry-run:set", vec![json!(v)]),
            Self::Installed => ("extension:list", vec![]),
            Self::ExtensionSchema => ("get-installed-extensions-settings-schema", vec![]),
            Self::ExtensionSettings => ("get-settings", vec![]),
            Self::CommandEnabled(id, v) => ("toggle-command-enabled", vec![json!(id), json!(v)]),
            Self::CommandHotkey(id, v) => ("update-command-hotkey", vec![json!(id), json!(v)]),
            Self::Preferences { extension, command } => (
                "preferences:setup",
                vec![json!({"extensionId":extension,"commandName":command})],
            ),
            Self::SavePreferences {
                extension,
                command,
                values,
            } => (
                "preferences:set",
                vec![json!({"extensionId":extension,"commandName":command,"values":values})],
            ),
            Self::Uninstall(id) => ("extension:uninstall", vec![json!(id)]),
            Self::OpenUrl(url) => ("open-external-url", vec![json!(url)]),
        }
    }
}
