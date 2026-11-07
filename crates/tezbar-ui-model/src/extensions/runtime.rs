use super::forms::FormState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};
// Every worker-emitted node is explicit; unknown nodes fail validation before entering a renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    List,
    #[serde(rename = "List.Section")]
    ListSection,
    #[serde(rename = "List.Item")]
    ListItem,
    #[serde(rename = "List.Item.Detail")]
    ListItemDetail,
    #[serde(rename = "List.EmptyView")]
    ListEmpty,
    #[serde(rename = "List.Dropdown")]
    ListDropdown,
    #[serde(rename = "List.Dropdown.Section")]
    ListDropdownSection,
    #[serde(rename = "List.Dropdown.Item")]
    ListDropdownItem,
    Grid,
    #[serde(rename = "Grid.Section")]
    GridSection,
    #[serde(rename = "Grid.Item")]
    GridItem,
    #[serde(rename = "Grid.EmptyView")]
    GridEmpty,
    Detail,
    Form,
    #[serde(rename = "Form.TextField")]
    TextField,
    #[serde(rename = "Form.TextArea")]
    TextArea,
    #[serde(rename = "Form.PasswordField")]
    PasswordField,
    #[serde(rename = "Form.Checkbox")]
    Checkbox,
    #[serde(rename = "Form.DatePicker")]
    DatePicker,
    #[serde(rename = "Form.Dropdown")]
    Dropdown,
    #[serde(rename = "Form.Dropdown.Section")]
    DropdownSection,
    #[serde(rename = "Form.Dropdown.Item")]
    DropdownItem,
    #[serde(rename = "Form.TagPicker")]
    TagPicker,
    #[serde(rename = "Form.TagPicker.Item")]
    TagPickerItem,
    #[serde(rename = "Form.FilePicker")]
    FilePicker,
    #[serde(rename = "Form.Description")]
    Description,
    #[serde(rename = "Form.Separator")]
    FormSeparator,
    #[serde(rename = "ActionPanel")]
    ActionPanel,
    #[serde(rename = "ActionPanel.Section")]
    ActionSection,
    Action,
    #[serde(rename = "Action.CopyToClipboard")]
    Copy,
    #[serde(rename = "Action.OpenInBrowser")]
    Open,
    #[serde(rename = "Action.Paste")]
    Paste,
    #[serde(rename = "Action.Push")]
    Push,
    #[serde(rename = "Action.Pop")]
    Pop,
    #[serde(rename = "Action.ShowInFinder")]
    ShowInFinder,
    #[serde(rename = "Action.SubmitForm")]
    SubmitForm,
    MenuBarExtra,
    #[serde(rename = "MenuBarExtra.Item")]
    MenuItem,
    #[serde(rename = "MenuBarExtra.Section")]
    MenuSection,
    #[serde(rename = "MenuBarExtra.Submenu")]
    MenuSubmenu,
    #[serde(rename = "MenuBarExtra.Separator")]
    MenuSeparator,
    #[serde(rename = "Tezbar.PreferenceSetup")]
    PreferenceSetup,
    #[serde(rename = "Detail.Metadata")]
    Metadata,
    #[serde(rename = "Detail.Metadata.Label")]
    MetadataLabel,
    #[serde(rename = "Detail.Metadata.Link")]
    MetadataLink,
    #[serde(rename = "Detail.Metadata.TagList")]
    MetadataTags,
    #[serde(rename = "Detail.Metadata.TagList.Item")]
    MetadataTag,
    #[serde(rename = "Detail.Metadata.Separator")]
    MetadataSeparator,
    #[serde(rename = "List.Item.Detail.Metadata")]
    ItemMetadata,
    #[serde(rename = "List.Item.Detail.Metadata.Label")]
    ItemMetadataLabel,
    #[serde(rename = "List.Item.Detail.Metadata.Link")]
    ItemMetadataLink,
    #[serde(rename = "List.Item.Detail.Metadata.TagList")]
    ItemMetadataTags,
    #[serde(rename = "List.Item.Detail.Metadata.TagList.Item")]
    ItemMetadataTag,
    #[serde(rename = "List.Item.Detail.Metadata.Separator")]
    ItemMetadataSeparator,
}
impl NodeKind {
    pub fn field(self) -> bool {
        matches!(
            self,
            Self::TextField
                | Self::TextArea
                | Self::PasswordField
                | Self::Checkbox
                | Self::DatePicker
                | Self::Dropdown
                | Self::TagPicker
                | Self::FilePicker
        )
    }
    pub fn row(self) -> bool {
        matches!(self, Self::ListItem | Self::GridItem | Self::MenuItem)
    }
    pub fn action(self) -> bool {
        matches!(
            self,
            Self::Action
                | Self::Copy
                | Self::Open
                | Self::Paste
                | Self::Push
                | Self::Pop
                | Self::ShowInFinder
                | Self::SubmitForm
        )
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Node {
    #[serde(rename = "type")]
    pub kind: NodeKind,
    #[serde(default)]
    pub props: Map<String, Value>,
    #[serde(default)]
    pub children: Vec<Node>,
    pub metadata: Option<Box<Node>>,
}
impl Node {
    pub fn string(&self, key: &str) -> Option<&str> {
        self.props.get(key).and_then(Value::as_str)
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        let mut count = 0;
        let mut bytes = 0;
        let mut ids = BTreeSet::new();
        self.validate_at(0, &mut count, &mut bytes, &mut ids)
    }
    fn validate_at(
        &self,
        depth: usize,
        count: &mut usize,
        bytes: &mut usize,
        fields: &mut BTreeSet<String>,
    ) -> Result<(), &'static str> {
        *count += 1;
        if depth > 64 || *count > 20_000 {
            return Err("Extension tree exceeds limits");
        }
        *bytes += serde_json::to_vec(&self.props)
            .map_err(|_| "Invalid props")?
            .len();
        if *bytes > 2 * 1024 * 1024 {
            return Err("Extension props exceed limits");
        }
        if self.kind.field() {
            if let Some(id) = self.string("id") {
                if !fields.insert(id.into()) {
                    return Err("Duplicate form field id");
                }
            }
        }
        for child in &self.children {
            child.validate_at(depth + 1, count, bytes, fields)?;
        }
        if let Some(m) = &self.metadata {
            m.validate_at(depth + 1, count, bytes, fields)?;
        }
        Ok(())
    }
    pub fn visit<'a>(&'a self, out: &mut Vec<&'a Node>) {
        out.push(self);
        for n in &self.children {
            n.visit(out);
        }
        if let Some(m) = &self.metadata {
            m.visit(out);
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shortcut {
    #[serde(default)]
    pub modifiers: Vec<String>,
    pub key: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Action {
    #[serde(default)]
    pub section: Option<String>,
    pub id: String,
    pub title: String,
    pub style: Option<String>,
    pub kind: Option<String>,
    pub shortcut: Option<Shortcut>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PollPolicy {
    Legacy,
    EventDriven,
    Interval(u64),
}
impl PollPolicy {
    fn interval(&self) -> Option<u64> {
        match self {
            Self::Legacy => Some(500),
            Self::EventDriven => None,
            Self::Interval(ms) => Some((*ms).max(500)),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionKey {
    pub extension_id: String,
    pub command_name: String,
    pub session_id: String,
}
/// Request token identifies latest renderer interest; worker generation identifies worker lifetime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestToken {
    pub key: SessionKey,
    pub generation: Option<String>,
    pub sequence: u64,
}
pub enum Effect {
    Refresh(RequestToken),
    Search {
        token: RequestToken,
        text: String,
    },
    LoadMore(RequestToken),
    Invoke {
        token: RequestToken,
        action_id: String,
        form_values: Option<BTreeMap<String, Value>>,
    },
    Dispose(SessionKey),
    Leave,
    Open(String),
    Toast {
        title: String,
        message: String,
    },
    Hud(String),
    PickFiles {
        token: RequestToken,
        field_id: String,
        multiple: bool,
    },
    NativeColorWheel {
        token: RequestToken,
        coordinates: Value,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    Working,
    Success,
    Error,
}
pub struct TreeReply {
    pub view_revision: u64,
    pub key: SessionKey,
    pub generation: Option<String>,
    pub root: Node,
    pub actions: Vec<Action>,
    pub refresh: PollPolicy,
    pub effects: Vec<WireEffect>,
    pub effect_sequence: Option<u64>,
}
impl TreeReply {
    pub fn from_wire(value: Value) -> Result<Self, &'static str> {
        if value["ok"] != true || value["mode"] != "view" {
            return Err("Expected view reply");
        }
        let string = |k: &str| {
            value[k]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .ok_or("Missing session identity")
        };
        let refresh = match value.get("refreshMs") {
            None => PollPolicy::Legacy,
            Some(Value::Null) => PollPolicy::EventDriven,
            Some(v) => PollPolicy::Interval(v.as_u64().ok_or("Invalid refresh interval")?),
        };
        let root: Node =
            serde_json::from_value(value["root"].clone()).map_err(|_| "Invalid extension node")?;
        root.validate()?;
        let actions =
            serde_json::from_value(value["actions"].clone()).map_err(|_| "Invalid actions")?;
        Ok(Self {
            view_revision: value["viewRevision"].as_u64().unwrap_or(0),
            key: SessionKey {
                extension_id: string("extensionId")?,
                command_name: string("commandName")?,
                session_id: string("sessionId")?,
            },
            generation: value["generation"].as_str().map(str::to_owned),
            effect_sequence: value["effectSequence"].as_u64(),
            root,
            actions,
            refresh,
            effects: serde_json::from_value(
                value.get("effects").cloned().unwrap_or_else(|| json!([])),
            )
            .map_err(|_| "Invalid effects")?,
        })
    }
}
#[derive(Clone, PartialEq, Eq)]
pub enum UiEvent {
    Select(String),
    Primary,
    Secondary,
    Action(String),
    ChangeField { id: String, value: Value },
    Dropdown { callback: String, value: String },
    Back,
    OpenActions,
    CloseActions,
    Menu(String),
    Link(String),
    PickFiles(String),
}
pub struct RuntimeSession {
    pub view_revision: u64,
    pub key: SessionKey,
    pub generation: Option<String>,
    pub root: Arc<Node>,
    pub actions: Vec<Action>,
    pub form: FormState,
    pub selection: Option<String>,
    pub action_panel: bool,
    pub notice: Option<Notice>,
    pub notice_message: Option<String>,
    pub search_text: String,
    rows: Vec<(String, String)>,
    callbacks: BTreeSet<String>,
    stack: Vec<Arc<Node>>,
    policy: PollPolicy,
    visible: bool,
    disposed: bool,
    revision: u64,
    sequence: u64,
    refreshing: bool,
    refresh_sequence: Option<u64>,
    dirty: bool,
    poll_at: Option<u64>,
    page_pending: bool,
    effects: VecDeque<Effect>,
    pending_submit: Option<u64>,
    records: WorkerRecords,
}
impl RuntimeSession {
    pub fn new(reply: TreeReply, now: u64) -> Result<Self, &'static str> {
        reply.root.validate()?;
        let mut s = Self {
            view_revision: reply.view_revision,
            key: reply.key,
            generation: reply.generation,
            root: Arc::new(reply.root),
            actions: reply.actions,
            form: FormState::default(),
            selection: None,
            action_panel: false,
            notice: None,
            notice_message: None,
            search_text: String::new(),
            rows: vec![],
            callbacks: BTreeSet::new(),
            stack: vec![],
            policy: reply.refresh,
            visible: true,
            disposed: false,
            revision: 0,
            sequence: 0,
            refreshing: false,
            refresh_sequence: None,
            dirty: false,
            poll_at: None,
            page_pending: false,
            effects: VecDeque::new(),
            pending_submit: None,
            records: WorkerRecords::default(),
        };
        s.records(reply.effects, reply.effect_sequence);
        s.rebuild();
        s.schedule(now);
        Ok(s)
    }
    fn rebuild(&mut self) {
        let previous_index = self
            .selection
            .as_ref()
            .and_then(|id| self.rows.iter().position(|r| &r.0 == id))
            .unwrap_or(0);
        self.rows.clear();
        self.callbacks = self.actions.iter().map(|a| a.id.clone()).collect();
        let mut nodes = vec![];
        self.root.visit(&mut nodes);
        for (index, n) in nodes.iter().enumerate() {
            for key in ["actionId", "backActionId"] {
                if let Some(id) = n.string(key) {
                    self.callbacks.insert(id.into());
                }
            }
            if n.kind.row() {
                let id = n
                    .string("id")
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("row:{index}"));
                let search = ["title", "subtitle"]
                    .into_iter()
                    .filter_map(|k| n.string(k))
                    .chain(
                        n.props
                            .get("keywords")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str),
                    )
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase();
                self.rows.push((id, search));
            }
        }
        self.form.reconcile(&self.root);
        let rows = self.filtered_ids();
        if !self.selection.as_ref().is_some_and(|id| rows.contains(id)) {
            self.selection = rows
                .get(previous_index.min(rows.len().saturating_sub(1)))
                .cloned();
        }
    }
    fn token(&mut self) -> RequestToken {
        self.sequence += 1;
        RequestToken {
            key: self.key.clone(),
            generation: self.generation.clone(),
            sequence: self.sequence,
        }
    }
    fn schedule(&mut self, now: u64) {
        self.poll_at = if self.visible && !self.disposed {
            self.policy.interval().map(|ms| now + ms)
        } else {
            None
        };
    }
    pub fn accepts(&self, t: &RequestToken) -> bool {
        !self.disposed
            && t.key == self.key
            && t.generation == self.generation
            && t.sequence == self.sequence
    }
    fn refresh(&mut self) -> Option<Effect> {
        if self.disposed {
            return None;
        }
        if !self.visible || self.refreshing {
            self.dirty = true;
            return None;
        }
        self.refreshing = true;
        self.dirty = false;
        self.poll_at = None;
        {
            let token = self.token();
            self.refresh_sequence = Some(token.sequence);
            Some(Effect::Refresh(token))
        }
    }
    pub fn invalidate(
        &mut self,
        session: &str,
        generation: Option<&str>,
        revision: u64,
    ) -> Option<Effect> {
        if session != self.key.session_id
            || generation != self.generation.as_deref()
            || revision <= self.revision
        {
            return None;
        }
        self.revision = revision;
        self.refresh()
    }
    pub fn tick(&mut self, now: u64) -> Option<Effect> {
        if self.poll_at.is_some_and(|t| now >= t) {
            self.refresh()
        } else {
            None
        }
    }
    pub fn visibility(&mut self, visible: bool, now: u64) -> Option<Effect> {
        if self.visible == visible {
            return None;
        }
        self.visible = visible;
        self.poll_at = None;
        if visible && (self.dirty || self.policy.interval().is_some()) {
            self.refresh()
        } else {
            self.schedule(now);
            None
        }
    }
    pub fn refresh_finished(
        &mut self,
        token: &RequestToken,
        reply: Option<TreeReply>,
        now: u64,
    ) -> Result<Option<Effect>, &'static str> {
        if self.pending_submit == Some(token.sequence)
            && token.key == self.key
            && token.generation == self.generation
        {
            self.pending_submit = None;
        }
        if !self.accepts(token) {
            if token.key == self.key
                && token.generation == self.generation
                && self.refresh_sequence == Some(token.sequence)
            {
                self.refreshing = false;
                self.refresh_sequence = None;
                if self.dirty && self.visible {
                    return Ok(self.refresh());
                }
                self.schedule(now);
            }
            return Ok(None);
        }
        self.notice = None;
        self.notice_message = None;
        if let Some(r) = reply {
            if r.key != self.key || r.generation != self.generation {
                return Err("Reply belongs to another session");
            }
            r.root.validate()?;
            self.records(r.effects, r.effect_sequence);
            self.view_revision = r.view_revision;
            self.root = Arc::new(r.root);
            self.actions = r.actions;
            self.policy = r.refresh;
            self.rebuild();
        }
        self.refreshing = false;
        self.refresh_sequence = None;
        self.page_pending = false;
        if self.dirty && self.visible {
            Ok(self.refresh())
        } else {
            self.schedule(now);
            Ok(None)
        }
    }
    pub fn failed(&mut self, token: &RequestToken, now: u64) -> Option<Effect> {
        if self.pending_submit == Some(token.sequence)
            && token.key == self.key
            && token.generation == self.generation
        {
            self.pending_submit = None;
        }
        if !self.accepts(token) {
            return None;
        }
        self.notice = Some(Notice::Error);
        self.pending_submit = None;
        self.refreshing = false;
        self.refresh_sequence = None;
        self.page_pending = false;
        if self.dirty && self.visible {
            self.refresh()
        } else {
            self.schedule(now);
            None
        }
    }
    pub fn search(&mut self, text: String) -> Option<Effect> {
        self.sequence += 1;
        self.search_text = text;
        self.selection = None;
        if self.root.props.get("__hasServerSearch") == Some(&Value::Bool(true)) {
            Some(Effect::Search {
                token: self.token(),
                text: self.search_text.clone(),
            })
        } else {
            self.selection = self.filtered_ids().first().cloned();
            None
        }
    }
    pub fn filtered_ids(&self) -> Vec<String> {
        let server = self.root.props.get("__hasServerSearch") == Some(&Value::Bool(true));
        let filtering = self.root.props.get("filtering") != Some(&Value::Bool(false));
        let needle = self.search_text.trim().to_lowercase();
        self.rows
            .iter()
            .filter(|(_, s)| server || !filtering || s.contains(&needle))
            .map(|(id, _)| id.clone())
            .collect()
    }
    pub fn select(&mut self, id: String) -> Option<Effect> {
        let rows = self.filtered_ids();
        let index = rows.iter().position(|r| *r == id)?;
        self.selection = Some(id);
        let has_more = self
            .root
            .props
            .get("pagination")
            .is_some_and(|p| p["hasMore"] == true)
            || self.root.props.get("__hasMore") == Some(&Value::Bool(true));
        if has_more && !self.page_pending && index + 3 >= rows.len() {
            self.page_pending = true;
            Some(Effect::LoadMore(self.token()))
        } else {
            None
        }
    }
    pub fn invoke(
        &mut self,
        id: &str,
        values: Option<BTreeMap<String, Value>>,
    ) -> Result<Effect, &'static str> {
        if self.disposed || !self.callbacks.contains(id) {
            return Err("Callback does not belong to this session");
        }
        let submit = self
            .actions
            .iter()
            .any(|a| a.id == id && a.kind.as_deref() == Some("submit-form"));
        if submit && self.pending_submit.is_some() {
            return Err("Form submission is in progress");
        }
        self.notice = Some(Notice::Working);
        self.notice_message = None;
        let token = self.token();
        if submit {
            self.pending_submit = Some(token.sequence);
        }
        Ok(Effect::Invoke {
            token,
            action_id: id.into(),
            form_values: values,
        })
    }
    pub fn primary(&mut self, secondary: bool) -> Result<Option<Effect>, &'static str> {
        let mut nodes = vec![];
        self.root.visit(&mut nodes);
        let row_actions = self
            .selection
            .as_ref()
            .and_then(|id| nodes.iter().find(|n| n.string("id") == Some(id.as_str())))
            .and_then(|n| n.props.get("actionIds"))
            .and_then(Value::as_array);
        let index = usize::from(secondary);
        let id = row_actions
            .and_then(|a| a.get(index))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| self.actions.get(index).map(|a| a.id.clone()));
        let Some(id) = id else {
            return Ok(None);
        };
        if self
            .actions
            .iter()
            .any(|a| a.id == id && a.kind.as_deref() == Some("submit-form"))
        {
            self.form.validate()?;
            return self.invoke(&id, Some(self.form.values().clone())).map(Some);
        }
        self.invoke(&id, None).map(Some)
    }
    pub fn shortcut(
        &mut self,
        key: &str,
        modifiers: &[String],
    ) -> Result<Option<Effect>, &'static str> {
        let mods: BTreeSet<_> = modifiers.iter().map(String::as_str).collect();
        let id = self
            .actions
            .iter()
            .find(|a| {
                a.shortcut.as_ref().is_some_and(|s| {
                    s.key.as_deref() == Some(key)
                        && s.modifiers
                            .iter()
                            .map(String::as_str)
                            .collect::<BTreeSet<_>>()
                            == mods
                })
            })
            .map(|a| a.id.clone());
        id.map(|id| self.invoke(&id, None)).transpose()
    }
    pub fn change(&mut self, id: &str, value: Value) -> Result<Option<Effect>, &'static str> {
        let callback = self.form.change(id, value.clone())?;
        callback
            .map(|id| self.invoke(&id, Some([("value".into(), value)].into())))
            .transpose()
    }
    pub fn push(&mut self, root: Node) -> Result<(), &'static str> {
        root.validate()?;
        if self.stack.len() >= 32 {
            return Err("Navigation stack is full");
        }
        self.stack.push(self.root.clone());
        self.root = Arc::new(root);
        self.rebuild();
        Ok(())
    }
    pub fn back(&mut self) -> Result<Option<Effect>, &'static str> {
        if let Some(id) = self.root.string("backActionId").map(str::to_owned) {
            return self.invoke(&id, None).map(Some);
        }
        if let Some(root) = self.stack.pop() {
            self.root = root;
            self.rebuild();
            return Ok(None);
        }
        Ok(Some(Effect::Leave))
    }
    pub fn dispose(&mut self) -> Option<Effect> {
        if self.disposed {
            return None;
        }
        self.disposed = true;
        self.sequence += 1;
        self.stack.clear();
        self.form.clear();
        self.callbacks.clear();
        self.poll_at = None;
        Some(Effect::Dispose(self.key.clone()))
    }
    pub fn drain_effects(&mut self) -> impl Iterator<Item = Effect> + '_ {
        self.effects.drain(..)
    }
    pub fn no_view(
        &mut self,
        token: &RequestToken,
        message: String,
        effects: Vec<WireEffect>,
        effect_sequence: Option<u64>,
        now: u64,
    ) {
        if self.pending_submit == Some(token.sequence)
            && token.key == self.key
            && token.generation == self.generation
        {
            self.pending_submit = None;
        }
        if !self.accepts(token) {
            return;
        }
        self.refreshing = false;
        self.refresh_sequence = None;
        self.page_pending = false;
        let has_record_notice = effects
            .iter()
            .any(|e| matches!(e, WireEffect::Toast { .. } | WireEffect::Hud { .. }));
        self.notice_message = (!has_record_notice && !message.is_empty()).then_some(message);
        self.notice = self.notice_message.as_ref().map(|_| Notice::Success);
        self.records(effects, effect_sequence);
        self.schedule(now);
    }
    pub fn action_filter(&self, query: &str) -> Vec<&Action> {
        let q = query.to_lowercase();
        self.actions
            .iter()
            .filter(|a| a.title.to_lowercase().contains(&q))
            .collect()
    }
    pub fn ui_event(&mut self, event: UiEvent) -> Result<Option<Effect>, &'static str> {
        match event {
            UiEvent::Select(id) => Ok(self.select(id)),
            UiEvent::Primary => self.primary(false),
            UiEvent::Secondary => self.primary(true),
            UiEvent::Action(id) | UiEvent::Menu(id) => self.invoke(&id, None).map(Some),
            UiEvent::Dropdown { callback, value } => self
                .invoke(&callback, Some([("value".into(), json!(value))].into()))
                .map(Some),
            UiEvent::Back => self.back(),
            UiEvent::OpenActions => {
                self.action_panel = true;
                Ok(None)
            }
            UiEvent::CloseActions => {
                self.action_panel = false;
                Ok(None)
            }
            UiEvent::PickFiles(field_id) => {
                let multiple = self.form.multiple(&field_id)?;
                Ok(Some(Effect::PickFiles {
                    token: self.token(),
                    field_id,
                    multiple,
                }))
            }
            UiEvent::Link(url) => {
                let u = url::Url::parse(&url).map_err(|_| "Invalid link")?;
                if !matches!(u.scheme(), "https" | "http" | "mailto") {
                    return Err("Unsupported link scheme");
                }
                Ok(Some(Effect::Open(url)))
            }
            UiEvent::ChangeField { id, value } => self.change(&id, value),
        }
    }
}
#[cfg(test)]
mod tests;

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WireEffect {
    Clipboard {
        value: String,
    },
    Open {
        value: String,
    },
    ShowInFinder {
        value: String,
    },
    Toast {
        title: Option<String>,
        message: Option<String>,
        style: Option<String>,
    },
    Hud {
        title: Option<String>,
        message: Option<String>,
    },
    /// Already executed by the worker/core. Never execute a script again in the native renderer.
    AppleScript {
        #[serde(default)]
        value: Option<String>,
    },
    PowershellScript {
        #[serde(default)]
        value: Option<String>,
    },
}
/// A bounded cursor per worker session/generation, also usable for initial no-view
/// replies that have no RuntimeSession. Values never authorize desktop operations.
#[derive(Default)]
pub struct WorkerRecords {
    sequence: u64,
    legacy_records: Vec<u64>,
}
impl WorkerRecords {
    /// Returns only the newest fresh toast/HUD notice. Legacy replies lack record
    /// identities: repeated identical notices are conservatively suppressed.
    pub fn notice(&mut self, effects: &[WireEffect], sequence: Option<u64>) -> Option<String> {
        use std::hash::{Hash, Hasher};
        let fingerprints: Vec<_> = effects
            .iter()
            .map(|e| {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                serde_json::to_string(e).unwrap_or_default().hash(&mut h);
                h.finish()
            })
            .collect();
        let base = sequence.unwrap_or(0).saturating_sub(effects.len() as u64);
        let mut notice = None;
        for (index, effect) in effects.iter().enumerate() {
            let fresh = match sequence {
                Some(end) => end > self.sequence && base + index as u64 + 1 > self.sequence,
                None => !self.legacy_records.contains(&fingerprints[index]),
            };
            if fresh {
                match effect {
                    WireEffect::Toast { title, message, .. }
                    | WireEffect::Hud { title, message } => {
                        notice = message
                            .as_ref()
                            .filter(|s| !s.is_empty())
                            .or(title.as_ref())
                            .cloned();
                    }
                    _ => {} // Copies, open/reveal and scripts already ran in worker.
                }
            }
        }
        if let Some(end) = sequence {
            self.sequence = self.sequence.max(end);
        }
        self.legacy_records = fingerprints.into_iter().rev().take(50).collect();
        notice
    }
}
impl RuntimeSession {
    fn records(&mut self, effects: Vec<WireEffect>, sequence: Option<u64>) {
        if let Some(message) = self.records.notice(&effects, sequence) {
            self.notice_message = Some(message);
            self.notice = Some(Notice::Success);
        }
    }
}
/// Adaptive grid colors use the same dark-first preference as the current renderer.
pub fn color_from_grid_content(content: &Value) -> Option<&str> {
    let value = content
        .get("value")
        .filter(|v| v.is_object())
        .unwrap_or(content);
    let color = value.get("color")?;
    color
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| color["dark"].as_str().filter(|s| !s.trim().is_empty()))
        .or_else(|| color["light"].as_str().filter(|s| !s.trim().is_empty()))
        .map(str::trim)
}
