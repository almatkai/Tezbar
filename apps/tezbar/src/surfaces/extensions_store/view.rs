use super::*;
use tezbar_ui::{
    controls::{self, ButtonTone},
    tokens::{self, Tokens},
};
#[derive(Clone)]
pub(super) enum Action {
    Settings,
    Back,
    Refresh,
    More,
    Select(String),
    Install(String),
    Remove(String),
    ConfirmRemove,
    CancelRemove,
    Open(String),
    Run(String, String),
}
impl StoreView {
    pub(super) fn action(&mut self, a: Action, w: &mut Window, cx: &mut Context<Self>) {
        if self.remove.is_some() && !matches!(a, Action::CancelRemove | Action::ConfirmRemove) {
            return;
        }
        match a {
            Action::Settings => self.emit_effect(StoreEffect::Settings, cx),
            Action::Back => self.emit_effect(StoreEffect::Back, cx),
            Action::Refresh => self.reload(cx),
            Action::More => self.more(cx),
            Action::Select(id) => self.select(id, cx),
            Action::Install(id) => self.install(id, cx),
            Action::Remove(id) => {
                self.remove = Some(id);
                self.cancel_focus.focus(w, cx);
            }
            Action::CancelRemove => {
                self.remove = None;
                self.search.focus_handle(cx).focus(w, cx);
            }
            Action::ConfirmRemove => {
                if let Some(id) = self.remove.take() {
                    self.mutate(id, true, cx);
                    self.search.focus_handle(cx).focus(w, cx);
                }
            }
            Action::Open(url) => {
                if let Some(url) = tezbar_ui::markdown::safe_link(&url) {
                    self.quiet(ExtensionRequest::Open(url), cx);
                }
            }
            Action::Run(id, cmd) => self.run(id, cmd, cx),
        }
        cx.notify();
    }
    fn button(
        &self,
        id: String,
        label: String,
        a: Action,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let owner = cx.entity().downgrade();
        controls::action_button(id, label, ButtonTone::Quiet, false, t, move |w, cx| {
            let _ = owner.update(cx, |v, cx| v.action(a.clone(), w, cx));
        })
    }
    fn row(&self, index: usize, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let e = &self.rows[index];
        let selected = self.catalog.selected.as_ref() == Some(&e.id);
        self.button(
            format!("store-row-{}", e.id),
            e.name.clone(),
            Action::Select(e.id.clone()),
            t,
            cx,
        )
        .role(Role::ListBoxOption)
        .aria_selected(selected)
        .h(px(tokens::ROW * t.zoom))
        .w_full()
        .bg(if selected {
            t.selected
        } else {
            transparent_black()
        })
        .into_any_element()
    }
    fn detail(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let Some(e) = self.selected() else {
            return div()
                .p(px(24.))
                .text_color(t.text3)
                .child("Choose an extension")
                .into_any_element();
        };
        let installed = self.catalog.installed.iter().any(|v| v.id == e.id);
        let mut d = div().flex().flex_col().gap(px(12.)).p(px(16.));
        d = d
            .child(
                div()
                    .id("store-extension-title")
                    .role(Role::Heading)
                    .text_size(px(15.))
                    .child(e.name.clone()),
            )
            .child(
                div()
                    .text_color(t.text3)
                    .child(format!("{} · {}", e.author, e.version)),
            )
            .child(self.detail.clone())
            .child(div().text_color(t.text3).child(e.id.clone()));
        if let Some((label, description)) = e.runtime_description(platform()) {
            d = d.child(
                div()
                    .child(label)
                    .child(div().text_color(t.text3).child(description)),
            );
        }
        if e.effective_runtime(platform()) == Some("rust") {
            d = d.child(div().child("Native Rust Extension"));
        }
        for key in ["platforms", "resources"] {
            if let Some(value) = e.metadata.get(key) {
                d = d.child(div().text_color(t.text3).child(format!(
                        "{key}: {}",
                        value
                            .as_array()
                            .map(|a| a
                                .iter()
                                .map(|v| v.as_str().unwrap_or(""))
                                .collect::<Vec<_>>()
                                .join(", "))
                            .unwrap_or_else(|| value.as_str().unwrap_or("").into())
                    )));
            }
        }
        for line in e.checked_lines(platform(), |stamp| format!("{stamp}")) {
            d = d.child(div().text_color(t.text3).child(line));
        }
        if let Some(guidance) = e.acquisition_guidance() {
            d = d.child(div().text_color(t.text2).child(guidance));
        }
        if let Some(progress) = self.catalog.installing.get(&e.id) {
            d = d.child(
                div()
                    .id("store-install-progress")
                    .role(Role::ProgressIndicator)
                    .child(format!("Installing… {progress}%")),
            );
        } else if installed {
            d = d.child(self.button(
                "store-uninstall".into(),
                "Remove extension".into(),
                Action::Remove(e.id.clone()),
                t,
                cx,
            ));
        } else if e.can_install(platform()) {
            d = d.child(self.button(
                "store-install".into(),
                "Install extension".into(),
                Action::Install(e.id.clone()),
                t,
                cx,
            ));
        }
        for (i, command) in e.commands().into_iter().enumerate() {
            let name = command["name"].as_str().unwrap_or("");
            let title = command["title"].as_str().unwrap_or(name);
            d = d.child(
                div()
                    .child(title.to_owned())
                    .child(div().text_color(t.text3).child(
                        tezbar_ui_model::extensions::catalog::command_status(&command, platform()),
                    ))
                    .when(installed, |d| {
                        d.child(self.button(
                            format!("store-command-{i}"),
                            "Run".into(),
                            Action::Run(e.id.clone(), name.into()),
                            t,
                            cx,
                        ))
                    }),
            );
        }
        for (i, url) in e
            .metadata
            .get("screenshotUrls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .take(4)
            .filter_map(Value::as_str)
            .enumerate()
        {
            if let Some(image) = self.artwork.image(i) {
                d = d.child(
                    img(image)
                        .w_full()
                        .max_h(px(240. * t.zoom))
                        .object_fit(ObjectFit::Contain),
                );
            }
            d = d.child(self.button(
                format!("store-screenshot-{i}"),
                format!("Open screenshot {}", i + 1),
                Action::Open(url.into()),
                t,
                cx,
            ));
        }
        if let Some(url) = e
            .metadata
            .get("repository")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                e.id.strip_prefix("raycast.").map(|slug| {
                    format!("https://github.com/raycast/extensions/tree/main/extensions/{slug}")
                })
            })
        {
            d = d.child(self.button(
                "store-readme".into(),
                "Open README".into(),
                Action::Open(url),
                t,
                cx,
            ));
        }
        for key in ["readmeUrl", "sourceUrl", "website"] {
            if let Some(url) = e.metadata.get(key).and_then(Value::as_str) {
                d = d.child(self.button(
                    format!("store-link-{key}"),
                    if key == "readmeUrl" {
                        "Open README".into()
                    } else {
                        key.into()
                    },
                    Action::Open(url.into()),
                    t,
                    cx,
                ));
            }
        }
        d.into_any_element()
    }
    fn bound_key(&mut self, key: &str, w: &mut Window, cx: &mut Context<Self>) {
        cx.propagate();
        self.key(
            &KeyDownEvent {
                keystroke: Keystroke {
                    key: key.into(),
                    key_char: None,
                    modifiers: w.modifiers(),
                },
                is_held: false,
                prefer_character_input: false,
            },
            w,
            cx,
        );
    }
    fn key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        if self.remove.is_some() {
            match e.keystroke.key.as_str() {
                "escape" if !e.is_held => {
                    self.action(Action::CancelRemove, w, cx);
                    cx.stop_propagation();
                }
                "enter" if !e.is_held => {
                    if self.cancel_focus.is_focused(w) {
                        self.action(Action::CancelRemove, w, cx);
                    } else if self.confirm_focus.is_focused(w) {
                        self.action(Action::ConfirmRemove, w, cx);
                    }
                    cx.stop_propagation();
                }
                "tab" => {
                    controls::FocusScope::cycle(
                        &[self.cancel_focus.clone(), self.confirm_focus.clone()],
                        e.keystroke.modifiers.shift,
                        w,
                        cx,
                    );
                    cx.stop_propagation();
                }
                _ => {}
            }
            return;
        }
        if self.search.read(cx).marked_range.is_some() {
            return;
        }
        match e.keystroke.key.as_str() {
            "escape" if !e.is_held => {
                self.action(
                    if self.remove.is_some() {
                        Action::CancelRemove
                    } else {
                        Action::Back
                    },
                    w,
                    cx,
                );
                cx.stop_propagation();
            }
            "up" | "down" => {
                let current = self
                    .rows
                    .iter()
                    .position(|r| Some(&r.id) == self.catalog.selected.as_ref())
                    .unwrap_or(0);
                let next = if e.keystroke.key == "up" {
                    current.saturating_sub(1)
                } else {
                    (current + 1).min(self.rows.len().saturating_sub(1))
                };
                if let Some(row) = self.rows.get(next) {
                    self.select(row.id.clone(), cx);
                    self.scroll.scroll_to_item(next, ScrollStrategy::Nearest);
                    if next + 3 >= self.rows.len() {
                        self.more(cx);
                    }
                }
                cx.stop_propagation();
            }
            "enter" if !e.is_held => {
                if self.remove.is_some() {
                    self.action(Action::ConfirmRemove, w, cx);
                } else if github_url(&self.catalog.query) {
                    self.install(self.catalog.query.clone(), cx);
                }
                cx.stop_propagation();
            }
            _ => {}
        }
    }
}
impl Render for StoreView {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::tokens(w);
        let header = controls::header(t)
            .gap(px(8.))
            .child(self.button("store-back".into(), "Back".into(), Action::Back, t, cx))
            .child(div().flex_1().child(self.search.clone()))
            .child(self.button(
                "store-refresh".into(),
                "Refresh".into(),
                Action::Refresh,
                t,
                cx,
            ));
        let mut list = div()
            .flex()
            .flex_col()
            .w(relative(0.42))
            .min_h_0()
            .border_r_1()
            .border_color(t.hairline);
        if self.catalog.loading {
            list = list.child(
                div()
                    .id("store-loading-status")
                    .p(px(12.))
                    .role(Role::Status)
                    .text_color(t.text3)
                    .child("Loading extensions…"),
            );
        }
        if github_url(&self.catalog.query) {
            list = list.child(self.button(
                "store-install-github".into(),
                "Install from GitHub".into(),
                Action::Install(self.catalog.query.clone()),
                t,
                cx,
            ));
        } else if self.rows.is_empty() && !self.catalog.loading {
            list = list.child(
                div()
                    .p(px(12.))
                    .text_color(t.text3)
                    .child("No matching extensions"),
            );
        }
        list = list.child(
            uniform_list(
                "extension-store-items",
                self.rows.len(),
                cx.processor(move |v, range: std::ops::Range<usize>, _, cx| {
                    range.map(|i| v.row(i, t, cx)).collect()
                }),
            )
            .track_scroll(&self.scroll.clone())
            .flex_1(),
        );
        if self.catalog.has_more {
            list = list.child(
                self.button(
                    "store-more".into(),
                    if self.catalog.loading_more {
                        "Loading…"
                    } else {
                        "Load more"
                    }
                    .into(),
                    Action::More,
                    t,
                    cx,
                ),
            );
        }
        let message = self
            .error
            .map(str::to_owned)
            .or_else(|| self.catalog.message.clone())
            .unwrap_or_else(|| {
                format!(
                    "{} extensions · {}",
                    self.catalog.total,
                    if cfg!(windows) {
                        "Tezbar-loved"
                    } else {
                        "Raycast catalog"
                    }
                )
            });
        controls::sheet(t)
            .id("extensions-store")
            .track_focus(&self.focus)
            .capture_action(cx.listener(|v, _: &tezbar_ui::text_input::Newline, w, cx| {
                v.bound_key("enter", w, cx)
            }))
            .capture_action(
                cx.listener(|v, _: &tezbar_ui::text_input::Up, w, cx| v.bound_key("up", w, cx)),
            )
            .capture_action(
                cx.listener(|v, _: &tezbar_ui::text_input::Down, w, cx| v.bound_key("down", w, cx)),
            )
            .capture_key_down(cx.listener(|v, e, w, cx| v.key(e, w, cx)))
            .child(header)
            .child(self.button(
                "store-settings".into(),
                "Installed Extension Settings".into(),
                Action::Settings,
                t,
                cx,
            ))
            .child(
                div().flex().flex_1().min_h_0().child(list).child(
                    div()
                        .id("store-detail")
                        .flex_1()
                        .overflow_y_scroll()
                        .child(self.detail(t, cx)),
                ),
            )
            .child(
                controls::footer(t)
                    .when_some(self.footer.clone(), |d, footer| d.child(footer))
                    .child(div().id("store-status").role(Role::Status).child(message)),
            )
            .when(self.remove.is_some(), |d| {
                d.child(
                    controls::popover("store-confirm-remove", "Remove extension?", t)
                        .absolute()
                        .top(px(40.))
                        .bottom(px(40.))
                        .left(px(40.))
                        .right(px(40.))
                        .child("Remove this extension and its commands?")
                        .child(
                            self.button(
                                "store-cancel-remove".into(),
                                "Cancel".into(),
                                Action::CancelRemove,
                                t,
                                cx,
                            )
                            .track_focus(&self.cancel_focus),
                        )
                        .child(
                            self.button(
                                "store-confirm-remove-button".into(),
                                "Remove".into(),
                                Action::ConfirmRemove,
                                t,
                                cx,
                            )
                            .text_color(t.danger)
                            .track_focus(&self.confirm_focus),
                        ),
                )
            })
    }
}
