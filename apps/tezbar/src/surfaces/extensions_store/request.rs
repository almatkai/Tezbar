//! Finite existing-core protocol. Payloads can contain passwords and clipboard text.
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, pin::Pin};

pub type ExtensionFuture = Pin<Box<dyn Future<Output = Result<Value, String>> + Send>>;
pub trait ExtensionTransport: Send + Sync + 'static {
    fn send(&self, request: ExtensionRequest) -> ExtensionFuture;
    /// Wait for already admitted work, including disposal, independently of view visibility.
    fn drain(&self) -> ExtensionFuture;
}

#[derive(Clone)]
pub enum ExtensionRequest {
    List,
    Search {
        query: String,
        offset: usize,
        limit: usize,
    },
    Install(String),
    Uninstall(String),
    Run {
        extension: String,
        command: String,
        arguments: BTreeMap<String, String>,
    },
    InvokeOwned {
        session: String,
        action: String,
        values: Option<BTreeMap<String, Value>>,
        source: u64,
    },
    Invoke {
        session: String,
        action: String,
        values: Option<BTreeMap<String, Value>>,
    },
    SearchText {
        session: String,
        text: String,
    },
    Refresh(String),
    RefreshOwned {
        session: String,
        view_revision: u64,
    },
    LoadMore(String),
    Dispose(String),
    PreferenceSetup {
        extension: String,
        command: Option<String>,
    },
    Preferences {
        extension: String,
        command: Option<String>,
    },
    SavePreferences {
        extension: String,
        command: Option<String>,
        values: BTreeMap<String, Value>,
    },
    Open(String),
    Copy(String),
}
impl std::fmt::Debug for ExtensionRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExtensionRequest(<redacted>)")
    }
}
impl ExtensionRequest {
    pub fn wire(self) -> (&'static str, Vec<Value>) {
        match self {
            Self::List => ("extension:list", vec![]),
            Self::Search {
                query,
                offset,
                limit,
            } => (
                "extension:search-store",
                vec![json!({"query":query,"offset":offset,"limit":limit})],
            ),
            Self::Install(id) => ("extension:install", vec![json!(id)]),
            Self::Uninstall(id) => ("extension:uninstall", vec![json!(id)]),
            Self::Run {
                extension,
                command,
                arguments,
            } => (
                "extension:run-command",
                vec![
                    json!({"extensionId":extension,"commandName":command,"argumentValues":arguments}),
                ],
            ),
            Self::InvokeOwned {
                session,
                action,
                values,
                source,
            } => (
                "extension:invoke-action",
                vec![
                    json!({"sessionId":session,"actionId":action,"formValues":values,"hostSource":source}),
                ],
            ),
            Self::Invoke {
                session,
                action,
                values,
            } => (
                "extension:invoke-action",
                vec![json!({"sessionId":session,"actionId":action,"formValues":values})],
            ),
            Self::SearchText { session, text } => (
                "extension:search-text-changed",
                vec![json!({"sessionId":session,"searchText":text})],
            ),
            Self::RefreshOwned {
                session,
                view_revision,
            } => (
                "extension:refresh-session",
                vec![json!({"sessionId":session,"viewRevision":view_revision})],
            ),
            Self::Refresh(session) => (
                "extension:refresh-session",
                vec![json!({"sessionId":session})],
            ),
            Self::LoadMore(session) => ("extension:load-more", vec![json!({"sessionId":session})]),
            Self::Dispose(session) => (
                "extension:dispose-session",
                vec![json!({"sessionId":session})],
            ),
            Self::PreferenceSetup { extension, command } => (
                "preferences:setup",
                vec![json!({"extensionId":extension,"commandName":command})],
            ),
            Self::Preferences { extension, command } => (
                "preferences:get",
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
            Self::Open(url) => ("shell:open", vec![json!(url)]),
            Self::Copy(text) => ("clipboard:write", vec![json!(text)]),
        }
    }
}

pub fn success(value: &Value) -> Result<(), String> {
    if value == &Value::Bool(false) || value.get("ok") == Some(&Value::Bool(false)) {
        // Core messages can include payloads; the presentation does not echo them.
        Err("Extension request failed. Retry or inspect extension diagnostics.".into())
    } else {
        Ok(())
    }
}
