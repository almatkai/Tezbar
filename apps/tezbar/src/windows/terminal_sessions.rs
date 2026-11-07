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
pub struct SessionsPanel {
    pub sessions: Vec<(String, String)>,
    relay: crate::events::Relay,
    focus: FocusHandle,
    _focus: Subscription,
}
impl SessionsPanel {
    pub fn new(
        sessions: Vec<(String, String)>,
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
                if let Some(i) = tezbar_ui_model::terminal::shortcut_index(
                    "",
                    &e.keystroke.key,
                    e.keystroke.modifiers.platform,
                    e.keystroke.modifiers.control,
                    e.keystroke.modifiers.alt,
                    e.keystroke.modifiers.shift,
                ) {
                    if let Some((id, _)) = this.sessions.get(i) {
                        let _ = this
                            .relay
                            .send(crate::events::Event::TerminalPanelAction(Some(id.clone())));
                        cx.stop_propagation();
                    }
                }
            }))
            .child(
                tezbar_ui::controls::button(
                    "terminal-new",
                    "New session",
                    tezbar_ui::controls::ButtonTone::Quiet,
                    false,
                    t,
                )
                .on_click(move |_, _, _| {
                    let _ = relay.send(crate::events::Event::TerminalPanelAction(None));
                }),
            )
            .child(tezbar_ui::controls::header(t).child("Terminal sessions"))
            .when(self.sessions.is_empty(), |d| d.child("Starting shell…"))
            .child(
                uniform_list(
                    "terminal-session-rows",
                    self.sessions.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, _| {
                        range
                            .map(|i| {
                                let (id, title) = &this.sessions[i];
                                let relay = this.relay.clone();
                                let selected = id.clone();
                                tezbar_ui::virtual_list::row(
                                    id.clone().into(),
                                    title.clone().into(),
                                    i,
                                    this.sessions.len(),
                                    false,
                                    t,
                                )
                                .on_click(move |_, _, _| {
                                    let _ = relay.send(crate::events::Event::TerminalPanelAction(
                                        Some(selected.clone()),
                                    ));
                                })
                                .child(title.clone())
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
