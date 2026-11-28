//! Ports of shared/textTransform{,ers}.ts. Work is bounded before allocation.
use base64::{engine::general_purpose, Engine};
use chrono::{DateTime, Utc};
use regex::Regex;
macro_rules! cached_regex {
    ($pattern:literal) => {{
        static RE: std::sync::LazyLock<Regex> =
            std::sync::LazyLock::new(|| Regex::new($pattern).unwrap());
        &*RE
    }};
}
use sha2::{Digest, Sha256, Sha512};
pub type TransformResult = Result<String, String>;
fn bound(input: &str) -> Result<(), String> {
    if input.len() > super::MAX_INPUT_BYTES {
        Err("Input is too long.".into())
    } else {
        Ok(())
    }
}
pub fn json_format(s: &str, pretty: bool) -> TransformResult {
    bound(s)?;
    if js_trim(s).is_empty() {
        return Err("Nothing to format — paste some JSON first.".into());
    }
    json_ordered(js_trim(s), pretty).map_err(|e| format!("Invalid JSON: {e}"))
}
/// ECMAScript trim includes BOM, but excludes NEL (Rust's is_whitespace includes it).
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c == '\u{feff}' || (c.is_whitespace() && c != '\u{85}'))
}
// Preserve object insertion order without adding a second JSON dependency. Deserialize to a tiny
// ordered tree using serde's visitor, then use serde_json's serializer for escaping and numbers.
#[derive(Debug)]
enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}
impl<'de> serde::Deserialize<'de> for Json {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Json;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON")
            }
            fn visit_unit<E>(self) -> Result<Json, E> {
                Ok(Json::Null)
            }
            fn visit_bool<E>(self, v: bool) -> Result<Json, E> {
                Ok(Json::Bool(v))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Json, E> {
                Ok(Json::Number(v as f64))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Json, E> {
                Ok(Json::Number(v as f64))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Json, E> {
                Ok(Json::Number(v))
            }
            fn visit_str<E>(self, v: &str) -> Result<Json, E> {
                Ok(Json::String(v.into()))
            }
            fn visit_string<E>(self, v: String) -> Result<Json, E> {
                Ok(Json::String(v))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut a: A) -> Result<Json, A::Error> {
                let mut v = vec![];
                while let Some(x) = a.next_element()? {
                    v.push(x);
                }
                Ok(Json::Array(v))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut a: A) -> Result<Json, A::Error> {
                let mut v: Vec<(String, Json)> = vec![];
                let mut indices = std::collections::HashMap::<String, usize>::new();
                while let Some((k, x)) = a.next_entry::<String, Json>()? {
                    if let Some(&index) = indices.get(&k) {
                        v[index].1 = x;
                    } else {
                        indices.insert(k.clone(), v.len());
                        v.push((k, x));
                    }
                }
                v.sort_by_key(|(k, _)| {
                    k.parse::<u32>()
                        .ok()
                        .filter(|n| *n < u32::MAX && n.to_string() == *k)
                        .map(|n| (0, n))
                        .unwrap_or((1, 0))
                });
                Ok(Json::Object(v))
            }
        }
        d.deserialize_any(V)
    }
}
fn json_render(v: &Json, pretty: bool) -> String {
    fn write(v: &Json, pretty: bool, depth: usize, out: &mut String) {
        let newline = |out: &mut String, n: usize| {
            if pretty {
                out.push('\n');
                out.push_str(&"  ".repeat(n));
            }
        };
        match v {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Number(n) => out.push_str(&if n.is_finite() {
                super::number(*n)
            } else {
                "null".into()
            }),
            Json::String(s) => out.push_str(&serde_json::to_string(s).unwrap()),
            Json::Array(values) => {
                out.push('[');
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    newline(out, depth + 1);
                    write(v, pretty, depth + 1, out);
                }
                if !values.is_empty() {
                    newline(out, depth);
                }
                out.push(']');
            }
            Json::Object(values) => {
                out.push('{');
                for (i, (k, v)) in values.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    newline(out, depth + 1);
                    out.push_str(&serde_json::to_string(k).unwrap());
                    out.push(':');
                    if pretty {
                        out.push(' ');
                    }
                    write(v, pretty, depth + 1, out);
                }
                if !values.is_empty() {
                    newline(out, depth);
                }
                out.push('}');
            }
        }
    }
    let mut out = String::new();
    write(v, pretty, 0, &mut out);
    out
}
fn json_ordered(s: &str, pretty: bool) -> TransformResult {
    let v: Json = serde_json::from_str(s).map_err(|e| e.to_string())?;
    Ok(json_render(&v, pretty))
}
fn atob_engine() -> general_purpose::GeneralPurpose {
    general_purpose::GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        general_purpose::GeneralPurposeConfig::new().with_decode_allow_trailing_bits(true),
    )
}
pub fn base64_encode(s: &str) -> TransformResult {
    bound(s)?;
    Ok(general_purpose::STANDARD.encode(s))
}
pub fn base64_decode(s: &str) -> TransformResult {
    bound(s)?;
    if s.trim().is_empty() {
        return Err("Nothing to decode — paste base64 first.".into());
    }
    let s = s
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .replace('-', "+")
        .replace('_', "/");
    let s = format!("{}{}", s, "=".repeat((4 - s.len() % 4) % 4));
    let bytes = atob_engine()
        .decode(s)
        .map_err(|_| "Input is not valid base64.")?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
pub fn url_encode(s: &str) -> TransformResult {
    bound(s)?;
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    Ok(out)
}
pub fn url_decode(s: &str, plus_space: bool) -> TransformResult {
    bound(s)?;
    if s.trim().is_empty() {
        return Err("Nothing to decode — paste an encoded string first.".into());
    }
    let s = if plus_space {
        s.trim().replace('+', " ")
    } else {
        s.to_string()
    };
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if i + 2 >= b.len() {
                return Err("Could not URL-decode: malformed URI".into());
            }
            let v = std::str::from_utf8(&b[i + 1..i + 3])
                .ok()
                .and_then(|s| u8::from_str_radix(s, 16).ok())
                .ok_or("Could not URL-decode: malformed URI")?;
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "Could not URL-decode: malformed UTF-8".into())
}
pub fn md5_hex(bytes: &[u8]) -> String {
    let mut data = bytes.to_vec();
    let bits = (bytes.len() as u64).wrapping_mul(8);
    data.push(128);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend(bits.to_le_bytes());
    let mut h = [0x67452301u32, 0xefcdab89, 0x98badcfe, 0x10325476];
    let shifts = [7, 12, 17, 22, 5, 9, 14, 20, 4, 11, 16, 23, 6, 10, 15, 21];
    for block in data.chunks_exact(64) {
        let mut m = [0u32; 16];
        for (i, c) in block.chunks_exact(4).enumerate() {
            m[i] = u32::from_le_bytes(c.try_into().unwrap());
        }
        let [mut a, mut b, mut c, mut d] = h;
        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | (!b & d), i),
                16..=31 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let k = ((i + 1) as f64)
                .sin()
                .abs()
                .mul_add(4294967296., 0.)
                .floor() as u32;
            let v = b.wrapping_add(
                a.wrapping_add(f)
                    .wrapping_add(k)
                    .wrapping_add(m[g])
                    .rotate_left(shifts[(i / 16) * 4 + i % 4]),
            );
            a = d;
            d = c;
            c = b;
            b = v;
        }
        for (x, v) in h.iter_mut().zip([a, b, c, d]) {
            *x = x.wrapping_add(v);
        }
    }
    h.iter()
        .flat_map(|v| v.to_le_bytes())
        .map(|b| format!("{b:02x}"))
        .collect()
}
#[derive(Clone, Copy, Debug)]
pub enum HashAlgorithm {
    Md5,
    Sha256,
    Sha512,
}
pub fn hash_text(s: &str, a: HashAlgorithm) -> TransformResult {
    bound(s)?;
    if s.is_empty() {
        return Err("Nothing to hash — input is empty.".into());
    }
    Ok(match a {
        HashAlgorithm::Md5 => md5_hex(s.as_bytes()),
        HashAlgorithm::Sha256 => format!("{:x}", Sha256::digest(s.as_bytes())),
        HashAlgorithm::Sha512 => format!("{:x}", Sha512::digest(s.as_bytes())),
    })
}
#[derive(Debug, PartialEq)]
pub struct WordCount {
    pub characters: usize,
    pub characters_no_spaces: usize,
    pub words: usize,
    pub sentences: usize,
    pub paragraphs: usize,
    pub lines: usize,
}
pub fn count_text(s: &str) -> WordCount {
    let words = cached_regex!(r"[\p{L}\p{N}]+(?:['’\-][\p{L}\p{N}]+)*")
        .find_iter(s)
        .count();
    let sentences = cached_regex!(r"[^.!?…\n][.!?…]+(?:\s|$)")
        .find_iter(s)
        .count();
    let paragraphs = cached_regex!(r"\n\s*\n+")
        .split(s)
        .filter(|s| !s.trim().is_empty())
        .count();
    WordCount {
        characters: s.chars().count(),
        characters_no_spaces: s.chars().filter(|c| !c.is_whitespace()).count(),
        words,
        sentences,
        paragraphs,
        lines: if s.is_empty() {
            0
        } else {
            s.split('\n').count()
        },
    }
}
pub fn word_count(s: &str) -> TransformResult {
    bound(s)?;
    if s.trim().is_empty() {
        return Err("Nothing to count — input is empty.".into());
    }
    let c = count_text(s);
    Ok(format!("Words: {}\nCharacters: {}\nCharacters (no spaces): {}\nSentences: {}\nParagraphs: {}\nLines: {}",c.words,c.characters,c.characters_no_spaces,c.sentences,c.paragraphs,c.lines))
}
pub fn split_words(s: &str) -> Vec<String> {
    let s = cached_regex!(r"([a-z\d])([A-Z])").replace_all(s, "$1 $2");
    let s = cached_regex!(r"([A-Z]+)([A-Z][a-z])").replace_all(&s, "$1 $2");
    cached_regex!(r"[\s_\-./\\]+")
        .split(&s)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}
#[derive(Clone, Copy, Debug)]
pub enum CaseStyle {
    Camel,
    Snake,
    Kebab,
    Constant,
    Title,
    Lower,
    Upper,
}
pub fn convert_case(s: &str, style: CaseStyle) -> TransformResult {
    bound(s)?;
    let w = split_words(s);
    if w.is_empty() {
        return Err("Nothing to convert — input has no words.".into());
    }
    let lower: Vec<_> = w.iter().map(|w| w.to_lowercase()).collect();
    let cap: Vec<_> = lower
        .iter()
        .map(|w| {
            let mut c = w.chars();
            c.next().unwrap().to_uppercase().collect::<String>() + c.as_str()
        })
        .collect();
    Ok(match style {
        CaseStyle::Camel => lower[0].clone() + &cap[1..].join(""),
        CaseStyle::Snake => lower.join("_"),
        CaseStyle::Kebab => lower.join("-"),
        CaseStyle::Constant => lower.join("_").to_uppercase(),
        CaseStyle::Title => cap.join(" "),
        CaseStyle::Lower => lower.join(" "),
        CaseStyle::Upper => lower.join(" ").to_uppercase(),
    })
}
/// Presentation data remains structured until the platform locale formatter is injected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelativeTime {
    pub unit: &'static str,
    pub value: i64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Timestamp {
    pub ms: i64,
    pub iso: String,
    pub relative: RelativeTime,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardCount {
    pub characters: usize,
    pub words: usize,
    pub lines: usize,
    pub bytes: usize,
}
/// M5/M6 inject the OS locale implementation; portable default is explicitly English.
pub trait PresentationFormatter {
    fn local_timestamp(&self, ms: i64) -> String;
    fn relative(&self, relative: &RelativeTime) -> String;
    fn count(&self, count: usize) -> String;
}
pub struct EnglishFormatter;
impl PresentationFormatter for EnglishFormatter {
    fn local_timestamp(&self, ms: i64) -> String {
        DateTime::<Utc>::from_timestamp_millis(ms)
            .unwrap()
            .with_timezone(&chrono::Local)
            .to_string()
    }
    fn relative(&self, r: &RelativeTime) -> String {
        let (unit, n) = (r.unit, r.value);
        if n == 0 {
            return if unit == "second" {
                "now".into()
            } else {
                format!("this {unit}")
            };
        }
        if n.abs() == 1 {
            if unit == "day" {
                return if n < 0 {
                    "yesterday".into()
                } else {
                    "tomorrow".into()
                };
            }
            if matches!(unit, "week" | "month" | "year") {
                return format!("{} {unit}", if n < 0 { "last" } else { "next" });
            }
        }
        format!(
            "{}{} {unit}{}{}",
            if n > 0 { "in " } else { "" },
            n.abs(),
            if n.abs() == 1 { "" } else { "s" },
            if n < 0 { " ago" } else { "" }
        )
    }
    fn count(&self, count: usize) -> String {
        let raw = count.to_string();
        let mut out = String::new();
        for (i, c) in raw.chars().enumerate() {
            if i > 0 && (raw.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
        }
        out
    }
}
pub fn relative_time(target: i64, now: i64) -> RelativeTime {
    let diff = target as f64 - now as f64;
    for (limit, unit, step) in [
        (60000., "second", 1000.),
        (3600000., "minute", 60000.),
        (86400000., "hour", 3600000.),
        (604800000., "day", 86400000.),
        (2592000000., "week", 604800000.),
        (31536000000., "month", 2592000000.),
        (f64::INFINITY, "year", 31536000000.),
    ] {
        if diff.abs() < limit {
            return RelativeTime {
                unit,
                value: (diff / step + 0.5).floor() as i64,
            };
        }
    }
    unreachable!()
}
pub fn parse_timestamp_data(s: &str, now_ms: i64) -> Result<Timestamp, String> {
    bound(s)?;
    let s = js_trim(s);
    if !cached_regex!(r"^-?\d+(\.\d+)?$").is_match(s) {
        return Err("Input is not a numeric unix timestamp.".into());
    }
    let mut ms = s.parse::<f64>().map_err(|_| "Timestamp is out of range.")?;
    if ms.abs() < 1e11 {
        ms = (ms * 1000. + 0.5).floor();
    }
    if !ms.is_finite() || ms.abs() > 8.64e15 {
        return Err("Timestamp is out of range.".into());
    }
    let ms = ms as i64;
    let d = DateTime::<Utc>::from_timestamp_millis(ms).ok_or("Timestamp is out of range.")?;
    Ok(Timestamp {
        ms,
        iso: d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        relative: relative_time(ms, now_ms),
    })
}
pub fn format_timestamp(data: &Timestamp, formatter: &impl PresentationFormatter) -> String {
    format!(
        "Unix (ms): {}\nISO 8601: {}\nLocal: {}\nRelative: {}",
        data.ms,
        data.iso,
        formatter.local_timestamp(data.ms),
        formatter.relative(&data.relative)
    )
}
pub fn parse_timestamp(s: &str, now_ms: i64) -> TransformResult {
    Ok(format_timestamp(
        &parse_timestamp_data(s, now_ms)?,
        &EnglishFormatter,
    ))
}
pub fn format_relative(target: i64, now: i64) -> String {
    EnglishFormatter.relative(&relative_time(target, now))
}
pub fn clipboard_count(s: &str) -> ClipboardCount {
    ClipboardCount {
        characters: s.chars().count(),
        words: s.split_whitespace().count(),
        lines: if s.is_empty() {
            0
        } else {
            cached_regex!(r"\r\n|\r|\n").split(s).count()
        },
        bytes: s.len(),
    }
}
pub fn format_clipboard_count(
    c: &ClipboardCount,
    formatter: &impl PresentationFormatter,
) -> String {
    format!(
        "Characters : {}\nWords      : {}\nLines      : {}\nBytes (utf-8): {}",
        formatter.count(c.characters),
        formatter.count(c.words),
        formatter.count(c.lines),
        formatter.count(c.bytes)
    )
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClipboardAction {
    Json,
    Base64,
    Encode,
    Hash,
    Count,
    Case,
    Qr,
}
pub fn apply_clipboard_action(a: ClipboardAction, s: &str) -> TransformResult {
    bound(s)?;
    match a {
        ClipboardAction::Json => {
            let trimmed = js_trim(s);
            match serde_json::from_str::<Json>(trimmed) {
                Ok(v) => Ok(json_render(&v, true)),
                Err(e)
                    if e.to_string().contains("recursion limit exceeded")
                        || e.to_string().contains("number out of range") =>
                {
                    Err("JSON exceeds the supported nesting (128) or finite-number range".into())
                }
                Err(_) => Ok(serde_json::to_string(s).unwrap()),
            }
        }
        ClipboardAction::Base64 => {
            let c = s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
            if !c.is_empty()
                && c.len() % 4 == 0
                && cached_regex!(r"^[A-Za-z0-9+/]+={0,2}$").is_match(&c)
                && atob_engine().decode(&c).is_ok()
            {
                base64_decode(&c)
            } else {
                base64_encode(s)
            }
        }
        ClipboardAction::Encode => {
            if cached_regex!(r"%[0-9a-fA-F]{2}").is_match(s) {
                url_decode(s, false)
            } else {
                url_encode(s)
            }
        }
        ClipboardAction::Hash => Ok(format!("{:x}", Sha256::digest(s.as_bytes()))),
        ClipboardAction::Count => Ok(format_clipboard_count(
            &clipboard_count(s),
            &EnglishFormatter,
        )),
        ClipboardAction::Case => Ok(if s == s.to_lowercase() {
            s.to_uppercase()
        } else if s == s.to_uppercase() {
            s.to_lowercase()
        } else {
            s.chars()
                .map(|c| {
                    if c.to_lowercase().to_string() == c.to_string() {
                        c.to_uppercase().to_string()
                    } else {
                        c.to_lowercase().to_string()
                    }
                })
                .collect()
        }),
        ClipboardAction::Qr => super::qr::svg_data_url(s),
    }
}
#[cfg(test)]
#[path = "transform_tests.rs"]
mod tests;
