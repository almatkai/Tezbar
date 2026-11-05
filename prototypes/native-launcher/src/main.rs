use gpui::*;
struct Launcher;
impl Render for Launcher {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().rounded(px(18.)).bg(rgb(0x1c1d21)).text_color(rgb(0xf5f6fa))
            .child(div().h(px(44.)).px(px(14.)).flex().items_center().child("Search apps, files and commands"))
    }
}
fn main() {
    let runtime = tokio::runtime::Runtime::new().expect("Tokio runtime");
    gpui_platform::application().run(move |cx| {
        gpui_tokio::init_from_handle(cx, runtime.handle().clone());
        cx.open_window(WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(760.), px(640.)), cx))),
            titlebar: None,
            is_resizable: false,
            window_background: WindowBackgroundAppearance::Blurred,
            ..Default::default()
        }, |_, cx| cx.new(|_| Launcher)).expect("launcher window");
        cx.activate(true);
    });
}
