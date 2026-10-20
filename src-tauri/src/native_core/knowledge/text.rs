use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

pub fn hash(text: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(text.as_ref()))
}
fn feature_hash(value: &[u16], seed: u32) -> u32 {
    value
        .iter()
        .fold(seed, |h, c| (h ^ *c as u32).wrapping_mul(16777619))
}
pub fn js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}
pub fn embed(value: &str) -> Vec<f64> {
    let normalized: String = value.to_lowercase().nfkc().collect();
    let words: Vec<_> = normalized
        .split(js_whitespace)
        .filter(|s| !s.is_empty())
        .collect();
    let mut vector = vec![0f32; 384];
    let mut add = |prefix: &str, word: &[u16]| {
        let mut feature: Vec<_> = prefix.encode_utf16().collect();
        feature.extend_from_slice(word);
        let bucket = feature_hash(&feature, 2166136261) as usize % 384;
        vector[bucket] += if feature_hash(&feature, 0x9e3779b9) & 1 == 0 {
            1.0
        } else {
            -1.0
        };
    };
    for word in words {
        let units: Vec<_> = word.encode_utf16().collect();
        add("w:", &units);
        for size in 2..=4 {
            for part in units.windows(size) {
                add("c:", part);
            }
        }
    }
    let magnitude = vector
        .iter()
        .map(|x| (*x as f64).powi(2))
        .sum::<f64>()
        .sqrt();
    let magnitude = if magnitude == 0.0 { 1.0 } else { magnitude };
    vector.into_iter().map(|x| x as f64 / magnitude).collect()
}
pub fn cosine(a: &[f64], b: &[f64]) -> f64 {
    let (mut dot, mut aa, mut bb) = (0., 0., 0.);
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        aa += x * x;
        bb += y * y;
    }
    if aa == 0. || bb == 0. {
        0.
    } else {
        dot / (aa * bb).sqrt()
    }
}
pub fn encode(v: &[f64]) -> Vec<u8> {
    v.iter().flat_map(|x| (*x as f32).to_le_bytes()).collect()
}
pub fn binary(v: &[f64]) -> Vec<u8> {
    let mut bytes = vec![0; (v.len() + 7) / 8];
    for (i, x) in v.iter().enumerate() {
        if *x >= 0. {
            bytes[i / 8] |= 1 << (i % 8);
        }
    }
    bytes
}
pub fn decode(v: rusqlite::types::ValueRef<'_>) -> Vec<f64> {
    match v {
        rusqlite::types::ValueRef::Text(s) => serde_json::from_slice(s).unwrap_or_default(),
        rusqlite::types::ValueRef::Blob(s) if s.len() % 4 == 0 => s
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()) as f64)
            .collect(),
        _ => vec![],
    }
}
pub fn truncate(text: &str, limit: usize) -> String {
    String::from_utf16_lossy(&text.encode_utf16().take(limit).collect::<Vec<_>>())
}
pub fn redact(input: &str) -> String {
    use std::sync::OnceLock;
    static RULES: OnceLock<Vec<(regex::Regex, &str)>> = OnceLock::new();
    let rules = RULES.get_or_init(|| {
        vec![
            (
                r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
                "[REDACTED_PRIVATE_KEY]",
            ),
            (r"\bsk-[A-Za-z0-9_-]{12,}\b", "[REDACTED_API_KEY]"),
            (r"\bgh[pousr]_[A-Za-z0-9_]{12,}\b", "[REDACTED_TOKEN]"),
            (r"\bAKIA[0-9A-Z]{16}\b", "[REDACTED_AWS_ACCESS_KEY]"),
            (
                r"(?i)\b(bearer\s+)[A-Za-z0-9._~+/\-]{16,}",
                "${1}[REDACTED_TOKEN]",
            ),
            (
                r"(?i)\b(password|passwd|api[_-]?key|secret|token)\s*[=:]\s*([^\s,;]+)",
                "${1}=[REDACTED]",
            ),
        ]
        .into_iter()
        .map(|(a, b)| (regex::Regex::new(a).unwrap(), b))
        .collect()
    });
    rules.iter().fold(input.to_string(), |s, (r, v)| {
        r.replace_all(&s, *v).into_owned()
    })
}
#[cfg(test)]
pub fn chunks(source: &str, pages: &[Value], embedding: bool) -> Vec<Value> {
    chunks_checked(source, pages, embedding, || Ok(())).unwrap()
}
pub fn chunks_checked(
    source: &str,
    pages: &[Value],
    embedding: bool,
    mut checkpoint: impl FnMut() -> Result<(), String>,
) -> Result<Vec<Value>, String> {
    let mut chunks = vec![];
    for page in pages {
        let extracted = page["extractedText"].as_str().unwrap_or("").trim();
        let ocr = page["ocr"]["text"].as_str().unwrap_or("").trim();
        let text = if !extracted.is_empty() && !ocr.is_empty() && !extracted.contains(ocr) {
            format!("{extracted}\n\n{ocr}")
        } else if !extracted.is_empty() {
            extracted.into()
        } else {
            ocr.into()
        };
        let text = text.replace('\0', "");
        let text = regex::Regex::new(r"[ \t]+\n")
            .unwrap()
            .replace_all(&text, "\n");
        let text = regex::Regex::new(r"\n{3,}")
            .unwrap()
            .replace_all(&text, "\n\n")
            .trim()
            .to_string();
        let units: Vec<_> = text.encode_utf16().collect();
        let mut start = 0;
        while start < units.len() && chunks.len() < 4000 {
            checkpoint()?;
            let mut end = units.len().min(start + 1200);
            if end < units.len() {
                let paragraph = (0..end.min(units.len().saturating_sub(1)) + 1)
                    .rev()
                    .find(|&i| units.get(i) == Some(&10) && units.get(i + 1) == Some(&10));
                let sentence = (0..end.min(units.len().saturating_sub(1)) + 1)
                    .rev()
                    .find(|&i| units.get(i) == Some(&46) && units.get(i + 1) == Some(&32));
                let boundary = paragraph.max(sentence);
                if let Some(boundary) = boundary {
                    if boundary > start + 660 {
                        end = boundary + usize::from(Some(boundary) == sentence);
                    }
                }
            }
            let value = String::from_utf16_lossy(&units[start..end])
                .trim()
                .to_string();
            if !value.is_empty() {
                let page = page["pageNumber"].as_u64().unwrap_or(0);
                let mut chunk = json!({"id":hash(format!("{source}:{page}:{start}:{value}")),"pageNumber":page,"text":value,"startOffset":start,"endOffset":end});
                if embedding {
                    chunk["embedding"] = json!(embed(&value));
                }
                chunks.push(chunk);
            }
            if end >= units.len() {
                break;
            }
            start = (start + 1).max(end.saturating_sub(180));
        }
    }
    Ok(chunks)
}
pub fn profile(depth: &str) -> String {
    // Field order is part of the legacy content-addressed cache key.
    let caps = if depth == "basic" {
        r#"["extracted-text","chunks"]"#
    } else {
        r#"["extracted-text","ocr","chunks","text-embeddings"]"#
    };
    format!(
        r#"{{"depth":"{depth}","capabilities":{caps},"maxPagesPerDocument":{},"maxOcrPagesPerDocument":{},"ocrEveryPage":{}}}"#,
        if depth == "basic" { "20" } else { "null" },
        if depth == "basic" {
            "0"
        } else if depth == "smart" {
            "20"
        } else {
            "null"
        },
        depth == "deep"
    )
}
