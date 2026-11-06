//! The panel geometry is shared with the future terminal sessions surface.
use gpui::{prelude::*, *};
pub const WIDTH: f32 = 300.;
pub const OVERHANG: f32 = 266.;
pub const TOP: f32 = 96.;
pub const BOTTOM: f32 = 86.;
pub const MIN_HEIGHT: f32 = 260.;
pub fn bounds(parent: Bounds<Pixels>) -> Bounds<Pixels> {
    Bounds::new(
        point(
            parent.right() - px(WIDTH - OVERHANG),
            parent.top() + px(TOP),
        ),
        size(
            px(WIDTH),
            (parent.size.height - px(TOP + BOTTOM)).max(px(MIN_HEIGHT)),
        ),
    )
}
pub struct SessionsPanel {
    pub sessions: Vec<(String, String)>,
    _focus: Subscription,
}
impl SessionsPanel {
    pub fn new(
        sessions: Vec<(String, String)>,
        relay: crate::events::Relay,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.observe_window_activation(w, move |_, w, _| {
            let _ = relay.send(crate::events::Event::PanelFocused(w.is_window_active()));
        });
        Self {
            sessions,
            _focus: focus,
        }
    }
}
impl Render for SessionsPanel {
    fn render(&mut self, w: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = tezbar_ui::tokens::tokens(w);
        tezbar_ui::controls::sheet(t)
            .child(tezbar_ui::controls::header(t).child("Terminal sessions"))
            .children(self.sessions.iter().enumerate().map(|(i, (id, title))| {
                tezbar_ui::virtual_list::row(
                    id.clone().into(),
                    title.clone().into(),
                    i,
                    self.sessions.len(),
                    false,
                    t,
                )
                .child(title.clone())
            }))
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
        assert_eq!(b.origin, point(px(746.), px(126.)));
        assert_eq!(b.size, size(px(300.), px(458.)));
    }
}
