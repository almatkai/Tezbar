//! Native HSV wheel. Painting is pure; pointer/key input sends the schema callback.
use super::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
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

/// HSV (hue in degrees, s/v in 0–1) to linear-free sRGB components in 0–1.
fn hsv_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let c = v * s;
    let h = h.rem_euclid(360.) / 60.;
    let x = c * (1. - (h % 2. - 1.).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.),
        1 => (x, c, 0.),
        2 => (0., c, x),
        3 => (0., x, c),
        4 => (x, 0., c),
        _ => (c, 0., x),
    };
    let m = v - c;
    (r + m, g + m, b + m)
}
fn byte(v: f32) -> u8 {
    (v.clamp(0., 1.) * 255.).round() as u8
}
fn hsv(h: f32, s: f32, v: f32, a: f32) -> Hsla {
    let (r, g, b) = hsv_rgb(h, s, v);
    rgba(
        (byte(r) as u32) << 24 | (byte(g) as u32) << 16 | (byte(b) as u32) << 8 | byte(a) as u32,
    )
    .into()
}
/// (device diameter, brightness ×10, texture). One live texture per view.
pub(super) type WheelImage = Rc<RefCell<Option<(u32, u32, Arc<RenderImage>)>>>;
pub(super) fn new_image() -> WheelImage {
    Rc::new(RefCell::new(None))
}
/// Continuous hue (angle, 0° at the top, clockwise) × saturation (radius) disc at
/// brightness `v`, in GPUI's straight BGRA. The rim is anti-aliased with one pixel
/// of alpha falloff, so the disc needs no border.
pub(super) fn wheel_pixels(diameter: u32, v: f32) -> image::RgbaImage {
    let mut buffer = image::RgbaImage::new(diameter, diameter);
    let radius = diameter as f32 / 2.;
    for (x, y, pixel) in buffer.enumerate_pixels_mut() {
        let dx = x as f32 + 0.5 - radius;
        let dy = y as f32 + 0.5 - radius;
        let distance = dx.hypot(dy);
        let alpha = (radius - distance + 0.5).clamp(0., 1.);
        if alpha == 0. {
            continue;
        }
        let hue = dx.atan2(-dy).to_degrees();
        let (r, g, b) = hsv_rgb(hue, (distance / radius).min(1.), v);
        *pixel = image::Rgba([byte(b), byte(g), byte(r), byte(alpha)]);
    }
    buffer
}
impl RuntimeView {
    pub(super) fn wheel(&self, diameter: f32, cx: &mut Context<Self>) -> AnyElement {
        let values = self.wheel_values();
        let bounds = self.wheel_bounds.clone();
        let paint_bounds = bounds.clone();
        let texture_cache = self.wheel_image.clone();
        let owner = cx.entity().downgrade();
        let drag = owner.clone();
        let up = owner.clone();
        let keys = owner.clone();
        div()
            .id("extension-hsv-wheel")
            .role(Role::Slider)
            .aria_label("Colour hue and saturation")
            .tab_index(0)
            .size(px(diameter))
            .flex_shrink_0()
            .cursor_crosshair()
            .on_mouse_down(MouseButton::Left, move |e, w, cx| {
                let _ = owner.update(cx, |v, cx| {
                    // Only a press on the disc starts a drag; the square's corners are inert.
                    v.wheel_drag = v.wheel_pointer(e.position, false, w, cx);
                });
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
                        // Window-level listeners keep a drag alive once the pointer leaves
                        // the disc (it pins to the rim) and end it wherever the button is released.
                        w.on_mouse_event(move |e: &MouseMoveEvent, phase, w, cx| {
                            if phase != DispatchPhase::Bubble {
                                return;
                            }
                            let _ = drag.update(cx, |v, cx| {
                                if !v.wheel_drag {
                                    return;
                                }
                                if e.pressed_button == Some(MouseButton::Left) {
                                    v.wheel_pointer(e.position, true, w, cx);
                                } else {
                                    v.wheel_drag = false;
                                }
                            });
                        });
                        w.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble && e.button == MouseButton::Left {
                                let _ = up.update(cx, |v, _| v.wheel_drag = false);
                            }
                        });
                        let center = b.center();
                        let side = b.size.width.min(b.size.height);
                        let radius = f32::from(side) / 2.;
                        let disc = Bounds::new(
                            center - point(side / 2., side / 2.),
                            size(side, side),
                        );
                        // One smooth texture at device resolution; rebuilt only when the
                        // size or brightness changes, releasing the previous one.
                        let device = (radius * 2. * w.scale_factor()).round().max(1.) as u32;
                        let brightness = (values[2].clamp(0., 100.) * 10.).round() as u32;
                        let texture = {
                            let mut slot = texture_cache.borrow_mut();
                            match &*slot {
                                Some((d, v, t)) if *d == device && *v == brightness => t.clone(),
                                _ => {
                                    if let Some((_, _, old)) = slot.take() {
                                        let _ = w.drop_image(old);
                                    }
                                    let pixels = wheel_pixels(device, brightness as f32 / 1000.);
                                    let t = Arc::new(RenderImage::new(vec![image::Frame::new(
                                        pixels,
                                    )]));
                                    *slot = Some((device, brightness, t.clone()));
                                    t
                                }
                            }
                        };
                        let _ = w.paint_image(disc, disc, Corners::default(), texture, 0, false);
                        // Marker: the picked colour in a white ring over a soft shadow.
                        let angle = values[0] as f32 * std::f32::consts::PI / 180.;
                        let r = radius * values[1] as f32 / 100.;
                        let at = center + point(px(r * angle.sin()), px(-r * angle.cos()));
                        let dot = |d: f32| Bounds::new(at - point(px(d / 2.), px(d / 2.)), size(px(d), px(d)));
                        w.paint_quad(quad(
                            dot(20.),
                            px(10.),
                            rgba(0x00000033),
                            px(0.),
                            transparent_black(),
                            Default::default(),
                        ));
                        w.paint_quad(quad(
                            dot(16.),
                            px(8.),
                            hsv(
                                values[0] as f32,
                                values[1] as f32 / 100.,
                                values[2] as f32 / 100.,
                                1.,
                            ),
                            px(2.5),
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
    /// Maps a pointer to hue/saturation. Outside the disc it is ignored unless `clamp`
    /// (an active drag), which pins it to the rim. Returns whether the point was used.
    fn wheel_pointer(
        &mut self,
        p: Point<Pixels>,
        clamp: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(bounds) = self.wheel_bounds.get() else {
            return false;
        };
        let center = bounds.center();
        let x = f32::from(p.x - center.x);
        let y = f32::from(p.y - center.y);
        let radius = f32::from(bounds.size.width.min(bounds.size.height)) / 2.;
        let Some((hue, saturation)) = wheel_coordinates(x, y, radius, clamp) else {
            return false;
        };
        let mut values = self.wheel_values();
        if values[0] == hue && values[1] == saturation {
            return true;
        }
        values[0] = hue;
        values[1] = saturation;
        self.wheel_update(values, w, cx);
        true
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
/// Whole-number hue (0–359°) and saturation (0–100 %) for a pointer offset from the
/// centre. `None` outside the disc unless `clamp`, which pins the point to the rim.
pub(super) fn wheel_coordinates(x: f32, y: f32, radius: f32, clamp: bool) -> Option<(f64, f64)> {
    let distance = x.hypot(y);
    if radius <= 0. || (!clamp && distance > radius) {
        return None;
    }
    let hue = (x.atan2(-y).to_degrees().round() + 360.) % 360.;
    let saturation = (distance / radius * 100.).min(100.).round();
    Some((hue as f64, saturation as f64))
}
/// Swatch colour for the current wheel values (alpha ignored).
pub(super) fn swatch([h, s, v, _]: [f64; 4]) -> Hsla {
    hsv(
        (h.rem_euclid(360.)) as f32,
        (s / 100.).clamp(0., 1.) as f32,
        (v / 100.).clamp(0., 1.) as f32,
        1.,
    )
}
pub(super) fn new_bounds() -> Rc<Cell<Option<Bounds<Pixels>>>> {
    Rc::new(Cell::new(None))
}
