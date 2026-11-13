//! The panel geometry is shared with the future terminal sessions surface.
use gpui::{prelude::*, *};
pub const WIDTH: f32 = 300.;
pub const OVERHANG: f32 = 266.;
pub const TOP: f32 = 96.;
pub const BOTTOM: f32 = 86.;
pub const MIN_HEIGHT: f32 = 260.;
pub fn bounds(parent: Bounds<Pixels>) -> Bounds<Pixels> {
    Bounds::new(
        point(parent.left() - px(OVERHANG), parent.top() + px(TOP)),
        size(
            px(WIDTH),
            (parent.size.height - px(TOP + BOTTOM)).max(px(MIN_HEIGHT)),
        ),
    )
}
#[derive(Clone, Debug, serde::Deserialize)]
pub struct SessionRow {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub running: bool,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub last_command: Option<String>,
    #[serde(default)]
    pub updated_at: Option<u64>,
}
/// The authenticated fixture channel predates typed production rows. Preserve
/// its [id, title] wire while also accepting the richer native row objects.
#[cfg(feature = "bench-control")]
pub fn fixture_rows(value: serde_json::Value) -> Result<Vec<SessionRow>, String> {
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Row {
        Native(SessionRow),
        Legacy((String, String)),
    }
    serde_json::from_value::<Vec<Row>>(value)
        .map(|rows| {
            rows.into_iter()
                .map(|row| match row {
                    Row::Native(row) => row,
                    Row::Legacy((id, title)) => SessionRow {
                        id,
                        title,
                        cwd: String::new(),
                        running: false,
                        active: false,
                        last_command: None,
                        updated_at: None,
                    },
                })
                .collect()
        })
        .map_err(|error| error.to_string())
}
pub struct SessionsPanel {
    pub sessions: Vec<SessionRow>,
    relay: crate::events::Relay,
    focus: FocusHandle,
    _focus: Subscription,
}
impl SessionsPanel {
    pub fn new(
        sessions: Vec<SessionRow>,
        relay: crate::events::Relay,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let handle = cx.focus_handle();
        handle.focus(w, cx);
        let relay_focus = relay.clone();
        let focus = cx.observe_window_activation(w, move |_, w, _| {
            let _ = relay_focus.send(crate::events::Event::PanelFocused(w.is_window_active()));
        });
        Self {
            sessions,
            relay,
            focus: handle,
            _focus: focus,
        }
    }
}
fn session_age(at: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let minutes = now.saturating_sub(at) / 60_000;
    match minutes {
        0 => "now".into(),
        1..=59 => format!("{minutes}m ago"),
        60..=1439 => format!("{}h ago", minutes / 60),
        _ => format!("{}d ago", minutes / 1440),
    }
}
impl Render for SessionsPanel {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tezbar_ui::tokens::tokens(w);
        let relay = self.relay.clone();
        tezbar_ui::controls::sheet(t)
            .font_family(tezbar_ui::tokens::font_family(cx, false))
            .p(px(12. * t.zoom))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "escape" {
                    let _ = this.relay.send(crate::events::Event::PanelHide);
                    cx.stop_propagation();
                }
                let m = e.keystroke.modifiers;
                if e.keystroke.key == "n"
                    && (m.platform || (cfg!(not(target_os = "macos")) && m.control))
                    && !m.alt
                    && !m.shift
                {
                    let _ = this
                        .relay
                        .send(crate::events::Event::TerminalPanelAction(None));
                    cx.stop_propagation();
                }
                if let Some(i) = tezbar_ui_model::terminal::shortcut_index(
                    "",
                    &e.keystroke.key,
                    e.keystroke.modifiers.platform,
                    e.keystroke.modifiers.control,
                    e.keystroke.modifiers.alt,
                    e.keystroke.modifiers.shift,
                ) {
                    if let Some(row) = this.sessions.get(i) {
                        let _ = this
                            .relay
                            .send(crate::events::Event::TerminalPanelAction(Some(
                                row.id.clone(),
                            )));
                        cx.stop_propagation();
                    }
                }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .px(px(4.))
                    .mb(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(11. * t.zoom))
                                    .text_color(t.success)
                                    .child("Terminal Sessions"),
                            )
                            .child(
                                div()
                                    .text_size(px(10.5 * t.zoom))
                                    .text_color(t.text4)
                                    .child(format!(
                                        "{} running",
                                        self.sessions.iter().filter(|s| s.running).count()
                                    )),
                            ),
                    )
                    .child(
                        tezbar_ui::controls::button(
                            "terminal-new",
                            "+",
                            tezbar_ui::controls::ButtonTone::Quiet,
                            false,
                            t,
                        )
                        .aria_label("New terminal session")
                        .size(px(28. * t.zoom))
                        .on_click(move |_, _, _| {
                            let _ = relay.send(crate::events::Event::TerminalPanelAction(None));
                        }),
                    ),
            )
            .when(self.sessions.is_empty(), |d| d.child("Starting shell…"))
            .child(
                uniform_list(
                    "terminal-session-rows",
                    self.sessions.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, _| {
                        range
                            .map(|i| {
                                let row = &this.sessions[i];
                                let relay = this.relay.clone();
                                let selected = row.id.clone();
                                tezbar_ui::virtual_list::row(
                                    row.id.clone().into(),
                                    format!("{}, {}", row.title, row.cwd).into(),
                                    i,
                                    this.sessions.len(),
                                    row.active,
                                    t,
                                )
                                .h(px(64. * t.zoom))
                                .px(px(10. * t.zoom))
                                .py(px(8. * t.zoom))
                                .on_click(move |_, _, _| {
                                    let _ = relay.send(crate::events::Event::TerminalPanelAction(
                                        Some(selected.clone()),
                                    ));
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
                                                .child(row.title.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(10. * t.zoom))
                                                .font_family(tezbar_ui::tokens::mono_family())
                                                .text_color(t.text4)
                                                .truncate()
                                                .child(format!(
                                                    "{}{}",
                                                    tezbar_ui_model::terminal::compact_path(
                                                        &row.cwd
                                                    ),
                                                    row.last_command
                                                        .as_ref()
                                                        .filter(|v| !v.is_empty())
                                                        .map(|v| format!(" · {v}"))
                                                        .unwrap_or_default()
                                                )),
                                        )
                                        .child(
                                            div()
                                                .mt(px(4.))
                                                .text_size(px(9.5 * t.zoom))
                                                .text_color(t.text4)
                                                .child(if row.running {
                                                    "running".to_owned()
                                                } else {
                                                    row.updated_at
                                                        .map(|at| {
                                                            format!("saved {}", session_age(at))
                                                        })
                                                        .unwrap_or_else(|| "saved".into())
                                                }),
                                        ),
                                )
                                .when(i < 9, |d| {
                                    d.child(
                                        div()
                                            .rounded(px(6.))
                                            .border_1()
                                            .border_color(t.hairline)
                                            .bg(t.pill)
                                            .px(px(6.))
                                            .py(px(2.))
                                            .text_size(px(9. * t.zoom))
                                            .child(format!(
                                                "{}{}",
                                                if cfg!(target_os = "macos") {
                                                    "⌘"
                                                } else {
                                                    "Ctrl+"
                                                },
                                                i + 1
                                            )),
                                    )
                                })
                                .into_any_element()
                            })
                            .collect()
                    }),
                )
                .flex_1(),
            )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "bench-control")]
    #[::core::prelude::v1::test]
    fn bench_wire_preserves_legacy_rows_and_native_metadata() {
        let rows = fixture_rows(serde_json::json!([
            ["legacy", "Shell"],
            {"id":"current", "title":"Project", "cwd":"/fixture", "running":true, "active":true},
            {"id":"minimal", "title":"Saved"}
        ]))
        .unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(
            (rows[0].id.as_str(), rows[0].title.as_str()),
            ("legacy", "Shell")
        );
        assert!(rows[0].cwd.is_empty() && !rows[0].running && !rows[0].active);
        assert_eq!(rows[1].cwd, "/fixture");
        assert!(rows[1].running && rows[1].active);
        assert!(rows[2].cwd.is_empty() && !rows[2].running && !rows[2].active);
        for invalid in [
            serde_json::json!([["missing-title"]]),
            serde_json::json!([{"id":"missing-title"}]),
            serde_json::json!([{"id":"bad", "title":"bad", "active":"yes"}]),
        ] {
            assert!(fixture_rows(invalid).is_err());
        }
    }
    #[::core::prelude::v1::test]
    fn panel_tracks_parent_and_minimum() {
        let b = bounds(Bounds::new(
            point(px(20.), px(30.)),
            size(px(760.), px(640.)),
        ));
        assert_eq!(b.origin, point(px(-246.), px(126.)));
        assert_eq!(b.size, size(px(300.), px(458.)));
    }
}
