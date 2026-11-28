use super::elements::{self, Icon};
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
        // Live turns follow the conversation timeline; settled turns keep their own
        // copy so tools, reasoning and text stay in the order they happened.
        let order = if streaming { timeline } else { &t.timeline[..] };
        if !order.is_empty() {
            for item in order {
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
                                settled: !streaming,
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
    /// shadcn `Button variant="ghost" size="icon"`: the label is the accessible name only.
    fn icon_button(
        &self,
        id: impl Into<String>,
        glyph: Icon,
        label: &'static str,
        action: Action,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.b(id, "", action, t, cx)
            .aria_label(label)
            .p_0()
            .size(px(28. * t.zoom))
            .flex_shrink_0()
            .rounded(px(7. * t.zoom))
            .bg(transparent_black())
            .hover(|d| d.bg(t.hover))
            .cursor_pointer()
            .child(elements::icon(glyph, 15. * t.zoom, t.text3))
    }
    /// shadcn `Button variant="ghost" size="sm"`.
    fn ghost(
        &self,
        id: impl Into<String>,
        label: impl Into<SharedString>,
        action: Action,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.b(id, label, action, t, cx)
            .h(px(28. * t.zoom))
            .px(px(9. * t.zoom))
            .py_0()
            .gap(px(6. * t.zoom))
            .rounded(px(7. * t.zoom))
            .bg(transparent_black())
            .hover(|d| d.bg(t.hover))
            .text_color(t.text2)
            .cursor_pointer()
    }
    /// shadcn `Button variant="outline" size="sm"`.
    fn outline(
        &self,
        id: impl Into<String>,
        label: impl Into<SharedString>,
        action: Action,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.b(id, label, action, t, cx)
            .h(px(30. * t.zoom))
            .px(px(12. * t.zoom))
            .py_0()
            .rounded(px(8. * t.zoom))
            .border_color(t.rim)
            .bg(elements::surface(t))
            .hover(|d| d.bg(t.hover))
            .cursor_pointer()
    }
    /// shadcn `Button` (default/primary): neutral fill, inverse text.
    fn primary(
        &self,
        id: impl Into<String>,
        label: impl Into<SharedString>,
        action: Action,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.b(id, label, action, t, cx)
            .h(px(30. * t.zoom))
            .px(px(12. * t.zoom))
            .py_0()
            .rounded(px(8. * t.zoom))
            .bg(t.text1)
            .text_color(elements::on_primary(t))
            .hover(|d| d.opacity(0.9))
            .cursor_pointer()
    }
    /// A content-sized floating menu anchored above the composer.
    fn menu(id: &'static str, label: &'static str, t: Tokens) -> Stateful<Div> {
        controls::popover(id, label, t)
            .occlude()
            .absolute()
            .bottom(gpui::relative(1.))
            .mb(px(6. * t.zoom))
            .rounded(px(12. * t.zoom))
            .border_color(t.hairline)
            .bg(t.popover_solid)
            .shadow_lg()
            .flex()
            .flex_col()
            .gap(px(1. * t.zoom))
    }
    /// A full-width menu row: leading glyph, title, optional trailing hint.
    fn menu_item(
        &self,
        id: impl Into<String>,
        label: impl Into<SharedString>,
        action: Action,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.b(id, "", action, t, cx)
            .aria_label(label)
            .w_full()
            .h_auto()
            .min_h(px(30. * t.zoom))
            .px(px(8. * t.zoom))
            .py(px(5. * t.zoom))
            .justify_start()
            .gap(px(8. * t.zoom))
            .rounded(px(7. * t.zoom))
            .line_height(gpui::relative(1.35))
            .font_weight(FontWeight::NORMAL)
            .bg(transparent_black())
            .hover(|d| d.bg(t.hover))
            .cursor_pointer()
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
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8. * t.zoom))
                    .h(px(40. * t.zoom))
                    .px(px(12. * t.zoom))
                    .border_b_1()
                    .border_color(t.hairline)
                    .child(elements::icon(Icon::Search, 14. * t.zoom, t.text3))
                    .child(div().flex_1().min_w_0().child(self.filter.clone())),
            )
            .when(count == 0, |d| {
                d.child(
                    div()
                        .id("history-status")
                        .role(Role::Status)
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(8. * t.zoom))
                        .p(px(24. * t.zoom))
                        .text_size(px(12. * t.zoom))
                        .text_color(t.text3)
                        .child(elements::icon(Icon::History, 18. * t.zoom, t.text4))
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
                                    .h(px(52. * t.zoom))
                                    .px(px(6. * t.zoom))
                                    .gap(px(2. * t.zoom))
                                    .child(
                                        this.b(
                                            format!("resume-{}", row.id),
                                            "",
                                            Action::Resume(row.id.clone()),
                                            t,
                                            cx,
                                        )
                                        .aria_label(row.title.clone())
                                        .flex_1()
                                        .min_w_0()
                                        .h(px(46. * t.zoom))
                                        .px(px(8. * t.zoom))
                                        .flex()
                                        .flex_col()
                                        .items_start()
                                        .justify_center()
                                        .gap(px(3. * t.zoom))
                                        .rounded(px(8. * t.zoom))
                                        .bg(transparent_black())
                                        .hover(|d| d.bg(t.hover))
                                        .cursor_pointer()
                                        .child(
                                            div()
                                                .w_full()
                                                .truncate()
                                                .text_size(px(12.5 * t.zoom))
                                                .line_height(px(16. * t.zoom))
                                                .text_color(t.text1)
                                                .child(row.title.clone()),
                                        )
                                        .child(
                                            div()
                                                .w_full()
                                                .truncate()
                                                .text_size(px(11. * t.zoom))
                                                .line_height(px(14. * t.zoom))
                                                .font_weight(FontWeight::NORMAL)
                                                .text_color(t.text3)
                                                .child(format!(
                                                    "{} · {}",
                                                    relative(row.updated_at),
                                                    row.preview
                                                )),
                                        ),
                                    )
                                    .child(this.icon_button(
                                        format!("delete-{}", row.id),
                                        Icon::Trash,
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
                .min_h_0()
                .py(px(4. * t.zoom)),
            )
            .into_any_element()
    }
    fn row(&mut self, index: usize, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.projection.get(index).cloned() else {
            return div().into_any_element();
        };
        let gutter = px(20. * t.zoom);
        match row {
            Row::Header(turn) => {
                let session = self.controller.active.clone().unwrap_or_default();
                let user = turn.role == chat::Role::User;
                let mut row = div()
                    .flex()
                    .flex_col()
                    .gap(px(6. * t.zoom))
                    .px(gutter)
                    .pt(px((if user { 16. } else { 6. }) * t.zoom))
                    .when(user, |d| d.items_end());
                for (index, r) in turn.references.iter().enumerate() {
                    if let Reference::Image { name, .. } = r {
                        let key = references::thumbnail_key(&session, &turn.id, index, r);
                        row = row.child(if let Some(image) = self.images.get(&key) {
                            div()
                                .rounded(px(10. * t.zoom))
                                .border_1()
                                .border_color(t.hairline)
                                .overflow_hidden()
                                .child(
                                    img(image)
                                        .max_h(px(140. * t.zoom))
                                        .max_w(px(220. * t.zoom))
                                        .id(format!("history-image-{}-{index}", turn.id))
                                        .aria_label(name.clone()),
                                )
                        } else {
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6. * t.zoom))
                                .h(px(28. * t.zoom))
                                .px(px(10. * t.zoom))
                                .rounded(px(8. * t.zoom))
                                .border_1()
                                .border_color(t.hairline)
                                .text_size(px(11.5 * t.zoom))
                                .text_color(t.text3)
                                .child(elements::icon(Icon::Image, 13. * t.zoom, t.text3))
                                .child(format!("{name} · preview unavailable or loading"))
                        });
                    }
                }
                row.into_any_element()
            }
            // AI Elements `Message`: user turns are right-aligned secondary bubbles,
            // assistant turns are unframed full-width prose.
            Row::Text { id, turn, .. } => {
                let user = turn.role == chat::Role::User;
                let selector = format!("chat-message-{id}");
                div()
                    .w_full()
                    .flex()
                    .px(gutter)
                    .py(px(4. * t.zoom))
                    .when(user, |d| d.justify_end())
                    .child(
                        div()
                            .debug_selector(move || selector.clone())
                            .when(user, |d| {
                                d.max_w(gpui::relative(0.82))
                                    .min_w_0()
                                    .rounded(px(14. * t.zoom))
                                    .bg(t.pill_strong)
                                    .px(px(14. * t.zoom))
                                    .py(px(7. * t.zoom))
                            })
                            .when(!user, |d| d.w_full())
                            .children(self.documents.get(&id).map(|d| d.view.clone())),
                    )
                    .into_any_element()
            }
            Row::Stage(turn, n) => {
                let stage = &turn.stages[n];
                let key = format!("{}-{}", turn.id, stage.index);
                let expanded = self.expanded.contains(&key);
                let running = matches!(stage.status, chat::StageStatus::Running);
                let failed = matches!(stage.status, chat::StageStatus::Failed);
                let thinking = stage.tool_name.as_deref().is_none_or(|n| n == "thinking")
                    && stage.label.to_lowercase().starts_with("thinking");
                // AI Elements `Reasoning`: a quiet status line, not a card.
                if thinking && stage.detail.is_none() && stage.output.is_none() {
                    let label = match stage.status {
                        chat::StageStatus::Running => "Thinking…".to_string(),
                        chat::StageStatus::Done => stage
                            .duration_ms
                            .map(|ms| format!("Thought for {}", elements::duration(ms)))
                            .unwrap_or_else(|| "Thought".into()),
                        chat::StageStatus::Failed => "Thinking stopped".into(),
                    };
                    return div()
                        .id(format!("stage-{key}"))
                        .role(Role::Status)
                        .aria_label(label.clone())
                        .flex()
                        .items_center()
                        .gap(px(7. * t.zoom))
                        .px(gutter)
                        .py(px(4. * t.zoom))
                        .text_size(px(12.5 * t.zoom))
                        .text_color(if failed { t.danger } else { t.text3 })
                        .child(if running {
                            elements::spinner(format!("thinking-{key}"), 14. * t.zoom, t.text3)
                                .into_any_element()
                        } else {
                            elements::icon(Icon::Brain, 14. * t.zoom, t.text3).into_any_element()
                        })
                        .child(label)
                        .into_any_element();
                }
                // AI Elements `Tool`: bordered collapsible with a status badge.
                let title = if stage.label.is_empty() {
                    stage.tool_name.clone().unwrap_or_else(|| "Tool".into())
                } else {
                    stage.label.clone()
                };
                let status = match stage.status {
                    chat::StageStatus::Running => "Running",
                    chat::StageStatus::Done => "Completed",
                    chat::StageStatus::Failed => "Error",
                };
                let mut header = self
                    .b(
                        format!("stage-{key}"),
                        "",
                        Action::Expand(key.clone()),
                        t,
                        cx,
                    )
                    .aria_label(format!("{title} · {status}"))
                    .aria_expanded(expanded)
                    .w_full()
                    .h_auto()
                    .justify_start()
                    .gap(px(8. * t.zoom))
                    .px(px(12. * t.zoom))
                    .py(px(9. * t.zoom))
                    .rounded(px(0.))
                    .line_height(px(18. * t.zoom))
                    .bg(transparent_black())
                    .hover(|d| d.bg(t.hover))
                    .cursor_pointer()
                    .child(elements::icon(Icon::Wrench, 14. * t.zoom, t.text3))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.5 * t.zoom))
                            .text_color(t.text1)
                            .child(title),
                    );
                header = header.child(
                    elements::badge(t)
                        .when(running, |d| {
                            d.child(elements::spinner(
                                format!("tool-spin-{key}"),
                                11. * t.zoom,
                                t.text2,
                            ))
                        })
                        .when(!running, |d| {
                            d.child(elements::icon(
                                if failed {
                                    Icon::CircleX
                                } else {
                                    Icon::CircleCheck
                                },
                                11. * t.zoom,
                                if failed { t.danger } else { t.success },
                            ))
                        })
                        .child(status),
                );
                if let Some(ms) = stage.duration_ms {
                    header = header.child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11. * t.zoom))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(t.text4)
                            .child(elements::duration(ms)),
                    );
                }
                header = header.child(elements::icon(
                    if expanded {
                        Icon::ChevronDown
                    } else {
                        Icon::ChevronRight
                    },
                    14. * t.zoom,
                    t.text3,
                ));
                div()
                    .px(gutter)
                    .py(px(4. * t.zoom))
                    .child(
                        div()
                            .w_full()
                            .rounded(px(10. * t.zoom))
                            .border_1()
                            .border_color(t.hairline)
                            .bg(elements::surface(t))
                            .overflow_hidden()
                            .child(header)
                            .when(expanded, |d| {
                                d.child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(10. * t.zoom))
                                        .p(px(12. * t.zoom))
                                        .border_t_1()
                                        .border_color(t.hairline)
                                        .when_some(stage.detail.clone(), |d, detail| {
                                            d.child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(6. * t.zoom))
                                                    .child(elements::caption("Parameters", t))
                                                    .child(elements::code_block(
                                                        format!("detail-{key}"),
                                                        detail,
                                                        t,
                                                    )),
                                            )
                                        })
                                        .when_some(stage.output.clone(), |d, output| {
                                            d.child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(6. * t.zoom))
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .h(px(22. * t.zoom))
                                                            .child(
                                                                elements::caption(
                                                                    if failed {
                                                                        "Error"
                                                                    } else {
                                                                        "Result"
                                                                    },
                                                                    t,
                                                                )
                                                                .flex_1(),
                                                            )
                                                            .child(
                                                                self.icon_button(
                                                                    format!("copy-{key}"),
                                                                    Icon::Copy,
                                                                    "Copy output",
                                                                    Action::Copy(output.clone()),
                                                                    t,
                                                                    cx,
                                                                )
                                                                .size(px(22. * t.zoom)),
                                                            ),
                                                    )
                                                    .child(
                                                        elements::code_block(
                                                            format!("output-{key}"),
                                                            output.chars().take(32768).collect(),
                                                            t,
                                                        )
                                                        .when(failed, |d| d.text_color(t.danger)),
                                                    )
                                                    .when(stage.output_truncated, |d| {
                                                        d.child(
                                                            div()
                                                                .text_size(px(11. * t.zoom))
                                                                .text_color(t.text4)
                                                                .child("Output truncated"),
                                                        )
                                                    }),
                                            )
                                        })
                                        .when(
                                            stage.detail.is_none() && stage.output.is_none(),
                                            |d| {
                                                d.child(
                                                    div()
                                                        .text_size(px(11.5 * t.zoom))
                                                        .text_color(t.text3)
                                                        .child(if running {
                                                            "Waiting for output…"
                                                        } else {
                                                            "No output"
                                                        }),
                                                )
                                            },
                                        ),
                                )
                            }),
                    )
                    .into_any_element()
            }
            // AI Elements `Actions` + `Loader`: icon-only ghost actions under the message.
            Row::End(turn, live) => {
                let user = turn.role == chat::Role::User;
                let retry = self.active().is_some_and(|c| {
                    !c.busy() && c.context.back().is_some_and(|t| t.id == turn.id)
                }) && turn.role == chat::Role::Assistant
                    && turn.error.is_some();
                let actions = !live || retry;
                let delete_selector = format!("chat-delete-{}", turn.id);
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(6. * t.zoom))
                    .px(if user { gutter } else { px(16. * t.zoom) })
                    .pt(px(2. * t.zoom))
                    .pb(px((if user { 4. } else { 14. }) * t.zoom))
                    .when(user, |d| d.items_end())
                    .when(live, |d| {
                        d.child(
                            div()
                                .id(format!("working-{}", turn.id))
                                .role(Role::Status)
                                .flex()
                                .items_center()
                                .gap(px(7. * t.zoom))
                                .px(px(4. * t.zoom))
                                .text_size(px(12.5 * t.zoom))
                                .text_color(t.text3)
                                .child(elements::spinner(
                                    format!("working-spin-{}", turn.id),
                                    14. * t.zoom,
                                    t.text3,
                                ))
                                .child("Working…"),
                        )
                    })
                    .when_some(turn.error.clone(), |d, e| {
                        d.child(
                            div().px(px(4. * t.zoom)).child(elements::alert(
                                format!("turn-error-{}", turn.id),
                                chat::format_error(
                                    &e,
                                    turn.response_meta
                                        .as_ref()
                                        .map(|m| m.provider.as_str())
                                        .unwrap_or(""),
                                ),
                                t,
                            )),
                        )
                    })
                    .when(actions, |d| {
                        d.child(
                            div()
                                .w_full()
                                .flex()
                                .when(user, |d| d.justify_end())
                                .items_center()
                                .gap(px(2. * t.zoom))
                                .when(retry, |d| {
                                    d.child(self.icon_button(
                                        format!("retry-{}", turn.id),
                                        Icon::Retry,
                                        "Try again",
                                        Action::Retry(turn.id.clone()),
                                        t,
                                        cx,
                                    ))
                                })
                                .when(!live && !turn.text.is_empty(), |d| {
                                    d.child(self.icon_button(
                                        format!("copy-{}", turn.id),
                                        Icon::Copy,
                                        "Copy",
                                        Action::Copy(turn.text.clone()),
                                        t,
                                        cx,
                                    ))
                                })
                                .when(!live, |d| {
                                    d.child(
                                        self.icon_button(
                                            format!("remove-{}", turn.id),
                                            Icon::Trash,
                                            "Delete message",
                                            Action::DeleteTurn(turn.id.clone()),
                                            t,
                                            cx,
                                        )
                                        .debug_selector(move || delete_selector.clone()),
                                    )
                                })
                                .when_some(turn.response_meta.clone(), |d, m| {
                                    d.child(
                                        div()
                                            .ml(px(6. * t.zoom))
                                            .min_w_0()
                                            .truncate()
                                            .text_size(px(11. * t.zoom))
                                            .text_color(t.text4)
                                            .child(format!(
                                                "{} · {}{}",
                                                m.provider_title,
                                                m.model,
                                                m.token_count
                                                    .map(|n| format!(" · {n} tokens"))
                                                    .unwrap_or_default()
                                            )),
                                    )
                                }),
                        )
                    })
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
        let expanded = self.expanded.contains(&key);
        div()
            .when_some(lines, |d, lines| {
                d.px(px(12. * t.zoom))
                    .pb(px(4. * t.zoom))
                    .child(
                        self.ghost("diagnostics", "", Action::Expand(key.clone()), t, cx)
                            .aria_label(format!("Diagnostics ({})", lines.len()))
                            .aria_expanded(expanded)
                            .text_size(px(11. * t.zoom))
                            .text_color(t.text3)
                            .child(elements::icon(Icon::Terminal, 13. * t.zoom, t.text3))
                            .child(format!("Diagnostics ({})", lines.len()))
                            .child(elements::icon(
                                if expanded {
                                    Icon::ChevronDown
                                } else {
                                    Icon::ChevronRight
                                },
                                12. * t.zoom,
                                t.text3,
                            )),
                    )
                    .when(expanded, |d| {
                        d.child(
                            elements::code_block("diagnostics-text", lines.join("\n"), t)
                                .max_h(px(100. * t.zoom))
                                .text_size(px(11. * t.zoom))
                                .text_color(t.text3),
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
    /// AI Elements `Confirmation`: the tool request, its exact command, and the decision.
    pub(super) fn approval(&self, t: Tokens, cx: &mut Context<Self>) -> Stateful<Div> {
        let Some(approval) = self.active().and_then(|c| c.approval.clone()) else {
            return div().id("no-tool-approval");
        };
        div()
            .id("tool-approval")
            .role(Role::AlertDialog)
            .aria_label(approval.title.clone())
            .mx(px(12. * t.zoom))
            .mb(px(8. * t.zoom))
            .p(px(12. * t.zoom))
            .rounded(px(12. * t.zoom))
            .border_1()
            .border_color(t.warning.opacity(0.4))
            .bg(t.popover_solid)
            .shadow_md()
            .flex()
            .flex_col()
            .gap(px(10. * t.zoom))
            .text_size(px(12.5 * t.zoom))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8. * t.zoom))
                    .child(elements::icon(Icon::ShieldAlert, 15. * t.zoom, t.warning))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.text1)
                            .child(approval.title),
                    ),
            )
            .child(
                elements::code_block(
                    "approval-command",
                    self.approval_body().unwrap_or_default(),
                    t,
                )
                .max_h(px(112. * t.zoom)),
            )
            .child(
                div()
                    .text_size(px(11. * t.zoom))
                    .text_color(t.text3)
                    .child("Only approve commands you expect this task to run."),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap(px(6. * t.zoom))
                    .when(approval.can_remember != Some(false), |d| {
                        d.child(
                            self.ghost(
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
                            .h(px(30. * t.zoom))
                            .track_focus(&self.always_focus),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        self.outline("deny", "Deny", Action::Decide(Decision::Deny), t, cx)
                            .track_focus(&self.deny_focus),
                    )
                    .child(
                        self.primary(
                            "allow-once",
                            "Allow once",
                            Action::Decide(Decision::Once),
                            t,
                            cx,
                        )
                        .track_focus(&self.allow_focus),
                    ),
            )
    }
    /// AI Elements `PromptInput`: one bordered surface with the textarea on top and a
    /// tool row (attachments, folder, model, permission mode, submit) underneath.
    fn composer(&mut self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let s = self.selection();
        let busy = self.active().is_some_and(|c| c.busy());
        let attachments = self
            .active()
            .map(|c| c.attachments.clone())
            .unwrap_or_default();
        let unsaved = self.active().is_some_and(|c| c.unsaved());
        let draft = !self.input.read(cx).content.trim().is_empty();
        let mut body = div()
            .debug_selector(|| "chat-composer".into())
            .relative()
            .mx(px(12. * t.zoom))
            .mb(px(10. * t.zoom))
            .rounded(px(16. * t.zoom))
            .border_1()
            .border_color(t.rim)
            .bg(elements::surface(t))
            .shadow_sm()
            .flex()
            .flex_col();

        // Floating menus share the composer as their anchor.
        let slash = self.slash_items(cx);
        if !slash.is_empty() {
            let mut commands = Self::menu("ai-commands", "AI commands", t)
                .role(Role::ListBox)
                .left(px(0.))
                .right(px(0.))
                .max_h(px(220. * t.zoom))
                .overflow_y_scroll();
            for (n, i) in slash.iter().enumerate() {
                let item = &self.items[*i];
                let name = item["name"].as_str().unwrap_or("");
                let title = item["title"].as_str().unwrap_or("");
                let extension = item["extensionName"].as_str().unwrap_or("");
                let selected = n == self.slash_index;
                commands = commands.child(
                    self.menu_item(
                        format!("command-{i}"),
                        format!("/{name}  {title} · {extension}"),
                        Action::ChoosePrompt(*i),
                        t,
                        cx,
                    )
                    .role(Role::ListBoxOption)
                    .aria_selected(selected)
                    .when(selected, |d| d.aria_active_descendant().bg(t.selected))
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_family(tezbar_ui::tokens::mono_family())
                            .text_color(t.text1)
                            .child(format!("/{name}")),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(t.text2)
                            .child(title.to_string()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11. * t.zoom))
                            .text_color(t.text4)
                            .child(extension.to_string()),
                    ),
                );
            }
            body = body.child(commands);
        }
        if self.directories_open {
            let mut menu = Self::menu("chat-directories", "Choose working folder", t)
                .left(px(0.))
                .w(px(320. * t.zoom))
                .max_w_full();
            for (n, p) in self.recent_directories.iter().enumerate() {
                menu = menu.child(
                    self.menu_item(
                        format!("recent-cwd-{n}"),
                        p.clone(),
                        Action::SetDirectory(p.clone()),
                        t,
                        cx,
                    )
                    .child(elements::icon(Icon::Folder, 14. * t.zoom, t.text3))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(t.text1)
                            .child(tezbar_ui_model::terminal::compact_path(p)),
                    ),
                );
            }
            menu = menu.child(
                self.menu_item(
                    "choose-directory",
                    "Choose another folder…",
                    Action::Directory,
                    t,
                    cx,
                )
                .child(elements::icon(Icon::Search, 14. * t.zoom, t.text3))
                .child(div().text_color(t.text2).child("Choose another folder…")),
            );
            body = body.child(menu);
        }
        let modes = [
            (
                ApprovalMode::Review,
                "Request Review",
                "Ask before every tool and command.",
            ),
            (
                ApprovalMode::Default,
                "Default",
                "Read in the working folder; ask before writes and other commands.",
            ),
            (
                ApprovalMode::Auto,
                "Auto (Preview)",
                "Read outside the folder and change files inside it.",
            ),
            (
                ApprovalMode::Turbo,
                "Turbo",
                "Change files outside the folder and run other commands.",
            ),
        ];
        let selected_mode = s.approval_mode.as_ref().unwrap_or(&ApprovalMode::Default);
        let mode_title = modes
            .iter()
            .find(|(mode, _, _)| mode == selected_mode)
            .map(|(_, title, _)| *title)
            .unwrap_or("Default");
        if self.modes_open {
            body = body.child(
                Self::menu("chat-permission-modes", "Choose permission mode", t)
                    .right(px(0.))
                    .w(px(340. * t.zoom))
                    .max_w_full()
                    .children(modes.iter().map(|(mode, title, subtitle)| {
                        let selected = mode == selected_mode;
                        self.menu_item(
                            format!("mode-{title}"),
                            *title,
                            Action::Mode(mode.clone()),
                            t,
                            cx,
                        )
                        .role(Role::ListBoxOption)
                        .aria_selected(selected)
                        .items_start()
                        .py(px(7. * t.zoom))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(2. * t.zoom))
                                .child(div().text_color(t.text1).child(*title))
                                .child(
                                    div()
                                        .text_size(px(11. * t.zoom))
                                        .text_color(t.text3)
                                        .child(*subtitle),
                                ),
                        )
                        .child(
                            div()
                                .size(px(14. * t.zoom))
                                .flex_shrink_0()
                                .when(selected, |d| {
                                    d.child(elements::icon(
                                        Icon::CircleCheck,
                                        14. * t.zoom,
                                        t.text1,
                                    ))
                                }),
                        )
                    })),
            );
        }
        if self.models_open {
            let options = tezbar_ui_model::settings::providers::model_options(
                &self.config,
                self.model_filter.read(cx).content.as_ref(),
            );
            let current = s.model.clone();
            let count = options.len();
            body = body.child(
                Self::menu("chat-models", "Choose model", t)
                    .role(Role::ListBox)
                    .right(px(0.))
                    .w(px(340. * t.zoom))
                    .max_w_full()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8. * t.zoom))
                            .h(px(34. * t.zoom))
                            .px(px(8. * t.zoom))
                            .mb(px(2. * t.zoom))
                            .border_b_1()
                            .border_color(t.hairline)
                            .child(elements::icon(Icon::Search, 14. * t.zoom, t.text3))
                            .child(div().flex_1().min_w_0().child(self.model_filter.clone())),
                    )
                    .child(
                        uniform_list(
                            "chat-model-options",
                            count,
                            cx.processor(move |v, range: std::ops::Range<usize>, _, cx| {
                                range
                                    .map(|n| {
                                        let m = &options[n];
                                        let provider =
                                            runtime::provider_title(&v.config, &m.provider);
                                        let selected = current.as_deref() == Some(m.model.as_str());
                                        v.menu_item(
                                            format!("model-{n}"),
                                            format!("{provider} · {}", m.title),
                                            Action::Model(m.provider.clone(), m.model.clone()),
                                            t,
                                            cx,
                                        )
                                        .h(px(30. * t.zoom))
                                        .min_h(px(30. * t.zoom))
                                        .py_0()
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .truncate()
                                                .text_color(t.text1)
                                                .child(m.title.clone()),
                                        )
                                        .child(
                                            div()
                                                .flex_shrink_0()
                                                .text_size(px(11. * t.zoom))
                                                .text_color(t.text4)
                                                .child(provider),
                                        )
                                        .child(div().size(px(14. * t.zoom)).flex_shrink_0().when(
                                            selected,
                                            |d| {
                                                d.child(elements::icon(
                                                    Icon::CircleCheck,
                                                    14. * t.zoom,
                                                    t.text1,
                                                ))
                                            },
                                        ))
                                        .into_any_element()
                                    })
                                    .collect()
                            }),
                        )
                        .h(px(180. * t.zoom)),
                    )
                    .child(
                        div()
                            .pt(px(2. * t.zoom))
                            .border_t_1()
                            .border_color(t.hairline)
                            .child(
                                self.menu_item(
                                    "manage-models",
                                    "Manage providers",
                                    Action::Settings,
                                    t,
                                    cx,
                                )
                                .child(elements::icon(Icon::Sparkles, 14. * t.zoom, t.text3))
                                .child(div().text_color(t.text2).child("Manage providers")),
                            ),
                    ),
            );
        }

        // Notices and context chips sit above the textarea, inside the input surface.
        let mut chips = div()
            .flex()
            .flex_col()
            .gap(px(8. * t.zoom))
            .px(px(10. * t.zoom))
            .pt(px(10. * t.zoom));
        let mut has_chips = false;
        if let Some(error) = self.active().and_then(|c| c.input_error.clone()) {
            has_chips = true;
            chips = chips.child(elements::alert("chat-error", error, t));
        }
        if let Some(cwd) = &self.project {
            has_chips = true;
            chips = chips.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6. * t.zoom))
                    .px(px(10. * t.zoom))
                    .py(px(4. * t.zoom))
                    .rounded(px(8. * t.zoom))
                    .border_1()
                    .border_color(t.warning.opacity(0.35))
                    .bg(t.warning.opacity(0.08))
                    .text_size(px(11.5 * t.zoom))
                    .text_color(t.text2)
                    .child(elements::icon(Icon::ShieldAlert, 13. * t.zoom, t.warning))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child("This folder has AI extensions. Not running."),
                    )
                    .child(
                        self.ghost(
                            format!("review-{cwd}"),
                            "Review",
                            Action::ReviewProject,
                            t,
                            cx,
                        )
                        .h(px(24. * t.zoom))
                        .text_color(t.text1),
                    )
                    .child(
                        self.icon_button(
                            "dismiss-project",
                            Icon::Close,
                            "Dismiss",
                            Action::DismissProject,
                            t,
                            cx,
                        )
                        .size(px(24. * t.zoom)),
                    ),
            );
        }
        if let Some(prompt) = &self.prompt {
            has_chips = true;
            let mut form = div()
                .flex()
                .flex_col()
                .gap(px(8. * t.zoom))
                .p(px(10. * t.zoom))
                .rounded(px(10. * t.zoom))
                .border_1()
                .border_color(t.hairline)
                .bg(t.bar);
            for name in prompt["arguments"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|a| a["name"].as_str())
            {
                let Some(input) = self.arguments.get(name) else {
                    continue;
                };
                form = form.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4. * t.zoom))
                        .text_size(px(11. * t.zoom))
                        .child(div().text_color(t.text3).child(name.to_string()))
                        .child(input.clone()),
                );
            }
            chips = chips.child(
                form.child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(6. * t.zoom))
                        .child(self.ghost("cancel-prompt", "Cancel", Action::CancelPrompt, t, cx))
                        .child(self.primary(
                            "insert-prompt",
                            "Insert in draft",
                            Action::ResolvePrompt,
                            t,
                            cx,
                        )),
                ),
            );
        }
        let mut tags = div().flex().flex_wrap().items_center().gap(px(6. * t.zoom));
        let mut has_tags = false;
        if let Some(profile) = s.profile {
            has_tags = true;
            let title = self
                .items
                .iter()
                .find(|i| i["kind"] == "agents" && i["id"] == profile)
                .and_then(|i| i["title"].as_str().or_else(|| i["name"].as_str()))
                .map(str::to_string)
                .unwrap_or_else(|| format!("{profile} (unavailable)"));
            tags = tags.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4. * t.zoom))
                    .h(px(26. * t.zoom))
                    .pl(px(8. * t.zoom))
                    .pr(px(2. * t.zoom))
                    .rounded(px(7. * t.zoom))
                    .border_1()
                    .border_color(t.hairline)
                    .bg(t.pill)
                    .text_size(px(11.5 * t.zoom))
                    .text_color(t.text2)
                    .child(elements::icon(Icon::Sparkles, 12. * t.zoom, t.text3))
                    .child(format!("Profile · {title}"))
                    .child(
                        self.icon_button(
                            "clear-profile",
                            Icon::Close,
                            "Remove profile",
                            Action::ClearProfile,
                            t,
                            cx,
                        )
                        .size(px(20. * t.zoom)),
                    ),
            );
        }
        for (n, r) in attachments.iter().enumerate() {
            let Reference::Image { blob_id, name, .. } = r else {
                continue;
            };
            has_tags = true;
            let preview = self.images.get(blob_id).map(|image| {
                img(image)
                    .size(px(20. * t.zoom))
                    .rounded(px(4. * t.zoom))
                    .id(format!("image-preview-{n}"))
                    .aria_label(name.clone())
                    .into_any_element()
            });
            tags = tags.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6. * t.zoom))
                    .h(px(28. * t.zoom))
                    .max_w(px(220. * t.zoom))
                    .pl(px(4. * t.zoom))
                    .pr(px(2. * t.zoom))
                    .rounded(px(7. * t.zoom))
                    .border_1()
                    .border_color(t.hairline)
                    .bg(t.pill)
                    .text_size(px(11.5 * t.zoom))
                    .text_color(t.text2)
                    .child(preview.unwrap_or_else(|| {
                        div()
                            .size(px(20. * t.zoom))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(elements::icon(Icon::Image, 13. * t.zoom, t.text3))
                            .into_any_element()
                    }))
                    .child(div().min_w_0().truncate().child(name.clone()))
                    .child(
                        self.b(
                            format!("remove-image-{n}"),
                            "",
                            Action::RemoveImage(n),
                            t,
                            cx,
                        )
                        .aria_label(format!("Remove {name}"))
                        .p_0()
                        .size(px(20. * t.zoom))
                        .flex_shrink_0()
                        .rounded(px(5. * t.zoom))
                        .bg(transparent_black())
                        .hover(|d| d.bg(t.hover))
                        .cursor_pointer()
                        .child(elements::icon(
                            Icon::Close,
                            12. * t.zoom,
                            t.text3,
                        )),
                    ),
            );
        }
        if !attachments.is_empty() {
            has_tags = true;
            tags = tags.child(
                self.ghost("retry-images", "Retry previews", Action::RetryImages, t, cx)
                    .h(px(26. * t.zoom))
                    .text_size(px(11. * t.zoom))
                    .text_color(t.text3),
            );
        }
        if has_tags {
            has_chips = true;
            chips = chips.child(tags);
        }
        if has_chips {
            body = body.child(chips);
        }

        let can_send = busy || draft || !attachments.is_empty();
        body.child(
            div()
                .px(px(14. * t.zoom))
                .pt(px(12. * t.zoom))
                .pb(px(4. * t.zoom))
                .min_h(px(44. * t.zoom))
                .max_h(px(160. * t.zoom))
                .text_size(px(13.5 * t.zoom))
                .child(self.input.clone()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(2. * t.zoom))
                .px(px(6. * t.zoom))
                .pt(px(2. * t.zoom))
                .pb(px(6. * t.zoom))
                .child(self.icon_button(
                    "choose-image",
                    Icon::Paperclip,
                    "Attach image",
                    Action::ChooseImages,
                    t,
                    cx,
                ))
                .child(self.icon_button(
                    "clipboard-image",
                    Icon::Clipboard,
                    "Paste image",
                    Action::ClipboardImage,
                    t,
                    cx,
                ))
                .child(self.icon_button(
                    "capture",
                    Icon::Monitor,
                    "Attach active screen",
                    Action::Capture,
                    t,
                    cx,
                ))
                .child(
                    self.ghost("cwd", "", Action::DirectoryMenu, t, cx)
                        .aria_label(
                            s.cwd
                                .as_deref()
                                .map(tezbar_ui_model::terminal::compact_path)
                                .unwrap_or_else(|| "Choose folder".into()),
                        )
                        .aria_expanded(self.directories_open)
                        .min_w_0()
                        .px(px(7. * t.zoom))
                        .gap(px(5. * t.zoom))
                        .text_size(px(11.5 * t.zoom))
                        .text_color(t.text3)
                        .child(elements::icon(Icon::Folder, 13. * t.zoom, t.text3))
                        .child(
                            div().min_w_0().truncate().child(
                                s.cwd
                                    .as_deref()
                                    .map(tezbar_ui_model::terminal::compact_path)
                                    .unwrap_or_else(|| "Choose folder".into()),
                            ),
                        ),
                )
                .child(div().flex_1().min_w(px(4. * t.zoom)))
                .when(unsaved, |d| {
                    d.child(
                        self.ghost("save-response", "Retry saving", Action::Save, t, cx)
                            .text_size(px(11.5 * t.zoom))
                            .text_color(t.warning),
                    )
                })
                .child(
                    self.ghost("models", "", Action::Models, t, cx)
                        .aria_label(s.model.clone().unwrap_or_else(|| "Choose model".into()))
                        .aria_expanded(self.models_open)
                        .min_w_0()
                        .max_w(px(180. * t.zoom))
                        .px(px(7. * t.zoom))
                        .gap(px(4. * t.zoom))
                        .text_size(px(11.5 * t.zoom))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .child(s.model.clone().unwrap_or_else(|| "Choose model".into())),
                        )
                        .child(elements::icon(Icon::ChevronDown, 12. * t.zoom, t.text3)),
                )
                .child(
                    self.ghost("permission-mode", "", Action::Modes, t, cx)
                        .aria_label("Agent permission mode")
                        .aria_description(mode_title)
                        .aria_expanded(self.modes_open)
                        .flex_shrink_0()
                        .px(px(7. * t.zoom))
                        .gap(px(4. * t.zoom))
                        .text_size(px(11.5 * t.zoom))
                        .when(
                            matches!(selected_mode, ApprovalMode::Turbo | ApprovalMode::Auto),
                            |d| d.text_color(t.warning),
                        )
                        .child(elements::icon(Icon::Shield, 12. * t.zoom, t.text3))
                        .child(mode_title)
                        .child(elements::icon(Icon::ChevronDown, 12. * t.zoom, t.text3)),
                )
                .child(
                    self.b(
                        "send",
                        "",
                        if busy { Action::Stop } else { Action::Send },
                        t,
                        cx,
                    )
                    .aria_label(if busy { "Stop agent" } else { "Send message" })
                    .debug_selector(|| "chat-send".into())
                    .ml(px(4. * t.zoom))
                    .p_0()
                    .size(px(30. * t.zoom))
                    .flex_shrink_0()
                    .rounded(px(9. * t.zoom))
                    .bg(if can_send { t.text1 } else { t.pill_strong })
                    .when(can_send, |d| d.cursor_pointer().hover(|d| d.opacity(0.9)))
                    .child(elements::icon(
                        if busy { Icon::Stop } else { Icon::ArrowUp },
                        15. * t.zoom,
                        if can_send {
                            elements::on_primary(t)
                        } else {
                            t.text4
                        },
                    )),
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
        if key == "escape"
            && (self.models_open
                || self.history_open
                || self.modes_open
                || self.directories_open
                || self.prompt.is_some())
        {
            self.models_open = false;
            self.history_open = false;
            self.modes_open = false;
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
            .font_family(tezbar_ui::tokens::font_family(cx, false))
            .relative()
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
                    .gap(px(6. * t.zoom))
                    .child(
                        div()
                            .size(px(26. * t.zoom))
                            .flex_shrink_0()
                            .rounded(px(8. * t.zoom))
                            .border_1()
                            .border_color(t.hairline)
                            .bg(t.pill)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(elements::icon(Icon::Sparkles, 14. * t.zoom, t.text2)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .ml(px(4. * t.zoom))
                            .truncate()
                            .text_size(px(13. * t.zoom))
                            .font_weight(FontWeight::MEDIUM)
                            .child(
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
                    .child(
                        self.icon_button(
                            "history",
                            Icon::History,
                            "Chat history",
                            Action::History,
                            t,
                            cx,
                        )
                        .aria_expanded(self.history_open)
                        .when(self.history_open, |d| d.bg(t.selected)),
                    )
                    .child(self.icon_button("new", Icon::NewChat, "New chat", Action::New, t, cx))
                    .child(controls::back_button(
                        self.b("back", "Back", Action::Back, t, cx),
                        t,
                    )),
            );
        if let Some(error) = &self.error {
            root = root.child(
                div().px(px(12. * t.zoom)).pt(px(8. * t.zoom)).child(
                    elements::alert("chat-error", error.clone(), t).child(
                        self.icon_button(
                            "dismiss-error",
                            Icon::Close,
                            "Dismiss",
                            Action::DismissError,
                            t,
                            cx,
                        )
                        .size(px(20. * t.zoom)),
                    ),
                ),
            );
        }
        if self.clear_inflight {
            root = root.child(
                div()
                    .id("chat-clearing")
                    .role(Role::Status)
                    .flex()
                    .items_center()
                    .gap(px(7. * t.zoom))
                    .px(px(16. * t.zoom))
                    .py(px(8. * t.zoom))
                    .text_size(px(12. * t.zoom))
                    .text_color(t.text3)
                    .child(elements::spinner(
                        "chat-clearing-spin",
                        13. * t.zoom,
                        t.text3,
                    ))
                    .child("Clearing conversations…"),
            );
        }
        if self.controller.active.is_none() {
            root = root.child(self.history(t, cx));
        } else {
            let empty = self.projection.is_empty() && !self.active().is_some_and(|c| c.busy());
            let transcript = if empty {
                // AI Elements `ConversationEmptyState` + `Suggestions`.
                div()
                    .id("chat-empty-transcript")
                    .debug_selector(|| "chat-empty-transcript".into())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(14. * t.zoom))
                    .px(px(24. * t.zoom))
                    .py(px(24. * t.zoom))
                    .child(
                        div()
                            .size(px(40. * t.zoom))
                            .rounded(px(12. * t.zoom))
                            .border_1()
                            .border_color(t.hairline)
                            .bg(t.pill)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(elements::icon(Icon::Sparkles, 18. * t.zoom, t.text2)),
                    )
                    .child(
                        div()
                            .max_w(px(400. * t.zoom))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(6. * t.zoom))
                            .text_center()
                            .child(
                                div()
                                    .text_size(px(16. * t.zoom))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("What do you want to check?"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.5 * t.zoom))
                                    .line_height(gpui::relative(1.5))
                                    .text_color(t.text3)
                                    .child("The agent can inspect this computer and use tools to get real answers. Commands that need permission appear here before they run."),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .justify_center()
                            .gap(px(8. * t.zoom))
                            .children(
                                [
                                    "What is using the most CPU?",
                                    "Show listening ports",
                                    "Find a file on my computer",
                                ]
                                .into_iter()
                                .enumerate()
                                .map(|(i, text)| {
                                    self.b(
                                        format!("suggestion-{i}"),
                                        text,
                                        Action::Draft(text.into()),
                                        t,
                                        cx,
                                    )
                                    .h(px(30. * t.zoom))
                                    .px(px(14. * t.zoom))
                                    .py_0()
                                    .rounded_full()
                                    .border_color(t.rim)
                                    .bg(elements::surface(t))
                                    .hover(|d| d.bg(t.hover))
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(t.text2)
                                    .cursor_pointer()
                                }),
                            ),
                    )
                    .child(
                        self.ghost(
                            "configure-provider",
                            "Configure a provider",
                            Action::Settings,
                            t,
                            cx,
                        )
                        .text_size(px(11.5 * t.zoom))
                        .text_color(t.text3),
                    )
                    .into_any_element()
            } else {
                list(
                    self.transcript.clone(),
                    cx.processor(move |v, index, _, cx| v.row(index, t, cx)),
                )
                .flex_1()
                .min_h_0()
                .pt(px(4. * t.zoom))
                .into_any_element()
            };
            root = root
                .child(transcript)
                .child(self.diagnostics_view(t, cx))
                .child(self.approval(t, cx))
                .child(self.composer(t, cx));
        }
        if self.history_open {
            root = root.child(
                div()
                    .id("chat-history-popover")
                    .occlude()
                    .absolute()
                    .right(px(12. * t.zoom))
                    .top(px((tokens::HEADER + 6.) * t.zoom))
                    .w(px(360. * t.zoom))
                    .h(px(320. * t.zoom))
                    .rounded(px(12. * t.zoom))
                    .border_1()
                    .border_color(t.hairline)
                    .bg(t.popover_solid)
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .h(px(36. * t.zoom))
                            .pl(px(12. * t.zoom))
                            .pr(px(4. * t.zoom))
                            .border_b_1()
                            .border_color(t.hairline)
                            .child(
                                div()
                                    .text_size(px(12. * t.zoom))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(t.text2)
                                    .child("Conversations"),
                            )
                            .child(self.icon_button(
                                "close-history",
                                Icon::Close,
                                "Close",
                                Action::History,
                                t,
                                cx,
                            )),
                    )
                    .child(self.history(t, cx)),
            );
        }
        // AI Elements has no dialog; this mirrors shadcn `AlertDialog`.
        if self.delete_pending.is_some() {
            root = root.child(
                div()
                    .id("chat-delete-scrim")
                    .occlude()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(24. * t.zoom))
                    .bg(hsla(0., 0., 0., if t.light { 0.18 } else { 0.45 }))
                    .child(
                        div()
                            .id("chat-delete-confirmation")
                            .role(Role::AlertDialog)
                            .aria_label("Delete conversations?")
                            .w(px(320. * t.zoom))
                            .max_w_full()
                            .p(px(16. * t.zoom))
                            .rounded(px(14. * t.zoom))
                            .border_1()
                            .border_color(t.rim)
                            .bg(t.popover_solid)
                            .shadow_lg()
                            .flex()
                            .flex_col()
                            .gap(px(6. * t.zoom))
                            .child(
                                div()
                                    .text_size(px(14. * t.zoom))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(if self.clear_pending {
                                        "Delete all saved conversations?"
                                    } else {
                                        "Delete this conversation?"
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(12. * t.zoom))
                                    .text_color(t.text3)
                                    .child("This can't be undone."),
                            )
                            .child(
                                div()
                                    .mt(px(10. * t.zoom))
                                    .flex()
                                    .justify_end()
                                    .gap(px(8. * t.zoom))
                                    .child(
                                        self.outline(
                                            "cancel-delete",
                                            "Cancel",
                                            Action::CancelDelete,
                                            t,
                                            cx,
                                        )
                                        .track_focus(&self.cancel_focus),
                                    )
                                    .child(
                                        self.primary(
                                            "confirm-delete",
                                            "Delete",
                                            Action::ConfirmDelete,
                                            t,
                                            cx,
                                        )
                                        .bg(t.danger)
                                        .text_color(rgb(0xffffff))
                                        .track_focus(&self.confirm_focus),
                                    ),
                            ),
                    ),
            );
        }
        let statuses = self.controller.background();
        let mut footer = controls::footer(t)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6. * t.zoom))
                    .flex_1()
                    .child(elements::icon(Icon::Sparkles, 12. * t.zoom, t.text3))
                    .child("AI"),
            )
            .child(div().text_size(px(10. * t.zoom)).text_color(t.text4).child(
                if cfg!(target_os = "macos") {
                    "Attach screen  ⌘⇧S    Send  ↵    New line  ⇧↵"
                } else {
                    "Attach screen  Ctrl+Shift+S    Send  ↵    New line  Shift+↵"
                },
            ));
        // In parent composition the shared footer owns these statuses and routing.
        for status in statuses
            .into_iter()
            .take(if self.footer.is_some() { 0 } else { 8 })
        {
            footer = footer.child(
                self.ghost(
                    format!("background-{}", status.session_id),
                    format!(
                        "{} · {}",
                        self.metadata
                            .get(&status.session_id)
                            .map(|m| m.0.as_str())
                            .unwrap_or("Chat"),
                        status.activity.label()
                    ),
                    Action::Resume(status.session_id),
                    t,
                    cx,
                )
                .h(px(22. * t.zoom))
                .text_size(px(10.5 * t.zoom))
                .when(
                    status.activity == chat::RunActivity::WaitingForApproval,
                    |d| d.text_color(t.warning),
                ),
            );
        }
        if let Some(indicator) = self.footer.clone() {
            footer = footer.child(indicator);
        }
        root.child(footer)
    }
}
