//! Per-conversation presentation ownership. No agent execution, transport, or image bytes.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalMode {
    Review,
    Default,
    Auto,
    Turbo,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub provider_title: Option<String>,
    pub cwd: Option<String>,
    pub profile: Option<String>,
    pub approval_mode: Option<ApprovalMode>,
}
/// Blob/storage identities, including canonical provider-signed history, never base64 copies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Reference {
    Image {
        blob_id: String,
        mime_type: String,
        name: String,
        width: Option<u32>,
        height: Option<u32>,
    },
    SignedHistory {
        turn_id: String,
    },
    ToolHistory {
        turn_id: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StageStatus {
    Running,
    Done,
    Failed,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stage {
    pub index: u64,
    pub label: String,
    pub status: StageStatus,
    pub detail: Option<String>,
    pub output: Option<String>,
    pub tool_name: Option<String>,
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub output_truncated: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimelineItem {
    Text { start: usize, end: usize },
    Stage(u64),
}
/// Internal presentation type. Use from_core_wire/to_core_wire at the durable chat seam.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub id: String,
    pub role: Role,
    pub text: String,
    pub error: Option<String>,
    #[serde(default)]
    pub response_meta: Option<ResponseMeta>,
    pub references: Vec<Reference>,
    pub stages: Vec<Stage>,
    pub created_at: u64,
    /// Arrival order of text segments and stages; empty for legacy turns, which
    /// render stages first and the text after.
    #[serde(skip)]
    pub timeline: Vec<TimelineItem>,
}
/// Separates a new text segment from earlier ones so copied or replayed turn text
/// keeps paragraphs apart. Segment ranges start after the break.
fn paragraph_break(text: &mut String) {
    if !text.is_empty() && !text.ends_with("\n\n") {
        text.push_str(if text.ends_with('\n') { "\n" } else { "\n\n" });
    }
}
/// Durable timeline entry as core stores it: text segments carry their own text.
enum StoredItem {
    Text(String),
    Stage(u64),
}
/// Rebuilds byte ranges for stored text segments by locating each one, in order, in
/// the turn text. Any segment that cannot be placed drops the timeline so the turn
/// falls back to the legacy layout instead of showing misordered or duplicated text.
fn align_timeline(text: &str, stages: &[Stage], items: Vec<StoredItem>) -> Vec<TimelineItem> {
    let mut cursor = 0;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        match item {
            StoredItem::Text(segment) => {
                let Some(at) = text.get(cursor..).and_then(|rest| rest.find(&segment)) else {
                    return vec![];
                };
                let start = cursor + at;
                cursor = start + segment.len();
                out.push(TimelineItem::Text { start, end: cursor });
            }
            StoredItem::Stage(index) => {
                if stages.iter().any(|s| s.index == index)
                    && !out.contains(&TimelineItem::Stage(index))
                {
                    out.push(TimelineItem::Stage(index));
                }
            }
        }
    }
    for s in stages {
        if !out.contains(&TimelineItem::Stage(s.index)) {
            out.push(TimelineItem::Stage(s.index));
        }
    }
    if text
        .get(cursor..)
        .is_some_and(|rest| !rest.trim().is_empty())
    {
        out.push(TimelineItem::Text {
            start: cursor,
            end: text.len(),
        });
    }
    out
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseMeta {
    pub provider: String,
    pub provider_title: String,
    pub model: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "usage_count"
    )]
    pub token_count: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnIdentity {
    pub id: String,
    pub role: Role,
    pub has_error: bool,
    pub has_images: bool,
}
impl From<&Turn> for TurnIdentity {
    fn from(t: &Turn) -> Self {
        Self {
            id: t.id.clone(),
            role: t.role.clone(),
            has_error: t.error.is_some(),
            has_images: t
                .references
                .iter()
                .any(|r| matches!(r, Reference::Image { .. })),
        }
    }
}
impl Turn {
    pub fn to_core_wire(&self) -> serde_json::Value {
        use serde_json::json;
        let attachments: Vec<_> = self.references.iter().filter_map(|r| match r {
            Reference::Image { name, mime_type, width, height, .. } => Some(json!({"kind":"image","name":name,"mimeType":mime_type,"width":width,"height":height})),
            _ => None,
        }).collect();
        let mut wire =
            json!({"id":self.id,"role":self.role,"text":self.text,"createdAt":self.created_at});
        if let Some(e) = &self.error {
            wire["error"] = json!(e);
        }
        if let Some(meta) = &self.response_meta {
            wire["responseMeta"] = json!(meta);
        }
        if !attachments.is_empty() {
            wire["attachments"] = json!(attachments);
        }
        if !self.stages.is_empty() {
            wire["stages"] = json!(self.stages);
        }
        let timeline: Vec<_> = self
            .timeline
            .iter()
            .filter_map(|item| match item {
                TimelineItem::Text { start, end } => self
                    .text
                    .get(*start..*end)
                    .filter(|t| !t.is_empty())
                    .map(|t| json!({"type":"text","text":t})),
                TimelineItem::Stage(index) => self
                    .stages
                    .iter()
                    .find(|s| s.index == *index)
                    .map(|s| json!({"type":"stage","stage":s})),
            })
            .collect();
        if !timeline.is_empty() {
            wire["timeline"] = json!(timeline);
        }
        wire
    }
    /// Core chat attachments are descriptors. Canonical image blobs and signed/tool history
    /// remain in the runtime store, resolved by session/turn identity by the adapter.
    pub fn from_core_wire(wire: serde_json::Value) -> Result<Self, serde_json::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Stored {
            id: String,
            role: Role,
            text: String,
            created_at: u64,
            error: Option<String>,
            response_meta: Option<ResponseMeta>,
            #[serde(default)]
            stages: Vec<Stage>,
            #[serde(default)]
            attachments: Vec<Image>,
            #[serde(default)]
            timeline: Vec<serde_json::Value>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Image {
            name: String,
            mime_type: String,
            width: Option<u32>,
            height: Option<u32>,
        }
        let t: Stored = serde_json::from_value(wire)?;
        let stored = t
            .timeline
            .iter()
            .filter_map(|v| match v["type"].as_str()? {
                "text" => v["text"].as_str().map(|s| StoredItem::Text(s.into())),
                "stage" => v["stage"]["index"].as_u64().map(StoredItem::Stage),
                _ => None,
            })
            .collect::<Vec<_>>();
        let timeline = if stored.is_empty() {
            vec![]
        } else {
            align_timeline(&t.text, &t.stages, stored)
        };
        Ok(Self {
            timeline,
            id: t.id,
            role: t.role,
            text: t.text,
            created_at: t.created_at,
            error: t.error,
            response_meta: t.response_meta,
            stages: t.stages,
            references: t
                .attachments
                .into_iter()
                .map(|i| Reference::Image {
                    blob_id: String::new(),
                    name: i.name,
                    mime_type: i.mime_type,
                    width: i.width,
                    height: i.height,
                })
                .collect(),
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub approval_id: String,
    pub title: String,
    pub command: String,
    pub can_remember: Option<bool>,
    pub remember_kind: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalFocus {
    Composer,
    AllowOnce,
    Deny,
    Always,
    ModePicker,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Once,
    Deny,
    Always,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum Event {
    Start {
        run_id: String,
        task: String,
    },
    Stage {
        run_id: String,
        stage: Stage,
    },
    Message {
        run_id: String,
        delta: String,
    },
    Answer {
        run_id: String,
        text: String,
    },
    Approval {
        run_id: String,
        #[serde(flatten)]
        approval: Approval,
    },
    ApprovalExpired {
        run_id: String,
        approval_id: String,
    },
    Done {
        run_id: String,
    },
    Error {
        run_id: String,
        message: String,
    },
    Log {
        run_id: String,
        source: String,
        line: String,
    },
}
impl Event {
    fn run_id(&self) -> &str {
        match self {
            Self::Start { run_id, .. }
            | Self::Stage { run_id, .. }
            | Self::Message { run_id, .. }
            | Self::Answer { run_id, .. }
            | Self::Approval { run_id, .. }
            | Self::ApprovalExpired { run_id, .. }
            | Self::Done { run_id }
            | Self::Error { run_id, .. }
            | Self::Log { run_id, .. } => run_id,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Boot {
    Panel,
    NewChat {
        session_id: String,
        cwd: Option<String>,
    },
    Screen {
        session_id: String,
    },
    Resume {
        session_id: String,
    },
    Submit {
        session_id: String,
        prompt: String,
        cwd: Option<String>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunPhase {
    Preparing {
        token: u64,
    },
    Running {
        run_id: String,
        started: bool,
        deadline: u64,
    },
    Settled,
}
#[derive(Clone)]
pub struct RunRequest {
    pub run_id: String,
    pub session_id: String,
    pub user_turn_id: String,
    pub assistant_turn_id: String,
    pub task: Arc<str>,
    pub prior_turn_ids: Vec<String>,
    /// Only unsaved prior turns, shared with the bounded dirty queue; no stage/payload clone.
    pub prior_turns: Vec<Arc<Turn>>,
    pub references: Vec<Reference>,
    pub selection: Selection,
    pub retry: bool,
    pub history_has_images: bool,
}
pub enum Effect {
    LoadHistory {
        token: u64,
    },
    LoadConversation {
        token: u64,
        session_id: String,
    },
    Capture {
        token: u64,
        session_id: String,
    },
    AppendUser {
        token: u64,
        session_id: String,
        turn: Arc<Turn>,
    },
    PrepareRetry {
        token: u64,
        session_id: String,
        user_turn_id: String,
        assistant_turn_id: String,
    },
    Start(RunRequest),
    Cancel {
        run_id: String,
    },
    PersistTurn {
        token: u64,
        session_id: String,
        turn: Arc<Turn>,
    },
    DeleteConversation {
        token: u64,
        session_id: String,
    },
    DeleteTurn {
        token: u64,
        session_id: String,
        turn_id: String,
    },
    Approve {
        run_id: String,
        approval_id: String,
        decision: Decision,
    },
    SaveSelection(Selection),
    FocusApproval {
        session_id: String,
    },
    ResolvePrompt {
        token: u64,
        id: String,
        arguments: BTreeMap<String, String>,
    },
    ReviewProject(String),
}
struct Preparation {
    token: u64,
    run_id: String,
    user: Arc<Turn>,
    assistant_id: String,
    selection: Selection,
    retry: bool,
}
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub turns: VecDeque<Arc<Turn>>,
    pub context: VecDeque<TurnIdentity>,
    pub selection: Selection,
    pub phase: RunPhase,
    pub approval: Option<Approval>,
    pub focus: ApprovalFocus,
    pub timeline: Vec<TimelineItem>,
    pub draft: String,
    pub attachments: Vec<Reference>,
    pub input_error: Option<String>,
    preparation: Option<Preparation>,
    assistant: Option<Turn>,
    canonical: bool,
    dirty: BTreeMap<String, (u64, Arc<Turn>)>,
    deleted: Option<u64>,
    draft_revision: u64,
}
impl Conversation {
    fn new(id: String) -> Self {
        Self {
            id,
            title: "New chat".into(),
            turns: VecDeque::new(),
            context: VecDeque::new(),
            selection: Selection::default(),
            phase: RunPhase::Settled,
            approval: None,
            focus: ApprovalFocus::Composer,
            timeline: vec![],
            draft: String::new(),
            attachments: vec![],
            input_error: None,
            preparation: None,
            assistant: None,
            canonical: false,
            dirty: BTreeMap::new(),
            deleted: None,
            draft_revision: 0,
        }
    }
    pub fn response(&self) -> Option<&Turn> {
        self.assistant.as_ref()
    }
    pub fn busy(&self) -> bool {
        self.phase != RunPhase::Settled
    }
    pub fn unsaved(&self) -> bool {
        !self.dirty.is_empty()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryRow {
    pub id: String,
    pub title: String,
    pub preview: String,
    pub updated_at: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackgroundStatus {
    pub session_id: String,
    pub busy: bool,
    pub waiting_approval: bool,
}
/// Entries with in-flight work or failed persistence are retained; admission is bounded.
pub struct ChatController {
    owners: BTreeMap<String, Conversation>,
    pub active: Option<String>,
    pub visible: bool,
    pub history: Option<Vec<HistoryRow>>,
    next: u64,
    boot_generation: u64,
    history_token: Option<u64>,
    load: Option<(u64, String, u64)>,
    capture: Option<(u64, String, u64)>,
    last_nonce: Option<(Boot, String)>,
    tombstones: BTreeSet<String>,
    max_owners: usize,
    max_turns: usize,
    max_text_bytes: usize,
    project_notices: BTreeSet<String>,
    turn_deletions: BTreeMap<u64, (String, String)>,
    prompt: Option<(u64, String, u64)>,
}
impl Default for ChatController {
    fn default() -> Self {
        Self {
            owners: BTreeMap::new(),
            active: None,
            visible: true,
            history: None,
            next: 0,
            boot_generation: 0,
            history_token: None,
            load: None,
            capture: None,
            last_nonce: None,
            tombstones: BTreeSet::new(),
            max_owners: 32,
            max_turns: 256,
            max_text_bytes: 4 * 1024 * 1024,
            project_notices: BTreeSet::new(),
            turn_deletions: BTreeMap::new(),
            prompt: None,
        }
    }
}
impl ChatController {
    fn token(&mut self) -> u64 {
        self.next += 1;
        self.next
    }
    pub fn conversation(&self, id: &str) -> Option<&Conversation> {
        self.owners.get(id).filter(|c| c.deleted.is_none())
    }
    /// Refresh a history popover without navigating away from its active conversation.
    pub fn refresh_history(&mut self) -> Effect {
        let token = self.token();
        self.history_token = Some(token);
        Effect::LoadHistory { token }
    }
    pub fn edit_draft(&mut self, id: &str, text: String) {
        if let Some(c) = self.owners.get_mut(id) {
            c.draft = text;
            c.draft_revision += 1;
        }
    }
    fn admit(&mut self, id: &str) -> Result<(), &'static str> {
        if self.owners.contains_key(id) {
            return Ok(());
        }
        if self.owners.len() >= self.max_owners {
            let evict = self
                .owners
                .iter()
                .find(|(id, c)| {
                    Some(id.as_str()) != self.active.as_deref()
                        && !c.busy()
                        && !c.unsaved()
                        && c.deleted.is_none()
                })
                .map(|(id, _)| id.clone());
            if let Some(id) = evict {
                self.owners.remove(&id);
            } else {
                return Err("Finish or save a conversation before opening another");
            }
        }
        self.owners.insert(id.into(), Conversation::new(id.into()));
        Ok(())
    }
    pub fn boot(&mut self, boot: Boot, nonce: Option<String>) -> Result<Vec<Effect>, &'static str> {
        if nonce
            .as_ref()
            .is_some_and(|n| self.last_nonce.as_ref() == Some(&(boot.clone(), n.clone())))
        {
            return Ok(vec![]);
        }
        self.last_nonce = nonce.map(|n| (boot.clone(), n));
        self.boot_generation += 1;
        self.capture = None;
        self.load = None;
        self.prompt = None;
        self.visible = true;
        for c in self.owners.values_mut() {
            c.focus = ApprovalFocus::Composer;
        }
        let mut effects = vec![];
        match boot {
            Boot::Panel => {
                self.active = None;
                let token = self.token();
                self.history_token = Some(token);
                effects.push(Effect::LoadHistory { token });
            }
            Boot::Resume { session_id } => {
                if self.tombstones.contains(&session_id) {
                    return Err("Conversation was deleted");
                }
                self.admit(&session_id)?;
                self.active = Some(session_id.clone());
                let c = &self.owners[&session_id];
                if !c.busy() && !c.unsaved() {
                    let rev = c.draft_revision;
                    let token = self.token();
                    self.load = Some((token, session_id.clone(), rev));
                    effects.push(Effect::LoadConversation { token, session_id });
                }
            }
            Boot::NewChat { session_id, cwd } => {
                self.fresh(session_id, cwd)?;
            }
            Boot::Screen { session_id } => {
                self.fresh(session_id, None)?;
                if let Some(e) = self.capture() {
                    effects.push(e);
                }
            }
            Boot::Submit {
                session_id,
                prompt,
                cwd,
            } => {
                self.fresh(session_id.clone(), cwd)?;
                self.edit_draft(&session_id, prompt);
            }
        }
        if let Some(id) = &self.active {
            if self
                .owners
                .get(id)
                .is_some_and(|c| c.approval.is_some() && c.deleted.is_none())
            {
                effects.push(Effect::FocusApproval {
                    session_id: id.clone(),
                });
            }
        }
        Ok(effects)
    }
    fn fresh(&mut self, id: String, cwd: Option<String>) -> Result<(), &'static str> {
        if self.owners.contains_key(&id) || self.tombstones.contains(&id) {
            return Err("New chat requires a fresh injected session id");
        }
        self.admit(&id)?;
        self.owners.get_mut(&id).unwrap().selection.cwd = cwd;
        self.active = Some(id);
        Ok(())
    }
    pub fn history_result(&mut self, token: u64, rows: Vec<HistoryRow>) {
        if self.history_token != Some(token) {
            return;
        }
        self.history_token = None;
        self.history = Some(
            rows.into_iter()
                .filter(|r| !self.tombstones.contains(&r.id))
                .take(200)
                .map(|mut r| {
                    truncate(&mut r.preview, 512);
                    r
                })
                .collect(),
        );
    }
    pub fn history_result_conversation(
        &mut self,
        token: u64,
        turns: Vec<Arc<Turn>>,
        selection: Selection,
    ) {
        let Some((t, id, draft_revision)) = self.load.take() else {
            return;
        };
        if t != token {
            self.load = Some((t, id, draft_revision));
            return;
        }
        let Some(c) = self.owners.get_mut(&id) else {
            return;
        };
        if c.busy() || c.unsaved() || c.deleted.is_some() || c.draft_revision != draft_revision {
            return;
        }
        c.context = turns
            .iter()
            .rev()
            .take(self.max_turns)
            .map(|t| TurnIdentity::from(t.as_ref()))
            .collect();
        c.context.make_contiguous().reverse();
        c.turns = turns
            .into_iter()
            .rev()
            .take(self.max_turns)
            .collect::<VecDeque<_>>();
        c.turns.make_contiguous().reverse();
        trim_history(&mut c.turns, self.max_turns, 2 * 1024 * 1024);
        c.selection = selection;
    }
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        for c in self.owners.values_mut() {
            c.focus = ApprovalFocus::Composer;
        }
        if !visible {
            self.capture = None;
            self.boot_generation += 1;
        }
    }
    pub fn capture(&mut self) -> Option<Effect> {
        if !self.visible || self.capture.is_some() {
            return None;
        }
        let id = self.active.clone()?;
        let token = self.token();
        self.capture = Some((token, id.clone(), self.boot_generation));
        Some(Effect::Capture {
            token,
            session_id: id,
        })
    }
    pub fn capture_result(&mut self, token: u64, image: Reference) {
        let Some((t, id, generation)) = &self.capture else {
            return;
        };
        if *t != token {
            return;
        }
        let id = id.clone();
        let valid = *generation == self.boot_generation
            && self.visible
            && self.active.as_deref() == Some(&id);
        self.capture = None;
        if valid {
            if let Some(c) = self.owners.get_mut(&id) {
                if c.attachments.len() < 8 && !c.attachments.contains(&image) {
                    c.attachments.push(image);
                }
            }
        }
    }
    /// A stale failure cannot unlock a newer capture request.
    pub fn capture_failed(&mut self, token: u64) {
        if self.capture.as_ref().is_some_and(|(t, _, _)| *t == token) {
            self.capture = None;
        }
    }
    /// Append canonical image references to the current draft. The adapter owns
    /// decoding/storage and must additionally fence its asynchronous load revision.
    /// Admission is atomic; duplicates do not consume the eight-image limit.
    pub fn attach_references(
        &mut self,
        session: &str,
        images: Vec<Reference>,
    ) -> Result<usize, &'static str> {
        if !self.visible || self.active.as_deref() != Some(session) {
            return Err("Conversation is no longer active");
        }
        let c = self
            .owners
            .get_mut(session)
            .ok_or("Conversation is unavailable")?;
        if c.deleted.is_some() || c.busy() {
            return Err("Conversation is unavailable for attachments");
        }
        if images.iter().any(|i| !matches!(i, Reference::Image { .. })) {
            return Err("Only image references can be attached");
        }
        let mut next = c.attachments.clone();
        for image in images {
            if !next.contains(&image) {
                next.push(image);
            }
        }
        if next.len() > 8 {
            return Err("Attach up to eight images");
        }
        let added = next.len() - c.attachments.len();
        c.attachments = next;
        Ok(added)
    }
    pub fn remove_attachment(&mut self, id: &str, index: usize) {
        if let Some(c) = self.owners.get_mut(id) {
            if index < c.attachments.len() {
                c.attachments.remove(index);
            }
        }
    }
    pub fn select(&mut self, id: &str, selection: Selection) -> Option<Effect> {
        let c = self.owners.get_mut(id)?;
        c.selection = selection.clone();
        Some(Effect::SaveSelection(selection))
    }
    /// IDs and clock are injected. An adapter appends the user first and returns its canonical stored turn.
    pub fn send(
        &mut self,
        id: &str,
        run_id: String,
        user_id: String,
        assistant_id: String,
        now: u64,
    ) -> Result<Effect, &'static str> {
        if self.owners.values().any(|c| {
            matches!(&c.phase,RunPhase::Running{run_id:r,..}if r==&run_id)
                || c.preparation.as_ref().is_some_and(|p| p.run_id == run_id)
        }) {
            return Err("Run id is already owned");
        }
        let token = self.token();
        let c = self.owners.get_mut(id).ok_or("No conversation")?;
        if c.dirty.len() >= 16 {
            return Err("Retry saving this conversation before sending more messages");
        }
        if c.busy() || c.deleted.is_some() {
            return Err("Conversation is busy or being deleted");
        }
        if c.draft.trim().is_empty() && c.attachments.is_empty() {
            return Err("Enter a message");
        }
        if c.draft.len() > self.max_text_bytes {
            return Err("Message is too large");
        }
        let user = Arc::new(Turn {
            id: user_id,
            role: Role::User,
            text: std::mem::take(&mut c.draft),
            error: None,
            response_meta: None,
            references: std::mem::take(&mut c.attachments),
            stages: vec![],
            created_at: now,
            timeline: vec![],
        });
        c.draft_revision += 1;
        c.input_error = None;
        c.preparation = Some(Preparation {
            token,
            run_id,
            user: user.clone(),
            assistant_id,
            selection: c.selection.clone(),
            retry: false,
        });
        c.phase = RunPhase::Preparing { token };
        Ok(Effect::AppendUser {
            token,
            session_id: id.into(),
            turn: user,
        })
    }
    pub fn retry(
        &mut self,
        id: &str,
        assistant_id: &str,
        run_id: String,
    ) -> Result<Effect, &'static str> {
        if self.owners.values().any(|c| {
            matches!(&c.phase,RunPhase::Running{run_id:r,..}if r==&run_id)
                || c.preparation.as_ref().is_some_and(|p| p.run_id == run_id)
        }) {
            return Err("Run id is already owned");
        }
        let token = self.token();
        let c = self.owners.get_mut(id).ok_or("No conversation")?;
        if c.dirty.len() >= 16 {
            return Err("Retry saving this conversation before sending more messages");
        }
        if c.busy() || c.deleted.is_some() {
            return Err("Conversation is busy or being deleted");
        }
        let index = c
            .turns
            .iter()
            .position(|t| t.id == assistant_id && t.role == Role::Assistant && t.error.is_some())
            .ok_or("No failed response")?;
        if c.context.back().is_none_or(|t| t.id != assistant_id) {
            return Err("Only the latest failed response can be retried");
        }
        let user = c
            .turns
            .iter()
            .take(index)
            .rev()
            .find(|t| t.role == Role::User)
            .cloned()
            .ok_or("No stored user turn")?;
        let effect = Effect::PrepareRetry {
            token,
            session_id: id.into(),
            user_turn_id: user.id.clone(),
            assistant_turn_id: assistant_id.into(),
        };
        c.preparation = Some(Preparation {
            token,
            run_id,
            user,
            assistant_id: assistant_id.into(),
            selection: c.selection.clone(),
            retry: true,
        });
        c.phase = RunPhase::Preparing { token };
        Ok(effect)
    }
    pub fn prepared(
        &mut self,
        id: &str,
        token: u64,
        canonical_user: Result<Arc<Turn>, String>,
        now: u64,
    ) -> Vec<Effect> {
        let Some(c) = self.owners.get_mut(id) else {
            return vec![];
        };
        if c.deleted.is_some() || !matches!(c.phase,RunPhase::Preparing{token:t}if t==token) {
            return vec![];
        }
        let Some(p) = c.preparation.take() else {
            return vec![];
        };
        if p.token != token {
            return vec![];
        }
        let user = match canonical_user {
            Ok(t) if t.role == Role::User && t.id == p.user.id => t,
            _ => {
                c.phase = RunPhase::Settled;
                c.input_error = Some("Could not prepare the message. Try again.".into());
                c.draft = p.user.text.clone();
                c.attachments = p.user.references.clone();
                return vec![];
            }
        };
        let boundary = c
            .context
            .iter()
            .position(|t| t.id == user.id)
            .unwrap_or(c.context.len());
        let prior_turn_ids: Vec<String> = c
            .context
            .range(..boundary)
            .filter(|t| t.id != p.assistant_id)
            .rev()
            .take(16)
            .map(|t| t.id.clone())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let history_has_images = c
            .context
            .iter()
            .filter(|t| prior_turn_ids.contains(&t.id))
            .any(|t| t.has_images);
        let prior_turns = prior_turn_ids
            .iter()
            .filter_map(|id| c.dirty.get(id).map(|(_, t)| t.clone()))
            .collect();
        remember(&mut c.context, &user, self.max_turns);
        if !c.turns.iter().any(|t| t.id == user.id) {
            c.turns.push_back(user.clone());
        }
        while c.turns.len() > self.max_turns {
            c.turns.pop_front();
        }
        c.assistant = Some(Turn {
            id: p.assistant_id.clone(),
            role: Role::Assistant,
            text: String::new(),
            error: None,
            response_meta: Some(ResponseMeta {
                provider: p.selection.provider.clone().unwrap_or_default(),
                provider_title: p
                    .selection
                    .provider_title
                    .clone()
                    .or_else(|| p.selection.provider.clone())
                    .unwrap_or_default(),
                model: p.selection.model.clone().unwrap_or_default(),
                token_count: None,
            }),
            references: vec![],
            stages: vec![],
            created_at: now,
            timeline: vec![],
        });
        c.approval = None;
        c.timeline.clear();
        c.canonical = false;
        c.phase = RunPhase::Running {
            run_id: p.run_id.clone(),
            started: false,
            deadline: now + 30_000,
        };
        vec![Effect::Start(RunRequest {
            run_id: p.run_id,
            session_id: id.into(),
            user_turn_id: user.id.clone(),
            assistant_turn_id: p.assistant_id,
            task: Arc::from(user.text.as_str()),
            prior_turn_ids,
            prior_turns,
            references: user.references.clone(),
            selection: p.selection,
            retry: p.retry,
            history_has_images,
        })]
    }
    pub fn event(&mut self, event: Event) -> Vec<Effect> {
        let run = event.run_id();
        let Some(id) = self
            .owners
            .iter()
            .find(|(_, c)| matches!(&c.phase,RunPhase::Running{run_id,..}if run_id==run))
            .map(|(id, _)| id.clone())
        else {
            return vec![];
        };
        let c = self.owners.get_mut(&id).unwrap();
        let max = self.max_text_bytes;
        let finish = match event {
            Event::Start { .. } => {
                if let RunPhase::Running { started, .. } = &mut c.phase {
                    *started = true;
                }
                false
            }
            Event::Message { delta, .. } => {
                if c.canonical {
                    return vec![];
                }
                if let Some(a) = &mut c.assistant {
                    let start = a.text.len();
                    if start + delta.len() > max {
                        a.error=Some("Response exceeded the display limit. Open saved history for the complete response.".into());
                        true
                    } else if let Some(TimelineItem::Text { end, .. }) = c.timeline.last_mut() {
                        a.text.push_str(&delta);
                        *end = a.text.len();
                        false
                    } else {
                        paragraph_break(&mut a.text);
                        let start = a.text.len();
                        a.text.push_str(&delta);
                        c.timeline.push(TimelineItem::Text {
                            start,
                            end: a.text.len(),
                        });
                        false
                    }
                } else {
                    false
                }
            }
            Event::Answer { text, .. } => {
                if let Some(a) = &mut c.assistant {
                    // The agent's canonical answer is only its final model turn, which
                    // begins after the last tool call (thinking happens inside a turn).
                    // Replace only that turn's streamed text so narration written
                    // between earlier tool calls keeps its place in the timeline.
                    let thinking = |index: &u64| {
                        a.stages.iter().any(|s| {
                            s.index == *index
                                && (s.tool_name.as_deref() == Some("thinking")
                                    || s.label.starts_with("Thinking"))
                        })
                    };
                    let boundary = c
                        .timeline
                        .iter()
                        .rposition(|i| matches!(i, TimelineItem::Stage(n) if !thinking(n)));
                    let first = boundary.map_or(0, |p| p + 1);
                    let cut = c.timeline[first..]
                        .iter()
                        .find_map(|i| match i {
                            TimelineItem::Text { start, .. } => Some(*start),
                            TimelineItem::Stage(_) => None,
                        })
                        .unwrap_or(if boundary.is_some() { a.text.len() } else { 0 });
                    let mut index = 0;
                    c.timeline.retain(|i| {
                        index += 1;
                        index <= first || matches!(i, TimelineItem::Stage(_))
                    });
                    a.text.truncate(cut);
                    paragraph_break(&mut a.text);
                    let start = a.text.len();
                    a.text.push_str(&text);
                    truncate(&mut a.text, max);
                    if a.text.len() > start {
                        c.timeline.push(TimelineItem::Text {
                            start,
                            end: a.text.len(),
                        });
                    }
                    c.canonical = true;
                }
                false
            }
            Event::Stage { mut stage, .. } => {
                if stage.tool_name.as_deref() == Some("thinking")
                    || stage.label.to_lowercase().starts_with("thinking")
                {
                    stage.label = "Thinking…".into();
                    stage.detail = None;
                    stage.output = None;
                }
                if let Some(o) = &mut stage.output {
                    truncate(o, 32 * 1024);
                }
                if let Some(a) = &mut c.assistant {
                    if let Some(old) = a.stages.iter_mut().find(|s| s.index == stage.index) {
                        *old = stage;
                    } else if a.stages.len() < 512 {
                        c.timeline.push(TimelineItem::Stage(stage.index));
                        a.stages.push(stage);
                    }
                }
                false
            }
            Event::Approval { approval, .. } => {
                c.approval = Some(approval);
                c.focus = ApprovalFocus::Composer;
                return if self.visible && self.active.as_deref() == Some(&id) && c.deleted.is_none()
                {
                    vec![Effect::FocusApproval { session_id: id }]
                } else {
                    vec![]
                };
            }
            Event::ApprovalExpired { approval_id, .. } => {
                if c.approval
                    .as_ref()
                    .is_some_and(|a| a.approval_id == approval_id)
                {
                    c.approval = None;
                }
                false
            }
            Event::Done { .. } => {
                if let Some(a) = &mut c.assistant {
                    if a.text.trim().is_empty() {
                        a.error = Some("Agent finished without a response.".into());
                    }
                }
                true
            }
            Event::Error { message, .. } => {
                if let Some(a) = &mut c.assistant {
                    a.error = Some(message);
                }
                true
            }
            // Core diagnostics are deliberately not copied into a UI cache: may contain sensitive wire data.
            Event::Log { .. } => false,
        };
        if finish {
            self.settle(&id, None)
        } else {
            vec![]
        }
    }
    fn settle(&mut self, id: &str, error: Option<&str>) -> Vec<Effect> {
        let token = self.token();
        let c = self.owners.get_mut(id).unwrap();
        c.phase = RunPhase::Settled;
        c.approval = None;
        let Some(mut a) = c.assistant.take() else {
            return vec![];
        };
        if let Some(e) = error {
            a.error = Some(e.into());
        }
        for s in &mut a.stages {
            if s.status == StageStatus::Running {
                s.status = StageStatus::Failed;
                s.detail = Some(
                    a.error
                        .clone()
                        .unwrap_or_else(|| "Run ended before this step finished.".into()),
                );
            }
        }
        a.timeline = c.timeline.clone();
        let turn = Arc::new(a);
        remember(&mut c.context, &turn, self.max_turns);
        if let Some(old) = c.turns.iter_mut().find(|t| t.id == turn.id) {
            *old = turn.clone();
        } else {
            c.turns.push_back(turn.clone());
        }
        while c.turns.len() > self.max_turns {
            c.turns.pop_front();
        }
        trim_history(&mut c.turns, self.max_turns, 2 * 1024 * 1024);
        c.dirty.insert(turn.id.clone(), (token, turn.clone()));
        if c.deleted.is_some() {
            vec![]
        } else {
            vec![Effect::PersistTurn {
                token,
                session_id: id.into(),
                turn,
            }]
        }
    }
    pub fn stop(&mut self, id: &str) -> Vec<Effect> {
        let Some(c) = self.owners.get_mut(id) else {
            return vec![];
        };
        let run = if let RunPhase::Running { run_id, .. } = &c.phase {
            Some(run_id.clone())
        } else {
            None
        };
        if let Some(p) = c.preparation.take() {
            c.draft = p.user.text.clone();
            c.attachments = p.user.references.clone();
        }
        c.phase = RunPhase::Settled;
        let mut effects = if c.assistant.is_some() {
            self.settle(id, Some("Stopped by you."))
        } else {
            vec![]
        };
        if let Some(run_id) = run {
            effects.insert(0, Effect::Cancel { run_id });
        }
        effects
    }
    pub fn tick(&mut self, now: u64) -> Vec<Effect> {
        let ids=self.owners.iter().filter(|(_,c)|matches!(c.phase,RunPhase::Running{started:false,deadline,..}if now>=deadline)).map(|(id,_)|id.clone()).collect::<Vec<_>>();
        let mut effects = vec![];
        for id in ids {
            let run = match &self.owners[&id].phase {
                RunPhase::Running { run_id, .. } => run_id.clone(),
                _ => continue,
            };
            effects.push(Effect::Cancel { run_id: run });
            effects.extend(self.settle(&id,Some("The agent did not start within 30 seconds. Check the selected provider and try again.")));
        }
        effects
    }
    pub fn persisted(&mut self, id: &str, token: u64, ok: bool) {
        if let Some(c) = self.owners.get_mut(id) {
            if ok {
                c.dirty.retain(|_, (t, _)| *t != token);
            }
        }
    }
    pub fn retry_persistence(&mut self, id: &str) -> Vec<Effect> {
        let Some(c) = self.owners.get(id) else {
            return vec![];
        };
        if c.deleted.is_some() {
            return vec![];
        }
        c.dirty
            .values()
            .map(|(token, turn)| Effect::PersistTurn {
                token: *token,
                session_id: id.into(),
                turn: turn.clone(),
            })
            .collect()
    }
    /// Fence every retained conversation before the adapter sends one canonical
    /// chat:clear. Settle each returned ticket with delete_result, including on
    /// failure, and execute its deferred persistence effects.
    pub fn begin_clear(&mut self) -> Vec<Effect> {
        self.boot_generation += 1;
        self.capture = None;
        self.load = None;
        self.prompt = None;
        self.history_token = None;
        let ids: Vec<_> = self.owners.keys().cloned().collect();
        let mut effects = Vec::new();
        for id in ids {
            if let Some(token) = self.owners[&id].deleted {
                effects.push(Effect::DeleteConversation {
                    token,
                    session_id: id,
                });
            } else {
                effects.extend(self.delete(&id));
            }
        }
        effects
    }
    pub fn delete(&mut self, id: &str) -> Vec<Effect> {
        let token = self.token();
        let Some(c) = self.owners.get_mut(id) else {
            return vec![];
        };
        if c.deleted.is_some() {
            return vec![];
        }
        c.deleted = Some(token);
        let mut effects = vec![];
        if let RunPhase::Running { run_id, .. } = &c.phase {
            effects.push(Effect::Cancel {
                run_id: run_id.clone(),
            });
        }
        effects.push(Effect::DeleteConversation {
            token,
            session_id: id.into(),
        });
        effects
    }
    pub fn delete_result(&mut self, id: &str, token: u64, ok: bool) -> Vec<Effect> {
        if !self
            .owners
            .get(id)
            .is_some_and(|c| c.deleted == Some(token))
        {
            return vec![];
        }
        if ok {
            self.owners.remove(id);
            self.tombstones.insert(id.into());
            if self.active.as_deref() == Some(id) {
                self.active = None;
            }
            if let Some(rows) = &mut self.history {
                rows.retain(|r| r.id != id);
            }
        } else {
            let c = self.owners.get_mut(id).unwrap();
            c.deleted = None;
            if let Some(p) = c.preparation.take() {
                c.draft = p.user.text.clone();
                c.attachments = p.user.references.clone();
            }
            if matches!(c.phase, RunPhase::Preparing { .. }) {
                c.phase = RunPhase::Settled;
            }
        }
        self.retry_persistence(id)
    }
    pub fn focus(&mut self, id: &str, focus: ApprovalFocus) {
        if self.visible && self.active.as_deref() == Some(id) {
            if let Some(c) = self.owners.get_mut(id) {
                c.focus = focus;
            }
        }
    }
    pub fn decide(&mut self, id: &str, decision: Decision) -> Option<Effect> {
        let c = self.owners.get_mut(id)?;
        if c.deleted.is_some() {
            return None;
        }
        let a = c.approval.as_ref()?;
        if decision == Decision::Always && a.can_remember == Some(false) {
            return None;
        }
        let RunPhase::Running { run_id, .. } = &c.phase else {
            return None;
        };
        let effect = Effect::Approve {
            run_id: run_id.clone(),
            approval_id: a.approval_id.clone(),
            decision,
        };
        c.approval = None;
        Some(effect)
    }
    pub fn enter(&mut self, id: &str) -> Option<Effect> {
        if !self.visible || self.active.as_deref() != Some(id) {
            return None;
        }
        let focus = self.owners.get(id)?.focus;
        match focus {
            ApprovalFocus::AllowOnce => self.decide(id, Decision::Once),
            ApprovalFocus::Deny => self.decide(id, Decision::Deny),
            ApprovalFocus::Always => self.decide(id, Decision::Always),
            _ => None,
        }
    }
    pub fn set_mode(&mut self, id: &str, mode: ApprovalMode) -> Option<Effect> {
        let c = self.owners.get_mut(id)?;
        c.selection.approval_mode = Some(mode);
        Some(Effect::SaveSelection(c.selection.clone()))
    }
    pub fn background(&self) -> Vec<BackgroundStatus> {
        self.owners
            .values()
            .filter(|c| c.deleted.is_none() && c.busy())
            .map(|c| BackgroundStatus {
                session_id: c.id.clone(),
                busy: true,
                waiting_approval: c.approval.is_some(),
            })
            .collect()
    }
    pub fn project_notice(&mut self, cwd: String) -> Option<Effect> {
        self.project_notices
            .insert(cwd.clone())
            .then_some(Effect::ReviewProject(cwd))
    }
    pub fn unmount(&mut self) -> Vec<Effect> {
        self.visible = false;
        self.capture = None;
        self.load = None;
        let ids = self.owners.keys().cloned().collect::<Vec<_>>();
        ids.into_iter().flat_map(|id| self.stop(&id)).collect()
    }
}
fn truncate(s: &mut String, n: usize) {
    if s.len() > n {
        let mut end = n;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
    }
}
pub fn title(text: &str) -> String {
    let s = text
        .lines()
        .find(|s| !s.trim().is_empty())
        .unwrap_or("")
        .trim();
    if s.is_empty() {
        "New chat".into()
    } else if s.chars().count() > 64 {
        format!("{}…", s.chars().take(61).collect::<String>())
    } else {
        s.into()
    }
}
pub fn history_placeholder(loading: bool) -> &'static str {
    if loading {
        "Loading conversations…"
    } else {
        "No saved conversations yet."
    }
}
pub fn follow_output(scroll_top: f64, client_height: f64, scroll_height: f64) -> bool {
    scroll_height - scroll_top - client_height <= 64.0
}
#[derive(Default)]
pub struct FrameCoalescer {
    dirty: bool,
    scheduled: Option<u64>,
}
impl FrameCoalescer {
    pub fn cancel(&mut self) {
        self.dirty = false;
        self.scheduled = None;
    }
    pub fn changed(&mut self, now: u64, frame_ms: u64) -> Option<u64> {
        self.dirty = true;
        if self.scheduled.is_none() {
            self.scheduled = Some(now + frame_ms.max(1));
        }
        self.scheduled
    }
    pub fn flush(&mut self, now: u64, force: bool) -> bool {
        if self.dirty && (force || self.scheduled.is_some_and(|t| now >= t)) {
            self.dirty = false;
            self.scheduled = None;
            true
        } else {
            false
        }
    }
}
pub mod meta;
#[cfg(test)]
mod tests;
/// Concise model/provider failures without rendering an entire JSON response body.
pub fn format_error(raw: &str, provider: &str) -> String {
    let trimmed = raw.trim();
    let detail = trimmed
        .find('{')
        .and_then(|i| serde_json::from_str::<serde_json::Value>(&trimmed[i..]).ok())
        .and_then(|v| v["error"]["message"].as_str().map(|s| s.trim().to_owned()));
    let message = detail.as_deref().unwrap_or(trimmed);
    static PATTERNS: std::sync::OnceLock<Vec<regex::Regex>> = std::sync::OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| [
        "(?i)failed to write to backend runner|backend runner (?:is still recovering|stopped|restarting)",
        r"(?i)supported API model names are (.+?), but you passed (.+?)(?:\.$|$)",
        r#"(?i)Model\s+["']([^"']+)["']\s+not found"#,
        r"(?i)\berror\s+(\d{3})\b",
    ].into_iter().map(|p| regex::Regex::new(p).expect("static pattern")).collect());
    let re = |s: &str| {
        patterns
            .iter()
            .find(|p| p.as_str() == s)
            .expect("known pattern")
    };
    if re("(?i)failed to write to backend runner|backend runner (?:is still recovering|stopped|restarting)").is_match(message){return "The background service disconnected. Tezbar is reconnecting. Wait a moment, then try again. Your message has been kept.".into();}
    if let Some(c) = re(r"(?i)supported API model names are (.+?), but you passed (.+?)(?:\.$|$)")
        .captures(message)
    {
        return format!(
            "Model \"{}\" is not supported by this provider. Choose {} and try again.",
            &c[2], &c[1]
        );
    }
    if let Some(c) = re(r#"(?i)Model\s+["']([^"']+)["']\s+not found"#).captures(message) {
        return format!(
            "The selected model \"{}\" is unavailable. Choose another model in AI settings and try again.",
            &c[1]
        );
    }
    if let Some(detail) = detail {
        let status = re(r"(?i)\berror\s+(\d{3})\b")
            .captures(trimmed)
            .map(|c| format!(" ({})", &c[1]))
            .unwrap_or_default();
        format!("{provider} request failed{status}: {detail}")
    } else {
        trimmed.into()
    }
}
fn trim_history(turns: &mut VecDeque<Arc<Turn>>, max_turns: usize, max_bytes: usize) {
    let mut bytes: usize = turns
        .iter()
        .map(|t| {
            t.text.len()
                + t.stages
                    .iter()
                    .map(|s| s.output.as_ref().map_or(0, String::len))
                    .sum::<usize>()
        })
        .sum();
    while turns.len() > 2 && (turns.len() > max_turns || bytes > max_bytes) {
        let t = turns.pop_front().unwrap();
        bytes = bytes.saturating_sub(
            t.text.len()
                + t.stages
                    .iter()
                    .map(|s| s.output.as_ref().map_or(0, String::len))
                    .sum::<usize>(),
        );
    }
}
impl ChatController {
    pub fn start_result(&mut self, run_id: &str, result: Result<String, String>) -> Vec<Effect> {
        match result {
            Ok(returned) if returned == run_id => vec![],
            Ok(_) => self.event(Event::Error {
                run_id: run_id.into(),
                message: "The agent returned an unexpected run id. Restart Tezbar and try again."
                    .into(),
            }),
            Err(message) => self.event(Event::Error {
                run_id: run_id.into(),
                message,
            }),
        }
    }
    pub fn delete_turn(&mut self, session: &str, turn: &str) -> Result<Effect, &'static str> {
        let c = self
            .conversation(session)
            .ok_or("Conversation unavailable")?;
        if c.busy() {
            return Err("Stop the run before deleting a turn");
        }
        if !c.turns.iter().any(|t| t.id == turn) {
            return Err("Turn unavailable");
        }
        let token = self.token();
        self.load = None;
        self.turn_deletions
            .insert(token, (session.into(), turn.into()));
        Ok(Effect::DeleteTurn {
            token,
            session_id: session.into(),
            turn_id: turn.into(),
        })
    }
    pub fn turn_deleted(&mut self, token: u64, success: bool) {
        let Some((session, turn)) = self.turn_deletions.remove(&token) else {
            return;
        };
        if success {
            if let Some(c) = self.owners.get_mut(&session) {
                c.turns.retain(|t| t.id != turn);
                c.context.retain(|t| t.id != turn);
                c.dirty.remove(&turn);
            }
        }
    }
    pub fn resolve_prompt(
        &mut self,
        session: &str,
        id: String,
        arguments: BTreeMap<String, String>,
    ) -> Result<Effect, &'static str> {
        let revision = self
            .conversation(session)
            .ok_or("Conversation unavailable")?
            .draft_revision;
        let token = self.token();
        self.prompt = Some((token, session.into(), revision));
        Ok(Effect::ResolvePrompt {
            token,
            id,
            arguments,
        })
    }
    pub fn prompt_resolved(&mut self, token: u64, text: Result<String, ()>) {
        let Some((t, id, revision)) = self.prompt.as_ref() else {
            return;
        };
        if *t != token {
            return;
        }
        let id = id.clone();
        let revision = *revision;
        self.prompt = None;
        if let Some(c) = self.owners.get_mut(&id) {
            if c.draft_revision == revision && c.deleted.is_none() {
                match text {
                    Ok(text) => {
                        c.draft = text;
                        c.draft_revision += 1;
                    }
                    Err(()) => {
                        c.input_error = Some("Could not resolve the prompt. Try again.".into())
                    }
                }
            }
        }
    }
    pub fn trim_idle(&mut self) {
        let active = self.active.as_deref();
        self.owners.retain(|id, c| {
            Some(id.as_str()) == active || c.busy() || c.unsaved() || c.deleted.is_some()
        });
    }
}

fn remember(context: &mut VecDeque<TurnIdentity>, turn: &Turn, max: usize) {
    let identity = TurnIdentity::from(turn);
    if let Some(old) = context.iter_mut().find(|t| t.id == turn.id) {
        *old = identity;
    } else {
        context.push_back(identity);
    }
    while context.len() > max {
        context.pop_front();
    }
}
impl RunRequest {
    pub fn core_prior_turns(&self) -> serde_json::Value {
        serde_json::Value::Array(self.prior_turns.iter().map(|t| serde_json::json!({"id":t.id,"role":t.role,"text":t.text,"error":t.error,"hasImages":t.references.iter().any(|r| matches!(r, Reference::Image { .. }))})).collect())
    }
}
impl ChatController {
    /// Optional usage supplied by the run transport, never by the current picker selection.
    pub fn token_count(&mut self, run: &str, count: u64) {
        for c in self.owners.values_mut() {
            if matches!(&c.phase, RunPhase::Running {run_id,..} if run_id == run) {
                if let Some(meta) = c.assistant.as_mut().and_then(|t| t.response_meta.as_mut()) {
                    meta.token_count = Some(count);
                }
            }
        }
    }
}

// Core normalizes usage through as_f64/round, so persisted JSON can contain 42.0.
fn usage_count<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<u64>, D::Error> {
    let value = Option::<f64>::deserialize(deserializer)?;
    value
        .map(|n| {
            if n.is_finite() && n >= 0.0 && n < u64::MAX as f64 {
                Ok(n.round() as u64)
            } else {
                Err(serde::de::Error::custom("Invalid token count"))
            }
        })
        .transpose()
}

impl ChatController {
    /// Preparation adapters resolve default provider/model/title from durable config
    /// before Start. The resolved snapshot belongs to this preparation token, never
    /// to a later picker edit. It also becomes the exact RunRequest selection.
    pub fn prepared_with_selection(
        &mut self,
        id: &str,
        token: u64,
        canonical_user: Result<Arc<Turn>, String>,
        resolved_selection: Selection,
        now: u64,
    ) -> Vec<Effect> {
        if resolved_selection
            .provider
            .as_deref()
            .is_none_or(str::is_empty)
            || resolved_selection
                .model
                .as_deref()
                .is_none_or(str::is_empty)
        {
            return self.prepared(id, token, Err("Unresolved run selection".into()), now);
        }
        if let Some(p) = self.owners.get_mut(id).and_then(|c| c.preparation.as_mut()) {
            if p.token == token {
                p.selection = resolved_selection;
            }
        }
        self.prepared(id, token, canonical_user, now)
    }
}
