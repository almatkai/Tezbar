//! CSS colour parsing and stable copy rows, ported from renderer/colorConverter.ts.
use regex::Regex;
macro_rules! cached_regex {
    ($pattern:literal) => {{
        static RE: std::sync::LazyLock<Regex> =
            std::sync::LazyLock::new(|| Regex::new($pattern).unwrap());
        &*RE
    }};
}
#[derive(Clone, Debug, PartialEq)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
    pub source: Source,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source {
    Hex,
    Rgb,
    Hsl,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ColorFormatRow {
    pub key: &'static str,
    pub title: &'static str,
    pub value: String,
}
fn round(x: f64, d: i32) -> f64 {
    let f = 10f64.powi(d);
    (x * f + 0.5).floor() / f
}
fn f(x: f64) -> String {
    super::number(x)
}
fn float(s: &str) -> Option<f64> {
    let c = cached_regex!(r"^[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?").find(s.trim())?;
    c.as_str().parse::<f64>().ok().filter(|n| n.is_finite())
}
pub fn parse_color(s: &str) -> Option<Rgba> {
    if s.len() > 4096 {
        return None;
    }
    let s = s.trim();
    if let Some(h) = s.strip_prefix('#') {
        if ![3, 4, 6, 8].contains(&h.len()) || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let h = if h.len() < 5 {
            h.chars().flat_map(|c| [c, c]).collect::<String>()
        } else {
            h.into()
        };
        let byte = |i| u8::from_str_radix(&h[i..i + 2], 16).ok().map(f64::from);
        return Some(Rgba {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a: if h.len() == 8 {
                round(byte(6)? / 255., 3)
            } else {
                1.
            },
            source: Source::Hex,
        });
    }
    let c = cached_regex!(r"(?i)^(rgba?|hsla?)\((.+)\)$").captures(s)?;
    let body = c[2].replace('/', " / ");
    let p: Vec<_> = if body.contains(',') {
        body.split(',').map(str::trim).collect()
    } else {
        body.split_whitespace().collect()
    };
    if p.len() < 3 {
        return None;
    }
    let a = if let Some(i) = p.iter().position(|s| *s == "/") {
        p.get(i + 1).copied()
    } else {
        p.get(3).copied()
    };
    let a = if let Some(a) = a {
        (float(a)? / if a.ends_with('%') { 100. } else { 1. }).clamp(0., 1.)
    } else {
        1.
    };
    let (r, g, b, source) = if c[1].to_lowercase().starts_with("rgb") {
        let ch = |s: &str| -> Option<f64> {
            Some(
                (if s.ends_with('%') {
                    float(s)?.clamp(0., 100.) * 2.55
                } else {
                    float(s)?.clamp(0., 255.)
                } + 0.5)
                    .floor(),
            )
        };
        (ch(p[0])?, ch(p[1])?, ch(p[2])?, Source::Rgb)
    } else {
        let h = float(p[0])?.rem_euclid(360.);
        let sat = (float(p[1])? / 100.).clamp(0., 1.);
        let l = (float(p[2])? / 100.).clamp(0., 1.);
        let chroma = (1. - (2. * l - 1.).abs()) * sat;
        let x = chroma * (1. - ((h / 60.) % 2. - 1.).abs());
        let m = l - chroma / 2.;
        let (r, g, b) = if h < 60. {
            (chroma, x, 0.)
        } else if h < 120. {
            (x, chroma, 0.)
        } else if h < 180. {
            (0., chroma, x)
        } else if h < 240. {
            (0., x, chroma)
        } else if h < 300. {
            (x, 0., chroma)
        } else {
            (chroma, 0., x)
        };
        (
            (255. * (r + m) + 0.5).floor(),
            (255. * (g + m) + 0.5).floor(),
            (255. * (b + m) + 0.5).floor(),
            Source::Hsl,
        )
    };
    Some(Rgba { r, g, b, a, source })
}
impl Rgba {
    fn hex(&self) -> String {
        format!(
            "#{:02X}{:02X}{:02X}",
            self.r as u8, self.g as u8, self.b as u8
        )
    }
    fn rgb(&self) -> String {
        format!("rgb({}, {}, {})", f(self.r), f(self.g), f(self.b))
    }
    fn rgba(&self) -> String {
        format!(
            "rgba({}, {}, {}{})",
            f(self.r),
            f(self.g),
            f(self.b),
            if self.a < 1. {
                format!(", {}", f(round(self.a, 3)))
            } else {
                String::new()
            }
        )
    }
    fn parts(&self) -> (f64, f64, f64, f64, f64) {
        let (r, g, b) = (self.r / 255., self.g / 255., self.b / 255.);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        let l = (max + min) / 2.;
        let h = if d == 0. {
            0.
        } else if max == r {
            60. * ((g - b) / d % 6.)
        } else if max == g {
            60. * ((b - r) / d + 2.)
        } else {
            60. * ((r - g) / d + 4.)
        };
        (
            round((h + 360.) % 360., 2),
            round(
                if d == 0. {
                    0.
                } else {
                    d / (1. - (2. * l - 1.).abs())
                } * 100.,
                3,
            ),
            round(l * 100., 3),
            round(if max == 0. { 0. } else { d / max } * 100., 3),
            round(max * 100., 3),
        )
    }
    fn hsl(&self, alpha: bool) -> String {
        let (h, s, l, _, _) = self.parts();
        format!(
            "{}({}, {}%, {}%{})",
            if alpha { "hsla" } else { "hsl" },
            f(h),
            f(s),
            f(l),
            if alpha && self.a < 1. {
                format!(", {}", f(round(self.a, 3)))
            } else {
                String::new()
            }
        )
    }
}
pub fn build_color_format_rows(c: &Rgba) -> Vec<ColorFormatRow> {
    let pct = |v| f(round(v / 255. * 100., 3));
    let (h, _, _, s, v) = c.parts();
    let a = if c.a < 1. {
        format!(", {}", f(round(c.a, 3)))
    } else {
        String::new()
    };
    vec![
        ("hex", "HEX", c.hex()),
        ("hex-lower", "HEX Lower Case", c.hex().to_lowercase()),
        (
            "hex-no-prefix",
            "HEX No Prefix",
            c.hex()[1..].to_lowercase(),
        ),
        ("rgb", "RGB", c.rgb()),
        (
            "rgb-percent",
            "RGB %",
            format!("rgb({}% {}% {}%)", pct(c.r), pct(c.g), pct(c.b)),
        ),
        ("rgba", "RGBA", c.rgba()),
        (
            "rgba-percent",
            "RGBA %",
            format!("rgba({}%, {}%, {}%{a})", pct(c.r), pct(c.g), pct(c.b)),
        ),
        ("hsla", "HSLA", c.hsl(true)),
        (
            "hsva",
            "HSVA",
            format!(
                "color(--hsv {} {}% {}%{})",
                f(h),
                f(s),
                f(v),
                if c.a < 1. {
                    format!(" / {}", f(round(c.a, 3)))
                } else {
                    String::new()
                }
            ),
        ),
    ]
    .into_iter()
    .map(|(key, title, value)| ColorFormatRow { key, title, value })
    .collect()
}
pub fn conversion_rows(s: &str) -> Vec<ColorFormatRow> {
    let Some(c) = parse_color(s) else {
        return vec![];
    };
    let mut rows = vec![
        ColorFormatRow {
            key: "rgb",
            title: "RGB",
            value: c.rgb(),
        },
        ColorFormatRow {
            key: "hex",
            title: "HEX",
            value: c.hex(),
        },
    ];
    if c.a < 1. {
        rows.extend([
            ColorFormatRow {
                key: "rgba",
                title: "RGBA",
                value: c.rgba(),
            },
            ColorFormatRow {
                key: "hexa",
                title: "HEX Alpha",
                value: format!("{}{:02X}", c.hex(), (c.a * 255. + 0.5).floor() as u8),
            },
        ]);
    }
    rows.push(ColorFormatRow {
        key: "hsl",
        title: "HSL",
        value: c.hsl(false),
    });
    if c.a < 1. {
        rows.push(ColorFormatRow {
            key: "hsla",
            title: "HSLA",
            value: c.hsl(true),
        });
    }
    if c.source != Source::Hex {
        rows.swap(0, 1);
    }
    rows
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn css_formats_and_alpha() {
        let c = parse_color("#f008").unwrap();
        assert_eq!(c.a, 0.533);
        assert_eq!(build_color_format_rows(&c).len(), 9);
        assert_eq!(conversion_rows("rgb(255 0 0 / 50%)")[0].value, "#FF0000");
        assert_eq!(parse_color("hsl(-120 100% 50%)").unwrap().b, 255.);
        for s in ["red", "#foo", "rgb(NaN 0 1)"] {
            assert!(parse_color(s).is_none());
        }
    }
}
