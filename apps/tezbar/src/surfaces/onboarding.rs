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
            let mut v = TextInput::new(cx);
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
            _subscriptions: vec![sub, relay],
            finish: None,
            load: None,
            shortcut_save: None,
            finishing: false,
        }
    }
    pub fn activate(&mut self, _w: &mut Window, cx: &mut Context<Self>) {
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
        if self.finishing {
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
        self.finish = Some(cx.spawn(async move |this, cx| {
            let r = gpui_tokio::Tokio::spawn(cx, store.drain()).await;
            let _ = this.update(cx, |v, cx| {
                v.finishing = false;
                if matches!(r, Ok(Ok(()))) {
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
            Step::Ai => {
                let cfg = if content.config.as_object().is_some_and(|m| !m.is_empty()) {
                    &content.config
                } else {
                    &self.initial_config
                };
                if let Some(provider) = cfg["provider"].as_str() {
                    if tezbar_ui_model::settings::providers::configured(cfg, provider) {
                        format!(
                            "{provider} · {}",
                            cfg["taskModelOverrides"]["chat"]
                                .as_str()
                                .or(cfg["model"].as_str())
                                .unwrap_or("Choose model")
                        )
                    } else {
                        "Connect a model".into()
                    }
                } else {
                    "Choose a provider".into()
                }
            }
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
                self.dirty_hotkey = true;
                self.recording = false;
                self.store.enqueue(
                    self.owner,
                    vec![Edit::Global {
                        field: "raymesHotkey".into(),
                        value: json!(hotkey),
                    }],
                );
                let store = self.store.clone();
                self.shortcut_save = Some(cx.spawn(async move |this, cx| {
                    let r = gpui_tokio::Tokio::spawn(cx, store.drain()).await;
                    let _ = this.update(cx, |v, cx| {
                        if matches!(r, Ok(Ok(()))) {
                            v.hotkey = hotkey;
                            v.done.insert("shortcut");
                        } else {
                            v.error = Some(
                                "Shortcut unavailable. Your previous shortcut is unchanged.".into(),
                            );
                        }
                        cx.notify()
                    });
                }));
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
impl Render for Onboarding {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::tokens(w);
        let mut rail = div()
            .id(("semantic", line!() as usize))
            .role(Role::TabList)
            .flex()
            .gap(px(8.));
        for (i, step) in self.steps.iter().enumerate() {
            let view = cx.entity().downgrade();
            let summary = self.summary(*step, cx);
            rail = rail.child(
                controls::action_button(
                    format!("onboarding-step-{i}"),
                    step.title(),
                    ButtonTone::Ghost,
                    false,
                    t,
                    move |w, cx| {
                        let _ = view.update(cx, |v, cx| v.go(i, w, cx));
                    },
                )
                .role(Role::Tab)
                .aria_selected(self.step == i)
                .aria_description(summary.clone())
                .track_focus(&self.rail_focus[i])
                .tab_index(if self.step == i { 0 } else { -1 })
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(div().text_size(px(10.)).text_color(t.text2).child(summary)),
            );
        }
        let body = match self.steps[self.step] {
            Step::Shortcut => {
                let view = cx.entity().downgrade();
                div()
                    .child("Press the keys together to set your Tezbar shortcut.")
                    .child(controls::action_button(
                        "onboarding-hotkey",
                        if self.recording {
                            "Press shortcut…".to_string()
                        } else {
                            self.hotkey.clone()
                        },
                        ButtonTone::Quiet,
                        false,
                        t,
                        move |w, cx| {
                            let _ = view.update(cx, |v, cx| {
                                v.recording = true;
                                w.focus(&v.focus, cx);
                                cx.notify()
                            });
                        },
                    ))
                    .into_any_element()
            }
            Step::Practice => {
                let mut d = div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(self.practice.clone())
                    .child(self.result.clone())
                    .child(format!(
                        "{} of 4 tried",
                        self.done.iter().filter(|s| **s != "shortcut").count()
                    ));
                for (i, row) in self.completions.iter().enumerate() {
                    let view = cx.entity().downgrade();
                    d = d.child(
                        controls::action_button(
                            format!("practice-folder-{i}"),
                            row["title"].as_str().unwrap_or("Folder").to_string(),
                            ButtonTone::Ghost,
                            false,
                            t,
                            move |w, cx| {
                                let _ = view.update(cx, |v, cx| v.complete_path(i, w, cx));
                            },
                        )
                        .role(Role::ListBoxOption)
                        .aria_selected(i == self.completion_selected),
                    );
                }
                d.into_any_element()
            }
            _ => div()
                .size_full()
                .child(self.content.clone())
                .into_any_element(),
        };
        let view = cx.entity().downgrade();
        let back = cx.entity().downgrade();
        let skip = cx.entity().downgrade();
        let retry = cx.entity().downgrade();
        controls::sheet(t)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .child(
                div()
                    .p(px(20.))
                    .child(
                        div()
                            .id(("semantic", line!() as usize))
                            .role(Role::Heading)
                            .child("Welcome to Tezbar"),
                    )
                    .child(rail),
            )
            .child(div().flex_1().min_h(px(0.)).p(px(20.)).child(body))
            .when_some(self.error.clone(), |d, e| {
                d.child(
                    div()
                        .id(("semantic", line!() as usize))
                        .role(Role::Alert)
                        .text_color(t.danger)
                        .child(e)
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
            .child(
                controls::footer(t)
                    .child(controls::action_button(
                        "onboarding-back",
                        "Back",
                        ButtonTone::Ghost,
                        self.step == 0 || self.finishing,
                        t,
                        move |w, cx| {
                            let _ = back.update(cx, |v, cx| v.go(v.step.saturating_sub(1), w, cx));
                        },
                    ))
                    .child(controls::action_button(
                        "onboarding-next",
                        if self.step + 1 == self.steps.len() {
                            "Finish"
                        } else {
                            "Continue"
                        },
                        ButtonTone::Primary,
                        self.finishing,
                        t,
                        move |w, cx| {
                            let _ = view.update(cx, |v, cx| {
                                if v.step + 1 == v.steps.len() {
                                    v.finish(cx)
                                } else {
                                    v.go(v.step + 1, w, cx)
                                }
                            });
                        },
                    ))
                    .child(controls::action_button(
                        "onboarding-skip",
                        "Skip tour",
                        ButtonTone::Ghost,
                        self.finishing,
                        t,
                        move |_, cx| {
                            let _ = skip.update(cx, |v, cx| v.finish(cx));
                        },
                    )),
            )
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
}
