//! The finite M6 request vocabulary. No arbitrary channel or second runtime.
use serde_json::{Value, json};
use tezbar_ui_model::{notes, utilities::SnippetDraft, vault};
#[derive(Clone)]
pub enum UtilityRequest {
    ReadText,
    ClipboardList,
    ClipboardRestore(String),
    ClipboardDelete(String),
    ClipboardPin(String),
    ClipboardReveal(String),
    ClipboardImage(String),
    ClipboardClear,
    WriteText(String),
    WritePng(String),
    SnippetsList,
    SnippetWrite {
        id: Option<String>,
        draft: SnippetDraft,
    },
    SnippetDelete(String),
    SnippetCopy(String),
    NotesList,
    NoteCreate(String),
    NoteSave {
        id: u64,
        html: String,
    },
    NoteDelete(u64),
    VaultStatus,
    VaultUnlock,
    VaultLock,
    VaultList,
    VaultGet(String),
    VaultSave(vault::SaveInput),
    VaultDelete(String),
    VaultCopy(String),
    SystemStats,
    KnowledgeSnapshot,
    Sources {
        query: String,
        offset: usize,
        limit: usize,
    },
    IndexStart,
    IndexPause,
    IndexResume,
    Permissions,
    Permission(tezbar_ui_model::utilities::PermissionId),
    BackgroundTasks,
    Open(String),
}
impl std::fmt::Debug for UtilityRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UtilityRequest([redacted])")
    }
}
impl UtilityRequest {
    pub fn wire(self) -> (&'static str, Vec<Value>) {
        match self {
            Self::ReadText => ("clipboard:read", vec![]),
            Self::ClipboardList => ("clipboard:list", vec![]),
            Self::ClipboardRestore(id) => ("clipboard:restore", vec![json!(id)]),
            Self::ClipboardDelete(id) => ("clipboard:delete", vec![json!(id)]),
            Self::ClipboardPin(id) => ("clipboard:toggle-pin", vec![json!(id)]),
            Self::ClipboardReveal(id) => ("clipboard:reveal", vec![json!(id)]),
            Self::ClipboardImage(id) => ("clipboard:image", vec![json!(id)]),
            Self::ClipboardClear => ("clipboard:clear", vec![]),
            Self::WriteText(text) => ("clipboard:write", vec![json!(text)]),
            Self::WritePng(url) => ("clipboard:write-png", vec![json!(url)]),
            Self::SnippetsList => ("snippets:list", vec![]),
            Self::SnippetWrite { id, draft } => match id {
                Some(id) => ("snippets:update", vec![json!(id), json!(draft)]),
                None => ("snippets:add", vec![json!(draft)]),
            },
            Self::SnippetDelete(id) => ("snippets:delete", vec![json!(id)]),
            Self::SnippetCopy(id) => ("snippets:copy", vec![json!(id)]),
            Self::NotesList => ("notes:list", vec![]),
            Self::NoteCreate(html) => ("notes:append", vec![json!(html)]),
            Self::NoteSave { id, html } => {
                ("notes:update", vec![json!({"createdAt":id,"text":html})])
            }
            Self::NoteDelete(id) => ("notes:delete", vec![json!(id)]),
            Self::VaultStatus => ("vault:status", vec![]),
            Self::VaultUnlock => ("vault:unlock", vec![]),
            Self::VaultLock => ("vault:lock", vec![]),
            Self::VaultList => ("vault:list", vec![]),
            Self::VaultGet(id) => ("vault:get", vec![json!(id)]),
            Self::VaultSave(item) => ("vault:save", vec![json!({"item":item})]),
            Self::VaultDelete(id) => ("vault:delete", vec![json!(id)]),
            Self::VaultCopy(id) => ("vault:copy", vec![json!(id)]),
            Self::SystemStats => ("system-stats:get", vec![]),
            Self::KnowledgeSnapshot => ("knowledge:snapshot", vec![]),
            Self::Sources {
                query,
                offset,
                limit,
            } => (
                "knowledge:sources:list",
                vec![json!({"query":query,"offset":offset,"limit":limit})],
            ),
            Self::IndexStart => ("knowledge:index:start", vec![]),
            Self::IndexPause => ("knowledge:index:pause", vec![]),
            Self::IndexResume => ("knowledge:index:resume", vec![]),
            Self::Permissions => ("permissions:snapshot", vec![]),
            Self::Permission(id) => ("permissions:request", vec![json!(id)]),
            Self::BackgroundTasks => ("background-tasks:list", vec![]),
            Self::Open(path) => ("shell:open", vec![json!(path)]),
        }
    }
}
pub fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, String> {
    if value["ok"] == false {
        return Err(value["error"]
            .as_str()
            .unwrap_or("The request failed.")
            .into());
    }
    serde_json::from_value(value).map_err(|_| "The core returned an incompatible response.".into())
}
pub fn success(value: Value) -> Result<(), String> {
    if value == false || value["ok"] == false {
        Err(value["error"]
            .as_str()
            .unwrap_or("The request failed.")
            .into())
    } else {
        Ok(())
    }
}
pub fn note(value: Value) -> Result<notes::Note, String> {
    decode(value)
}
