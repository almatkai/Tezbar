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
impl Render for SessionsPanel {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tezbar_ui::tokens::tokens(w);
        let relay = self.relay.clone();
        tezbar_ui::controls::sheet(t)
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
                tezbar_ui::controls::button(
                    "terminal-new",
                    if cfg!(target_os = "macos") {
                        "New session · ⌘N"
                    } else {
                        "New session · Ctrl+N"
                    },
                    tezbar_ui::controls::ButtonTone::Quiet,
                    false,
                    t,
                )
                .on_click(move |_, _, _| {
                    let _ = relay.send(crate::events::Event::TerminalPanelAction(None));
                }),
            )
            .child(tezbar_ui::controls::header(t).child(format!(
                "Terminal sessions · {} running",
                self.sessions.iter().filter(|s| s.running).count()
            )))
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
                                        .child(div().truncate().child(row.title.clone()))
                                        .child(
                                            div()
                                                .text_size(px(11. * t.zoom))
                                                .text_color(if row.active {
                                                    rgb(0xffffff).into()
                                                } else {
                                                    t.text3
                                                })
                                                .truncate()
                                                .child(tezbar_ui_model::terminal::compact_path(
                                                    &row.cwd,
                                                )),
                                        ),
                                )
                                .when(i < 9, |d| {
                                    d.child(div().text_size(px(11. * t.zoom)).child(format!(
                                        "{}{}",
                                        if cfg!(target_os = "macos") {
                                            "⌘"
                                        } else {
                                            "Ctrl+"
                                        },
                                        i + 1
                                    )))
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
