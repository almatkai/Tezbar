//! Retained native chat. ChatController owns conversations; core owns execution and canonical data.
mod extensions;
mod references;
mod request;
mod runtime;
#[cfg(test)]
mod tests;
mod view;
use super::settings::SettingsStore;
use gpui::{prelude::*, *};
pub use request::ChatRequest;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tezbar_ui::{
    image_cache::ImageCache,
    markdown::{DocumentEffect, MarkdownView},
    text_input::{ByteSelection, Committed, FieldOptions, TextInput},
};
use tezbar_ui_model::{
    chat::{
        self, ApprovalFocus, ApprovalMode, Boot, ChatController, Decision, Effect, Reference,
        RunPhase, Selection, Turn,
    },
    settings::{Edit, OwnerId},
    ui_state::UiState,
};
pub type ChatReply = Result<Value, String>;
pub type ChatFuture = std::pin::Pin<Box<dyn std::future::Future<Output = ChatReply> + Send>>;
pub trait ChatTransport: Send + Sync + 'static {
    fn send(&self, request: ChatRequest) -> ChatFuture;
}
/// Parent handles these through its existing navigation/host owners. A ticket is view-local.
#[derive(Clone)]
pub enum ChatEffect {
    Back,
    Settings,
    ReviewProject { cwd: String, nonce: String },
    ChooseImages { ticket: u64 },
    ClipboardImage { ticket: u64 },
    DocumentImage { ticket: u64, source: String },
    Background(Vec<chat::BackgroundStatus>),
}
impl EventEmitter<ChatEffect> for ChatView {}
#[derive(Clone)]
pub(crate) enum Action {
    Back,
    Panel,
    New,
    Resume(String),
    Send,
    Stop,
    Retry(String),
    Save,
    RetryImages,
    Delete(String),
    ConfirmDelete,
    Clear,
    CancelDelete,
    DeleteTurn(String),
    Capture,
    ChooseImages,
    ClipboardImage,
    RemoveImage(usize),
    Directory,
    DirectoryMenu,
    SetDirectory(String),
    Models,
    Model(String, String),
    Mode(ApprovalMode),
    Decide(Decision),
    Settings,
    ReviewProject,
    DismissProject,
    DismissError,
    ChoosePrompt(usize),
    ResolvePrompt,
    CancelPrompt,
    ClearProfile,
    Expand(String),
    Copy(String),
    Open(String),
}
struct DocumentEntry {
    view: Entity<MarkdownView>,
    _subscription: Subscription,
}
/// An admission-fenced host completion, never a global clipboard/screen inbox.
struct AttachmentTicket {
    session: String,
    revision: u64,
    mount: u64,
}
pub struct ChatView {
    pub controller: ChatController,
    transport: Arc<dyn ChatTransport>,
    store: SettingsStore,
    owner: OwnerId,
    state: UiState,
    window: AnyWindowHandle,
    pub(crate) input: Entity<TextInput>,
    filter: Entity<TextInput>,
    model_filter: Entity<TextInput>,
    _subscriptions: Vec<Subscription>,
    focus: FocusHandle,
    allow_focus: FocusHandle,
    deny_focus: FocusHandle,
    always_focus: FocusHandle,
    confirm_focus: FocusHandle,
    cancel_focus: FocusHandle,
    jobs: BTreeMap<u64, Task<()>>,
    next_job: u64,
    clock: Option<Task<()>>,
    frame: Option<Task<()>>,
    documents: BTreeMap<String, DocumentEntry>,
    images: ImageCache,
    document_images: BTreeMap<u64, references::PendingImage>,
    blobs: references::ImagePool,
    config: Value,
    activations: std::collections::VecDeque<(Boot, String)>,
    owners: BTreeSet<String>,
    disposed: bool,
    quit_preparing: bool,
    decode_jobs: BTreeSet<u64>,
    decode_slots: Arc<tokio::sync::Semaphore>,
    thumbnail_attempts: BTreeSet<String>,
    init_generation: u64,
    read_generation: u64,
    mount: u64,
    draft_revision: u64,
    attachment_tickets: BTreeMap<u64, AttachmentTicket>,
    pending_capture: Option<(u64, AttachmentTicket)>,
    error: Option<String>,
    loading: bool,
    history_error: bool,
    delete_pending: Option<Vec<String>>,
    clear_pending: bool,
    clear_inflight: bool,
    deleting: BTreeSet<String>,
    models_open: bool,
    directories_open: bool,
    recent_directories: Vec<String>,
    metadata: BTreeMap<String, (String, Option<String>)>,
    metadata_requested: BTreeSet<String>,
    items: Vec<Value>,
    item_scope: Option<String>,
    prompt: Option<Value>,
    arguments: BTreeMap<String, Entity<TextInput>>,
    prompt_generation: u64,
    slash_index: usize,
    slash_dismissed: bool,
    project: Option<String>,
    dismissed_projects: BTreeSet<String>,
    expanded: BTreeSet<String>,
    diagnostics: BTreeMap<String, Vec<String>>,
    runs: BTreeMap<String, String>,
    last_background: Vec<chat::BackgroundStatus>,
    footer: Option<AnyView>,
    effects: Vec<(u64, ChatEffect)>,
    mutation_jobs: std::collections::BTreeSet<u64>,
    scroll: ScrollHandle,
    transcript: ListState,
    projection: Vec<view::Row>,
    follow: bool,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn identity(prefix: &str) -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "{prefix}-{}-{}",
        now(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
fn ok(result: &ChatReply) -> bool {
    matches!(result,Ok(v) if v["ok"] != false)
}
impl ChatView {
    pub fn new(
        transport: Arc<dyn ChatTransport>,
        store: SettingsStore,
        state: UiState,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            let mut v = TextInput::new(cx);
            v.configure(FieldOptions {
                name: "Message".into(),
                placeholder: "Ask anything…".into(),
                multiline: true,
                ..Default::default()
            });
            v
        });
        let filter = cx.new(|cx| {
            let mut v = TextInput::new(cx);
            v.configure(FieldOptions {
                name: "Filter past chats".into(),
                placeholder: "Search conversations…".into(),
                ..Default::default()
            });
            v
        });
        let model_filter = cx.new(|cx| {
            let mut v = TextInput::new(cx);
            v.configure(FieldOptions {
                name: "Search models".into(),
                placeholder: "Find a model…".into(),
                ..Default::default()
            });
            v
        });
        let transcript = ListState::new(0, ListAlignment::Top, px(200.));
        transcript.set_scroll_handler(cx.listener(|v, e: &ListScrollEvent, _, _| {
            v.follow = e.visible_range.end + 1 >= v.transcript.item_count();
        }));
        let sub = cx.subscribe(&input, |this, _, e: &Committed, cx| {
            if let Some(id) = this.controller.active.clone() {
                this.controller.edit_draft(&id, e.0.clone());
                this.draft_revision += 1;
                this.prompt_generation += 1;
                this.slash_index = 0;
                this.slash_dismissed = false;
            }
            cx.notify();
        });
        let a = cx.observe(&filter, |_, _, cx| cx.notify());
        let b = cx.observe(&model_filter, |_, _, cx| cx.notify());
        let release = cx.on_release(|v, cx| {
            v.images.release_all_windows(cx);
            v.document_images.clear();
            v.store.retire(v.owner);
            if v.disposed {
                return;
            }
            let effects = v.controller.unmount();
            let mut requests = vec![];
            for e in effects {
                match e {
                    Effect::Cancel { run_id } => requests.push(ChatRequest::Cancel(run_id)),
                    Effect::PersistTurn {
                        session_id, turn, ..
                    } => {
                        let selection = v
                            .controller
                            .conversation(&session_id)
                            .map(|c| c.selection.clone())
                            .unwrap_or_default();
                        let title = v
                            .metadata
                            .get(&session_id)
                            .map(|m| m.0.clone())
                            .unwrap_or_else(|| chat::title(&turn.text));
                        requests.push(ChatRequest::Append {
                            session: session_id,
                            title,
                            selection,
                            turn,
                        });
                    }
                    _ => {}
                }
            }
            if !requests.is_empty() {
                let transport = v.transport.clone();
                gpui_tokio::Tokio::spawn(cx, async move {
                    let _ = tokio::time::timeout(Duration::from_secs(8), async move {
                        for request in requests {
                            let _ = transport.send(request).await;
                        }
                    })
                    .await;
                })
                .detach();
            }
        });
        Self {
            controller: ChatController::default(),
            transport,
            owner: store.owner(),
            store,
            state,
            window: w.window_handle(),
            input,
            filter,
            model_filter,
            _subscriptions: vec![sub, a, b, release],
            focus: cx.focus_handle(),
            allow_focus: cx.focus_handle(),
            deny_focus: cx.focus_handle(),
            always_focus: cx.focus_handle(),
            confirm_focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            jobs: BTreeMap::new(),
            next_job: 0,
            clock: None,
            frame: None,
            documents: BTreeMap::new(),
            images: ImageCache::default(),
            document_images: BTreeMap::new(),
            blobs: Default::default(),
            config: Value::Null,
            activations: Default::default(),
            owners: BTreeSet::new(),
            disposed: false,
            quit_preparing: false,
            decode_jobs: BTreeSet::new(),
            decode_slots: Arc::new(tokio::sync::Semaphore::new(4)),
            thumbnail_attempts: BTreeSet::new(),
            init_generation: 0,
            read_generation: 0,
            mount: 0,
            draft_revision: 0,
            attachment_tickets: BTreeMap::new(),
            pending_capture: None,
            error: None,
            loading: false,
            history_error: false,
            delete_pending: None,
            clear_pending: false,
            clear_inflight: false,
            deleting: BTreeSet::new(),
            models_open: false,
            directories_open: false,
            recent_directories: vec![],
            metadata: BTreeMap::new(),
            metadata_requested: BTreeSet::new(),
            items: vec![],
            item_scope: None,
            prompt: None,
            arguments: BTreeMap::new(),
            prompt_generation: 0,
            slash_index: 0,
            slash_dismissed: false,
            project: None,
            dismissed_projects: BTreeSet::new(),
            expanded: BTreeSet::new(),
            diagnostics: BTreeMap::new(),
            runs: BTreeMap::new(),
            last_background: vec![],
            footer: None,
            effects: vec![],
            mutation_jobs: Default::default(),
            scroll: ScrollHandle::new(),
            transcript,
            projection: vec![],
            follow: true,
        }
    }
    pub fn set_task_indicator(&mut self, footer: AnyView, cx: &mut Context<Self>) {
        self.footer = Some(footer);
        cx.notify();
    }
    pub fn take_effects(&mut self) -> Vec<ChatEffect> {
        self.take_host_effects()
            .into_iter()
            .map(|(_, effect)| effect)
            .collect()
    }
    pub(crate) fn take_host_effects(&mut self) -> Vec<(u64, ChatEffect)> {
        std::mem::take(&mut self.effects)
    }
    pub(crate) fn host_generation(&self) -> u64 {
        self.mount
    }
    #[cfg(test)]
    pub(crate) fn footer_entity_id(&self) -> Option<EntityId> {
        self.footer.as_ref().map(|v| v.entity_id())
    }
    fn emit(&mut self, e: ChatEffect, cx: &mut Context<Self>) {
        if matches!(e, ChatEffect::Background(_)) {
            self.effects
                .retain(|(_, e)| !matches!(e, ChatEffect::Background(_)));
        }
        if self.effects.len() >= 64 {
            // Complete refused tickets synchronously. Never emit an unqueued effect.
            match e {
                ChatEffect::ChooseImages { ticket } | ChatEffect::ClipboardImage { ticket } => {
                    self.attachment_tickets.remove(&ticket);
                    self.error = Some("Native action queue is busy. Retry.".into());
                }
                ChatEffect::DocumentImage { ticket, .. } => {
                    let view = cx.entity().downgrade();
                    let window = self.window;
                    cx.defer(move |cx| {
                        let _ = window.update(cx, |_, w, cx| {
                            let _ = view.update(cx, |v, cx| {
                                v.document_image_result(
                                    ticket,
                                    Err("Image queue is busy. Retry image previews.".into()),
                                    w,
                                    cx,
                                )
                            });
                        });
                    });
                }
                _ => {
                    self.error = Some("Native action queue is busy. Retry.".into());
                }
            }
            cx.notify();
            return;
        }
        self.effects.push((self.mount, e.clone()));
        cx.emit(e);
        cx.notify();
    }
    pub fn activate(
        &mut self,
        boot: Boot,
        nonce: String,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.disposed || self.quit_preparing {
            return Err("Chat is preparing to quit".into());
        }
        if let Some((previous, _)) = self.activations.iter().find(|(_, n)| n == &nonce) {
            return if previous == &boot {
                Ok(())
            } else {
                Err("Navigation identity was reused".into())
            };
        }
        if self.clear_inflight {
            return Err("Conversations are being cleared".into());
        }
        let submit = matches!(&boot, Boot::Submit { .. });
        let activation = (boot.clone(), nonce.clone());
        let effects = self
            .controller
            .boot(boot, Some(nonce))
            .map_err(str::to_string)?;
        self.activations.push_back(activation);
        if self.activations.len() > 256 {
            self.activations.pop_front();
        }
        if let Some(id) = &self.controller.active {
            self.owners.insert(id.clone());
        }
        self.loading = false;
        self.history_error = false;
        self.directories_open = false;
        self.documents.clear();
        self.document_images.clear();
        self.thumbnail_attempts.clear();
        self.cancel_decode_waiters();
        self.images.release(w, cx);
        self.prune();
        self.init_generation += 1;
        self.mount += 1;
        self.draft_revision += 1;
        self.pending_capture = None;
        self.attachment_tickets.clear();
        self.prompt = None;
        self.arguments.clear();
        self.prompt_generation += 1;
        self.project = None;
        self.delete_pending = None;
        self.clear_pending = false;
        self.models_open = false;
        self.error = None;
        self.sync_input(w, cx);
        self.execute(effects, w, cx);
        self.refresh_scope(cx);
        self.refresh_documents(cx);
        self.refresh_thumbnails(w, cx);
        if self.clock.is_none() {
            let window = self.window;
            self.clock = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(250))
                        .await;
                    if cx
                        .update_window(window, |_, w, cx| {
                            this.update(cx, |v, cx| {
                                let e = v.controller.tick(now());
                                if !e.is_empty() {
                                    v.execute(e, w, cx);
                                }
                            })
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
        if self.active().is_none_or(|c| c.approval.is_none()) {
            self.input.focus_handle(cx).focus(w, cx);
        }
        if submit {
            self.action(Action::Send, w, cx);
        }
        cx.notify();
        Ok(())
    }
    pub(crate) fn set_preview_admission(&mut self, slots: Arc<tokio::sync::Semaphore>) {
        self.decode_slots = slots;
    }
    fn cancel_decode_waiters(&mut self) {
        for job in std::mem::take(&mut self.decode_jobs) {
            self.jobs.remove(&job);
        }
    }
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.disposed || self.controller.visible == visible {
            return;
        }
        self.controller.set_visible(visible);
        self.mount += 1;
        self.pending_capture = None;
        self.attachment_tickets.clear();
        self.prompt_generation += 1;
        if !visible {
            self.documents.clear();
            self.document_images.clear();
            self.thumbnail_attempts.clear();
            self.cancel_decode_waiters();
            self.images.release_all_windows(cx);
            self.frame = None;
            self.prompt = None;
            self.arguments.clear();
            self.models_open = false;
            self.expanded.clear();
        } else {
            self.refresh_documents(cx);
        }
        cx.notify();
    }
    pub fn set_ui_state(&mut self, state: &UiState, cx: &mut Context<Self>) {
        self.state = state.clone();
        cx.notify();
    }
    fn sync_input(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let text = self.active().map(|c| c.draft.clone()).unwrap_or_default();
        if self.input.read(cx).content.as_ref() == text {
            return;
        }
        self.input.update(cx, |v, cx| {
            v.replace_silently(
                &text,
                ByteSelection {
                    range: text.len()..text.len(),
                    reversed: false,
                },
                w,
                cx,
            )
        });
    }
    fn active(&self) -> Option<&chat::Conversation> {
        self.controller
            .active
            .as_deref()
            .and_then(|id| self.controller.conversation(id))
    }
    fn publish(&mut self, cx: &mut Context<Self>) {
        let status = self.controller.background();
        if status != self.last_background {
            self.last_background = status.clone();
            self.emit(ChatEffect::Background(status), cx);
        }
    }
    fn prune(&mut self) {
        if self.clear_inflight {
            return;
        }
        let controller = &self.controller;
        let deleting = &self.deleting;
        let retained = |id: &str| controller.conversation(id).is_some() || deleting.contains(id);
        self.owners.retain(|id| retained(id));
        self.runs.retain(|_, id| retained(id));
        self.metadata.retain(|id, _| retained(id));
        self.metadata_requested.retain(|id| retained(id));
        self.diagnostics.retain(|id, _| retained(id));
        self.blobs.prune_owners(controller, deleting);
    }
    fn selection(&self) -> Selection {
        self.active()
            .map(|c| c.selection.clone())
            .unwrap_or_default()
    }
    pub(crate) fn action(&mut self, a: Action, w: &mut Window, cx: &mut Context<Self>) {
        if self.disposed || self.quit_preparing {
            return;
        }
        if self.clear_inflight
            && !matches!(a, Action::Back | Action::Settings | Action::DismissError)
        {
            return;
        }
        if self.delete_pending.is_some()
            && !matches!(
                a,
                Action::ConfirmDelete
                    | Action::CancelDelete
                    | Action::Back
                    | Action::Settings
                    | Action::DismissError
            )
        {
            return;
        }
        let restore_focus = self.approval_has_focus(w);
        let id = self.controller.active.clone();
        let result: Result<Vec<Effect>, String> = (|| {
            Ok(match a {
                Action::Back => {
                    self.emit(ChatEffect::Back, cx);
                    vec![]
                }
                Action::Settings => {
                    self.emit(ChatEffect::Settings, cx);
                    vec![]
                }
                Action::Panel => {
                    self.activate(Boot::Panel, identity("nav"), w, cx)?;
                    vec![]
                }
                Action::New => {
                    self.activate(
                        Boot::NewChat {
                            session_id: identity("chat"),
                            cwd: self.selection().cwd,
                        },
                        identity("nav"),
                        w,
                        cx,
                    )?;
                    vec![]
                }
                Action::Resume(s) => {
                    self.activate(Boot::Resume { session_id: s }, identity("nav"), w, cx)?;
                    vec![]
                }
                Action::Send => {
                    if self.pending_capture.is_some() || !self.attachment_tickets.is_empty() {
                        return Err("Wait for the image action to finish before sending.".into());
                    }
                    let s = id.as_deref().ok_or("Open a conversation")?;
                    vec![
                        self.controller
                            .send(
                                s,
                                identity("agent"),
                                identity("user"),
                                identity("assistant"),
                                now(),
                            )
                            .map_err(str::to_string)?,
                    ]
                }
                Action::Stop => id
                    .as_deref()
                    .map(|s| self.controller.stop(s))
                    .unwrap_or_default(),
                Action::Retry(turn) => {
                    let s = id.as_deref().ok_or("Open a conversation")?;
                    vec![
                        self.controller
                            .retry(s, &turn, identity("agent"))
                            .map_err(str::to_string)?,
                    ]
                }
                Action::RetryImages => {
                    self.thumbnail_attempts.clear();
                    self.document_images.clear();
                    self.cancel_decode_waiters();
                    self.images.release(w, cx);
                    for document in self.documents.values() {
                        document.view.update(cx, |v, cx| v.reset_images(cx));
                    }
                    vec![]
                }
                Action::Save => id
                    .as_deref()
                    .map(|s| self.controller.retry_persistence(s))
                    .unwrap_or_default(),
                Action::Delete(s) => {
                    self.delete_pending = Some(vec![s]);
                    self.cancel_focus.focus(w, cx);
                    vec![]
                }
                Action::Clear => {
                    self.clear_pending = true;
                    let mut ids = self
                        .controller
                        .history
                        .as_ref()
                        .into_iter()
                        .flatten()
                        .map(|r| r.id.clone())
                        .collect::<BTreeSet<_>>();
                    ids.extend(
                        self.controller
                            .background()
                            .into_iter()
                            .map(|r| r.session_id),
                    );
                    if let Some(s) = id {
                        ids.insert(s);
                    }
                    self.delete_pending = Some(ids.into_iter().collect());
                    self.cancel_focus.focus(w, cx);
                    vec![]
                }
                Action::CancelDelete => {
                    self.delete_pending = None;
                    self.clear_pending = false;
                    self.input.focus_handle(cx).focus(w, cx);
                    vec![]
                }
                Action::ConfirmDelete if self.clear_pending => {
                    self.clear_pending = false;
                    self.delete_pending = None;
                    self.clear_all(w, cx);
                    vec![]
                }
                Action::ConfirmDelete => {
                    let ids = self.delete_pending.take().unwrap_or_default();
                    let mut e = vec![];
                    for s in ids {
                        self.deleting.insert(s.clone());
                        if self.controller.conversation(&s).is_some() {
                            e.extend(self.controller.delete(&s));
                        } else {
                            self.request(
                                ChatRequest::Delete(s.clone()),
                                runtime::Completion::DeleteStored(s),
                                cx,
                            );
                        }
                    }
                    e
                }
                Action::DeleteTurn(t) => vec![
                    self.controller
                        .delete_turn(id.as_deref().ok_or("Open a conversation")?, &t)
                        .map_err(str::to_string)?,
                ],
                Action::Capture => {
                    if !self.controller.visible
                        || self.active().is_none_or(|c| c.busy())
                        || id.as_ref().is_some_and(|s| self.deleting.contains(s))
                    {
                        return Err("Wait until this conversation is ready".into());
                    }
                    self.controller.capture().into_iter().collect()
                }
                Action::ChooseImages | Action::ClipboardImage => {
                    let ticket = self.attachment_ticket()?;
                    self.emit(
                        if matches!(a, Action::ChooseImages) {
                            ChatEffect::ChooseImages { ticket }
                        } else {
                            ChatEffect::ClipboardImage { ticket }
                        },
                        cx,
                    );
                    vec![]
                }
                Action::RemoveImage(n) => {
                    if let Some(s) = id {
                        self.controller.remove_attachment(&s, n);
                        self.blobs.retain_controller(&self.controller);
                    }
                    vec![]
                }
                Action::DirectoryMenu => {
                    self.directories_open = !self.directories_open;
                    if self.directories_open {
                        self.request(
                            ChatRequest::CompleteDirectory("/".into()),
                            runtime::Completion::Directories(self.init_generation),
                            cx,
                        );
                    }
                    vec![]
                }
                Action::Directory => {
                    let s = id.ok_or("Open a conversation")?;
                    self.request(
                        ChatRequest::ChooseDirectory,
                        runtime::Completion::Directory {
                            session: s,
                            mount: self.mount,
                        },
                        cx,
                    );
                    vec![]
                }
                Action::SetDirectory(p) => {
                    let s = id.ok_or("Open a conversation")?;
                    let mut selection = self.selection();
                    selection.cwd = Some(p.clone());
                    self.directories_open = false;
                    self.init_generation += 1;
                    let _ = self.controller.select(&s, selection);
                    self.request(
                        ChatRequest::RecordDirectory(p),
                        runtime::Completion::Quiet,
                        cx,
                    );
                    self.refresh_scope(cx);
                    vec![]
                }
                Action::Models => {
                    self.models_open = !self.models_open;
                    if self.models_open {
                        self.model_filter.focus_handle(cx).focus(w, cx);
                    }
                    vec![]
                }
                Action::Model(provider, model) => {
                    let s = id.ok_or("Open a conversation")?;
                    let mut selection = self.selection();
                    selection.provider_title =
                        Some(runtime::provider_title(&self.config, &provider));
                    selection.provider = Some(provider);
                    selection.model = Some(model);
                    self.models_open = false;
                    self.controller.select(&s, selection).into_iter().collect()
                }
                Action::Mode(mode) => {
                    if let Some(s) = id.as_deref() {
                        let _ = self.controller.set_mode(s, mode.clone());
                        self.save_mode(mode, cx);
                    }
                    vec![]
                }
                Action::Decide(d) => id
                    .as_deref()
                    .and_then(|s| self.controller.decide(s, d))
                    .into_iter()
                    .collect(),
                Action::ReviewProject => {
                    if let Some(cwd) = self.project.clone() {
                        self.emit(
                            ChatEffect::ReviewProject {
                                cwd,
                                nonce: identity("review"),
                            },
                            cx,
                        );
                    }
                    vec![]
                }
                Action::DismissProject => {
                    if let Some(p) = self.project.take() {
                        self.dismissed_projects.insert(p);
                    }
                    vec![]
                }
                Action::DismissError => {
                    self.error = None;
                    vec![]
                }
                Action::ChoosePrompt(n) => {
                    self.choose_prompt(n, w, cx);
                    vec![]
                }
                Action::ClearProfile => {
                    if self
                        .active()
                        .is_some_and(|c| c.turns.is_empty() && !c.busy())
                    {
                        let mut s = self.selection();
                        s.profile = None;
                        if let Some(id) = &id {
                            let _ = self.controller.select(id, s);
                        }
                    }
                    vec![]
                }
                Action::ResolvePrompt => {
                    self.resolve_prompt(cx);
                    vec![]
                }
                Action::CancelPrompt => {
                    self.prompt = None;
                    self.arguments.clear();
                    self.slash_dismissed = true;
                    self.prompt_generation += 1;
                    vec![]
                }
                Action::Expand(key) => {
                    if !self.expanded.remove(&key) {
                        if self.expanded.len() >= 128 {
                            self.expanded.clear();
                        }
                        self.expanded.insert(key);
                    }
                    vec![]
                }
                Action::Copy(text) => {
                    self.request(ChatRequest::Copy(text), runtime::Completion::Quiet, cx);
                    vec![]
                }
                Action::Open(url) => {
                    if let Some(url) = tezbar_ui::markdown::safe_link(&url) {
                        self.request(ChatRequest::OpenUrl(url), runtime::Completion::Quiet, cx);
                    }
                    vec![]
                }
            })
        })();
        match result {
            Ok(effects) => self.execute(effects, w, cx),
            Err(e) => self.error = Some(e),
        }
        self.prune();
        self.sync_input(w, cx);
        self.refresh_documents(cx);
        self.refresh_thumbnails(w, cx);
        if restore_focus
            && self.controller.visible
            && self.active().is_some_and(|c| c.approval.is_none())
        {
            self.input.focus_handle(cx).focus(w, cx);
        }
        self.publish(cx);
        cx.notify();
    }
    fn approval_has_focus(&self, w: &Window) -> bool {
        self.controller.visible
            && [&self.allow_focus, &self.deny_focus, &self.always_focus]
                .iter()
                .any(|f| f.is_focused(w))
    }
    /// Actual dispose is explicit; hiding never calls this. Parent retains entity until settled.
    pub fn pending(&self) -> bool {
        !self.jobs.is_empty()
            || self.owners.iter().any(|id| {
                self.controller
                    .conversation(id)
                    .is_some_and(|c| c.busy() || c.unsaved())
            })
    }
    pub(crate) fn quit_settlement(&self) -> Result<bool, String> {
        if !self.mutation_jobs.is_empty() {
            return Ok(false);
        }
        if self.owners.iter().any(|id| {
            self.controller
                .conversation(id)
                .is_some_and(|c| c.unsaved())
        }) {
            return Err("Chat response could not be saved. Retry quit to save it again.".into());
        }
        Ok(true)
    }
    pub(crate) fn retry_quit_persistence(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if !self.mutation_jobs.is_empty() {
            return;
        }
        let mut effects = Vec::new();
        for id in self.owners.clone() {
            effects.extend(self.controller.retry_persistence(&id));
        }
        self.execute(effects, w, cx);
        cx.notify();
    }
    pub(crate) fn prepare_quit(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.quit_preparing = true;
        self.retry_quit_persistence(w, cx);
    }
    pub(crate) fn revive_after_quit(&mut self, cx: &mut Context<Self>) {
        self.quit_preparing = false;
        if self.disposed {
            self.disposed = false;
            self.owner = self.store.owner();
        }
        cx.notify();
    }
    pub fn dispose(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.disposed {
            return;
        }
        self.disposed = true;
        self.jobs.retain(|id, _| self.mutation_jobs.contains(id));
        self.mount += 1;
        self.init_generation += 1;
        self.read_generation += 1;
        self.prompt_generation += 1;
        self.attachment_tickets.clear();
        self.pending_capture = None;
        self.document_images.clear();
        let e = self.controller.unmount();
        self.execute(e, w, cx);
        self.cancel_decode_waiters();
        self.images.release(w, cx);
        self.documents.clear();
        self.frame = None;
        self.store.retire(self.owner);
    }
}
