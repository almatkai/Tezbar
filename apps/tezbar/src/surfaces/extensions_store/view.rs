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
        let installed = self.catalog.installed.iter().any(|v| v.id == e.id);
        let subtitle = if installed {
            "Installed".to_owned()
        } else {
            e.runtime_description(platform())
                .map(|(label, _)| label.to_owned())
                .unwrap_or_else(|| e.author.clone())
        };
        self.button(
            format!("store-row-{}", e.id),
            String::new(),
            Action::Select(e.id.clone()),
            t,
            cx,
        )
        .aria_label(e.name.clone())
        .flex()
        .items_center()
        .role(Role::ListBoxOption)
        .aria_selected(selected)
        .h(px(48. * t.zoom))
        .w_full()
        .px(px(8. * t.zoom))
        .gap(px(10. * t.zoom))
        .bg(if selected {
            t.selected
        } else {
            transparent_black()
        })
        .child(if let Some(image) = self.artwork.row_icon(&e.id) {
            div()
                .size(px(28. * t.zoom))
                .rounded(px(7.))
                .overflow_hidden()
                .child(img(image).size_full().object_fit(ObjectFit::Cover))
        } else {
            div()
                .size(px(28. * t.zoom))
                .rounded(px(7.))
                .bg(t.pill)
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(15.))
                .child(e.name.chars().next().unwrap_or(' ').to_string())
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .truncate()
                        .text_size(px(12. * t.zoom))
                        .child(e.name.clone()),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(10.5 * t.zoom))
                        .text_color(t.text4)
                        .child(subtitle),
                ),
        )
        .into_any_element()
    }
    fn detail(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        if github_url(&self.catalog.query) {
            let repository = self
                .catalog
                .query
                .trim()
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("");
            return div().flex().flex_col().justify_center().h_full().px(px(32. * t.zoom)).py(px(24. * t.zoom)).gap(px(12.))
                .child(div().text_size(px(11.)).text_color(t.text4).child("Install from GitHub"))
                .child(div().text_size(px(20. * t.zoom)).child(repository.to_owned()))
                .child(div().text_size(px(13.)).text_color(t.text3).child(self.catalog.query.clone()))
                .child(div().mt(px(12.)).text_size(px(13.)).text_color(t.text2)
                    .child("Tezbar will download this public repository, validate its package manifest and install the extension."))
                .child(self.button("store-install-github-detail".into(), "Install extension".into(), Action::Install(self.catalog.query.clone()), t, cx))
                .into_any_element();
        }
        let Some(e) = self.selected() else {
            return div()
                .p(px(24.))
                .text_color(t.text3)
                .child("Choose an extension")
                .into_any_element();
        };
        let installed = self.catalog.installed.iter().any(|v| v.id == e.id);
        let mut d = div()
            .flex()
            .flex_col()
            .gap(px(12. * t.zoom))
            .p(px(20. * t.zoom))
            // Body copy size; rows without their own size used to inherit the 16px default.
            .text_size(px(12. * t.zoom));
        let icon = div()
            .size(px(56. * t.zoom))
            .rounded(px(14.))
            .overflow_hidden()
            .border_1()
            .border_color(t.hairline)
            .bg(t.pill)
            .flex()
            .items_center()
            .justify_center();
        let icon = if let Some(image) = self.artwork.icon().or_else(|| self.artwork.row_icon(&e.id))
        {
            icon.child(img(image).size_full().object_fit(ObjectFit::Cover))
        } else {
            icon.child(
                e.name
                    .chars()
                    .next()
                    .unwrap_or(' ')
                    .to_uppercase()
                    .to_string(),
            )
        };
        d = d
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12. * t.zoom))
                    .child(icon)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div()
                                    .id("store-extension-title")
                                    .role(Role::Heading)
                                    .text_size(px(20. * t.zoom))
                                    .child(e.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11. * t.zoom))
                                    .text_color(t.text4)
                                    .child(format!("{} · {}", e.author, e.version)),
                            ),
                    ),
            )
            .child(self.detail.clone());
        if let Some((label, description)) = e.runtime_description(platform()) {
            d = d.child(
                div()
                    .child(label)
                    .child(div().text_color(t.text3).child(description)),
            );
        }
        if e.effective_runtime(platform()) == Some("rust") {
            d = d.child(
                div()
                    .rounded(px(10.))
                    .border_1()
                    .border_color(t.hairline)
                    .bg(t.bar)
                    .p(px(14.))
                    .child(div().text_size(px(12.)).child("Native Rust Extension"))
                    .child(
                        div()
                            .mt(px(4.))
                            .text_size(px(11.))
                            .text_color(t.text3)
                            .child("Executes in-process with zero latency, low memory footprint, and no external runtime dependency."),
                    ),
            );
        }
        d = d.child(
            div()
                .border_t_1()
                .border_color(t.hairline)
                .pt(px(12.))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(t.text4)
                        .child("Extension ID"),
                )
                .child(
                    div()
                        .mt(px(4.))
                        .text_size(px(12.))
                        .font_family(tezbar_ui::tokens::mono_family())
                        .text_color(t.text2)
                        .child(e.id.clone()),
                ),
        );
        for key in ["platforms", "resources"] {
            if let Some(value) = e.metadata.get(key) {
                d = d.child(div().text_color(t.text3).child(format!(
                        "{}: {}",
                        if key=="platforms" {"Available on"} else {"Includes"},
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
        for (i, command) in e.commands().into_iter().enumerate() {
            let name = command["name"].as_str().unwrap_or("");
            let title = command["title"].as_str().unwrap_or(name);
            d = d.child(
                div()
                    .rounded(px(7.))
                    .bg(t.bar)
                    .px(px(12.))
                    .py(px(8.))
                    .child(div().text_size(px(12.)).child(title.to_owned()))
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
        if e.metadata
            .get("screenshotUrls")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
        {
            d = d.child(
                div()
                    .text_size(px(11.))
                    .text_color(t.text4)
                    .child("No screenshots available."),
            );
        }
        let mut screenshots = div().flex().flex_wrap().gap(px(12. * t.zoom));
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
            let tile = self
                .button(
                    format!("store-screenshot-{i}"),
                    String::new(),
                    Action::Open(url.into()),
                    t,
                    cx,
                )
                .aria_label(format!("Open screenshot {}", i + 1))
                .w(relative(0.48))
                .h(px(150. * t.zoom))
                .p_0()
                .rounded(px(8.))
                .border_1()
                .border_color(t.hairline)
                .bg(t.bar)
                .overflow_hidden()
                .flex()
                .items_center()
                .justify_center();
            screenshots = screenshots.child(if let Some(image) = self.artwork.image(i) {
                tile.child(img(image).size_full().object_fit(ObjectFit::Contain))
            } else {
                tile.child(
                    div()
                        .text_size(px(11.))
                        .text_color(t.text4)
                        .child("Loading screenshot…"),
                )
            });
        }
        d = d.child(screenshots);
        if let Some(url) = e
            .metadata
            .get("readmeUrl")
            .or_else(|| e.metadata.get("repository"))
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
        for key in ["sourceUrl", "website"] {
            if let Some(url) = e.metadata.get(key).and_then(Value::as_str) {
                d = d.child(self.button(
                    format!("store-link-{key}"),
                    if key == "readmeUrl" {
                        "Open README".into()
                    } else {
                        if key == "sourceUrl" {
                            "Source code".into()
                        } else {
                            "Website".into()
                        }
                    },
                    Action::Open(url.into()),
                    t,
                    cx,
                ));
            }
        }
        d.into_any_element()
    }
    fn detail_footer(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        if github_url(&self.catalog.query) {
            return div().into_any_element();
        }
        let Some(e) = self.selected() else {
            return div().into_any_element();
        };
        let installed = self.catalog.installed.iter().any(|v| v.id == e.id);
        let mut d = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.))
            .border_t_1()
            .border_color(t.hairline)
            .px(px(16.))
            .py(px(12.))
            .child(
                div()
                    .flex_1()
                    .text_size(px(11.))
                    .text_color(t.text4)
                    .child(e.version.clone()),
            );
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
            .child(controls::back_button(
                self.button("store-back".into(), "Back".into(), Action::Back, t, cx),
                t,
            ))
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
            .w(relative(0.34))
            .min_w(px(160. * t.zoom))
            .min_h_0()
            .px(px(2.));
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
                cx.processor(move |v, range: std::ops::Range<usize>, w, cx| {
                    v.schedule_visible_range(range.clone(), w, cx);
                    v.schedule_row_icons(range.clone(), w, cx);
                    range.map(|i| v.row(i, t, cx)).collect()
                }),
            )
            .track_scroll(&self.scroll.clone())
            .flex_1(),
        );
        if self.catalog.loading_more {
            list = list.child(
                div()
                    .id("store-loading-more")
                    .p(px(8.))
                    .role(Role::Status)
                    .text_color(t.text3)
                    .child("Loading more extensions…"),
            );
        } else if self.catalog.has_more && self.catalog.pagination_failed {
            list = list.child(self.button(
                "store-more".into(),
                "Retry loading more".into(),
                Action::More,
                t,
                cx,
            ));
        }
        let message = self
            .error
            .clone()
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
            .font_family(tezbar_ui::tokens::font_family(cx, false))
            .relative()
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
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap(px(12.))
                    .p(px(8.))
                    .child(list)
                    .child(
                        div()
                            .id("store-detail")
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .rounded(px(tokens::ROW_RADIUS))
                            .border_1()
                            .border_color(t.hairline)
                            .bg(t.bar)
                            .child(
                                div()
                                    .id("store-detail-content")
                                    .flex_1()
                                    .overflow_y_scroll()
                                    .child(self.detail(t, cx)),
                            )
                            .child(self.detail_footer(t, cx)),
                    ),
            )
            .child(
                controls::footer(t)
                    .when_some(self.footer.clone(), |d, footer| d.child(footer))
                    .child(
                        div()
                            .id("store-status")
                            .flex_1()
                            .role(Role::Status)
                            .child(message),
                    )
                    .child(self.button(
                        "store-settings".into(),
                        "Extension Settings".into(),
                        Action::Settings,
                        t,
                        cx,
                    )),
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
