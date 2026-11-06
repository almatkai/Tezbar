use super::*;
use controls::ButtonTone;
impl Utilities {
    fn list_content(&self, id: &str, label: &str, t: tokens::Tokens) -> AnyElement {
        if self.surface == Surface::Clipboard {
            let pinned = self
                .clipboard
                .visible()
                .iter()
                .any(|e| e.id == id && e.pinned);
            return div()
                .flex()
                .w_full()
                .min_w_0()
                .gap(px(8.))
                .child(div().flex_1().truncate().child(label.to_owned()))
                .when(pinned, |d| {
                    d.child(div().id(SharedString::from(format!("pinned-{id}"))).aria_label("Pinned").text_color(t.text3).child("⌖"))
                })
                .into_any_element();
        }
        if self.surface == Surface::Notes {
            if let Some(note) = id
                .parse::<u64>()
                .ok()
                .and_then(|id| self.note_labels.get(&id))
            {
                return div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .gap(px(2.))
                    .child(div().truncate().child(note.title.clone()))
                    .child(
                        div()
                            .text_size(px(11. * t.zoom))
                            .text_color(t.text3)
                            .truncate()
                            .child(note.summary.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10. * t.zoom))
                            .text_color(t.text3)
                            .child(note.date.clone()),
                    )
                    .into_any_element();
            }
        } else if self.surface == Surface::SystemMonitor {
            if let Some(row) = self.monitor_display.iter().find(|row| row.id == id) {
                let mut content = div().flex().flex_col().w_full().gap(px(4.)).child(
                    div()
                        .flex()
                        .justify_between()
                        .gap(px(8.))
                        .child(format!("{} · {}", row.section, row.label))
                        .child(row.value.clone()),
                );
                if let Some(percent) = row.percent {
                    let color = if percent >= 85. {
                        t.danger
                    } else if percent >= 65. {
                        t.warning
                    } else {
                        t.success
                    };
                    content = content.child(
                        div()
                            .id(SharedString::from(format!("monitor-meter-{}", row.id)))
                            .role(Role::ProgressIndicator)
                            .aria_label(format!("{}: {percent:.0}%", row.label))
                            .h(px(3.))
                            .w_full()
                            .bg(t.hairline)
                            .child(
                                div()
                                    .h_full()
                                    .w(relative((percent / 100.) as f32))
                                    .bg(color),
                            ),
                    );
                }
                return content.into_any_element();
            }
        }
        div().child(label.to_owned()).into_any_element()
    }
    pub fn act(&mut self, a: Action, w: &mut Window, cx: &mut Context<Self>) {
        if !self.visible {
            return;
        }
        match a {
            Action::Refresh => self.refresh(cx),
            Action::Confirm(yes) => {
                if let Some((action, ids)) = self.confirmation.take() {
                    if yes && ids == self.confirmation_keys() {
                        self.dispatch(action, w, cx);
                    }
                }
            }
            Action::Delete | Action::Clear => {
                self.confirmation = Some((a, self.confirmation_keys()))
            }
            Action::Cancel if self.fields.is_empty() => cx.emit(UtilityEffect::Back),
            Action::OpenTask(i) => self.task_action(i, cx),
            Action::OpenTaskId(id) => {
                if let Some(index) = self.background.all().iter().position(|t| t.id == id) {
                    self.task_action(index, cx);
                }
            }
            Action::ReleaseVersion(v) => self.open_release(Some(v), cx),
            _ => self.dispatch(a, w, cx),
        }
        self.sync_selection_preview(cx);
        cx.notify();
    }
    fn dispatch(&mut self, a: Action, w: &mut Window, cx: &mut Context<Self>) {
        match self.surface {
            Surface::Clipboard => self.clipboard_action(a, cx),
            Surface::Snippets => self.snippet_action(a, w, cx),
            Surface::Notes => self.note_action(a, cx),
            Surface::Vault => self.vault_action(a, w, cx),
            Surface::Emoji => self.emoji_action(a, cx),
            Surface::SystemMonitor => self.monitor_action(a, cx),
            Surface::Indexing => self.index_action(a, cx),
            Surface::Permissions => self.permission_action(a, cx),
            _ => {}
        }
    }
    fn confirmation_keys(&self) -> Vec<String> {
        let mut ids = self
            .rows()
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        ids.push(format!("selection:{}", self.selected()));
        ids
    }
    pub(super) fn rows(&self) -> Vec<(String, String)> {
        match self.surface {
            Surface::Clipboard => self.clipboard_rows(),
            Surface::Snippets => self
                .snippets
                .visible()
                .iter()
                .map(|r| {
                    (
                        r.id.clone(),
                        format!(
                            "{} · {}{}",
                            r.title,
                            r.trigger,
                            if r.readonly { " · Built-in" } else { "" }
                        ),
                    )
                })
                .collect(),
            Surface::Notes => self
                .notes
                .visible()
                .iter()
                .map(|r| {
                    (
                        r.created_at.to_string(),
                        self.note_labels
                            .get(&r.created_at)
                            .map(|label| label.title.clone())
                            .unwrap_or_else(|| "Note".into()),
                    )
                })
                .collect(),
            Surface::Vault => self
                .vault
                .visible()
                .iter()
                .map(|r| (r.id.clone(), r.name.clone()))
                .collect(),
            Surface::Emoji => self
                .emoji
                .visible()
                .iter()
                .enumerate()
                .map(|(i, s)| (format!("emoji-{i}-{s}"), s.to_string()))
                .collect(),
            Surface::Indexing => self
                .indexing
                .page
                .as_ref()
                .map(|p| {
                    p.sources
                        .iter()
                        .map(|s| (s.id.clone(), format!("{} · {}", s.title, s.status)))
                        .collect()
                })
                .unwrap_or_default(),
            Surface::SystemMonitor => self.monitor_rows(),
            _ => vec![],
        }
    }
    fn selected(&self) -> usize {
        match self.surface {
            Surface::Clipboard => self.clipboard.selected,
            Surface::Snippets => self.snippets.selected,
            Surface::Notes => self
                .notes
                .visible()
                .iter()
                .position(|n| Some(n.created_at) == self.notes.selected)
                .unwrap_or(0),
            Surface::Vault => self.vault.selected,
            Surface::Emoji => self.emoji.selected,
            Surface::Indexing => self.indexing.selected,
            _ => 0,
        }
    }
    fn hover(
        &mut self,
        index: usize,
        event: &MouseMoveEvent,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.pressed_button.is_none() && self.pointer != Some(event.position) {
            self.pointer = Some(event.position);
            if self.selected() != index {
                self.act(Action::Select(index), w, cx);
            }
        }
    }
    fn key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        if self.vault.icon_open {
            self.icon_key(e, w, cx);
            return;
        }
        if self.search.read(cx).marked_range.is_some() || self.editor.read(cx).composing(cx) {
            cx.propagate();
            return;
        }
        let key = e.keystroke.key.as_str();
        let command = e.keystroke.modifiers.platform;
        if key == "escape" {
            if self.confirmation.is_some() {
                self.confirmation = None;
            } else if !self.fields.is_empty() {
                self.dispatch(Action::Cancel, w, cx);
            } else {
                cx.emit(UtilityEffect::Back);
            }
            cx.stop_propagation();
            return;
        }
        if key == "enter" && self.surface == Surface::ReleaseNotes {
            let version = self.search.read(cx).content.trim().to_string();
            if !version.is_empty() {
                self.act(Action::ReleaseVersion(version), w, cx);
            }
            cx.stop_propagation();
            return;
        }
        let in_note_editor = self.editor.focus_handle(cx).contains_focused(w, cx);
        if command {
            let special = match (&self.surface, key) {
                (Surface::Clipboard, "s") => Some(Action::VaultDraft),
                (Surface::Clipboard, "r") => Some(Action::Reveal),
                (Surface::Clipboard, "c") if e.keystroke.modifiers.shift => Some(Action::Clear),
                (Surface::Snippets | Surface::Vault, "e") => Some(Action::Edit),
                (Surface::Snippets | Surface::Vault, "backspace" | "delete") => {
                    Some(Action::Delete)
                }
                (Surface::Vault, "l") => Some(Action::Lock),
                (Surface::Snippets | Surface::Vault, "enter") if !self.fields.is_empty() => {
                    Some(Action::Save)
                }
                (Surface::Indexing, ",") => Some(Action::Settings),
                _ => None,
            };
            if let Some(a) = special {
                self.act(a, w, cx);
                cx.stop_propagation();
                return;
            }
            if matches!(self.surface, Surface::Clipboard | Surface::Snippets) {
                if let Ok(slot) = key.parse::<usize>() {
                    if (1..=9).contains(&slot) && slot <= self.rows().len() {
                        self.act(Action::Select(slot - 1), w, cx);
                        if self.surface == Surface::Snippets {
                            self.act(Action::Activate, w, cx);
                        }
                        cx.stop_propagation();
                        return;
                    }
                }
            }
        }
        if key == "enter" && self.surface == Surface::Vault && !self.vault.model.unlocked {
            self.act(Action::Unlock, w, cx);
            cx.stop_propagation();
            return;
        }
        if key == "enter" && !command && self.surface == Surface::Notes && !in_note_editor {
            self.editor.focus_handle(cx).focus(w, cx);
            cx.stop_propagation();
            return;
        }
        if key == "p"
            && !command
            && self.surface == Surface::Clipboard
            && !self.search.focus_handle(cx).contains_focused(w, cx)
        {
            self.act(Action::Pin, w, cx);
            cx.stop_propagation();
            return;
        }
        let a = match (command, key) {
            (true, "n") => Some(Action::New),
            (true, "d") if !in_note_editor => Some(Action::Delete),
            (true, "r") => Some(Action::Refresh),
            (true, "enter") => Some(Action::Copy),
            (_, "enter") if self.fields.is_empty() => Some(Action::Activate),
            (_, "up" | "down" | "left" | "right")
                if self.fields.is_empty()
                    && !self.editor.focus_handle(cx).contains_focused(w, cx) =>
            {
                let step = match key {
                    "up" if self.surface == Surface::Emoji => -8,
                    "down" if self.surface == Surface::Emoji => 8,
                    "up" | "left" => -1,
                    _ => 1,
                };
                let selected = self
                    .selected()
                    .saturating_add_signed(step)
                    .min(self.rows().len().saturating_sub(1));
                self.scroll.scroll_to_item(
                    if self.surface == Surface::Emoji {
                        selected / 8
                    } else {
                        selected
                    },
                    ScrollStrategy::Nearest,
                );
                Some(Action::Select(selected))
            }
            _ => None,
        };
        if let Some(a) = a {
            self.act(a, w, cx);
            cx.stop_propagation();
        } else {
            cx.propagate();
        }
    }
    fn action_button(
        &self,
        label: impl Into<SharedString>,
        a: Action,
        disabled: bool,
        cx: &mut Context<Self>,
        t: tokens::Tokens,
    ) -> Stateful<Div> {
        let label: SharedString = label.into();
        let accessibility = a.clone();
        let entity = cx.entity().downgrade();
        let keyboard = a.clone();
        controls::button(label.clone(), label, ButtonTone::Ghost, disabled, t)
            .on_key_down(cx.listener(move |this, e: &KeyDownEvent, w, cx| {
                if !disabled && matches!(e.keystroke.key.as_str(), "enter" | "space") {
                    this.act(keyboard.clone(), w, cx);
                    cx.stop_propagation();
                } else {
                    cx.propagate();
                }
            }))
            .on_click(cx.listener(move |this, _, w, cx| {
                if !disabled {
                    this.act(a.clone(), w, cx);
                }
            }))
            .on_a11y_action(AccessibleAction::Click, move |_, w, cx| {
                if !disabled {
                    let _ = entity.update(cx, |this, cx| this.act(accessibility.clone(), w, cx));
                }
            })
    }
    fn status(&self) -> (bool, Option<&str>) {
        match self.surface {
            Surface::Clipboard => (self.clipboard.loading, self.clipboard.error.as_deref()),
            Surface::Snippets => (self.snippets.loading, self.snippets.error.as_deref()),
            Surface::Notes => (self.notes.loading, self.notes.error.as_deref()),
            Surface::Vault => (self.vault.loading, self.vault.error.as_deref()),
            Surface::Indexing => (self.indexing.loading, self.indexing.error.as_deref()),
            Surface::SystemMonitor => (self.monitor.loading, self.monitor.error.as_deref()),
            _ => (false, None),
        }
    }
    fn toolbar(&self, cx: &mut Context<Self>, t: tokens::Tokens) -> Div {
        let empty = self.rows().is_empty();
        let readonly = self
            .snippets
            .visible()
            .get(self.snippets.selected)
            .is_some_and(|s| s.readonly);
        let mut bar = div().flex().flex_wrap().gap(px(4.)).p(px(8.));
        let buttons: Vec<(&'static str, Action, bool)> = match self.surface {
            Surface::Clipboard => vec![
                ("Restore", Action::Activate, empty),
                ("Copy", Action::Copy, empty),
                ("Pin", Action::Pin, empty),
                ("Delete", Action::Delete, empty),
                ("Clear history", Action::Clear, empty),
                ("Reveal file", Action::Reveal, empty),
                ("Save to Vault", Action::VaultDraft, empty),
            ],
            Surface::Snippets => {
                if self.fields.is_empty() {
                    vec![
                        ("New snippet", Action::New, false),
                        ("Edit", Action::Edit, empty || readonly),
                        ("Insert", Action::Activate, empty),
                        ("Delete", Action::Delete, empty || readonly),
                    ]
                } else {
                    vec![
                        ("Save snippet", Action::Save, false),
                        ("Cancel", Action::Cancel, false),
                    ]
                }
            }
            Surface::Notes => vec![
                ("New note", Action::New, false),
                ("Copy note", Action::Copy, self.notes.selected.is_none()),
                ("Delete note", Action::Delete, self.notes.selected.is_none()),
                ("Retry save", Action::RetryNote, false),
            ],
            Surface::Vault => {
                if self.vault.model.unlocked {
                    if self.fields.is_empty() {
                        vec![
                            ("New item", Action::New, self.vault.busy),
                            ("Edit item", Action::Edit, self.vault.detail.is_none()),
                            ("Copy secret", Action::Copy, empty),
                            ("Delete item", Action::Delete, empty),
                            ("Lock", Action::Lock, false),
                            (
                                "Reveal value",
                                Action::RevealSecret,
                                self.vault.detail.is_none(),
                            ),
                        ]
                    } else {
                        vec![
                            ("Save item", Action::Save, self.vault.busy),
                            ("Cancel", Action::Cancel, false),
                            ("Lock", Action::Lock, false),
                        ]
                    }
                } else {
                    vec![(
                        "Unlock Vault",
                        Action::Unlock,
                        self.vault.supported != Some(true),
                    )]
                }
            }
            Surface::SystemMonitor => vec![
                ("Refresh", Action::Refresh, false),
                (
                    "Auto refresh",
                    Action::AutoRefresh(!self.monitor.auto_refresh),
                    false,
                ),
                ("Copy snapshot", Action::Copy, self.monitor.value.is_none()),
            ],
            Surface::Indexing => vec![
                ("Refresh", Action::Refresh, self.indexing.loading),
                ("Start indexing", Action::IndexStart, false),
                ("Pause indexing", Action::IndexPause, false),
                ("Resume indexing", Action::IndexResume, false),
                (
                    "Load more",
                    Action::MoreSources,
                    !self.indexing.page.as_ref().is_some_and(|p| p.has_more),
                ),
                ("Open file", Action::Activate, empty),
                ("Reveal", Action::Reveal, empty),
                ("Edit in Settings", Action::Settings, false),
            ],
            _ => vec![],
        };
        for (label, a, disabled) in buttons {
            bar = bar.child(self.action_button(label, a, disabled, cx, t));
        }
        bar
    }
    fn preview(&self, cx: &mut Context<Self>, t: tokens::Tokens) -> AnyElement {
        let mut pane = div()
            .id("utility-detail")
            .role(Role::Document)
            .aria_label("Selection preview")
            .flex_1()
            .min_w_0()
            .p(px(16.));
        match self.surface {
            Surface::Notes => return pane.child(self.editor.clone()).into_any_element(),
            Surface::Clipboard => {
                if let Some(image) = &self.preview_image {
                    pane = pane.child(img(image.clone()).max_w_full().max_h(px(240. * t.zoom)));
                }
                pane = pane.child(self.preview.clone());
                if let Some(result) = &self.clipboard.conversion {
                    if let Some(image) = &self.qr_image {
                        pane = pane
                            .child(img(image.clone()).w(px(220. * t.zoom)).h(px(220. * t.zoom)));
                    }
                    pane = pane.child(div().mt(px(12.)).child(match result {
                        Ok(s) if s.starts_with("data:image/") => String::new(),
                        Ok(s) => s.clone(),
                        Err(e) => e.clone(),
                    }));
                }
                if self
                    .clipboard
                    .selected_entry()
                    .is_some_and(|e| !e.is_secret && e.text().is_some())
                {
                    for (label, a) in [
                        (
                            "JSON",
                            tezbar_core::instant::transform::ClipboardAction::Json,
                        ),
                        (
                            "Base64",
                            tezbar_core::instant::transform::ClipboardAction::Base64,
                        ),
                        (
                            "Encode",
                            tezbar_core::instant::transform::ClipboardAction::Encode,
                        ),
                        (
                            "Hash",
                            tezbar_core::instant::transform::ClipboardAction::Hash,
                        ),
                        (
                            "Count",
                            tezbar_core::instant::transform::ClipboardAction::Count,
                        ),
                        (
                            "Case",
                            tezbar_core::instant::transform::ClipboardAction::Case,
                        ),
                        ("QR", tezbar_core::instant::transform::ClipboardAction::Qr),
                    ] {
                        pane =
                            pane.child(self.action_button(label, Action::Convert(a), false, cx, t));
                    }
                    pane = pane
                        .child(self.action_button(
                            "Copy result",
                            Action::CopyResult,
                            self.clipboard.conversion.is_none(),
                            cx,
                            t,
                        ))
                        .child(self.action_button(
                            "Save PNG",
                            Action::SaveQr,
                            self.clipboard.conversion.is_none(),
                            cx,
                            t,
                        ))
                        .child(self.action_button(
                            "Copy QR",
                            Action::CopyQr,
                            self.clipboard.conversion.is_none(),
                            cx,
                            t,
                        ));
                }
            }
            Surface::Snippets => {
                if let Some(item) = self.snippets.visible().get(self.snippets.selected) {
                    let _ = item;
                    pane = pane.child(self.preview.clone());
                }
            }
            Surface::Vault => {
                if let Some(d) = &self.vault.detail {
                    pane = pane.child(d.name.clone()).child(div().mt(px(12.)).child(
                        if self.vault.revealed {
                            d.value.clone()
                        } else {
                            "••••••••".into()
                        },
                    ));
                } else {
                    pane = pane.child(if self.vault.supported == Some(false) {
                        "Vault is not supported on this platform."
                    } else if !self.vault.model.unlocked {
                        "Unlock Vault to view saved items."
                    } else {
                        "Select an item."
                    });
                }
            }
            _ => {}
        }
        pane.into_any_element()
    }
}
impl Render for Utilities {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::tokens(w);
        let title = super::super::command_bar::navigation::surface_title(&self.surface);
        let mut body = div().flex().flex_col().flex_1().min_h_0();
        if self.surface == Surface::Indexing {
            body = body.child(
                div()
                    .id("indexing-status")
                    .role(Role::Status)
                    .p(px(12. * t.zoom))
                    .child(self.background.index_summary.clone())
                    .child(
                        div().h(px(3.)).w_full().bg(t.hairline).child(
                            div()
                                .h_full()
                                .w(relative(self.background.index_progress))
                                .bg(t.success),
                        ),
                    ),
            );
        }
        if self.surface == Surface::Permissions {
            if let Some(snapshot) = &self.permissions.snapshot {
                for status in &snapshot.statuses {
                    let descriptor = &status.descriptor;
                    body = body.child(
                        div()
                            .id(SharedString::from(format!(
                                "permission-{:?}",
                                descriptor.id
                            )))
                            .p(px(12.))
                            .border_b_1()
                            .border_color(t.hairline)
                            .child(descriptor.title.clone())
                            .child(div().text_color(t.text3).child(descriptor.summary.clone()))
                            .child(div().child(format!("{:?}", status.state)))
                            .child(self.action_button(
                                format!("Request {}", descriptor.title),
                                Action::Permission(descriptor.id.clone()),
                                self.permissions.pending.is_some()
                                    || matches!(
                                        status.state,
                                        model::PermissionState::Granted
                                            | model::PermissionState::Unsupported
                                    ),
                                cx,
                                t,
                            ))
                            .when_some(descriptor.settings_url.clone(), |d, url| {
                                d.child(self.action_button(
                                    "Open System Settings",
                                    Action::PermissionSettings(url),
                                    false,
                                    cx,
                                    t,
                                ))
                            }),
                    );
                }
            }
        } else if self.surface == Surface::ReleaseNotes {
            body = match &self.release.model.state {
                tezbar_update::notes::State::Ready(n) => body
                    .child(format!(
                        "v{} · {}",
                        n.version,
                        n.published_at
                            .as_deref()
                            .unwrap_or("Bundled with this build")
                    ))
                    .when_some(self.release.markdown.clone(), |d, m| d.child(m)),
                tezbar_update::notes::State::Error(e) => body
                    .child(e.clone())
                    .child(self.action_button("Retry", Action::Refresh, false, cx, t)),
                _ => body.child("Loading release notes…"),
            };
        } else if !self.fields.is_empty() {
            if self.surface == Surface::Vault {
                let mut bar = div().flex().flex_wrap();
                for kind in [
                    tezbar_ui_model::vault::Kind::ApiKey,
                    tezbar_ui_model::vault::Kind::Token,
                    tezbar_ui_model::vault::Kind::SshKey,
                    tezbar_ui_model::vault::Kind::Server,
                    tezbar_ui_model::vault::Kind::Password,
                    tezbar_ui_model::vault::Kind::Note,
                ] {
                    let label = kind_label(kind);
                    bar = bar.child(self.action_button(
                        label,
                        Action::VaultDraftKind(kind),
                        false,
                        cx,
                        t,
                    ));
                }
                body = body.child(bar);
                body = body.child(
                    self.action_button("Change icon", Action::ChangeIcon, false, cx, t)
                        .track_focus(&self.icon_focus),
                );
                if self.vault.icon_open {
                    let mut picker = controls::popover("vault-icon-picker", "Change icon", t)
                        .on_mouse_down_out(cx.listener(|this, _, w, cx| {
                            this.vault.icon_open = false;
                            this.icon_focus.focus(w, cx);
                            cx.notify();
                        }))
                        .on_key_down(cx.listener(Self::icon_key));
                    if let Some(filter) = &self.icon_filter {
                        picker = picker.child(filter.clone());
                    }
                    let choices = self.vault.icons();
                    if choices.is_empty() {
                        picker = picker.child("No icons match. Change your filter.");
                    }
                    let mut grid = div()
                        .id("vault-icon-grid")
                        .role(Role::Grid)
                        .flex()
                        .flex_wrap()
                        .w(px(224. * t.zoom));
                    for (index, icon) in choices.iter().enumerate() {
                        grid = grid.child(
                            self.action_button(
                                icon.label.clone(),
                                Action::VaultIcon(icon.id.clone()),
                                false,
                                cx,
                                t,
                            )
                            .track_focus(
                                &self.icon_cell_focuses[tezbar_ui_model::vault::ICONS
                                    .iter()
                                    .position(|id| *id == icon.id)
                                    .unwrap_or(0)],
                            )
                            .text_size(px(0.))
                            .size(px(28. * t.zoom))
                            .child(
                                svg()
                                    .data(icon.svg.as_bytes())
                                    .size(px(14. * t.zoom))
                                    .text_color(t.text2),
                            )
                            .aria_selected(index == self.vault.icon_selected),
                        );
                    }
                    picker = picker.child(grid).child(self.action_button(
                        "Use default",
                        Action::DefaultIcon,
                        false,
                        cx,
                        t,
                    ));
                    body = body.child(picker);
                }
            }
            for field in &self.fields {
                body = body.child(div().p(px(8.)).child(field.clone()));
            }
        } else {
            if self.surface == Surface::Vault {
                let mut filters = div().flex().flex_wrap().child(self.action_button(
                    "All types",
                    Action::VaultKind(None),
                    false,
                    cx,
                    t,
                ));
                for kind in [
                    tezbar_ui_model::vault::Kind::ApiKey,
                    tezbar_ui_model::vault::Kind::Token,
                    tezbar_ui_model::vault::Kind::SshKey,
                    tezbar_ui_model::vault::Kind::Server,
                    tezbar_ui_model::vault::Kind::Password,
                    tezbar_ui_model::vault::Kind::Note,
                ] {
                    filters = filters.child(self.action_button(
                        kind_label(kind),
                        Action::VaultKind(Some(kind)),
                        false,
                        cx,
                        t,
                    ));
                }
                body = body.child(filters);
            }
            if self.surface == Surface::Emoji {
                let mut categories = Vec::new();
                for entry in tezbar_core::instant::emoji::DATA.iter() {
                    if !categories.contains(&entry.category) {
                        categories.push(entry.category.clone());
                    }
                }
                let mut bar = div().flex().flex_wrap().gap(px(4.));
                bar =
                    bar.child(self.action_button("All", Action::EmojiCategory(None), false, cx, t));
                for category in categories {
                    let name = category.clone();
                    bar = bar.child(
                        controls::segment(
                            SharedString::from(format!("category-{name}")),
                            SharedString::from(name),
                            self.emoji.category.as_deref() == Some(&category),
                            t,
                        )
                        .on_click(cx.listener(move |this, _, w, cx| {
                            this.act(Action::EmojiCategory(Some(category.clone())), w, cx)
                        })),
                    );
                }
                body = body.child(bar);
            }
            if self.surface == Surface::Clipboard {
                let mut bar = div().flex();
                for (label, filter) in [
                    ("All", model::ClipboardFilter::All),
                    ("Text", model::ClipboardFilter::Text),
                    ("Images", model::ClipboardFilter::Image),
                    ("URLs", model::ClipboardFilter::Url),
                    ("Files", model::ClipboardFilter::File),
                ] {
                    bar = bar.child(
                        controls::segment(label, label, self.clipboard.filter() == filter, t)
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.act(Action::Filter(filter), w, cx)
                            })),
                    );
                }
                body = body.child(bar);
            }
            if self.surface == Surface::SystemMonitor {
                let mut tabs = div().flex().flex_wrap();
                for tab in [
                    "overview",
                    "cpu",
                    "memory",
                    "storage",
                    "network",
                    "battery",
                    "processes",
                ] {
                    tabs = tabs.child(
                        controls::segment(tab, tab, self.monitor_tab == tab, t).on_click(
                            cx.listener(move |this, _, w, cx| {
                                this.act(Action::MonitorTab(tab.into()), w, cx)
                            }),
                        ),
                    );
                }
                body = body.child(tabs);
            }
            let rows = self.rows();
            let count = rows.len();
            let mut split = div().flex().flex_1().min_h_0();
            if count == 0 {
                split = split.child(div().p(px(16.)).text_color(t.text3).child(
                    if self.status().0 {
                        "Loading…"
                    } else {
                        match self.surface {
                            Surface::Clipboard=>"Clipboard history is empty. Copy text or an image in another app.",
                            Surface::Snippets=>"No snippets match. Clear the search or create a snippet.",
                            Surface::Notes=>"No notes match. Clear the search or create a note.",
                            Surface::Vault if self.vault.supported==Some(false)=>"Vault is not supported on this platform.",
                            Surface::Vault if !self.vault.model.unlocked=>"Unlock Vault to view saved items.",
                            Surface::Vault=>"No Vault items match. Clear the search or add an item.",
                            Surface::Emoji=>"No emoji match. Try a name or category.",
                            Surface::Indexing=>"No indexed sources match. Change the filter or edit folders in Settings.",
                            _=>"No system data matched. Clear the search or refresh.",
                        }
                    },
                ));
            } else if self.surface == Surface::Emoji {
                split = split.child(
                    tezbar_ui::virtual_list::grid(
                        "emoji-grid",
                        rows.iter()
                            .map(|(id, _)| SharedString::from(id.clone()))
                            .collect(),
                        8,
                        cx,
                        |this, i, w, cx| {
                            let t = tokens::tokens(w);
                            let label = this
                                .emoji
                                .visible()
                                .get(i)
                                .copied()
                                .unwrap_or("")
                                .to_string();
                            controls::button(
                                SharedString::from(format!("emoji-cell-{i}")),
                                SharedString::from(label),
                                ButtonTone::Ghost,
                                false,
                                t,
                            )
                            .text_size(px(28. * t.zoom))
                            .aria_selected(this.emoji.selected == i)
                            .on_mouse_move(cx.listener(move |this, e: &MouseMoveEvent, w, cx| {
                                this.hover(i, e, w, cx)
                            }))
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.act(Action::Select(i), w, cx);
                                this.act(Action::Activate, w, cx);
                            }))
                            .into_any_element()
                        },
                    )
                    .track_scroll(&self.scroll)
                    .flex_1(),
                );
            } else {
                split = split.child(
                    uniform_list(
                        "utility-list",
                        count,
                        cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                            range
                                .map(|i| {
                                    let (id, label) = &rows[i];
                                    tezbar_ui::virtual_list::row(
                                        SharedString::from(id.clone()),
                                        SharedString::from(label.clone()),
                                        i,
                                        count,
                                        this.selected() == i,
                                        t,
                                    )
                                    .h(px(if matches!(
                                        this.surface,
                                        Surface::Notes | Surface::SystemMonitor
                                    ) {
                                        64.
                                    } else {
                                        36.
                                    } * t.zoom))
                                    .child(this.list_content(id, label, t))
                                    .on_mouse_move(cx.listener(
                                        move |this, e: &MouseMoveEvent, w, cx| {
                                            this.hover(i, e, w, cx)
                                        },
                                    ))
                                    .on_click(cx.listener(
                                        move |this, _, w, cx| this.act(Action::Select(i), w, cx),
                                    ))
                                })
                                .collect::<Vec<_>>()
                        }),
                    )
                    .track_scroll(&self.scroll)
                    .flex_1(),
                );
                if matches!(
                    self.surface,
                    Surface::Clipboard | Surface::Notes | Surface::Snippets | Surface::Vault
                ) {
                    split = split.child(self.preview(cx, t));
                }
            }
            body = body.child(split);
        }
        let (loading, error) = self.status();
        let note_status = if self.surface == Surface::Notes {
            self.notes
                .selected
                .and_then(|id| self.notes.save_states.get(&id))
                .map(|state| match state {
                    note_model::SaveState::Dirty => "Unsaved changes".into(),
                    note_model::SaveState::Saving => "Saving…".into(),
                    note_model::SaveState::Saved => "Saved".into(),
                    note_model::SaveState::Error(e) => format!("Could not save: {e}"),
                    note_model::SaveState::Idle => String::new(),
                })
        } else {
            None
        };
        let status = if let Some(e) = self.error.as_deref().or(error) {
            e.to_string()
        } else if self.surface == Surface::Permissions && self.permissions.banner.is_some() {
            self.permissions.banner.as_ref().unwrap().1.clone()
        } else if self.surface == Surface::Vault && self.vault.model.message.is_some() {
            self.vault.model.message.unwrap().into()
        } else if let Some(status) = note_status.filter(|s| !s.is_empty()) {
            status
        } else if let Some(m) = &self.message {
            m.clone()
        } else if loading {
            "Loading…".into()
        } else {
            format!("{} items", self.rows().len())
        };
        let mut root = controls::sheet(t)
            .id("utilities")
            .track_focus(&self.focus)
            .key_context("Utilities")
            .on_key_down(cx.listener(Self::key))
            .child(
                controls::header(t)
                    .child(self.action_button("Back", Action::Cancel, false, cx, t))
                    .child(title),
            )
            .child(div().px(px(12.)).py(px(8.)).child(self.search.clone()))
            .child(self.toolbar(cx, t))
            .child(body)
            .child(
                controls::footer(t)
                    .id("utility-status")
                    .role(Role::Status)
                    .aria_label("Utility status")
                    .child(status),
            );
        if self.confirmation.is_some() {
            root = root.child(
                controls::popover("utility-confirmation", "Confirm deletion", t)
                    .child(if matches!(self.confirmation, Some((Action::Clear, _))) {
                        "Clear all clipboard history? This cannot be undone."
                    } else {
                        "Delete the selected items? This cannot be undone."
                    })
                    .child(self.action_button(
                        if matches!(self.confirmation, Some((Action::Clear, _))) {
                            "Clear history"
                        } else {
                            "Delete"
                        },
                        Action::Confirm(true),
                        false,
                        cx,
                        t,
                    ))
                    .child(self.action_button("Cancel", Action::Confirm(false), false, cx, t)),
            );
        }
        root
    }
}

pub(super) fn kind_label(kind: tezbar_ui_model::vault::Kind) -> &'static str {
    use tezbar_ui_model::vault::Kind;
    match kind {
        Kind::ApiKey => "API Key",
        Kind::Token => "Token",
        Kind::SshKey => "SSH Key",
        Kind::Server => "Server",
        Kind::Password => "Password",
        Kind::Note => "Note",
    }
}
