//! Finite native-core vocabulary. Neither credentials nor image bytes implement Debug.
use serde_json::{Value, json};
use std::sync::Arc;
use tezbar_ui_model::chat::{Decision, RunRequest, Selection, Turn};
#[derive(Clone)]
pub enum ChatRequest {
    Config(Option<String>),
    List,
    Get(String),
    Append {
        session: String,
        title: String,
        selection: Selection,
        turn: Arc<Turn>,
    },
    Delete(String),
    Clear(Vec<String>),
    HistoryImage {
        session: String,
        turn: String,
        index: usize,
    },
    DeleteTurn {
        session: String,
        turn: String,
    },
    Run {
        request: RunRequest,
        images: Vec<Value>,
    },
    Cancel(String),
    Approve {
        run: String,
        approval: String,
        decision: Decision,
    },
    Capture,
    ChooseDirectory,
    CompleteDirectory(String),
    RecordDirectory(String),
    ComposerItems(Option<String>),
    Contributions(Option<String>),
    Resolve {
        item: Value,
        arguments: Value,
        resource: bool,
    },
    Copy(String),
    OpenUrl(String),
    GenerateMeta {
        session: String,
        prompt: String,
        answer: String,
    },
    UpdateTitle {
        session: String,
        title: String,
        icon: Option<String>,
    },
}
impl std::fmt::Debug for ChatRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChatRequest(<redacted>)")
    }
}
impl ChatRequest {
    pub fn wire(self) -> (&'static str, Vec<Value>) {
        match self {
            Self::Config(cwd) => ("llm-config-get", vec![json!(cwd)]),
            Self::List => ("chat:list", vec![json!(200)]),
            Self::Get(id) => ("chat:get", vec![json!(id)]),
            Self::Append {
                session,
                title,
                selection,
                turn,
            } => (
                "chat:append",
                vec![
                    json!({"session":{"id":session,"title":title,"createdAt":turn.created_at,"updatedAt":turn.created_at,"workingDirectory":selection.cwd,"agentProfileId":selection.profile},"turn":turn.to_core_wire()}),
                ],
            ),
            Self::Delete(id) => ("chat:delete", vec![json!(id)]),
            Self::Clear(ids) => ("chat:clear", vec![Value::Null, json!(ids)]),
            Self::HistoryImage {
                session,
                turn,
                index,
            } => (
                "chat:image",
                vec![json!({"sessionId":session,"turnId":turn,"index":index})],
            ),
            Self::DeleteTurn { session, turn } => (
                "chat:delete-turn",
                vec![json!({"sessionId":session,"turnId":turn})],
            ),
            Self::Run { request: r, images } => (
                "agent:run",
                vec![
                    json!({"runId":r.run_id,"sessionId":r.session_id,"userTurnId":r.user_turn_id,"assistantTurnId":r.assistant_turn_id,"task":r.task.as_ref(),"priorTurnIds":r.prior_turn_ids,"priorTurns":r.core_prior_turns(),"retry":r.retry,"historyHasImages":r.history_has_images,"cwd":r.selection.cwd,"agentProfileId":r.selection.profile,"runSelection":{"provider":r.selection.provider,"model":r.selection.model},"images":images}),
                ],
            ),
            Self::Cancel(run) => ("agent:cancel", vec![json!(run)]),
            Self::Approve {
                run,
                approval,
                decision,
            } => (
                "agent:approve",
                vec![
                    json!({"runId":run,"approvalId":approval,"decision":match decision {Decision::Once=>"once",Decision::Deny=>"deny",Decision::Always=>"always"}}),
                ],
            ),
            Self::Capture => ("agent:capture-active-screen", vec![]),
            Self::ChooseDirectory => ("knowledge:choose-folder", vec![]),
            Self::CompleteDirectory(q) => ("path:complete", vec![json!(q)]),
            Self::RecordDirectory(p) => ("directory-visit:record", vec![json!(p)]),
            Self::ComposerItems(cwd) => ("ai-extensions:composer-items", vec![json!(cwd)]),
            Self::Contributions(cwd) => ("ai-extensions:list", vec![json!(cwd)]),
            Self::Resolve {
                item,
                arguments,
                resource,
            } => (
                if resource {
                    "ai-extensions:read-resource"
                } else {
                    "ai-extensions:prompt-resolve"
                },
                vec![item, arguments],
            ),
            Self::Copy(text) => ("clipboard:write", vec![json!(text)]),
            Self::OpenUrl(url) => ("open-external-url", vec![json!(url)]),
            Self::GenerateMeta {
                session,
                prompt,
                answer,
            } => (
                "chat:generate-meta",
                vec![json!({"id":session,"prompt":prompt,"answer":answer})],
            ),
            Self::UpdateTitle {
                session,
                title,
                icon,
            } => (
                "chat:update-title",
                vec![json!({"id":session,"title":title,"icon":icon})],
            ),
        }
    }
}
