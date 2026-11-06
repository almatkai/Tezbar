//! Native command bar. All I/O starts in event/effect methods; Render only constructs elements.
mod interactions;
mod locale;
pub mod navigation;
mod presentation;
#[cfg(test)]
mod tests;
mod view;
use crate::{
    core_client::{CoreClient, Request, RequestAdapter},
    events::{CoreEvent, CoreEvents, Event, EventSubscription, Relay},
    windows::resize,
};
use gpui::{prelude::*, *};
use navigation::{Navigation, Payload, UpdateAction};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tezbar_ui::{
    controls,
    image_cache::ImageCache,
    markdown::{DocumentEffect, MarkdownView},
    text_input::{ClipboardEffect, Committed, FieldOptions, TextInput},
    tokens,
};
use tezbar_ui_model::{
    launcher::{
        self, Action, Arguments, Mode, Row,
        controller::{Effect, Input, Launcher as Model},
    },
    router::{Effect as RouteEffect, Router, Surface},
    ui_state::{self, UiState},
};
struct Form {
    row: Row,
    args: Arguments,
    labels: Vec<presentation::Field>,
    inputs: Vec<Entity<TextInput>>,
    ticket: u64,
    generation: u64,
    _subscriptions: Vec<Subscription>,
}
#[derive(Clone)]
struct Activation {
    ticket: u64,
    generation: u64,
    row: Row,
    query: String,
    rank: usize,
}
#[derive(Clone, Hash)]
enum IconSource {
    Data(String),
    Asset { kind: String, path: String },
}
struct PinDrag {
    id: String,
    start: Point<Pixels>,
    active: bool,
    target: Option<usize>,
}
pub struct Launcher {
    resize: Option<resize::Gesture>,
    window: AnyWindowHandle,
    writes: Vec<Task<()>>,
    pub input: Entity<TextInput>,
    pub rows: Vec<Arc<Row>>,
    pub ready: bool,
    pub error: Option<String>,
    pub visible: bool,
    pub zoom: f32,
    pub model: Model,
    clock: Instant,
    executor: BackgroundExecutor,
    scroll: UniformListScrollHandle,
    timer: Option<Task<()>>,
    timer_at: Option<u64>,
    tasks: Vec<Task<()>>,
    adapter: Arc<dyn RequestAdapter>,
    relay: Relay,
    images: ImageCache,
    icons: HashMap<String, Arc<RenderImage>>,
    icon_queue: std::collections::VecDeque<(String, IconSource)>,
    home: Vec<Arc<Row>>,
    paths: HashMap<String, Value>,
    pub state: UiState,
    pub update_status: tezbar_update::AppUpdateStatus,
    checked_update: bool,
    state_path: Option<PathBuf>,
    state_writer: Arc<tokio::sync::Mutex<()>>,
    persist_revision: Arc<std::sync::atomic::AtomicU64>,
    write_error: Arc<std::sync::Mutex<Option<String>>>,
    prefix: Option<char>,
    argument_revision: u64,
    argument_rows: Vec<Row>,
    argument_task: Option<Task<()>>,
    form: Option<Form>,
    form_activation: Option<Activation>,
    menu: Option<(Row, usize)>,
    pin_picker: Option<(Row, usize)>,
    drag: Option<PinDrag>,
    pin_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    pointer: Option<Point<Pixels>>,
    suppress_pin_click: bool,
    navigation: Option<Navigation>,
    nonce: u64,
    deep_navigation: bool,
    terminal_prompt: String,
    terminal_settings: bool,
    terminal_draft: ui_state::TerminalDefaults,
    model_config: Value,
    model_menu: bool,
    model_choices: Vec<(String, String)>,
    model_choice: usize,
    model_saving: bool,

    history: bool,
    history_index: usize,
    notice: Option<String>,
    message_kind: Option<String>,
    message_document: Entity<MarkdownView>,
    notice_timer: Option<Task<()>>,
    answer: Entity<MarkdownView>,
    answer_text: String,
    answer_revision: u64,
    answer_active: bool,
    answer_busy: bool,
    _events: Option<EventSubscription>,
    _subscriptions: Vec<Subscription>,
}
impl Launcher {
    pub fn new(core: CoreClient, relay: Relay, w: &mut Window, cx: &mut Context<Self>) -> Self {
        let path = core.0.ctx.paths.config_dir.join("ui-state-v1.json");
        Self::with_adapter(Arc::new(core), relay, Some(path), w, cx)
    }
    pub fn with_adapter(
        adapter: Arc<dyn RequestAdapter>,
        relay: Relay,
        path: Option<PathBuf>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            let mut i = TextInput::new(cx);
            i.configure(FieldOptions {
                name: "Search apps, files and commands".into(),
                placeholder: "Search apps, files and commands".into(),
                border: false,
                ..Default::default()
            });
            i
        });
        let answer = cx.new(|_| MarkdownView::default());
        let message_document = cx.new(|_| MarkdownView::default());
        let committed = cx.subscribe_in(&input, w, |this, _, e: &Committed, _, cx| {
            this.query(e.0.clone(), cx)
        });
        let composition = cx.observe_in(&input, w, |this, input, _, cx| {
            let active = input.read(cx).marked_range.is_some();
            if active != this.model.composing {
                this.dispatch(Input::Composition(active), cx);
            }
        });
        let clipboard =
            cx.subscribe_in(&input, w, |_, field, e: &ClipboardEffect, w, cx| match e {
                ClipboardEffect::Read => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                        field.update(cx, |i, cx| i.paste_text(&text, w, cx));
                    }
                }
                ClipboardEffect::Write(text) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()))
                }
            });
        let doc = cx.subscribe_in(&answer, w, |this, _, e: &DocumentEffect, _, cx| match e {
            DocumentEffect::Copy(text) => this.request_notice(
                Request::Activate {
                    action: json!(Action::CopyText { text: text.clone() }),
                    query: String::new(),
                    rank: 1,
                    result_id: "answer:copy".into(),
                },
                cx,
            ),
            DocumentEffect::OpenLink(url) => this.request_notice(Request::Open(url.clone()), cx),
            DocumentEffect::Image(_) => {}
        });
        let message_copy = cx.subscribe_in(
            &message_document,
            w,
            |this, _, e: &DocumentEffect, _, cx| {
                if let DocumentEffect::Copy(text) = e {
                    this.request_notice(
                        Request::Activate {
                            action: json!(Action::CopyText { text: text.clone() }),
                            query: String::new(),
                            rank: 1,
                            result_id: "command-result:copy".into(),
                        },
                        cx,
                    );
                }
            },
        );
        let queue = relay.clone();
        let activation = cx.observe_window_activation(w, move |this, w, _| {
            if this.visible && !w.is_window_active() {
                let _ = queue.send(Event::Blur(0));
            }
        });
        let appearance = cx.observe_window_appearance(w, |_, _, cx| cx.notify());
        let queue = relay.clone();
        let bounds = cx.observe_window_bounds(w, move |_, _, _| {
            let _ = queue.send(Event::Bounds);
        });
        let release = cx.on_release(|this, cx| this.images.release_all_windows(cx));
        let (state, error) = match path
            .as_ref()
            .filter(|p| p.exists())
            .map(std::fs::read_to_string)
        {
            Some(Ok(raw)) => match UiState::decode(&raw) {
                Ok(s) => (s, None),
                Err(e) => (UiState::default(), Some(e)),
            },
            Some(Err(e)) => (UiState::default(), Some(e.to_string())),
            None => (UiState::default(), None),
        };
        // Corrupt state is never overwritten. M11 alone owns legacy discovery/import.
        let state_path = if error.is_none() { path } else { None };
        let events = cx.try_global::<CoreEvents>().cloned().map(|hub| {
            hub.subscribe(
                &cx.entity(),
                vec![
                    "launcher:invalidated",
                    "backend-status",
                    "stream-token",
                    "stream-done",
                    "stream-error",
                    "update-status",
                    "terminal:sessions-changed",
                    "terminal:exit",
                    "app:open-surface",
                    "run-extension-command-from-hotkey",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect(),
                |entity, event, cx| {
                    entity.update(cx, |this, cx| this.core_event(event, cx));
                },
                cx,
            )
        });
        Self {
            resize: None,
            window: w.window_handle(),
            writes: vec![],
            input,
            rows: vec![],
            ready: false,
            error,
            visible: false,
            zoom: 1.,
            model: Model::default(),
            clock: cx.background_executor().now(),
            executor: cx.background_executor().clone(),
            scroll: UniformListScrollHandle::new(),
            timer: None,
            timer_at: None,
            tasks: vec![],
            adapter,
            relay,
            images: ImageCache::default(),
            icons: HashMap::new(),
            icon_queue: Default::default(),
            home: vec![],
            paths: HashMap::new(),
            state: state.clone(),
            update_status: tezbar_update::AppUpdateStatus::Idle,
            checked_update: false,
            state_path,
            state_writer: Arc::default(),
            persist_revision: Arc::default(),
            write_error: Arc::default(),
            prefix: None,
            argument_revision: 0,
            argument_rows: vec![],
            argument_task: None,
            form: None,
            form_activation: None,
            menu: None,
            pin_picker: None,
            drag: None,
            pin_bounds: Default::default(),
            pointer: None,
            suppress_pin_click: false,
            navigation: None,
            nonce: 0,
            deep_navigation: false,
            terminal_prompt: String::new(),
            terminal_settings: false,
            terminal_draft: state.terminal_defaults.clone(),
            model_config: json!({}),
            model_menu: false,
            model_choices: vec![],
            model_choice: 0,
            model_saving: false,
            history: false,
            history_index: 0,
            notice: None,
            message_kind: None,
            message_document,
            notice_timer: None,
            answer,
            answer_text: String::new(),
            answer_revision: 0,
            answer_active: false,
            answer_busy: false,
            _events: events,
            _subscriptions: vec![
                committed,
                composition,
                clipboard,
                doc,
                message_copy,
                activation,
                appearance,
                bounds,
                release,
            ],
        }
    }
    fn now(&self) -> u64 {
        self.executor.now().duration_since(self.clock).as_millis() as u64
    }
    fn dispatch(&mut self, input: Input, cx: &mut Context<Self>) {
        self.tasks.retain(|t| !t.is_ready());
        let effects = self.model.update(input, self.now());
        for effect in effects {
            self.effect(effect, cx);
        }
        self.rows = self.model.display_rows();
        self.sync_input(cx);
        if self.error.is_none() {
            self.error = self.model.error.clone();
        }
        cx.notify();
    }
    pub fn query(&mut self, text: String, cx: &mut Context<Self>) {
        if self.navigation.is_some() || self.form.is_some() || self.form_activation.is_some() {
            return;
        }
        if !self.ready || !self.visible {
            return;
        }
        let previous_mode = launcher::input_mode(&self.model.input, self.model.terminal_mode).mode;
        self.answer_active = false;
        self.deep_navigation = false;
        self.menu = None;
        self.error = None;
        let text = if let Some(prefix) = self.prefix {
            if text.starts_with(prefix) {
                text
            } else {
                format!("{prefix}{text}")
            }
        } else {
            text
        };
        if let Some(prefix) = text.chars().next().filter(|c| matches!(c, '!' | '`' | '>')) {
            self.prefix = Some(prefix);
            self.model.terminal_mode = prefix == '>';
            let display = text[1..].to_owned();
            if self.input.read(cx).content.as_ref() != display {
                let field = self.input.clone();
                let window = self.window;
                cx.defer(move |cx| {
                    let _ = window.update(cx, |_, w, cx| {
                        field.update(cx, |i, cx| i.set_text(&display, w, cx))
                    });
                });
            }
        }

        self.dispatch(Input::Edit(text), cx);
        let mode = launcher::input_mode(&self.model.input, self.model.terminal_mode).mode;
        if mode == Mode::Terminal && previous_mode != Mode::Terminal {
            self.load_terminal(cx);
        }
        if mode == Mode::Ai && previous_mode != Mode::Ai {
            self.open_models(cx);
            self.model_menu = false;
        }
    }
    pub fn show(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if !self.visible {
            self.visible = true;
            self.checked_update = false;
            self.model.session.reset_transport();
            if self.ready {
                self.dispatch(Input::Show, cx);
            }
            self.rows = self.home.clone();
        }
        cx.activate(true);
        w.activate_window();
        self.input.focus_handle(cx).focus(w, cx);
        cx.notify();
    }
    pub fn hide(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.visible = false;
        self.dispatch(Input::Hide, cx);
        self.model.session.reset_transport();
        self.timer = None;
        self.timer_at = None;
        self.tasks.clear();
        self.form = None;
        self.form_activation = None;
        self.menu = None;
        self.pin_picker = None;
        self.drag = None;
        self.answer_active = false;
        self.answer_busy = false;
        self.model.terminal_mode = false;
        self.prefix = None;
        self.argument_task = None;
        self.argument_rows.clear();
        self.model_menu = false;
        self.terminal_settings = false;
        self.navigation = None;
        self.notice_timer = None;
        self.notice = None;
        self.message_kind = None;
        self.answer_revision += 1;
        let revision = self.answer_revision;
        self.message_document
            .update(cx, |v, cx| v.set_content(revision, "", true, cx));
        self.input.update(cx, |i, cx| {
            i.cancel_composition(w, cx);
            i.clear_silently(w, cx);
        });
        self.images.release(w, cx);
        self.icons.clear();
        self.icon_queue.clear();
        cx.notify();
    }
    fn edit(&mut self, text: &str, w: &mut Window, cx: &mut Context<Self>) {
        self.prefix = text.chars().next().filter(|c| matches!(c, '!' | '`' | '>'));
        let display = if self.prefix.is_some() {
            &text[1..]
        } else {
            text
        };
        self.input.update(cx, |i, cx| i.set_text(display, w, cx));
        self.query(text.into(), cx);
    }
    fn sync_input(&mut self, cx: &mut Context<Self>) {
        let raw = self.model.input.clone();
        let prefix = raw.chars().next().filter(|c| matches!(c, '!' | '`' | '>'));
        let display = if prefix.is_some() {
            raw[1..].to_owned()
        } else {
            raw
        };
        if self.input.read(cx).marked_range.is_some() || self.form.is_some() {
            return;
        }
        self.prefix = prefix;
        if self.input.read(cx).content.as_ref() == display {
            return;
        }
        let entity = cx.entity();
        let window = self.window;
        let generation = self.model.session.generation;
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, w, cx| {
                entity.update(cx, |this, cx| {
                    if generation == this.model.session.generation
                        && this.input.read(cx).marked_range.is_none()
                    {
                        this.input.update(cx, |i, cx| {
                            i.clear_silently(w, cx);
                            i.content = display.clone().into();
                            let end = display.encode_utf16().count();
                            i.set_selected_text_range(end..end, w, cx);
                        });
                    }
                })
            });
        });
    }
    fn effect(&mut self, e: Effect, cx: &mut Context<Self>) {
        match e {
            Effect::Schedule { at_ms } => {
                if self.timer_at.is_some_and(|at| at <= at_ms) {
                    return;
                }
                self.timer_at = Some(at_ms);
                let delay = at_ms.saturating_sub(self.now());
                self.timer = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(delay))
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.timer_at = None;
                        this.dispatch(Input::Tick, cx);
                    });
                }));
            }
            Effect::Search { generation, query } => self.search(generation, query, 0, cx),
            Effect::CompletePath { generation, query } => self.search(generation, query, 1, cx),
            Effect::CompleteApplications { generation, query } => {
                self.search(generation, query, 2, cx)
            }
            Effect::SearchChats { generation, query } => self.search(generation, query, 3, cx),
            Effect::Execute {
                ticket,
                generation,
                row_id,
                action,
                query,
                rank,
            } => {
                let row = self
                    .model
                    .session
                    .rows
                    .iter()
                    .find(|r| r.id == row_id)
                    .map(|r| r.as_ref().clone())
                    .unwrap_or_else(|| {
                        let mut r = Row::command(&row_id, &row_id);
                        r.action = action;
                        r
                    });
                self.activate(
                    Activation {
                        ticket,
                        generation,
                        row,
                        query,
                        rank: rank + 1,
                    },
                    cx,
                );
            }
            Effect::RecordQuery(q) => {
                launcher::record_recent(&mut self.state.launcher_query_history, &q, 1);
                self.persist(cx);
            }
            Effect::ResolveDirectory { ticket, path } => {
                let adapter = self.adapter.clone();
                self.tasks.push(cx.spawn(async move |this, cx| {
                    let result = gpui_tokio::Tokio::spawn(cx, async move {
                        let path = adapter
                            .send(Request::ResolveDirectory(path.clone()))
                            .await?
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                            .ok_or_else(|| format!("Directory does not exist: {path}"))?;
                        adapter.send(Request::DirectoryVisit(path.clone())).await?;
                        Ok::<_, String>(path)
                    })
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()));
                    let _ = this.update(cx, |this, cx| {
                        this.dispatch(Input::DirectoryResolved { ticket, result }, cx)
                    });
                }));
            }
            Effect::OpenAiChat { ticket, boot } => {
                let (prompt, cwd) = match boot {
                    Some(launcher::AiBoot::Submit {
                        prompt,
                        working_directory,
                    }) => (Some(prompt), working_directory),
                    Some(launcher::AiBoot::NewChat { working_directory }) => {
                        (None, Some(working_directory))
                    }
                    None => (None, None),
                };
                self.navigation_complete(ticket);
                self.navigate(
                    Surface::AiChat,
                    Payload::Ai {
                        prompt,
                        cwd,
                        resume: None,
                        capture: false,
                    },
                    cx,
                );
            }
            Effect::OpenTerminal {
                ticket,
                command,
                session_id,
            } => {
                self.navigation_complete(ticket);
                self.navigate(
                    Surface::Terminal,
                    Payload::Terminal {
                        command,
                        cwd: None,
                        session: session_id,
                        defaults: self.state.terminal_defaults.clone(),
                    },
                    cx,
                );
            }
            Effect::OpenNote { ticket, note_id } => {
                if let Some(row) = self
                    .model
                    .session
                    .rows
                    .get(self.model.session.selected)
                    .map(|r| r.as_ref().clone())
                {
                    self.record(
                        &Activation {
                            ticket,
                            generation: self.model.session.generation,
                            row,
                            query: self.model.input.clone(),
                            rank: self.model.session.selected + 1,
                        },
                        cx,
                    );
                }
                self.navigation_complete(ticket);
                self.navigate(
                    Surface::Notes,
                    Payload::Note {
                        created_at: note_id,
                    },
                    cx,
                );
            }
            Effect::Hide => {
                let _ = self.relay.send(Event::Hide);
            }
        }
    }
    fn search(&mut self, generation: u64, query: String, kind: u8, cx: &mut Context<Self>) {
        let adapter = self.adapter.clone();
        let prefs = self.state.clone();
        let status = self.update_status.clone();
        let checked = self.checked_update;
        self.tasks.push(cx.spawn(async move|this,cx| {
            let q=query.clone();
            let result=gpui_tokio::Tokio::spawn(cx,async move {
                if kind==1 || kind==2 {return presentation::path_rows(adapter.send(Request::CompletePath(q)).await?);}
                if kind==3 {let rows=presentation::history_rows(adapter.send(Request::Chats).await?,false)?;let q=q.to_lowercase();return Ok((rows.into_iter().filter(|r|r.title.to_lowercase().contains(&q)).collect(),HashMap::new()));}
                let request=if kind==2 {format!("`{}",q.trim_start_matches('`'))}else{q.clone()};
                let value=adapter.send(Request::Search(request)).await?;
                let mut rows:Vec<Row>=serde_json::from_value(value).map_err(|e|e.to_string())?;
                if kind==2 {rows.retain(|r|r.category=="applications");}
                else if !q.starts_with('!') {
                    let mut instant=presentation::instant_rows(&q);
                    instant.append(&mut rows);rows=instant;
                    if q.is_empty(){
                        rows.sort_by_key(|r|prefs.recent_extension_commands.iter().position(|id|id==&r.id).map(|i|i+1).unwrap_or(100));
                        // Pins are displayed separately, but remain in the authoritative activation set.
                        for pin in prefs.pins.iter().rev(){if !rows.iter().any(|r|r.id==pin.id){rows.insert(0,Row{id:pin.id.clone(),title:pin.title.clone(),subtitle:pin.subtitle.clone(),category:pin.category.clone(),score:0.,action:pin.action.clone(),icon_data_url:pin.icon_data_url.clone(),ephemeral:false});}}
                    }
                }
                if kind==0 && !q.starts_with('!') {
                    rows.retain(|r| !matches!(&r.action,Action::InvokeCommand{command_id,..} if command_id=="check-for-updates"));
                    if let Some(update)=presentation::update_row(&status,checked,&q){rows.insert(0,update);}
                    let mut refs=rows.into_iter().map(Arc::new).collect::<Vec<_>>();
                    launcher::release_notes_row(&mut refs,&q,prefs.release_notes_version.as_deref()!=Some(env!("CARGO_PKG_VERSION")),env!("CARGO_PKG_VERSION"));
                    rows=refs.into_iter().map(|r|r.as_ref().clone()).collect();
                }
                Ok::<_,String>((rows,HashMap::new()))
            }).await.unwrap_or_else(|e|Err(e.to_string()));
            let _=this.update(cx,|this,cx| {
                if this.visible && generation==this.model.session.generation {
                    if let Ok((_,paths))=&result {this.paths=paths.clone();}
                }
                this.dispatch(Input::SearchComplete{generation,result:result.map(|(rows,_)|rows)},cx);
                if query.is_empty()&&generation==this.model.session.generation {this.home=this.model.session.rows.clone();}
                this.prepare_icons(cx);
                if kind==0 && this.visible && generation==this.model.session.generation {this.currency(query,generation,cx);}

            });
        }));
    }
    fn currency(&mut self, query: String, generation: u64, cx: &mut Context<Self>) {
        let mut conversion = tezbar_core::instant::currency::Conversion::default();
        let Some(tezbar_core::instant::currency::ConversionEffect::FetchRates {
            generation: currency_generation,
            from,
        }) = conversion
            .input(&query, &self.state.currency)
            .into_iter()
            .next()
        else {
            return;
        };
        let adapter = self.adapter.clone();
        self.tasks.push(cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move {
                let value = adapter.send(Request::Currency(from)).await?;
                let rates =
                    serde_json::from_value(value["rates"].clone()).map_err(|e| e.to_string());
                let effects =
                    conversion.complete(currency_generation, rates.as_ref().map_err(Clone::clone));
                Ok::<_, String>((conversion.formatted(&locale::NativeLocale), effects))
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| {
                if !this.visible
                    || generation != this.model.session.generation
                    || this.model.session.pending()
                {
                    return;
                }
                if let Ok((Some(result), effects)) = result {
                    let mut row = Row::command("currency:conversion", &result.formatted);
                    row.id = "currency:conversion".into();
                    row.category = "calculator".into();
                    row.subtitle = result.amount_formatted;
                    row.ephemeral = true;
                    row.action = Action::CopyText {
                        text: result.clipboard,
                    };
                    this.model
                        .session
                        .rows
                        .retain(|r| r.id != "currency:conversion");
                    this.model.session.rows.insert(0, Arc::new(row));
                    for e in effects {
                        if let tezbar_core::instant::currency::ConversionEffect::RecordTarget(
                            code,
                        ) = e
                        {
                            this.state.currency.record(&code, epoch_ms());
                            this.persist(cx);
                        }
                    }
                    this.rows = this.model.display_rows();
                    cx.notify();
                }
            });
        }));
    }
    fn persist(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.state_path.clone() else {
            return;
        };
        let state = self.state.clone();
        let lock = self.state_writer.clone();
        let revision = self.persist_revision.clone();
        let serial = revision.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        self.writes.retain(|t| !t.is_ready());
        self.writes.push(cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move {
                let _guard = lock.lock().await;
                if revision.load(std::sync::atomic::Ordering::SeqCst) != serial {
                    return Ok(());
                }
                tokio::task::spawn_blocking(move || state.write_atomic(&path))
                    .await
                    .map_err(|e| e.to_string())?
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| {
                if this
                    .persist_revision
                    .load(std::sync::atomic::Ordering::SeqCst)
                    == serial
                {
                    *this.write_error.lock().unwrap() = result.as_ref().err().cloned();
                    if let Err(e) = result {
                        this.error = Some(format!("Could not save launcher state: {e}"));
                        cx.notify();
                    }
                }
            });
        }));
    }
    fn navigation_complete(&mut self, ticket: u64) {
        // Successful navigation keeps the window open for its destination. Consume the model's
        // completion/reset but not its window-hide effect; app owns the destination lifecycle.
        let _ = self.model.update(
            Input::ActivationComplete {
                ticket,
                result: Ok(()),
            },
            self.now(),
        );
    }
    fn finish_local(&mut self, a: &Activation) {
        self.navigation_complete(a.ticket);
    }
    fn activate(&mut self, a: Activation, cx: &mut Context<Self>) {
        if a.generation != self.model.session.generation || !self.visible {
            self.finish_local(&a);
            return;
        }
        if let Some((args, labels)) = presentation::arguments(&a.row) {
            self.form_activation = Some(a.clone());
            // Entity fields need a Window for focus only. Construction and subscriptions are GPUI-owned.
            let mut subscriptions = Vec::new();
            let mut inputs = Vec::new();
            for (index, label) in labels.iter().enumerate() {
                let value = args.values[index].clone();
                let label = label.clone();
                let field = cx.new(|cx| {
                    let mut i = TextInput::new(cx);
                    i.configure(FieldOptions {
                        name: label.title.clone().into(),
                        placeholder: label.placeholder.into(),
                        password: label.password,
                        ..Default::default()
                    });
                    i.content = value.clone().into();
                    i
                });
                subscriptions.push(cx.subscribe(&field, move |this, _, e: &Committed, cx| {
                    this.argument_changed(index, &e.0, cx)
                }));
                let window = self.window;
                subscriptions.push(cx.subscribe(
                    &field,
                    move |_, field, e: &ClipboardEffect, cx| {
                        let field = field.clone();
                        let _ = window.update(cx, move |_, w, cx| match e {
                            ClipboardEffect::Read => {
                                if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text())
                                {
                                    field.update(cx, |i, cx| i.paste_text(&text, w, cx));
                                }
                            }
                            ClipboardEffect::Write(text) => {
                                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()))
                            }
                        });
                    },
                ));
                inputs.push(field);
            }
            self.form = Some(Form {
                row: a.row,
                args,
                labels,
                inputs,
                ticket: a.ticket,
                generation: a.generation,
                _subscriptions: subscriptions,
            });
            let entity = cx.entity();
            let window = self.window;
            cx.defer(move |cx| {
                let _ = window.update(cx, |_, w, cx| {
                    entity.update(cx, |this, cx| {
                        if let Some(form) = &this.form {
                            if let Some(field) = form.inputs.first() {
                                let value = form.args.values[0].clone();
                                field.update(cx, |i, cx| i.set_text(&value, w, cx));
                                field.focus_handle(cx).focus(w, cx);
                            }
                        }
                    })
                });
            });
            cx.notify();
            return;
        }
        self.execute_activation(a, cx);
    }
    fn execute_activation(&mut self, a: Activation, cx: &mut Context<Self>) {
        if let Action::InvokeCommand {
            command_id,
            payload,
        } = &a.row.action
        {
            if command_id == "activate-deep-search" {
                self.finish_local(&a);
                self.model.visible = true;
                self.dispatch(
                    Input::Edit(format!(
                        "!{}",
                        payload["query"]
                            .as_str()
                            .unwrap_or(a.query.trim_start_matches('!'))
                    )),
                    cx,
                );
                return;
            }
            if command_id == "__chat-session" || command_id == "__terminal-session" {
                let terminal = command_id == "__terminal-session";
                let p = if terminal {
                    Payload::Terminal {
                        command: None,
                        cwd: payload["cwd"].as_str().map(str::to_owned),
                        session: payload["sessionId"].as_str().map(str::to_owned),
                        defaults: self.state.terminal_defaults.clone(),
                    }
                } else {
                    Payload::Ai {
                        prompt: None,
                        cwd: None,
                        resume: payload["id"].as_str().map(str::to_owned),
                        capture: false,
                    }
                };
                self.finish_local(&a);
                self.navigate(
                    if terminal {
                        Surface::Terminal
                    } else {
                        Surface::AiChat
                    },
                    p,
                    cx,
                );
                return;
            }
        }
        let command = match &a.row.action {
            Action::InvokeCommand { command_id, .. } | Action::RunNativeCommand { command_id } => {
                Some(command_id.clone())
            }
            _ => None,
        };
        if let Some(command) = command {
            let mut router = Router::default();
            if let Some(effects) = router.invoke(&command, &a.query, self.now()) {
                self.record(&a, cx);
                self.finish_local(&a);
                for e in effects {
                    match e {
                        RouteEffect::Navigate(surface) => {
                            let p = if let Surface::Settings(tab) = &surface {
                                Payload::Settings { tab: tab.clone() }
                            } else {
                                Payload::None
                            };
                            self.navigate(surface, p, cx);
                        }
                        RouteEffect::QuitConfirm => {
                            let _ = self.relay.send(Event::Quit);
                        }
                        RouteEffect::UpdateAction => self.update_action(cx),
                        RouteEffect::ActivateDeepSearch(q) => {
                            self.model.input = format!("!{q}");
                            self.dispatch(Input::Invalidate { text: None }, cx);
                        }
                        _ => {}
                    }
                }
                return;
            }
        }
        if let Action::OpenVaultItem { item_id } = &a.row.action {
            self.record(&a, cx);
            self.finish_local(&a);
            self.navigate(
                Surface::Vault,
                Payload::Vault {
                    item_id: item_id.clone(),
                },
                cx,
            );
            return;
        }
        if a.row.category == "quick-notes" {
            if let Some(created_at) = a.row.id.strip_prefix("note:").and_then(|v| v.parse().ok()) {
                self.record(&a, cx);
                self.finish_local(&a);
                self.navigate(Surface::Notes, Payload::Note { created_at }, cx);
                return;
            }
        }
        let adapter = self.adapter.clone();
        let request = if let Action::RunExtensionCommand {
            extension_id,
            command_name,
            extra,
        } = &a.row.action
        {
            self.record(&a, cx);
            Request::ExtensionRun(
                json!({"extensionId":extension_id,"commandName":command_name,"argumentValues":extra.get("argumentValues").cloned().unwrap_or(json!({}))}),
            )
        } else {
            Request::Activate {
                action: json!(a.row.action),
                query: a.query.clone(),
                rank: a.rank,
                result_id: a.row.id.clone(),
            }
        };
        self.tasks.push(cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move { adapter.send(request).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| {
                if a.generation != this.model.session.generation || !this.visible {
                    this.finish_local(&a);
                    return;
                }
                match result {
                    Ok(value) if value["ok"] != false => {
                        if matches!(a.row.action, Action::RunExtensionCommand { .. }) {
                            launcher::record_recent(
                                &mut this.state.recent_extension_commands,
                                &a.row.id,
                                20,
                            );
                            this.persist(cx);
                            if value["mode"] == "view"
                                || value["root"].is_object()
                                || value["sessionId"].is_string()
                                || value["preferenceSetup"].is_object()
                            {
                                this.finish_local(&a);
                                this.navigate(
                                    Surface::ExtensionRuntime,
                                    Payload::Extension { result: value },
                                    cx,
                                );
                                return;
                            }
                        }
                        let effects: Vec<tezbar_ui_model::extensions::WireEffect> =
                            serde_json::from_value(value["effects"].clone()).unwrap_or_default();
                        let mut records = tezbar_ui_model::extensions::WorkerRecords::default();
                        let worker_notice =
                            records.notice(&effects, value["effectSequence"].as_u64());
                        // Worker/core effects have already run. Only toast/HUD text is presented here.
                        if let Some(message) = worker_notice
                            .as_deref()
                            .or_else(|| value["message"].as_str())
                            .filter(|m| !m.is_empty())
                        {
                            this.finish_local(&a);
                            this.model.input.clear();
                            this.model.session.hide();
                            this.model.session.reset_transport();
                            let ms = match value["kind"].as_str() {
                                Some("password") => 9000,
                                Some("copied" | "toggle" | "info") => 7000,
                                _ => 4000,
                            };
                            this.sync_input(cx);
                            this.dispatch(Input::Show, cx);
                            this.card_message(
                                message.to_owned(),
                                value["kind"].as_str().map(str::to_owned),
                                ms,
                                cx,
                            );
                        } else {
                            this.dispatch(
                                Input::ActivationComplete {
                                    ticket: a.ticket,
                                    result: Ok(()),
                                },
                                cx,
                            );
                        }
                    }
                    Ok(value) => this.dispatch(
                        Input::ActivationComplete {
                            ticket: a.ticket,
                            result: Err(value["message"]
                                .as_str()
                                .unwrap_or("Action failed")
                                .into()),
                        },
                        cx,
                    ),
                    Err(e) => this.dispatch(
                        Input::ActivationComplete {
                            ticket: a.ticket,
                            result: Err(e),
                        },
                        cx,
                    ),
                }
                cx.notify();
            });
        }));
    }
    fn record(&mut self, a: &Activation, cx: &mut Context<Self>) {
        let adapter = self.adapter.clone();
        let req = Request::Record {
            action: json!(a.row.action),
            query: a.query.clone(),
            rank: a.rank,
            result_id: a.row.id.clone(),
        };
        self.writes.push(cx.spawn(async move |_, cx| {
            let _ = gpui_tokio::Tokio::spawn(cx, async move { adapter.send(req).await }).await;
        }));
    }
    fn message(&mut self, text: String, ms: u64, cx: &mut Context<Self>) {
        self.card_message(text, None, ms, cx);
    }
    fn card_message(
        &mut self,
        text: String,
        kind: Option<String>,
        ms: u64,
        cx: &mut Context<Self>,
    ) {
        self.notice = Some(text.clone());
        self.message_kind = kind;
        self.answer_revision += 1;
        let revision = self.answer_revision;
        self.message_document
            .update(cx, |view, cx| view.set_content(revision, &text, true, cx));
        self.notice_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(ms))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.notice = None;
                this.message_kind = None;
                this.answer_revision += 1;
                let revision = this.answer_revision;
                this.message_document
                    .update(cx, |view, cx| view.set_content(revision, "", true, cx));
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn navigate(&mut self, surface: Surface, payload: Payload, cx: &mut Context<Self>) {
        self.nonce += 1;
        let intent = Navigation {
            nonce: self.nonce,
            surface,
            payload,
        };
        self.navigation = Some(intent.clone());
        self.model.session.hide();
        self.model.session.reset_transport();
        self.timer = None;
        self.timer_at = None;
        self.sync_input(cx);
        let _ = self.relay.send(Event::Navigate(intent));
        cx.notify();
    }
    /// Foundation drag trackers can call these without knowing command-bar state.
    pub fn update_pointer_gesture(
        &mut self,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let Some(gesture) = self.resize {
            gesture.update(w, self.zoom, cx)
        } else {
            Ok(())
        }
    }
    pub fn end_pointer_gesture(&mut self, cx: &mut Context<Self>) {
        self.resize = None;
        let _ = self.relay.send(Event::SnapEnd);
        cx.notify();
    }
    pub fn receive_navigation(&mut self, intent: Navigation, cx: &mut Context<Self>) {
        self.navigation = Some(intent);
        cx.notify();
    }
    fn request_notice(&mut self, request: Request, cx: &mut Context<Self>) {
        let adapter = self.adapter.clone();
        let generation = self.model.session.generation;
        self.tasks.push(cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move { adapter.send(request).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| {
                if this.visible && generation == this.model.session.generation {
                    match result {
                        Ok(v) => {
                            this.message(v["message"].as_str().unwrap_or("Done").into(), 2500, cx)
                        }
                        Err(e) => {
                            this.error = Some(e);
                            cx.notify();
                        }
                    }
                }
            });
        }));
    }
    fn refresh_update_rows(&mut self, cx: &mut Context<Self>) {
        if launcher::input_mode(&self.model.input, self.model.terminal_mode).mode != Mode::Basic
            || self.model.session.pending()
        {
            cx.notify();
            return;
        }
        let selected = self
            .model
            .session
            .rows
            .get(self.model.session.selected)
            .map(|r| r.id.clone());
        self.model.session.rows.retain(|row|!matches!(&row.action,Action::InvokeCommand{command_id,..}if command_id=="check-for-updates"));
        let status = tezbar_ui_model::update_policy::effective(
            self.update_status.clone(),
            self.state.last_update_check,
            epoch_ms() as f64,
        );
        if let Some(row) = presentation::update_row(&status, self.checked_update, &self.model.input)
        {
            self.model.session.rows.insert(0, Arc::new(row));
        }
        if let Some(id) = selected {
            if let Some(index) = self.model.session.rows.iter().position(|r| r.id == id) {
                self.model.session.selected = index;
            }
        }
        self.rows = self.model.display_rows();
        if self.model.input.is_empty() {
            self.home = self.model.session.rows.clone();
        }
        cx.notify();
    }
    fn update_action(&mut self, cx: &mut Context<Self>) {
        if let Some(action) = presentation::update_action(&self.update_status) {
            if matches!(action, UpdateAction::Check) {
                self.checked_update = true;
            }
            let _ = self.relay.send(Event::Update(action));
        }
        cx.notify();
    }
    pub fn core_ready(&mut self, cx: &mut Context<Self>) {
        if self.ready && self.visible {
            self.form = None;
            self.form_activation = None;
            self.dispatch(Input::BackendChanged, cx);
            self.dispatch(Input::Show, cx);
            let query = self.input.read(cx).content.to_string();
            self.query(query, cx);
        }
    }
    pub fn state_write_error(&self) -> Option<String> {
        self.write_error.lock().unwrap().clone()
    }
    pub fn drain_state(&mut self) -> Vec<Task<()>> {
        std::mem::take(&mut self.writes)
    }
    fn core_event(&mut self, event: &CoreEvent, cx: &mut Context<Self>) {
        match event {
            CoreEvent::Invalidated { text } => {
                if let Some(form) = &self.form {
                    let index = form.args.selected;
                    let value = form.args.values[index].clone();
                    if text.as_ref().is_none_or(|t| t == &value) {
                        self.argument_changed(index, &value, cx);
                    }
                } else {
                    self.dispatch(Input::Invalidate { text: text.clone() }, cx);
                }
            }
            CoreEvent::OpenSurface(payload) => {
                self.nonce += 1;
                if let Some(intent) = Navigation::from_core(payload, self.nonce) {
                    self.navigate(intent.surface, intent.payload, cx);
                }
            }
            CoreEvent::Backend(value) => {
                let ready = value["status"] == "ready" || value.as_str() == Some("ready");
                if ready && self.visible {
                    self.ready = true;
                    self.dispatch(Input::BackendChanged, cx);
                }
            }
            CoreEvent::Other { channel, .. }
                if channel == "terminal:sessions-changed" || channel == "terminal:exit" =>
            {
                if self.visible && self.model.terminal_mode {
                    self.load_terminal(cx);
                }
            }
            CoreEvent::Other { channel, payload } if channel == "update-status" => {
                if let Ok(status) = serde_json::from_value(payload.clone()) {
                    self.update_status = status;
                    if self.checked_update
                        && !matches!(self.update_status, tezbar_update::AppUpdateStatus::Checking)
                    {
                        self.state.last_update_check = Some(epoch_ms() as f64);
                        self.persist(cx);
                    }
                    self.refresh_update_rows(cx);
                }
            }
            CoreEvent::Other { channel, payload }
                if channel == "run-extension-command-from-hotkey" =>
            {
                let adapter = self.adapter.clone();
                let request = Request::ExtensionRun(payload.clone());
                self.tasks.push(cx.spawn(async move |this, cx| {
                    let result =
                        gpui_tokio::Tokio::spawn(cx, async move { adapter.send(request).await })
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()));
                    let _ = this.update(cx, |this, cx| match result {
                        Ok(result) => this.navigate(
                            Surface::ExtensionRuntime,
                            Payload::Extension { result },
                            cx,
                        ),
                        Err(e) => {
                            this.error = Some(e);
                            cx.notify();
                        }
                    });
                }));
            }
            CoreEvent::Other { channel, payload } if self.answer_active => match channel.as_str() {
                "stream-token" => {
                    if let Some(token) = payload.as_str() {
                        if self.answer_text.len() + token.len() <= 1024 * 1024 {
                            self.answer_text.push_str(token);
                            self.update_answer(false, cx);
                        }
                    }
                }
                "stream-done" => {
                    self.answer_busy = false;
                    self.update_answer(true, cx);
                }
                "stream-error" => {
                    self.answer_busy = false;
                    self.error = Some(payload.as_str().unwrap_or("Answer failed").into());
                    cx.notify();
                }
                _ => {}
            },
            _ => {}
        }
    }
    fn update_answer(&mut self, settled: bool, cx: &mut Context<Self>) {
        self.answer_revision += 1;
        let rev = self.answer_revision;
        let text = self.answer_text.clone();
        self.answer
            .update(cx, |v, cx| v.set_content(rev, &text, settled, cx));
        cx.notify();
    }
    fn load_terminal(&mut self, cx: &mut Context<Self>) {
        let adapter = self.adapter.clone();
        let generation = self.model.session.generation;
        self.tasks.push(cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move {
                let prompt = adapter.send(Request::TerminalPrompt).await?;
                let rows = presentation::history_rows(
                    adapter.send(Request::TerminalSessions).await?,
                    true,
                )?;
                Ok::<_, String>((rows, prompt))
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| {
                if this.visible
                    && generation == this.model.session.generation
                    && this.model.terminal_mode
                {
                    match result {
                        Ok((mut rows, prompt)) => {
                            this.terminal_prompt = prompt["prompt"]
                                .as_str()
                                .or_else(|| prompt["cwd"].as_str())
                                .unwrap_or_default()
                                .into();
                            rows.sort_by_key(|r| {
                                this.state
                                    .terminal_pinned_sessions
                                    .iter()
                                    .position(|id| r.id == format!("terminal:{id}"))
                                    .unwrap_or(1000)
                            });
                            this.model.session.rows = rows.into_iter().map(Arc::new).collect();
                            this.model.session.displayed = Some(generation);
                            this.rows = this.model.session.rows.clone();
                        }
                        Err(e) => this.error = Some(e),
                    }
                    cx.notify();
                }
            });
        }));
    }
    fn prepare_icons(&mut self, cx: &mut Context<Self>) {
        self.icons.clear();
        self.icon_queue = self
            .rows
            .iter()
            .take(120)
            .filter_map(|r| {
                let source = if let Some(url) = &r.icon_data_url {
                    Some(IconSource::Data(url.clone()))
                } else {
                    match &r.action {
                        Action::OpenApp {
                            app_path: Some(path),
                            ..
                        } => Some(IconSource::Asset {
                            kind: "application".into(),
                            path: path.clone(),
                        }),
                        Action::OpenFile { path } | Action::OpenWithApp { path, .. } => {
                            Some(IconSource::Asset {
                                kind: "file".into(),
                                path: path.clone(),
                            })
                        }
                        Action::RunExtensionCommand { extra, .. } => extra
                            .get("iconPath")
                            .and_then(Value::as_str)
                            .map(|path| IconSource::Asset {
                                kind: "extension".into(),
                                path: path.into(),
                            }),
                        _ => None,
                    }
                };
                source.map(|source| (r.id.clone(), source))
            })
            .collect();
        self.pump_icons(cx);
    }
    fn pump_icons(&mut self, cx: &mut Context<Self>) {
        let generation = self.model.session.generation;
        while let Some((id, url)) = self.icon_queue.pop_front() {
            let key = {
                use std::hash::{Hash, Hasher};
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                url.hash(&mut hash);
                format!("{id}:{:016x}", hash.finish())
            };
            if let Some(image) = self.images.get(&key) {
                self.icons.insert(id, image);
                continue;
            }
            let Some(ticket) = self.images.request(&key) else {
                self.icon_queue.push_front((id, url));
                break;
            };
            let window = self.window;
            let adapter = self.adapter.clone();
            self.tasks.push(cx.spawn(async move |this, cx| {
                let source = match url {
                    IconSource::Data(url) => Some(url),
                    IconSource::Asset { kind, path } => gpui_tokio::Tokio::spawn(cx, async move {
                        adapter.send(Request::AssetIcon { kind, path }).await
                    })
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .and_then(|v| v.as_str().map(str::to_owned)),
                };
                let data = cx
                    .background_executor()
                    .spawn(async move {
                        source
                            .and_then(|url| {
                                url.strip_prefix("data:image/png;base64,")
                                    .and_then(decode_base64)
                            })
                            .and_then(|bytes| tezbar_ui::image_cache::decode_png(&bytes).ok())
                    })
                    .await;
                let _ = window.update(cx, |_, w, cx| {
                    this.update(cx, |this, cx| {
                        this.images.complete(ticket, data, w, cx);
                        if generation == this.model.session.generation {
                            if let Some(image) = this.images.get(&key) {
                                this.icons.insert(id, image);
                            }
                        }
                        this.pump_icons(cx);
                        cx.notify();
                    })
                });
            }));
        }
    }
}
fn decode_base64(text: &str) -> Option<Vec<u8>> {
    if text.len() > 2 * 1024 * 1024 * 4 / 3 + 4 {
        return None;
    }
    let mut bytes = Vec::with_capacity(text.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in text.bytes() {
        let n = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => return None,
        };
        acc = (acc << 6) | n as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((acc >> bits) as u8);
        }
    }
    Some(bytes)
}

fn epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
