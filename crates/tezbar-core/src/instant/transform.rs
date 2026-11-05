//! Ports of shared/textTransform{,ers}.ts. Work is bounded before allocation.
use base64::{engine::general_purpose, Engine};
use chrono::{DateTime, Utc};
use regex::Regex;
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
    if s.trim().is_empty() {
        return Err("Nothing to format — paste some JSON first.".into());
    }
    let v: serde_json::Value = serde_json::from_str(s).map_err(|e| format!("Invalid JSON: {e}"))?;
    // serde_json's map is sorted; JS insertion ordering is restored by the dedicated serializer below.
    json_ordered(s, pretty).or_else(|_| {
        if pretty {
            serde_json::to_string_pretty(&v)
        } else {
            serde_json::to_string(&v)
        }
        .map_err(|e| e.to_string())
    })
}
// Preserve object insertion order without adding a second JSON dependency. Deserialize to a tiny
// ordered tree using serde's visitor, then use serde_json's serializer for escaping and numbers.
#[derive(Debug)]
enum Json {
    Null,
    Bool(bool),
    Number(serde_json::Number),
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
                Ok(Json::Number(v.into()))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Json, E> {
                Ok(Json::Number(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Json, E> {
                Ok(serde_json::Number::from_f64(v)
                    .map(Json::Number)
                    .unwrap_or(Json::Null))
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
                while let Some((k, x)) = a.next_entry::<String, Json>()? {
                    if let Some(e) = v.iter_mut().find(|e| e.0 == k) {
                        e.1 = x;
                    } else {
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
impl serde::Serialize for Json {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{SerializeMap, SerializeSeq};
        match self {
            Self::Null => s.serialize_unit(),
            Self::Bool(v) => s.serialize_bool(*v),
            Self::Number(v) => v.serialize(s),
            Self::String(v) => s.serialize_str(v),
            Self::Array(v) => {
                let mut a = s.serialize_seq(Some(v.len()))?;
                for e in v {
                    a.serialize_element(e)?;
                }
                a.end()
            }
            Self::Object(v) => {
                let mut a = s.serialize_map(Some(v.len()))?;
                for (k, v) in v {
                    a.serialize_entry(k, v)?;
                }
                a.end()
            }
        }
    }
}
fn json_ordered(s: &str, pretty: bool) -> TransformResult {
    let v: Json = serde_json::from_str(s).map_err(|e| e.to_string())?;
    if pretty {
        serde_json::to_string_pretty(&v)
    } else {
        serde_json::to_string(&v)
    }
    .map_err(|e| e.to_string())
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
    let bytes = general_purpose::STANDARD
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
    let words = Regex::new(r"[\p{L}\p{N}]+(?:['’\-][\p{L}\p{N}]+)*")
        .unwrap()
        .find_iter(s)
        .count();
    let sentences = Regex::new(r"[^.!?…\n][.!?…]+(?:\s|$)")
        .unwrap()
        .find_iter(s)
        .count();
    let paragraphs = Regex::new(r"\n\s*\n+")
        .unwrap()
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
    let s = Regex::new(r"([a-z\d])([A-Z])")
        .unwrap()
        .replace_all(s, "$1 $2");
    let s = Regex::new(r"([A-Z]+)([A-Z][a-z])")
        .unwrap()
        .replace_all(&s, "$1 $2");
    Regex::new(r"[\s_\-./\\]+")
        .unwrap()
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
/// UTC deterministic representation; adapters supply localized `Local` text when rendering.
pub fn parse_timestamp(s: &str, now_ms: i64) -> TransformResult {
    bound(s)?;
    if !Regex::new(r"^-?\d+(\.\d+)?$").unwrap().is_match(s.trim()) {
        return Err("Input is not a numeric unix timestamp.".into());
    }
    let mut ms = s
        .trim()
        .parse::<f64>()
        .map_err(|_| "Timestamp is out of range.")?;
    if ms.abs() < 1e11 {
        ms = (ms * 1000. + 0.5).floor();
    }
    if !ms.is_finite() || ms.abs() > 8.64e15 {
        return Err("Timestamp is out of range.".into());
    }
    let d =
        DateTime::<Utc>::from_timestamp_millis(ms as i64).ok_or("Timestamp is out of range.")?;
    Ok(format!(
        "Unix (ms): {}\nISO 8601: {}\nLocal: {}\nRelative: {}",
        super::number(ms),
        d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        d,
        format_relative(ms as i64, now_ms)
    ))
}
pub fn format_relative(target: i64, now: i64) -> String {
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
            let n = (diff / step + 0.5).floor() as i64;
            if n == 0 {
                return if unit == "second" {
                    "now".into()
                } else {
                    format!("this {unit}")
                };
            }
            if unit == "day" && n.abs() == 1 {
                return if n < 0 {
                    "yesterday".into()
                } else {
                    "tomorrow".into()
                };
            }
            return format!(
                "{}{} {unit}{}{}",
                if n > 0 { "in " } else { "" },
                n.abs(),
                if n.abs() == 1 { "" } else { "s" },
                if n < 0 { " ago" } else { "" }
            );
        }
    }
    unreachable!()
}
#[derive(Clone, Copy, Debug)]
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
            if serde_json::from_str::<serde_json::Value>(s).is_ok() {
                json_format(s, true)
            } else {
                Ok(serde_json::to_string(s).unwrap())
            }
        }
        ClipboardAction::Base64 => {
            let c = s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
            if !c.is_empty()
                && c.len() % 4 == 0
                && Regex::new(r"^[A-Za-z0-9+/]+={0,2}$").unwrap().is_match(&c)
                && general_purpose::STANDARD.decode(&c).is_ok()
            {
                base64_decode(&c)
            } else {
                base64_encode(s)
            }
        }
        ClipboardAction::Encode => {
            if Regex::new(r"%[0-9a-fA-F]{2}").unwrap().is_match(s) {
                url_decode(s, false)
            } else {
                url_encode(s)
            }
        }
        ClipboardAction::Hash => Ok(format!("{:x}", Sha256::digest(s.as_bytes()))),
        ClipboardAction::Count => {
            let lines = if s.is_empty() {
                0
            } else {
                Regex::new(r"\r\n|\r|\n").unwrap().split(s).count()
            };
            Ok(format!(
                "Characters : {}\nWords      : {}\nLines      : {}\nBytes (utf-8): {}",
                s.chars().count(),
                s.split_whitespace().count(),
                lines,
                s.len()
            ))
        }
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
