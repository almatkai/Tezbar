use super::*;
use controls::ButtonTone;
impl Utilities {
    fn list_content(&self, id: &str, label: &str, t: tokens::Tokens) -> AnyElement {
        if self.surface == Surface::Vault {
            if let Some(item) = self.vault.rows.iter().find(|r| r.id == id) {
                return div()
                    .flex()
                    .items_center()
                    .gap(px(10. * t.zoom))
                    .min_w_0()
                    .child(vault_glyph(item.kind, item.icon.as_deref(), 18., t))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .gap(px(2. * t.zoom))
                            .child(div().truncate().child(item.name.clone()))
                            .child(
                                div()
                                    .text_size(px(11. * t.zoom))
                                    .text_color(t.text3)
                                    .child(kind_label(item.kind)),
                            ),
                    )
                    .into_any_element();
            }
        }
        if self.surface == Surface::Clipboard {
            if let Some((index, entry)) = self
                .clipboard
                .visible()
                .iter()
                .enumerate()
                .find(|(_, e)| e.id == id)
            {
                let (glyph, subtitle) = match &entry.content {
                    model::ClipboardContent::Image {
                        width,
                        height,
                        byte_size,
                        ..
                    } => ("▧", format!("{width} × {height} · {byte_size} bytes")),
                    model::ClipboardContent::File { paths, .. } => (
                        "▱",
                        if paths.len() > 1 {
                            format!("{} paths", paths.len())
                        } else {
                            paths.first().cloned().unwrap_or_default()
                        },
                    ),
                    model::ClipboardContent::Text { char_count, .. } => (
                        if entry.is_url() { "↗" } else { "T" },
                        if entry.is_url() {
                            "URL".into()
                        } else if *char_count > 40 {
                            format!("{char_count} chars")
                        } else {
                            String::new()
                        },
                    ),
                };
                let icon = match glyph {
                    "▧" => {
                        r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 14 14" fill="none"> <rect x="1.5" y="2.5" width="11" height="9" rx="1.5" stroke="currentColor" stroke-width="1.2" /> <circle cx="5" cy="6" r="1" fill="currentColor" /> <path d="m2.5 10 3-2.5 3 2L11 8l1.5 1.5" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" /> </svg>"#
                    }
                    "▱" => {
                        r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 14 14" fill="none"> <path d="M2.5 3.5a1 1 0 0 1 1-1h2.5l1 1.2H10.5a1 1 0 0 1 1 1V10a1 1 0 0 1-1 1h-7a1 1 0 0 1-1-1V3.5Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" /> </svg>"#
                    }
                    "↗" => {
                        r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 14 14" fill="none"> <path d="M6 8a2 2 0 0 0 2.8 0l2-2a2 2 0 0 0-2.8-2.8l-.6.6" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" /> <path d="M8 6a2 2 0 0 0-2.8 0l-2 2a2 2 0 0 0 2.8 2.8l.6-.6" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" /> </svg>"#
                    }
                    "T" => {
                        r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 14 14" fill="none"> <path d="M3.5 2.5h5l2.5 2.5v6.5a.5.5 0 0 1-.5.5h-7a.5.5 0 0 1-.5-.5v-8.5a.5.5 0 0 1 .5-.5Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" /> <path d="M5 7h4M5 9h3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" /> </svg>"#
                    }
                    _ => unreachable!(),
                };
                return div()
                    .flex()
                    .items_center()
                    .w_full()
                    .min_w_0()
                    .gap(px(8. * t.zoom))
                    .child(
                        div()
                            .size(px(28. * t.zoom))
                            .flex_shrink_0()
                            .overflow_hidden()
                            .border_1()
                            .border_color(t.hairline)
                            .rounded(px(7. * t.zoom))
                            .bg(t.field)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(t.text3)
                            .child(
                                if let Some(image) = self.preview_image.as_ref().filter(|_| {
                                    self.clipboard
                                        .selected_entry()
                                        .is_some_and(|selected| selected.id == id)
                                        && matches!(
                                            entry.content,
                                            model::ClipboardContent::Image { .. }
                                        )
                                        && !entry.is_secret
                                }) {
                                    img(image.clone())
                                        .size_full()
                                        .object_fit(ObjectFit::Cover)
                                        .into_any_element()
                                } else {
                                    svg()
                                        .data(icon.as_bytes())
                                        .size(px(14. * t.zoom))
                                        .into_any_element()
                                },
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(12.5 * t.zoom))
                                    .child(label.to_owned()),
                            )
                            .when(!subtitle.is_empty(), |d| {
                                d.child(
                                    div()
                                        .mt(px(2. * t.zoom))
                                        .truncate()
                                        .text_size(px(10.5 * t.zoom))
                                        .text_color(t.text3)
                                        .child(subtitle),
                                )
                            }),
                    )
                    .when(entry.pinned, |d| d.child(controls::pin(t)))
                    .when(index < 9, |d| {
                        d.child(
                            div()
                                .text_size(px(9. * t.zoom))
                                .text_color(t.text3)
                                .child(platform_keys(&format!("⌘{}", index + 1))),
                        )
                    })
                    .into_any_element();
            }
        }
        if self.surface == Surface::Snippets {
            if let Some(item) = self.snippets.visible().iter().find(|r| r.id == id) {
                return div()
                    .flex()
                    .items_center()
                    .w_full()
                    .gap(px(8. * t.zoom))
                    .child(
                        div()
                            .size(px(20. * t.zoom))
                            .rounded(px(5. * t.zoom))
                            .bg(rgb(0x14b8a6))
                            .flex_shrink_0()
                            .flex().items_center().justify_center()
                            .child(svg().data(br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3H7a2 2 0 0 0-2 2v5a2 2 0 0 1-2 2 2 2 0 0 1 2 2v5c0 1.1.9 2 2 2h1M16 21h1a2 2 0 0 0 2-2v-5c0-1.1.9-2 2-2a2 2 0 0 1-2-2V5a2 2 0 0 0-2-2h-1"/></svg>"# as &[u8]).size(px(12. * t.zoom)).text_color(rgb(0xffffff))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(div().truncate().child(item.title.clone()))
                            .child(
                                div()
                                    .text_size(px(11. * t.zoom))
                                    .text_color(t.text3)
                                    .truncate()
                                    .child(item.subtitle.clone()),
                            ),
                    )
                    .when(item.readonly, |d| {
                        d.child(
                            div()
                                .text_size(px(10. * t.zoom))
                                .text_color(t.text3)
                                .child("Built-in"),
                        )
                    })
                    .into_any_element();
            }
        }
        if self.surface == Surface::Indexing {
            if let Some(source) = self
                .indexing
                .page
                .as_ref()
                .and_then(|p| p.sources.iter().find(|r| r.id == id))
            {
                return div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12. * t.zoom))
                    .w_full()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(11.5 * t.zoom))
                                    .truncate()
                                    .child(source.title.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(9.5 * t.zoom))
                                    .text_color(t.text3)
                                    .truncate()
                                    .child(source.path.clone()),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(9.5 * t.zoom))
                            .text_color(t.text3)
                            .text_right()
                            .child(source.status.clone())
                            .child(div().child(format!(
                                "{} / {} pages",
                                source.indexed_page_count, source.total_page_count
                            ))),
                    )
                    .into_any_element();
            }
        }
        if self.surface == Surface::Notes {
            if let Some(note) = id
                .parse::<u64>()
                .ok()
                .and_then(|id| self.note_labels.get(&id))
            {
                return div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12. * t.zoom))
                    .w_full()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(13. * t.zoom))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(note.title.clone()),
                            )
                            .child(
                                div()
                                    .mt(px(2. * t.zoom))
                                    .truncate()
                                    .text_size(px(11. * t.zoom))
                                    .text_color(t.text3)
                                    .child(note.summary.clone()),
                            ),
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
    pub(super) fn key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        if self.vault.icon_open {
            self.icon_key(e, w, cx);
            return;
        }
        if self.search.read(cx).marked_range.is_some() || self.editor.read(cx).composing(cx) {
            cx.propagate();
            return;
        }
        let key = e.keystroke.key.as_str();
        // Cmd on macOS, Ctrl on Windows/Linux (where `platform` is the OS-reserved Win key).
        let command = e.keystroke.modifiers.secondary(cx);
        if self.surface == Surface::Permissions && key == "r" {
            self.act(Action::Refresh, w, cx);
            cx.stop_propagation();
            return;
        }
        if key == "escape" {
            if self.confirmation.is_some() {
                self.confirmation = None;
            } else if !self.fields.is_empty() {
                self.dispatch(Action::Cancel, w, cx);
            } else if self.surface != Surface::Vault
                && !self.search.read(cx).content.is_empty()
                && (self.surface == Surface::Emoji
                    || self.search.focus_handle(cx).contains_focused(w, cx))
            {
                self.search
                    .update(cx, |input, cx| input.clear_silently(w, cx));
                self.search_changed(String::new(), cx);
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
            if self.surface == Surface::Emoji && self.emoji.query.trim().is_empty() {
                if let Ok(slot) = key.parse::<usize>() {
                    if (1..=9).contains(&slot) {
                        if let Some(emoji) = self.emoji.recent.get(slot - 1).cloned() {
                            self.emoji.category = None;
                            self.emoji.query.clear();
                            if let Some(index) =
                                self.emoji.visible().iter().position(|e| *e == emoji)
                            {
                                self.act(Action::Select(index), w, cx);
                                self.act(Action::Activate, w, cx);
                            }
                        }
                        cx.stop_propagation();
                        return;
                    }
                }
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
        if key == "enter"
            && !command
            && !e.keystroke.modifiers.shift
            && !self.fields.is_empty()
            && ((self.surface == Surface::Vault
                && self
                    .fields
                    .first()
                    .is_some_and(|field| field.focus_handle(cx).contains_focused(w, cx)))
                || (self.surface == Surface::Snippets
                    && self
                        .fields
                        .iter()
                        .take(2)
                        .any(|field| field.focus_handle(cx).contains_focused(w, cx))))
        {
            self.act(Action::Save, w, cx);
            cx.stop_propagation();
            return;
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
    pub(super) fn action_button(
        &self,
        label: impl Into<SharedString>,
        action: Action,
        disabled: bool,
        cx: &mut Context<Self>,
        t: tokens::Tokens,
    ) -> Stateful<Div> {
        let label = label.into();
        self.action_button_with_id(label.clone(), label, action, disabled, cx, t)
    }
    fn action_button_with_id(
        &self,
        id: impl Into<ElementId>,
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
        let debug_label = label.clone();
        let tone = if self.surface == Surface::Vault {
            match a {
                Action::Unlock | Action::New | Action::Save | Action::Copy => ButtonTone::Primary,
                Action::Delete | Action::Confirm(true) => ButtonTone::Danger,
                Action::Cancel => ButtonTone::Quiet,
                Action::VaultKind(_) | Action::VaultDraftKind(_) => ButtonTone::Ghost,
                _ => ButtonTone::Quiet,
            }
        } else if self.surface == Surface::Snippets && matches!(a, Action::Save) {
            ButtonTone::Primary
        } else {
            ButtonTone::Ghost
        };
        controls::button(id, label, tone, disabled, t)
            .debug_selector(move || debug_label.to_string())
            .when(self.surface == Surface::Vault, |d| {
                d.flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .border_1()
                    .border_color(transparent_black())
                    .when(disabled, |d| d.tab_index(-1).opacity(0.5))
                    .when(!disabled, |d| {
                        d.cursor_pointer()
                            .hover(move |d| {
                                d.bg(if matches!(tone, ButtonTone::Primary) {
                                    t.accent_hover
                                } else {
                                    t.hover
                                })
                            })
                            .focus(move |d| d.border_color(t.accent_strong))
                    })
            })
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
    fn icon_action(
        &self,
        id: &'static str,
        label: &'static str,
        glyph: &'static str,
        action: Action,
        disabled: bool,
        cx: &mut Context<Self>,
        t: tokens::Tokens,
    ) -> AnyElement {
        let path = match glyph {
            "+" => "M12 5v14M5 12h14",
            "×" => "M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7M14 10v7",
            "↻" => "M20 7v5h-5M4 17v-5h5M6 6a8 8 0 0 1 13 2l1 4M4 12l1 4a8 8 0 0 0 13 2",
            _ => "M4 6h16M4 12h16M4 18h16M8 4v4M16 10v4M10 16v4",
        };
        let image = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="{path}"/></svg>"#
        );
        self.action_button_with_id(id, label, action, disabled, cx, t)
            .text_size(px(0.))
            .p_0()
            .size(px(28. * t.zoom))
            .rounded(px(8. * t.zoom))
            .when(self.surface != Surface::Vault && !disabled, |d| {
                d.cursor_pointer().hover(move |d| d.bg(t.hover))
            })
            .aria_label(label)
            .child(
                svg()
                    .data(image.as_bytes())
                    .size(px(14. * t.zoom))
                    .text_color(t.text3),
            )
            .into_any_element()
    }
    fn utility_chips(&self, cx: &mut Context<Self>, t: tokens::Tokens) -> Stateful<Div> {
        let mut chips = div()
            .id("utility-chips")
            .debug_selector(|| "utility-chips".into())
            .flex()
            .items_center()
            .min_w_0()
            .max_w(relative(0.55))
            .gap(px(6. * t.zoom))
            .overflow_x_scroll();
        if self.surface == Surface::Emoji {
            let mut categories = Vec::new();
            for entry in tezbar_core::instant::emoji::DATA.iter() {
                if !categories.contains(&entry.category) {
                    categories.push(entry.category.clone());
                }
            }
            let mut bar = div()
                .flex()
                .flex_shrink_0()
                .gap(px(4. * t.zoom))
                .px(px(0.))
                .pb(px(0.))
                .text_size(px(12. * t.zoom));
            bar = bar.child(self.action_button("All", Action::EmojiCategory(None), false, cx, t));
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
            chips = chips.child(bar);
        }
        if self.surface == Surface::Clipboard {
            let mut bar = div()
                .flex()
                .flex_shrink_0()
                .gap(px(4. * t.zoom))
                .px(px(0.))
                .pb(px(0.))
                .text_size(px(12. * t.zoom));
            for (label, filter) in [
                ("All", model::ClipboardFilter::All),
                ("Text", model::ClipboardFilter::Text),
                ("Image", model::ClipboardFilter::Image),
                ("URL", model::ClipboardFilter::Url),
                ("File", model::ClipboardFilter::File),
            ] {
                bar = bar.child(
                    controls::segment(
                        label,
                        {
                            let count = self
                                .clipboard
                                .rows
                                .iter()
                                .filter(|e| match filter {
                                    model::ClipboardFilter::All => true,
                                    model::ClipboardFilter::Text => {
                                        matches!(e.content, model::ClipboardContent::Text { .. })
                                            && !e.is_url()
                                    }
                                    model::ClipboardFilter::Image => {
                                        matches!(e.content, model::ClipboardContent::Image { .. })
                                    }
                                    model::ClipboardFilter::Url => e.is_url(),
                                    model::ClipboardFilter::File => {
                                        matches!(e.content, model::ClipboardContent::File { .. })
                                    }
                                })
                                .count();
                            if filter != model::ClipboardFilter::All && count > 0 {
                                SharedString::from(format!("{label} {count}"))
                            } else {
                                label.into()
                            }
                        },
                        self.clipboard.filter() == filter,
                        t,
                    )
                    .text_color(if self.clipboard.filter() == filter {
                        t.text1
                    } else {
                        t.text3
                    })
                    .on_click(
                        cx.listener(move |this, _, w, cx| this.act(Action::Filter(filter), w, cx)),
                    ),
                );
            }
            chips = chips.child(bar);
        }
        chips
    }
    fn emoji_cell(
        &self,
        index: usize,
        emoji: &str,
        cx: &mut Context<Self>,
        t: tokens::Tokens,
    ) -> Stateful<Div> {
        controls::button(
            SharedString::from(format!("emoji-cell-{index}")),
            SharedString::from(emoji.to_string()),
            ButtonTone::Ghost,
            false,
            t,
        )
        .debug_selector(move || format!("emoji-cell-{index}"))
        .flex()
        .items_center()
        .justify_center()
        .h(px(36. * t.zoom))
        .text_size(px(20. * t.zoom))
        .border_1()
        .border_color(if self.emoji.selected == index {
            t.accent
        } else {
            t.hairline
        })
        .aria_selected(self.emoji.selected == index)
        .bg(if self.emoji.selected == index {
            t.selected
        } else {
            t.field
        })
        .on_mouse_move(
            cx.listener(move |this, event: &MouseMoveEvent, w, cx| this.hover(index, event, w, cx)),
        )
        .on_click(cx.listener(move |this, _, w, cx| {
            this.act(Action::Select(index), w, cx);
            this.act(Action::Activate, w, cx);
        }))
    }
    fn footer_hints(&self, t: tokens::Tokens) -> Vec<Div> {
        let hints: &[(&str, &str)] = match self.surface {
            Surface::Clipboard => &[
                ("↵", "Copy"),
                ("P", "Pin"),
                ("⌘ S", "Save to Vault"),
                ("⌘ D", "Delete"),
                ("Esc", "Back"),
            ],
            Surface::Snippets if !self.fields.is_empty() => &[("↵", "Save"), ("Esc", "Cancel")],
            Surface::Snippets => &[
                ("⌘ N", "New"),
                ("↵", "Copy"),
                ("⌘ E", "Edit"),
                ("⌘ ⌫", "Delete"),
                ("Esc", "Back"),
            ],
            Surface::Notes => &[
                ("⌘ N", "Create"),
                ("⌘ ↵", "Copy note"),
                ("⌘ B", "Bold"),
                ("⌘ I", "Italic"),
                ("⌘ D", "Delete"),
                ("Esc", "Back"),
            ],
            Surface::Emoji => &[
                ("↑ ↓ ← →", "Navigate"),
                ("↵", "Copy"),
                ("⌘ 1-9", "Recent"),
                ("Esc", "Back"),
            ],
            Surface::Indexing => &[("↵", "Open"), ("⌘ ,", "Settings"), ("Esc", "Back")],
            Surface::Permissions => &[("R", "Refresh"), ("Esc", "Back")],
            Surface::Vault if !self.vault.model.unlocked => &[("↵", "Unlock"), ("Esc", "Back")],
            Surface::Vault if !self.fields.is_empty() => &[("⌘ ↵", "Save"), ("Esc", "Cancel")],
            Surface::Vault => &[
                ("↵", "Copy"),
                ("⌘ N", "New"),
                ("⌘ E", "Edit"),
                ("⌘ ⌫", "Delete"),
                ("⌘ L", "Lock"),
                ("Esc", "Back"),
            ],
            _ => &[("Esc", "Back")],
        };
        hints
            .iter()
            .map(|(keys, label)| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(4. * t.zoom))
                    .child(controls::keycap(platform_keys(keys), t))
                    .child(*label)
            })
            .collect()
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
        let mut bar = div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .flex_wrap()
            .gap(px(8. * t.zoom))
            .p(px(8. * t.zoom));
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
                        ("Save", Action::Save, false),
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
                        vec![("New item", Action::New, self.vault.busy)]
                    } else {
                        vec![
                            (
                                "Save",
                                Action::Save,
                                self.vault.busy
                                    || self
                                        .vault
                                        .model
                                        .draft
                                        .as_ref()
                                        .is_none_or(|draft| draft.value.trim().is_empty()),
                            ),
                            ("Cancel", Action::Cancel, false),
                        ]
                    }
                } else {
                    vec![]
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
    fn clipboard_transform_row(&self, cx: &mut Context<Self>, t: tokens::Tokens) -> AnyElement {
        use tezbar_core::instant::transform::ClipboardAction as C;
        let owned = self
            .clipboard
            .selected_entry()
            .is_some_and(|entry| self.clipboard_ui.owner.as_deref() == Some(&entry.id));
        let mut chips = div().flex().flex_wrap().items_center().gap(px(4. * t.zoom));
        for (label, action) in [
            ("JSON", C::Json),
            ("Base64", C::Base64),
            ("Encode", C::Encode),
            ("Hash", C::Hash),
            ("Count", C::Count),
            ("Case", C::Case),
            ("QR", C::Qr),
        ] {
            let active = owned && self.clipboard_ui.active == Some(action);
            let busy = active && self.clipboard_ui.pending.is_some();
            let icon = match action {
                C::Json => {
                    r#"<svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"> <path d="M4.5 2.5c-1 0-1.5.5-1.5 1.5v2c0 .8-.4 1.2-1.2 1.5.8.3 1.2.7 1.2 1.5v2c0 1 .5 1.5 1.5 1.5M9.5 2.5c1 0 1.5.5 1.5 1.5v2c0 .8.4 1.2 1.2 1.5-.8.3-1.2.7-1.2 1.5v2c0 1-.5 1.5-1.5 1.5" /> </svg>"#
                }
                C::Base64 => {
                    r#"<svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"> <rect x="2" y="2.5" width="10" height="9" rx="1.5" /> <path d="M4.5 6.5h5M4.5 8.5h3" /> </svg>"#
                }
                C::Encode => {
                    r#"<svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"> <path d="M5.5 8.5 8.5 5.5M7.25 3.25l1-1a2.25 2.25 0 0 1 3.18 3.18l-1 1M6.75 10.75l-1 1a2.25 2.25 0 0 1-3.18-3.18l1-1" /> </svg>"#
                }
                C::Hash => {
                    r#"<svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"> <path d="M5 2 4 12M10 2 9 12M2.5 5h9M2 9h9" /> </svg>"#
                }
                C::Count => {
                    r#"<svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"> <path d="M3 2.5h8M3 7h8M3 11.5h5" /> </svg>"#
                }
                C::Case => {
                    r#"<svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"> <path d="M3 10.5 5.4 3.5h.2L8 10.5M3.8 8.5h3.4M9.5 10.5v-3.8c0-.6.4-1 1-1 1 0 1.5.7 1.5 1.5 0 .9-.6 1.5-1.5 1.6l1.7 1.7" /> </svg>"#
                }
                C::Qr => {
                    r#"<svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"> <rect x="2" y="2" width="4" height="4" rx=".5" /> <rect x="8" y="2" width="4" height="4" rx=".5" /> <rect x="2" y="8" width="4" height="4" rx=".5" /> <path d="M8 8h1.5M11 8h1M8 11v1M10 10h2v2h-1.5" /> </svg>"#
                }
            };
            chips = chips.child(
                self.action_button(label, Action::Convert(action), busy, cx, t)
                    .flex()
                    .items_center()
                    .gap(px(4. * t.zoom))
                    .px(px(6. * t.zoom))
                    .py(px(2. * t.zoom))
                    .text_size(px(0.))
                    .border_1()
                    .border_color(if active { t.success } else { t.hairline })
                    .bg(if active { t.pill_strong } else { t.pill })
                    .text_color(if active { t.success } else { t.text3 })
                    .aria_toggled(if active {
                        Toggled::True
                    } else {
                        Toggled::False
                    })
                    .child(svg().data(icon.as_bytes()).size(px(11. * t.zoom)))
                    .child(div().text_size(px(10.5 * t.zoom)).child(label)),
            );
        }
        let mut row = div()
            .id("clipboard-conversions")
            .debug_selector(|| "clipboard-conversions".into())
            .flex()
            .flex_col()
            .gap(px(6. * t.zoom))
            .border_t_1()
            .border_color(t.hairline)
            .py(px(8. * t.zoom))
            .child(chips);
        if let Some(result) = &self.clipboard.conversion {
            let mut panel = div()
                .id("clipboard-conversion-result")
                .debug_selector(|| "clipboard-conversion-result".into())
                .flex()
                .flex_col()
                .gap(px(6. * t.zoom))
                .rounded(px(8. * t.zoom))
                .border_1()
                .border_color(t.hairline)
                .bg(t.field)
                .p(px(8. * t.zoom));
            match result {
                Err(error) => {
                    panel = panel.child(
                        div()
                            .text_size(px(11. * t.zoom))
                            .text_color(t.danger)
                            .child(error.clone()),
                    );
                }
                Ok(output) => {
                    let qr = owned
                        && self.clipboard_ui.active == Some(C::Qr)
                        && output.starts_with("data:image/");
                    if qr {
                        if let Some(image) = &self.qr_image {
                            panel = panel.child(img(image.clone()).size(px(120. * t.zoom)));
                        }
                        panel = panel.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6. * t.zoom))
                                .child(
                                    self.action_button(
                                        "Copy Image",
                                        Action::CopyQr,
                                        self.qr_image.is_none(),
                                        cx,
                                        t,
                                    )
                                    .text_size(px(11. * t.zoom))
                                    .px(px(8. * t.zoom))
                                    .py(px(4. * t.zoom)),
                                )
                                .child(
                                    self.action_button(
                                        "Download Image",
                                        Action::SaveQr,
                                        self.qr_image.is_none(),
                                        cx,
                                        t,
                                    )
                                    .text_size(px(11. * t.zoom))
                                    .px(px(8. * t.zoom))
                                    .py(px(4. * t.zoom)),
                                ),
                        );
                    } else {
                        panel = panel.child(
                            div()
                                .id("clipboard-conversion-output")
                                .max_h(px(160. * t.zoom))
                                .overflow_y_scroll()
                                .font_family(tokens::font_family(cx, true))
                                .text_size(px(11. * t.zoom))
                                .line_height(relative(1.5))
                                .whitespace_normal()
                                .child(output.clone()),
                        );
                    }
                    panel = panel.child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8. * t.zoom))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(if qr { 9.5 } else { 11. } * t.zoom))
                                    .text_color(t.text4)
                                    .child(if qr { output.clone() } else { "Result".into() }),
                            )
                            .child(
                                self.action_button("Copy result", Action::CopyResult, false, cx, t)
                                    .text_size(px(10.5 * t.zoom))
                                    .px(px(6. * t.zoom))
                                    .py(px(2. * t.zoom)),
                            ),
                    );
                }
            }
            row = row.child(panel);
        }
        row.into_any_element()
    }
    fn preview(&self, cx: &mut Context<Self>, t: tokens::Tokens) -> AnyElement {
        let mut pane = div()
            .id("utility-detail")
            .debug_selector(|| "utility-detail".into())
            .role(Role::Document)
            .aria_label("Selection preview")
            .flex_1()
            .min_w_0()
            .p(px(16. * t.zoom))
            .border_l_1()
            .border_color(t.hairline);
        match self.surface {
            Surface::Notes => {
                if self.notes.selected.is_none() {
                    return pane
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_center()
                        .text_size(px(12. * t.zoom))
                        .text_color(t.text4)
                        .child("Select a note to edit.")
                        .into_any_element();
                }
                return pane.child(self.editor.clone()).into_any_element();
            }
            Surface::Clipboard => {
                if self.clipboard.selected_entry().is_none() {
                    return pane
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_center()
                        .text_size(px(12. * t.zoom))
                        .text_color(t.text4)
                        .child("Your clipboard history preview will show up here.")
                        .into_any_element();
                }
                if let Some(image) = &self.preview_image {
                    pane = pane.child(img(image.clone()).max_w_full().max_h(px(240. * t.zoom)));
                }
                if self.clipboard.selected_entry().is_some_and(|e| e.is_secret) {
                    return pane
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(4. * t.zoom))
                        .child(
                            div()
                                .text_size(px(13. * t.zoom))
                                .font_weight(FontWeight::MEDIUM)
                                .child("Secure item"),
                        )
                        .child(
                            div()
                                .text_size(px(11. * t.zoom))
                                .text_color(t.text4)
                                .child("This value is hidden. Press Enter to paste it back."),
                        )
                        .into_any_element();
                }
                pane = pane.flex().flex_col().gap(px(8. * t.zoom)).child({
                    let entry = self.clipboard.selected_entry().unwrap();
                    let mut content = div()
                        .id("clipboard-plain-preview")
                        .debug_selector(|| "clipboard-plain-preview".into())
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .font_family(tokens::font_family(cx, true))
                        .text_size(px(12. * t.zoom))
                        .line_height(relative(1.55))
                        .whitespace_normal()
                        .text_color(if entry.is_url() {
                            if t.light {
                                t.link
                            } else {
                                rgb(0xbae6fd).into()
                            }
                        } else {
                            t.text1
                        });
                    match &entry.content {
                        model::ClipboardContent::Text { text, .. } => {
                            content = content.child(text.clone());
                        }
                        model::ClipboardContent::Image { .. } => {
                            if self.preview_image.is_none() {
                                content = content.child("Loading preview…");
                            }
                        }
                        model::ClipboardContent::File { paths, .. } => {
                            for path in paths {
                                content = content.child(
                                    div()
                                        .mb(px(4. * t.zoom))
                                        .border_1()
                                        .border_color(t.hairline)
                                        .rounded(px(8. * t.zoom))
                                        .px(px(10. * t.zoom))
                                        .py(px(6. * t.zoom))
                                        .child(
                                            div().text_size(px(12. * t.zoom)).child(
                                                path.rsplit(['/', '\\'])
                                                    .next()
                                                    .unwrap_or(path)
                                                    .to_string(),
                                            ),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(10.5 * t.zoom))
                                                .text_color(t.text4)
                                                .child(path.clone()),
                                        ),
                                );
                            }
                        }
                    }
                    content
                });
                if self
                    .clipboard
                    .selected_entry()
                    .is_some_and(|entry| !entry.is_secret && entry.text().is_some())
                {
                    pane = pane.child(self.clipboard_transform_row(cx, t));
                }
                if let Some(entry) = self.clipboard.selected_entry() {
                    let kind = match entry.content {
                        model::ClipboardContent::Text { .. } => {
                            if entry.is_url() {
                                "URL"
                            } else {
                                "Text"
                            }
                        }
                        model::ClipboardContent::Image { .. } => "Image",
                        model::ClipboardContent::File { .. } => "File",
                    };
                    pane = pane.child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_t_1()
                            .border_color(t.hairline)
                            .py(px(8. * t.zoom))
                            .text_size(px(11. * t.zoom))
                            .text_color(t.text4)
                            .child(kind)
                            .child(note_model::format_date(
                                entry.created_at,
                                &*self.presentation.0,
                            )),
                    );
                }
            }
            Surface::Snippets => {
                if self.snippets.visible().is_empty() {
                    return pane
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_center()
                        .text_size(px(12. * t.zoom))
                        .text_color(t.text4)
                        .child("Select a snippet to preview expanded text.")
                        .into_any_element();
                }
                if let Some(item) = self.snippets.visible().get(self.snippets.selected) {
                    pane = pane
                        .p(px(12. * t.zoom))
                        .flex()
                        .flex_col()
                        .gap(px(8. * t.zoom))
                        .when(!item.readonly, |pane| {
                            pane.child(div().flex().items_center().justify_end().when(
                                !item.readonly,
                                |d| {
                                    d.child(
                                        div()
                                            .flex()
                                            .gap(px(4. * t.zoom))
                                            .child(self.action_button(
                                                "Edit",
                                                Action::Edit,
                                                false,
                                                cx,
                                                t,
                                            ))
                                            .child(self.action_button(
                                                "Delete",
                                                Action::Delete,
                                                false,
                                                cx,
                                                t,
                                            )),
                                    )
                                },
                            ))
                        })
                        .child(
                            div()
                                .id("snippets-plain-preview")
                                .debug_selector(|| "snippets-plain-preview".into())
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .whitespace_normal()
                                .rounded(px(7. * t.zoom))
                                .border_1()
                                .border_color(t.hairline)
                                .bg(if t.light {
                                    rgb(0xffffff)
                                } else {
                                    rgba(0x00000033)
                                })
                                .p(px(10. * t.zoom))
                                .font_family(tokens::font_family(cx, true))
                                .text_size(px(11.5 * t.zoom))
                                .line_height(relative(1.625))
                                .text_color(t.text2)
                                .child(item.preview.clone()),
                        )
                        .when(item.body != item.preview, |pane| {
                            pane.child(
                                div()
                                    .text_size(px(10. * t.zoom))
                                    .text_color(t.text4)
                                    .truncate()
                                    .child(format!("Template: {}", item.body)),
                            )
                        });
                }
            }
            Surface::Vault => {
                pane = pane
                    .flex()
                    .flex_col()
                    .gap(px(16. * t.zoom))
                    .h_full()
                    .overflow_y_scroll()
                    .text_size(px(12.5 * t.zoom));
                if let Some(d) = &self.vault.detail {
                    pane = pane
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10. * t.zoom))
                                .child(vault_glyph(d.kind, d.icon.as_deref(), 22., t))
                                .child(
                                    div()
                                        .min_w_0()
                                        .child(
                                            div()
                                                .truncate()
                                                .text_size(px(14. * t.zoom))
                                                .font_weight(FontWeight::MEDIUM)
                                                .child(d.name.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(11. * t.zoom))
                                                .text_color(t.text4)
                                                .child(kind_label(d.kind)),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_start()
                                .gap(px(8. * t.zoom))
                                .rounded(px(8. * t.zoom))
                                .border_1()
                                .border_color(t.hairline)
                                .bg(t.field)
                                .px(px(12. * t.zoom))
                                .py(px(8. * t.zoom))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .font_family(tokens::font_family(cx, true))
                                        .whitespace_normal()
                                        .text_color(if self.vault.revealed {
                                            t.text1
                                        } else {
                                            t.text3
                                        })
                                        .child(if self.vault.revealed {
                                            d.value.clone()
                                        } else {
                                            "••••••••••••••••".into()
                                        }),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_shrink_0()
                                        .gap(px(4. * t.zoom))
                                        .child(self.action_button(
                                            if self.vault.revealed {
                                                "Hide"
                                            } else {
                                                "Reveal"
                                            },
                                            Action::RevealSecret,
                                            false,
                                            cx,
                                            t,
                                        ))
                                        .child(self.action_button(
                                            "Copy",
                                            Action::Copy,
                                            self.vault.busy,
                                            cx,
                                            t,
                                        )),
                                ),
                        )
                        .when_some(
                            d.note.as_ref().filter(|n| !n.trim().is_empty()),
                            |pane, note| {
                                pane.child(
                                    div()
                                        .text_color(t.text2)
                                        .whitespace_normal()
                                        .child(note.clone()),
                                )
                            },
                        );
                    if let Some(header) = self.vault.rows.iter().find(|h| h.id == d.id) {
                        let mut date = format!(
                            "Added {}",
                            note_model::format_date(header.created_at, &*self.presentation.0)
                        );
                        if header.updated_at != header.created_at {
                            date.push_str(&format!(
                                " · Updated {}",
                                note_model::format_date(header.updated_at, &*self.presentation.0)
                            ));
                        }
                        pane = pane.child(
                            div()
                                .text_size(px(11. * t.zoom))
                                .text_color(t.text4)
                                .child(date),
                        );
                    }
                }
            }
            _ => {}
        }
        pane.into_any_element()
    }
    fn vault_gate(&self, cx: &mut Context<Self>, t: tokens::Tokens) -> Stateful<Div> {
        let checking = self.vault.loading || self.vault.supported.is_none();
        let unsupported = self.vault.supported == Some(false);
        let failed = self.vault.error.is_some() && self.vault.supported.is_none();
        let title = if failed {
            "Could not open Vault"
        } else if checking {
            "Opening Vault…"
        } else if unsupported {
            "Vault is unavailable"
        } else {
            "Vault is locked"
        };
        let description = if failed {
            "Try opening Vault again."
        } else if unsupported {
            "Vault is available on macOS and Windows for now."
        } else if checking {
            "Checking access to your saved items."
        } else if cfg!(windows) {
            "Windows Hello: your PIN, fingerprint or face"
        } else {
            "Touch ID or your Mac password"
        };
        let mut content = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(12. * t.zoom))
            .max_w(px(320. * t.zoom))
            .w_full()
            .child(vault_glyph(
                tezbar_ui_model::vault::Kind::Password,
                None,
                32.,
                t,
            ))
            .child(
                div()
                    .text_size(px(15. * t.zoom))
                    .font_weight(FontWeight::MEDIUM)
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(12. * t.zoom))
                    .text_color(t.text3)
                    .child(description),
            );
        if let Some(error) = &self.vault.error {
            content = content.child(
                div()
                    .text_size(px(12. * t.zoom))
                    .text_color(t.danger)
                    .child(error.clone()),
            );
        }
        if failed {
            content = content.child(self.action_button(
                "Retry",
                Action::Refresh,
                self.vault.loading,
                cx,
                t,
            ));
        } else if !checking && !unsupported {
            content = content.child(
                self.action_button(
                    if self.vault.busy {
                        "Unlocking…"
                    } else {
                        "Unlock Vault"
                    },
                    Action::Unlock,
                    self.vault.busy,
                    cx,
                    t,
                )
                .mt(px(4. * t.zoom))
                .px(px(20. * t.zoom))
                .py(px(8. * t.zoom)),
            );
        }
        div()
            .id("vault-gate")
            .flex()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .p(px(24. * t.zoom))
            .text_center()
            .child(content)
    }
}
fn vault_glyph(
    kind: tezbar_ui_model::vault::Kind,
    icon: Option<&str>,
    size: f32,
    t: tokens::Tokens,
) -> AnyElement {
    let id = tezbar_ui_model::vault::resolve_icon(kind, icon);
    let choice = vault::icon_choices()
        .iter()
        .find(|i| i.id == id)
        .unwrap_or(&vault::icon_choices()[0]);
    svg()
        .data(choice.svg.as_bytes())
        .size(px(size * t.zoom))
        .flex_shrink_0()
        .text_color(t.text2)
        .into_any_element()
}
impl Render for Utilities {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.surface == Surface::SystemMonitor {
            return self.render_monitor(w, cx);
        }
        let t = tokens::tokens(w);
        let title = super::super::command_bar::navigation::surface_title(&self.surface);
        let mut body = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .px(px(8. * t.zoom))
            .py(px(8. * t.zoom));
        if self.surface == Surface::Indexing {
            let snapshot = self.background.knowledge.as_ref().unwrap_or(&Value::Null);
            let status = &snapshot["status"];
            let number = |key: &str| status[key].as_u64().unwrap_or(0);
            let state = status["state"].as_str().unwrap_or("idle");
            let label = match state {
                "scanning" => "Scanning folders",
                "indexing" => "Indexing now",
                "paused" => "Paused",
                "failed" => "Needs attention",
                "completed" => "Up to date",
                _ => "Standing by",
            };
            let depth = snapshot["config"]["depth"]
                .as_str()
                .or_else(|| snapshot["depth"].as_str())
                .unwrap_or("balanced");
            let mut metrics = div().flex().gap(px(8. * t.zoom)).mt(px(10. * t.zoom));
            for (label, value, note) in [
                (
                    "Files",
                    number("sourceCount").to_string(),
                    format!("{} bytes source data", number("sourceBytes")),
                ),
                (
                    "Local index",
                    format!("{} bytes", snapshot["storageBytes"].as_u64().unwrap_or(0)),
                    format!("{} searchable chunks", number("chunkCount")),
                ),
                (
                    "Pages understood",
                    if number("totalPageCount") > 0 {
                        format!(
                            "{} / {}",
                            number("indexedPageCount"),
                            number("totalPageCount")
                        )
                    } else {
                        "—".into()
                    },
                    if number("partialSourceCount") > 0 {
                        format!("{} partially indexed", number("partialSourceCount"))
                    } else {
                        "No partial documents".into()
                    },
                ),
                (
                    "Last update",
                    if number("lastCompletedAt") == 0 {
                        "Not yet".into()
                    } else {
                        number("lastCompletedAt").to_string()
                    },
                    format!("{depth} depth"),
                ),
            ] {
                metrics = metrics.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .px(px(12. * t.zoom))
                        .py(px(10. * t.zoom))
                        .child(
                            div()
                                .text_size(px(11. * t.zoom))
                                .text_color(t.text3)
                                .child(label),
                        )
                        .child(
                            div()
                                .mt(px(4. * t.zoom))
                                .text_size(px(13. * t.zoom))
                                .font_weight(FontWeight::SEMIBOLD)
                                .truncate()
                                .child(value),
                        )
                        .child(
                            div()
                                .mt(px(2. * t.zoom))
                                .text_size(px(9.5 * t.zoom))
                                .text_color(t.text3)
                                .truncate()
                                .child(note),
                        ),
                );
            }
            body = body
                .p(px(12. * t.zoom))
                .child(
                    div()
                        .id("indexing-status")
                        .role(Role::Status)
                        .p(px(14. * t.zoom))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(10. * t.zoom))
                                        .child(div().size(px(8. * t.zoom)).rounded_full().bg(
                                            if state == "failed" {
                                                t.danger
                                            } else if state == "paused" {
                                                t.warning
                                            } else {
                                                t.success
                                            },
                                        ))
                                        .child(
                                            div()
                                                .text_size(px(15. * t.zoom))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(label),
                                        )
                                        .child(
                                            div()
                                                .rounded_full()
                                                .border_1()
                                                .border_color(t.hairline)
                                                .px(px(8. * t.zoom))
                                                .text_size(px(11. * t.zoom))
                                                .text_color(t.text3)
                                                .child(depth.to_string()),
                                        ),
                                )
                                .child(div().text_size(px(19. * t.zoom)).child(format!(
                                    "{:.0}%",
                                    self.background.index_progress * 100.
                                ))),
                        )
                        .child(
                            div()
                                .mt(px(6. * t.zoom))
                                .text_size(px(10.5 * t.zoom))
                                .text_color(t.text3)
                                .child(
                                    status["detail"]
                                        .as_str()
                                        .unwrap_or("Preparing local knowledge status…")
                                        .to_string(),
                                ),
                        )
                        .child(
                            div()
                                .mt(px(12. * t.zoom))
                                .h(px(6. * t.zoom))
                                .w_full()
                                .rounded_full()
                                .bg(t.hairline)
                                .child(
                                    div()
                                        .h_full()
                                        .rounded_full()
                                        .w(relative(self.background.index_progress))
                                        .bg(t.text3),
                                ),
                        )
                        .child(
                            div()
                                .mt(px(6. * t.zoom))
                                .flex()
                                .justify_between()
                                .text_size(px(9.5 * t.zoom))
                                .text_color(t.text3)
                                .child(format!("{} processed", number("processedSources")))
                                .child(format!(
                                    "{} remaining · {} failed",
                                    number("queuedSources"),
                                    number("failedSources")
                                )),
                        ),
                )
                .child(metrics);
        }
        if self.surface == Surface::Vault && !self.vault.model.unlocked {
            body = body.child(self.vault_gate(cx, t));
        } else if self.surface == Surface::Permissions {
            body = body.overflow_hidden().gap(px(8. * t.zoom)).px(px(20. * t.zoom)).py(px(16. * t.zoom))
                .child(div().mb(px(4. * t.zoom)).text_size(px(11.5 * t.zoom)).text_color(t.text3)
                    .child("System access used by automation, voice, calendar, and screen-aware features."));
            if let Some(message) = self
                .error
                .as_deref()
                .or_else(|| self.permissions.banner.as_ref().map(|b| b.1.as_str()))
            {
                body = body.child(
                    div()
                        .mb(px(12. * t.zoom))
                        .rounded(px(8. * t.zoom))
                        .border_1()
                        .border_color(t.hairline)
                        .bg(t.field)
                        .px(px(12. * t.zoom))
                        .py(px(8. * t.zoom))
                        .child(message.to_string()),
                );
            }
            if let Some(snapshot) = &self.permissions.snapshot {
                for status in &snapshot.statuses {
                    let descriptor = &status.descriptor;
                    let needs_action = !matches!(
                        status.state,
                        model::PermissionState::Granted | model::PermissionState::Unsupported
                    );
                    let state_label = match status.state {
                        model::PermissionState::Granted => "Granted",
                        model::PermissionState::Denied => "Denied",
                        model::PermissionState::Restricted => "Restricted",
                        model::PermissionState::NotDetermined => "Not granted",
                        model::PermissionState::Unsupported => "N/A",
                    };
                    body = body.child(
                        div()
                            .id(SharedString::from(format!(
                                "permission-{:?}",
                                descriptor.id
                            )))
                            .flex()
                            .items_center()
                            .gap(px(16. * t.zoom))
                            .rounded(px(8. * t.zoom))
                            .border_1()
                            .border_color(t.hairline)
                            .bg(t.field)
                            .px(px(14. * t.zoom))
                            .py(px(12. * t.zoom))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8. * t.zoom))
                                            .child(
                                                div()
                                                    .text_size(px(13. * t.zoom))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(descriptor.title.clone()),
                                            )
                                            .child(
                                                div()
                                                    .rounded(px(5. * t.zoom))
                                                    .px(px(6. * t.zoom))
                                                    .py(px(2. * t.zoom))
                                                    .bg(t.pill_strong)
                                                    .text_size(px(11. * t.zoom))
                                                    .child(state_label),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .mt(px(6. * t.zoom))
                                            .text_size(px(12. * t.zoom))
                                            .text_color(t.text3)
                                            .child(descriptor.rationale.clone()),
                                    )
                                    .when(needs_action, |d| {
                                        d.child(
                                            div()
                                                .mt(px(4. * t.zoom))
                                                .text_size(px(11.5 * t.zoom))
                                                .text_color(t.text3)
                                                .child(descriptor.remediation.clone()),
                                        )
                                    }),
                            )
                            .when(needs_action, |d| {
                                d.child(
                                    self.action_button_with_id(
                                        SharedString::from(format!("request-{:?}", descriptor.id)),
                                        if self.permissions.pending.as_ref() == Some(&descriptor.id)
                                        {
                                            "Opening..."
                                        } else {
                                            "Request"
                                        },
                                        Action::Permission(descriptor.id.clone()),
                                        self.permissions.pending.is_some(),
                                        cx,
                                        t,
                                    )
                                    .min_w(px(112. * t.zoom)),
                                )
                            })
                            .when(!needs_action, |d| {
                                d.child(
                                    div()
                                        .w(px(112. * t.zoom))
                                        .text_center()
                                        .text_size(px(11.5 * t.zoom))
                                        .text_color(t.text3)
                                        .child("Ready"),
                                )
                            }),
                    );
                }
            } else {
                body = body.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child("Loading..."),
                );
            }
        } else if self.surface == Surface::ReleaseNotes {
            body = body
                .text_size(px(14. * t.zoom))
                .text_color(t.text3)
                .overflow_hidden()
                .px(px(20. * t.zoom))
                .py(px(16. * t.zoom))
                .child(
                    div()
                        .mb(px(12. * t.zoom))
                        .text_size(px(12. * t.zoom))
                        .text_color(t.text3)
                        .child("Release notes for your Tezbar version."),
                );
            body = match &self.release.model.state {
                tezbar_update::notes::State::Ready(n) => body
                    .when(n.body.trim().is_empty(), |d| {
                        d.child("No release notes were published for this version.")
                    })
                    .when_some(self.release.markdown.clone(), |d, m| d.child(m)),
                tezbar_update::notes::State::Error(_) => body
                    .child("Couldn’t load the Tezbar release notes. Please try again.")
                    .child(self.action_button("Retry", Action::Refresh, false, cx, t)),
                _ => body.child(
                    div()
                        .id("release-notes-loading")
                        .role(Role::Status)
                        .aria_label("Loading release notes")
                        .flex()
                        .flex_col()
                        .gap(px(12. * t.zoom))
                        .child(
                            div()
                                .h(px(16. * t.zoom))
                                .w(relative(2. / 3.))
                                .rounded(px(4. * t.zoom))
                                .bg(t.pill_strong),
                        )
                        .child(
                            div()
                                .h(px(16. * t.zoom))
                                .w_full()
                                .rounded(px(4. * t.zoom))
                                .bg(t.pill),
                        )
                        .child(
                            div()
                                .h(px(16. * t.zoom))
                                .w(relative(0.8))
                                .rounded(px(4. * t.zoom))
                                .bg(t.pill),
                        ),
                ),
            };
        } else if !self.fields.is_empty() {
            if self.surface == Surface::Vault {
                let draft = self.vault.model.draft.as_ref().unwrap();
                body = body.child(
                    self.action_button("Change icon", Action::ChangeIcon, false, cx, t)
                        .track_focus(&self.icon_focus)
                        .text_size(px(0.))
                        .p_0()
                        .size(px(32. * t.zoom))
                        .child(vault_glyph(draft.kind, draft.icon.as_deref(), 18., t)),
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
                            .p_0()
                            .size(px(28. * t.zoom))
                            .bg(if index == self.vault.icon_selected {
                                t.pill_strong
                            } else {
                                transparent_black()
                            })
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
            let icon_controls = body.p_0().flex_none();
            body =
                div().flex().flex_col().flex_1().min_h_0().gap(px(
                    if self.surface == Surface::Vault {
                        12.
                    } else {
                        0.
                    } * t.zoom,
                ));
            let mut icon_controls = Some(icon_controls);
            for i in if self.surface == Surface::Vault {
                vec![1, 0, 2]
            } else {
                (0..self.fields.len()).collect()
            } {
                if self.surface == Surface::Vault && i == 2 && !self.vault.show_note {
                    body = body.child(
                        controls::button("vault-add-note", "Add note", ButtonTone::Ghost, false, t)
                            .debug_selector(|| "Add note".into())
                            .text_size(px(11.5 * t.zoom))
                            .px_0()
                            .py_0()
                            .text_color(t.text3)
                            .self_start()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.vault.show_note = true;
                                cx.notify();
                            })),
                    );
                    continue;
                }
                let field = &self.fields[i];
                body = body.child(
                    div()
                        .id(SharedString::from(format!("utility-field-{i}")))
                        .debug_selector(move || format!("utility-field-{i}"))
                        .p(px(if self.surface == Surface::Vault {
                            0.
                        } else {
                            8.
                        } * t.zoom))
                        .when(
                            matches!(self.surface, Surface::Vault | Surface::Snippets),
                            |d| {
                                d.child(
                                    div()
                                        .mb(px(6. * t.zoom))
                                        .text_size(px(11. * t.zoom))
                                        .text_color(t.text3)
                                        .child(if self.surface == Surface::Vault {
                                            ["Name", "Value", "Note"][i.min(2)]
                                        } else {
                                            ["Title", "Trigger", "Body"][i.min(2)]
                                        }),
                                )
                            },
                        )
                        .child(if self.surface == Surface::Vault && i == 0 {
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8. * t.zoom))
                                .child(icon_controls.take().unwrap())
                                .child(div().flex_1().min_w_0().child(field.clone()))
                                .into_any_element()
                        } else {
                            field.clone().into_any_element()
                        }),
                );
            }
            if self.surface == Surface::Vault {
                body = body.child(self.toolbar(cx, t).p_0().pt(px(4. * t.zoom)));
                body = div().flex().flex_col().flex_1().min_h_0().child(
                    div()
                        .id("vault-editor")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .p(px(16. * t.zoom))
                        .child(div().w_full().child(body)),
                );
                let mut master = div()
                    .flex()
                    .flex_col()
                    .w(relative(0.44))
                    .max_w(px(340. * t.zoom))
                    .min_h_0();
                for (i, (id, label)) in self.rows().iter().enumerate() {
                    master = master.child(
                        div()
                            .id(SharedString::from(format!("vault-draft-row-{id}")))
                            .px(px(8. * t.zoom))
                            .py(px(6. * t.zoom))
                            .rounded(px(8. * t.zoom))
                            .bg(if self.selected() == i {
                                t.selected
                            } else {
                                transparent_black()
                            })
                            .child(self.list_content(id, label, t)),
                    );
                }
                body = div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .gap(px(12. * t.zoom))
                    .child(master)
                    .child(body.border_l_1().border_color(t.hairline));
            }
        } else {
            if self.surface == Surface::SystemMonitor {
                let mut tabs = div()
                    .flex()
                    .flex_wrap()
                    .gap(px(4. * t.zoom))
                    .px(px(12. * t.zoom))
                    .pb(px(8. * t.zoom))
                    .text_size(px(12. * t.zoom));
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
            let mut split = div().flex().flex_1().min_h_0().gap(px(12. * t.zoom));
            if count == 0 {
                if self.surface == Surface::Vault {
                    let filtered = !self.vault.query.trim().is_empty() || self.vault.kind.is_some();
                    split = split.items_center().justify_center().child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(10. * t.zoom))
                            .p(px(24. * t.zoom))
                            .text_center()
                            .child(vault_glyph(
                                tezbar_ui_model::vault::Kind::Password,
                                None,
                                28.,
                                t,
                            ))
                            .child(div().font_weight(FontWeight::MEDIUM).child(
                                if self.vault.loading {
                                    "Loading items…"
                                } else if filtered {
                                    "No matching items"
                                } else {
                                    "No secrets yet"
                                },
                            ))
                            .child(div().text_size(px(12. * t.zoom)).text_color(t.text3).child(
                                if filtered {
                                    "Try another name."
                                } else {
                                    "Press ⌘N and paste"
                                },
                            )),
                    );
                } else {
                    split = split.child(div().p(px(16.)).text_color(t.text3).child(
                    if self.status().0 {
                        "Loading…"
                    } else {
                        match self.surface {
                            Surface::Clipboard=>"Clipboard is empty",
                            Surface::Snippets=>"No snippets match your search.",
                            Surface::Notes=>"No notes found",
                            Surface::Vault if self.vault.supported==Some(false)=>"Vault is not supported on this platform.",
                            Surface::Vault if !self.vault.model.unlocked=>"Unlock Vault to view saved items.",
                            Surface::Vault=>"No Vault items match. Clear the search or add an item.",
                            Surface::Emoji=>"No emojis found for that query.",
                            Surface::Indexing=>"No files match this filter. Folder and depth changes live in Knowledge Settings.",
                            _=>"No system data matched. Clear the search or refresh.",
                        }
                    },
                ));
                }
            } else if self.surface == Surface::Emoji {
                let visible = self.emoji.visible();
                let mut sections: Vec<(String, Vec<(usize, String)>)> = Vec::new();
                let recent_count =
                    if self.emoji.query.trim().is_empty() && self.emoji.category.is_none() {
                        self.emoji
                            .recent
                            .iter()
                            .filter(|s| {
                                tezbar_core::instant::emoji::DATA
                                    .iter()
                                    .any(|e| e.char == ***s)
                            })
                            .count()
                    } else {
                        0
                    };
                for (i, emoji) in visible.iter().enumerate() {
                    let heading = if !self.emoji.query.trim().is_empty() {
                        String::new()
                    } else if i < recent_count {
                        "Recently Used".into()
                    } else {
                        tezbar_core::instant::emoji::DATA
                            .iter()
                            .find(|e| e.char == **emoji)
                            .map(|e| e.category.clone())
                            .unwrap_or_default()
                    };
                    if sections.last().is_none_or(|s| s.0 != heading) {
                        sections.push((heading, vec![]));
                    }
                    sections.last_mut().unwrap().1.push((i, emoji.to_string()));
                }
                let mut grid_body = div()
                    .id("emoji-grid")
                    .debug_selector(|| "emoji-grid".into())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(12. * t.zoom))
                    .pt(px(12. * t.zoom));
                for (heading, entries) in sections {
                    let mut grid = div().flex().flex_col().gap(px(4. * t.zoom)).w_full();
                    if heading == "Recently Used" {
                        let mut recent = div().flex().flex_wrap().gap(px(4. * t.zoom));
                        for (index, emoji) in &entries {
                            recent = recent.child(
                                self.emoji_cell(*index, emoji, cx, t)
                                    .w(px(36. * t.zoom))
                                    .flex_shrink_0(),
                            );
                        }
                        grid = grid.child(recent);
                    } else {
                        for chunk in entries.chunks(8) {
                            let mut row = div().flex().gap(px(4. * t.zoom)).w_full();
                            for (index, emoji) in chunk {
                                row = row.child(
                                    self.emoji_cell(*index, emoji, cx, t)
                                        .flex_1()
                                        .min_w_0()
                                        .px_0(),
                                );
                            }
                            for _ in chunk.len()..8 {
                                row = row.child(div().flex_1().min_w_0());
                            }
                            grid = grid.child(row);
                        }
                    }
                    grid_body = grid_body.child(
                        div()
                            .mb(px(12. * t.zoom))
                            .when(!heading.is_empty(), |d| {
                                d.child(
                                    div()
                                        .mb(px(4. * t.zoom))
                                        .text_size(px(11. * t.zoom))
                                        .text_color(t.text3)
                                        .child(heading),
                                )
                            })
                            .child(grid),
                    );
                }
                split = split.child(grid_body);
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
                                    .when(this.surface == Surface::Vault, |d| {
                                        d.bg(if this.selected() == i {
                                            t.selected
                                        } else {
                                            transparent_black()
                                        })
                                        .text_color(t.text1)
                                    })
                                    .h(px(if matches!(
                                        this.surface,
                                        Surface::Notes | Surface::SystemMonitor
                                    ) {
                                        64.
                                    } else if matches!(
                                        this.surface,
                                        Surface::Vault
                                            | Surface::Clipboard
                                            | Surface::Snippets
                                            | Surface::Indexing
                                    ) {
                                        48.
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
                                        move |this, _, w, cx| {
                                            this.act(Action::Select(i), w, cx);
                                            if this.surface == Surface::Snippets {
                                                this.act(Action::Activate, w, cx);
                                            }
                                        },
                                    ))
                                })
                                .collect::<Vec<_>>()
                        }),
                    )
                    .track_scroll(&self.scroll)
                    .debug_selector(|| "utility-master".into())
                    .flex_1()
                    .when(
                        matches!(
                            self.surface,
                            Surface::Vault
                                | Surface::Clipboard
                                | Surface::Notes
                                | Surface::Snippets
                        ),
                        |list| list.flex_none().w(relative(0.44)).max_w(px(340. * t.zoom)),
                    ),
                );
                if matches!(
                    self.surface,
                    Surface::Clipboard | Surface::Notes | Surface::Snippets | Surface::Vault
                ) {
                    split = split.child(self.preview(cx, t));
                }
            }
            if count == 0
                && matches!(
                    self.surface,
                    Surface::Clipboard | Surface::Notes | Surface::Snippets | Surface::Vault
                )
            {
                split = div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap(px(12. * t.zoom))
                    .child(split.flex_none().w(relative(0.44)).max_w(px(340. * t.zoom)))
                    .child(self.preview(cx, t));
            }
            body = body.child(split).when(
                self.surface == Surface::Indexing
                    && self.indexing.page.as_ref().is_some_and(|p| p.has_more),
                |d| {
                    d.child(self.action_button(
                        "Load more",
                        Action::MoreSources,
                        self.indexing.loading,
                        cx,
                        t,
                    ))
                },
            );
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
        } else if self.surface == Surface::Vault && !self.vault.model.unlocked {
            if self.vault.busy {
                "Unlocking…".into()
            } else if self.vault.supported == Some(false) {
                "Unavailable".into()
            } else if self.vault.supported.is_none() {
                "Opening Vault…".into()
            } else {
                "Locked".into()
            }
        } else {
            format!("{} items", self.rows().len())
        };
        let body = body.pb(px(38. * t.zoom));
        let mut root = controls::sheet(t)
            .text_size(px(12.5 * t.zoom))
            .relative()
            .line_height(relative(1.5))
            .id("utilities")
            .role(Role::Group)
            .aria_label(title)
            .track_focus(&self.focus)
            .key_context("Utilities")
            .on_key_down(cx.listener(Self::key))
            .when(self.surface == Surface::Vault, |d| {
                d.text_size(px(12.5 * t.zoom))
            })
            .child(
                controls::header(t)
                    .id("utility-header")
                    .debug_selector(|| "utility-header".into())
                    .gap(px(8. * t.zoom))
                    .child(controls::back_button(
                        self.action_button("Back", Action::Cancel, false, cx, t),
                        t,
                    ))
                    .when(
                        matches!(self.surface, Surface::Permissions | Surface::ReleaseNotes)
                            || (self.surface == Surface::Vault
                                && (!self.vault.model.unlocked || !self.fields.is_empty()))
                            || (self.surface == Surface::Snippets && !self.fields.is_empty()),
                        |d| {
                            d.child(div().flex_1().text_size(px(15. * t.zoom)).child(
                                if self.surface == Surface::ReleaseNotes {
                                    "What’s new in Tezbar"
                                } else if self.surface == Surface::Vault && !self.fields.is_empty()
                                {
                                    if self
                                        .vault
                                        .model
                                        .draft
                                        .as_ref()
                                        .is_some_and(|d| d.id.is_some())
                                    {
                                        "Edit secret"
                                    } else {
                                        "New secret"
                                    }
                                } else {
                                    title
                                },
                            ))
                        },
                    )
                    .when(
                        matches!(
                            self.surface,
                            Surface::Clipboard
                                | Surface::Notes
                                | Surface::Emoji
                                | Surface::Indexing
                        ) || (matches!(self.surface, Surface::Vault | Surface::Snippets)
                            && self.fields.is_empty()
                            && (self.surface != Surface::Vault || self.vault.model.unlocked)),
                        |d| {
                            d.child(
                                div()
                                    .id("utility-search")
                                    .debug_selector({
                                        let vault = self.surface == Surface::Vault;
                                        move || {
                                            if vault {
                                                "vault-search".into()
                                            } else {
                                                "utility-search".into()
                                            }
                                        }
                                    })
                                    .flex_1()
                                    .min_w_0()
                                    .child(self.search.clone()),
                            )
                        },
                    )
                    .when(
                        matches!(
                            self.surface,
                            Surface::Notes | Surface::Snippets | Surface::Vault
                        ) && self.fields.is_empty()
                            && (self.surface != Surface::Vault || self.vault.model.unlocked),
                        |d| {
                            d.child(self.icon_action(
                                "utility-new",
                                "New",
                                "+",
                                Action::New,
                                false,
                                cx,
                                t,
                            ))
                        },
                    )
                    .when(self.surface == Surface::Notes, |d| {
                        d.child(self.icon_action(
                            "notes-delete",
                            "Delete",
                            "×",
                            Action::Delete,
                            self.notes.selected.is_none(),
                            cx,
                            t,
                        ))
                    })
                    .when(
                        matches!(self.surface, Surface::Permissions | Surface::Indexing),
                        |d| {
                            d.child(self.icon_action(
                                "utility-refresh",
                                "Refresh",
                                "↻",
                                Action::Refresh,
                                self.status().0,
                                cx,
                                t,
                            ))
                        },
                    )
                    .when(self.surface == Surface::Indexing, |d| {
                        d.child(self.icon_action(
                            "index-settings",
                            "Edit in Settings",
                            "⚙",
                            Action::Settings,
                            false,
                            cx,
                            t,
                        ))
                    })
                    .when(
                        matches!(self.surface, Surface::Clipboard | Surface::Emoji),
                        |d| d.child(self.utility_chips(cx, t)),
                    )
                    .when(self.surface == Surface::ReleaseNotes, |d| {
                        d.child(div().text_size(px(12. * t.zoom)).text_color(t.text3).child(
                            format!(
                                    "v{}",
                                    self.release
                                        .version
                                        .as_deref()
                                        .unwrap_or(&self.release.current)
                                ),
                        ))
                    }),
            )
            .child(
                if matches!(self.surface, Surface::Permissions | Surface::ReleaseNotes) {
                    div()
                        .id("utility-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(body)
                        .into_any_element()
                } else {
                    body.when(
                        !self.fields.is_empty() && self.surface != Surface::Vault,
                        |d| d.child(self.toolbar(cx, t)),
                    )
                    .into_any_element()
                },
            )
            .child(
                controls::footer(t)
                    .id("utility-status")
                    .debug_selector(|| "utility-footer".into())
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .role(Role::Status)
                    .aria_label("Utility status")
                    .when(
                        !matches!(self.surface, Surface::ReleaseNotes | Surface::Permissions),
                        |d| d.child(title).child(status),
                    )
                    .child(div().flex_1())
                    .children(self.footer_hints(t)),
            );
        if self.confirmation.is_some() {
            root = root.child(
                controls::popover("utility-confirmation", "Confirm deletion", t)
                    .child(if matches!(self.confirmation, Some((Action::Clear, _))) {
                        "Clear all clipboard history? This cannot be undone."
                    } else {
                        "Delete the selected items? This cannot be undone."
                    })
                    .child(self.action_button_with_id(
                        "utility-confirm-accept",
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
                    .child(self.action_button_with_id(
                        "utility-confirm-cancel",
                        "Cancel",
                        Action::Confirm(false),
                        false,
                        cx,
                        t,
                    )),
            );
        }
        root
    }
}

/// Shortcut hint text for the primary modifier handled by `Modifiers::secondary`:
/// `⌘` on macOS, `Ctrl` on Windows/Linux.
pub(super) fn platform_keys(keys: &str) -> String {
    if cfg!(target_os = "macos") {
        keys.to_owned()
    } else {
        keys.replace("⌘ ", "Ctrl+").replace('⌘', "Ctrl+")
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

#[cfg(test)]
mod svg_namespace_tests {
    /// usvg renders nothing for an `<svg>` root without the SVG namespace, which left
    /// clipboard kind icons and conversion chips blank.
    #[::core::prelude::v1::test]
    fn inline_icons_declare_the_svg_namespace() {
        let open = concat!("<", "svg ");
        for (file, source) in [
            ("utilities/view.rs", include_str!("view.rs")),
            ("utilities/system_monitor.rs", include_str!("system_monitor.rs")),
            ("command_bar/view.rs", include_str!("../command_bar/view.rs")),
            ("command_bar/icons.rs", include_str!("../command_bar/icons.rs")),
            ("settings/view.rs", include_str!("../settings/view.rs")),
        ] {
            for (at, _) in source.match_indices(open) {
                let end = source[at..].find('>').map_or(source.len(), |e| at + e);
                let tag = &source[at..end];
                // Raw strings spell `xmlns="…"`, escaped strings `xmlns=\"…\"`.
                assert!(
                    tag.contains("xmlns=") && tag.contains("http://www.w3.org/2000/svg"),
                    "{file}: inline icon without SVG namespace: {tag}"
                );
            }
        }
    }
}
