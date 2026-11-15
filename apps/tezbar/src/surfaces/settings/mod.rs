//! Retained native Settings. Effects start only from activation, input or subscribed events.
mod barrier;
mod contributions;
pub use barrier::EffectBarrier;
mod dialogs;
mod panels;
mod request;
mod store;
#[cfg(test)]
pub(crate) mod tests;
pub(crate) mod view;
use super::command_bar::navigation::{Navigation, Payload, UpdateAction};
use gpui::{prelude::*, *};
pub use request::{AccountAction, SettingsRequest};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{Duration, Instant},
};
pub use store::SettingsStore;
use tezbar_ui::text_input::{Committed, FieldOptions, TextInput};
use tezbar_ui_model::{
    knowledge_settings as knowledge,
    settings::{
        self, DeviceEffect, DevicePhase, DeviceSignIn, Edit, ProviderDraft,
        contributions::{Action as ContributionAction, Contributions},
    },
    ui_state::{Theme, UiState},
};
pub type Reply = Result<Value, String>;
pub type SettingsFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Reply> + Send>>;
pub trait SettingsTransport: Send + Sync + 'static {
    fn send(&self, request: SettingsRequest) -> SettingsFuture;
    fn durable_config(&self) -> SettingsFuture;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    General,
    Ai,
    Knowledge,
    Extensions,
    Permissions,
    Storage,
    Advanced,
}
impl Tab {
    pub const ALL: [Self; 7] = [
        Self::General,
        Self::Ai,
        Self::Knowledge,
        Self::Extensions,
        Self::Permissions,
        Self::Storage,
        Self::Advanced,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Ai => "ai",
            Self::Knowledge => "knowledge",
            Self::Extensions => "extensions",
            Self::Permissions => "permissions",
            Self::Storage => "storage",
            Self::Advanced => "advanced",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Ai => "AI",
            Self::Knowledge => "Knowledge",
            Self::Extensions => "Extensions",
            Self::Permissions => "Permissions",
            Self::Storage => "Storage",
            Self::Advanced => "Advanced",
        }
    }
    pub fn parse(s: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|t| t.id() == s)
            .unwrap_or(Self::General)
    }
}
/// The parent applies these through its retained atomic UI state and existing host owners.
#[derive(Clone)]
pub enum SettingsEffect {
    RetryPersistence,
    Theme(Theme),
    RememberTab(String),
    Currency(Option<String>),
    AutoUpdate(bool),
    UpdateChannel(String),
    Update(UpdateAction),
    ReleaseNotes,
    LauncherLayout {
        width: Option<f64>,
        height: Option<f64>,
        zoom: Option<f64>,
        reset: bool,
    },
    BrowseStore,
    Quit,
    Close,
    OpenPermissions,
    LegacyCacheCleanup,
}
impl EventEmitter<SettingsEffect> for SettingsContent {}
#[derive(Clone)]
enum FieldKind {
    Global(&'static str, f64),
    Provider(String, String, bool),
    Local,
}
struct Field {
    input: Entity<TextInput>,
    _subscription: Subscription,
    kind: FieldKind,
    dirty: bool,
    value: String,
}
#[derive(Clone)]
enum Completion {
    Config(u64),
    Storage,
    Clipboard,
    ClipboardMutation,
    DryRun,
    DryRunMutation,
    Installed,
    Schema,
    ExtensionSettings,
    ExtensionsMutation,
    Contribution(u64, ContributionAction),
    KnowledgeRead(u64),
    KnowledgeMutation(u64),
    Folder,
    Models(String, u64),
    OnboardingModels(String, u64),
    DeviceStart(u64),
    DevicePoll(u64, String, String),
    Account(u64, bool),
    Accounts(u64),
    Permissions,
    Permission(u64),
    Statuses,
    Pi,
    Preferences(String, Option<String>, u64),
    Simple,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsDialog {
    Import,
    Extension { id: String, section: String },
    Model(String),
    CustomProvider,
    Connection,
    Preferences(String, Option<String>),
    Permissions,
}
#[derive(Clone)]
pub enum Action {
    Dialog(Option<SettingsDialog>),
    Menu(String),
    SetField(String, String),
    ModelFlag(String, &'static str, bool),
    ModelProtocol(String, String),
    ModelCapability(String, String),
    ModelsVisible(bool),
    CreateNamedProvider,
    PreviewConnection,
    ImportFile,
    InstalledCommand(String, Option<String>),
    Tab(Tab),
    Refresh,
    Retry,
    Save,
    Theme(Theme),
    Currency,
    AutoUpdate(bool),
    Update(UpdateAction),
    UpdateChannel(String),
    ReleaseNotes,
    Layout,
    ResetLayout,
    Zoom(f64),
    RecordHotkey,
    RecordCommandHotkey(String),
    Toggle(&'static str, bool),
    Provider(String),
    SaveProvider,
    Discover,
    ConnectOnboarding,
    DiscoverOnboarding,
    FinishOnboardingConnect(String, String),
    EditOnboardingConnection,
    AddModel,
    RemoveModel(String),
    SaveModel(String),
    SelectModel(String),
    ActivateProvider,
    SaveTask(String),
    CreateProvider,
    RemoveProvider,
    DeleteProvider(String),
    SignIn,
    BeginSignIn {
        token: u64,
        client_id: Option<String>,
    },
    CancelSignIn,
    Account(AccountAction),
    AccountCallback,
    Knowledge(knowledge::Operation),
    KnowledgeControl(knowledge::Operation, String),
    ChooseFolder,
    ClipboardToggle(&'static str, bool),
    ClipboardSize,
    ClearImages,
    Vacuum,
    LegacyCleanup,
    DryRun(bool),
    Quit,
    EmitQuit,
    Permission(tezbar_ui_model::utilities::PermissionId),
    RequestPermission(tezbar_ui_model::utilities::PermissionId),
    OpenPermissionSettings(String),
    OpenPermissions,
    ShowPermissions,
    BrowseStore,
    InstalledSelect(String),
    CommandEnabled(String, bool),
    CommandHotkey(String),
    CommandAlias(String),
    Preferences(String, Option<String>),
    SavePreferences(String, Option<String>),
    PreferenceValue(String, Value),
    Uninstall(String),
    Contribution(ContributionIntent),
    Confirm(bool),
    Close,
}
#[derive(Clone)]
pub enum ContributionIntent {
    Preview,
    AddPreview,
    Select(String),
    Test(String),
    Enable(String, bool),
    Remove(String),
    Review(String),
    Accept(String),
    Operation(String, ContributionAction, Vec<Value>),
    SaveRow(String, String, usize, String),
    SaveCollection(String, String),
    SetTool(String, String, bool, Option<String>),
    ResetSecrets,
    SaveSecret(String, String, Option<String>),
    SaveSavedSecret(String, usize),
    ReloadSecretFields(String),
    Instructions,
    UseAgentsMd(bool),
    ReviewProject(String),
    TrustProject,
    DismissProject,
}
pub struct SettingsContent {
    pub embedded: bool,
    pub tab: Tab,
    pub store: SettingsStore,
    owner: u64,
    transport: Arc<dyn SettingsTransport>,
    pub config: Value,
    local_switches: BTreeMap<&'static str, Value>,
    pub provider: ProviderDraft,
    pub knowledge: knowledge::KnowledgeSettings,
    pub contributions: Contributions,
    pub permissions: tezbar_ui_model::utilities::Permissions,
    pub device: DeviceSignIn,
    state: UiState,
    fields: BTreeMap<String, Field>,
    jobs: BTreeMap<u64, Task<()>>,
    next_job: u64,
    reads: BTreeMap<&'static str, u64>,
    latest_reads: BTreeMap<&'static str, u64>,
    load: u64,
    pub error: Option<String>,
    parent_error: Option<String>,
    pub message: Option<String>,
    saving: bool,
    after_save: Vec<Action>,
    timer: Option<Task<()>>,
    visible: bool,
    activated: bool,
    config_stale: bool,
    contributions_stale: bool,
    started: Instant,
    pub window: AnyWindowHandle,
    tabs: Vec<FocusHandle>,
    focus: FocusHandle,
    knowledge_focus: Vec<FocusHandle>,
    onboarding_connected: Option<(String, String)>,
    onboarding_editing: bool,
    onboarding_checking: bool,
    storage: Value,
    clipboard: Value,
    dry_run: bool,
    installed: Value,
    schema: Value,
    extension_settings: Value,
    accounts: Value,
    account_status: String,
    account_generation: u64,
    provider_statuses: Value,
    pi: Value,
    selected_extension: Option<String>,
    selected_installed: Option<String>,
    selected_command: Option<String>,
    dialog: Option<SettingsDialog>,
    menu: Option<String>,
    dialog_restore: Option<FocusHandle>,
    dialog_focus: FocusHandle,
    secret_slots: BTreeMap<String, Vec<settings::contributions::SecretDescriptor>>,
    oauth_sessions: BTreeMap<(String, String), u64>,
    review: Option<Value>,
    confirmation: Option<Action>,
    confirm_restore: Option<FocusHandle>,
    confirm_focus: Vec<FocusHandle>,
    hotkey_recording: bool,
    command_hotkey_recording: Option<String>,
    device_identity: Option<(String, String)>,
    pub update_status: tezbar_update::AppUpdateStatus,
    pub update_channel: String,
    layout: Option<(String, tezbar_host::layout::Profile)>,
    preferences: BTreeMap<(String, Option<String>), (u64, Value)>,
    event_subscription: Option<crate::events::EventSubscription>,
    last_navigation: Option<u64>,
    last_project_review: Option<String>,
}
impl SettingsContent {
    pub fn new(
        store: SettingsStore,
        state: &UiState,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let owner = store.owner();
        let transport = store.transport();
        Self {
            embedded: false,
            tab: Tab::parse(&state.settings_tab),
            store,
            owner,
            transport,
            config: json!({}),
            local_switches: BTreeMap::new(),
            provider: ProviderDraft::default(),
            knowledge: Default::default(),
            contributions: Default::default(),
            permissions: Default::default(),
            device: Default::default(),
            state: state.clone(),
            fields: BTreeMap::new(),
            jobs: BTreeMap::new(),
            next_job: 0,
            reads: BTreeMap::new(),
            latest_reads: BTreeMap::new(),
            load: 0,
            error: None,
            parent_error: None,
            message: None,
            saving: false,
            after_save: vec![],
            timer: None,
            visible: true,
            activated: false,
            config_stale: false,
            contributions_stale: false,
            started: Instant::now(),
            window: w.window_handle(),
            tabs: (0..Tab::ALL.len()).map(|_| cx.focus_handle()).collect(),
            focus: cx.focus_handle(),
            knowledge_focus: vec![cx.focus_handle(), cx.focus_handle()],
            onboarding_connected: None,
            onboarding_editing: false,
            onboarding_checking: false,
            storage: Value::Null,
            clipboard: Value::Null,
            dry_run: false,
            installed: Value::Null,
            schema: Value::Null,
            extension_settings: Value::Null,
            accounts: Value::Null,
            account_status: String::new(),
            account_generation: 0,
            provider_statuses: Value::Null,
            pi: Value::Null,
            selected_extension: None,
            selected_installed: None,
            selected_command: None,
            dialog: None,
            menu: None,
            dialog_restore: None,
            dialog_focus: cx.focus_handle(),
            secret_slots: BTreeMap::new(),
            oauth_sessions: BTreeMap::new(),
            review: None,
            confirmation: None,
            confirm_restore: None,
            confirm_focus: vec![cx.focus_handle(), cx.focus_handle()],
            hotkey_recording: false,
            command_hotkey_recording: None,
            device_identity: None,
            update_status: tezbar_update::AppUpdateStatus::Idle,
            update_channel: String::new(),
            layout: None,
            preferences: BTreeMap::new(),
            event_subscription: None,
            last_navigation: None,
            last_project_review: None,
        }
    }
    pub fn attach_events(&mut self, events: &crate::events::CoreEvents, cx: &mut Context<Self>) {
        self.event_subscription = Some(events.subscribe(
            &cx.entity(),
            vec![
                "llm-config-changed".into(),
                "ai-extensions-changed".into(),
                "knowledge:status".into(),
            ],
            |view, event, cx| {
                view.update(cx, |v, cx| v.core_event(event, cx));
            },
            cx,
        ));
    }
    pub fn activate(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.activated {
            return;
        }
        self.activated = true;
        self.prepare_fields(w, cx);
        self.refresh(cx);
        self.refresh_timer(cx);
    }
    pub fn navigate(&mut self, intent: &Navigation, w: &mut Window, cx: &mut Context<Self>) {
        if self.last_navigation == Some(intent.nonce) {
            return;
        }
        self.last_navigation = Some(intent.nonce);
        if let Payload::Settings { tab } = &intent.payload {
            self.select_tab(Tab::parse(tab), w, cx)
        }
        if matches!(&intent.payload, Payload::ExtensionPreferences { .. }) {
            self.select_tab(Tab::Extensions, w, cx);
        }
        self.activate(w, cx);
        if let Payload::ExtensionPreferences { extension, command } = &intent.payload {
            self.action(
                Action::Preferences(extension.clone(), command.clone()),
                w,
                cx,
            );
        }
    }
    /// Explicit review handoff from the retained parent/M8 notice. Never starts a connection.
    pub fn review_project(
        &mut self,
        cwd: String,
        nonce: String,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.last_project_review.as_ref() == Some(&nonce) {
            return;
        }
        self.last_project_review = Some(nonce);
        self.set_tab(Tab::Ai, w, cx);
        self.activate(w, cx);
        self.action(
            Action::Contribution(ContributionIntent::ReviewProject(cwd)),
            w,
            cx,
        );
    }
    pub fn activate_tab(&mut self, tab: Tab, w: &mut Window, cx: &mut Context<Self>) {
        self.visible = true;
        if !self.activated {
            self.tab = tab;
            self.activate(w, cx)
        } else {
            self.select_tab(tab, w, cx);
            self.set_visible(true, cx)
        }
    }
    pub fn set_tab(&mut self, tab: Tab, w: &mut Window, cx: &mut Context<Self>) {
        self.select_tab(tab, w, cx)
    }
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        if visible {
            if self.config_stale {
                self.config_stale = false;
                self.read_config(cx)
            }
            if self.contributions_stale && self.tab == Tab::Ai {
                self.contributions_stale = false;
                let effect = self.contributions.load();
                self.contribution_effect(effect, cx)
            }
        }
        let effects = self.knowledge.visible(visible, self.now());
        self.knowledge_effects(effects, cx);
        self.refresh_timer(cx);
    }
    pub fn set_ui_state(&mut self, state: &UiState, cx: &mut Context<Self>) {
        self.state = state.clone();
        cx.notify()
    }
    pub fn parent_effect_finished(&mut self, result: Result<(), ()>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => {
                self.parent_error = None;
                self.message = Some("Saved".into())
            }
            Err(()) => {
                self.parent_error = Some("Could not save Settings. Retry saved changes.".into());
                self.message = None
            }
        }
        cx.notify();
    }
    pub fn set_update(
        &mut self,
        status: tezbar_update::AppUpdateStatus,
        channel: String,
        cx: &mut Context<Self>,
    ) {
        self.update_status = status;
        self.update_channel = channel;
        cx.notify()
    }
    pub fn set_layout(
        &mut self,
        name: String,
        profile: tezbar_host::layout::Profile,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.layout = Some((name, profile));
        for (key, v) in [
            ("layout.width", profile.width),
            ("layout.height", profile.height),
        ] {
            self.field(
                key,
                key,
                FieldKind::Local,
                &v.to_string(),
                false,
                false,
                w,
                cx,
            );
        }
        cx.notify()
    }
    fn now(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
    fn select_tab(&mut self, tab: Tab, w: &mut Window, cx: &mut Context<Self>) {
        if self.tab == tab {
            return;
        }
        self.tab = tab;
        self.hotkey_recording = false;
        self.command_hotkey_recording = None;
        self.menu = None;
        if !self.embedded {
            cx.emit(SettingsEffect::RememberTab(tab.id().into()));
        }
        let e = self.knowledge.activate(tab == Tab::Knowledge, self.now());
        self.knowledge_effects(e, cx);
        self.load_tab(cx);
        if self.embedded {
            w.focus(&self.focus, cx)
        } else {
            w.focus(
                &self.tabs[Tab::ALL.iter().position(|t| *t == tab).unwrap()],
                cx,
            );
        }
        cx.notify();
    }
    fn core_event(&mut self, event: &crate::events::CoreEvent, cx: &mut Context<Self>) {
        if let crate::events::CoreEvent::Other { channel, payload } = event {
            match channel.as_str() {
                "llm-config-changed" => {
                    if self.visible {
                        self.read_config(cx)
                    } else {
                        self.config_stale = true
                    }
                }
                "ai-extensions-changed" => {
                    if !self.visible {
                        self.config_stale = true;
                        self.contributions_stale = true;
                        return;
                    }
                    self.read_config(cx);
                    if self.tab == Tab::Ai {
                        let e = self.contributions.load();
                        self.contribution_effect(e, cx)
                    }
                }
                "knowledge:status" => {
                    self.knowledge.status_event(payload.clone());
                    cx.notify()
                }
                _ => {}
            }
        }
    }
    fn needs_timer(&self) -> bool {
        self.activated
            && self.visible
            && (self.tab == Tab::Knowledge
                || matches!(
                    self.device.phase,
                    DevicePhase::Waiting { .. } | DevicePhase::Polling
                )
                || (self.tab == Tab::Ai && self.account_status == "pending"))
    }
    fn refresh_timer(&mut self, cx: &mut Context<Self>) {
        if !self.needs_timer() {
            self.timer = None;
            return;
        }
        if self.timer.is_some() {
            return;
        }
        self.timer = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                if !this
                    .update(cx, |this, cx| {
                        if !this.needs_timer() {
                            this.timer = None;
                            return false;
                        }
                        this.tick(cx);
                        true
                    })
                    .unwrap_or(false)
                {
                    break;
                }
            }
        }));
    }
    fn tick(&mut self, cx: &mut Context<Self>) {
        let e = self.knowledge.tick(self.now());
        self.knowledge_effects(e, cx);
        if let Some(DeviceEffect::Poll { token }) = self.device.tick(self.now()) {
            if let Some((code, uri)) = self.device_identity.clone() {
                self.request(
                    SettingsRequest::DevicePoll,
                    Completion::DevicePoll(token, code, uri),
                    cx,
                )
            }
        }
        if self.visible
            && self.tab == Tab::Ai
            && self.account_status == "pending"
            && !self.jobs.is_empty()
        {
            return;
        }
        if self.visible
            && self.tab == Tab::Ai
            && self.account_status == "pending"
            && self.now() % 1000 < 250
        {
            self.request(
                SettingsRequest::Account(AccountAction::Status),
                Completion::Account(self.account_generation, false),
                cx,
            )
        }
    }
    fn read_config(&mut self, cx: &mut Context<Self>) {
        self.load += 1;
        self.request(SettingsRequest::Config, Completion::Config(self.load), cx)
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.read_config(cx);
        if self.tab == Tab::Knowledge {
            let mut effects = self.knowledge.activate(true, self.now());
            if effects.is_empty() {
                effects = self.knowledge.refresh(self.now(), false)
            }
            self.knowledge_effects(effects, cx)
        } else {
            self.load_tab(cx)
        }
    }
    fn load_tab(&mut self, cx: &mut Context<Self>) {
        match self.tab {
            Tab::Ai => {
                let e = self.contributions.load();
                self.contribution_effect(e, cx);
                self.request(SettingsRequest::ProviderStatuses, Completion::Statuses, cx);
                self.request(SettingsRequest::PiExtensions, Completion::Pi, cx);
                self.request(
                    SettingsRequest::Account(AccountAction::List),
                    Completion::Accounts(self.account_generation),
                    cx,
                )
            }
            Tab::Knowledge => {
                let e = self.knowledge.activate(true, self.now());
                self.knowledge_effects(e, cx)
            }
            Tab::Storage => {
                self.request(SettingsRequest::Storage, Completion::Storage, cx);
                self.request(SettingsRequest::ClipboardConfig, Completion::Clipboard, cx)
            }
            Tab::Permissions => {
                self.request(SettingsRequest::Permissions, Completion::Permissions, cx)
            }
            Tab::Advanced => self.request(SettingsRequest::DryRun, Completion::DryRun, cx),
            Tab::Extensions => {
                self.request(SettingsRequest::Installed, Completion::Installed, cx);
                self.request(SettingsRequest::ExtensionSchema, Completion::Schema, cx);
                self.request(
                    SettingsRequest::ExtensionSettings,
                    Completion::ExtensionSettings,
                    cx,
                )
            }
            _ => {}
        }
    }
    fn request(
        &mut self,
        request: SettingsRequest,
        completion: Completion,
        cx: &mut Context<Self>,
    ) {
        if self.jobs.len() >= if request.interrupt() { 16 } else { 15 } {
            self.error = Some("Settings actions are still finishing. Try again.".into());
            return;
        }
        self.next_job += 1;
        let id = self.next_job;
        let scope = completion.scope();
        if let Some(scope) = scope {
            self.latest_reads.insert(scope, id);
        }
        let transport = self.transport.clone();
        let window = self.window;
        let ticket = if request.durable() {
            match self.store.admit(request.clone()) {
                Ok(id) => Some(id),
                Err(e) => {
                    self.error = Some(e);
                    return;
                }
            }
        } else {
            None
        };
        let store = self.store.clone();
        let task = cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move {
                if let Some(id) = ticket {
                    store.operation_result(id).await
                } else {
                    transport.send(request).await
                }
            })
            .await
            .unwrap_or_else(|_| Err("Could not complete Settings action".into()));
            let _ = cx.update_window(window, |_, w, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.jobs.remove(&id);
                    if scope.is_none_or(|scope| this.latest_reads.get(scope) == Some(&id)) {
                        this.complete(completion, result, w, cx);
                    }
                    this.refresh_timer(cx);
                    cx.notify()
                });
            });
        });
        self.jobs.insert(id, task);
    }
    fn flush(&mut self, after: Option<Action>, cx: &mut Context<Self>) {
        if let Some(after) = after {
            if self.after_save.len() < 16 {
                self.after_save.push(after);
            } else {
                self.error = Some("Settings actions are still finishing. Retry.".into());
                return;
            }
        }
        if self.saving {
            return;
        }
        self.saving = true;
        self.message = Some("Saving…".into());
        let drain = self.store.drain_owner(self.owner);
        let window = self.window;
        self.next_job += 1;
        let id = self.next_job;
        let task = cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, drain)
                .await
                .unwrap_or_else(|_| Err(settings::CLOSE_ERROR.into()));
            let _ = cx.update_window(window, |_, w, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.jobs.remove(&id);
                    this.saving = false;
                    let rejected = result
                        .as_ref()
                        .err()
                        .is_some_and(|e| e == store::REJECTED_CHANGE);
                    let pending = this.store.owner_pending(this.owner);
                    match &result {
                        Ok(()) if !pending => {
                            this.error = None;
                            this.message = Some("Saved".into());
                        }
                        Ok(()) => {}
                        Err(error) => {
                            this.message = None;
                            if this.store.validation_error(this.owner).is_none()
                                || this.error.is_none()
                            {
                                this.error = Some(error.clone());
                            }
                            if !rejected {
                                this.after_save.clear();
                                this.device.saved(this.device_token(), false, None);
                            }
                        }
                    }
                    if result.is_ok() || rejected {
                        if let Some(config) = this.store.canonical() {
                            this.hydrate(config, w, cx);
                        }
                        if !pending {
                            let actions = std::mem::take(&mut this.after_save);
                            for action in actions {
                                this.action(action, w, cx);
                            }
                            if rejected {
                                this.error = Some(store::REJECTED_CHANGE.into());
                            }
                        }
                    }
                    if pending {
                        this.flush(None, cx);
                    }
                    cx.notify();
                });
            });
        });
        self.jobs.insert(id, task);
    }
    fn device_token(&self) -> u64 {
        self.reads.get("device").copied().unwrap_or(0)
    }
    fn queue(&mut self, edits: Vec<Edit>, cx: &mut Context<Self>) {
        self.store.enqueue(self.owner, edits);
        self.flush(None, cx)
    }
    fn complete(&mut self, c: Completion, r: Reply, w: &mut Window, cx: &mut Context<Self>) {
        let r = r.and_then(|v| {
            if v["ok"] == false {
                Err("Could not complete the action. Check the values and retry.".into())
            } else {
                Ok(v)
            }
        });
        match c {
            Completion::ExtensionsMutation => {
                if r.is_err() {
                    self.error = Some(
                        "Could not update the extension. Its saved settings are unchanged.".into(),
                    );
                }
                self.request(
                    SettingsRequest::ExtensionSettings,
                    Completion::ExtensionSettings,
                    cx,
                );
                self.request(SettingsRequest::ExtensionSchema, Completion::Schema, cx);
                self.request(SettingsRequest::Installed, Completion::Installed, cx);
            }
            Completion::ClipboardMutation => {
                if r.is_err() {
                    self.error = Some("Could not save clipboard settings. Retry.".into())
                }
                self.request(SettingsRequest::ClipboardConfig, Completion::Clipboard, cx)
            }
            Completion::DryRunMutation => {
                if r.is_err() {
                    self.error = Some("Could not save Dry-run preference. Retry.".into())
                }
                self.request(SettingsRequest::DryRun, Completion::DryRun, cx)
            }
            Completion::Permissions => match r {
                Ok(v) => match serde_json::from_value(v) {
                    Ok(v) => self.permissions.snapshot = Some(v),
                    Err(_) => self.error = Some("Could not read permission status.".into()),
                },
                Err(_) => self.error = Some("Could not read permission status. Retry.".into()),
            },
            Completion::Permission(t) => {
                let effects = self.permissions.complete(
                    t,
                    r.and_then(|v| {
                        serde_json::from_value(v)
                            .map_err(|_| "Could not request permission.".into())
                    }),
                );
                if !effects.is_empty() {
                    self.request(SettingsRequest::Permissions, Completion::Permissions, cx)
                }
            }
            Completion::Config(t) => {
                if self.load == t {
                    match r {
                        Ok(v) => self.hydrate(v, w, cx),
                        Err(_) => self.error = Some("Could not load Settings. Retry.".into()),
                    }
                }
            }
            Completion::Contribution(t, action) => {
                if action == ContributionAction::OAuthStart
                    && self.oauth_sessions.values().any(|token| *token == t)
                {
                    if let Ok(v) = &r {
                        if let Some(open) = v["url"].as_str().and_then(request::trusted_open) {
                            self.request(open, Completion::Simple, cx)
                        }
                    }
                }
                let next = self.contributions.reply(t, r.map_err(|_| ()));
                if let Some(e) = next {
                    self.contribution_effect(e, cx)
                }
                self.prepare_contributions(w, cx)
            }
            Completion::KnowledgeRead(t) => self.knowledge.read_result(t, decode(r), self.now()),
            Completion::KnowledgeMutation(t) => {
                let e = self.knowledge.mutation_result(t, decode(r), self.now());
                self.knowledge_effects(e, cx)
            }
            Completion::Folder => {
                if let Ok(Value::String(path)) = r {
                    if !path.is_empty() {
                        self.action(
                            Action::Knowledge(knowledge::Operation::AddRoot(path)),
                            w,
                            cx,
                        )
                    }
                }
            }
            Completion::OnboardingModels(p, t) => {
                self.onboarding_checking = false;
                if self.provider.provider == p {
                    match r {
                        Ok(v) if v.as_array().is_some_and(|v|!v.is_empty()) => {
                            let models = v.as_array().unwrap().iter().cloned().map(|v|if v.is_string(){json!({"id":v})}else{v}).collect::<Vec<_>>();
                            let model = models.iter().find(|m|m["id"] == self.provider.selected_model).or_else(||models.first()).map(|m|text(&m["id"])).unwrap_or_default();
                            if let Some(edit) = self.provider.discovered_models(t, models) {
                                self.store.discover(self.owner, edit);
                                self.provider.selected_model = model.clone();
                                self.queue(vec![Edit::Activate { provider:p.clone(), model:model.clone() }], cx);
                                self.flush(Some(Action::FinishOnboardingConnect(p, model)), cx);
                            }
                        }
                        _ => self.error = Some("Couldn’t verify the connection. You can finish this later in Settings → AI.".into()),
                    }
                }
            }
            Completion::Models(p, t) => match r {
                Ok(v) => {
                    let models = v
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|v| if v.is_string() { json!({"id":v}) } else { v })
                        .collect::<Vec<_>>();
                    // The core (like the React backend) answers an unreachable endpoint or a
                    // rejected key with an empty list. Say so instead of a silent no-op.
                    if models.is_empty() && self.provider.provider == p {
                        self.error = Some(if p == "ollama" {
                            "No models found. Start Ollama and check its base URL, then Refresh."
                        } else {
                            "No models found. Check the API key and base URL, then Refresh."
                        }
                        .into());
                    }
                    if self.provider.provider == p {
                        if let Some(e) = self.provider.discovered_models(t, models) {
                            self.store.discover(self.owner, e);
                            self.flush(None, cx)
                        }
                    }
                }
                Err(_) => {
                    self.error =
                        Some("Could not discover models. Your saved models are unchanged.".into())
                }
            },
            Completion::DeviceStart(t) => match r {
                Ok(v) => {
                    let code = text(&v["user_code"]);
                    let uri = text(&v["verification_uri"]);
                    if self
                        .device
                        .started(
                            t,
                            code.clone(),
                            uri.clone(),
                            v["interval"].as_u64().unwrap_or(5),
                            self.now(),
                        )
                        .is_ok()
                    {
                        self.device_identity = Some((code, uri.clone()));
                        self.request(SettingsRequest::OpenUrl(uri), Completion::Simple, cx)
                    }
                }
                Err(_)
                    if t == self.device_token() && self.device.phase == DevicePhase::Starting =>
                {
                    self.device.phase = DevicePhase::Failed;
                    self.error = Some("Could not start GitHub sign-in. Retry.".into())
                }
                Err(_) => {}
            },
            Completion::DevicePoll(t, code, uri) => match r {
                Ok(v) => {
                    self.device.polled(
                        t,
                        v["status"].as_str().unwrap_or("error"),
                        code,
                        uri,
                        self.now(),
                    );
                    if self.device.phase == DevicePhase::Complete {
                        if let Some(f) = self.fields.get_mut("provider.copilot.apiKey") {
                            f.dirty = false;
                        }
                        self.read_config(cx)
                    }
                }
                Err(_) => self.device.polled(t, "error", code, uri, self.now()),
            },
            Completion::Account(g, open) => {
                if g == self.account_generation {
                    match r {
                        Ok(v) => {
                            self.account_status = text(&v["status"]);
                            if open {
                                // The authorize URL comes from the core's own sign-in start
                                // (accounts.google.com); `open-external-url` admits GitHub only.
                                if let Some(open) =
                                    v["url"].as_str().and_then(request::trusted_open)
                                {
                                    self.account_status = "pending".into();
                                    self.request(open, Completion::Simple, cx)
                                }
                            }
                            if self.account_status != "pending" {
                                self.request(
                                    SettingsRequest::Account(AccountAction::List),
                                    Completion::Accounts(g),
                                    cx,
                                )
                            }
                        }
                        Err(e) => {
                            self.account_status = "error".into();
                            // Already filtered by the store (`user_error`).
                            self.error = Some(e.into())
                        }
                    }
                }
            }
            Completion::Accounts(g) => {
                if g == self.account_generation {
                    if let Ok(v) = r {
                        self.accounts = v
                    }
                }
            }
            Completion::Preferences(id, command, g) => {
                if self
                    .preferences
                    .get(&(id.clone(), command.clone()))
                    .is_some_and(|(t, _)| *t == g)
                {
                    if let Ok(v) = r {
                        self.prepare_preferences(&id, command.clone(), &v, w, cx);
                        self.preferences.insert((id, command), (g, v));
                    }
                }
            }
            other => match r {
                Err(e) => self.error = Some(e.into()),
                Ok(v) => match other {
                    Completion::Storage => self.storage = v,
                    Completion::Clipboard => {
                        self.clipboard = v;
                        self.field(
                            "clipboard.size",
                            "Maximum image megapixels",
                            FieldKind::Local,
                            &text(&self.clipboard["maxImageMegapixels"]),
                            false,
                            false,
                            w,
                            cx,
                        );
                    }
                    Completion::DryRun => self.dry_run = v.as_bool().unwrap_or(false),
                    Completion::Installed => {
                        self.installed = v;
                        self.prepare_extension_fields(w, cx);
                    }
                    Completion::Schema => {
                        self.schema = v;
                        self.prepare_extension_fields(w, cx);
                    }
                    Completion::ExtensionSettings => {
                        self.extension_settings = v;
                        self.prepare_extension_fields(w, cx);
                    }
                    Completion::Statuses => self.provider_statuses = v,
                    Completion::Pi => self.pi = v,
                    Completion::Simple => {
                        self.message = Some("Done".into());
                    }
                    _ => {}
                },
            },
        }
    }
    fn knowledge_effects(&mut self, effects: Vec<knowledge::Effect>, cx: &mut Context<Self>) {
        for e in effects {
            match e {
                knowledge::Effect::Read { token } => {
                    if self.knowledge.accepts_read(token) {
                        self.request(
                            SettingsRequest::KnowledgeSnapshot,
                            Completion::KnowledgeRead(token),
                            cx,
                        )
                    }
                }
                knowledge::Effect::Mutate { token, operation } => self.request(
                    SettingsRequest::Knowledge(operation),
                    Completion::KnowledgeMutation(token),
                    cx,
                ),
                knowledge::Effect::Focus { .. } => {}
            }
        }
    }
    fn field(
        &mut self,
        id: &str,
        label: &str,
        kind: FieldKind,
        value: &str,
        password: bool,
        multiline: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(f) = self.fields.get_mut(id) {
            if !f.dirty && f.value != value {
                f.value = value.into();
                f.input.update(cx, |v, cx| {
                    v.replace_silently(
                        value,
                        tezbar_ui::text_input::ByteSelection {
                            range: 0..0,
                            reversed: false,
                        },
                        w,
                        cx,
                    )
                });
            }
            return;
        }
        let input = cx.new(|cx| {
            let mut v = TextInput::new_native(cx);
            v.configure(FieldOptions {
                name: label.to_string().into(),
                // Filters have no visible label: their name is the hint.
                placeholder: if id.ends_with(".filter") {
                    label.to_string().into()
                } else {
                    "".into()
                },
                password,
                multiline,
                ..Default::default()
            });
            v.content = value.to_string().into();
            v
        });
        let key = id.to_string();
        let sub = cx.subscribe(&input, move |this, _, e: &Committed, cx| {
            if let Some(f) = this.fields.get_mut(&key) {
                f.dirty = true;
                f.value = e.0.clone();
            }
            this.field_changed(&key, cx);
        });
        self.fields.insert(
            id.into(),
            Field {
                input,
                _subscription: sub,
                kind,
                dirty: false,
                value: value.into(),
            },
        );
    }
    fn field_changed(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(f) = self.fields.get(key) else {
            return;
        };
        let value = f.value.clone();
        let kind = f.kind.clone();
        let edit = match kind {
            FieldKind::Global(name, scale) => {
                let value = if name == "headerTimeoutSeconds" && value.trim().is_empty() {
                    Value::Null
                } else if scale > 0. {
                    match value.parse::<f64>() {
                        Ok(v) if v.is_finite() && v >= 0. => json!((v * scale).round() as u64),
                        _ => {
                            self.error = Some("Enter a non-negative number.".into());
                            self.store.validation(
                                self.owner,
                                key,
                                Some("Correct the invalid numeric field before closing."),
                            );
                            cx.notify();
                            return;
                        }
                    }
                } else {
                    json!(value)
                };
                Some(Edit::Global {
                    field: name.into(),
                    value,
                })
            }
            FieldKind::Provider(p, name, is_json) => {
                if p == "copilot"
                    && name == "copilotGithubToken"
                    && !matches!(
                        self.device.phase,
                        DevicePhase::Idle
                            | DevicePhase::Failed
                            | DevicePhase::Cancelled
                            | DevicePhase::Complete
                    )
                {
                    self.error = Some("Cancel sign-in before changing the Copilot token.".into());
                    cx.notify();
                    return;
                }
                if name == "protocol"
                    && !value.is_empty()
                    && !matches!(
                        value.as_str(),
                        "openai-chat" | "openai-responses" | "anthropic" | "gemini" | "ollama"
                    )
                {
                    self.error = Some("Choose a supported provider protocol.".into());
                    self.store.validation(
                        self.owner,
                        key,
                        Some("Correct the provider protocol before closing."),
                    );
                    cx.notify();
                    return;
                }
                let v = if is_json {
                    match serde_json::from_str::<Value>(&value) {
                        Ok(v) if v.is_object() => v,
                        _ => {
                            self.error = Some("Enter valid JSON before saving.".into());
                            self.store.validation(
                                self.owner,
                                key,
                                Some("Correct invalid JSON before closing."),
                            );
                            cx.notify();
                            return;
                        }
                    }
                } else {
                    json!(value)
                };
                if self.provider.provider == p {
                    self.provider.edit(name, v).ok()
                } else {
                    None
                }
            }
            FieldKind::Local => None,
        };
        self.store.validation(self.owner, key, None);
        if let Some(e) = edit {
            self.store.enqueue(self.owner, vec![e]);
            self.flush(None, cx)
        }
        cx.notify();
    }
    fn value(&self, id: &str) -> String {
        self.fields
            .get(id)
            .map(|f| f.value.clone())
            .unwrap_or_default()
    }
    fn clear_account_callback(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(field) = self.fields.get_mut("account.callback") {
            field.value.clear();
            field.dirty = false;
            field
                .input
                .update(cx, |input, cx| input.clear_silently(w, cx));
        }
    }
    fn hydrate(&mut self, config: Value, w: &mut Window, cx: &mut Context<Self>) {
        self.local_switches
            .retain(|key, value| config[*key] != *value);
        self.config = config;
        for (key, value) in &self.local_switches {
            self.config[*key] = value.clone();
        }
        for (key, label, default) in [
            ("uiStateRetentionMs", "Last Screen, seconds", 60000),
            (
                "extensionRuntimeTimeoutMs",
                "Extension Return, minutes",
                300000,
            ),
            ("aiModeTimeoutMs", "AI Return, minutes", 300000),
            ("terminalModeTimeoutMs", "Terminal Return, minutes", 300000),
            ("memoryMaxItems", "Memory items per request", 3),
            ("headerTimeoutSeconds", "Header wait, seconds", 30),
        ] {
            let value = self.config.get(key).cloned().unwrap_or_else(|| {
                if key == "headerTimeoutSeconds" {
                    Value::Null
                } else {
                    json!(default)
                }
            });
            let scale = if key == "uiStateRetentionMs" {
                1000.
            } else if key.ends_with("TimeoutMs") {
                60000.
            } else {
                1.
            };
            let display = if value.is_null() && key == "headerTimeoutSeconds" {
                String::new()
            } else {
                text(&json!(value.as_f64().unwrap_or(default as f64) / scale))
            };
            self.field(
                key,
                label,
                FieldKind::Global(key, scale),
                &display,
                false,
                false,
                w,
                cx,
            );
        }
        let p = if self.provider.provider.is_empty() {
            self.config["provider"]
                .as_str()
                .unwrap_or("ollama")
                .to_string()
        } else {
            self.provider.provider.clone()
        };
        self.provider.load(p, &self.config);
        for task in ["chat", "search", "action"] {
            for (kind, map) in [
                ("provider", "taskProviderOverrides"),
                ("model", "taskModelOverrides"),
            ] {
                self.field(
                    &format!("task.{task}.{kind}"),
                    &format!("{task} {kind}"),
                    FieldKind::Local,
                    &text(&self.config[map][task]),
                    false,
                    false,
                    w,
                    cx,
                );
            }
        }
        self.provider_fields(w, cx);
    }
    fn provider_fields(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let p = self.provider.provider.clone();
        for model in self.provider.models.clone() {
            let id = text(&model["id"]);
            self.field(
                &format!("model.{p}.{id}"),
                &format!("Metadata for {id}"),
                FieldKind::Local,
                &text(&model),
                false,
                true,
                w,
                cx,
            );
        }
        for (field, label, password, is_json) in [
            ("apiKey", "API key", true, false),
            ("baseURL", "Base URL", false, false),
            ("protocol", "Protocol", false, false),
            ("headers", "Headers JSON", true, false),
            (
                "githubOAuthClientId",
                "GitHub OAuth client ID",
                false,
                false,
            ),
        ] {
            let wire_field = provider_field(&p, field);
            let definition = self.config["customProviders"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|row| row["id"] == p);
            let value = self
                .provider
                .field(wire_field)
                .cloned()
                .or_else(|| definition.and_then(|row| row.get(field)).cloned())
                .unwrap_or_else(|| {
                    if field == "baseURL" {
                        settings::provider_catalog()["DEFAULT_BASE_URL"][&p].clone()
                    } else if is_json {
                        json!({})
                    } else {
                        json!("")
                    }
                });
            self.field(
                &format!("provider.{p}.{field}"),
                label,
                FieldKind::Provider(p.clone(), wire_field.into(), is_json),
                &text(&value),
                password,
                is_json,
                w,
                cx,
            );
            if let Some(f) = self.fields.get(&format!("provider.{p}.{field}")) {
                if f.dirty {
                    let v = if is_json {
                        serde_json::from_str(&f.value).ok()
                    } else {
                        Some(json!(f.value))
                    };
                    if let Some(v) = v {
                        let _ = self.provider.edit(wire_field.into(), v);
                    }
                }
            }
        }
    }
    fn prepare_fields(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        for (id, label, multi, secret) in [
            ("connection.name", "Connection name", false, false),
            ("connection.transport", "Connection type", false, false),
            ("connection.command", "Command", false, false),
            ("connection.args", "Arguments, one per line", true, false),
            ("connection.url", "Web address", false, false),
            (
                "connection.cwd",
                "Working directory (optional)",
                false,
                false,
            ),
            ("connection.timeout", "Time limit, seconds", false, false),
            ("connection.env", "Environment variables (JSON)", true, true),
            ("provider.name", "Provider name (e.g. Model)", false, false),
            (
                "provider.url",
                "Base URL (e.g. http://127.0.0.1:8080/v1)",
                false,
                false,
            ),
            ("provider.protocol", "Protocol", false, false),
            ("model.new", "Model ID", false, false),
            ("model.metadata", "Model metadata JSON", true, false),
            (
                "provider.new",
                "Custom provider definition JSON",
                true,
                false,
            ),
            ("currency", "Default currency", false, false),
            ("account.callback", "Callback URL", false, true),
            ("import", "Paste mcpServers or manifest", true, true),
            ("instructions", "Custom instructions", true, false),
            ("secret.value", "New secret value", false, true),
            ("secret.key", "Secret name", false, false),
            (
                "extensions.filter",
                "Filter installed extensions",
                false,
                false,
            ),
            (
                "allow.confirm",
                "Type the extension name to allow all tools",
                false,
                false,
            ),
        ] {
            let value = if id == "currency" {
                self.state.currency.pinned.clone().unwrap_or_default()
            } else {
                String::new()
            };
            self.field(id, label, FieldKind::Local, &value, secret, multi, w, cx)
        }
    }
    fn confirm(&mut self, action: Action, w: &mut Window, cx: &mut Context<Self>) {
        self.confirm_restore = w.focused(cx);
        self.confirmation = Some(action);
        w.focus(&self.confirm_focus[1], cx);
    }
    fn durable_error(&self) -> Option<&str> {
        if self.store.durable_failures() > 0 {
            Some(settings::CLOSE_ERROR)
        } else {
            self.parent_error.as_deref()
        }
    }
    pub fn action(&mut self, a: Action, w: &mut Window, cx: &mut Context<Self>) {
        if self.confirmation.is_some() && !matches!(&a, Action::Confirm(_)) {
            return;
        }
        if self.store.validation_error(self.owner).is_none() {
            self.error = None;
        }
        if !matches!(&a, Action::Menu(_)) {
            self.menu = None;
        }
        match a {
            Action::ImportFile => self.import_configuration(cx),
            Action::PreviewConnection => {
                let name = self.value("connection.name");
                let transport = if self.value("connection.transport") == "http" {
                    "http"
                } else {
                    "stdio"
                };
                let mut connection = json!({"id":"connection","name":name,"transport":transport});
                if name.trim().is_empty() {
                    self.error = Some("Enter a connection name.".into());
                } else {
                    if transport == "http" {
                        connection["url"] = json!(self.value("connection.url"));
                    } else {
                        connection["command"] = json!(self.value("connection.command"));
                        connection["args"] = json!(
                            self.value("connection.args")
                                .lines()
                                .map(str::to_owned)
                                .collect::<Vec<_>>()
                        );
                        let cwd = self.value("connection.cwd");
                        if !cwd.trim().is_empty() {
                            connection["cwd"] = json!(cwd);
                        }
                        let timeout = self
                            .value("connection.timeout")
                            .parse::<u64>()
                            .unwrap_or(120);
                        connection["timeoutMs"] = json!(timeout.clamp(1, 1800) * 1000);
                        let env = self.value("connection.env");
                        if !env.trim().is_empty() {
                            match serde_json::from_str::<Value>(&env) {
                                Ok(v) if v.is_object() => connection["env"] = v,
                                _ => {
                                    self.error =
                                        Some("Environment variables must be a JSON object.".into());
                                    cx.notify();
                                    return;
                                }
                            }
                        }
                    }
                    let manifest = json!({"manifestVersion":1,"id":format!("user.native-{}",self.next_job),"name":name,"connections":[connection]});
                    let effect = self.contributions.import_preview(manifest.to_string());
                    self.contribution_effect(effect, cx);
                }
            }
            Action::ConnectOnboarding => {
                if !self.onboarding_checking && !self.saving {
                    self.flush(Some(Action::DiscoverOnboarding), cx);
                }
            }
            Action::FinishOnboardingConnect(provider, model) => {
                self.onboarding_connected = Some((provider, model));
                self.onboarding_editing = false;
                self.message = Some("Connected".into());
            }
            Action::EditOnboardingConnection => self.onboarding_editing = true,
            Action::DiscoverOnboarding => {
                let p = self.provider.provider.clone();
                let base_url = self
                    .provider
                    .field(provider_field(&p, "baseURL"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let key = self
                    .provider
                    .field(provider_field(&p, "apiKey"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                if p != "ollama" && key.as_ref().is_none_or(|key| key.trim().is_empty()) {
                    self.error = Some("Enter an API key to connect this provider.".into());
                } else {
                    self.onboarding_checking = true;
                    self.request(
                        SettingsRequest::Models {
                            provider: p.clone(),
                            base_url,
                            key,
                        },
                        Completion::OnboardingModels(p, self.provider.discovery_token()),
                        cx,
                    );
                }
            }
            Action::Dialog(dialog) => {
                self.menu = None;
                if matches!(
                    &dialog,
                    Some(SettingsDialog::Connection | SettingsDialog::Import)
                ) {
                    self.contributions.preview = None;
                }
                if dialog.is_none() {
                    for key in ["import", "connection.env"] {
                        if let Some(field) = self.fields.get_mut(key) {
                            field.value.clear();
                            field.dirty = false;
                            field
                                .input
                                .update(cx, |input, cx| input.clear_silently(w, cx));
                        }
                    }
                }
                if dialog.is_some() && self.dialog.is_none() {
                    self.dialog_restore = w.focused(cx);
                    w.focus(&self.dialog_focus, cx);
                } else if dialog.is_none() {
                    if let Some(focus) = self.dialog_restore.take() {
                        w.focus(&focus, cx);
                    }
                }
                if let Some(SettingsDialog::Preferences(id, command)) = &dialog {
                    self.action(Action::Preferences(id.clone(), command.clone()), w, cx);
                }
                self.dialog = dialog;
            }
            Action::Menu(key) => {
                self.menu = if self.menu.as_ref() == Some(&key) {
                    None
                } else {
                    Some(key)
                }
            }
            Action::SetField(key, value) => {
                if key.starts_with("connection.") || key == "import" {
                    self.contributions.preview = None;
                }
                self.action(Action::PreferenceValue(key.clone(), json!(value)), w, cx);
                self.field_changed(&key, cx);
                if key == "currency" {
                    self.action(Action::Currency, w, cx);
                }
                self.menu = None;
            }
            Action::ModelFlag(id, field, value) => {
                let mut models = self.provider.models.clone();
                if let Some(model) = models.iter_mut().find(|m| m["id"] == id) {
                    model[field] = json!(value);
                }
                self.provider.models = models.clone();
                self.queue(
                    vec![Edit::Models {
                        provider: self.provider.provider.clone(),
                        models,
                    }],
                    cx,
                );
            }
            Action::ModelProtocol(id, protocol) => {
                let mut models = self.provider.models.clone();
                if let Some(model) = models.iter_mut().find(|m| m["id"] == id) {
                    if protocol.is_empty() {
                        model.as_object_mut().map(|m| m.remove("protocol"));
                    } else {
                        model["protocol"] = json!(protocol);
                    }
                }
                self.provider.models = models.clone();
                self.queue(
                    vec![Edit::Models {
                        provider: self.provider.provider.clone(),
                        models,
                    }],
                    cx,
                );
            }
            Action::ModelCapability(id, capability) => {
                let mut models = self.provider.models.clone();
                if let Some(model) = models.iter_mut().find(|m| m["id"] == id) {
                    let mut caps = model["capabilities"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    if caps.iter().any(|c| c == &capability) {
                        caps.retain(|c| c != &capability);
                    } else {
                        caps.push(json!(capability));
                    }
                    model["capabilities"] = json!(caps);
                }
                self.provider.models = models.clone();
                self.queue(
                    vec![Edit::Models {
                        provider: self.provider.provider.clone(),
                        models,
                    }],
                    cx,
                );
            }
            Action::ModelsVisible(visible) => {
                let mut models = self.provider.models.clone();
                for model in &mut models {
                    model["hiddenFromPicker"] = json!(!visible);
                }
                self.provider.models = models.clone();
                self.queue(
                    vec![Edit::Models {
                        provider: self.provider.provider.clone(),
                        models,
                    }],
                    cx,
                );
            }
            Action::CreateNamedProvider => {
                let name = self.value("provider.name").trim().to_owned();
                let url = self.value("provider.url").trim().to_owned();
                if name.is_empty() || url.is_empty() {
                    self.error = Some("Enter a provider name and Base URL.".into());
                } else {
                    // React `createCustomProvider`: `{id,title,subtitle}` in the list, the
                    // endpoint in `providerConfigs[id]` — the only place the core reads it.
                    let mut slug = String::new();
                    for c in name.to_lowercase().chars() {
                        if c.is_ascii_alphanumeric() {
                            slug.push(c);
                        } else if !slug.ends_with('-') {
                            slug.push('-');
                        }
                    }
                    let slug = match slug.trim_matches('-') {
                        "" => "provider".to_owned(),
                        slug => slug.to_owned(),
                    };
                    let taken = |id: &str| {
                        self.config["customProviders"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|p| p["id"] == id)
                    };
                    let mut provider = format!("custom:{slug}");
                    let mut suffix = 2;
                    while taken(&provider) {
                        provider = format!("custom:{slug}-{suffix}");
                        suffix += 1;
                    }
                    let url = url.replacen("://0.0.0.0", "://127.0.0.1", 1);
                    let protocol = self.value("provider.protocol");
                    let field = |field: &str, value: &str| Edit::Field {
                        provider: provider.clone(),
                        field: field.into(),
                        value: json!(value),
                    };
                    let mut edits = vec![
                        Edit::CreateProvider {
                            provider: provider.clone(),
                            value: json!({"id":provider,"title":name,"subtitle":"Custom OpenAI-compatible endpoint"}),
                        },
                        field("baseURL", &url),
                        field("openaiCompatibleBaseURL", &url),
                    ];
                    edits.push(field(
                        "protocol",
                        if protocol.is_empty() { "openai-chat" } else { &protocol },
                    ));
                    self.queue(edits, cx);
                    self.dialog = None;
                    self.provider.load(provider, &self.config);
                    self.provider_fields(w, cx);
                }
            }
            Action::InstalledCommand(id, command) => {
                self.selected_installed = Some(id);
                self.selected_command = command;
            }
            Action::Permission(id) => self.flush(Some(Action::RequestPermission(id)), cx),
            Action::RequestPermission(id) => {
                if let Some(tezbar_ui_model::utilities::PermissionEffect::Request {
                    generation,
                    id,
                }) = self.permissions.request(id)
                {
                    self.request(
                        SettingsRequest::Permission(id),
                        Completion::Permission(generation),
                        cx,
                    )
                }
            }
            Action::OpenPermissionSettings(url) => {
                if url.starts_with("x-apple.systempreferences:") || url.starts_with("ms-settings:")
                {
                    if let Some(open) = request::trusted_open(&url) {
                        self.request(open, Completion::Simple, cx)
                    }
                }
            }
            Action::Tab(t) => self.select_tab(t, w, cx),
            Action::Refresh => self.refresh(cx),
            Action::Retry => {
                if self.store.validation_error(self.owner).is_some()
                    && self.store.durable_failures() == 0
                    && self.parent_error.is_none()
                {
                    cx.notify();
                    return;
                }
                cx.emit(SettingsEffect::RetryPersistence);
                self.store.retry();
                self.flush(Some(Action::Refresh), cx)
            }
            Action::Save | Action::SaveProvider => self.flush(None, cx),
            Action::Theme(theme) => {
                self.state.theme = theme;
                cx.emit(SettingsEffect::Theme(theme));
            }
            Action::Currency => {
                let c = self.value("currency").trim().to_uppercase();
                if c.is_empty() || tezbar_core::instant::currency::valid_code(&c) {
                    cx.emit(SettingsEffect::Currency((!c.is_empty()).then_some(c)))
                } else {
                    self.error = Some("Use a three-letter currency code.".into())
                }
            }
            Action::AutoUpdate(v) => {
                self.state.auto_update_enabled = v;
                cx.emit(SettingsEffect::AutoUpdate(v))
            }
            Action::Update(a) => cx.emit(SettingsEffect::Update(a)),
            Action::UpdateChannel(v) => cx.emit(SettingsEffect::UpdateChannel(v)),
            Action::ReleaseNotes => cx.emit(SettingsEffect::ReleaseNotes),
            Action::Layout => {
                let width = self
                    .value("layout.width")
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite() && *v > 0.);
                let height = self
                    .value("layout.height")
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite() && *v > 0.);
                if width.is_some() && height.is_some() {
                    cx.emit(SettingsEffect::LauncherLayout {
                        width,
                        height,
                        zoom: None,
                        reset: false,
                    })
                } else {
                    self.error = Some("Enter a positive width and height.".into())
                }
            }
            Action::ResetLayout => cx.emit(SettingsEffect::LauncherLayout {
                width: None,
                height: None,
                zoom: None,
                reset: true,
            }),
            Action::Zoom(v) => {
                let zoom = v.clamp(0.75, 2.);
                if let Some((_, profile)) = self.layout.as_mut() {
                    profile.zoom = zoom;
                }
                cx.emit(SettingsEffect::LauncherLayout {
                    width: None,
                    height: None,
                    zoom: Some(zoom),
                    reset: false,
                });
            }
            Action::RecordCommandHotkey(id) => {
                self.command_hotkey_recording = Some(id);
                w.focus(&self.focus, cx);
            }
            Action::RecordHotkey => {
                self.hotkey_recording = true;
                w.focus(&self.focus, cx)
            }
            Action::Toggle(field, value) => {
                self.local_switches.insert(field, json!(value));
                self.config[field] = json!(value);
                self.queue(
                    vec![Edit::Global {
                        field: field.into(),
                        value: json!(value),
                    }],
                    cx,
                )
            }
            Action::Provider(p) => {
                self.provider.load(p, &self.config);
                self.provider_fields(w, cx)
            }
            Action::Discover => {
                let p = self.provider.provider.clone();
                let base_url = self
                    .provider
                    .field(provider_field(&p, "baseURL"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let key = self
                    .provider
                    .field(provider_field(&p, "apiKey"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                self.request(
                    SettingsRequest::Models {
                        provider: p.clone(),
                        base_url,
                        key,
                    },
                    Completion::Models(p, self.provider.discovery_token()),
                    cx,
                )
            }
            Action::SelectModel(model) => {
                let e = self.provider.select_model(model);
                self.queue(vec![e], cx)
            }
            Action::ActivateProvider => self.queue(
                vec![Edit::Activate {
                    provider: self.provider.provider.clone(),
                    model: self.provider.selected_model.clone(),
                }],
                cx,
            ),
            Action::SaveTask(task) => {
                let provider = self.value(&format!("task.{task}.provider"));
                let model = self.value(&format!("task.{task}.model"));
                self.queue(
                    vec![Edit::TaskSelection {
                        task,
                        provider,
                        model,
                    }],
                    cx,
                )
            }
            Action::AddModel => {
                let id = self.value("model.new");
                if !id.trim().is_empty() {
                    let mut models = self.provider.models.clone();
                    let mut m = if self.value("model.metadata").is_empty() {
                        json!({})
                    } else {
                        match serde_json::from_str::<Value>(&self.value("model.metadata")) {
                            Ok(v) if v.is_object() => v,
                            _ => {
                                self.error = Some("Model metadata must be an object.".into());
                                return;
                            }
                        }
                    };
                    m["id"] = json!(id.trim());
                    models.push(m);
                    let models = settings::providers::normalize_models(&models);
                    self.provider.models = models.clone();
                    self.queue(
                        vec![Edit::Models {
                            provider: self.provider.provider.clone(),
                            models,
                        }],
                        cx,
                    )
                }
            }
            Action::SaveModel(id) => {
                let p = self.provider.provider.clone();
                match serde_json::from_str::<Value>(&self.value(&format!("model.{p}.{id}"))) {
                    Ok(model) if model["id"] == id => {
                        let models = self
                            .provider
                            .models
                            .iter()
                            .map(|m| {
                                if m["id"] == id {
                                    model.clone()
                                } else {
                                    m.clone()
                                }
                            })
                            .collect::<Vec<_>>();
                        let models = settings::providers::normalize_models(&models);
                        self.provider.models = models.clone();
                        self.queue(
                            vec![Edit::Models {
                                provider: p,
                                models,
                            }],
                            cx,
                        )
                    }
                    _ => {
                        self.error =
                            Some("Keep the model ID and enter valid model metadata.".into())
                    }
                }
            }
            Action::RemoveModel(id) => {
                let edit = self.provider.remove_models(BTreeSet::from([id]));
                self.queue(vec![edit], cx)
            }
            Action::CreateProvider => {
                match serde_json::from_str::<Value>(&self.value("provider.new")) {
                    Ok(v) if v["id"].as_str().is_some_and(|id| id.starts_with("custom:")) => {
                        let provider = text(&v["id"]);
                        self.fields.retain(|key, _| {
                            !key.starts_with(&format!("provider.{provider}."))
                                && !key.starts_with(&format!("model.{provider}."))
                        });
                        self.queue(vec![Edit::CreateProvider { provider, value: v }], cx)
                    }
                    _ => {
                        self.error =
                            Some("Enter a custom provider definition with a custom: ID.".into())
                    }
                }
            }
            Action::RemoveProvider => {
                if self.provider.provider.starts_with("custom:") {
                    self.confirm(
                        Action::DeleteProvider(self.provider.provider.clone()),
                        w,
                        cx,
                    )
                }
            }
            Action::DeleteProvider(id) => {
                self.queue(vec![Edit::RemoveProvider(id)], cx);
                self.provider.load("ollama".into(), &self.config);
                self.provider_fields(w, cx)
            }
            Action::SignIn => {
                if matches!(
                    self.device.phase,
                    DevicePhase::Saving
                        | DevicePhase::Starting
                        | DevicePhase::Waiting { .. }
                        | DevicePhase::Polling
                ) {
                    return;
                }
                let token = self.device.saving();
                self.reads.insert("device", token);
                let client_id = self
                    .provider
                    .field("githubOAuthClientId")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                self.flush(Some(Action::BeginSignIn { token, client_id }), cx);
            }
            Action::BeginSignIn { token, client_id } => {
                if let Some(DeviceEffect::Start { token, client_id }) =
                    self.device.saved(token, true, client_id)
                {
                    self.request(
                        SettingsRequest::DeviceStart(client_id),
                        Completion::DeviceStart(token),
                        cx,
                    )
                }
            }
            Action::CancelSignIn => {
                self.device.cancel();
                self.reads
                    .insert("device", self.device_token().wrapping_add(1));
                self.request(SettingsRequest::DeviceCancel, Completion::Simple, cx)
            }
            Action::Account(a) => {
                self.account_generation += 1;
                let g = self.account_generation;
                let open = matches!(a, AccountAction::SignIn);
                if matches!(a, AccountAction::Cancel) {
                    self.account_status = "cancelled".into();
                    self.clear_account_callback(w, cx);
                }
                self.request(
                    SettingsRequest::Account(a),
                    Completion::Account(g, open),
                    cx,
                )
            }
            Action::AccountCallback => {
                let callback = self.value("account.callback");
                if callback.trim().is_empty() {
                    self.error = Some("Paste the callback URL first.".into());
                } else {
                    self.clear_account_callback(w, cx);
                    self.action(Action::Account(AccountAction::Paste(callback)), w, cx)
                }
            }
            Action::KnowledgeControl(operation, trigger) => {
                let e = self.knowledge.begin(operation, trigger, self.now());
                self.knowledge_effects(e, cx)
            }
            Action::Knowledge(operation) => {
                let e = self
                    .knowledge
                    .begin(operation, "knowledge-action".into(), self.now());
                self.knowledge_effects(e, cx)
            }
            Action::ChooseFolder => {
                self.request(SettingsRequest::ChooseFolder, Completion::Folder, cx)
            }
            Action::ClipboardToggle(k, v) => self.request(
                SettingsRequest::ClipboardSet(json!({k:v})),
                Completion::ClipboardMutation,
                cx,
            ),
            Action::ClipboardSize => match self.value("clipboard.size").parse::<f64>() {
                Ok(v) if v.is_finite() && v > 0. => self.request(
                    SettingsRequest::ClipboardSet(json!({"maxImageMegapixels":v})),
                    Completion::ClipboardMutation,
                    cx,
                ),
                _ => self.error = Some("Enter a positive image size.".into()),
            },
            Action::ClearImages => {
                self.request(SettingsRequest::ClearImages, Completion::Simple, cx)
            }
            Action::Vacuum => self.request(SettingsRequest::Vacuum, Completion::Simple, cx),
            Action::LegacyCleanup => cx.emit(SettingsEffect::LegacyCacheCleanup),
            Action::DryRun(v) => self.request(
                SettingsRequest::SetDryRun(v),
                Completion::DryRunMutation,
                cx,
            ),
            Action::Quit => self.flush(Some(Action::EmitQuit), cx),
            Action::EmitQuit => cx.emit(SettingsEffect::Quit),
            Action::OpenPermissions => self.flush(Some(Action::ShowPermissions), cx),
            Action::ShowPermissions => cx.emit(SettingsEffect::OpenPermissions),
            Action::BrowseStore => cx.emit(SettingsEffect::BrowseStore),
            Action::InstalledSelect(id) => {
                self.selected_installed = Some(id);
                self.selected_command = None;
            }
            Action::CommandEnabled(id, v) => self.request(
                SettingsRequest::CommandEnabled(id, v),
                Completion::ExtensionsMutation,
                cx,
            ),
            Action::CommandHotkey(id) => {
                let hotkey = self.value(&format!("command.hotkey.{id}"));
                self.request(
                    SettingsRequest::CommandHotkey(id, hotkey),
                    Completion::ExtensionsMutation,
                    cx,
                )
            }
            Action::CommandAlias(id) => {
                let value = self.value(&format!("command.alias.{id}"));
                self.store.alias(self.owner, id, value);
                self.flush(None, cx)
            }
            Action::Preferences(id, command) => {
                let g = self.next_job + 1;
                self.preferences
                    .insert((id.clone(), command.clone()), (g, Value::Null));
                self.request(
                    SettingsRequest::Preferences {
                        extension: id.clone(),
                        command: command.clone(),
                    },
                    Completion::Preferences(id, command, g),
                    cx,
                )
            }
            Action::PreferenceValue(key, value) => {
                if let Some(f) = self.fields.get_mut(&key) {
                    f.dirty = true;
                    f.value = text(&value);
                    let value = f.value.clone();
                    f.input.update(cx, |v, cx| {
                        v.replace_silently(
                            &value,
                            tezbar_ui::text_input::ByteSelection {
                                range: 0..0,
                                reversed: false,
                            },
                            w,
                            cx,
                        )
                    });
                }
            }
            Action::SavePreferences(id, command) => self.save_preferences(id, command, cx),
            Action::Uninstall(id) => self.confirm(Action::Uninstall(id), w, cx),
            Action::Contribution(intent) => self.contribution_action(intent, w, cx),
            Action::Confirm(yes) => {
                if let Some(focus) = self.confirm_restore.take() {
                    w.focus(&focus, cx)
                }
                if let Some(a) = self.confirmation.take() {
                    if yes {
                        match a {
                            Action::Quit => self.flush(Some(Action::EmitQuit), cx),
                            Action::DeleteProvider(id) => {
                                self.action(Action::DeleteProvider(id), w, cx)
                            }
                            Action::Uninstall(id) => self.request(
                                SettingsRequest::Uninstall(id),
                                Completion::ExtensionsMutation,
                                cx,
                            ),
                            Action::Contribution(ContributionIntent::ResetSecrets) => {
                                let e = self.contributions.operation(
                                    "",
                                    ContributionAction::ResetSecrets,
                                    vec![],
                                );
                                self.accept_contribution(e, cx)
                            }
                            Action::Contribution(ContributionIntent::Remove(id)) => {
                                let e = self.contributions.operation(
                                    &id,
                                    ContributionAction::Remove,
                                    vec![json!(id)],
                                );
                                self.accept_contribution(e, cx)
                            }
                            _ => {}
                        }
                    }
                }
            }
            Action::Close => cx.emit(SettingsEffect::Close),
        }
        self.refresh_timer(cx);
        cx.notify();
    }
    fn prepare_extension_fields(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let rows = view::merge_installed(&self.schema, &self.installed);
        let mut keep = std::collections::BTreeSet::new();
        for row in rows.as_array().into_iter().flatten() {
            let id = text(&row["extName"]);
            for command in row["commands"].as_array().into_iter().flatten() {
                let id = format!("extcmd:{id}:{}", text(&command["name"]));
                for (kind, label, source) in [
                    ("alias", "Alias", "commandAliases"),
                    ("hotkey", "Shortcut", "commandHotkeys"),
                ] {
                    let key = format!("command.{kind}.{id}");
                    keep.insert(key.clone());
                    let value = text(&self.extension_settings[source][&id]);
                    self.field(&key, label, FieldKind::Local, &value, false, false, w, cx);
                }
            }
        }
        // Dropping a removed field also drops its input subscription.
        self.fields
            .retain(|key, _| !key.starts_with("command.") || keep.contains(key));
    }
    fn confirmation_copy(&self, action: &Action) -> (String, String, String) {
        let (title, description, accept) = match action {
            Action::Quit => ("Quit Tezbar?".into(), "Background tasks and terminals will stop.".into(), "Quit Tezbar"),
            Action::DeleteProvider(id) => {
                let name = self.config["customProviders"].as_array().into_iter().flatten()
                    .find(|p| p["id"].as_str() == Some(id)).and_then(|p| p["name"].as_str()).unwrap_or("this provider");
                (format!("Remove {name}?"), "Its saved configuration will be removed. Existing conversations stay in history.".into(), "Remove provider")
            }
            Action::Uninstall(id) => {
                let rows = view::merge_installed(&self.schema, &self.installed);
                let name = rows.as_array().into_iter().flatten().find(|p| p["extName"].as_str() == Some(id))
                    .and_then(|p| p["title"].as_str()).unwrap_or("this extension");
                (format!("Uninstall {name}?"), "Its commands will no longer be available.".into(), "Uninstall extension")
            }
            Action::Contribution(ContributionIntent::ResetSecrets) => ("Reset saved connection secrets?".into(), "Saved tokens, passwords and provider credentials from AI extensions will be removed. Enter them again to reconnect.".into(), "Reset connection secrets"),
            Action::Contribution(ContributionIntent::Remove(id)) => {
                let name = self.contributions.display["extensions"].as_array().into_iter().flatten()
                    .find(|e| e["id"].as_str() == Some(id)).and_then(|e| e["name"].as_str()).unwrap_or("this AI extension");
                (format!("Remove {name}?"), "Its connections will stop and its saved secrets will be removed.".into(), "Remove AI extension")
            }
            _ => ("Discard these changes?".into(), "This action cannot be undone.".into(), "Discard changes"),
        };
        (title, description, accept.into())
    }
    fn prepare_preferences(
        &mut self,
        id: &str,
        command: Option<String>,
        v: &Value,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for f in v["preferences"].as_array().into_iter().flatten() {
            if command.is_some() && f["commandName"].as_str() != command.as_deref() {
                continue;
            }
            let name = text(&f["name"]);
            let key = format!(
                "pref.{id}.{}.{name}",
                command.as_deref().unwrap_or("$extension")
            );
            let value = text(&v["values"][&name]);
            self.field(
                &key,
                f["title"].as_str().unwrap_or(&name),
                FieldKind::Local,
                &value,
                f["type"] == "password",
                false,
                w,
                cx,
            );
        }
    }
    fn save_preferences(&mut self, id: String, _command: Option<String>, cx: &mut Context<Self>) {
        use tezbar_ui_model::extensions::forms::{PreferenceField, Preferences};
        let setups = self
            .preferences
            .iter()
            .filter(|((extension, _), _)| *extension == id)
            .map(|((_, command), (_, setup))| {
                let fields = setup["preferences"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|f| PreferenceField {
                        name: text(&f["name"]),
                        command: f["commandName"].as_str().map(str::to_owned),
                        kind: text(&f["type"]),
                        required: f["required"] == true,
                        options: f["data"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|v| text(&v["value"]))
                            .collect(),
                    })
                    .collect();
                let values = setup["values"]
                    .as_object()
                    .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default();
                (command.clone(), fields, values)
            })
            .collect();
        let mut model = Preferences::merge(setups);
        for field in model.fields.clone() {
            let key = format!(
                "pref.{id}.{}.{}",
                field.command.as_deref().unwrap_or("$extension"),
                field.name
            );
            if let Some(f) = self.fields.get(&key) {
                let value = if field.kind == "checkbox" {
                    json!(f.value == "true")
                } else {
                    json!(f.value)
                };
                let _ = model.edit(&field.key(), value);
            }
        }
        match model.partition() {
            Ok(groups) => {
                for (command, values) in groups {
                    self.request(
                        SettingsRequest::SavePreferences {
                            extension: id.clone(),
                            command,
                            values: json!(values),
                        },
                        Completion::Simple,
                        cx,
                    )
                }
            }
            Err(e) => self.error = Some(e.into()),
        }
    }
}
impl Drop for SettingsContent {
    fn drop(&mut self) {
        self.store.retire(self.owner)
    }
}
fn text(v: &Value) -> String {
    v.as_str().map(str::to_owned).unwrap_or_else(|| {
        if v.is_null() {
            String::new()
        } else {
            v.to_string()
        }
    })
}
fn decode(r: Reply) -> Result<knowledge::Snapshot, String> {
    r.and_then(|v| serde_json::from_value(v).map_err(|_| "Could not read Knowledge status".into()))
}

fn provider_field<'a>(provider: &str, field: &'a str) -> &'a str {
    match (provider, field) {
        ("gemini", "apiKey") => "geminiApiKey",
        ("copilot", "apiKey") => "copilotGithubToken",
        ("openai-compatible", "baseURL") => "openaiCompatibleBaseURL",
        _ => field,
    }
}
impl Completion {
    fn scope(&self) -> Option<&'static str> {
        match self {
            Self::Storage => Some("storage"),
            Self::Clipboard => Some("clipboard"),
            Self::DryRun => Some("dry-run"),
            Self::Installed => Some("installed"),
            Self::Schema => Some("schema"),
            Self::ExtensionSettings => Some("extension-settings"),
            Self::Statuses => Some("statuses"),
            Self::Pi => Some("pi"),
            Self::Permissions => Some("permissions"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod native_input_tests;
