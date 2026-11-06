use gpui::{prelude::*, *};
#[derive(Default)]
pub struct SnapOverlay {
    pub x: bool,
    pub y: bool,
}
impl Render for SnapOverlay {
    fn render(&mut self, w: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = tezbar_ui::tokens::tokens(w);
        div()
            .size_full()
            .when(self.x, |d| {
                d.child(
                    div()
                        .absolute()
                        .left(relative(0.5))
                        .h_full()
                        .w(px(1.))
                        .bg(t.rim),
                )
            })
            .when(self.y, |d| {
                d.child(
                    div()
                        .absolute()
                        .top(relative(0.5))
                        .w_full()
                        .h(px(1.))
                        .bg(t.rim),
                )
            })
    }
}
