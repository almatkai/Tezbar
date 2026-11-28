//! Resize tracks the OS cursor in desktop coordinates, so moving the window cannot distort deltas.
use gpui::{prelude::*, *};
use tezbar_host::layout::{ResizeEdge, resized_rect};
#[derive(Clone, Copy)]
pub struct Gesture {
    bounds: Bounds<Pixels>,
    origin: (f64, f64),
    edge: ResizeEdge,
}
fn cursor(w: &Window) -> Option<(f64, f64)> {
    let (x, y) = tezbar_host::monitors::cursor_point()?;
    let scale = if cfg!(windows) {
        w.scale_factor() as f64
    } else {
        1.
    };
    Some((x / scale, y / scale))
}
impl Gesture {
    pub fn begin(edge: ResizeEdge, w: &Window) -> Option<Self> {
        Some(Self {
            bounds: w.bounds(),
            origin: cursor(w)?,
            edge,
        })
    }
    pub fn update(self, w: &mut Window, zoom: f32, cx: &mut App) -> Result<(), String> {
        let Some((x, y)) = cursor(w) else {
            return Ok(());
        };
        let display = w
            .display(cx)
            .map(|display| display.visible_bounds())
            .unwrap_or(self.bounds);
        let b = self.bounds;
        let (x, y, width, height) = resized_rect(
            (
                f64::from(b.left()),
                f64::from(b.top()),
                f64::from(b.size.width),
                f64::from(b.size.height),
            ),
            (x - self.origin.0, y - self.origin.1),
            self.edge,
            (480. * zoom as f64, 360. * zoom as f64),
            (
                f64::from(display.size.width),
                f64::from(display.size.height),
            ),
        );
        super::native::place(
            w,
            Bounds::new(
                point(px(x as f32), px(y as f32)),
                size(px(width as f32), px(height as f32)),
            ),
            cx,
        )
    }
}
pub fn handles<V: 'static>(
    cx: &mut Context<V>,
    start: fn(&mut V, ResizeEdge, &mut Window, &mut Context<V>),
) -> Vec<Stateful<Div>> {
    use ResizeEdge::*;
    [
        (North, CursorStyle::ResizeUp),
        (South, CursorStyle::ResizeDown),
        (West, CursorStyle::ResizeLeft),
        (East, CursorStyle::ResizeRight),
        (NorthWest, CursorStyle::ResizeUpLeftDownRight),
        (NorthEast, CursorStyle::ResizeUpRightDownLeft),
        (SouthWest, CursorStyle::ResizeUpRightDownLeft),
        (SouthEast, CursorStyle::ResizeUpLeftDownRight),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (edge, cursor))| {
        let corner = matches!(edge, NorthWest | NorthEast | SouthWest | SouthEast);
        div()
            .id(("resize", i))
            .debug_selector(move || format!("window-resize-{i}"))
            .absolute()
            .cursor(cursor)
            .when(corner, |d| d.size(px(8.)))
            .when(!corner && matches!(edge, North | South), |d| {
                d.left(px(8.)).right(px(8.)).h(px(4.))
            })
            .when(!corner && matches!(edge, East | West), |d| {
                d.top(px(8.)).bottom(px(8.)).w(px(4.))
            })
            .when(matches!(edge, North | NorthWest | NorthEast), |d| d.top_0())
            .when(matches!(edge, South | SouthWest | SouthEast), |d| {
                d.bottom_0()
            })
            .when(matches!(edge, West | NorthWest | SouthWest), |d| d.left_0())
            .when(matches!(edge, East | NorthEast | SouthEast), |d| {
                d.right_0()
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _, w, cx| {
                    start(view, edge, w, cx);
                    cx.stop_propagation();
                }),
            )
    })
    .collect()
}
