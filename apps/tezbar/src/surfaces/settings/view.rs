use super::*;
use tezbar_ui::{
    controls::{self, ButtonTone},
    tokens::{self, Tokens},
};
impl SettingsContent {
    pub(super) fn button(
        &self,
        id: String,
        label: impl Into<SharedString>,
        action: Action,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let view = cx.entity().downgrade();
        controls::action_button(
            id.clone(),
            label,
            ButtonTone::Quiet,
            false,
            t,
            move |w, cx| {
                let _ = view.update(cx, |v, cx| v.action(action.clone(), w, cx));
            },
        )
        .debug_selector(move || id.clone())
    }
    pub(super) fn b(
        &self,
        label: &str,
        action: Action,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let scope = match &action {
            Action::Knowledge(
                knowledge::Operation::RemoveRoot(id) | knowledge::Operation::RootEnabled { id, .. },
            ) => id.clone(),
            Action::Knowledge(knowledge::Operation::RootDepth { id, depth }) => {
                format!("{id}-{depth}")
            }
            Action::CommandAlias(id)
            | Action::CommandHotkey(id)
            | Action::CommandEnabled(id, _)
            | Action::Uninstall(id)
            | Action::Preferences(id, _) => id.clone(),
            _ => String::new(),
        };
        self.button(
            if scope.is_empty() {
                format!("settings-{label}")
            } else {
                format!("settings-{scope}-{label}")
            },
            label.to_owned(),
            action,
            t,
            cx,
        )
    }
    pub(super) fn choice_selected(&self, action: &Action) -> bool {
        match action {
            Action::SetField(key, value) => {
                self.value(key) == *value
                    || (key == "connection.transport"
                        && self.value(key).is_empty()
                        && value == "stdio")
            }
            Action::Theme(theme) => self.state.theme == *theme,
            Action::UpdateChannel(channel) => self.update_channel == *channel,
            Action::Knowledge(knowledge::Operation::Depth(depth)) => self
                .knowledge
                .snapshot
                .as_ref()
                .is_some_and(|s| s.settings.depth == *depth),
            Action::Knowledge(knowledge::Operation::Speed(speed)) => self
                .knowledge
                .snapshot
                .as_ref()
                .is_some_and(|s| s.settings.indexing_speed == *speed),
            Action::Knowledge(knowledge::Operation::RootDepth { id, depth }) => {
                self.knowledge.snapshot.as_ref().is_some_and(|s| {
                    s.roots.iter().any(|r| {
                        r["id"].as_str() == Some(id)
                            && r["depth"].as_str().unwrap_or("inherit") == depth
                    })
                })
            }
            _ => false,
        }
    }
    pub(super) fn choice(
        &self,
        label: &str,
        action: Action,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = self.choice_selected(&action);
        self.b(label, action, t, cx)
            .role(Role::RadioButton)
            .aria_toggled(if selected {
                Toggled::True
            } else {
                Toggled::False
            })
            .when(selected, |d| d.bg(t.selected))
    }
    pub(super) fn field_view(&self, id: &str) -> AnyElement {
        self.fields
            .get(id)
            .map(|f| {
                div()
                    .min_h(px(32.))
                    .child(f.input.clone())
                    .into_any_element()
            })
            .unwrap_or_else(|| div().child("Loading…").into_any_element())
    }
    pub(super) fn labeled(&self, id: &str, label: &str) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(div().text_size(px(12.)).child(label.to_owned()))
            .child(self.field_view(id))
    }
    pub(super) fn toggle(
        &self,
        label: &str,
        key: &'static str,
        checked: bool,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.check(
            format!("toggle-{key}"),
            label.to_owned(),
            checked,
            Action::Toggle(key, !checked),
            t,
            cx,
        )
    }
    pub(super) fn tab_control(
        &self,
        index: usize,
        tab: Tab,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let weak = cx.entity().downgrade();
        let key_view = weak.clone();
        // The legacy toolbar has a 20px icon over a 12px label, in a 54px tile.
        div()
            .id(format!("settings-tab-{}", tab.id()))
            .role(Role::Tab)
            .aria_label(tab.title())
            .aria_selected(self.tab == tab)
            .track_focus(&self.tabs[index])
            .tab_index(if self.tab == tab { 0 } else { -1 })
            .h(px(54.))
            .min_w(px(86.))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(4.))
            .rounded(px(8.))
            .border_1()
            .border_color(if self.tab == tab {
                t.rim
            } else {
                transparent_black().into()
            })
            .bg(if self.tab == tab {
                t.pill_strong
            } else {
                transparent_black().into()
            })
            .text_color(if self.tab == tab { t.text1 } else { t.text3 })
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(tab_icon(tab).size(px(20.)).text_color(if self.tab == tab {
                t.text1
            } else {
                t.text3
            }))
            .child(tab.title())
            .on_click(move |_, w, cx| {
                let _ = weak.update(cx, |v, cx| v.action(Action::Tab(tab), w, cx));
            })
            .on_key_down(move |event, w, cx| {
                if !event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    let _ = key_view.update(cx, |v, cx| v.action(Action::Tab(tab), w, cx));
                    cx.stop_propagation();
                }
            })
    }
    pub fn onboarding_connection(&self) -> Option<(String, String)> {
        if let Some(connection) = &self.onboarding_connected {
            return Some(connection.clone());
        }
        let provider = self.config["provider"].as_str()?;
        if ["ollama", "opencode", "antigravity"].contains(&provider)
            || !settings::providers::configured(&self.config, provider)
        {
            return None;
        }
        let model = self.config["providerSelectedModels"][provider]
            .as_str()
            .or(self.config["model"].as_str())?;
        Some((provider.to_owned(), model.to_owned()))
    }
    /// Compact onboarding compositions use the same retained fields/actions, not Settings pages.
    pub fn onboarding_body(&self, tab: Tab, t: Tokens, cx: &mut Context<Self>) -> Div {
        use super::panels::{card, line, note, provider_title, stack};
        match tab {
            Tab::Ai=>{
                if !self.onboarding_editing {
                    if let Some((provider,model))=self.onboarding_connection() {
                        return stack().child(card(t).flex_row().items_center().gap(px(12.)).child(div().text_color(t.success).child("✓"))
                            .child(div().flex_1().text_size(px(12.5)).child(format!("Connected to {}, using {model}",provider_title(&provider,&self.config))))
                            .child(self.b("Change",Action::EditOnboardingConnection,t,cx)))
                            .child(note("Gemini, DeepSeek, GitHub Copilot, and OpenAI-compatible servers are in Settings → AI.",t));
                    }
                }
                let p=self.provider.provider.clone();
                let mut providers=div().flex().gap(px(8.));
                for (id,title,note_text) in [("anthropic","Anthropic","Claude models"),("openai","OpenAI","GPT models"),("ollama","Ollama","Local, no API key")]{providers=providers.child(self.button(format!("onboarding-provider-{id}"),title,Action::Provider(id.into()),t,cx).flex_1().flex().flex_col().items_start().p(px(12.)).border_1().border_color(if p==id{t.accent_strong}else{t.hairline}).bg(if p==id{t.selected}else{transparent_black().into()}).role(Role::RadioButton).aria_toggled(if p==id{Toggled::True}else{Toggled::False}).child(note(note_text,t)));}
                let mut form=line();
                if p=="ollama"{form=form.child(note("Uses the models Ollama is serving at localhost:11434. Nothing leaves this computer.",t).flex_1());}else{form=form.child(div().flex_1().child(self.field_view(&format!("provider.{p}.apiKey"))));}
                let weak=cx.entity().downgrade();
                form=form.child(controls::action_button("onboarding-connect",if self.onboarding_checking||self.saving{"Checking…"}else{"Connect"},ButtonTone::Primary,self.onboarding_checking||self.saving,t,move |w,cx|{let _=weak.update(cx,|v,cx|v.action(Action::ConnectOnboarding,w,cx));}).min_w(px(92.)));
                stack().child(card(t).child(providers).child(form).child(note(format!("Your key is stored on this computer and sent only to {}.",provider_title(&p,&self.config)),t)))
                    .when(!self.provider.models.is_empty(),|d|d.child(self.select("AI model",&self.provider.selected_model,self.provider.models.iter().map(|m|(text(&m["id"]),Action::SelectModel(text(&m["id"])))) .collect(),t,cx)))
                    .child(note("Gemini, DeepSeek, GitHub Copilot, and OpenAI-compatible servers are in Settings → AI.",t))
            }
            Tab::Knowledge=>stack().child(self.render_folders(true,t,cx)).child(self.render_index_status(t,cx)),
            Tab::Permissions=>stack().child(self.render_permission_cards(t,cx)).child(note("Tezbar only uses these permissions for the features you choose. Change them anytime in System Settings.",t)),
            _=>stack(),
        }
    }
}
impl Render for SettingsContent {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::tokens(w);
        if self.embedded {
            return div()
                .size_full()
                .flex()
                .flex_col()
                .text_color(t.text1)
                .text_size(px(12.5))
                .line_height(relative(1.5))
                .font_family(tokens::font_family(cx, false))
                .track_focus(&self.focus)
                .on_key_down(cx.listener(|this, e: &KeyDownEvent, w, cx| this.key(e, w, cx)))
                .child(self.onboarding_body(self.tab, t, cx))
                .when_some(self.error.clone(), |d, error| {
                    d.child(
                        div()
                            .id("onboarding-settings-error")
                            .role(Role::Alert)
                            .mt(px(12.))
                            .text_size(px(12.))
                            .text_color(t.danger)
                            .child(error),
                    )
                })
                .when_some(self.durable_error().map(str::to_owned), |d, error| {
                    d.child(
                        div()
                            .id("onboarding-save-error")
                            .role(Role::Alert)
                            .text_color(t.danger)
                            .child(error)
                            .child(self.b("Retry saved changes", Action::Retry, t, cx)),
                    )
                })
                .into_any_element();
        }
        let mut tabs = div()
            .id("settings-tabs")
            .role(Role::TabList)
            .flex()
            .items_end()
            .justify_center()
            .gap(px(8.));
        for (index, tab) in Tab::ALL.into_iter().enumerate() {
            tabs = tabs.child(self.tab_control(index, tab, t, cx));
        }
        let body = match self.tab {
            Tab::General => self.render_general(t, cx),
            Tab::Ai => self.render_ai(t, cx),
            Tab::Voice => self.render_voice(t, cx),
            Tab::Knowledge => self.render_knowledge(t, cx),
            Tab::Extensions => self.render_extensions(t, cx),
            Tab::Permissions => self.render_permissions(t, cx),
            Tab::Storage => self.render_storage(t, cx),
            Tab::Advanced => self.render_advanced(t, cx),
        };
        let mut root = div()
            .id("native-settings")
            .line_height(relative(1.5))
            .font_family(tokens::font_family(cx, false))
            .role(Role::Dialog)
            .aria_label("Tezbar Settings")
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(t.settings_panel)
            .text_color(t.text1)
            .text_size(px(12.5))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, w, cx| this.key(e, w, cx)))
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(16.))
                    .pt(px(8.))
                    .pb(px(8.))
                    .border_b_1()
                    .border_color(t.hairline)
                    .child(
                        div()
                            .id("settings-heading")
                            .role(Role::Heading)
                            .on_mouse_down(MouseButton::Left, |_, window, _| {
                                window.start_window_move();
                            })
                            // `start_window_move` is a no-op on Windows; the heading is a caption there.
                            .when(cfg!(windows), |d| d.window_control_area(WindowControlArea::Drag))
                            .text_center()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.text2)
                            .mb(px(8.))
                            .child("Tezbar Settings"),
                    )
                    .child(tabs),
            )
            .child(
                div()
                    .id("settings-content")
                    .role(Role::TabPanel)
                    .aria_label(self.tab.title())
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .px(px(20.))
                    .py(px(16.))
                    .child(
                        div()
                            .w_full()
                            .max_w(px(match self.tab {
                                Tab::Ai => 850.,
                                Tab::Knowledge => 650.,
                                Tab::Extensions => 1100.,
                                _ => 610.,
                            }))
                            .mx_auto()
                            .when(self.tab == Tab::Extensions, |d| d.h_full())
                            .child(body),
                    ),
            );
        if let Some(error) = self.durable_error() {
            root = root.child(
                div()
                    .id("settings-save-error")
                    .role(Role::Alert)
                    .text_color(t.danger)
                    .px(px(16.))
                    .child(error.to_owned())
                    .child(self.b("Retry saved changes", Action::Retry, t, cx)),
            );
        }
        if let Some(error) = self
            .error
            .as_ref()
            .filter(|e| e.as_str() != settings::CLOSE_ERROR)
            .map(String::as_str)
            .or(self.store.validation_error(self.owner))
        {
            root = root.child(
                div()
                    .id("settings-action-error")
                    .role(Role::Alert)
                    .text_color(t.danger)
                    .px(px(16.))
                    .child(error.to_owned()),
            );
        }
        root = root.child(
            div().debug_selector(|| "settings-footer".into())
                .flex()
                .items_center()
                .flex_shrink_0()
                .gap(px(8.))
                .px(px(16.))
                .py(px(8.))
                .border_t_1()
                .border_color(t.hairline)
                .text_size(px(11.))
                .text_color(t.text3)
                .child(controls::keycap("Enter", t))
                .child(self.b("Save", Action::Save, t, cx).bg(transparent_black()))
                .child(controls::keycap("Esc", t))
                .child(
                    self.b("Close", Action::Close, t, cx)
                        .bg(transparent_black()),
                )
                .child(div().flex_1())
                .child(self.message.clone().unwrap_or_default()),
        );
        if let Some(dialog) = self.render_dialog(t, cx) {
            root = root.child(dialog);
        }
        if let Some(action) = self.confirmation.as_ref() {
            let (title, description, accept) = self.confirmation_copy(action);
            let weak = cx.entity().downgrade();
            root = root.child(
                div()
                    .absolute()
                    .inset_0()
                    .occlude()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgba(0x00000066))
                    .child(
                        div()
                            .id("settings-confirmation")
                            .role(Role::AlertDialog)
                            .aria_label(title.clone())
                            .w_full()
                            .max_w(px(440.))
                            .p(px(20.))
                            .rounded(px(12.))
                            .border_1()
                            .border_color(t.rim)
                            .bg(t.popover_solid)
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.text1)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(t.text2)
                                    .child(description),
                            )
                            .child(
                                div()
                                    .flex()
                                    .justify_end()
                                    .gap(px(8.))
                                    .child(
                                        self.b("Cancel", Action::Confirm(false), t, cx)
                                            .track_focus(&self.confirm_focus[1]),
                                    )
                                    .child(
                                        controls::action_button(
                                            "confirm-settings-action",
                                            accept,
                                            ButtonTone::Danger,
                                            false,
                                            t,
                                            move |w, cx| {
                                                let _ = weak.update(cx, |v, cx| {
                                                    v.action(Action::Confirm(true), w, cx)
                                                });
                                            },
                                        )
                                        .track_focus(&self.confirm_focus[0]),
                                    ),
                            ),
                    ),
            );
        }
        root.into_any_element()
    }
}
impl SettingsContent {
    pub(super) fn key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        if e.is_held {
            return;
        }
        let key = e.keystroke.key.as_str();
        if self.confirmation.is_some() {
            if key == "tab" {
                controls::FocusScope::cycle(
                    &self.confirm_focus,
                    e.keystroke.modifiers.shift,
                    w,
                    cx,
                );
                cx.stop_propagation();
            } else if key == "escape" {
                self.action(Action::Confirm(false), w, cx);
                cx.stop_propagation();
            }
            return;
        }
        if self.dialog.is_some() {
            if key == "escape" {
                self.action(Action::Dialog(None), w, cx);
                cx.stop_propagation();
                return;
            }
            if key == "tab" {
                // Skip background tab stops when wrapping, including a parent window's controls.
                for _ in 0..1024 {
                    if e.keystroke.modifiers.shift {
                        w.focus_prev(cx)
                    } else {
                        w.focus_next(cx)
                    }
                    if self.dialog_focus.contains_focused(w, cx) {
                        break;
                    }
                }
                cx.stop_propagation();
                return;
            }
        }
        if let Some(command) = self.command_hotkey_recording.clone() {
            cx.stop_propagation();
            if key == "escape" {
                self.command_hotkey_recording = None;
                cx.notify();
                return;
            }
            let shortcut = if matches!(key, "backspace" | "delete") {
                Some(String::new())
            } else {
                accelerator(&e.keystroke)
            };
            if let Some(shortcut) = shortcut {
                self.command_hotkey_recording = None;
                self.action(
                    Action::SetField(format!("command.hotkey.{command}"), shortcut),
                    w,
                    cx,
                );
                self.action(Action::CommandHotkey(command), w, cx);
            }
            return;
        }
        if self.hotkey_recording {
            cx.stop_propagation();
            if key == "escape" {
                self.hotkey_recording = false;
                cx.notify();
                return;
            }
            let value = if matches!(key, "backspace" | "delete") {
                Some(
                    if cfg!(windows) {
                        "Control+Space"
                    } else {
                        "Alt+Space"
                    }
                    .to_owned(),
                )
            } else {
                accelerator(&e.keystroke)
            };
            if let Some(v) = value {
                self.hotkey_recording = false;
                self.queue(
                    vec![Edit::Global {
                        field: "raymesHotkey".into(),
                        value: json!(v),
                    }],
                    cx,
                )
            }
            return;
        }
        if key == "tab" {
            if e.keystroke.modifiers.shift {
                w.focus_prev(cx)
            } else {
                w.focus_next(cx)
            }
            cx.stop_propagation();
        } else if key == "escape" {
            if self.menu.take().is_some() {
                cx.notify();
            } else if !self.embedded {
                self.action(Action::Close, w, cx);
            }
            cx.stop_propagation();
        } else if key == "enter" && w.focused(cx).as_ref() == Some(&self.focus) {
            self.action(Action::Save, w, cx);
            cx.stop_propagation();
        } else if matches!(key, "left" | "right" | "home" | "end")
            && self.tabs.iter().any(|f| f.is_focused(w))
        {
            let current = Tab::ALL.iter().position(|t| *t == self.tab).unwrap();
            let i = match key {
                "home" => 0,
                "end" => 7,
                "left" => (current + 7) % 8,
                _ => (current + 1) % 8,
            };
            self.select_tab(Tab::ALL[i], w, cx);
            cx.stop_propagation();
        }
    }
}
pub(super) fn tab_icon(tab: Tab) -> Svg {
    let data:&'static [u8]=match tab{
        Tab::Ai=>br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3l1.7 5.1L19 10l-5.3 1.9L12 17l-1.7-5.1L5 10l5.3-1.9L12 3Z M19 15l.8 2.2L22 18l-2.2.8L19 21l-.8-2.2L16 18l2.2-.8L19 15Z"/></svg>"#,
        Tab::Voice=>br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round"><path d="M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3Z M5 11a7 7 0 0 0 14 0 M12 18v3"/></svg>"#,
        Tab::Knowledge=>br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linejoin="round"><path d="M4 5.5A2.5 2.5 0 0 1 6.5 3H11v16H6.5A2.5 2.5 0 0 0 4 21.5v-16Z M20 5.5A2.5 2.5 0 0 0 17.5 3H13v16h4.5a2.5 2.5 0 0 1 2.5 2.5v-16Z"/></svg>"#,
        Tab::Extensions=>br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linejoin="round"><path d="M8 3h3a2 2 0 1 1 4 0h3v5a2 2 0 1 1 0 4v5h-5a2 2 0 1 1-4 0H4v-5a2 2 0 1 1 0-4V3h4Z"/></svg>"#,
        Tab::Permissions=>br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round"><rect x="5" y="10" width="14" height="10" rx="2"/><path d="M8 10V7a4 4 0 0 1 8 0v3"/></svg>"#,
        Tab::Storage=>br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2"><ellipse cx="12" cy="6" rx="9" ry="3"/><path d="M3 6v12c0 1.7 4 3 9 3s9-1.3 9-3V6 M3 12c0 1.7 4 3 9 3s9-1.3 9-3"/></svg>"#,
        Tab::Advanced=>br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linejoin="round"><path d="M14.7 6.3a4 4 0 0 0-5 5L4 17v3h3l5.7-5.7a4 4 0 0 0 5-5l-2.4 2.4-3-3 2.4-2.4Z"/></svg>"#,
        Tab::General=>br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.32 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.32 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831 2.34 2.34 0 0 1 2.33-4.033 2.34 2.34 0 0 0 3.32-1.915Z"/><circle cx="12" cy="12" r="3"/></svg>"#,
    };
    svg().data(data)
}
pub(crate) fn accelerator(k: &Keystroke) -> Option<String> {
    let mut parts = vec![];
    if k.modifiers.platform {
        parts.push("Command".to_owned())
    }
    if k.modifiers.control {
        parts.push("Control".into())
    }
    if k.modifiers.alt {
        parts.push("Alt".into())
    }
    if k.modifiers.shift {
        parts.push("Shift".into())
    }
    let function_key = k
        .key
        .strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n) && k.key == format!("f{n}"));
    if parts.is_empty() && !function_key {
        return None;
    }
    if matches!(k.key.as_str(), "shift" | "control" | "alt" | "cmd") {
        return None;
    }
    parts.push(match k.key.as_str() {
        "space" => "Space".into(),
        "enter" => "Return".into(),
        v => v.to_uppercase(),
    });
    Some(parts.join("+"))
}
pub(super) fn update_label(s: &tezbar_update::AppUpdateStatus) -> String {
    use tezbar_update::AppUpdateStatus::*;
    match s {
        Idle => "Ready to check for updates".into(),
        Checking => "Checking for updates…".into(),
        UpToDate { version } => format!("Up to date · {version}"),
        Available { version, .. } => format!("Update available · {version}"),
        ManualAvailable { version, .. } => format!("Installer available · {version}"),
        Downloading {
            downloaded, total, ..
        } => format!("Downloading update… {downloaded} / {}", total.unwrap_or(0)),
        Ready { .. } => "Restart to update".into(),
        Error { .. } => "Could not check for updates. Retry.".into(),
    }
}
pub(super) fn format_device(s: &DevicePhase) -> String {
    match s {
        DevicePhase::Waiting { code, .. } => format!("Enter code {code} in your browser"),
        DevicePhase::Saving => "Saving provider before sign-in…".into(),
        DevicePhase::Starting | DevicePhase::Polling => "Waiting for GitHub…".into(),
        DevicePhase::Complete => "Signed in".into(),
        DevicePhase::Failed => "Sign-in failed. Retry.".into(),
        DevicePhase::Cancelled => "Sign-in cancelled".into(),
        DevicePhase::Idle => "Sign in to GitHub Copilot".into(),
    }
}
pub(super) fn safe_display(v: &Value) -> String {
    fn lines(v: &Value, path: &str, literal_keys: bool, out: &mut Vec<String>) {
        match v {
            Value::Object(map) => {
                for (key, value) in map {
                    let label = if literal_keys {
                        key.to_owned()
                    } else {
                        display_name(key)
                    };
                    let path = if path.is_empty() {
                        label
                    } else {
                        format!("{path} · {label}")
                    };
                    lines(value, &path, matches!(key.as_str(), "env" | "headers"), out);
                }
            }
            Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    lines(item, &format!("{path} {}", i + 1), literal_keys, out);
                }
            }
            Value::Null => out.push(format!("{path}: Not set")),
            Value::String(value) => out.push(format!("{path}: {value}")),
            Value::Bool(value) => {
                out.push(format!("{path}: {}", if *value { "Yes" } else { "No" }))
            }
            Value::Number(value) => out.push(format!("{path}: {value}")),
        }
    }
    let mut out = vec![];
    lines(v, "", false, &mut out);
    out.join("\n")
}
pub(super) fn display_name(value: &str) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    let mut words = String::new();
    for (i, c) in chars.iter().copied().enumerate() {
        if matches!(c, '_' | '-') {
            words.push(' ');
            continue;
        }
        if i > 0
            && c.is_uppercase()
            && (chars[i - 1].is_lowercase()
                || (chars[i - 1].is_uppercase()
                    && chars.get(i + 1).is_some_and(|c| c.is_lowercase())))
        {
            words.push(' ');
        }
        words.push(c);
    }
    words
        .split_whitespace()
        .map(|word| match word.to_ascii_lowercase().as_str() {
            "url" => "URL".into(),
            "id" => "ID".into(),
            "api" => "API".into(),
            "mcp" => "MCP".into(),
            "json" => "JSON".into(),
            "ui" => "UI".into(),
            _ => {
                let mut chars = word.chars();
                chars
                    .next()
                    .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                    .unwrap_or_default()
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}
pub(super) fn download_progress(done: u64, total: Option<u64>) -> String {
    let done = tezbar_ui_model::utilities::monitor::decimal_bytes(done as f64);
    match total {
        Some(total) if total > 0 => format!(
            "{done} of {}",
            tezbar_ui_model::utilities::monitor::decimal_bytes(total as f64)
        ),
        _ => format!("{done} downloaded"),
    }
}
fn normalize_extension_id(id: &str) -> String {
    id.trim()
        .trim_start_matches('@')
        .strip_prefix("raycast.")
        .unwrap_or(id.trim().trim_start_matches('@'))
        .replace(['\\', '/'], "-")
}
pub(super) fn merge_installed(schema: &Value, installed: &Value) -> Value {
    let mut rows = BTreeMap::new();
    for row in schema.as_array().into_iter().flatten() {
        rows.insert(
            normalize_extension_id(row["extName"].as_str().unwrap_or("")),
            row.clone(),
        );
    }
    for row in installed.as_array().into_iter().flatten() {
        let id = row["slug"].as_str().or(row["id"].as_str()).unwrap_or("");
        let name = normalize_extension_id(id);
        if name.is_empty() || rows.contains_key(&name) {
            continue;
        }
        let commands=row["commands"].as_array().into_iter().flatten().map(|c|json!({"name":c["name"],"title":c["title"],"description":c["description"],"mode":c["mode"],"preferences":[]})).collect::<Vec<_>>();
        rows.insert(name.clone(),json!({"extName":name,"extensionId":row["id"],"title":row["name"].as_str().unwrap_or(&name),"description":row["description"],"preferences":[],"commands":commands}));
    }
    let mut rows = rows.into_values().collect::<Vec<_>>();
    rows.sort_by(|a, b| text(&a["title"]).cmp(&text(&b["title"])));
    json!(rows)
}
