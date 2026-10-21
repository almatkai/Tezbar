//! Native V2 port of thomas/color-picker (MIT), from the installed Raycast bundle.
//! Behavior: src/{pick-color,extract-color,menu-bar,organize-colors,favorite-colors,
//! generate-colors,color-names,color-wheel,convert-color}; history schema from lib/history.ts.
//! v2_color_picker_names.json contains the six color-namer 1.4.0 palettes bundled
//! with that extension (color-namer 1.4.0, Copyright (c) Zeke Sikelianos 2016,
//! MIT; NTC names by Chirag Mehta). Full upstream license:
//! v2_color_picker_LICENSE.txt. Upstream tag: https://github.com/colorjs/color-namer/tree/v1.4.0
//! Color calculations are Rust implementations of CSS Color 4 / ColorJS spaces;
//! naming uses the source's CIE76 distance in D65 Lab (chroma-js default).
//! No JavaScript or installed extension code is executed. Native desktop sampling
//! delegates only to Tezbar's packaged AppKit/Windows helper.

use super::v2_sdk as sdk;
use crate::native_core::common::{CoreContext, CoreResult};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const SLUG: &str = "color-picker";
pub const COLOR_NAMER_LICENSE: &str = include_str!("v2_color_picker_LICENSE.txt");
const FORMATS: &[(&str, &str)] = &[
    ("hex", "HEX"),
    ("hex-lower-case", "HEX Lower Case"),
    ("hex-no-prefix", "HEX No Prefix"),
    ("rgb", "RGB"),
    ("rgb-percentage", "RGB %"),
    ("rgba", "RGBA"),
    ("rgba-percentage", "RGBA %"),
    ("hsla", "HSLA"),
    ("hsva", "HSVA"),
    ("oklch", "OKLCH"),
    ("lch", "LCH"),
    ("p3", "Display P3"),
];

pub fn commands() -> &'static [&'static str] {
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        &[
            "pick-color",
            "extract-color",
            "menu-bar",
            "organize-colors",
            "favorite-colors",
            "generate-colors",
            "color-names",
            "color-wheel",
            "convert-color",
        ]
    } else {
        &[
            "extract-color",
            "organize-colors",
            "favorite-colors",
            "generate-colors",
            "color-names",
            "color-wheel",
            "convert-color",
        ]
    }
}

#[derive(Clone, Copy, Debug)]
struct Color {
    r: f64,
    g: f64,
    b: f64,
    a: f64,
}

fn number(s: &str) -> Result<f64, String> {
    s.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("Invalid color component: {s}"))
}
fn scaled(s: &str, scale: f64) -> Result<f64, String> {
    if let Some(v) = s.strip_suffix('%') {
        Ok(number(v)? * scale / 100.0)
    } else {
        number(s)
    }
}
fn hue(s: &str) -> Result<f64, String> {
    let v = if let Some(v) = s.strip_suffix("turn") {
        number(v)? * 360.0
    } else if let Some(v) = s.strip_suffix("grad") {
        number(v)? * 0.9
    } else if let Some(v) = s.strip_suffix("rad") {
        number(v)?.to_degrees()
    } else {
        number(s.strip_suffix("deg").unwrap_or(s))?
    };
    Ok(v.rem_euclid(360.0))
}
fn triplet(body: &str) -> Result<([&str; 3], f64), String> {
    let (parts, alpha) = if body.contains(',') {
        if body.contains('/') {
            return Err("Cannot mix comma and slash color syntax".into());
        }
        let v: Vec<_> = body.split(',').map(str::trim).collect();
        if v.len() != 3 && v.len() != 4 {
            return Err("Expected three color channels and optional alpha".into());
        }
        (v[..3].to_vec(), v.get(3).copied())
    } else {
        let v: Vec<_> = body.split('/').collect();
        if v.len() > 2 {
            return Err("Too many alpha separators".into());
        }
        (
            v[0].split_whitespace().collect(),
            v.get(1).map(|s| s.trim()),
        )
    };
    if parts.len() != 3 {
        return Err("Expected exactly three color channels".into());
    }
    let a = alpha
        .map(|s| scaled(s, 1.0))
        .transpose()?
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    Ok(([parts[0], parts[1], parts[2]], a))
}
fn linear(v: f64) -> f64 {
    if v.abs() <= 0.04045 {
        v / 12.92
    } else {
        v.signum() * ((v.abs() + 0.055) / 1.055).powf(2.4)
    }
}
fn gamma(v: f64) -> f64 {
    if v.abs() <= 0.0031308 {
        v * 12.92
    } else {
        v.signum() * (1.055 * v.abs().powf(1.0 / 2.4) - 0.055)
    }
}
fn mul(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    m.map(|r| r[0] * v[0] + r[1] * v[1] + r[2] * v[2])
}
const RGB_XYZ: [[f64; 3]; 3] = [
    [0.4123907993, 0.3575843394, 0.1804807884],
    [0.2126390059, 0.7151686788, 0.0721923154],
    [0.0193308187, 0.1191947798, 0.9505321522],
];
const XYZ_RGB: [[f64; 3]; 3] = [
    [3.2409699419, -1.5373831776, -0.4986107603],
    [-0.9692436363, 1.8759675015, 0.0415550574],
    [0.0556300797, -0.2039769589, 1.0569715142],
];
const P3_XYZ: [[f64; 3]; 3] = [
    [0.4865709486, 0.2656676932, 0.1982172852],
    [0.2289745641, 0.6917385218, 0.0792869141],
    [0.0, 0.0451133819, 1.0439443689],
];
const XYZ_P3: [[f64; 3]; 3] = [
    [2.4934969119, -0.9313836179, -0.4027107845],
    [-0.8294889696, 1.7626640603, 0.0236246858],
    [0.0358458302, -0.0761723893, 0.9568845240],
];
const TO_D50: [[f64; 3]; 3] = [
    [1.0479298208, 0.0229467933, -0.0501922295],
    [0.0296278157, 0.9904344846, -0.0170738250],
    [-0.0092430582, 0.0150551449, 0.7518742814],
];
const TO_D65: [[f64; 3]; 3] = [
    [0.9554734527, -0.0230985369, 0.0632593087],
    [-0.0283697069, 1.0099954580, 0.0210413989],
    [0.0123140017, -0.0205076964, 1.3303659366],
];
fn lab(xyz: [f64; 3], white: [f64; 3]) -> [f64; 3] {
    let f = |v: f64| {
        if v > 216.0 / 24389.0 {
            v.cbrt()
        } else {
            (24389.0 / 27.0 * v + 16.0) / 116.0
        }
    };
    let v = [
        f(xyz[0] / white[0]),
        f(xyz[1] / white[1]),
        f(xyz[2] / white[2]),
    ];
    [
        116.0 * v[1] - 16.0,
        500.0 * (v[0] - v[1]),
        200.0 * (v[1] - v[2]),
    ]
}
fn unlab(v: [f64; 3]) -> [f64; 3] {
    let f = |v: f64| {
        if v > 6.0 / 29.0 {
            v.powi(3)
        } else {
            3.0 * (6.0_f64 / 29.0).powi(2) * (v - 4.0 / 29.0)
        }
    };
    let y = (v[0] + 16.0) / 116.0;
    [
        0.9642956764 * f(y + v[1] / 500.0),
        f(y),
        0.8251046025 * f(y - v[2] / 200.0),
    ]
}
fn polar(v: [f64; 3]) -> [f64; 3] {
    let c = v[1].hypot(v[2]);
    [
        v[0],
        c,
        if c < 0.00001 {
            0.0
        } else {
            v[2].atan2(v[1]).to_degrees().rem_euclid(360.0)
        },
    ]
}
fn cartesian(v: [f64; 3]) -> [f64; 3] {
    [
        v[0],
        v[1] * v[2].to_radians().cos(),
        v[1] * v[2].to_radians().sin(),
    ]
}
fn oklab(rgb: [f64; 3]) -> [f64; 3] {
    let v = mul(
        [
            [0.4122214708, 0.5363325363, 0.0514459929],
            [0.2119034982, 0.6806995451, 0.1073969566],
            [0.0883024619, 0.2817188376, 0.6299787005],
        ],
        rgb,
    )
    .map(f64::cbrt);
    mul(
        [
            [0.2104542553, 0.7936177850, -0.0040720468],
            [1.9779984951, -2.4285922050, 0.4505937099],
            [0.0259040371, 0.7827717662, -0.8086757660],
        ],
        v,
    )
}
fn unoklab(v: [f64; 3]) -> [f64; 3] {
    let v = mul(
        [
            [1.0, 0.3963377774, 0.2158037573],
            [1.0, -0.1055613458, -0.0638541728],
            [1.0, -0.0894841775, -1.2914855480],
        ],
        v,
    )
    .map(|x| x.powi(3));
    mul(
        [
            [4.0767416621, -3.3077115913, 0.2309699292],
            [-1.2684380046, 2.6097574011, -0.3413193965],
            [-0.0041960863, -0.7034186147, 1.7076147010],
        ],
        v,
    )
}

impl Color {
    fn from_rgb(v: [f64; 3], a: f64) -> Self {
        Self {
            r: v[0],
            g: v[1],
            b: v[2],
            a,
        }
    }
    fn from_xyz(v: [f64; 3], a: f64) -> Self {
        Self::from_rgb(mul(XYZ_RGB, v).map(gamma), a)
    }
    fn rgb(self) -> [f64; 3] {
        [self.r, self.g, self.b]
    }
    fn xyz(self) -> [f64; 3] {
        mul(RGB_XYZ, self.rgb().map(linear))
    }
    fn hsv(h: f64, s: f64, v: f64, a: f64) -> Self {
        let c = v * s;
        let x = c * (1.0 - ((h.rem_euclid(360.0) / 60.0) % 2.0 - 1.0).abs());
        let m = v - c;
        let v = match (h.rem_euclid(360.0) / 60.0) as u8 {
            0 => [c, x, 0.0],
            1 => [x, c, 0.0],
            2 => [0.0, c, x],
            3 => [0.0, x, c],
            4 => [x, 0.0, c],
            _ => [c, 0.0, x],
        };
        Self::from_rgb(v.map(|x| x + m), a)
    }
    fn hsl(h: f64, s: f64, l: f64, a: f64) -> Self {
        let v = l + s * l.min(1.0 - l);
        Self::hsv(h, if v == 0.0 { 0.0 } else { 2.0 * (1.0 - l / v) }, v, a)
    }
    fn hsv_parts(self) -> [f64; 3] {
        let [r, g, b] = self.rgb();
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        let h = if d.abs() < 1e-12 {
            0.0
        } else if max == r {
            60.0 * ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            60.0 * ((b - r) / d + 2.0)
        } else {
            60.0 * ((r - g) / d + 4.0)
        };
        [h, if max == 0.0 { 0.0 } else { d / max }, max]
    }
    fn hsl_parts(self) -> [f64; 3] {
        let [h, _, max] = self.hsv_parts();
        let min = self.r.min(self.g).min(self.b);
        let l = (max + min) / 2.0;
        [
            h,
            if max == min {
                0.0
            } else {
                (max - min) / (1.0 - (2.0 * l - 1.0).abs())
            },
            l,
        ]
    }
    fn hex(self) -> String {
        let b = self
            .rgb()
            .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
        let mut s = format!("#{:02X}{:02X}{:02X}", b[0], b[1], b[2]);
        if self.a < 1.0 {
            s.push_str(&format!("{:02X}", (self.a * 255.0).round() as u8));
        }
        s
    }
    fn parse(input: &str) -> Result<Self, String> {
        let text = input.trim().to_ascii_lowercase();
        if text == "transparent" {
            return Ok(Self::from_rgb([0.0; 3], 0.0));
        }
        if text == "rebeccapurple" {
            return Self::parse("#663399");
        }
        let hex = text.strip_prefix('#').unwrap_or(&text);
        if [3, 4, 6, 8].contains(&hex.len()) && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            let expanded = if hex.len() < 5 {
                hex.chars().flat_map(|c| [c, c]).collect::<String>()
            } else {
                hex.to_string()
            };
            let mut b = [255u8; 4];
            for (i, chunk) in expanded.as_bytes().chunks(2).enumerate() {
                b[i] =
                    u8::from_str_radix(std::str::from_utf8(chunk).map_err(|e| e.to_string())?, 16)
                        .map_err(|e| e.to_string())?;
            }
            return Ok(Self::from_rgb(
                [
                    b[0] as f64 / 255.0,
                    b[1] as f64 / 255.0,
                    b[2] as f64 / 255.0,
                ],
                b[3] as f64 / 255.0,
            ));
        }
        if let Some(v) = palettes()
            .get("html")
            .and_then(Value::as_array)
            .and_then(|v| v.iter().find(|v| v["name"].as_str() == Some(&text)))
        {
            return Self::parse(v["hex"].as_str().ok_or("Invalid named color")?);
        }
        let (kind,body)=text.split_once('(').ok_or("Expected HEX, a CSS color name, rgb(), hsl(), hsv(), lab(), lch(), oklch(), or color()")?;
        let body = body
            .strip_suffix(')')
            .ok_or("Color function is missing its closing parenthesis")?;
        let (space, body) = if kind == "color" {
            body.split_once(char::is_whitespace)
                .ok_or("color() requires a color space")?
        } else {
            (kind, body)
        };
        let (v, a) = triplet(body)?;
        let parsed = match space {
            "rgb" | "rgba" => Self::from_rgb(
                [
                    scaled(v[0], 255.0)? / 255.0,
                    scaled(v[1], 255.0)? / 255.0,
                    scaled(v[2], 255.0)? / 255.0,
                ]
                .map(|x| x.clamp(0.0, 1.0)),
                a,
            ),
            "hsl" | "hsla" => Self::hsl(
                hue(v[0])?,
                (scaled(v[1], 100.0)? / 100.0).clamp(0.0, 1.0),
                (scaled(v[2], 100.0)? / 100.0).clamp(0.0, 1.0),
                a,
            ),
            "hsv" | "hsva" | "--hsv" => Self::hsv(
                hue(v[0])?,
                (scaled(v[1], 100.0)? / 100.0).clamp(0.0, 1.0),
                (scaled(v[2], 100.0)? / 100.0).clamp(0.0, 1.0),
                a,
            ),
            "srgb" => Self::from_rgb(
                [scaled(v[0], 1.0)?, scaled(v[1], 1.0)?, scaled(v[2], 1.0)?],
                a,
            ),
            "srgb-linear" => Self::from_rgb(
                [scaled(v[0], 1.0)?, scaled(v[1], 1.0)?, scaled(v[2], 1.0)?].map(gamma),
                a,
            ),
            "display-p3" => Self::from_xyz(
                mul(
                    P3_XYZ,
                    [scaled(v[0], 1.0)?, scaled(v[1], 1.0)?, scaled(v[2], 1.0)?].map(linear),
                ),
                a,
            ),
            "oklab" | "oklch" => {
                let mut c = [
                    scaled(v[0], 1.0)?,
                    scaled(v[1], 0.4)?,
                    if space == "oklch" {
                        hue(v[2])?
                    } else {
                        scaled(v[2], 0.4)?
                    },
                ];
                if space == "oklch" {
                    c = cartesian(c);
                }
                Self::from_rgb(unoklab(c).map(gamma), a)
            }
            "lab" | "lch" => {
                let mut c = [
                    scaled(v[0], 100.0)?,
                    scaled(v[1], if space == "lab" { 125.0 } else { 150.0 })?,
                    if space == "lch" {
                        hue(v[2])?
                    } else {
                        scaled(v[2], 125.0)?
                    },
                ];
                if space == "lch" {
                    c = cartesian(c);
                }
                Self::from_xyz(mul(TO_D65, unlab(c)), a)
            }
            _ => return Err(format!("Unsupported color space: {space}")),
        };
        if !parsed.rgb().iter().all(|v| v.is_finite()) {
            return Err("Color components exceed the supported numeric range".into());
        }
        Ok(parsed)
    }
    fn format(self, format: &str) -> Result<String, String> {
        let rgb = self.rgb().map(|v| v.clamp(0.0, 1.0));
        let a = num(self.a);
        let tail = if self.a < 1.0 {
            format!(" / {a}")
        } else {
            String::new()
        };
        let rgba = rgb.map(|v| num(v * 255.0));
        let pct = rgb.map(|v| format!("{}%", num(v * 100.0)));
        Ok(match format {
            "hex" => self.hex(),
            "hex-lower-case" => self.hex().to_lowercase(),
            "hex-no-prefix" => self.hex()[1..].to_lowercase(),
            "rgb" => format!("rgb({} {} {}{tail})", rgba[0], rgba[1], rgba[2]),
            "rgb-percentage" => format!("rgb({} {} {}{tail})", pct[0], pct[1], pct[2]),
            "rgba" => format!("rgba({}, {}, {}, {a})", rgba[0], rgba[1], rgba[2]),
            "rgba-percentage" => format!("rgba({}, {}, {}, {a})", pct[0], pct[1], pct[2]),
            "hsla" => {
                let [h, s, l] = Self::from_rgb(rgb, self.a).hsl_parts();
                format!(
                    "hsla({}, {}%, {}%, {a})",
                    num(h),
                    num(s * 100.0),
                    num(l * 100.0)
                )
            }
            "hsva" => {
                let [h, s, v] = Self::from_rgb(rgb, self.a).hsv_parts();
                format!(
                    "color(hsv {} {} {}{tail})",
                    num(h),
                    num(s * 100.0),
                    num(v * 100.0)
                )
            }
            "oklch" => {
                let [l, c, h] = polar(oklab(self.rgb().map(linear)));
                format!("oklch({}% {} {}{tail})", num(l * 100.0), num(c), num(h))
            }
            "lch" => {
                let [l, c, h] = polar(lab(
                    mul(TO_D50, self.xyz()),
                    [0.9642956764, 1.0, 0.8251046025],
                ));
                format!("lch({}% {} {}{tail})", num(l), num(c), num(h))
            }
            "p3" => {
                let p = mul(XYZ_P3, self.xyz()).map(gamma).map(num);
                format!("color(display-p3 {} {} {}{tail})", p[0], p[1], p[2])
            }
            _ => return Err(format!("Unknown color format: {format}")),
        })
    }
    fn from_value(v: &Value) -> Result<Self, String> {
        if let Some(s) = v.as_str() {
            return Self::parse(s);
        }
        if let Some(s) = v["hex"].as_str() {
            return Self::parse(s);
        }
        let get = |key: &str| {
            v[key]
                .as_f64()
                .filter(|x| x.is_finite())
                .ok_or_else(|| format!("Missing color channel: {key}"))
        };
        let scale = if v.get("colorSpace").is_some() {
            1.0
        } else {
            255.0
        };
        let a = v["alpha"].as_f64().unwrap_or(1.0);
        if !a.is_finite() {
            return Err("Invalid alpha".into());
        }
        let color = Self::from_rgb(
            [
                get("red")? / scale,
                get("green")? / scale,
                get("blue")? / scale,
            ],
            a.clamp(0.0, 1.0),
        );
        match v["colorSpace"].as_str().unwrap_or("srgb") {
            "srgb" | "sRGB" => Ok(color),
            "p3" | "display-p3" => Ok(Self::from_xyz(
                mul(P3_XYZ, color.rgb().map(linear)),
                color.a,
            )),
            s => Err(format!("Unsupported sampled color space: {s}")),
        }
    }
}

fn num(v: f64) -> String {
    let v = if v.abs() < 0.000005 { 0.0 } else { v };
    format!("{v:.5}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}
fn palettes() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("v2_color_picker_names.json"))
            .expect("bundled color names are valid JSON")
    })
}

fn history(store: &Value) -> Result<Vec<Value>, String> {
    match store.get("history") {
        None | Some(Value::Null) => Ok(vec![]),
        Some(Value::Array(v)) => Ok(v.clone()),
        Some(Value::String(v)) => serde_json::from_str::<Vec<Value>>(v)
            .map_err(|e| format!("Invalid serialized color history: {e}")),
        _ => Err("Color history must be an array".into()),
    }
}
fn key(v: &Value) -> Option<String> {
    Color::from_value(&v["color"]).ok().map(Color::hex)
}
fn add_history(store: &mut Value, color: Color, favorite: Option<bool>) -> Result<(), String> {
    let mut items = history(store)?;
    let hex = color.hex();
    let found = items.iter().position(|v| key(v).as_deref() == Some(&hex));
    let mut item = found.map(|i| items[i].clone()).unwrap_or_else(|| json!({}));
    item["color"] = json!(hex);
    item["date"] = json!(chrono::Utc::now().to_rfc3339());
    if let Some(v) = favorite {
        item["isFavorite"] = json!(v);
    }
    if let Some(i) = found.filter(|i| items[*i]["isFavorite"] == true) {
        items[i] = item;
    } else {
        items.retain(|v| key(v).as_deref() != Some(&hex));
        items.insert(0, item);
    }
    let max = 200_usize.saturating_sub(items.iter().filter(|v| v["isFavorite"] == true).count());
    let mut count = 0;
    items.retain(|v| {
        if v["isFavorite"] == true {
            return true;
        }
        count += 1;
        count <= max
    });
    store["history"] = json!(items);
    Ok(())
}
async fn remember(ctx: &CoreContext, c: Color, favorite: Option<bool>) -> CoreResult {
    sdk::update_store(ctx, SLUG, move |v| add_history(v, c, favorite)).await
}

#[derive(Default, serde::Deserialize)]
struct LegacyColorStore {
    history: Option<Value>,
    #[serde(rename = "lastConvertedColorFormat")]
    last_format: Option<Value>,
}

/// Read only the known Color Picker schema, never preferences or credentials.
/// This runs at first use in the app. Development/tests use temporary fixtures;
/// there is no migration-time shell command or installed-extension mutation.
fn legacy_import(data_dir: &Path) -> CoreResult {
    let roots = [
        data_dir.join("extensions/color-picker"),
        data_dir.join("extensions/packages/color-picker"),
        data_dir.join("extensions/packages/raycast.color-picker"),
        data_dir.join("extension-registry/packages/color-picker"),
        data_dir.join("extension-registry/packages/raycast.color-picker"),
    ];
    let mut imported = vec![];
    let mut format = None;
    for root in roots {
        for relative in [
            ".tezbar-support/cache/shared.json",
            ".tezbar-local-storage.json",
        ] {
            let path = root.join(relative);
            let bytes = match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(format!("Cannot migrate legacy Color Picker storage: {e}")),
            };
            if bytes.len() > 16 * 1024 * 1024 {
                return Err(
                    "Legacy Color Picker storage exceeds 16 MiB; original data was preserved"
                        .into(),
                );
            }
            let legacy: LegacyColorStore = serde_json::from_slice(&bytes).map_err(|_| {
                "Legacy Color Picker storage is malformed; original data was preserved".to_string()
            })?;
            if let Some(v) = legacy
                .last_format
                .and_then(|v| v.as_str().map(str::to_string))
            {
                if FORMATS.iter().any(|(f, _)| *f == v) {
                    format = Some(v);
                }
            }
            let Some(raw) = legacy.history else {
                continue;
            };
            let items = history(&json!({"history":raw}))?;
            for item in items {
                let c = Color::from_value(&item["color"]).map_err(|_| {
                    "Legacy history contains an invalid color; original data was preserved"
                        .to_string()
                })?;
                let hex = c.hex();
                // Copy only the documented item keys. Favorites remain favorites,
                // and source ordering is retained; no synthetic dates are added.
                let mut entry = json!({"color":hex,"isFavorite":item["isFavorite"]==true});
                for k in ["title", "date"] {
                    if let Some(v) = item[k].as_str() {
                        entry[k] = json!(v);
                    }
                }
                if let Some(index) = imported
                    .iter()
                    .position(|v| key(v).as_deref() == Some(&hex))
                {
                    if entry["isFavorite"] == true {
                        imported[index]["isFavorite"] = json!(true);
                    }
                    if imported[index]["title"].as_str().is_none() && entry["title"].is_string() {
                        imported[index]["title"] = entry["title"].clone();
                    }
                } else {
                    imported.push(entry);
                }
            }
        }
    }
    Ok(json!({"history":imported,"lastConvertedColorFormat":format}))
}

async fn migrated_store(ctx: &CoreContext) -> CoreResult {
    let current = sdk::read_store(ctx, SLUG).await?;
    if current["legacyMigrationVersion"].as_u64().unwrap_or(0) >= 1 {
        return Ok(current);
    }
    // A populated Native V2 history is authoritative. A previous empty history
    // may have been initialized by the pilot, so it still needs the first import.
    let data_dir = ctx.paths.data_dir.clone();
    let imported = tokio::task::spawn_blocking(move || legacy_import(&data_dir))
        .await
        .map_err(|e| e.to_string())??;
    sdk::update_store(ctx, SLUG, move |store| {
        if store["legacyMigrationVersion"].as_u64().unwrap_or(0) >= 1 {
            return Ok(());
        }
        let mut items = history(store)?;
        for entry in history(&imported)? {
            if !items.iter().any(|v| key(v) == key(&entry)) {
                items.push(entry);
            }
        }
        if !items.is_empty() || store.get("history").is_some() {
            store["history"] = json!(items);
        }
        if store.get("lastConvertedColorFormat").is_none()
            && imported["lastConvertedColorFormat"].is_string()
        {
            store["lastConvertedColorFormat"] = imported["lastConvertedColorFormat"].clone();
        }
        store["legacyMigrationVersion"] = json!(1);
        Ok(())
    })
    .await
}

struct Ui {
    actions: Vec<Value>,
    map: serde_json::Map<String, Value>,
}
impl Ui {
    fn new() -> Self {
        Self {
            actions: vec![],
            map: serde_json::Map::new(),
        }
    }
    fn action(&mut self, id: impl AsRef<str>, title: impl AsRef<str>, op: Value) -> String {
        let id = format!("color-picker:{}", id.as_ref());
        if !self.map.contains_key(&id) {
            self.actions
                .push(json!({"id":id,"title":title.as_ref(),"kind":"action"}));
            self.map.insert(id.clone(), op);
        }
        id
    }
    fn submit(&mut self, id: &str, title: &str, op: Value) -> String {
        let id = self.action(id, title, op);
        if let Some(v) = self.actions.last_mut() {
            v["kind"] = json!("submit-form");
        }
        id
    }
    fn finish(self, state: &mut Value, title: &str, root: Value) -> Value {
        state["actionMap"] = json!(self.map);
        sdk::view(title, root, self.actions)
    }
    fn colors(
        &mut self,
        c: Color,
        item: Option<&Value>,
        state: &Value,
        format: &str,
        primary_paste: bool,
    ) -> Result<Vec<String>, String> {
        let hex = c.hex();
        let mut ids = vec![];
        let copy = self.action(
            format!("copy:{hex}:{format}"),
            "Copy Color",
            json!({"op":"copy","color":hex,"format":format}),
        );
        let paste = self.action(
            format!("paste:{hex}:{format}"),
            "Paste to Active App",
            json!({"op":"paste","color":hex,"format":format}),
        );
        if primary_paste {
            ids.extend([paste, copy]);
        } else {
            ids.extend([copy, paste]);
        }
        for (f, label) in FORMATS {
            ids.push(self.action(
                format!("copy:{hex}:{f}"),
                format!("Copy as {label}"),
                json!({"op":"copy","color":hex,"format":f}),
            ));
        }
        let favorite = item.is_some_and(|v| v["isFavorite"] == true);
        ids.push(self.action(
            format!("favorite:{hex}"),
            if favorite {
                "Remove from Favorites"
            } else {
                "Add to Favorites"
            },
            json!({"op":"favorite","color":hex,"value":!favorite}),
        ));
        let selected = state["selected"]
            .as_array()
            .is_some_and(|v| v.iter().any(|v| v.as_str() == Some(&hex)));
        ids.push(self.action(
            format!("select:{hex}"),
            if selected {
                "Deselect Color"
            } else {
                "Select Color"
            },
            json!({"op":"select","color":hex}),
        ));
        if item.is_some() {
            ids.push(self.action(
                format!("edit:{hex}"),
                "Edit Title",
                json!({"op":"edit","color":hex}),
            ));
            if favorite {
                for (dir, title) in [("up", "Move Favorite Up"), ("down", "Move Favorite Down")] {
                    ids.push(self.action(
                        format!("{dir}:{hex}"),
                        title,
                        json!({"op":"move","color":hex,"direction":dir}),
                    ));
                }
            }
            ids.push(self.action(
                format!("delete:{hex}"),
                "Delete Color",
                json!({"op":"delete","color":hex}),
            ));
        }
        Ok(ids)
    }
}

fn preferred(payload: &Value) -> &str {
    payload["preferences"]["colorFormat"]
        .as_str()
        .filter(|f| FORMATS.iter().any(|(v, _)| v == f))
        .unwrap_or("hex")
}
fn arg<'a>(payload: &'a Value, key: &str) -> Option<&'a str> {
    payload["argumentValues"][key]
        .as_str()
        .or_else(|| payload["arguments"][key].as_str())
}
fn list(children: Vec<Value>, search: bool) -> Value {
    json!({"type":"List","props":{"__hasServerSearch":search,"searchBarPlaceholder":if search{"Enter a color or search colors…"}else{"Search colors…"}},"children":children})
}
fn row(id: &str, title: &str, subtitle: &str, color: Option<Color>, ids: Vec<String>) -> Value {
    let icon = color.map(|c| json!({"source":"CircleFilled","tintColor":c.hex()}));
    json!({"type":"List.Item","props":{"id":id,"title":title,"subtitle":subtitle,"icon":icon,"actionIds":ids}})
}
fn empty(title: &str, description: &str) -> Value {
    json!({"type":"List.EmptyView","props":{"title":title,"description":description}})
}
fn field(id: &str, title: &str, value: &str) -> Value {
    // FormRuntime.collectFormFields reads props.value, not React defaultValue.
    json!({"type":"Form.TextField","props":{"id":id,"title":title,"value":value}})
}

fn conversion_view(state: &mut Value, store: &Value) -> CoreResult {
    let mut ui = Ui::new();
    let text = state["colorText"].as_str().unwrap_or("");
    let mut rows = vec![];
    match Color::parse(text) {
        Ok(c) => {
            let last = store["lastConvertedColorFormat"].as_str().unwrap_or("");
            let mut formats = FORMATS.to_vec();
            formats.sort_by_key(|(f, _)| *f != last);
            for (f, label) in formats {
                let value = c.format(f)?;
                let id = ui.action(
                    format!("convert:{f}"),
                    "Copy Converted Color",
                    json!({"op":"copy-conversion","color":c.hex(),"value":value,"format":f}),
                );
                rows.push(row(f, label, &value, Some(c), vec![id]));
            }
        }
        Err(e) => rows.push(empty(
            if text.is_empty() {
                "Enter a color"
            } else {
                "Invalid color"
            },
            if text.is_empty() {
                "Type HEX, RGB, HSL, HSV, Lab, OKLCH or Display P3."
            } else {
                &e
            },
        )),
    }
    ui.action("enter-convert", "Enter Color", json!({"op":"convert-form"}));
    let wheel = state["wheelColor"].is_string();
    if wheel {
        ui.action(
            "edit-wheel",
            "Adjust Hue, Saturation and Brightness",
            json!({"op":"wheel-form"}),
        );
    }
    Ok(ui.finish(
        state,
        if wheel {
            "Color Wheel"
        } else {
            "Convert Color"
        },
        list(rows, true),
    ))
}

fn names_view(state: &mut Value, payload: &Value) -> CoreResult {
    let mut ui = Ui::new();
    let text = state["colorText"].as_str().unwrap_or("");
    let mut children = vec![];
    let proximity = state["nameSort"] == "proximity";
    ui.action(
        "sort-names",
        if proximity {
            "Group by Palette"
        } else {
            "Sort by Proximity"
        },
        json!({"op":"sort-names"}),
    );
    if let Ok(c) = Color::parse(text) {
        let target = naming_lab(c);
        let mut groups: Vec<_> = palettes()
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, rows)| {
                let mut rows: Vec<_> = rows
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|v| {
                        let other = Color::parse(v["hex"].as_str()?).ok()?;
                        let lab = naming_lab(other);
                        let distance = target
                            .iter()
                            .zip(lab)
                            .map(|(a, b)| (a - b).powi(2))
                            .sum::<f64>()
                            .sqrt();
                        Some((v, distance, other))
                    })
                    .collect();
                rows.sort_by(|a, b| a.1.total_cmp(&b.1));
                (name, rows)
            })
            .collect();
        groups.sort_by(|a, b| {
            let a = a.1.first().map(|v| v.1).unwrap_or(f64::MAX);
            let b = b.1.first().map(|v| v.1).unwrap_or(f64::MAX);
            a.total_cmp(&b)
        });
        let count = payload["preferences"]["colorNamesPerGroup"]
            .as_str()
            .unwrap_or("5")
            .parse::<usize>()
            .ok()
            .filter(|v| [5, 10, 25].contains(v))
            .unwrap_or_else(|| {
                if payload["preferences"]["colorNamesPerGroup"] == "Infinity" {
                    usize::MAX
                } else {
                    5
                }
            });
        let page = state["nameLimit"].as_u64().unwrap_or(250) as usize;
        let mut build = |palette: &str, v: &Value, d: f64, c: Color| {
            let name = v["name"].as_str().unwrap_or("");
            let hex = v["hex"].as_str().unwrap_or("");
            let id = format!("name:{palette}:{hex}:{name}");
            let copy = ui.action(&id, "Copy Name", json!({"op":"copy-text","text":name}));
            let hex_id = ui.action(
                format!("name-hex:{palette}:{hex}:{name}"),
                "Copy Hex",
                json!({"op":"copy-text","text":hex}),
            );
            row(
                &id,
                name,
                &format!("{hex} · ΔE {}", num(d)),
                Some(c),
                vec![copy, hex_id],
            )
        };
        if proximity {
            let mut all: Vec<_> = groups
                .iter()
                .flat_map(|(g, rows)| rows.iter().map(move |v| (*g, v)))
                .collect();
            all.sort_by(|a, b| a.1 .1.total_cmp(&b.1 .1));
            let mut seen = HashSet::new();
            for (g, (v, d, c)) in all
                .into_iter()
                .filter(|(_, v)| seen.insert(v.0["name"].as_str().unwrap_or("").to_lowercase()))
                .take(page)
            {
                children.push(build(g, v, *d, *c));
            }
        } else {
            for (g, rows) in groups {
                let rows = rows
                    .into_iter()
                    .take(count.min(page))
                    .map(|(v, d, c)| build(g, v, d, c))
                    .collect::<Vec<_>>();
                children.push(json!({"type":"List.Section","props":{"title":g},"children":rows}));
            }
        }
    } else {
        children.push(empty(
            "Enter a color to see its names",
            "Search HEX (#00FF00), CSS names, RGB or HSL.",
        ));
    }
    Ok(ui.finish(state, "Color Names", list(children, true)))
}

fn naming_lab(c: Color) -> [f64; 3] {
    // Match chroma-js 1.x constants, rather than D50 CSS Lab.
    let v = mul(
        [
            [0.4124564, 0.3575761, 0.1804375],
            [0.2126729, 0.7151522, 0.0721750],
            [0.0193339, 0.1191920, 0.9503041],
        ],
        c.rgb().map(linear),
    );
    lab(v, [0.95047, 1.0, 1.08883])
}

fn collection_view(state: &mut Value, store: &Value, payload: &Value, command: &str) -> CoreResult {
    let mut ui = Ui::new();
    let mut rows = vec![];
    let format = preferred(payload);
    let query = state["query"].as_str().unwrap_or("").to_lowercase();
    let stored = matches!(command, "organize-colors" | "favorite-colors");
    let items = if stored {
        history(store)?
    } else {
        state["colors"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|color| json!({"color":color}))
            .collect()
    };
    let mut sections = [vec![], vec![]];
    let mut all = vec![];
    for (i, item) in items.iter().enumerate() {
        if command == "favorite-colors" && item["isFavorite"] != true {
            continue;
        }
        let c = Color::from_value(&item["color"])?;
        let hex = c.hex();
        all.push(json!(hex));
        let label = c.format(format)?;
        let title = item["title"].as_str().unwrap_or("");
        if !query.is_empty() && !format!("{label} {title}").to_lowercase().contains(&query) {
            continue;
        }
        let ids = ui.colors(
            c,
            if stored { Some(item) } else { None },
            state,
            format,
            payload["preferences"]["primaryAction"] == "paste",
        )?;
        let selected = state["selected"]
            .as_array()
            .is_some_and(|v| v.contains(&json!(hex)));
        let title = format!(
            "{}{label}{}",
            if selected { "✓ " } else { "" },
            if title.is_empty() {
                String::new()
            } else {
                format!(" · {title}")
            }
        );
        let node = if command == "extract-color" || command == "generate-colors" {
            json!({"type":"Grid.Item","props":{"id":hex,"title":title,"content":{"color":c.hex()},"actionIds":ids}})
        } else {
            row(
                &format!("color:{i}:{hex}"),
                &title,
                item["date"].as_str().unwrap_or(""),
                Some(c),
                ids,
            )
        };
        if stored {
            sections[if item["isFavorite"] == true { 0 } else { 1 }].push(node);
        } else {
            rows.push(node);
        }
    }
    state["allColors"] = json!(all);
    ui.action("add-color", "Add Color", json!({"op":"add-form"}));
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        ui.action("pick", "Pick Color", json!({"op":"pick"}));
    }
    ui.action(
        "select-all",
        "Select All Colors",
        json!({"op":"select-all"}),
    );
    ui.action(
        "clear-selection",
        "Clear Selection",
        json!({"op":"clear-selection"}),
    );
    if state["selected"].as_array().is_some_and(|v| !v.is_empty()) {
        for (fmt, title) in [
            ("text", "Copy Selected Colors"),
            ("json", "Copy Colors as JSON"),
            ("css-classes", "Copy Colors as CSS Classes"),
            ("css-variables", "Copy Colors as CSS Variables"),
        ] {
            ui.action(
                format!("export:{fmt}"),
                title,
                json!({"op":"export","format":fmt}),
            );
        }
    }
    if stored {
        ui.action(
            "clear-history",
            "Delete All Colors…",
            json!({"op":"clear-confirm"}),
        );
    }
    if stored {
        for (index, title) in [(0, "Favorites"), (1, "History")] {
            if !sections[index].is_empty() {
                rows.push(json!({"type":"List.Section","props":{"title":title},"children":sections[index]}));
            }
        }
    }
    let title = match command {
        "organize-colors" => "Organize Colors",
        "favorite-colors" => "Favorite Colors",
        "extract-color" => "Extracted Colors",
        _ => "Generate Colors",
    };
    let root = if command == "extract-color" || command == "generate-colors" {
        if rows.is_empty() {
            rows.push(json!({"type":"Grid.EmptyView","props":{"title":"No colors found","description":"No palette was returned."}}));
        }
        json!({"type":"Grid","props":{"columns":5},"children":rows})
    } else {
        if rows.is_empty() {
            rows.push(empty(
                "No colors yet",
                "Pick a desktop color or add a color to your collection.",
            ));
        }
        list(rows, true)
    };
    Ok(ui.finish(state, title, root))
}

fn form_view(state: &mut Value, kind: &str, color: Option<&str>, store: &Value) -> CoreResult {
    let mut ui = Ui::new();
    let (title, fields, submit) = match kind {
        "add" => (
            "Add Color",
            vec![field("color", "Color", ""), field("title", "Title", "")],
            ui.submit("add-submit", "Save Color", json!({"op":"add-submit"})),
        ),
        "convert" => (
            "Convert Color",
            vec![field(
                "color",
                "Color",
                state["colorText"].as_str().unwrap_or(""),
            )],
            ui.submit("convert-submit", "Convert", json!({"op":"convert-submit"})),
        ),
        "edit" => {
            let color = color.ok_or("No color selected")?;
            let items = history(store)?;
            let title = items
                .iter()
                .find(|v| key(v).as_deref() == Some(color))
                .and_then(|v| v["title"].as_str())
                .unwrap_or("");
            (
                "Edit Color Title",
                vec![field("title", "Title", title)],
                ui.submit(
                    "edit-submit",
                    "Save Title",
                    json!({"op":"edit-submit","color":color}),
                ),
            )
        }
        "generate" => (
            "Generate Colors",
            vec![field("prompt", "Describe your palette", "")],
            ui.submit(
                "generate-submit",
                "Generate Colors",
                json!({"op":"generate-submit"}),
            ),
        ),
        "extract" => (
            "Extract Colors from Image",
            vec![
                json!({"type":"Form.FilePicker","props":{"id":"image","title":"Image","allowMultipleSelection":false,"canChooseDirectories":false}}),
                field("path", "Full Image Path (if needed)", ""),
            ],
            ui.submit(
                "extract-submit",
                "Extract Colors",
                json!({"op":"extract-submit"}),
            ),
        ),
        "wheel" => {
            let c = wheel_color(state)?;
            let [h, s, v] = c.hsv_parts();
            (
                "Color Wheel",
                vec![
                    field("hue", "Hue (degrees)", &num(h)),
                    field("saturation", "Saturation (%)", &num(s * 100.0)),
                    field("value", "Brightness (%)", &num(v * 100.0)),
                    field("alpha", "Opacity (%)", &num(c.a * 100.0)),
                ],
                ui.submit("wheel-submit", "Use Color", json!({"op":"wheel-submit"})),
            )
        }
        "clear" => (
            "Delete All Colors",
            vec![
                json!({"type":"Form.Description","props":{"text":"This removes all favorites and history. Type DELETE to confirm."}}),
                field("confirmation", "Confirmation", ""),
            ],
            ui.submit(
                "clear-submit",
                "Delete All Colors",
                json!({"op":"clear-submit"}),
            ),
        ),
        _ => return Err("Unknown color form".into()),
    };
    state["form"] = json!(kind);
    state["formColor"] = json!(color);
    let mut root = json!({"type":"Form","children":fields});
    if kind == "wheel" {
        let c = wheel_color(state)?;
        let [h, s, v] = c.hsv_parts();
        let change = ui.submit(
            "wheel-change",
            "Update Color Wheel",
            json!({"op":"wheel-change"}),
        );
        let mut formats = vec![];
        for (f, title) in FORMATS {
            let value = c.format(f)?;
            let id = ui.action(
                format!("wheel-copy:{f}"),
                format!("Copy as {title}"),
                json!({"op":"wheel-copy","format":f}),
            );
            formats.push(json!({"key":f,"title":title,"value":value,"actionId":id}));
        }
        // Renderer hook: send only HSV coordinates through changeActionId and
        // formValues. Rust returns conversion strings and executes copy/history.
        // The existing numeric Form remains usable before the wheel is wired.
        root["props"] = json!({"nativeColorWheel":{"hue":h,"saturation":s*100.0,"value":v*100.0,"alpha":c.a*100.0,"preview":c.hex(),"formats":formats,"changeActionId":change,"saveActionId":submit},"backActionId":format!("{}:back",SLUG)});
    }
    ui.action("back", "Back", json!({"op":"back"}));
    Ok(ui.finish(state, title, root))
}

fn wheel_color(state: &Value) -> Result<Color, String> {
    if let Some(v) = state["wheelCoordinates"]
        .as_array()
        .filter(|v| v.len() == 4)
    {
        let n = v
            .iter()
            .map(|v| {
                v.as_f64()
                    .ok_or("Invalid color wheel coordinates".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Color::hsv(n[0], n[1] / 100.0, n[2] / 100.0, n[3] / 100.0))
    } else {
        Color::parse(state["wheelColor"].as_str().unwrap_or("#FF0000"))
    }
}

fn generated_colors(response: &Value) -> Result<Vec<Value>, String> {
    let text = response.as_str().ok_or("AI service did not return text")?;
    let colors: Vec<Value> = serde_json::from_str(text)
        .map_err(|e| format!("AI did not return a valid JSON color array: {e}"))?;
    if colors.len() > 256 {
        return Err("AI returned more than 256 colors".into());
    }
    let mut result = vec![];
    for color in colors {
        let text = color.as_str().ok_or("AI returned a non-string color")?;
        if !text.starts_with('#') {
            return Err("AI returned a color without a HEX prefix".into());
        }
        let parsed = Color::parse(text)?;
        result.push(json!(parsed.hex()));
    }
    Ok(result)
}
async fn generate(ctx: &CoreContext, prompt: &str) -> Result<Vec<Value>, String> {
    if prompt.trim().is_empty() {
        return Err("Describe the palette to generate".into());
    }
    if prompt.len() > 16000 {
        return Err("Palette prompt is too long".into());
    }
    let prompt=format!("Generate colors based on the following prompt. Return ONLY a parsable minified JSON array of HEX color strings, with no markdown. Return [] if it is not possible to generate colors.\nPrompt: {prompt}\nJSON colors:");
    let response = ctx
        .request(
            "core:ai:extension-ask",
            vec![json!(prompt), json!({"stream":false})],
        )
        .await?;
    generated_colors(&response)
}

/// Deterministic weighted median-cut palette; decode only a bounded local file.
fn extract_image(path: &Path) -> Result<Vec<Value>, String> {
    let metadata = std::fs::metadata(path).map_err(|e| format!("Cannot read image: {e}"))?;
    if !metadata.is_file() || metadata.len() > 32 * 1024 * 1024 {
        return Err("Choose an image file smaller than 32 MiB".into());
    }
    let mut reader = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|e| format!("Cannot decode image: {e}"))?;
    // Upscaling a small image introduces interpolated colors that were never
    // present in its pixels. Only downsample images above the sample budget.
    let image = if image.width() > 160 || image.height() > 160 {
        image.thumbnail(160, 160)
    } else {
        image
    }.to_rgba8();
    palette_from_pixels(image.pixels().map(|p| p.0))
}
fn palette_from_pixels(pixels: impl Iterator<Item = [u8; 4]>) -> Result<Vec<Value>, String> {
    let mut histogram: BTreeMap<[u8; 3], u32> = BTreeMap::new();
    for [r, g, b, a] in pixels {
        if a >= 128 {
            *histogram.entry([r, g, b]).or_default() += 1;
        }
    }
    if histogram.is_empty() {
        return Ok(vec![]);
    }
    let mut buckets = vec![histogram.into_iter().collect::<Vec<_>>()];
    let range = |b: &Vec<([u8; 3], u32)>| {
        let mut min = [255; 3];
        let mut max = [0; 3];
        for (c, _) in b {
            for i in 0..3 {
                min[i] = min[i].min(c[i]);
                max[i] = max[i].max(c[i]);
            }
        }
        std::array::from_fn::<_, 3, _>(|i| max[i] - min[i])
    };
    while buckets.len() < 40 {
        let Some((index, _)) = buckets
            .iter()
            .enumerate()
            .filter(|(_, v)| v.len() > 1)
            .map(|(i, v)| {
                (
                    i,
                    *range(v).iter().max().unwrap() as u64
                        * v.iter().map(|(_, n)| *n as u64).sum::<u64>(),
                )
            })
            .max_by_key(|(_, score)| *score)
        else {
            break;
        };
        let mut bucket = buckets.remove(index);
        let ranges = range(&bucket);
        let axis = (0..3).max_by_key(|i| ranges[*i]).unwrap();
        bucket.sort_by_key(|(c, _)| c[axis]);
        let total = bucket.iter().map(|(_, n)| *n as u64).sum::<u64>();
        let mut cumulative = 0;
        let mut split = 1;
        for (i, (_, n)) in bucket.iter().enumerate().take(bucket.len() - 1) {
            cumulative += *n as u64;
            split = i + 1;
            if cumulative * 2 >= total {
                break;
            }
        }
        let other = bucket.split_off(split);
        buckets.push(bucket);
        buckets.push(other);
    }
    buckets.sort_by_key(|b| std::cmp::Reverse(b.iter().map(|(_, n)| *n as u64).sum::<u64>()));
    let mut seen = HashSet::new();
    let mut out = vec![];
    for b in buckets {
        let n = b.iter().map(|(_, n)| *n as f64).sum::<f64>();
        let v = std::array::from_fn(|i| {
            b.iter()
                .map(|(c, n)| c[i] as f64 * (*n as f64))
                .sum::<f64>()
                / n
                / 255.0
        });
        let hex = Color::from_rgb(v, 1.0).hex();
        if seen.insert(hex.clone()) {
            out.push(json!(hex));
        }
    }
    Ok(out)
}
async fn selected_image() -> Result<Option<PathBuf>, String> {
    #[cfg(target_os = "macos")]
    let output = tokio::process::Command::new("/usr/bin/osascript")
        .args([
            "-e",
            r#"tell application "Finder"
if (count of selection) = 0 then return ""
return POSIX path of (item 1 of (get selection) as alias)
end tell"#,
        ])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    #[cfg(target_os="windows")]
    let output=tokio::process::Command::new("powershell.exe").args(["-NoLogo","-NoProfile","-NonInteractive","-Command",r#"[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false); $w=(New-Object -ComObject Shell.Application).Windows() | Where-Object { $_.FullName -match 'explorer.exe$' } | Select-Object -First 1; if($null-ne $w){$w.Document.SelectedItems() | Select-Object -First 1 | ForEach-Object {$_.Path}}"#]).output().await.map_err(|e|e.to_string())?;
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        if !output.status.success() {
            return Err(format!(
                "Cannot read selected image: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let text = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
        let text = text.trim();
        Ok(if text.is_empty() {
            None
        } else {
            Some(PathBuf::from(text))
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(None)
    }
}
fn helper_path(ctx: &CoreContext) -> Result<PathBuf, String> {
    let name = if cfg!(target_os = "windows") {
        "windows.ps1"
    } else {
        "color-picker-helper"
    };
    let candidates = [
        ctx.paths
            .resource_dir
            .join("native/color-picker")
            .join(name),
        ctx.paths
            .resource_dir
            .join("_up_/native/color-picker")
            .join(name),
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../native/color-picker")
            .join(name),
    ];
    candidates.into_iter().find(|p|p.is_file()).ok_or_else(||format!("Native color sampler is missing ({name}); reinstall Tezbar with native/color-picker resources"))
}
async fn pick_native(ctx: &CoreContext) -> Result<Option<Color>, String> {
    if !cfg!(any(target_os = "macos", target_os = "windows")) {
        return Err("Desktop sampling is available on macOS and Windows".into());
    }
    let path = helper_path(ctx)?;
    ctx.request("core:platform:suppress-blur", vec![json!(true)])
        .await?;
    if let Err(e) = ctx
        .request("core:platform:visibility", vec![json!(false)])
        .await
    {
        let _ = ctx
            .request("core:platform:suppress-blur", vec![json!(false)])
            .await;
        return Err(e);
    }
    let output = if cfg!(target_os = "windows") {
        tokio::process::Command::new("powershell.exe")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Sta",
                "-File",
            ])
            .arg(path)
            .kill_on_drop(true)
            .output()
            .await
    } else {
        tokio::process::Command::new(path)
            .kill_on_drop(true)
            .output()
            .await
    };
    // Always release blur suppression, including a cancelled native picker.
    let released = ctx
        .request("core:platform:suppress-blur", vec![json!(false)])
        .await;
    let output = output.map_err(|e| format!("Cannot launch native color sampler: {e}"))?;
    released?;
    if !output.status.success() {
        return Err(format!(
            "Native color sampler failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Invalid native color sampler response: {e}"))?;
    if value.is_null() {
        Ok(None)
    } else {
        Color::from_value(&value).map(Some)
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
const TRAY_ID: &str = "native-v2-color-picker";
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn tray_event(
    ctx: CoreContext,
    id: String,
    format: String,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    Box::pin(async move {
        let result: CoreResult = async {
            let current = sdk::read_store(&ctx, SLUG).await?;
            let format = current["trayColorFormat"]
                .as_str()
                .unwrap_or(&format)
                .to_string();
            if id == "color-picker:tray-pick" {
                if let Some(c) = pick_native(&ctx).await? {
                    sdk::copy(&ctx, &c.format(&format)?).await?;
                    remember(&ctx, c, None).await?;
                }
            } else if id == "color-picker:tray-configure" {
                ctx.request(
                    "core:platform:open-settings",
                    vec![json!("settings-extensions")],
                )
                .await?;
            } else if id == "color-picker:tray-hide" {
                sdk::update_store(&ctx, SLUG, |v| {
                    v["trayEnabled"] = json!(false);
                    Ok(())
                })
                .await?;
                let app = ctx
                    .app
                    .clone()
                    .ok_or("Desktop application is unavailable")?;
                let other = app.clone();
                app.run_on_main_thread(move || {
                    if let Some(tray) = other.tray_by_id(TRAY_ID) {
                        let _ = tray.set_visible(false);
                    }
                })
                .map_err(|e| e.to_string())?;
                return Ok(sdk::no_view("Color Picker menu hidden"));
            } else if let Some(hex) = id.strip_prefix("color-picker:tray-copy:") {
                let c = Color::parse(hex)?;
                sdk::copy(&ctx, &c.format(&format)?).await?;
            } else if let Some(hex) = id.strip_prefix("color-picker:tray-delete:") {
                let hex = Color::parse(hex)?.hex();
                sdk::update_store(&ctx, SLUG, move |store| {
                    let mut items = history(store)?;
                    items.retain(|v| key(v).as_deref() != Some(&hex));
                    store["history"] = json!(items);
                    Ok(())
                })
                .await?;
            } else {
                return Ok(sdk::unchanged());
            }
            let store = sdk::read_store(&ctx, SLUG).await?;
            configure_tray(&ctx, &store, &format).await
        }
        .await;
        if let Err(error) = result {
            log::warn!("Color Picker tray: {error}");
            ctx.emit(
                "extension:effect",
                json!({"kind":"toast","style":"failure","title":"Color Picker","message":error}),
            );
        }
    })
}
#[cfg(any(target_os = "macos", target_os = "windows"))]
async fn configure_tray(ctx: &CoreContext, store: &Value, format: &str) -> CoreResult {
    let app = ctx
        .app
        .clone()
        .ok_or("The Color Picker menu needs a running desktop application")?;
    let items = history(store)?;
    let visible = store["trayEnabled"] != false;
    let saved_format = format.to_string();
    sdk::update_store(ctx, SLUG, move |v| {
        v["trayColorFormat"] = json!(saved_format);
        Ok(())
    })
    .await?;
    let ctx = ctx.clone();
    let format = format.to_string();
    let app_for_ui = app.clone();
    let (send, receive) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let result = (|| -> Result<(), String> {
            use tauri::menu::{MenuBuilder, SubmenuBuilder};
            use tauri::tray::TrayIconBuilder;
            let mut menu =
                MenuBuilder::new(&app_for_ui).text("color-picker:tray-pick", "Pick Color");
            for (favorites, title) in [(true, "Favorites"), (false, "Recent Colors")] {
                let mut submenu = SubmenuBuilder::new(&app_for_ui, title);
                let mut count = 0;
                for item in items
                    .iter()
                    .filter(|v| (v["isFavorite"] == true) == favorites)
                    .take(9)
                {
                    let c = Color::from_value(&item["color"])?;
                    let name = item["title"].as_str().unwrap_or("");
                    let label = format!(
                        "{}{}",
                        c.format(&format)?,
                        if name.is_empty() {
                            String::new()
                        } else {
                            format!(" · {name}")
                        }
                    );
                    submenu = submenu.text(format!("color-picker:tray-copy:{}", c.hex()), label);
                    count += 1;
                }
                if count > 0 {
                    let submenu = submenu.build().map_err(|e| e.to_string())?;
                    menu = menu.item(&submenu);
                }
            }
            let mut remove = SubmenuBuilder::new(&app_for_ui, "Delete Recent Color");
            let mut count = 0;
            for item in items.iter().filter(|v| v["isFavorite"] != true).take(9) {
                let c = Color::from_value(&item["color"])?;
                remove = remove.text(
                    format!("color-picker:tray-delete:{}", c.hex()),
                    c.format(&format)?,
                );
                count += 1;
            }
            if count > 0 {
                menu = menu.item(&remove.build().map_err(|e| e.to_string())?);
            }
            let menu = menu
                .separator()
                .text("color-picker:tray-configure", "Configure Color Picker")
                .text("color-picker:tray-hide", "Hide Color Picker Menu")
                .build()
                .map_err(|e| e.to_string())?;
            if let Some(tray) = app_for_ui.tray_by_id(TRAY_ID) {
                tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
                tray.set_visible(visible).map_err(|e| e.to_string())?;
            } else {
                // A small neutral eyedropper silhouette, drawn locally; no image downloads.
                let mut pixels = vec![0u8; 18 * 18 * 4];
                for y in 2..16 {
                    for x in 2..16 {
                        if ((x as i32 - y as i32).abs() <= 2 && (5..14).contains(&x))
                            || ((x + y > 20) && (x + y < 27))
                        {
                            let i = (y * 18 + x) * 4;
                            pixels[i..i + 4].copy_from_slice(&[80, 80, 80, 255]);
                        }
                    }
                }
                let callback_ctx = ctx.clone();
                let callback_format = format.clone();
                TrayIconBuilder::with_id(TRAY_ID)
                    .icon(tauri::image::Image::new_owned(pixels, 18, 18))
                    .icon_as_template(true)
                    .tooltip("Color Picker")
                    .menu(&menu)
                    .on_menu_event(move |_, event| {
                        let id = event.id.as_ref();
                        if id.starts_with("color-picker:tray-") {
                            tauri::async_runtime::spawn(tray_event(
                                callback_ctx.clone(),
                                id.to_string(),
                                callback_format.clone(),
                            ));
                        }
                    })
                    .build(&app_for_ui)
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        let _ = send.send(result);
    })
    .map_err(|e| e.to_string())?;
    receive
        .await
        .map_err(|_| "Color Picker tray creation was interrupted".to_string())??;
    Ok(sdk::no_view("Color Picker menu is active"))
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
async fn configure_tray(_: &CoreContext, _: &Value, _: &str) -> CoreResult {
    Err("The native Color Picker menu is available on macOS and Windows".into())
}

async fn refresh_active_tray(ctx: &CoreContext, store: &Value, format: &str) -> Result<(), String> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    if ctx
        .app
        .as_ref()
        .is_some_and(|app| app.tray_by_id(TRAY_ID).is_some())
    {
        configure_tray(ctx, store, format).await?;
    }
    let _ = (ctx, store, format);
    Ok(())
}

pub async fn handle(
    ctx: &CoreContext,
    command: &str,
    operation: &str,
    payload: &Value,
    state: &mut Value,
) -> CoreResult {
    if !commands().contains(&command) {
        return Err(format!("Unsupported Color Picker command: {command}"));
    }
    if !state.is_object() {
        return Err("Invalid Color Picker session state".into());
    }
    if operation == "extension:dispose-session" {
        *state = json!({});
        return Ok(sdk::unchanged());
    }
    if ![
        "extension:run-command",
        "extension:invoke-action",
        "extension:search-text-changed",
        "extension:refresh-session",
        "extension:load-more",
    ]
    .contains(&operation)
    {
        return Err(format!("Unsupported Color Picker operation: {operation}"));
    }
    let mut store = migrated_store(ctx).await?;
    if operation == "extension:run-command" {
        *state = json!({"selected":[],"query":"","nameLimit":250});
        match command {
            "pick-color" => return pick_and_copy(ctx, payload, &store).await,
            "menu-bar" => {
                store = sdk::update_store(ctx, SLUG, |v| {
                    v["trayEnabled"] = json!(true);
                    Ok(())
                })
                .await?;
                return configure_tray(ctx, &store, preferred(payload)).await;
            }
            "color-wheel" => return form_view(state, "wheel", None, &store),
            "convert-color" => {
                let text = if let Some(v) = arg(payload, "text").filter(|s| !s.trim().is_empty()) {
                    v.to_string()
                } else {
                    ctx.request("core:platform:selected-text", vec![])
                        .await
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_string))
                        .unwrap_or_default()
                };
                state["colorText"] = json!(text);
            }
            "generate-colors" => {
                if let Some(prompt) = arg(payload, "prompt").filter(|s| !s.trim().is_empty()) {
                    state["colors"] = json!(generate(ctx, prompt).await?);
                } else {
                    return form_view(state, "generate", None, &store);
                }
            }
            "extract-color" => {
                let path = if let Some(path) = arg(payload, "path").filter(|s| !s.is_empty()) {
                    Some(PathBuf::from(path))
                } else {
                    selected_image().await?
                };
                if let Some(path) = path {
                    state["colors"] =
                        json!(tokio::task::spawn_blocking(move || extract_image(&path))
                            .await
                            .map_err(|e| e.to_string())??);
                } else {
                    return form_view(state, "extract", None, &store);
                }
            }
            _ => {}
        }
    } else if operation == "extension:search-text-changed" {
        let text = payload["searchText"]
            .as_str()
            .ok_or("Color search text must be a string")?;
        if text.len() > 4096 {
            return Err("Color search is too long".into());
        }
        if matches!(command, "color-names" | "convert-color") {
            state["colorText"] = json!(text);
        } else {
            state["query"] = json!(text);
        }
        state["nameLimit"] = json!(250);
    } else if operation == "extension:load-more" {
        if command != "color-names" {
            return Ok(sdk::unchanged());
        }
        state["nameLimit"] = json!((state["nameLimit"].as_u64().unwrap_or(250) + 250).min(5000));
    } else if operation == "extension:invoke-action" {
        let id = payload["actionId"]
            .as_str()
            .ok_or("Color action ID is required")?;
        let action = state["actionMap"]
            .get(id)
            .cloned()
            .ok_or("Unknown or stale Color Picker action")?;
        let op = action["op"].as_str().ok_or("Invalid Color Picker action")?;
        let color = || Color::parse(action["color"].as_str().ok_or("Action has no color")?);
        let form = &payload["formValues"];
        match op {
            "copy-text" => {
                sdk::copy(ctx, action["text"].as_str().ok_or("No text to copy")?).await?;
                return Ok(sdk::no_view("Copied to clipboard"));
            }
            "copy-conversion" => {
                let value = action["value"].as_str().ok_or("No converted color")?;
                sdk::copy(ctx, value).await?;
                let format = action["format"].clone();
                sdk::update_store(ctx, SLUG, move |s| {
                    s["lastConvertedColorFormat"] = format;
                    Ok(())
                })
                .await?;
                return Ok(sdk::no_view("Copied converted color"));
            }
            "copy" | "paste" => {
                let c = color()?;
                let text = c.format(action["format"].as_str().unwrap_or(preferred(payload)))?;
                if op == "paste" {
                    ctx.request("core:platform:paste-text", vec![json!(text)])
                        .await?;
                } else {
                    sdk::copy(ctx, &text).await?;
                }
                store = remember(ctx, c, None).await?;
                refresh_active_tray(ctx, &store, preferred(payload)).await?;
                return Ok(sdk::no_view(if op == "paste" {
                    "Pasted color"
                } else {
                    "Copied color"
                }));
            }
            "pick" => {
                if let Some(c) = pick_native(ctx).await? {
                    sdk::copy(ctx, &c.format(preferred(payload))?).await?;
                    store = remember(ctx, c, None).await?;
                }
                ctx.request("core:platform:visibility", vec![json!(true)])
                    .await?;
            }
            "favorite" => {
                store = remember(ctx, color()?, Some(action["value"] == true)).await?;
            }
            "delete" | "move" | "edit-submit" => {
                let hex = color()?.hex();
                let op = op.to_string();
                let direction = action["direction"].as_str().unwrap_or("").to_string();
                let title = form["title"].as_str().unwrap_or("").to_string();
                if title.len() > 1024 {
                    return Err("Color title must be at most 1024 bytes".into());
                }
                store = sdk::update_store(ctx, SLUG, move |store| {
                    let mut items = history(store)?;
                    let i = items
                        .iter()
                        .position(|v| key(v).as_deref() == Some(&hex))
                        .ok_or("Color no longer exists in history")?;
                    match op.as_str() {
                        "delete" => {
                            items.remove(i);
                        }
                        "edit-submit" => {
                            items[i]["title"] = json!(title);
                        }
                        _ => {
                            let favorites: Vec<_> = items
                                .iter()
                                .enumerate()
                                .filter(|(_, v)| v["isFavorite"] == true)
                                .map(|(i, _)| i)
                                .collect();
                            let p = favorites
                                .iter()
                                .position(|index| *index == i)
                                .ok_or("Only favorites can be reordered")?;
                            let dest = if direction == "up" {
                                p.checked_sub(1)
                            } else {
                                Some(p + 1)
                            };
                            if let Some(j) = dest.and_then(|j| favorites.get(j)) {
                                items.swap(i, *j);
                            }
                        }
                    }
                    store["history"] = json!(items);
                    Ok(())
                })
                .await?;
                state["form"] = Value::Null;
            }
            "edit" => return form_view(state, "edit", action["color"].as_str(), &store),
            "add-form" => return form_view(state, "add", None, &store),
            "convert-form" => return form_view(state, "convert", None, &store),
            "wheel-form" => return form_view(state, "wheel", None, &store),
            "convert-submit" => {
                let c = Color::parse(form["color"].as_str().ok_or("Enter a color")?)?;
                state["colorText"] = json!(c.format("rgb")?);
                state["form"] = Value::Null;
            }
            "add-submit" => {
                let c = Color::parse(form["color"].as_str().ok_or("Enter a color")?)?;
                let title = form["title"].as_str().unwrap_or("").to_string();
                if title.len() > 1024 {
                    return Err("Color title must be at most 1024 bytes".into());
                }
                store = sdk::update_store(ctx, SLUG, move |s| {
                    add_history(s, c, None)?;
                    if let Some(v) = s["history"]
                        .as_array_mut()
                        .and_then(|v| v.iter_mut().find(|v| key(v).as_deref() == Some(&c.hex())))
                    {
                        v["title"] = json!(title);
                    }
                    Ok(())
                })
                .await?;
                state["form"] = Value::Null;
            }
            "wheel-copy" => {
                let c = wheel_color(state)?;
                let text = c.format(action["format"].as_str().unwrap_or(preferred(payload)))?;
                sdk::copy(ctx, &text).await?;
                store = remember(ctx, c, None).await?;
                refresh_active_tray(ctx, &store, preferred(payload)).await?;
                return Ok(sdk::no_view("Copied color"));
            }
            "wheel-submit" | "wheel-change" => {
                let component = |key: &str, min: f64, max: f64| -> Result<f64, String> {
                    let n = if let Some(n) = form[key].as_f64().filter(|n| n.is_finite()) {
                        n
                    } else {
                        number(form[key].as_str().ok_or_else(|| format!("Enter {key}"))?)?
                    };
                    if n < min || n > max {
                        return Err(format!("{key} must be between {min} and {max}"));
                    }
                    Ok(n)
                };
                let c = Color::hsv(
                    component("hue", 0.0, 360.0)?,
                    component("saturation", 0.0, 100.0)? / 100.0,
                    component("value", 0.0, 100.0)? / 100.0,
                    component("alpha", 0.0, 100.0)? / 100.0,
                );
                state["wheelCoordinates"] = json!([
                    component("hue", 0.0, 360.0)?,
                    component("saturation", 0.0, 100.0)?,
                    component("value", 0.0, 100.0)?,
                    component("alpha", 0.0, 100.0)?
                ]);
                state["wheelColor"] = json!(c.hex());
                state["colorText"] = json!(c.format("rgb")?);
                if op == "wheel-change" {
                    return form_view(state, "wheel", None, &store);
                }
                store = remember(ctx, c, None).await?;
                state["form"] = Value::Null;
            }
            "generate-submit" => {
                state["colors"] = json!(
                    generate(
                        ctx,
                        form["prompt"].as_str().ok_or("Enter a palette prompt")?
                    )
                    .await?
                );
                state["form"] = Value::Null;
            }
            "extract-submit" => {
                let text = form["path"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .or_else(|| {
                        form["image"]
                            .as_array()
                            .and_then(|v| v.first())
                            .and_then(Value::as_str)
                    })
                    .ok_or("Choose an image or enter its full path")?;
                let path = PathBuf::from(text);
                if !path.is_absolute() {
                    return Err("Enter the image's full absolute path (the WebView file picker did not provide a native path)".into());
                }
                state["colors"] = json!(tokio::task::spawn_blocking(move || extract_image(&path))
                    .await
                    .map_err(|e| e.to_string())??);
                state["form"] = Value::Null;
            }
            "select" => {
                let hex = color()?.hex();
                let mut selected = state["selected"].as_array().cloned().unwrap_or_default();
                if selected.contains(&json!(hex)) {
                    selected.retain(|v| v != &json!(hex));
                } else {
                    selected.push(json!(hex));
                }
                state["selected"] = json!(selected);
            }
            "select-all" => {
                state["selected"] = state["allColors"].clone();
            }
            "clear-selection" => {
                state["selected"] = json!([]);
            }
            "export" => {
                let values = state["selected"].as_array().ok_or("No colors selected")?;
                let colors: Vec<Color> = values
                    .iter()
                    .map(Color::from_value)
                    .collect::<Result<_, _>>()?;
                let output = export_colors(
                    &colors,
                    action["format"].as_str().unwrap_or("text"),
                    preferred(payload),
                )?;
                sdk::copy(ctx, &output).await?;
                store = sdk::update_store(ctx, SLUG, move |s| {
                    for c in colors {
                        add_history(s, c, None)?;
                    }
                    Ok(())
                })
                .await?;
                refresh_active_tray(ctx, &store, preferred(payload)).await?;
                return Ok(sdk::no_view("Copied selected colors"));
            }
            "sort-names" => {
                state["nameSort"] = json!(if state["nameSort"] == "proximity" {
                    "platform"
                } else {
                    "proximity"
                });
            }
            "clear-confirm" => return form_view(state, "clear", None, &store),
            "clear-submit" => {
                if form["confirmation"] != "DELETE" {
                    return Err("Type DELETE to confirm removal of all colors".into());
                }
                store = sdk::update_store(ctx, SLUG, |s| {
                    s["history"] = json!([]);
                    Ok(())
                })
                .await?;
                state["selected"] = json!([]);
                state["form"] = Value::Null;
            }
            "back" => {
                state["form"] = Value::Null;
                if command == "color-wheel" {
                    return form_view(state, "wheel", None, &store);
                }
            }
            _ => return Err(format!("Unsupported color action: {op}")),
        }
        refresh_active_tray(ctx, &store, preferred(payload)).await?;
    } else if operation == "extension:refresh-session" {
        if let Some(kind) = state["form"].as_str().map(str::to_string) {
            // Retain the edit target across host refreshes without accepting an
            // arbitrary action target supplied by the renderer.
            let color = state["formColor"].as_str().map(str::to_string);
            return form_view(state, &kind, color.as_deref(), &store);
        }
    }
    match command {
        "convert-color" => conversion_view(state, &store),
        "color-names" => names_view(state, payload),
        "color-wheel" => {
            if state["colorText"].as_str().is_some() {
                conversion_view(state, &store)
            } else {
                form_view(state, "wheel", None, &store)
            }
        }
        "menu-bar" => configure_tray(ctx, &store, preferred(payload)).await,
        "pick-color" => Ok(sdk::unchanged()),
        _ => collection_view(state, &store, payload, command),
    }
}

fn export_colors(colors: &[Color], format: &str, color_format: &str) -> Result<String, String> {
    let values = colors
        .iter()
        .map(|c| c.format(color_format))
        .collect::<Result<Vec<_>, _>>()?;
    match format {
        "text" => Ok(values.join(";")),
        "json" => {
            serde_json::to_string_pretty(&json!({"colors":values})).map_err(|e| e.to_string())
        }
        "css-classes" => Ok(values
            .iter()
            .enumerate()
            .map(|(i, v)| format!(".color-{} {{\n  color: {v};\n}}", i + 1))
            .collect::<Vec<_>>()
            .join("\n\n")),
        "css-variables" => Ok(format!(
            ":root {{\n{}\n}}",
            values
                .iter()
                .enumerate()
                .map(|(i, v)| format!("  --color-{}: {v};", i + 1))
                .collect::<Vec<_>>()
                .join("\n")
        )),
        _ => Err("Unknown palette export format".into()),
    }
}
async fn pick_and_copy(ctx: &CoreContext, payload: &Value, store: &Value) -> CoreResult {
    let _ = store;
    if payload["launchContext"]["callbackLaunchOptions"].is_object() {
        return Err("Cross-extension picker callbacks need the Native V2 launch-command callback hook; no color was picked or copied".into());
    }
    let Some(c) = pick_native(ctx).await? else {
        return Ok(sdk::no_view("Color picking cancelled"));
    };
    let text = c.format(preferred(payload))?;
    sdk::copy(ctx, &text).await?;
    let store = remember(ctx, c, None).await?;
    refresh_active_tray(ctx, &store, preferred(payload)).await?;
    let suffix = if payload["preferences"]["showColorName"] == true {
        let target = naming_lab(c);
        let mut best = None;
        for rows in palettes().as_object().unwrap().values() {
            for v in rows.as_array().unwrap() {
                let other = Color::parse(v["hex"].as_str().unwrap())?;
                let distance = target
                    .iter()
                    .zip(naming_lab(other))
                    .map(|(a, b)| (a - b).powi(2))
                    .sum::<f64>();
                if best.as_ref().is_none_or(|(d, _)| distance < *d) {
                    best = Some((distance, v["name"].as_str().unwrap()));
                }
            }
        }
        best.map(|(_, name)| format!(" ({name})"))
            .unwrap_or_default()
    } else {
        String::new()
    };
    Ok(sdk::no_view(&format!(
        "Copied color {text}{suffix} to clipboard"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_core::common::CorePaths;
    use std::sync::{Arc, Mutex};

    type Calls = Arc<Mutex<Vec<(String, Vec<Value>)>>>;
    fn context(dir: &Path, ai: Value) -> (CoreContext, Calls) {
        let ctx = CoreContext::headless(CorePaths {
            data_dir: dir.join("data"),
            config_dir: dir.join("config"),
            resource_dir: dir.join("resources"),
            temp_dir: dir.join("temp"),
            home_dir: dir.to_path_buf(),
        });
        let calls = Arc::new(Mutex::new(vec![]));
        let recorded = calls.clone();
        ctx.set_router(Arc::new(move |channel, args| {
            recorded.lock().unwrap().push((channel.clone(), args));
            let ai = ai.clone();
            Box::pin(async move {
                match channel.as_str() {
                    "clipboard:write" | "core:platform:paste-text" => Ok(json!({"ok":true})),
                    "core:platform:selected-text" => Ok(json!("hsl(120 100% 50%)")),
                    "core:ai:extension-ask" => {
                        if ai.is_null() {
                            Err("Fixture AI unavailable".into())
                        } else {
                            Ok(ai)
                        }
                    }
                    _ => Err(format!("Unexpected OS effect in fixture: {channel}")),
                }
            })
        }))
        .unwrap();
        (ctx, calls)
    }
    fn near(a: f64, b: f64, tolerance: f64) {
        assert!((a - b).abs() < tolerance, "{a} differs from {b}");
    }
    fn action(view: &Value, title: &str) -> String {
        view["actions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["title"] == title)
            .unwrap_or_else(|| panic!("Missing action {title}: {view}"))["id"]
            .as_str()
            .unwrap()
            .to_string()
    }
    async fn invoke(
        ctx: &CoreContext,
        command: &str,
        state: &mut Value,
        id: &str,
        form: Value,
    ) -> CoreResult {
        handle(
            ctx,
            command,
            "extension:invoke-action",
            &json!({"actionId":id,"formValues":form}),
            state,
        )
        .await
    }
    fn rows<'a>(node: &'a Value, out: &mut Vec<&'a Value>) {
        if node["type"] == "List.Item" || node["type"] == "Grid.Item" {
            out.push(node);
        }
        if let Some(children) = node["children"].as_array() {
            for child in children {
                rows(child, out);
            }
        }
    }

    #[test]
    fn css_syntax_alpha_and_hue_units_are_strict() {
        for (input, expected) in [
            ("#abc", "#AABBCC"),
            ("#abcd", "#AABBCCDD"),
            ("#12345678", "#12345678"),
            ("fF0000", "#FF0000"),
            ("rgb(100% 0% 0% / 50%)", "#FF000080"),
            ("rgba(255, 0, 0, 0)", "#FF000000"),
            ("hsl(-240deg 100% 50%)", "#00FF00"),
            ("hsl(0.5turn 100% 50% / .5)", "#00FFFF80"),
            ("hsv(240 100% 100%)", "#0000FF"),
            ("color(hsv 240 100 100 / .25)", "#0000FF40"),
            ("color(--hsv 240 100% 100%)", "#0000FF"),
            ("rebeccapurple", "#663399"),
            ("transparent", "#00000000"),
        ] {
            assert_eq!(Color::parse(input).unwrap().hex(), expected, "{input}");
        }
        near(hue("3.141592653589793rad").unwrap(), 180.0, 1e-9);
        near(hue("200grad").unwrap(), 180.0, 1e-9);
        for input in [
            "",
            "#12",
            "#12345",
            "#GGG",
            "#123456789",
            "rgb(1 2)",
            "rgb(1 2 3 4)",
            "rgb(1,2,3 / .5)",
            "rgb(1,,3)",
            "rgb(1 2 3 /)",
            "rgb(1 2 3 / .5 / .5)",
            "rgb(1px 2 3)",
            "rgb(NaN 2 3)",
            "rgb(inf 2 3)",
            "rgb(1 2 3)junk",
            "hsl(0% 100% 50%)",
            "color(bogus 1 2 3)",
            "rgb(1 2 3 / 1e999)",
        ] {
            assert!(
                Color::parse(input).is_err(),
                "Accepted malformed input: {input}"
            );
        }
    }

    #[test]
    fn all_twelve_formats_round_trip_and_preserve_alpha() {
        let c = Color::parse("#3A92D7B0").unwrap();
        for (format, _) in FORMATS {
            let text = c.format(format).unwrap();
            let result = Color::parse(&text).unwrap_or_else(|e| panic!("{format}: {text}: {e}"));
            for (a, b) in c.rgb().iter().zip(result.rgb()) {
                near(*a, b, 0.0002);
            }
            near(c.a, result.a, 0.00001);
        }
        assert!(c.format("unknown").is_err());
        let red = Color::parse("red").unwrap();
        let lab = lab(mul(TO_D50, red.xyz()), [0.9642956764, 1.0, 0.8251046025]);
        near(lab[0], 54.2905, 0.001);
        near(lab[1], 80.8049, 0.002);
        near(lab[2], 69.891, 0.003);
        let ok = oklab(red.rgb().map(linear));
        near(ok[0], 0.62795536, 1e-7);
        near(ok[1], 0.22486306, 1e-7);
        near(ok[2], 0.1258463, 1e-7);
        let p3 = mul(XYZ_P3, red.xyz()).map(gamma);
        near(p3[0], 0.91748756, 1e-6);
        near(p3[1], 0.20028681, 1e-6);
        near(p3[2], 0.13856059, 1e-6);
    }

    #[test]
    fn helper_and_legacy_colors_use_their_declared_units() {
        assert_eq!(
            Color::from_value(
                &json!({"red":1.0,"green":0.0,"blue":0.0,"alpha":0.5,"colorSpace":"srgb"})
            )
            .unwrap()
            .hex(),
            "#FF000080"
        );
        assert_eq!(
            Color::from_value(&json!({"red":255,"green":0,"blue":0,"alpha":1}))
                .unwrap()
                .hex(),
            "#FF0000"
        );
        assert!(
            Color::from_value(&json!({"red":1,"green":0,"blue":0,"colorSpace":"bogus"})).is_err()
        );
    }

    #[test]
    fn source_history_serialization_favorites_and_unknown_keys_are_preserved() {
        let favorite = json!({"color":{"red":255,"green":0,"blue":0,"alpha":1},"title":"Brand","isFavorite":true,"date":"2020-01-01","custom":42});
        let mut store = json!({"history":serde_json::to_string(&vec![favorite]).unwrap(),"unknown":{"keep":true}});
        for i in 0..220 {
            add_history(
                &mut store,
                Color::parse(&format!("#{:06X}", i)).unwrap(),
                None,
            )
            .unwrap();
        }
        assert_eq!(history(&store).unwrap().len(), 200);
        assert_eq!(store["unknown"]["keep"], true);
        let index = history(&store)
            .unwrap()
            .iter()
            .position(|v| v["isFavorite"] == true)
            .unwrap();
        add_history(&mut store, Color::parse("red").unwrap(), None).unwrap();
        assert_eq!(store["history"][index]["title"], "Brand");
        assert_eq!(store["history"][index]["custom"], 42);
        assert_eq!(store["history"][index]["isFavorite"], true);
        assert_eq!(
            store["history"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|v| key(v).as_deref() == Some("#FF0000"))
                .count(),
            1
        );
        let mut corrupt = json!({"history":"{broken"});
        assert!(add_history(&mut corrupt, Color::parse("blue").unwrap(), None).is_err());
        assert_eq!(corrupt["history"], "{broken");
    }

    #[test]
    fn form_values_follow_the_actual_form_runtime_wire_contract() {
        let field = field("title", "Title", "Brand");
        assert_eq!(field["props"]["value"], "Brand");
        assert!(field["props"].get("defaultValue").is_none());
        let runtime = include_str!("../../../../src/renderer/src/raycast-api/form-runtime.tsx");
        assert!(runtime.contains("value: node.props?.value,"));
        let mut state = json!({"colorText":"#123456"});
        let form = form_view(&mut state, "convert", None, &json!({})).unwrap();
        assert_eq!(form["root"]["children"][0]["props"]["value"], "#123456");
        let form = form_view(&mut state, "wheel", None, &json!({})).unwrap();
        assert_eq!(form["root"]["children"][0]["props"]["value"], "0");
        assert_eq!(form["root"]["children"][1]["props"]["value"], "100");
        let store = json!({"history":[{"color":"#123456","title":"Brand"}]});
        let form = form_view(&mut state, "edit", Some("#123456"), &store).unwrap();
        assert_eq!(form["root"]["children"][0]["props"]["value"], "Brand");
    }

    #[tokio::test]
    async fn one_time_migration_imports_only_known_keys_from_temporary_legacy_stores() {
        let temp = tempfile::tempdir().unwrap();
        let (ctx, _) = context(temp.path(), Value::Null);
        let root = ctx.paths.data_dir.join("extensions/color-picker");
        std::fs::create_dir_all(root.join(".tezbar-support/cache")).unwrap();
        let legacy = json!({"history":serde_json::to_string(&json!([
            {"color":{"red":255,"green":0,"blue":0,"alpha":1},"title":"Brand","isFavorite":true,"date":"2020-01-01","unknownItem":"discard"},
            {"color":"#0000FF","isFavorite":false}
        ])).unwrap(),"unrelatedSecret":"fixture-only-never-import"});
        let path = root.join(".tezbar-support/cache/shared.json");
        let original = serde_json::to_vec(&legacy).unwrap();
        std::fs::write(&path, &original).unwrap();
        std::fs::write(
            root.join(".tezbar-local-storage.json"),
            serde_json::to_vec(&json!({"lastConvertedColorFormat":"hsla","unrelated":"discard"}))
                .unwrap(),
        )
        .unwrap();
        sdk::update_store(&ctx, SLUG, |v| {
            v["future"] = json!({"keep":true});
            Ok(())
        })
        .await
        .unwrap();
        let store = migrated_store(&ctx).await.unwrap();
        assert_eq!(store["history"].as_array().unwrap().len(), 2);
        assert_eq!(store["history"][0]["isFavorite"], true);
        assert_eq!(store["history"][0]["title"], "Brand");
        assert_eq!(store["lastConvertedColorFormat"], "hsla");
        assert_eq!(store["future"]["keep"], true);
        assert!(store.get("unrelatedSecret").is_none());
        assert!(store["history"][0].get("unknownItem").is_none());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        // Deletion after import must never resurrect the source history.
        sdk::update_store(&ctx, SLUG, |v| {
            v["history"] = json!([]);
            Ok(())
        })
        .await
        .unwrap();
        assert!(history(&migrated_store(&ctx).await.unwrap())
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn malformed_legacy_history_is_preserved_and_does_not_mark_migration_complete() {
        let temp = tempfile::tempdir().unwrap();
        let (ctx, _) = context(temp.path(), Value::Null);
        let root = ctx.paths.data_dir.join("extensions/color-picker");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(".tezbar-local-storage.json");
        std::fs::write(&path, b"{broken").unwrap();
        assert!(migrated_store(&ctx).await.is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"{broken");
        assert!(sdk::read_store(&ctx, SLUG)
            .await
            .unwrap()
            .get("legacyMigrationVersion")
            .is_none());
    }

    #[test]
    fn palettes_and_nearest_name_views_use_source_data() {
        assert_eq!(palettes().as_object().unwrap().len(), 6);
        assert_eq!(palettes()["ntc"].as_array().unwrap().len(), 1566);
        let mut state = json!({"colorText":"#FF0000"});
        let view = names_view(
            &mut state,
            &json!({"preferences":{"colorNamesPerGroup":"5"}}),
        )
        .unwrap();
        let mut items = vec![];
        rows(&view["root"], &mut items);
        assert_eq!(items.len(), 30);
        assert!(items.iter().any(|v| v["props"]["title"] == "red"
            && v["props"]["subtitle"].as_str().unwrap().contains("ΔE 0")));
        state["nameSort"] = json!("proximity");
        let view = names_view(&mut state, &json!({})).unwrap();
        let mut items = vec![];
        rows(&view["root"], &mut items);
        let names: HashSet<_> = items
            .iter()
            .map(|v| v["props"]["title"].as_str().unwrap().to_lowercase())
            .collect();
        assert_eq!(items.len(), names.len());
    }

    #[tokio::test]
    async fn conversion_selected_text_and_clipboard_use_native_router() {
        let temp = tempfile::tempdir().unwrap();
        let (ctx, calls) = context(temp.path(), Value::Null);
        let mut state = json!({});
        let view = handle(
            &ctx,
            "convert-color",
            "extension:run-command",
            &json!({}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(view["root"]["children"].as_array().unwrap().len(), 12);
        let id = view["root"]["children"][0]["props"]["actionIds"][0]
            .as_str()
            .unwrap();
        invoke(&ctx, "convert-color", &mut state, id, json!({}))
            .await
            .unwrap();
        {
            let log = calls.lock().unwrap();
            assert_eq!(log[0].0, "core:platform:selected-text");
            assert_eq!(log[1], ("clipboard:write".into(), vec![json!("#00FF00")]));
        }
        assert_eq!(
            sdk::read_store(&ctx, SLUG).await.unwrap()["lastConvertedColorFormat"],
            "hex"
        );
        assert!(invoke(
            &ctx,
            "convert-color",
            &mut state,
            "color-picker:copy:arbitrary",
            json!({})
        )
        .await
        .is_err());
        let view = handle(
            &ctx,
            "convert-color",
            "extension:search-text-changed",
            &json!({"searchText":"rgb(invalid)"}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(
            view["root"]["children"][0]["props"]["title"],
            "Invalid color"
        );
        assert_eq!(calls.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn favorite_title_selection_export_and_delete_are_persistent() {
        let temp = tempfile::tempdir().unwrap();
        let (ctx, calls) = context(temp.path(), Value::Null);
        let mut state = json!({});
        let view = handle(
            &ctx,
            "organize-colors",
            "extension:run-command",
            &json!({}),
            &mut state,
        )
        .await
        .unwrap();
        let id = action(&view, "Add Color");
        let view = invoke(&ctx, "organize-colors", &mut state, &id, json!({}))
            .await
            .unwrap();
        let id = action(&view, "Save Color");
        let view = invoke(
            &ctx,
            "organize-colors",
            &mut state,
            &id,
            json!({"color":"hsl(240 100% 50%)","title":"Blue brand"}),
        )
        .await
        .unwrap();
        let id = action(&view, "Add to Favorites");
        let view = invoke(&ctx, "organize-colors", &mut state, &id, json!({}))
            .await
            .unwrap();
        assert_eq!(
            sdk::read_store(&ctx, SLUG).await.unwrap()["history"][0]["isFavorite"],
            true
        );
        let id = action(&view, "Edit Title");
        let view = invoke(&ctx, "organize-colors", &mut state, &id, json!({}))
            .await
            .unwrap();
        let refreshed = handle(
            &ctx,
            "organize-colors",
            "extension:refresh-session",
            &json!({}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(refreshed["title"], "Edit Color Title");
        let id = action(&view, "Save Title");
        let view = invoke(
            &ctx,
            "organize-colors",
            &mut state,
            &id,
            json!({"title":"New title"}),
        )
        .await
        .unwrap();
        let id = action(&view, "Select All Colors");
        let view = invoke(&ctx, "organize-colors", &mut state, &id, json!({}))
            .await
            .unwrap();
        let id = action(&view, "Copy Colors as JSON");
        invoke(&ctx, "organize-colors", &mut state, &id, json!({}))
            .await
            .unwrap();
        let copied = calls.lock().unwrap().last().unwrap().1[0]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(
            serde_json::from_str::<Value>(&copied).unwrap(),
            json!({"colors":["#0000FF"]})
        );
        let store = sdk::read_store(&ctx, SLUG).await.unwrap();
        assert_eq!(store["history"][0]["title"], "New title");
        let view = handle(
            &ctx,
            "favorite-colors",
            "extension:run-command",
            &json!({}),
            &mut state,
        )
        .await
        .unwrap();
        let id = action(&view, "Delete All Colors…");
        let view = invoke(&ctx, "favorite-colors", &mut state, &id, json!({}))
            .await
            .unwrap();
        let id = action(&view, "Delete All Colors");
        assert!(invoke(
            &ctx,
            "favorite-colors",
            &mut state,
            &id,
            json!({"confirmation":"no"})
        )
        .await
        .is_err());
        assert_eq!(
            history(&sdk::read_store(&ctx, SLUG).await.unwrap())
                .unwrap()
                .len(),
            1
        );
        invoke(
            &ctx,
            "favorite-colors",
            &mut state,
            &id,
            json!({"confirmation":"DELETE"}),
        )
        .await
        .unwrap();
        assert!(history(&sdk::read_store(&ctx, SLUG).await.unwrap())
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn ai_uses_real_router_contract_and_rejects_invalid_responses() {
        let temp = tempfile::tempdir().unwrap();
        let (ctx, calls) = context(temp.path(), json!(r##"["#123456","#abcdef"]"##));
        let mut state = json!({});
        let view = handle(
            &ctx,
            "generate-colors",
            "extension:run-command",
            &json!({"argumentValues":{"prompt":"Ocean"}}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(view["root"]["type"], "Grid");
        assert_eq!(view["root"]["children"].as_array().unwrap().len(), 2);
        assert_eq!(calls.lock().unwrap()[0].0, "core:ai:extension-ask");
        assert!(generated_colors(&json!("Here are some colors")).is_err());
        assert!(generated_colors(&json!("[1]")).is_err());
        assert!(generated_colors(&json!("[\"red\"]")).is_err());
        assert!(generated_colors(&json!("[\"#nothex\"]")).is_err());
        let other = tempfile::tempdir().unwrap();
        let (ctx, _) = context(other.path(), Value::Null);
        assert!(handle(
            &ctx,
            "generate-colors",
            "extension:run-command",
            &json!({"argumentValues":{"prompt":"Ocean"}}),
            &mut json!({})
        )
        .await
        .unwrap_err()
        .contains("unavailable"));
    }

    #[tokio::test]
    async fn wheel_calculates_in_rust_and_never_samples_the_real_screen() {
        let temp = tempfile::tempdir().unwrap();
        let (ctx, calls) = context(temp.path(), Value::Null);
        let mut state = json!({});
        let view = handle(
            &ctx,
            "color-wheel",
            "extension:run-command",
            &json!({}),
            &mut state,
        )
        .await
        .unwrap();
        let id = action(&view, "Use Color");
        let view = invoke(
            &ctx,
            "color-wheel",
            &mut state,
            &id,
            json!({"hue":"120","saturation":"100","value":"50","alpha":"50"}),
        )
        .await
        .unwrap();
        assert_eq!(state["wheelColor"], "#00800080");
        assert_eq!(view["title"], "Color Wheel");
        assert!(calls.lock().unwrap().is_empty());
        let id = action(&view, "Adjust Hue, Saturation and Brightness");
        let view = invoke(&ctx, "color-wheel", &mut state, &id, json!({}))
            .await
            .unwrap();
        let id = action(&view, "Use Color");
        assert!(invoke(
            &ctx,
            "color-wheel",
            &mut state,
            &id,
            json!({"hue":"NaN","saturation":"100","value":"100","alpha":"100"})
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn wheel_renderer_bridge_uses_numeric_coordinates_and_rust_format_values() {
        let temp = tempfile::tempdir().unwrap();
        let (ctx, calls) = context(temp.path(), Value::Null);
        let mut state = json!({});
        let view = handle(
            &ctx,
            "color-wheel",
            "extension:run-command",
            &json!({}),
            &mut state,
        )
        .await
        .unwrap();
        let change = view["root"]["props"]["nativeColorWheel"]["changeActionId"]
            .as_str()
            .unwrap();
        let view = invoke(
            &ctx,
            "color-wheel",
            &mut state,
            change,
            json!({"hue":240.0,"saturation":100.0,"value":100.0,"alpha":100.0}),
        )
        .await
        .unwrap();
        let wire = &view["root"]["props"]["nativeColorWheel"];
        assert_eq!(wire["preview"], "#0000FF");
        assert_eq!(wire["formats"].as_array().unwrap().len(), 12);
        assert_eq!(wire["formats"][0]["value"], "#0000FF");
        assert!(history(&sdk::read_store(&ctx, SLUG).await.unwrap())
            .unwrap()
            .is_empty());
        let copy = wire["formats"][0]["actionId"].as_str().unwrap();
        invoke(&ctx, "color-wheel", &mut state, copy, json!({}))
            .await
            .unwrap();
        assert_eq!(
            calls.lock().unwrap().last().unwrap().1,
            vec![json!("#0000FF")]
        );
        assert_eq!(
            sdk::read_store(&ctx, SLUG).await.unwrap()["history"][0]["color"],
            "#0000FF"
        );
    }

    #[tokio::test]
    async fn extract_command_uses_only_the_requested_fixture_file() {
        let temp = tempfile::tempdir().unwrap();
        let (ctx, calls) = context(temp.path(), Value::Null);
        let mut state = json!({});
        let path = temp.path().join("colors.png");
        image::RgbaImage::from_pixel(4, 4, image::Rgba([12, 34, 56, 255]))
            .save(&path)
            .unwrap();
        let view = handle(
            &ctx,
            "extract-color",
            "extension:run-command",
            &json!({"argumentValues":{"path":path.to_string_lossy()}}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(view["root"]["type"], "Grid");
        assert_eq!(
            view["root"]["children"][0]["props"]["content"]["color"],
            "#0C2238"
        );
        assert!(calls.lock().unwrap().is_empty());
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        assert!(handle(
            &ctx,
            "menu-bar",
            "extension:run-command",
            &json!({}),
            &mut json!({})
        )
        .await
        .unwrap_err()
        .contains("running desktop application"));
    }

    #[test]
    fn image_palette_uses_fixture_pixels_and_ignores_transparency() {
        let colors = palette_from_pixels(
            [
                [255, 0, 0, 255],
                [255, 0, 0, 255],
                [0, 0, 255, 255],
                [0, 255, 0, 0],
            ]
            .into_iter(),
        )
        .unwrap();
        assert_eq!(colors, vec![json!("#FF0000"), json!("#0000FF")]);
        assert!(palette_from_pixels([[0, 0, 0, 0]].into_iter())
            .unwrap()
            .is_empty());
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("fixture.png");
        image::RgbaImage::from_fn(8, 8, |x, _| {
            if x < 4 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 255, 255])
            }
        })
        .save(&path)
        .unwrap();
        let colors = extract_image(&path).unwrap();
        assert_eq!(colors.len(), 2);
        assert!(colors.contains(&json!("#FF0000")));
        assert!(colors.contains(&json!("#0000FF")));
        let bad = temp.path().join("bad.png");
        std::fs::write(&bad, b"not an image").unwrap();
        assert!(extract_image(&bad).is_err());
    }
}
