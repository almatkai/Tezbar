use crate::{
    search_session::SearchSession,
    text_input::{Committed, TextInput},
    theme,
};
use app_lib::launcher_facade::{Activation, Launcher, Row, Support};
use gpui::prelude::*;
use gpui::*;
use gpui_tokio::Tokio;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};
actions!(launcher, [Quit]);
pub struct LauncherView {
    core: Launcher,
    input: Entity<TextInput>,
    session: SearchSession<Row>,
    scroll: UniformListScrollHandle,
    ready: bool,
    error: Option<String>,
    message: Option<String>,
    message_error: bool,
    visible: bool,
    started: Instant,
    committed: Instant,
    painted_generation: Option<u64>,
    search_ms: f64,
    echo_ms: f64,
    results_ms: f64,
    frames: u64,
    enter_timer: Option<Task<()>>,
    activation_started: Option<Instant>,
    activation_title: Option<String>,
    activation_ms: f64,
    search_task: Option<Task<()>>,
    pump: Option<Task<()>>,
    idle: Option<Task<()>>,
    activation: Option<Task<()>>,
    notice: Option<Task<()>>,
    icons_task: Option<Task<()>>,
    icons: HashMap<String, Option<Arc<RenderImage>>>,
    lru: VecDeque<String>,
    _subscriptions: Vec<Subscription>,
    last_pointer: Option<Point<Pixels>>,
}
impl LauncherView {
    pub fn new(core: Launcher, show: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(TextInput::new);
        input.focus_handle(cx).focus(window, cx);
        let subscription = cx.subscribe_in(&input, window, |this, _, event: &Committed, w, cx| {
            this.commit(event.0.clone(), w, cx)
        });
        let active = cx.observe_window_activation(window, |this, w, cx| {
            if this.visible && !w.is_window_active() {
                this.hide(w, cx);
            }
        });
        let appearance = cx.observe_window_appearance(window, |_, _, cx| cx.notify());
        let quitting = core.clone();
        let quit = cx.on_app_quit(move |_, _| {
            let core = quitting.clone();
            async move {
                let _ = tokio::time::timeout(Duration::from_secs(5), core.shutdown()).await;
            }
        });
        let mut view = Self {
            core: core.clone(),
            input,
            session: SearchSession::default(),
            scroll: UniformListScrollHandle::new(),
            ready: false,
            error: None,
            message: None,
            message_error: false,
            visible: show,
            started: Instant::now(),
            committed: Instant::now(),
            painted_generation: None,
            search_ms: 0.,
            echo_ms: 0.,
            results_ms: 0.,
            frames: 0,
            enter_timer: None,
            activation_started: None,
            activation_title: None,
            activation_ms: 0.,
            search_task: None,
            pump: None,
            idle: None,
            activation: None,
            notice: None,
            icons_task: None,
            icons: HashMap::new(),
            lru: VecDeque::new(),
            _subscriptions: vec![subscription, active, appearance, quit],
            last_pointer: Some(window.mouse_position()),
        };
        let ready = Tokio::spawn(cx, async move { core.ready().await });
        view.search_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = ready.await.map_err(|e| e.to_string()).and_then(|r| r);
            let _ = this.update_in(cx, |this, w, cx| match result {
                Ok(()) => {
                    this.ready = true;
                    // The field may contain IME preedit. Search only the last
                    // committed generation, preserving a pending Enter ticket.
                    if this.session.generation == 0 {
                        this.commit(String::new(), w, cx);
                    } else {
                        this.schedule(w, cx);
                    }
                }
                Err(error) => {
                    this.error = Some(format!("Search couldn't start: {error}"));
                    cx.notify();
                }
            });
        }));
        view
    }
    fn now(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
    fn commit(&mut self, text: String, w: &mut Window, cx: &mut Context<Self>) {
        self.session.commit(text, self.now());
        self.committed = Instant::now();
        self.message = None;
        self.message_error = false;
        if self.ready {
            self.error = None;
        }
        self.enter_timer = None;
        let g = self.session.generation;
        let entity = cx.entity();
        let start = self.committed;
        w.on_next_frame(move |w, _cx| {
            w.on_next_frame(move |_, cx| {
                entity.update(cx, |this, _| {
                    if g == this.session.generation {
                        this.echo_ms = start.elapsed().as_secs_f64() * 1000.;
                        this.frames += 1;
                    }
                })
            });
        });
        self.schedule(w, cx);
        cx.notify();
    }
    fn schedule(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if !self.ready {
            return;
        }
        if self.pump.is_some() {
            return;
        }
        let Some(delay) = self.session.request_delay_ms(self.now()) else {
            return;
        };
        self.pump = Some(cx.spawn_in(w, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(delay))
                .await;
            let _ = this.update_in(cx, |this, w, cx| {
                this.pump = None;
                this.request(w, cx);
            });
        }));
    }
    fn request(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some((g, query)) = self.session.take_request(self.now()) else {
            return;
        };
        let core = self.core.clone();
        let task = Tokio::spawn(cx, async move {
            let started = Instant::now();
            let response = core.search(&query).await;
            (response, started.elapsed().as_secs_f64() * 1000.)
        });
        self.search_task = Some(cx.spawn_in(w, async move |this, cx| {
            let result = task
                .await
                .map_err(|e| e.to_string())
                .and_then(|(response, ms)| response.map(|rows| (rows, ms)));
            let ms = result.as_ref().map(|(_, ms)| *ms).unwrap_or(0.);
            let _ = this.update_in(cx, |this, w, cx| {
                let now = this.now();
                let applied = match result {
                    Ok((rows, _)) => this.session.complete(g, rows, now),
                    Err(error) => {
                        let applied = this.session.fail(g, now);
                        if applied {
                            this.error = Some(format!("Search failed: {error}"));
                        }
                        applied
                    }
                };
                if applied {
                    this.search_ms = ms;
                    this.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
                    this.prepare_icons_after_frame(w, cx);
                    let entity = cx.entity();
                    let committed = this.committed;
                    w.on_next_frame(move |w, _cx| {
                        w.on_next_frame(move |_, cx| {
                            entity.update(cx, |this, _| {
                                if g == this.session.generation && this.session.displayed == Some(g)
                                {
                                    this.painted_generation = Some(g);
                                    this.results_ms = committed.elapsed().as_secs_f64() * 1000.;
                                    this.frames += 1;
                                }
                            })
                        })
                    });
                    if this.session.deferred_enter(now) {
                        this.activate(w, cx);
                    }
                }
                if this.session.pending() {
                    this.schedule(w, cx);
                }
                cx.notify();
            });
        }));
        // Notify only once at the delayed waiting threshold; no polling timer.
        self.notice = Some(cx.spawn_in(w, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;
            let _ = this.update_in(cx, |_, _, cx| cx.notify());
        }));
    }
    pub fn show(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.idle = None;
        self.visible = true;
        w.activate_window();
        cx.activate(true);
        self.input.focus_handle(cx).focus(w, cx);
        self.input.update(cx, |input, cx| input.set_text("", w, cx));
        let entity = cx.entity();
        w.on_next_frame(move |w, _cx| {
            w.on_next_frame(move |_, cx| entity.update(cx, |this, _| this.frames += 1))
        });
        cx.notify();
    }
    pub fn hide(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if !self.visible {
            return;
        }
        self.visible = false;
        self.session.hide();
        self.input
            .update(cx, |input, cx| input.clear_silently(w, cx));
        self.pump = None;
        self.notice = None;
        self.enter_timer = None;
        self.icons_task = None;
        self.icons.clear();
        self.lru.clear();
        cx.hide();
        let core = self.core.clone();
        self.idle = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(5)).await;
            if this.read_with(cx, |this, _| !this.visible).unwrap_or(false) {
                let _ = Tokio::spawn(cx, async move {
                    core.trim_idle_memory().await;
                })
                .await;
            }
        }));
        cx.notify();
    }
    fn enter(&mut self, repeat: bool, w: &mut Window, cx: &mut Context<Self>) {
        if self.session.enter(self.now(), repeat) {
            self.activate(w, cx);
        } else if self.session.pending() && !repeat {
            self.enter_timer = Some(cx.spawn_in(w, async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let _ = this.update_in(cx, |this, _, cx| {
                    if this.session.expire_enter(this.now()) {
                        this.message = Some("Still searching — press Enter again".into());
                        cx.notify();
                    }
                });
            }));
        }
    }
    fn activate(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.message = None;
        self.message_error = false;
        let generation = self.session.generation;
        let Some(row) = self.session.rows.get(self.session.selected) else {
            self.session.activation_finished(generation);
            return;
        };
        if row.support == Support::Unsupported {
            self.message = Some(format!("{} isn't available in the prototype", row.title));
            self.session.activation_finished(generation);
            self.notice = Some(cx.spawn_in(w, async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                let _ = this.update_in(cx, |this, _, cx| {
                    this.message = None;
                    cx.notify();
                });
            }));
            cx.notify();
            return;
        }
        let title = row.title.clone();
        self.activation_title = Some(title.clone());
        let ticket = row.ticket(&self.session.query, self.session.selected + 1);
        let core = self.core.clone();
        let task = Tokio::spawn(cx, async move { core.activate(ticket).await });
        let started = Instant::now();
        self.activation_started = Some(started);
        self.activation = Some(cx.spawn_in(w, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, w, cx| {
                this.activation_ms = started.elapsed().as_secs_f64() * 1000.;
                this.activation_started = None;
                this.activation_title = None;
                let current = this.session.activation_finished(generation);
                if current {
                    match result {
                        Ok(Activation::Done) => this.hide(w, cx),
                        Ok(Activation::Failed(error)) => {
                            this.message_error = true;
                            this.message = Some(format!("Couldn't open {title}: {error}"));
                        }
                        Ok(Activation::Unsupported) => {
                            this.message =
                                Some(format!("{title} isn't available in the prototype"));
                        }
                        Err(error) => {
                            this.message_error = true;
                            this.message = Some(format!("Couldn't open {title}: {error}"));
                        }
                    }
                };
                eprintln!(
                    "activation_response_ms={:.3}",
                    started.elapsed().as_secs_f64() * 1000.
                );
                cx.notify();
            });
        }));
        self.notice = Some(cx.spawn_in(w, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;
            let _ = this.update_in(cx, |_, _, cx| cx.notify());
        }));
        cx.notify();
    }
    fn prepare_icons_after_frame(&self, w: &Window, cx: &Context<Self>) {
        let weak = cx.weak_entity();
        w.on_next_frame(move |w, cx| {
            let _ = weak.update(cx, |this, cx| this.prepare_icons(w, cx));
        });
    }
    fn prepare_icons(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if !self.visible {
            return;
        }
        let top = (-self.scroll.0.borrow().base_handle.offset().y / px(36.)).max(0.) as usize;
        for icon in self
            .session
            .rows
            .iter()
            .skip(top)
            .take(18)
            .filter_map(|r| r.icon.as_ref())
        {
            let key = icon.key();
            if self.icons.contains_key(&key) {
                self.lru.retain(|k| k != &key);
                self.lru.push_back(key);
            }
        }
        let icons: Vec<_> = self
            .session
            .rows
            .iter()
            .skip(top)
            .take(18)
            .filter_map(|r| r.icon.clone())
            .filter(|i| !self.icons.contains_key(&i.key()))
            .collect();
        if icons.is_empty() {
            return;
        }
        let core = self.core.clone();
        let g = self.session.displayed;
        let task = Tokio::spawn(cx, async move {
            let mut decoded = vec![];
            for icon in icons {
                let image = core
                    .icon(&icon)
                    .await
                    .and_then(|png| image::load_from_memory(&png).ok())
                    .map(|i| Arc::new(RenderImage::new(vec![image::Frame::new(i.to_rgba8())])));
                decoded.push((icon.key(), image));
            }
            decoded
        });
        self.icons_task = Some(cx.spawn_in(w, async move |this, cx| {
            if let Ok(decoded) = task.await {
                let _ = this.update_in(cx, |this, _, cx| {
                    if !this.visible || this.session.displayed != g {
                        return;
                    }
                    for (key, image) in decoded {
                        this.lru.retain(|k| k != &key);
                        this.lru.push_back(key.clone());
                        this.icons.insert(key, image);
                        while this.lru.len() > 128 {
                            if let Some(old) = this.lru.pop_front() {
                                this.icons.remove(&old);
                            }
                        }
                    }
                    cx.notify();
                });
            }
        }));
    }
    pub fn control(
        &mut self,
        request: Value,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Value, String> {
        match request["op"].as_str().unwrap_or("") {
            "status" | "ready" => Ok(
                json!({"ready":self.ready,"generation":self.session.generation,"displayed":self.session.displayed,"painted_generation":self.painted_generation,"pending":self.session.pending(),"visible":self.visible,"result_count":self.session.rows.len(),"rows":self.session.rows.iter().map(|r|json!({"id":r.id,"category":r.category,"supported":r.support==Support::Native})).collect::<Vec<_>>(),"selected":self.session.selected,"stale_dropped":self.session.stale_dropped,"activating":self.session.activating,"frames":self.frames,"input_echo_ms":self.echo_ms,"input_results_ms":self.results_ms,"core_search_ms":self.search_ms,"activation_ms":self.activation_ms,"error":self.error}),
            ),
            "show" => {
                self.show(w, cx);
                Ok(json!(null))
            }
            "hide" => {
                self.hide(w, cx);
                Ok(json!(null))
            }
            "query" | "type" => {
                let text = request["text"].as_str().ok_or("missing text")?;
                self.input.update(cx, |input, cx| {
                    if request["op"] == "query" {
                        input.set_text(text, w, cx)
                    } else {
                        input.replace_text_in_range(None, text, w, cx)
                    }
                });
                Ok(json!({"expected_generation":self.session.generation+1}))
            }
            "select" => {
                let index = request["index"].as_u64().ok_or("missing index")? as usize;
                if index >= self.session.rows.len() {
                    return Err("row out of range".into());
                }
                self.session.selected = index;
                cx.notify();
                Ok(json!(null))
            }
            "activation" => {
                self.enter(false, w, cx);
                Ok(json!(null))
            }
            "quit" => {
                cx.quit();
                Ok(json!(null))
            }
            _ => Err("unknown prototype operation".into()),
        }
    }
    fn key(&mut self, event: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        if self.input.read(cx).marked_range.is_some() {
            if event.keystroke.key == "escape" {
                self.input
                    .update(cx, |input, cx| input.cancel_composition(w, cx));
                cx.stop_propagation();
            }
            return;
        }
        match event.keystroke.key.as_str() {
            "up" | "down" => {
                self.session
                    .move_selection(if event.keystroke.key == "up" { -1 } else { 1 });
                self.scroll
                    .scroll_to_item(self.session.selected, ScrollStrategy::Top);
                self.prepare_icons_after_frame(w, cx);
                cx.stop_propagation();
                cx.notify();
            }
            "enter" => {
                self.enter(event.is_held, w, cx);
                cx.stop_propagation();
            }
            "escape" => {
                self.hide(w, cx);
                cx.stop_propagation();
            }
            _ => {}
        }
    }
}
fn category(category: &str) -> &'static str {
    match category {
        "applications" => "Application",
        "files" => "File",
        "knowledge" => "Document",
        "quick-notes" => "Note",
        "vault" => "Vault",
        "extensions" => "Extension",
        "commands" | "native-commands" => "Command",
        "snippets" => "Snippet",
        "quick-links" => "Quicklink",
        "clipboard" => "Clipboard",
        "store" => "Store",
        _ => "Command",
    }
}
fn glyph(category: &str) -> &'static str {
    match category {
        "applications" => "app.svg",
        "files" | "knowledge" => "file.svg",
        "quick-notes" => "note.svg",
        "vault" => "key.svg",
        "extensions" => "puzzle.svg",
        "snippets" => "snippet.svg",
        "quick-links" => "link.svg",
        _ => "command.svg",
    }
}
impl Render for LauncherView {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(w);
        let message = if let Some(m) = &self.message {
            m.clone()
        } else if !self.ready && self.error.is_some() {
            String::new()
        } else if !self.ready {
            "Starting search…".into()
        } else if self.session.activating {
            if self
                .activation_started
                .is_some_and(|t| t.elapsed() > Duration::from_millis(150))
            {
                format!(
                    "Opening {}…",
                    self.activation_title.as_deref().unwrap_or("")
                )
            } else {
                String::new()
            }
        } else if self.session.pending() && self.committed.elapsed() > Duration::from_millis(150) {
            "Searching…".into()
        } else {
            let n = self.session.rows.len();
            format!("{n} {}", if n == 1 { "result" } else { "results" })
        };
        let list = if let Some(error) = self.error.clone() {
            div()
                .p(px(16.))
                .text_size(px(13.))
                .text_color(t.danger)
                .child(error)
                .into_any_element()
        } else if self.session.rows.is_empty() && self.ready && !self.session.pending() {
            div()
                .p(px(16.))
                .text_size(px(13.))
                .text_color(t.text3)
                .child(if self.session.query.is_empty() {
                    "Type to search apps, files, notes and commands.".into()
                } else {
                    format!("No results for “{}”", self.session.query)
                })
                .into_any_element()
        } else {
            uniform_list(
                "results",
                self.session.rows.len(),
                cx.processor(|this, range: std::ops::Range<usize>, w, cx| {
                    let t = theme::tokens(w);
                    range
                        .map(|ix| {
                            let row = &this.session.rows[ix];
                            let selected = ix == this.session.selected;
                            let support = row.support == Support::Native;
                            let icon = row
                                .icon
                                .as_ref()
                                .and_then(|i| this.icons.get(&i.key()))
                                .and_then(|v| v.clone());
                            let icon = if let Some(image) = icon {
                                img(image).size(px(22.)).rounded(px(6.)).into_any_element()
                            } else {
                                div()
                                    .size(px(22.))
                                    .rounded(px(6.))
                                    .bg(t.pill_strong)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        svg().path(glyph(&row.category)).size(px(14.)).text_color(
                                            if selected { white().into() } else { t.text2 },
                                        ),
                                    )
                                    .into_any_element()
                            };
                            div()
                                .id(SharedString::from(row.id.clone()))
                                .w_full()
                                .h(px(36.))
                                .px(px(8.))
                                .rounded(px(8.))
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .when(selected, |d| d.bg(rgb(0x4f52d9)))
                                .on_mouse_move(cx.listener(
                                    move |this, event: &MouseMoveEvent, _, cx| {
                                        if this.last_pointer != Some(event.position) {
                                            this.last_pointer = Some(event.position);
                                            this.session.selected = ix;
                                            cx.notify();
                                        }
                                    },
                                ))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _, w, cx| {
                                        if this.session.click(ix) {
                                            this.activate(w, cx);
                                        }
                                    }),
                                )
                                .child(div().flex_shrink_0().child(icon))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .items_baseline()
                                        .gap(px(8.))
                                        .overflow_hidden()
                                        .child(
                                            div()
                                                .max_w(relative(0.7))
                                                .text_size(px(13.))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(if selected {
                                                    white().into()
                                                } else {
                                                    t.text1
                                                })
                                                .truncate()
                                                .child(row.title.clone()),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .text_size(px(12.))
                                                .text_color(if selected {
                                                    rgba(0xffffffb8).into()
                                                } else {
                                                    t.text4
                                                })
                                                .truncate()
                                                .child(row.subtitle.clone()),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .px(px(6.))
                                        .py(px(2.))
                                        .rounded(px(5.))
                                        .text_size(px(10.5))
                                        .line_height(px(14.))
                                        .text_color(if selected {
                                            white().into()
                                        } else if support {
                                            t.text3
                                        } else {
                                            t.text4
                                        })
                                        .when(support, |d| {
                                            d.bg(if selected {
                                                rgba(0xffffff29).into()
                                            } else {
                                                t.pill
                                            })
                                        })
                                        .child(if support {
                                            category(&row.category)
                                        } else {
                                            "Not in prototype"
                                        }),
                                )
                        })
                        .collect()
                }),
            )
            .track_scroll(&self.scroll)
            .h_full()
            .on_scroll(cx.listener(|_this, _, w, cx| {
                cx.defer_in(w, |this, w, cx| this.prepare_icons(w, cx))
            }))
            .into_any_element()
        };
        let supported = self
            .session
            .rows
            .get(self.session.selected)
            .is_some_and(|r| r.support == Support::Native);
        let hint = |label: &str, key: &str| {
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .child(label.to_owned())
                .child(
                    div()
                        .h(px(16.))
                        .px(px(5.))
                        .rounded(px(5.))
                        .bg(t.pill_strong)
                        .text_size(px(10.))
                        .text_color(t.text2)
                        .child(key.to_owned()),
                )
        };
        div()
            .size_full()
            .rounded(px(18.))
            .overflow_hidden()
            .bg(t.sheet)
            .border_1()
            .border_color(t.rim)
            .font_family(
                if cx
                    .text_system()
                    .all_font_names()
                    .iter()
                    .any(|n| n == "Geist")
                {
                    "Geist"
                } else if cfg!(target_os = "macos") {
                    ".SystemUIFont"
                } else {
                    "Segoe UI"
                },
            )
            .text_color(t.text1)
            .flex()
            .flex_col()
            .on_key_down(cx.listener(Self::key))
            .on_action(cx.listener(|_, _: &Quit, _, cx| cx.quit()))
            .child(
                div()
                    .h(px(43.))
                    .flex_shrink_0()
                    .px(px(13.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .border_b_1()
                    .border_color(t.hairline)
                    .child(
                        div()
                            .size(px(24.))
                            .rounded(px(7.))
                            .bg(rgb(0x5b5ef0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(svg().path("search.svg").size(px(14.)).text_color(white())),
                    )
                    .child(self.input.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .px(px(5.))
                    .pt(px(6.))
                    .pb(px(6.))
                    .child(list),
            )
            .child(
                div()
                    .h(px(32.))
                    .flex_shrink_0()
                    .pl(px(7.))
                    .pr(px(11.))
                    .border_t_1()
                    .border_color(t.hairline)
                    .bg(t.bar)
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(11.))
                    .text_color(t.text3)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(if self.message_error {
                                t.danger
                            } else {
                                t.text3
                            })
                            .child(message),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(12.))
                            .when(supported, |d| d.child(hint("Open", "↵")))
                            .child(hint("Hide", "Esc")),
                    ),
            )
    }
}
