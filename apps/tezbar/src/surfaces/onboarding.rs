//! Native onboarding. Boot admission remains the app's coreReady responsibility.
use super::settings::{SettingsContent, SettingsRequest, SettingsStore, Tab};
use gpui::{prelude::*, *};
use serde_json::{Value, json};
use tezbar_ui::{
    controls::{self, ButtonTone},
    text_input::{Committed, FieldOptions, TextInput},
    tokens,
};
use tezbar_ui_model::{settings::Edit, ui_state::UiState};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Shortcut,
    Practice,
    Ai,
    Knowledge,
    Permissions,
}
impl Step {
    pub fn title(self) -> &'static str {
        match self {
            Self::Shortcut => "Shortcut",
            Self::Practice => "Try the bar",
            Self::Ai => "AI model",
            Self::Knowledge => "DeepSearch",
            Self::Permissions => "Permissions",
        }
    }
}
pub enum OnboardingEffect {
    Finished,
    Settings(super::settings::SettingsEffect),
}
impl EventEmitter<OnboardingEffect> for Onboarding {}
pub struct Onboarding {
    pub content: Entity<SettingsContent>,
    pub step: usize,
    pub steps: Vec<Step>,
    pub error: Option<String>,
    store: SettingsStore,
    owner: u64,
    practice: Entity<TextInput>,
    result: String,
    completions: Vec<Value>,
    completion_selected: usize,
    practice_generation: u64,
    practice_load: Option<Task<()>>,
    practice_copy: Option<Task<()>>,
    practice_terminal: bool,
    practice_cwd: Option<String>,
    done: std::collections::BTreeSet<&'static str>,
    hotkey: String,
    initial_config: Value,
    rail_focus: Vec<FocusHandle>,
    recording: bool,
    dirty_hotkey: bool,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
    finish: Option<Task<()>>,
    load: Option<Task<()>>,
    shortcut_save: Option<Task<()>>,
    pub finishing: bool,
    completed: bool,
    splash: bool,
    icon: std::sync::Arc<Image>,
}
impl Onboarding {
    pub fn new(
        store: SettingsStore,
        state: &UiState,
        macos: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let content = cx.new(|cx| {
            let mut v = SettingsContent::new(store.clone(), state, w, cx);
            v.embedded = true;
            v.tab = Tab::General;
            v
        });
        let practice = cx.new(|cx| {
            let mut v = TextInput::new_native(cx);
            v.configure(FieldOptions {
                name: "Try the command bar".into(),
                placeholder: "= 12 * 8".into(),
                ..Default::default()
            });
            v
        });
        let sub = cx.subscribe(&practice, |this, _, e: &Committed, cx| {
            this.practice_committed(&e.0, cx);
            cx.notify()
        });
        let relay = cx.subscribe(&content, |_, _, e: &super::settings::SettingsEffect, cx| {
            cx.emit(OnboardingEffect::Settings(e.clone()))
        });
        let summaries = cx.observe(&content, |_, _, cx| cx.notify());
        let mut steps = vec![Step::Shortcut, Step::Practice, Step::Ai, Step::Knowledge];
        if macos {
            steps.push(Step::Permissions)
        }
        let rail_focus = (0..steps.len()).map(|_| cx.focus_handle()).collect();
        Self {
            owner: store.owner(),
            store,
            content,
            steps,
            step: 0,
            error: None,
            practice,
            result: String::new(),
            completions: vec![],
            completion_selected: 0,
            practice_generation: 0,
            practice_load: None,
            practice_copy: None,
            practice_terminal: false,
            practice_cwd: None,
            done: Default::default(),
            initial_config: Value::Null,
            rail_focus,
            hotkey: if cfg!(windows) {
                "Control+Space"
            } else {
                "Alt+Space"
            }
            .into(),
            recording: false,
            dirty_hotkey: false,
            focus: cx.focus_handle(),
            _subscriptions: vec![sub, relay, summaries],
            finish: None,
            load: None,
            shortcut_save: None,
            finishing: false,
            completed: false,
            splash: true,
            icon: std::sync::Arc::new(Image::from_bytes(
                ImageFormat::Png,
                include_bytes!("../../../../resources/app-icons/app-icon.png").to_vec(),
            )),
        }
    }
    pub fn activate(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.splash {
            w.focus(&self.focus, cx);
        }
        if self.load.is_some() {
            return;
        }
        let transport = self.store.transport();
        self.load = Some(cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, transport.durable_config()).await;
            let _ = this.update(cx, |v, cx| {
                if let Ok(Ok(config)) = result {
                    if !v.dirty_hotkey {
                        if let Some(hotkey) = config["raymesHotkey"].as_str() {
                            v.hotkey = hotkey.into()
                        }
                    }
                    v.initial_config = config;
                }
                cx.notify()
            });
        }));
    }
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if !visible {
            self.practice_generation += 1;
            self.practice_load = None;
        }
        let content_visible = visible
            && matches!(
                self.steps[self.step],
                Step::Ai | Step::Knowledge | Step::Permissions
            );
        self.content
            .update(cx, |view, cx| view.set_visible(content_visible, cx));
    }
    pub fn go(&mut self, index: usize, w: &mut Window, cx: &mut Context<Self>) {
        self.splash = false;
        self.step = index.min(self.steps.len() - 1);
        self.practice_generation += 1;
        self.practice_load = None;
        self.recording = false;
        let tab = match self.steps[self.step] {
            Step::Ai => Some(Tab::Ai),
            Step::Knowledge => Some(Tab::Knowledge),
            Step::Permissions => Some(Tab::Permissions),
            _ => None,
        };
        if let Some(tab) = tab {
            self.content.update(cx, |v, cx| v.activate_tab(tab, w, cx))
        } else {
            self.content.update(cx, |v, cx| v.set_visible(false, cx));
        }
        cx.notify();
    }
    pub fn finish(&mut self, cx: &mut Context<Self>) {
        if self.finishing || self.completed {
            return;
        }
        self.finishing = true;
        self.error = None;
        self.store.enqueue(
            self.owner,
            vec![Edit::Global {
                field: "hasCompletedOnboarding".into(),
                value: json!(true),
            }],
        );
        let store = self.store.clone();
        let drain = store.drain_owner(self.owner);
        self.finish = Some(cx.spawn(async move |this, cx| {
            let r = gpui_tokio::Tokio::spawn(cx, drain).await;
            let _ = this.update(cx, |v, cx| {
                v.finishing = false;
                if matches!(r, Ok(Ok(()))) {
                    v.completed = true;
                    cx.emit(OnboardingEffect::Finished)
                } else {
                    v.error = Some("Could not finish saving. Retry to complete onboarding.".into())
                }
                cx.notify()
            });
        }));
    }
    pub fn retry_finish(&mut self, cx: &mut Context<Self>) {
        self.store.retry();
        self.finish(cx)
    }
    fn summary(&self, step: Step, cx: &App) -> String {
        let content = self.content.read(cx);
        match step {
            Step::Shortcut => self.hotkey.clone(),
            Step::Practice => format!(
                "{} of 4 tried",
                self.done.iter().filter(|s| **s != "shortcut").count()
            ),
            Step::Ai => content
                .onboarding_connection()
                .map(|(provider, model)| format!("{provider} · {model}"))
                .unwrap_or_else(|| "Not connected".into()),
            Step::Knowledge => content
                .knowledge
                .snapshot
                .as_ref()
                .map(|s| {
                    format!(
                        "{} folders · {}",
                        s.roots.iter().filter(|r| r["enabled"] == true).count(),
                        s.status["state"].as_str().unwrap_or("Ready")
                    )
                })
                .unwrap_or_else(|| "Choose folders".into()),
            Step::Permissions => content
                .permissions
                .snapshot
                .as_ref()
                .map(|s| {
                    format!(
                        "{} of {} granted",
                        s.statuses
                            .iter()
                            .filter(
                                |p| p.state == tezbar_ui_model::utilities::PermissionState::Granted
                            )
                            .count(),
                        s.statuses.len()
                    )
                })
                .unwrap_or_else(|| "Review access".into()),
        }
    }
    fn practice_changed(&mut self, q: &str) {
        if q.starts_with('>') || q.ends_with('>') && q.starts_with('/') {
            self.practice_terminal = true;
            self.practice_cwd = q
                .strip_suffix('>')
                .filter(|p| p.starts_with('/'))
                .map(str::to_owned);
        }
        let mode = tezbar_ui_model::launcher::input_mode(q, self.practice_terminal);
        self.result = match mode.mode {
            tezbar_ui_model::launcher::Mode::Terminal => {
                self.done.insert("terminal");
                format!(
                    "Terminal mode · {}",
                    self.practice_cwd
                        .as_deref()
                        .unwrap_or("Type a command in the launcher")
                )
            }
            tezbar_ui_model::launcher::Mode::Ai => {
                self.done.insert("ai");
                "AI mode · Ask a question using your selected provider".into()
            }
            tezbar_ui_model::launcher::Mode::Path => {
                "Folder mode · Type a folder path to browse files".into()
            }
            _ => {
                if let Some(r) = tezbar_core::instant::calculator::evaluate_expression(
                    q.strip_prefix('=').unwrap_or(q).trim(),
                ) {
                    self.done.insert("math");
                    r.formatted
                } else {
                    "Use = for math, / for folders, > for Terminal, or Space for AI".into()
                }
            }
        };
    }
    fn practice_committed(&mut self, q: &str, cx: &mut Context<Self>) {
        self.practice_changed(q);
        self.practice_generation += 1;
        let generation = self.practice_generation;
        self.practice_load = None;
        self.completions.clear();
        self.completion_selected = 0;
        let mode = tezbar_ui_model::launcher::input_mode(q, self.practice_terminal);
        if mode.is_slash_input && mode.mode != tezbar_ui_model::launcher::Mode::Ai {
            let query = mode.slash_query;
            let transport = self.store.transport();
            self.practice_load = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(90))
                    .await;
                let reply = gpui_tokio::Tokio::spawn(
                    cx,
                    transport.send(SettingsRequest::CompletePath(query.clone())),
                )
                .await;
                let _ = this.update(cx, |v, cx| {
                    if v.practice_generation != generation || v.steps[v.step] != Step::Practice {
                        return;
                    }
                    v.completions = reply
                        .ok()
                        .and_then(Result::ok)
                        .and_then(|v| v.as_array().cloned())
                        .unwrap_or_default();
                    if !v.completions.is_empty() && query.len() > 1 {
                        v.done.insert("folder");
                    }
                    if v.completions.is_empty() {
                        v.result = "No folder matches that path.".into();
                    }
                    cx.notify()
                });
            }));
        }
        cx.notify();
    }
    fn complete_path(&mut self, index: usize, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(row) = self.completions.get(index) {
            if row["kind"] == "directory" {
                if let Some(value) = row["value"].as_str() {
                    let value = value.to_owned();
                    self.practice.update(cx, |v, cx| v.set_text(&value, w, cx));
                }
            }
        }
        self.practice.focus_handle(cx).focus(w, cx);
    }
    fn key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        if e.is_held {
            return;
        }
        if self.splash && !self.recording {
            self.splash = false;
            w.focus(&self.focus, cx);
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if e.keystroke.key == "enter"
            && (e.keystroke.modifiers.platform || e.keystroke.modifiers.control)
        {
            if self.step + 1 == self.steps.len() {
                self.finish(cx);
            } else {
                self.go(self.step + 1, w, cx);
            }
            cx.stop_propagation();
            return;
        }
        if self.rail_focus.iter().any(|f| f.is_focused(w))
            && matches!(e.keystroke.key.as_str(), "left" | "right" | "home" | "end")
        {
            let index = match e.keystroke.key.as_str() {
                "left" => (self.step + self.steps.len() - 1) % self.steps.len(),
                "right" => (self.step + 1) % self.steps.len(),
                "home" => 0,
                _ => self.steps.len() - 1,
            };
            self.go(index, w, cx);
            w.focus(&self.rail_focus[index], cx);
            cx.stop_propagation();
            return;
        }
        if self.steps[self.step] == Step::Practice
            && self.practice.focus_handle(cx).is_focused(w)
            && self.practice.read(cx).marked_range.is_none()
        {
            let key = e.keystroke.key.as_str();
            if key == "escape" {
                self.practice_terminal = false;
                self.practice_cwd = None;
                self.practice.update(cx, |v, cx| v.set_text("", w, cx));
                cx.stop_propagation();
                return;
            }
            if key == "backspace"
                && self.practice_terminal
                && self.practice.read(cx).content.is_empty()
            {
                self.practice_terminal = false;
                let value = self.practice_cwd.take().unwrap_or_default();
                self.practice.update(cx, |v, cx| v.set_text(&value, w, cx));
                cx.stop_propagation();
                return;
            }
            if !self.completions.is_empty() {
                if key == "down" || key == "up" {
                    self.completion_selected = if key == "down" {
                        (self.completion_selected + 1) % self.completions.len()
                    } else {
                        (self.completion_selected + self.completions.len() - 1)
                            % self.completions.len()
                    };
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if key == "enter" || key == "tab" {
                    self.complete_path(self.completion_selected, w, cx);
                    cx.stop_propagation();
                    return;
                }
            } else if key == "enter" {
                let q = self.practice.read(cx).content.to_string();
                if let Some(calc) = tezbar_core::instant::calculator::evaluate_expression(
                    q.strip_prefix('=').unwrap_or(&q).trim(),
                ) {
                    if let Ok(id) = self
                        .store
                        .admit(SettingsRequest::PracticeCopy(calc.clipboard))
                    {
                        let store = self.store.clone();
                        self.practice_copy = Some(cx.spawn(async move |this, cx| {
                            let result = gpui_tokio::Tokio::spawn(cx, async move {
                                store.operation_result(id).await
                            })
                            .await;
                            let _ = this.update(cx, |v, cx| {
                                v.result = if matches!(result, Ok(Ok(_))) {
                                    "Copied".into()
                                } else {
                                    "Could not copy. Try again.".into()
                                };
                                cx.notify()
                            });
                        }));
                    }
                    cx.stop_propagation();
                    return;
                }
            }
        }
        if self.recording {
            cx.stop_propagation();
            if e.keystroke.key == "escape" {
                self.recording = false;
                return;
            }
            if let Some(hotkey) = super::settings::view::accelerator(&e.keystroke) {
                self.save_hotkey(hotkey, cx);
            }
            return;
        }
        if e.keystroke.key == "tab" {
            if e.keystroke.modifiers.shift {
                w.focus_prev(cx)
            } else {
                w.focus_next(cx)
            }
            cx.stop_propagation()
        }
    }
}
impl Drop for Onboarding {
    fn drop(&mut self) {
        self.store.retire(self.owner)
    }
}
impl Onboarding {
    fn save_hotkey(&mut self, hotkey: String, cx: &mut Context<Self>) {
        self.dirty_hotkey = true;
        self.recording = false;
        self.error = None;
        self.store.enqueue(
            self.owner,
            vec![Edit::Global {
                field: "raymesHotkey".into(),
                value: json!(hotkey),
            }],
        );
        let store = self.store.clone();
        let drain = store.drain_owner(self.owner);
        self.shortcut_save = Some(cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, drain).await;
            let _ = this.update(cx, |v, cx| {
                if matches!(result, Ok(Ok(()))) {
                    v.hotkey = hotkey;
                    v.done.insert("shortcut");
                } else {
                    v.error =
                        Some("Shortcut unavailable. Your previous shortcut is unchanged.".into());
                }
                cx.notify();
            });
        }));
    }
    fn step_done(&self, step: Step, cx: &App) -> bool {
        match step {
            Step::Shortcut => true,
            Step::Practice => self.done.iter().filter(|s| **s != "shortcut").count() == 4,
            Step::Ai => self.content.read(cx).onboarding_connection().is_some(),
            Step::Knowledge => self
                .content
                .read(cx)
                .knowledge
                .snapshot
                .as_ref()
                .is_some_and(|s| !s.roots.is_empty()),
            Step::Permissions => self
                .content
                .read(cx)
                .permissions
                .snapshot
                .as_ref()
                .is_some_and(|s| {
                    s.statuses.iter().all(|p| {
                        matches!(
                            p.state,
                            tezbar_ui_model::utilities::PermissionState::Granted
                                | tezbar_ui_model::utilities::PermissionState::Unsupported
                        )
                    })
                }),
        }
    }
    fn heading(&self, title: &str, description: &str, t: tokens::Tokens) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .mb(px(20.))
            .child(
                div()
                    .text_size(px(20.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title.to_owned()),
            )
            .child(
                div()
                    .max_w(px(460.))
                    .text_size(px(12.5))
                    .line_height(px(19.))
                    .text_color(t.text3)
                    .child(description.to_owned()),
            )
    }
    fn practice_body(&self, t: tokens::Tokens, cx: &mut Context<Self>) -> Div {
        let q = self.practice.read(cx).content.to_string();
        let mode = tezbar_ui_model::launcher::input_mode(&q, self.practice_terminal);
        let mut response = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(16.))
            .min_h(px(124.));
        for (i, row) in self.completions.iter().enumerate() {
            let weak = cx.entity().downgrade();
            response = response.child(
                controls::action_button(
                    format!("practice-folder-{i}"),
                    row["title"].as_str().unwrap_or("Folder").to_owned(),
                    ButtonTone::Ghost,
                    false,
                    t,
                    move |w, cx| {
                        let _ = weak.update(cx, |v, cx| v.complete_path(i, w, cx));
                    },
                )
                .role(Role::ListBoxOption)
                .aria_selected(i == self.completion_selected)
                .bg(if i == self.completion_selected {
                    t.selected
                } else {
                    transparent_black().into()
                })
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(t.text4)
                        .child(row["subtitle"].as_str().unwrap_or("").to_owned()),
                ),
            );
        }
        if self.completions.is_empty() {
            response = response.child(
                div()
                    .text_size(px(
                        if self.done.contains("math")
                            && mode.mode == tezbar_ui_model::launcher::Mode::Basic
                        {
                            28.
                        } else {
                            12.5
                        },
                    ))
                    .child(if self.result.is_empty() {
                        "Try one of the examples below.".into()
                    } else {
                        self.result.clone()
                    }),
            );
            let hint = match mode.mode {
                tezbar_ui_model::launcher::Mode::Terminal => {
                    "Backspace on an empty line goes back to search."
                }
                tezbar_ui_model::launcher::Mode::Ai => {
                    "Ask anything. Tezbar can read files and run commands, and asks before anything risky."
                }
                _ => "Enter copies it. Units work too, like 5 km to mi.",
            };
            response = response.child(div().text_size(px(11.5)).text_color(t.text4).child(hint));
        }
        let input = div()
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(16.))
            .h(px(48.))
            .child(
                div()
                    .text_color(if self.practice_terminal {
                        t.success
                    } else {
                        t.text3
                    })
                    .child(if self.practice_terminal {
                        ">_"
                    } else if mode.mode == tezbar_ui_model::launcher::Mode::Ai {
                        "AI"
                    } else {
                        "⌕"
                    }),
            )
            .child(div().flex_1().child(self.practice.clone()));
        let bar = div()
            .flex()
            .flex_col()
            .min_h(px(176.))
            .rounded(px(12.))
            .border_1()
            .border_color(t.hairline)
            .bg(t.panel)
            .overflow_hidden()
            .child(input)
            .child(div().border_t_1().border_color(t.hairline).child(response));
        let tasks = [
            (
                "math",
                "=",
                "Do quick math",
                "Type a sum, like 12% of 480",
                "12% of 480",
            ),
            (
                "folder",
                "/",
                "Jump to a folder",
                "Start with / and a folder name",
                "/",
            ),
            (
                "terminal",
                ">",
                "Open a terminal",
                "Press > on an empty bar",
                ">",
            ),
            (
                "ai",
                "Space",
                "Ask the AI",
                "Start with a space, then ask",
                " ",
            ),
        ];
        let mut grid = div().flex().flex_col().gap(px(8.)).mt(px(12.));
        for pair in tasks.chunks(2) {
            let mut row = div().flex().gap(px(8.));
            for &(id, trigger, title, hint, example) in pair {
                let weak = cx.entity().downgrade();
                let done = self.done.contains(id);
                row = row.child(
                    controls::action_button(
                        format!("onboarding-practice-{id}"),
                        title,
                        ButtonTone::Ghost,
                        false,
                        t,
                        move |w, cx| {
                            let _ = weak.update(cx, |v, cx| {
                                v.practice_terminal = false;
                                v.practice_cwd = None;
                                v.practice
                                    .update(cx, |input, cx| input.set_text(example, w, cx));
                                v.practice_committed(example, cx);
                                v.practice.focus_handle(cx).focus(w, cx);
                            });
                        },
                    )
                    .flex_1()
                    .min_w(px(0.))
                    .min_h(px(62.))
                    .p(px(10.))
                    .flex()
                    .flex_col()
                    .items_start()
                    .border_1()
                    .border_color(if done { t.success } else { t.hairline })
                    .rounded(px(8.))
                    .bg(if done { t.selected } else { t.panel })
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(t.text4)
                            .child(format!("{}  {hint}", if done { "✓" } else { trigger })),
                    ),
                );
            }
            grid = grid.child(row);
        }
        div().child(bar).child(grid)
    }
}
impl Render for Onboarding {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::tokens(w);
        if self.splash {
            let weak = cx.entity().downgrade();
            return controls::sheet(t)
                .font_family(tokens::font_family(cx, false))
                .id("onboarding-splash")
                .role(Role::Group)
                .aria_label("Welcome to Tezbar")
                .track_focus(&self.focus)
                .on_key_down(cx.listener(Self::key))
                .items_center()
                .justify_center()
                .gap(px(16.))
                .child(
                    div()
                        .size(px(80.))
                        .rounded(px(12.))
                        .overflow_hidden()
                        .child(img(self.icon.clone()).size_full()),
                )
                .child(
                    div()
                        .text_size(px(28.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Welcome to Tezbar"),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(t.text3)
                        .child("Five quick steps and it’s set up the way you work."),
                )
                .child(
                    controls::action_button(
                        "onboarding-start",
                        "Press any key to start",
                        ButtonTone::Ghost,
                        false,
                        t,
                        move |w, cx| {
                            let _ = weak.update(cx, |v, cx| v.go(0, w, cx));
                        },
                    )
                    .text_size(px(11.))
                    .text_color(t.text4),
                )
                .into_any_element();
        }
        let mut rail = div()
            .id("onboarding-step-rail")
            .role(Role::TabList)
            .w(px(200.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(t.hairline)
            .px(px(12.))
            .py(px(20.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(8.))
                    .child(img(self.icon.clone()).size(px(28.)).rounded(px(8.)))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Set up Tezbar"),
                    ),
            );
        let mut steps = div().flex_1().mt(px(24.)).flex().flex_col().gap(px(4.));
        for (i, step) in self.steps.iter().enumerate() {
            let weak = cx.entity().downgrade();
            let current = self.step == i;
            let done = self.step_done(*step, cx);
            let summary = self.summary(*step, cx);
            steps = steps.child(
                div()
                    .id(format!("onboarding-step-{i}"))
                    .role(Role::Tab)
                    .aria_label(step.title())
                    .aria_selected(current)
                    .aria_description(summary.clone())
                    .track_focus(&self.rail_focus[i])
                    .tab_index(if current { 0 } else { -1 })
                    .cursor_pointer()
                    .w_full()
                    .px(px(8.))
                    .py(px(8.))
                    .rounded(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .bg(if current {
                        t.selected
                    } else {
                        transparent_black().into()
                    })
                    .hover(|s| {
                        if current {
                            s
                        } else {
                            s.bg(t.pill)
                        }
                    })
                    .on_mouse_down(MouseButton::Left, move |_, w, cx| {
                        let _ = weak.update(cx, |v, cx| v.go(i, w, cx));
                    })
                    .child(
                        div()
                            .size(px(22.))
                            .flex_shrink_0()
                            .rounded_full()
                            .border_1()
                            .border_color(if done {
                                t.success
                            } else if current {
                                t.accent_strong
                            } else {
                                t.hairline
                            })
                            .bg(if done {
                                t.selected
                            } else {
                                transparent_black().into()
                            })
                            .text_size(px(12.))
                            .text_color(if done { t.success } else { t.accent_strong })
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(if done {
                                "✓"
                            } else if current {
                                "●"
                            } else {
                                ""
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .justify_center()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .line_height(px(16.))
                                    .font_weight(if current {
                                        FontWeight::MEDIUM
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .text_color(if current { t.text1 } else { t.text2 })
                                    .child(step.title()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .line_height(px(14.))
                                    .text_color(t.text4)
                                    .child(summary),
                            ),
                    ),
            );
        }
        let skip = cx.entity().downgrade();
        rail = rail.child(steps).child(
            controls::action_button(
                "onboarding-skip",
                "Skip setup",
                ButtonTone::Ghost,
                self.finishing,
                t,
                move |_, cx| {
                    let _ = skip.update(cx, |v, cx| v.finish(cx));
                },
            )
            .text_size(px(11.5))
            .text_color(t.text4),
        );
        let step = self.steps[self.step];
        let (title, description) = match step {
            Step::Shortcut => (
                "Pick the shortcut that opens Tezbar",
                "Tezbar stays out of the way until you press this, from any app. Press it again, or Esc, to put it away.",
            ),
            Step::Practice => (
                "Try the bar",
                "One bar, a few starting characters. Try these here — nothing opens or runs until you do it in the real bar.",
            ),
            Step::Ai => (
                "Connect an AI model",
                "AI chat can read files and run commands for you, asking before anything risky. Bring your own key, or use a local model.",
            ),
            Step::Knowledge => (
                "Choose what DeepSearch reads",
                "DeepSearch reads the text inside documents, PDFs, and screenshots, with OCR running on this computer, so you can find a file by what it says. Start a search with ! to use it.",
            ),
            Step::Permissions => (
                "Allow what you’ll use",
                "A few features need macOS access. Allow only what you want to use — the rest can wait.",
            ),
        };
        let mut body = div().w_full().child(self.heading(title, description, t));
        match step {
            Step::Shortcut => {
                let mut keys = div().flex().items_center().gap(px(8.)).h(px(52.));
                for key in self.hotkey.split('+') {
                    keys = keys.child(
                        div()
                            .min_w(px(48.))
                            .h(px(48.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .px(px(12.))
                            .rounded(px(9.))
                            .border_1()
                            .border_color(t.rim)
                            .bg(t.pill)
                            .text_size(px(18.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(
                                match key {
                                    "Alt" => "⌥",
                                    "Command" => "⌘",
                                    "Control" => "⌃",
                                    "Shift" => "⇧",
                                    v => v,
                                }
                                .to_owned(),
                            ),
                    );
                }
                let change = cx.entity().downgrade();
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(16.))
                        .h(px(148.))
                        .rounded(px(12.))
                        .border_1()
                        .border_color(if self.recording {
                            t.accent_strong
                        } else {
                            t.hairline
                        })
                        .bg(t.panel)
                        .child(if self.recording {
                            div()
                                .text_size(px(14.))
                                .text_color(t.text3)
                                .child("Press the keys together…")
                        } else {
                            keys
                        })
                        .child(div().text_size(px(11.5)).text_color(t.text4).child(
                            if self.recording {
                                "Hold a modifier, then press a key. Esc cancels."
                            } else {
                                "Works in every app, even full-screen ones."
                            },
                        )),
                );
                let mut shortcut_actions =
                    div().flex().items_center().gap(px(12.)).mt(px(16.)).child(
                        controls::action_button(
                            "onboarding-hotkey",
                            if self.recording {
                                "Cancel"
                            } else {
                                "Change shortcut"
                            },
                            if self.recording {
                                ButtonTone::Ghost
                            } else {
                                ButtonTone::Primary
                            },
                            self.finishing,
                            t,
                            move |w, cx| {
                                let _ = change.update(cx, |v, cx| {
                                    v.recording = !v.recording;
                                    w.focus(&v.focus, cx);
                                    cx.notify();
                                });
                            },
                        ),
                    );
                let default = if cfg!(windows) {
                    "Control+Space"
                } else {
                    "Alt+Space"
                };
                if self.hotkey != default && !self.recording {
                    let reset = cx.entity().downgrade();
                    shortcut_actions = shortcut_actions.child(controls::action_button(
                        "onboarding-reset-hotkey",
                        format!("Reset to {default}"),
                        ButtonTone::Quiet,
                        self.finishing,
                        t,
                        move |_, cx| {
                            let _ = reset.update(cx, |v, cx| v.save_hotkey(default.into(), cx));
                        },
                    ));
                }
                body = body.child(shortcut_actions);
            }
            Step::Practice => body = body.child(self.practice_body(t, cx)),
            _ => body = body.child(self.content.clone()),
        }
        let next = cx.entity().downgrade();
        let back = cx.entity().downgrade();
        let retry = cx.entity().downgrade();
        let last = self.step + 1 == self.steps.len();
        let done = self.step_done(step, cx);
        let footer = div().debug_selector(|| "onboarding-footer".into())
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.))
            .px(px(28.))
            .py(px(12.))
            .border_t_1()
            .border_color(t.hairline)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(11.))
                    .text_color(t.text4)
                    .child(controls::keycap(
                        if cfg!(windows) { "Ctrl" } else { "⌘" },
                        t,
                    ))
                    .child(controls::keycap("↵", t))
                    .child(if last { "Finish" } else { "Continue" }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .when(self.step > 0, |d| {
                        d.child(controls::back_button(
                            controls::action_button(
                                "onboarding-back",
                                "Back",
                                ButtonTone::Ghost,
                                self.finishing,
                                t,
                                move |w, cx| {
                                    let _ = back
                                        .update(cx, |v, cx| v.go(v.step.saturating_sub(1), w, cx));
                                },
                            ),
                            t,
                        ))
                    })
                    .child(
                        controls::action_button(
                            "onboarding-next",
                            if self.finishing {
                                "Saving…"
                            } else if last {
                                "Start using Tezbar"
                            } else if done {
                                "Continue"
                            } else {
                                "Skip for now"
                            },
                            ButtonTone::Primary,
                            self.finishing,
                            t,
                            move |w, cx| {
                                let _ = next.update(cx, |v, cx| {
                                    if v.step + 1 == v.steps.len() {
                                        v.finish(cx)
                                    } else {
                                        v.go(v.step + 1, w, cx)
                                    }
                                });
                            },
                        )
                        .min_w(px(112.)),
                    ),
            );
        let is_centered_step = matches!(step, Step::Shortcut | Step::Practice);
        let section = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .child(
                div()
                    .id("onboarding-step-content")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .px(px(28.))
                    .py(px(20.))
                    .flex()
                    .flex_col()
                    .when(is_centered_step, |d| d.justify_center())
                    .child(body),
            )
            .when_some(self.error.clone(), |d, error| {
                d.child(
                    div()
                        .id("onboarding-error")
                        .role(Role::Alert)
                        .px(px(28.))
                        .pb(px(12.))
                        .text_size(px(12.))
                        .text_color(t.danger)
                        .child(error)
                        .child(controls::action_button(
                            "onboarding-retry",
                            "Retry",
                            ButtonTone::Quiet,
                            self.finishing,
                            t,
                            move |_, cx| {
                                let _ = retry.update(cx, |v, cx| v.retry_finish(cx));
                            },
                        )),
                )
            })
            .child(footer);
        controls::sheet(t)
            .font_family(tokens::font_family(cx, false))
            .id("native-onboarding")
            .role(Role::Group)
            .aria_label("Onboarding")
            .flex_row()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .child(rail)
            .child(section)
            .into_any_element()
    }
}

/// Call after coreReady and before showing either surface. One retry, then fail open.
pub fn boot_gate(store: SettingsStore) -> tezbar_core::host::HostFuture<bool> {
    Box::pin(async move {
        for attempt in 0..2 {
            match store.transport().durable_config().await {
                Ok(config) => return Ok(config["hasCompletedOnboarding"] != true),
                Err(_) if attempt == 0 => {
                    tokio::time::sleep(std::time::Duration::from_millis(400)).await
                }
                Err(_) => return Ok(false),
            }
        }
        Ok(false)
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::settings::tests::{Fixture, runtime, settle};
    use ::core::prelude::v1::test;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    #[gpui::test]
    fn native_finish_failure_retains_surface_and_retry_is_durable(cx: &mut TestAppContext) {
        let rt = runtime(cx);
        let fixture = Arc::new(Fixture {
            config: Mutex::new(json!({})),
            fail: AtomicBool::new(true),
            ..Default::default()
        });
        let store = SettingsStore::new(fixture.clone());
        let events = std::rc::Rc::new(std::cell::Cell::new(0));
        let count = events.clone();
        let (view, cx) = cx.add_window_view(|w, cx| {
            let mut v = Onboarding::new(store, &UiState::default(), false, w, cx);
            let sub = cx.subscribe(&cx.entity(), move |_, _, e: &OnboardingEffect, _| {
                if matches!(e, OnboardingEffect::Finished) {
                    count.set(count.get() + 1)
                }
            });
            v._subscriptions.push(sub);
            v
        });
        view.update(cx, |v, cx| v.finish(cx));
        settle(&rt, cx);
        assert!(view.read_with(cx, |v, _| v.error.is_some()));
        assert_eq!(events.get(), 0);
        fixture.fail.store(false, Ordering::SeqCst);
        view.update(cx, |v, cx| v.retry_finish(cx));
        settle(&rt, cx);
        assert_eq!(events.get(), 1);
        assert_eq!(
            fixture.config.lock().unwrap()["hasCompletedOnboarding"],
            true
        );
    }
    #[gpui::test]
    fn late_initialization_cannot_replace_a_new_native_shortcut(cx: &mut TestAppContext) {
        let rt = runtime(cx);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let fixture = Arc::new(Fixture {
            config: Mutex::new(json!({"raymesHotkey":"Alt+Space"})),
            ..Default::default()
        });
        *fixture.read_gate.lock().unwrap() = Some(rx);
        let store = SettingsStore::new(fixture.clone());
        let (view, cx) =
            cx.add_window_view(|w, cx| Onboarding::new(store, &UiState::default(), false, w, cx));
        cx.update(|w, cx| view.update(cx, |v, cx| v.activate(w, cx)));
        settle(&rt, cx);
        cx.update(|w, cx| {
            view.update(cx, |v, cx| {
                v.recording = true;
                v.key(
                    &KeyDownEvent {
                        keystroke: Keystroke::parse("ctrl-k").unwrap(),
                        is_held: false,
                        prefer_character_input: false,
                    },
                    w,
                    cx,
                )
            })
        });
        settle(&rt, cx);
        tx.send(()).unwrap();
        settle(&rt, cx);
        assert_eq!(view.read_with(cx, |v, _| v.hotkey.clone()), "Control+K");
        assert_eq!(fixture.config.lock().unwrap()["raymesHotkey"], "Control+K");
    }
    #[tokio::test]
    async fn boot_gate_retries_once_then_fails_open() {
        let fixture = Arc::new(Fixture::default());
        fixture.fail_reads.store(2, Ordering::SeqCst);
        let store = SettingsStore::new(fixture.clone());
        assert!(!boot_gate(store).await.unwrap());
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 2);
    }

    #[gpui::test]
    fn native_practice_path_completion_is_scoped_and_never_opens_files(cx: &mut TestAppContext) {
        let rt = runtime(cx);
        let fixture = Arc::new(Fixture::default());
        let store = SettingsStore::new(fixture.clone());
        let (view, cx) =
            cx.add_window_view(|w, cx| Onboarding::new(store, &UiState::default(), false, w, cx));
        cx.update(|w, cx| {
            view.update(cx, |v, cx| {
                v.go(1, w, cx);
                v.practice
                    .update(cx, |input, cx| input.set_text("/fixture", w, cx));
            })
        });
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(90));
        settle(&rt, cx);
        assert!(view.read_with(cx, |v, _| v.done.contains("folder")));
        assert_eq!(view.read_with(cx, |v, _| v.completions.len()), 1);
        cx.update(|w, cx| view.update(cx, |v, cx| v.complete_path(0, w, cx)));
        settle(&rt, cx);
        assert_eq!(
            view.read_with(cx, |v, cx| v.practice.read(cx).content.to_string()),
            "/fixture/"
        );
        assert!(!fixture.calls.lock().unwrap().iter().any(|(c, _)| matches!(
            *c,
            "shell:open" | "search:execute" | "terminal:create" | "agent:run"
        )));
    }
    #[gpui::test]
    fn native_practice_uses_real_calculator_and_windows_omits_permission_step(
        cx: &mut TestAppContext,
    ) {
        let _rt = runtime(cx);
        let fixture = Arc::new(Fixture::default());
        let store = SettingsStore::new(fixture);
        let (view, cx) =
            cx.add_window_view(|w, cx| Onboarding::new(store, &UiState::default(), false, w, cx));
        view.update(cx, |v, _| {
            v.practice_changed("= 12 * 8");
            assert_eq!(v.result, "96");
            v.practice_changed("/fixture");
            v.practice_changed("> echo fixture");
            v.practice_terminal = false;
            v.practice_changed(" ask fixture");
            assert_eq!(v.done.len(), 3);
            assert!(!v.steps.contains(&Step::Permissions));
        });
    }

    #[gpui::test]
    fn shortcut_and_practice_steps_are_vertically_centered(cx: &mut TestAppContext) {
        let fixture = Arc::new(Fixture::default());
        let store = SettingsStore::new(fixture);
        let (view, cx) =
            cx.add_window_view(|w, cx| Onboarding::new(store, &UiState::default(), false, w, cx));
        cx.update(|w, cx| {
            view.update(cx, |v, cx| {
                v.go(0, w, cx);
            })
        });
        assert_eq!(view.read_with(cx, |v, _| v.step), 0);
        cx.update(|w, cx| {
            view.update(cx, |v, cx| {
                v.go(1, w, cx);
            })
        });
        assert_eq!(view.read_with(cx, |v, _| v.step), 1);
    }
}

#[cfg(test)]
mod review_regressions {
    // Independent-review rejection regression.
    use super::*;
    use crate::surfaces::settings::{
        SettingsFuture, SettingsTransport,
        tests::{Fixture, runtime, settle},
    };
    use ::core::prelude::v1::test;
    use std::sync::{Arc, Mutex};
    /// The real core returns {"ok":false} when the shortcut is registered by another app.
    struct Conflict(Arc<Fixture>);
    impl SettingsTransport for Conflict {
        fn durable_config(&self) -> SettingsFuture {
            self.0.durable_config()
        }
        fn send(&self, request: SettingsRequest) -> SettingsFuture {
            if let SettingsRequest::Save(patch) = &request {
                if patch.get("raymesHotkey").is_some() {
                    return Box::pin(async {
                        Ok(json!({"ok":false,"accelerator":"Control+K","error":"in use"}))
                    });
                }
            }
            self.0.send(request)
        }
    }
    #[gpui::test]
    fn review_conflicting_shortcut_does_not_block_finish_or_skip(cx: &mut TestAppContext) {
        let rt = runtime(cx);
        let fixture = Arc::new(Fixture {
            config: Mutex::new(json!({"raymesHotkey":"Alt+Space"})),
            ..Default::default()
        });
        let store = SettingsStore::new(Arc::new(Conflict(fixture.clone())));
        let finished = std::rc::Rc::new(std::cell::Cell::new(0));
        let count = finished.clone();
        let (view, cx) = cx.add_window_view(|w, cx| {
            let mut v = Onboarding::new(store.clone(), &UiState::default(), false, w, cx);
            let sub = cx.subscribe(&cx.entity(), move |_, _, e: &OnboardingEffect, _| {
                if matches!(e, OnboardingEffect::Finished) {
                    count.set(count.get() + 1)
                }
            });
            v._subscriptions.push(sub);
            v
        });
        cx.update(|w, cx| view.update(cx, |v, cx| v.activate(w, cx)));
        settle(&rt, cx);
        cx.update(|w, cx| {
            view.update(cx, |v, cx| {
                v.recording = true;
                v.key(
                    &KeyDownEvent {
                        keystroke: Keystroke::parse("ctrl-k").unwrap(),
                        is_held: false,
                        prefer_character_input: false,
                    },
                    w,
                    cx,
                )
            })
        });
        settle(&rt, cx);
        let shortcut_error = view.read_with(cx, |v, _| v.error.clone());
        let kept = view.read_with(cx, |v, _| v.hotkey.clone());
        // The user accepts the "previous shortcut is unchanged" outcome and skips the tour.
        view.update(cx, |v, cx| {
            v.error = None;
            v.finish(cx)
        });
        settle(&rt, cx);
        let skip_error = view.read_with(cx, |v, _| v.error.clone());
        view.update(cx, |v, cx| v.retry_finish(cx));
        settle(&rt, cx);
        let retry_error = view.read_with(cx, |v, _| v.error.clone());
        let barrier =
            rt.block_on(store.drain_with(crate::surfaces::settings::EffectBarrier::default()));
        eprintln!(
            "OPUS shortcut_error={shortcut_error:?} kept={kept:?} skip_error={skip_error:?} retry_error={retry_error:?} finished={} hasCompleted={} close/quit barrier={barrier:?}",
            finished.get(),
            fixture.config.lock().unwrap()["hasCompletedOnboarding"]
        );
        assert_eq!(kept, "Alt+Space");
        assert_eq!(
            finished.get(),
            1,
            "a rejected shortcut (old binding kept) prevents Skip/Finish forever"
        );
    }
    #[tokio::test]
    async fn review_rejection_belongs_to_its_revision_not_a_later_finish() {
        let fixture = Arc::new(Fixture {
            config: Mutex::new(json!({"raymesHotkey":"Alt+Space"})),
            ..Default::default()
        });
        let store = SettingsStore::new(Arc::new(Conflict(fixture.clone())));
        let owner = store.owner();
        store.enqueue(
            owner,
            vec![Edit::Global {
                field: "raymesHotkey".into(),
                value: json!("Control+K"),
            }],
        );
        let shortcut = store.drain_owner(owner);
        store.enqueue(
            owner,
            vec![Edit::Global {
                field: "hasCompletedOnboarding".into(),
                value: json!(true),
            }],
        );
        let finish = store.drain_owner(owner);
        let (shortcut, finish) = tokio::join!(shortcut, finish);
        assert!(shortcut.is_err());
        finish.unwrap();
        store.drain().await.unwrap();
        assert_eq!(fixture.config.lock().unwrap()["raymesHotkey"], "Alt+Space");
        assert_eq!(
            fixture.config.lock().unwrap()["hasCompletedOnboarding"],
            true
        );
    }
}

#[cfg(test)]
mod rerecord_regressions {
    use super::*;
    use crate::surfaces::settings::tests::{Fixture, RerecordTransport, runtime, settle};
    use ::core::prelude::v1::test;
    use std::sync::{Arc, Mutex, atomic::Ordering};
    fn press(v: &mut Onboarding, key: &'static str, w: &mut Window, cx: &mut Context<Onboarding>) {
        v.recording = true;
        v.key(
            &KeyDownEvent {
                keystroke: Keystroke::parse(key).unwrap(),
                is_held: false,
                prefer_character_input: false,
            },
            w,
            cx,
        )
    }
    // Real Onboarding key handler: re-record while the first (rejected) save is still in flight.
    #[gpui::test]
    fn review_nb1_onboarding_rerecord_shows_the_accepted_shortcut(cx: &mut TestAppContext) {
        let rt = runtime(cx);
        let fixture = Arc::new(Fixture {
            config: Mutex::new(json!({"raymesHotkey":"Alt+Space"})),
            ..Default::default()
        });
        let (release, hold) = tokio::sync::oneshot::channel();
        let transport = Arc::new(RerecordTransport::new(fixture.clone(), Some(hold)));
        let store = SettingsStore::new(transport.clone());
        let (view, cx) = cx.add_window_view(|w, cx| {
            Onboarding::new(store.clone(), &UiState::default(), false, w, cx)
        });
        cx.update(|w, cx| view.update(cx, |v, cx| v.activate(w, cx)));
        settle(&rt, cx);
        cx.update(|w, cx| view.update(cx, |v, cx| press(v, "ctrl-k", w, cx)));
        settle(&rt, cx);
        let in_flight = transport.entered.load(Ordering::SeqCst);
        cx.update(|w, cx| view.update(cx, |v, cx| press(v, "ctrl-j", w, cx)));
        settle(&rt, cx);
        let _ = release.send(());
        settle(&rt, cx);
        let hotkey = view.read_with(cx, |v, _| v.hotkey.clone());
        let error = view.read_with(cx, |v, _| v.error.clone());
        let done = view.read_with(cx, |v, _| v.done.contains("shortcut"));
        let durable = fixture.config.lock().unwrap()["raymesHotkey"].clone();
        eprintln!(
            "B6 onboarding rerecord: first_in_flight={in_flight} durable={durable} shown_hotkey={hotkey:?} error={error:?} shortcut_done={done}"
        );
        assert_eq!(durable, json!("Control+J"));
        assert_eq!(
            hotkey, "Control+J",
            "onboarding shows the old shortcut although the new one saved"
        );
        assert!(error.is_none(), "accepted shortcut reported as unavailable");
        assert!(done, "accepted shortcut did not finish its onboarding step");
    }
}
