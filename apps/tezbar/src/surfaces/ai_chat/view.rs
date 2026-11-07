use super::*;
use tezbar_ui::{
    controls::{self, ButtonTone},
    tokens::{self, Tokens},
};
#[derive(Clone)]
pub(super) enum Row {
    Header(Arc<Turn>),
    Stage(Arc<Turn>, usize),
    Text {
        id: String,
        turn: Arc<Turn>,
        range: std::ops::Range<usize>,
        settled: bool,
    },
    End(Arc<Turn>, bool),
}
pub(super) fn project(
    turns: &[Arc<Turn>],
    live: Option<&str>,
    timeline: &[chat::TimelineItem],
) -> Vec<Row> {
    let mut rows = vec![];
    for t in turns {
        rows.push(Row::Header(t.clone()));
        let streaming = live == Some(t.id.as_str());
        if streaming && !timeline.is_empty() {
            for item in timeline {
                match item {
                    chat::TimelineItem::Stage(index) => {
                        if let Some(n) = t.stages.iter().position(|s| s.index == *index) {
                            rows.push(Row::Stage(t.clone(), n));
                        }
                    }
                    chat::TimelineItem::Text { start, end } => {
                        if t.text.get(*start..*end).is_some() {
                            rows.push(Row::Text {
                                id: format!("{}-text-{start}", t.id),
                                turn: t.clone(),
                                range: *start..*end,
                                settled: false,
                            });
                        }
                    }
                }
            }
        } else {
            for n in 0..t.stages.len() {
                rows.push(Row::Stage(t.clone(), n));
            }
            if !t.text.is_empty() {
                rows.push(Row::Text {
                    id: t.id.clone(),
                    turn: t.clone(),
                    range: 0..t.text.len(),
                    settled: true,
                });
            }
        }
        rows.push(Row::End(t.clone(), streaming));
    }
    rows
}
fn relative(at: u64) -> String {
    let seconds = now().saturating_sub(at) / 1000;
    match seconds {
        0..=59 => "Just now".into(),
        60..=3599 => format!("{}m ago", seconds / 60),
        3600..=86399 => format!("{}h ago", seconds / 3600),
        _ => format!("{}d ago", seconds / 86400),
    }
}
impl ChatView {
    pub(super) fn b(
        &self,
        id: impl Into<String>,
        label: impl Into<SharedString>,
        action: Action,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let view = cx.entity().downgrade();
        controls::action_button(
            id.into(),
            label,
            ButtonTone::Quiet,
            false,
            t,
            move |w, cx| {
                let _ = view.update(cx, |v, cx| v.action(action.clone(), w, cx));
            },
        )
    }
    fn history_rows(&self, cx: &App) -> Vec<chat::HistoryRow> {
        let q = self.filter.read(cx).content.to_lowercase();
        self.controller
            .history
            .as_ref()
            .into_iter()
            .flatten()
            .filter(|r| {
                format!("{} {}", r.title, r.preview)
                    .to_lowercase()
                    .contains(&q)
            })
            .cloned()
            .collect()
    }
    fn history(&mut self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let rows = self.history_rows(cx);
        let count = rows.len();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .p(px(12.))
            .gap(px(8.))
            .child(self.filter.clone())
            .when(count == 0, |d| {
                d.child(
                    div()
                        .id("history-status")
                        .role(Role::Status)
                        .p(px(20.))
                        .text_color(t.text3)
                        .child(if self.history_error {
                            "Could not load conversations."
                        } else {
                            chat::history_placeholder(self.loading)
                        }),
                )
            })
            .child(
                uniform_list(
                    "past-chats",
                    count,
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|i| {
                                let row = &rows[i];
                                div()
                                    .id(format!("chat-history-{}", row.id))
                                    .role(Role::ListBoxOption)
                                    .aria_label(row.title.clone())
                                    .aria_position_in_set(i + 1)
                                    .aria_size_of_set(count)
                                    .flex()
                                    .items_center()
                                    .h(px(56.))
                                    .gap(px(8.))
                                    .child(
                                        this.b(
                                            format!("resume-{}", row.id),
                                            row.title.clone(),
                                            Action::Resume(row.id.clone()),
                                            t,
                                            cx,
                                        )
                                        .flex_1()
                                        .child(
                                            div().text_size(px(10.)).text_color(t.text3).child(
                                                format!(
                                                    "{} · {}",
                                                    row.preview,
                                                    relative(row.updated_at)
                                                ),
                                            ),
                                        ),
                                    )
                                    .child(this.b(
                                        format!("delete-{}", row.id),
                                        "Delete",
                                        Action::Delete(row.id.clone()),
                                        t,
                                        cx,
                                    ))
                                    .into_any_element()
                            })
                            .collect()
                    }),
                )
                .flex_1()
                .min_h_0(),
            )
            .child(self.b("clear-all", "Clear all conversations", Action::Clear, t, cx))
            .into_any_element()
    }
    fn row(&mut self, index: usize, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.projection.get(index).cloned() else {
            return div().into_any_element();
        };
        match row {
            Row::Header(turn) => {
                let session = self.controller.active.clone().unwrap_or_default();
                let mut row = div()
                    .px(px(20.))
                    .pt(px(16.))
                    .text_size(px(11.))
                    .text_color(t.text3)
                    .child(if turn.role == chat::Role::User {
                        "You"
                    } else {
                        "Agent"
                    });
                for (index, r) in turn.references.iter().enumerate() {
                    if let Reference::Image { name, .. } = r {
                        let key = references::thumbnail_key(&session, &turn.id, index, r);
                        row = row.child(if let Some(image) = self.images.get(&key) {
                            div().child(
                                img(image)
                                    .max_h(px(140.))
                                    .max_w(px(220.))
                                    .id(format!("history-image-{}-{index}", turn.id))
                                    .aria_label(name.clone()),
                            )
                        } else {
                            div().child(format!("Image · {name} · preview unavailable or loading"))
                        });
                    }
                }
                row.into_any_element()
            }
            Row::Text { id, .. } => {
                let height = self
                    .documents
                    .get(&id)
                    .map(|d| {
                        let document = &d.view.read(cx).document;
                        let lines = document
                            .blocks
                            .iter()
                            .map(|b| {
                                let text = b.text();
                                text.lines()
                                    .map(|l| l.chars().count() / 80 + 1)
                                    .sum::<usize>()
                                    .max(1)
                                    + 1
                            })
                            .sum::<usize>();
                        (lines as f32 * 20. + 12.).clamp(48., 440.)
                    })
                    .unwrap_or(48.);
                div()
                    .px(px(20.))
                    .py(px(6.))
                    .h(px(height))
                    .children(self.documents.get(&id).map(|d| d.view.clone()))
                    .into_any_element()
            }
            Row::Stage(turn, n) => {
                let stage = &turn.stages[n];
                let key = format!("{}-{}", turn.id, stage.index);
                let expanded = self.expanded.contains(&key);
                let label = format!(
                    "{} {}{}",
                    match stage.status {
                        chat::StageStatus::Running => "…",
                        chat::StageStatus::Done => "✓",
                        chat::StageStatus::Failed => "×",
                    },
                    stage.label,
                    stage
                        .duration_ms
                        .map(|ms| format!(" · {ms} ms"))
                        .unwrap_or_default()
                );
                div()
                    .px(px(20.))
                    .py(px(3.))
                    .child(
                        self.b(
                            format!("stage-{key}"),
                            label,
                            Action::Expand(key.clone()),
                            t,
                            cx,
                        )
                        .aria_expanded(expanded),
                    )
                    .when(expanded, |d| {
                        d.child(
                            div()
                                .text_size(px(11.))
                                .text_color(t.text2)
                                .children(stage.detail.clone())
                                .child(
                                    div()
                                        .id(format!("output-{key}"))
                                        .max_h(px(180.))
                                        .overflow_y_scroll()
                                        .child(
                                            stage
                                                .output
                                                .as_deref()
                                                .unwrap_or("")
                                                .chars()
                                                .take(32768)
                                                .collect::<String>(),
                                        ),
                                )
                                .when(stage.output_truncated, |d| d.child("Output truncated"))
                                .when(stage.output.is_some(), |d| {
                                    d.child(self.b(
                                        format!("copy-{key}"),
                                        "Copy output",
                                        Action::Copy(stage.output.clone().unwrap_or_default()),
                                        t,
                                        cx,
                                    ))
                                }),
                        )
                    })
                    .into_any_element()
            }
            Row::End(turn, live) => {
                let retry = self.active().is_some_and(|c| {
                    !c.busy() && c.context.back().is_some_and(|t| t.id == turn.id)
                }) && turn.role == chat::Role::Assistant
                    && turn.error.is_some();
                div()
                    .px(px(20.))
                    .pb(px(12.))
                    .text_size(px(11.))
                    .text_color(t.text3)
                    .when(live, |d| {
                        d.child(
                            div()
                                .id(format!("working-{}", turn.id))
                                .role(Role::Status)
                                .child("Working…"),
                        )
                    })
                    .when_some(turn.error.clone(), |d, e| {
                        d.child(
                            div()
                                .id(format!("turn-error-{}", turn.id))
                                .role(Role::Alert)
                                .text_color(t.danger)
                                .child(chat::format_error(
                                    &e,
                                    turn.response_meta
                                        .as_ref()
                                        .map(|m| m.provider.as_str())
                                        .unwrap_or(""),
                                )),
                        )
                    })
                    .when_some(turn.response_meta.clone(), |d, m| {
                        d.child(format!(
                            "{} · {}{}",
                            m.provider_title,
                            m.model,
                            m.token_count
                                .map(|n| format!(" · {n} tokens"))
                                .unwrap_or_default()
                        ))
                    })
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .when(retry, |d| {
                                d.child(self.b(
                                    format!("retry-{}", turn.id),
                                    "Try again",
                                    Action::Retry(turn.id.clone()),
                                    t,
                                    cx,
                                ))
                            })
                            .when(!live && !turn.text.is_empty(), |d| {
                                d.child(self.b(
                                    format!("copy-{}", turn.id),
                                    "Copy",
                                    Action::Copy(turn.text.clone()),
                                    t,
                                    cx,
                                ))
                            })
                            .when(!live, |d| {
                                d.child(self.b(
                                    format!("remove-{}", turn.id),
                                    "Delete message",
                                    Action::DeleteTurn(turn.id.clone()),
                                    t,
                                    cx,
                                ))
                            }),
                    )
                    .into_any_element()
            }
        }
    }
    fn diagnostics_view(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let lines = self
            .controller
            .active
            .as_ref()
            .and_then(|id| self.diagnostics.get(id));
        let key = "diagnostics".to_string();
        div()
            .when_some(lines, |d, lines| {
                d.child(self.b(
                    "diagnostics",
                    format!("Diagnostics ({})", lines.len()),
                    Action::Expand(key.clone()),
                    t,
                    cx,
                ))
                .when(self.expanded.contains(&key), |d| {
                    d.child(
                        div()
                            .id("diagnostics-text")
                            .max_h(px(100.))
                            .overflow_y_scroll()
                            .text_size(px(11.))
                            .text_color(t.text3)
                            .child(lines.join("\n")),
                    )
                })
            })
            .into_any_element()
    }
    /// Core ai/ext formats and scrubs tool JSON before emitting `command`.
    /// Keep this exact body at the rendered data seam, including indentation.
    pub(crate) fn approval_body(&self) -> Option<String> {
        self.active()
            .and_then(|c| c.approval.as_ref())
            .map(|a| a.command.clone())
    }
    pub(super) fn approval(&self, t: Tokens, cx: &mut Context<Self>) -> Stateful<Div> {
        let Some(approval) = self.active().and_then(|c| c.approval.clone()) else {
            return div().id("no-tool-approval");
        };
        div()
            .id("tool-approval")
            .role(Role::AlertDialog)
            .aria_label(approval.title.clone())
            .mx(px(16.))
            .p(px(12.))
            .border_1()
            .border_color(t.rim)
            .rounded(px(7.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .text_size(px(12.))
            .child(approval.title)
            .child(
                div()
                    .id("approval-command")
                    .max_h(px(130.))
                    .overflow_y_scroll()
                    .text_color(t.text2)
                    .child(self.approval_body().unwrap_or_default()),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        self.b(
                            "allow-once",
                            "Allow once",
                            Action::Decide(Decision::Once),
                            t,
                            cx,
                        )
                        .track_focus(&self.allow_focus),
                    )
                    .child(
                        self.b("deny", "Deny", Action::Decide(Decision::Deny), t, cx)
                            .track_focus(&self.deny_focus),
                    )
                    .when(approval.can_remember != Some(false), |d| {
                        d.child(
                            self.b(
                                "allow-always",
                                if approval.remember_kind.as_deref() == Some("tool") {
                                    "Always allow this tool"
                                } else {
                                    "Always allow this command"
                                },
                                Action::Decide(Decision::Always),
                                t,
                                cx,
                            )
                            .track_focus(&self.always_focus),
                        )
                    }),
            )
    }
    fn composer(&mut self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let s = self.selection();
        let busy = self.active().is_some_and(|c| c.busy());
        let attachments = self
            .active()
            .map(|c| c.attachments.clone())
            .unwrap_or_default();
        let unsaved = self.active().is_some_and(|c| c.unsaved());
        let mut body = div().px(px(16.)).py(px(10.)).flex().flex_col().gap(px(8.));
        if let Some(error) = self.active().and_then(|c| c.input_error.clone()) {
            body = body.child(
                div()
                    .id("chat-error")
                    .role(Role::Alert)
                    .text_color(t.danger)
                    .text_size(px(12.))
                    .child(error),
            );
        }
        if let Some(cwd) = &self.project {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .text_size(px(11.))
                    .child("This folder has AI extensions. Not running.")
                    .child(self.b(
                        format!("review-{cwd}"),
                        "Review",
                        Action::ReviewProject,
                        t,
                        cx,
                    ))
                    .child(self.b("dismiss-project", "Dismiss", Action::DismissProject, t, cx)),
            );
        }
        if self.directories_open {
            for (n, p) in self.recent_directories.iter().enumerate() {
                body = body.child(self.b(
                    format!("recent-cwd-{n}"),
                    p.clone(),
                    Action::SetDirectory(p.clone()),
                    t,
                    cx,
                ));
            }
            body = body.child(self.b(
                "choose-directory",
                "Choose another folder…",
                Action::Directory,
                t,
                cx,
            ));
        }
        if let Some(profile) = s.profile {
            let title = self
                .items
                .iter()
                .find(|i| i["kind"] == "agents" && i["id"] == profile)
                .and_then(|i| i["title"].as_str().or_else(|| i["name"].as_str()))
                .map(str::to_string)
                .unwrap_or_else(|| format!("{profile} (unavailable)"));
            body = body.child(
                div()
                    .flex()
                    .text_size(px(11.))
                    .child(format!("Profile · {title}"))
                    .child(self.b(
                        "clear-profile",
                        "Remove profile",
                        Action::ClearProfile,
                        t,
                        cx,
                    )),
            );
        }
        let slash = self.slash_items(cx);
        let mut commands = div()
            .id("ai-commands")
            .role(Role::ListBox)
            .aria_label("AI commands")
            .max_h(px(180.))
            .overflow_y_scroll();
        for (n, i) in slash.iter().enumerate() {
            let item = &self.items[*i];
            commands = commands.child(
                self.b(
                    format!("command-{i}"),
                    format!(
                        "/{}  {} · {}",
                        item["name"].as_str().unwrap_or(""),
                        item["title"].as_str().unwrap_or(""),
                        item["extensionName"].as_str().unwrap_or("")
                    ),
                    Action::ChoosePrompt(*i),
                    t,
                    cx,
                )
                .role(Role::ListBoxOption)
                .aria_selected(n == self.slash_index)
                .when(n == self.slash_index, |d| d.aria_active_descendant()),
            );
        }
        if !slash.is_empty() {
            body = body.child(commands);
        }
        if let Some(prompt) = &self.prompt {
            for name in prompt["arguments"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|a| a["name"].as_str())
            {
                let Some(input) = self.arguments.get(name) else {
                    continue;
                };
                body = body.child(
                    div()
                        .text_size(px(11.))
                        .child(name.to_string())
                        .child(input.clone()),
                );
            }
            body = body.child(
                div()
                    .flex()
                    .gap(px(6.))
                    .child(self.b(
                        "insert-prompt",
                        "Insert in draft",
                        Action::ResolvePrompt,
                        t,
                        cx,
                    ))
                    .child(self.b("cancel-prompt", "Cancel", Action::CancelPrompt, t, cx)),
            );
        }
        for (n, r) in attachments.iter().enumerate() {
            if let Reference::Image { blob_id, name, .. } = r {
                if let Some(image) = self.images.get(blob_id) {
                    body = body.child(
                        img(image)
                            .max_h(px(100.))
                            .max_w(px(180.))
                            .id(format!("image-preview-{n}"))
                            .aria_label(name.clone()),
                    );
                }
            }
            if let Reference::Image { name, .. } = r {
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .text_size(px(11.))
                        .child(format!("Image · {name}"))
                        .child(self.b(
                            format!("remove-image-{n}"),
                            format!("Remove {name}"),
                            Action::RemoveImage(n),
                            t,
                            cx,
                        )),
                );
            }
        }
        body.child(
            div()
                .max_h(px(160.))
                .min_h(px(48.))
                .child(self.input.clone()),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.))
                .child(self.b("capture", "Capture screen", Action::Capture, t, cx))
                .child(self.b("choose-image", "Attach image", Action::ChooseImages, t, cx))
                .child(self.b(
                    "retry-images",
                    "Retry image previews",
                    Action::RetryImages,
                    t,
                    cx,
                ))
                .child(self.b(
                    "clipboard-image",
                    "Paste image",
                    Action::ClipboardImage,
                    t,
                    cx,
                ))
                .child(self.b(
                    "cwd",
                    s.cwd.unwrap_or_else(|| "Choose folder".into()),
                    Action::DirectoryMenu,
                    t,
                    cx,
                ))
                .child(self.b(
                    "models",
                    s.model.unwrap_or_else(|| "Choose model".into()),
                    Action::Models,
                    t,
                    cx,
                ))
                .when(unsaved, |d| {
                    d.child(self.b("save-response", "Retry saving", Action::Save, t, cx))
                })
                .child(self.b(
                    "send",
                    if busy { "Stop" } else { "Send" },
                    if busy { Action::Stop } else { Action::Send },
                    t,
                    cx,
                )),
        )
        .child(
            div()
                .flex()
                .gap(px(6.))
                .text_size(px(10.))
                .child("Permission mode")
                .children(
                    [
                        (ApprovalMode::Review, "Request Review"),
                        (ApprovalMode::Default, "Default"),
                        (ApprovalMode::Auto, "Auto (Preview)"),
                        (ApprovalMode::Turbo, "Turbo"),
                    ]
                    .into_iter()
                    .map(|(m, label)| {
                        self.b(
                            format!("mode-{label}"),
                            label,
                            Action::Mode(m.clone()),
                            t,
                            cx,
                        )
                        .aria_selected(s.approval_mode.as_ref() == Some(&m))
                    }),
                ),
        )
        .into_any_element()
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
        if !self.controller.visible || self.disposed || e.is_held {
            return;
        }
        let composing = std::iter::once(&self.input)
            .chain(std::iter::once(&self.filter))
            .chain(std::iter::once(&self.model_filter))
            .chain(self.arguments.values())
            .any(|input| {
                input.focus_handle(cx).is_focused(w) && input.read(cx).marked_range.is_some()
            });
        if composing && matches!(e.keystroke.key.as_str(), "enter" | "escape") {
            return;
        }
        let key = e.keystroke.key.as_str();
        let modifiers = e.keystroke.modifiers;
        if self.delete_pending.is_some() {
            if key == "escape" {
                cx.stop_propagation();
                self.action(Action::CancelDelete, w, cx);
                return;
            }
            if key == "tab" {
                cx.stop_propagation();
                controls::FocusScope::cycle(
                    &[self.cancel_focus.clone(), self.confirm_focus.clone()],
                    modifiers.shift,
                    w,
                    cx,
                );
                return;
            }
            if self.input.focus_handle(cx).is_focused(w) {
                cx.stop_propagation();
                return;
            }
        }
        if key == "escape"
            && self.input.read(cx).marked_range.is_some()
            && self.input.focus_handle(cx).is_focused(w)
        {
            return;
        }
        if self.delete_pending.is_some() {
            return;
        }
        if key == "escape" && (self.models_open || self.directories_open || self.prompt.is_some()) {
            self.models_open = false;
            self.directories_open = false;
            self.prompt = None;
            self.arguments.clear();
            self.prompt_generation += 1;
            self.input.focus_handle(cx).focus(w, cx);
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if key == "enter" && !modifiers.shift {
            if let Some(prompt) = &self.prompt {
                let names = prompt["arguments"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|a| a["name"].as_str())
                    .collect::<Vec<_>>();
                if let Some(index) = names.iter().position(|n| {
                    self.arguments.get(*n).is_some_and(|f| {
                        f.focus_handle(cx).is_focused(w) && f.read(cx).marked_range.is_none()
                    })
                }) {
                    cx.stop_propagation();
                    if let Some(next) = names.get(index + 1).and_then(|n| self.arguments.get(*n)) {
                        next.focus_handle(cx).focus(w, cx);
                    } else {
                        self.resolve_prompt(cx);
                    }
                    return;
                }
            }
        }
        if key == "tab"
            && self.active().is_some_and(|c| c.approval.is_some())
            && [
                self.allow_focus.clone(),
                self.deny_focus.clone(),
                self.always_focus.clone(),
            ]
            .iter()
            .any(|f| f.is_focused(w))
        {
            let mut handles = vec![self.allow_focus.clone(), self.deny_focus.clone()];
            if self
                .active()
                .and_then(|c| c.approval.as_ref())
                .is_some_and(|a| a.can_remember != Some(false))
            {
                handles.push(self.always_focus.clone());
            }
            handles.push(self.input.focus_handle(cx));
            controls::FocusScope::cycle(&handles, modifiers.shift, w, cx);
            cx.stop_propagation();
            return;
        }
        if key == "escape" && self.slash_items(cx).is_empty() {
            cx.stop_propagation();
            self.action(Action::Back, w, cx);
            return;
        }
        if (modifiers.platform || modifiers.control) && !composing {
            let action = match key {
                "n" => Some(Action::New),
                "." => Some(Action::Stop),
                "," => Some(Action::Settings),
                _ => None,
            };
            if let Some(action) = action {
                cx.stop_propagation();
                self.action(action, w, cx);
                return;
            }
        }
        if key == "s" && modifiers.shift && (modifiers.platform || modifiers.control) {
            cx.stop_propagation();
            self.action(Action::Capture, w, cx);
            return;
        }
        if !self.input.focus_handle(cx).is_focused(w) {
            return;
        }
        if self.input.read(cx).marked_range.is_some() {
            return;
        }
        if key == "enter" && modifiers.shift {
            cx.stop_propagation();
            self.input.update(cx, |input, cx| {
                input.replace_text_in_range(None, "\n", w, cx)
            });
            return;
        }
        let items = self.slash_items(cx);
        if !items.is_empty() {
            match key {
                "up" | "down" => {
                    self.slash_index = (self.slash_index
                        + if key == "down" { 1 } else { items.len() - 1 })
                        % items.len();
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                "enter" if !modifiers.shift => {
                    cx.stop_propagation();
                    self.action(
                        Action::ChoosePrompt(items[self.slash_index.min(items.len() - 1)]),
                        w,
                        cx,
                    );
                    return;
                }
                "escape" => {
                    self.slash_dismissed = true;
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }
        if key == "enter" && !modifiers.shift {
            cx.stop_propagation();
            self.action(Action::Send, w, cx);
        }
    }
}
impl Render for ChatView {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::tokens(w);
        let mut root = controls::sheet(t)
            .id("native-ai-chat")
            .track_focus(&self.focus)
            // GPUI resolves TextInput actions before low-level key capture. Intercept only
            // the shared actions whose focused chat meaning differs from ordinary editing.
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
            .child(
                controls::header(t)
                    .gap(px(8.))
                    .child(self.b("back", "Back", Action::Back, t, cx))
                    .child(
                        div().flex_1().text_size(px(13.)).child(
                            self.active()
                                .map(|c| {
                                    self.metadata
                                        .get(&c.id)
                                        .map(|m| m.0.clone())
                                        .unwrap_or_else(|| c.title.clone())
                                })
                                .unwrap_or_else(|| "Past chats".into()),
                        ),
                    )
                    .child(self.b("history", "Past chats", Action::Panel, t, cx))
                    .child(self.b("new", "New chat", Action::New, t, cx))
                    .child(self.b("settings", "AI Settings", Action::Settings, t, cx)),
            );
        if let Some(error) = &self.error {
            root = root.child(
                div()
                    .id("chat-error")
                    .role(Role::Alert)
                    .px(px(16.))
                    .py(px(6.))
                    .text_size(px(12.))
                    .text_color(t.danger)
                    .child(error.clone())
                    .child(self.b("dismiss-error", "Dismiss", Action::DismissError, t, cx)),
            );
        }
        if self.clear_inflight {
            root = root.child(
                div()
                    .id("chat-clearing")
                    .role(Role::Status)
                    .child("Clearing conversations…"),
            );
        }
        if self.delete_pending.is_some() {
            root = root.child(
                div()
                    .id("chat-delete-confirmation")
                    .role(Role::AlertDialog)
                    .aria_label("Delete conversations?")
                    .p(px(16.))
                    .child(if self.clear_pending {
                        "Delete all saved conversations?"
                    } else {
                        "Delete this conversation?"
                    })
                    .child(
                        self.b("confirm-delete", "Delete", Action::ConfirmDelete, t, cx)
                            .track_focus(&self.confirm_focus),
                    )
                    .child(
                        self.b("cancel-delete", "Cancel", Action::CancelDelete, t, cx)
                            .track_focus(&self.cancel_focus),
                    ),
            );
        }
        if self.controller.active.is_none() {
            root = root.child(self.history(t, cx));
        } else {
            root = root
                .child(
                    list(
                        self.transcript.clone(),
                        cx.processor(move |v, index, _, cx| v.row(index, t, cx)),
                    )
                    .flex_1()
                    .min_h_0(),
                )
                .child(self.diagnostics_view(t, cx))
                .child(self.approval(t, cx))
                .child(self.composer(t, cx));
        }
        if self.models_open {
            let options = tezbar_ui_model::settings::providers::model_options(
                &self.config,
                self.model_filter.read(cx).content.as_ref(),
            );
            let count = options.len();
            root = root.child(
                div()
                    .id("chat-models")
                    .role(Role::ListBox)
                    .aria_label("Choose model")
                    .max_h(px(240.))
                    .child(self.model_filter.clone())
                    .child(
                        uniform_list(
                            "chat-model-options",
                            count,
                            cx.processor(move |v, range: std::ops::Range<usize>, _, cx| {
                                range
                                    .map(|n| {
                                        let m = &options[n];
                                        v.b(
                                            format!("model-{n}"),
                                            format!(
                                                "{} · {}",
                                                runtime::provider_title(&v.config, &m.provider),
                                                m.title
                                            ),
                                            Action::Model(m.provider.clone(), m.model.clone()),
                                            t,
                                            cx,
                                        )
                                        .h(px(30.))
                                        .into_any_element()
                                    })
                                    .collect()
                            }),
                        )
                        .h(px(180.)),
                    )
                    .child(self.b("manage-models", "Manage providers", Action::Settings, t, cx)),
            );
        }
        let statuses = self.controller.background();
        let mut footer = controls::footer(t).child("AI");
        // In parent composition the shared footer owns these statuses and routing.
        for status in statuses
            .into_iter()
            .take(if self.footer.is_some() { 0 } else { 8 })
        {
            footer = footer.child(self.b(
                format!("background-{}", status.session_id),
                format!(
                        "{} · {}",
                        self.metadata
                            .get(&status.session_id)
                            .map(|m| m.0.as_str())
                            .unwrap_or("Chat"),
                        if status.waiting_approval {
                            "Approval needed"
                        } else {
                            "Running"
                        }
                    ),
                Action::Resume(status.session_id),
                t,
                cx,
            ));
        }
        if let Some(indicator) = self.footer.clone() {
            footer = footer.child(indicator);
        }
        root.child(footer)
    }
}
