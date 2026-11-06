//! Native utility surfaces; models own transitions, this owner executes each emitted intent once.
mod request;
pub use request::UtilityRequest;
mod background_tasks;
mod clipboard;
mod emoji;
mod indexing;
mod notes;
mod permissions;
mod release_notes;
mod snippets;
mod system_monitor;
#[cfg(test)]
mod tests;
mod vault;
mod view;
use super::command_bar::navigation::{Navigation, Payload};
use crate::events::{CoreEvent, CoreEvents, EventSubscription};
use gpui::{prelude::*, *};
use request::{decode, success};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tezbar_ui::{
    controls,
    rich_text::RichText,
    text_input::{Committed, FieldOptions, TextInput},
    tokens,
};
use tezbar_ui_model::{
    notes as note_model, router::Surface, ui_state::UiState, utilities as model,
};
pub type Reply = Result<Value, String>;
pub trait UtilityTransport: Send + Sync + 'static {
    fn send(
        &self,
        request: UtilityRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Reply> + Send>>;
    /// Exact-tag HTTPS GET, at most 1 MiB, 10 seconds, cancelled when this future is dropped.
    fn release(
        &self,
        url: String,
        accept: &'static str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send>>;
}
/// Parent owns actual window/host/UI-state effects. Secrets must never be logged.
pub enum UtilityEffect {
    Navigate(Surface, Payload),
    Back,
    UpdateAction(super::command_bar::navigation::UpdateAction),
    OpenExtension {
        extension_id: String,
        command_name: String,
    },
    Hide,
    HideThenPaste(String),
    RecentEmoji(Vec<String>),
    ReleaseRead(String),
    SaveQr(Vec<u8>),
    OpenSettings(String),
    Reveal(std::path::PathBuf),
}
impl EventEmitter<UtilityEffect> for Utilities {}
#[derive(Clone)]
pub enum Action {
    Refresh,
    New,
    Select(usize),
    Activate,
    Copy,
    Delete,
    Confirm(bool),
    Pin,
    Clear,
    Filter(model::ClipboardFilter),
    Convert(tezbar_core::instant::transform::ClipboardAction),
    CopyResult,
    CopyQr,
    SaveQr,
    VaultDraft,
    Reveal,
    Edit,
    Save,
    Cancel,
    EmojiCategory(Option<String>),
    MonitorTab(String),
    AutoRefresh(bool),
    MoreSources,
    IndexStart,
    IndexPause,
    IndexResume,
    Settings,
    Permission(model::PermissionId),
    PermissionSettings(String),
    Lock,
    Unlock,
    RevealSecret,
    VaultDraftKind(tezbar_ui_model::vault::Kind),
    VaultIcon(String),
    ChangeIcon,
    DefaultIcon,
    ToggleTasks,
    VaultKind(Option<tezbar_ui_model::vault::Kind>),
    OpenTask(usize),
    OpenTaskId(String),
    RetryNote,
    ReleaseVersion(String),
}
#[derive(Clone)]
enum Completion {
    Clipboard(model::ClipboardEffect),
    Snippet(model::CrudEffect),
    Note(note_model::Effect),
    Monitor(u64),
    Sources(u64),
    Knowledge(u64),
    Permissions(u64),
    Permission(u64),
    Vault(vault::Completion),
    Background(u64),
    Paste(
        WeakEntity<TextInput>,
        u64,
        tezbar_ui::text_input::ByteSelection,
    ),
    NotePaste(Option<u64>, tezbar_ui::text_input::ByteSelection),
    Simple,
}
struct Job {
    _task: Task<()>,
    durable: bool,
}
pub struct Utilities {
    pub surface: Surface,
    pub visible: bool,
    pub clipboard: model::Clipboard,
    pub snippets: model::Crud,
    pub notes: note_model::Notes,
    pub emoji: model::EmojiPicker,
    pub indexing: model::Indexing,
    pub permissions: model::Permissions,
    pub monitor: model::UtilityPoll<Value>,
    pub vault: vault::VaultScreen,
    pub background: background_tasks::Background,
    pub release: release_notes::Release,
    pub search: Entity<TextInput>,
    pub editor: Entity<RichText>,
    preview: Entity<tezbar_ui::markdown::MarkdownView>,
    preview_revision: u64,
    fields: Vec<Entity<TextInput>>,
    icon_filter: Option<Entity<TextInput>>,
    icon_subscriptions: Vec<Subscription>,
    icon_focus: FocusHandle,
    icon_cell_focuses: Vec<FocusHandle>,
    pointer: Option<Point<Pixels>>,
    subscriptions: Vec<Subscription>,
    field_subscriptions: Vec<Subscription>,
    window: AnyWindowHandle,
    images: tezbar_ui::image_cache::ImageCache,
    preview_image: Option<Arc<RenderImage>>,
    qr_image: Option<Arc<RenderImage>>,
    events: Option<EventSubscription>,
    transport: Arc<dyn UtilityTransport>,
    presentation: SharedFormatter,
    jobs: BTreeMap<u64, Job>,
    completion: Arc<tokio::sync::Notify>,
    next_job: u64,
    epoch: u64,
    read_generation: u64,
    nonce: Option<u64>,
    pending_vault_seed: Option<(String, String)>,
    timer: Option<Task<()>>,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    confirmation: Option<(Action, Vec<String>)>,
    pub message: Option<String>,
    pub error: Option<String>,
    note_labels: BTreeMap<u64, (u64, String, String)>,
    tasks_open: bool,
    monitor_tab: String,
    requested_note: Option<u64>,
    note_creates: BTreeMap<u64, (u64, Option<u64>)>,
}
impl Utilities {
    pub fn new(
        transport: Arc<dyn UtilityTransport>,
        state: &UiState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| {
            let mut v = TextInput::new(cx);
            v.configure(FieldOptions {
                name: "Search utilities".into(),
                placeholder: "Search".into(),
                ..Default::default()
            });
            v
        });
        let editor = cx.new(|cx| RichText::new(window, cx));
        let preview = cx.new(|_| tezbar_ui::markdown::MarkdownView::default());
        let preview_sub = Self::markdown_subscription(&preview, cx);
        let release_owner = cx.on_release(|this, cx| {
            this.images.release_all_windows(cx);
            this.jobs.clear();
            this.timer = None;
            this.events = None;
        });
        let sub = cx.subscribe(&search, |this, _, e: &Committed, cx| {
            this.search_changed(e.0.clone(), cx)
        });
        let edit_sub = cx.subscribe(&editor, |this, _, e: &tezbar_ui::rich_text::Edit, cx| {
            this.edit_note(e.clone(), cx)
        });
        let rich_clip = cx.subscribe(
            &editor,
            |this, _, e: &tezbar_ui::rich_text::Clipboard, cx| match e {
                tezbar_ui::rich_text::Clipboard::Read { owner } => {
                    let selection = this.editor.read(cx).input.read(cx).selection_bytes();
                    this.request(
                        UtilityRequest::ReadText,
                        Completion::NotePaste(*owner, selection),
                        false,
                        cx,
                    );
                }
                tezbar_ui::rich_text::Clipboard::Write(s) => this.request(
                    UtilityRequest::WriteText(s.clone()),
                    Completion::Simple,
                    false,
                    cx,
                ),
            },
        );
        let search_clip = Self::clipboard_subscription(&search, cx);
        Self {
            surface: Surface::Clipboard,
            visible: false,
            clipboard: Default::default(),
            snippets: model::Crud::new(model::CrudKind::Snippet),
            notes: Default::default(),
            emoji: model::EmojiPicker {
                recent: state.recent_emojis.clone(),
                ..Default::default()
            },
            indexing: Default::default(),
            permissions: Default::default(),
            monitor: model::UtilityPoll::new(model::PollKind::SystemMonitor),
            vault: Default::default(),
            background: Default::default(),
            release: Default::default(),
            search,
            editor,
            preview,
            preview_revision: 0,
            fields: vec![],
            icon_filter: None,
            icon_subscriptions: vec![],
            icon_focus: cx.focus_handle(),
            icon_cell_focuses: tezbar_ui_model::vault::ICONS
                .iter()
                .map(|_| cx.focus_handle())
                .collect(),
            pointer: None,
            subscriptions: vec![
                sub,
                edit_sub,
                rich_clip,
                search_clip,
                preview_sub,
                release_owner,
            ],
            field_subscriptions: vec![],
            window: window.window_handle(),
            images: Default::default(),
            preview_image: None,
            qr_image: None,
            events: None,
            transport,
            presentation: SharedFormatter(Arc::new(
                tezbar_core::instant::transform::EnglishFormatter,
            )),
            jobs: BTreeMap::new(),
            completion: Arc::new(tokio::sync::Notify::new()),
            next_job: 0,
            epoch: 0,
            read_generation: 0,
            nonce: None,
            pending_vault_seed: None,
            timer: None,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            confirmation: None,
            message: None,
            error: None,
            note_labels: Default::default(),
            tasks_open: false,
            monitor_tab: "overview".into(),
            requested_note: None,
            note_creates: BTreeMap::new(),
        }
    }
    pub fn attach_events(&mut self, events: &CoreEvents, cx: &mut Context<Self>) {
        self.events = Some(events.subscribe(
            &cx.entity(),
            vec![
                "clipboard:changed".into(),
                "knowledge:status".into(),
                "background-tasks:changed".into(),
                "update-status".into(),
            ],
            |owner, event, cx| owner.update(cx, |this, cx| this.core_event(event, cx)),
            cx,
        ));
    }
    pub fn navigate(
        &mut self,
        navigation: Navigation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.nonce == Some(navigation.nonce) {
            return;
        }
        self.nonce = Some(navigation.nonce);
        self.set_visible(false, window, cx);
        self.surface = navigation.surface;
        self.epoch += 1;
        self.confirmation = None;
        self.fields.clear();
        self.field_subscriptions.clear();
        self.message = None;
        self.error = None;
        self.search.update(cx, |s, cx| s.clear_silently(window, cx));
        self.requested_note = match &navigation.payload {
            Payload::Note { created_at } => Some(*created_at),
            _ => None,
        };
        if let Payload::Vault { item_id } = &navigation.payload {
            self.vault.requested = Some(item_id.clone());
        }
        if self.surface == Surface::Vault {
            if let Some((id, text)) = self.pending_vault_seed.take() {
                self.vault.model.paste_draft(text);
                if let Some(d) = &mut self.vault.model.draft {
                    d.clipboard_entry_id = Some(id);
                }
            }
        }
        self.set_visible(true, window, cx);
        if let Payload::New(_) = navigation.payload {
            self.act(Action::New, window, cx);
        }
        if let Some(first) = self.fields.first() {
            first.focus_handle(cx).focus(window, cx);
        } else {
            window.focus(&self.search.focus_handle(cx), cx);
        }
    }
    pub fn set_visible(&mut self, visible: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        self.epoch += 1;
        self.timer = None;
        if visible {
            self.monitor.show(self.now());
            self.refresh(cx);
            self.refresh_tasks(cx);
            self.schedule(cx);
        } else {
            self.monitor.hide();
            self.indexing
                .search(self.indexing.query.clone(), self.now());
            self.jobs.retain(|_, j| j.durable);
            self.background.hidden();
            self.release.hide();
            self.vault.conceal();
            self.fields.clear();
            self.field_subscriptions.clear();
            self.images.release(window, cx);
            self.preview_image = None;
            self.qr_image = None;
            self.editor
                .update(cx, |e, cx| e.cancel_composition(window, cx));
        }
        if !visible
            && self.notes.save_states.values().any(|s| {
                matches!(
                    s,
                    note_model::SaveState::Dirty | note_model::SaveState::Saving
                )
            })
        {
            self.schedule(cx);
        }
        cx.notify();
    }
    pub fn set_presentation(
        &mut self,
        formatter: Arc<dyn tezbar_core::instant::transform::PresentationFormatter + Send + Sync>,
    ) {
        self.presentation = SharedFormatter(formatter);
    }
    fn now(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
    fn schedule(&mut self, cx: &mut Context<Self>) {
        if self.timer.is_some() {
            return;
        }
        self.timer = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
                let keep = this
                    .update(cx, |this, cx| {
                        this.tick(cx);
                        this.visible
                            || this.notes.save_states.values().any(|s| {
                                matches!(
                                    s,
                                    note_model::SaveState::Dirty | note_model::SaveState::Saving
                                )
                            })
                    })
                    .unwrap_or(false);
                if !keep {
                    let _ = this.update(cx, |this, _| this.timer = None);
                    break;
                }
            }
        }));
    }
    fn tick(&mut self, cx: &mut Context<Self>) {
        let now = self.now();
        for e in self.notes.tick(now) {
            self.note_effect(e, cx);
        }
        if !self.visible {
            return;
        }
        if self.surface == Surface::SystemMonitor {
            if let Some(e) = self.monitor.tick(now) {
                self.request(
                    UtilityRequest::SystemStats,
                    Completion::Monitor(e.generation),
                    false,
                    cx,
                );
            }
        }
        if self.background.due(now) {
            self.refresh_tasks(cx);
        }
        if self.surface == Surface::Indexing {
            if let Some(e) = self.indexing.tick(now) {
                self.index_effect(e, cx);
            }
        }
        cx.notify();
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.visible {
            return;
        }
        match self.surface {
            Surface::Clipboard => {
                let e = self.clipboard.load();
                self.clipboard_effect(e, cx);
            }
            Surface::Snippets => {
                let e = self.snippets.load();
                self.snippet_effect(e, cx);
            }
            Surface::Notes => {
                let e = self.notes.load();
                self.note_effect(e, cx);
            }
            Surface::Vault => self.vault_refresh(cx),
            Surface::SystemMonitor => {
                self.monitor.refresh(self.now());
                self.tick(cx);
            }
            Surface::Indexing => {
                if let Some(e) = self.indexing.load(false) {
                    self.index_effect(e, cx);
                }
                self.read_generation += 1;
                self.request(
                    UtilityRequest::KnowledgeSnapshot,
                    Completion::Knowledge(self.read_generation),
                    false,
                    cx,
                );
            }
            Surface::Permissions => {
                self.read_generation += 1;
                self.request(
                    UtilityRequest::Permissions,
                    Completion::Permissions(self.read_generation),
                    false,
                    cx,
                );
            }
            Surface::ReleaseNotes => self.open_release(None, cx),
            _ => {}
        }
    }
    fn request(
        &mut self,
        request: UtilityRequest,
        completion: Completion,
        durable: bool,
        cx: &mut Context<Self>,
    ) {
        if self.jobs.len() >= 16 {
            self.complete(
                completion,
                Err("Too many pending utility requests. Retry when they finish.".into()),
                cx,
            );
            return;
        }
        self.next_job += 1;
        let id = self.next_job;
        let epoch = self.epoch;
        let transport = self.transport.clone();
        let task = cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move { transport.send(request).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| {
                this.jobs.remove(&id);
                if durable || this.epoch == epoch {
                    this.complete(completion, result, cx);
                    cx.notify();
                }
                this.completion.notify_waiters();
            });
        });
        self.jobs.insert(
            id,
            Job {
                _task: task,
                durable,
            },
        );
    }
    fn complete(&mut self, c: Completion, r: Reply, cx: &mut Context<Self>) {
        match c {
            Completion::Clipboard(e) => self.clipboard_complete(e, r, cx),
            Completion::Snippet(e) => self.snippet_complete(e, r, cx),
            Completion::Note(e) => self.note_complete(e, r, cx),
            Completion::Monitor(g) => {
                self.monitor.complete(g, r, self.now());
            }
            Completion::Sources(g) => {
                self.indexing.complete(g, r.and_then(decode));
            }
            Completion::Knowledge(g) => {
                if g == self.read_generation {
                    match r {
                        Ok(v) => self.knowledge_changed(v),
                        Err(e) => self.error = Some(e),
                    }
                }
            }
            Completion::Permissions(g) => {
                if g == self.read_generation {
                    match r.and_then(decode) {
                        Ok(v) => {
                            self.permissions.snapshot = Some(v);
                            self.error = None;
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
            }
            Completion::Permission(g) => {
                for e in self.permissions.complete(g, r.and_then(decode)) {
                    if e == model::PermissionEffect::Refresh {
                        self.refresh(cx);
                    }
                }
            }
            Completion::Vault(c) => self.vault_complete(c, r, cx),
            Completion::Background(g) => self.background.complete(g, r),
            Completion::Paste(field, epoch, selection) => {
                if epoch == self.epoch {
                    match r.and_then(decode::<String>) {
                        Ok(text) => {
                            let window = self.window;
                            cx.defer(move |cx| {
                                let _ = window.update(cx, |_, w, cx| {
                                    let _ = field.update(cx, |f, cx| {
                                        if f.selection_bytes() == selection {
                                            f.paste_text(&text, w, cx);
                                        }
                                    });
                                });
                            });
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
            }
            Completion::NotePaste(owner, selection) => {
                if owner == self.notes.selected {
                    match r.and_then(decode::<String>) {
                        Ok(text) => {
                            let editor = self.editor.downgrade();
                            let window = self.window;
                            cx.defer(move |cx| {
                                let _ = window.update(cx, |_, w, cx| {
                                    let _ = editor.update(cx, |e, cx| {
                                        if e.input.read(cx).selection_bytes() == selection {
                                            e.paste_text(owner, &text, w, cx);
                                        }
                                    });
                                });
                            });
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
            }
            Completion::Simple => match r.and_then(success) {
                Ok(()) => self.message = Some("Done".into()),
                Err(e) => self.error = Some(e),
            },
        }
        self.sync_selection_preview(cx);
    }
    fn search_changed(&mut self, q: String, cx: &mut Context<Self>) {
        match self.surface {
            Surface::Clipboard => {
                if let Some(e) = self.clipboard.set_query(q) {
                    self.clipboard_effect(e, cx);
                }
            }
            Surface::Snippets => {
                self.snippets.query = q;
                self.snippets.selected = 0;
            }
            Surface::Notes => self.notes.query = q,
            Surface::Emoji => {
                self.emoji.query = q;
                self.emoji.selected = 0;
            }
            Surface::Vault => self.vault.query = q,
            Surface::Indexing => self.indexing.search(q, self.now()),
            Surface::SystemMonitor => self.monitor.query = q,
            _ => {}
        }
        cx.notify();
    }
    pub fn core_event(&mut self, e: &CoreEvent, cx: &mut Context<Self>) {
        match e {
            CoreEvent::Other { channel, payload } if channel == "knowledge:status" => {
                self.knowledge_changed(payload.clone())
            }
            CoreEvent::Other { channel, payload } if channel == "update-status" => {
                self.background.update = Some(payload.clone())
            }
            _ => {}
        }
        if self.visible {
            match e.channel() {
                "clipboard:changed" if self.surface == Surface::Clipboard => self.refresh(cx),
                "background-tasks:changed" => self.refresh_tasks(cx),
                _ => {}
            }
        }
        cx.notify();
    }
    /// Flush all dirty note revisions, then await durable effects. Parent retains this entity until resolved.
    pub fn drain(&mut self, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        // One explicit quit attempt retries each failed owner once. The wait below never replays it.
        let selected = self.notes.selected;
        let failed = self
            .notes
            .save_states
            .iter()
            .filter_map(|(id, state)| {
                matches!(state, note_model::SaveState::Error(_)).then_some(*id)
            })
            .collect::<Vec<_>>();
        for id in failed {
            self.notes.selected = Some(id);
            self.notes.retry(self.now());
        }
        self.notes.selected = selected;
        if self.snippets.error.is_some() && self.snippets.draft.is_some() {
            if let Ok(effect) = self.snippets.save() {
                self.snippet_effect(effect, cx);
            }
        }
        for e in self.notes.tick(u64::MAX) {
            self.note_effect(e, cx);
        }
        let completion = self.completion.clone();
        cx.spawn(async move |this, cx| {
            loop {
                let notified = completion.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                let state = this
                    .update(cx, |this, cx| {
                        if let Some(e) = this.notes.save_states.values().find_map(|s| {
                            if let note_model::SaveState::Error(e) = s {
                                Some(e.clone())
                            } else {
                                None
                            }
                        }) {
                            return Some(Err(e));
                        }
                        if this.snippets.draft.is_some() {
                            if let Some(error) = &this.snippets.error {
                                return Some(Err(error.clone()));
                            }
                        }
                        for e in this.notes.tick(u64::MAX) {
                            this.note_effect(e, cx);
                        }
                        if this.jobs.values().any(|j| j.durable) {
                            None
                        } else {
                            Some(Ok(()))
                        }
                    })
                    .unwrap_or(Some(Err("The utility owner closed before saving.".into())));
                if let Some(result) = state {
                    return result;
                }
                notified.await;
            }
        })
    }
}
impl Focusable for Utilities {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
#[derive(Clone)]
struct SharedFormatter(
    Arc<dyn tezbar_core::instant::transform::PresentationFormatter + Send + Sync>,
);
impl tezbar_core::instant::transform::PresentationFormatter for SharedFormatter {
    fn local_timestamp(&self, ms: i64) -> String {
        self.0.local_timestamp(ms)
    }
    fn relative(&self, time: &tezbar_core::instant::transform::RelativeTime) -> String {
        self.0.relative(time)
    }
    fn count(&self, n: usize) -> String {
        self.0.count(n)
    }
}
impl Utilities {
    fn markdown_subscription(
        view: &Entity<tezbar_ui::markdown::MarkdownView>,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.subscribe(
            view,
            |this, _, effect: &tezbar_ui::markdown::DocumentEffect, cx| {
                use tezbar_ui::markdown::DocumentEffect;
                match effect {
                    DocumentEffect::Copy(text) => this.request(
                        UtilityRequest::WriteText(text.clone()),
                        Completion::Simple,
                        false,
                        cx,
                    ),
                    DocumentEffect::OpenLink(url) => {
                        if tezbar_ui::markdown::safe_link(url).is_some() {
                            this.request(
                                UtilityRequest::Open(url.clone()),
                                Completion::Simple,
                                false,
                                cx,
                            );
                        }
                    }
                    DocumentEffect::Image(_) => {}
                }
            },
        )
    }
    fn sync_selection_preview(&mut self, cx: &mut Context<Self>) {
        let text = match self.surface {
            Surface::Clipboard => self
                .clipboard
                .preview_text()
                .unwrap_or_else(|| "Select an item to preview.".into()),
            Surface::Snippets => self
                .snippets
                .visible()
                .get(self.snippets.selected)
                .map(|s| s.preview.clone())
                .unwrap_or_default(),
            _ => return,
        };
        let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
        let fence = "`".repeat((longest + 1).max(3));
        let markdown = format!("{fence}text\n{text}\n{fence}");
        self.preview_revision += 1;
        self.preview.update(cx, |view, cx| {
            view.set_content(self.preview_revision, &markdown, true, cx)
        });
    }
}
