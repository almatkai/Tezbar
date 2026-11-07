//! Finite first-party requests; core:* routes remain inaccessible to generic RPC.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tezbar_ui_model::terminal::{Defaults, KeepAliveFor, SaveFor, Session};

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Create {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restore_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restore_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub cols: u16,
    pub rows: u16,
    #[serde(flatten)]
    pub defaults: Defaults,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save_for: Option<SaveFor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_alive_for: Option<KeepAliveFor>,
}
#[derive(Clone)]
pub enum TerminalRequest {
    Replay {
        token: String,
        position: u64,
    },
    Release(String),
    Input {
        id: String,
        generation: String,
        bytes: Vec<u8>,
    },
    List,
    Prompt,
    Create(Create),
    Attach {
        id: String,
        cols: u16,
        rows: u16,
    },
    DetachGeneration {
        id: String,
        generation: String,
    },
    Resize {
        id: String,
        cols: u16,
        rows: u16,
    },
    Cwd(String),
    Update(Update),
    Kill(String),
    Delete(String),
    DeleteHistory(String),
    PruneHistory(Vec<String>),
}
impl std::fmt::Debug for TerminalRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TerminalRequest([redacted])")
    }
}
impl TerminalRequest {
    pub fn retained_bytes(&self) -> usize {
        let optional = |s: &Option<String>| s.as_ref().map_or(0, String::len);
        match self {
            Self::Create(r) => {
                optional(&r.cwd)
                    + optional(&r.initial_command)
                    + optional(&r.restore_session_id)
                    + optional(&r.restore_command)
                    + optional(&r.name)
                    + 128
            }
            Self::Update(r) => {
                r.session_id.len()
                    + optional(&r.name)
                    + optional(&r.cwd)
                    + optional(&r.last_command)
                    + 128
            }
            Self::Input { bytes, .. } => bytes.len(),
            Self::Attach { id, .. } | Self::Resize { id, .. } => id.len() + 64,
            Self::Cwd(id) | Self::Kill(id) | Self::Delete(id) | Self::DeleteHistory(id) => {
                id.len() + 64
            }
            Self::PruneHistory(ids) => ids.iter().map(|s| s.len() + 32).sum(),
            _ => 64,
        }
    }
    pub fn wire(self) -> (&'static str, Vec<Value>) {
        use base64::Engine;
        match self {
            Self::Replay { token, position } => (
                "core:terminal:replay-read",
                vec![json!({"replay":token,"position":position,"limit":65536})],
            ),
            Self::DetachGeneration { id, generation } => (
                "core:terminal:stream-detach",
                vec![json!({"sessionId":id,"generation":generation})],
            ),
            Self::Release(token) => (
                "core:terminal:replay-release",
                vec![json!({"replay":token})],
            ),
            Self::Input {
                id,
                generation,
                bytes,
            } => (
                "core:terminal:input",
                vec![
                    json!({"sessionId":id,"generation":generation,"bytes":base64::engine::general_purpose::STANDARD.encode(bytes)}),
                ],
            ),
            Self::List => ("terminal:list", vec![]),
            Self::Prompt => ("terminal:get-prompt-info", vec![]),
            Self::Create(r) => ("terminal:create", vec![json!(r)]),
            Self::Attach { id, cols, rows } => (
                "core:terminal:stream-attach",
                vec![json!({"sessionId":id,"cols":cols,"rows":rows})],
            ),
            Self::Resize { id, cols, rows } => (
                "terminal:resize",
                vec![json!({"sessionId":id,"cols":cols,"rows":rows})],
            ),
            Self::Cwd(id) => ("terminal:get-cwd", vec![json!({"sessionId":id})]),
            Self::Update(r) => ("terminal:update", vec![json!(r)]),
            Self::Kill(id) => ("terminal:kill", vec![json!({"sessionId":id})]),
            Self::Delete(id) => ("terminal:delete", vec![json!({"sessionId":id})]),
            Self::DeleteHistory(id) => ("terminal:delete-history", vec![json!({"sessionId":id})]),
            Self::PruneHistory(ids) => ("terminal:prune-history", vec![json!({"sessionIds":ids})]),
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attached {
    pub session_id: String,
    pub summary: Session,
    #[serde(default)]
    pub generation: String,
    #[serde(default)]
    pub replay: String,
    #[serde(default)]
    pub watermark: u64,
    #[serde(default)]
    pub origin: Option<u64>,
}
pub type Reply = Result<Value, String>;
pub trait TerminalTransport: Send + Sync + 'static {
    /// Same CoreClient/Tokio admission owner. Effectful work must survive UI cancellation
    /// long enough to return its identity and detach an abandoned create/attach.
    fn send(
        &self,
        request: TerminalRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Reply> + Send>>;
}
