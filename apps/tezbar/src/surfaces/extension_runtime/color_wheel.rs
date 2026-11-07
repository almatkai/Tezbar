//! Native HSV wheel. Painting is pure; pointer/key input sends the schema callback.
use super::*;
use std::{cell::Cell, rc::Rc};
use tezbar_core::instant::color::{ColorFormatRow, Rgba, Source, build_color_format_rows};

pub(super) fn legacy(n: &Node) -> bool {
    let markdown = n.string("markdown").unwrap_or("").trim();
    markdown
        .strip_prefix("![")
        .and_then(|m| m.split_once("]("))
        .is_some_and(|(alt, target)| {
            alt.to_lowercase().contains("color wheel")
                && target.ends_with(')')
                && !target.contains('\n')
        })
}
fn initial(n: &Node) -> [f64; 4] {
    let raw = n.props.get("initialColor");
    let byte = |key| {
        raw.and_then(|r| r.get(key))
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .map(|v| if v <= 1. { v * 255. } else { v })
            .map(|v| v.clamp(0., 255.).round())
    };
    let (r, g, b) = match (byte("red"), byte("green"), byte("blue")) {
        (Some(r), Some(g), Some(b)) => (r / 255., g / 255., b / 255.),
        _ => (1., 44. / 255., 72. / 255.),
    };
    let alpha = raw
        .and_then(|r| r.get("alpha"))
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .map(|v| if v > 1. { v / 255. } else { v })
        .unwrap_or(1.)
        .clamp(0., 1.);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta == 0. {
        0.
    } else if max == r {
        60. * ((g - b) / delta).rem_euclid(6.)
    } else if max == g {
        60. * ((b - r) / delta + 2.)
    } else {
        60. * ((r - g) / delta + 4.)
    };
    [
        hue,
        if max == 0. { 0. } else { delta / max * 100. },
        max * 100.,
        alpha,
    ]
}

fn hsv(h: f32, s: f32, v: f32, a: f32) -> Hsla {
    let c = v * s;
    let h = h / 60.;
    let x = c * (1. - (h % 2. - 1.).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.),
        1 => (x, c, 0.),
        2 => (0., c, x),
        3 => (0., x, c),
        4 => (x, 0., c),
        _ => (c, 0., x),
    };
    rgba(
        ((255. * (r + v - c)) as u32) << 24
            | ((255. * (g + v - c)) as u32) << 16
            | ((255. * (b + v - c)) as u32) << 8
            | (255. * a) as u32,
    )
    .into()
}
impl RuntimeView {
    pub(super) fn wheel(&self, cx: &mut Context<Self>) -> AnyElement {
        let values = self.wheel_values();
        let bounds = self.wheel_bounds.clone();
        let paint_bounds = bounds.clone();
        let owner = cx.entity().downgrade();
        let drag = owner.clone();
        let up = owner.clone();
        let keys = owner.clone();
        div()
            .id("extension-hsv-wheel")
            .role(Role::Slider)
            .aria_label("Colour hue and saturation")
            .tab_index(0)
            .size(px(240.))
            .on_mouse_down(MouseButton::Left, move |e, w, cx| {
                let _ = owner.update(cx, |v, cx| {
                    v.wheel_drag = true;
                    v.wheel_pointer(e.position, w, cx);
                });
            })
            .on_mouse_move(move |e, w, cx| {
                let _ = drag.update(cx, |v, cx| {
                    if v.wheel_drag {
                        v.wheel_pointer(e.position, w, cx);
                    }
                });
            })
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                let _ = up.update(cx, |v, _| v.wheel_drag = false);
            })
            .on_key_down(move |e, w, cx| {
                if matches!(e.keystroke.key.as_str(), "left" | "right" | "up" | "down") {
                    let _ = keys.update(cx, |v, cx| {
                        let mut values = v.wheel_values();
                        match e.keystroke.key.as_str() {
                            "left" => values[0] = (values[0] + 359.) % 360.,
                            "right" => values[0] = (values[0] + 1.) % 360.,
                            "up" => values[1] = (values[1] + 1.).min(100.),
                            _ => values[1] = (values[1] - 1.).max(0.),
                        };
                        v.wheel_update(values, w, cx);
                    });
                    cx.stop_propagation();
                }
            })
            .child(
                canvas(
                    move |b, _, _| {
                        paint_bounds.set(Some(b));
                    },
                    move |b, _, w, _| {
                        let center = b.center();
                        let radius = f32::from(b.size.width.min(b.size.height)) / 2.;
                        for ring in 0..12 {
                            for sector in 0..72 {
                                let a = sector as f32 * std::f32::consts::TAU / 72.;
                                let z = (sector + 1) as f32 * std::f32::consts::TAU / 72.;
                                let inner = radius * ring as f32 / 12.;
                                let outer = radius * (ring + 1) as f32 / 12.;
                                let mut path = PathBuilder::fill();
                                for (i, (r, t)) in [(inner, a), (outer, a), (outer, z), (inner, z)]
                                    .into_iter()
                                    .enumerate()
                                {
                                    let p = center + point(px(r * t.sin()), px(-r * t.cos()));
                                    if i == 0 {
                                        path.move_to(p);
                                    } else {
                                        path.line_to(p);
                                    }
                                }
                                path.close();
                                if let Ok(path) = path.build() {
                                    w.paint_path(
                                        path,
                                        hsv(
                                            sector as f32 * 5.,
                                            (ring as f32 + 0.5) / 12.,
                                            values[2] as f32 / 100.,
                                            1.,
                                        ),
                                    );
                                }
                            }
                        }
                        let angle = values[0] as f32 * std::f32::consts::PI / 180.;
                        let r = radius * values[1] as f32 / 100.;
                        w.paint_quad(quad(
                            Bounds::new(
                                center + point(px(r * angle.sin() - 4.), px(-r * angle.cos() - 4.)),
                                size(px(8.), px(8.)),
                            ),
                            px(4.),
                            hsv(
                                values[0] as f32,
                                values[1] as f32 / 100.,
                                values[2] as f32 / 100.,
                                1.,
                            ),
                            px(2.),
                            white(),
                            Default::default(),
                        ));
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }
    pub(super) fn wheel_values(&self) -> [f64; 4] {
        let Some(s) = &self.session else {
            return [0., 100., 100., 1.];
        };
        s.form.wheel.unwrap_or_else(|| {
            if legacy(&s.root) {
                return initial(&s.root);
            }
            ["hue", "saturation", "value", "alpha"].map(|key| {
                s.root
                    .props
                    .get("nativeColorWheel")
                    .and_then(|v| v[key].as_f64())
                    .unwrap_or(if key == "alpha" { 1. } else { 100. })
            })
        })
    }
    fn wheel_pointer(&mut self, p: Point<Pixels>, w: &mut Window, cx: &mut Context<Self>) {
        let Some(bounds) = self.wheel_bounds.get() else {
            return;
        };
        let center = bounds.center();
        let x = f32::from(p.x - center.x);
        let y = f32::from(p.y - center.y);
        let radius = f32::from(bounds.size.width.min(bounds.size.height)) / 2.;
        if radius <= 0. || x.hypot(y) > radius {
            return;
        }
        let mut values = self.wheel_values();
        values[0] = ((x.atan2(-y).to_degrees() + 360.) % 360.) as f64;
        values[1] = ((x * x + y * y).sqrt() / radius * 100.).min(100.) as f64;
        self.wheel_update(values, w, cx);
    }
    pub(super) fn color_formats(&self) -> Vec<ColorFormatRow> {
        let [h, s, v, a] = self.wheel_values();
        let c = v / 100. * s / 100.;
        let x = c * (1. - ((h / 60.) % 2. - 1.).abs());
        let m = v / 100. - c;
        let (r, g, b) = match (h / 60.) as u32 {
            0 => (c, x, 0.),
            1 => (x, c, 0.),
            2 => (0., c, x),
            3 => (0., x, c),
            4 => (x, 0., c),
            _ => (c, 0., x),
        };
        build_color_format_rows(&Rgba {
            r: ((r + m) * 255.).round(),
            g: ((g + m) * 255.).round(),
            b: ((b + m) * 255.).round(),
            a: a.clamp(0., 1.),
            source: Source::Rgb,
        })
    }
    pub(super) fn copy_color_format(&mut self, key: &str, cx: &mut Context<Self>) {
        if !self.visible || self.disposed || !self.session.as_ref().is_some_and(|s| legacy(&s.root))
        {
            return;
        }
        if let Some(row) = self.color_formats().into_iter().find(|r| r.key == key) {
            self.quiet(ExtensionRequest::Copy(row.value), cx);
        }
    }
    pub(super) fn wheel_update(
        &mut self,
        values: [f64; 4],
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(s) = &self.session else {
            return;
        };
        self.next_ticket += 1;
        let ticket = self.next_ticket;
        self.color_ticket = Some((ticket, s.key.clone(), s.generation.clone()));
        // Coalesce pointer bursts on the canonical callback queue, keeping the in-flight operation.
        if let Some(action) = s
            .root
            .props
            .get("nativeColorWheel")
            .and_then(|v| v["changeActionId"].as_str())
        {
            self.queue.retain(
                |(_, r)| !matches!(r,ExtensionRequest::InvokeOwned{action:id,..}if id==action),
            );
        }
        self.color_chosen(ticket, Ok(values), w, cx);
    }
}
pub(super) fn new_bounds() -> Rc<Cell<Option<Bounds<Pixels>>>> {
    Rc::new(Cell::new(None))
}
