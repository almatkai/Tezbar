//! Ignore-location Bitap and field normalization compatible with the project's Fuse.js configuration.
//! Algorithm adapted from Fuse.js (Apache-2.0); see fuse-license.txt. UTF-16 preserves JS masks.
use std::collections::HashMap;
fn chunk(text: &[u16], pattern: &[u16], threshold: f64, min_match: usize) -> (bool, f64) {
    let n = pattern.len();
    if n == 0 {
        return (false, 1.);
    }
    let mut alphabet = HashMap::new();
    for (i, c) in pattern.iter().enumerate() {
        *alphabet.entry(*c).or_insert(0u32) |= 1u32 << (n - i - 1);
    }
    let mut threshold = threshold;
    if text.windows(n).any(|w| w == pattern) {
        threshold = 0.;
    }
    let finish = text.len() + n;
    let mut last = vec![0u32; finish + 2];
    let mut best = false;
    let mut final_score: f64 = 1.;
    let mut matched = vec![false; text.len()];
    for errors in 0..n {
        let mut bits = vec![0u32; finish + 2];
        bits[finish + 1] = (1u32 << errors).wrapping_sub(1);
        for j in (1..=finish).rev() {
            let mask = text
                .get(j - 1)
                .and_then(|c| alphabet.get(c))
                .copied()
                .unwrap_or(0);
            if j - 1 < text.len() {
                matched[j - 1] = mask != 0;
            }
            bits[j] = ((bits[j + 1] << 1) | 1) & mask;
            if errors > 0 {
                bits[j] |= ((last[j + 1] | last[j]) << 1) | 1 | last[j + 1];
            }
            if bits[j] & (1u32 << (n - 1)) != 0 {
                final_score = errors as f64 / n as f64;
                if final_score <= threshold {
                    threshold = final_score;
                    best = true;
                    if j == 1 {
                        break;
                    }
                }
            }
        }
        if (errors + 1) as f64 / n as f64 > threshold {
            break;
        }
        last = bits;
    }
    if min_match > 1 {
        let mut run = 0;
        let mut good = false;
        for m in matched {
            run = if m { run + 1 } else { 0 };
            good |= run >= min_match;
        }
        best &= good;
    }
    (best, final_score.max(0.001))
}
pub fn score(text: &str, pattern: &str, threshold: f64, min_match: usize) -> Option<f64> {
    let text: Vec<_> = text.to_lowercase().encode_utf16().collect();
    let p: Vec<_> = pattern.to_lowercase().encode_utf16().collect();
    if p.is_empty() {
        return None;
    }
    if text == p {
        return Some(0.);
    }
    let mut chunks = vec![];
    if p.len() <= 32 {
        chunks.push(&p[..]);
    } else {
        let end = p.len() - p.len() % 32;
        for i in (0..end).step_by(32) {
            chunks.push(&p[i..i + 32]);
        }
        if p.len() % 32 != 0 {
            chunks.push(&p[p.len() - 32..]);
        }
    }
    let mut any = false;
    let mut total = 0.;
    for p in &chunks {
        let (hit, s) = chunk(&text, p, threshold, min_match);
        any |= hit;
        total += s;
    }
    any.then_some(total / chunks.len() as f64)
}
pub fn field_norm(s: &str) -> f64 {
    let count = s.split(' ').filter(|s| !s.is_empty()).count().max(1);
    (1000. / (count as f64).sqrt() + 0.5).floor() / 1000.
}
