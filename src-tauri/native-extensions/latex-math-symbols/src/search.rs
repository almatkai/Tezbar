// Modified Rust translation of Fuse.js v6.6.2 BitapSearch/search/computeScore/norm.
// Copyright (c) 2022 Kiro Risk. Apache-2.0; see libraries/fuse.js/LICENSE and NOTICE.
// Only the actual source's two scalar keys and default fuzzy options are implemented.
use crate::Item;
use std::collections::HashMap;

fn units(s: &str) -> Vec<u16> {
    s.to_lowercase().encode_utf16().collect()
}

fn norm(s: &str) -> f64 {
    // Fuse uses /[^ ]+/g: newlines and tabs are not token separators.
    let tokens = s.split(' ').filter(|s| !s.is_empty()).count();
    ((1.0 / (tokens as f64).sqrt()) * 1000.0).round() / 1000.0
}

pub(crate) fn score(query: &str, item: &Item) -> Option<f64> {
    let pattern = units(query);
    let mut total = 1.0;
    let mut matched = false;
    for field in [&item.title, &item.subtitle] {
        if let Some(score) = field_score(&pattern, &units(field)) {
            // createIndex uses original keys, with weight 1 (not KeyStore's normalized keys).
            total *= if score == 0.0 { f64::EPSILON } else { score }.powf(norm(field));
            matched = true;
        }
    }
    matched.then_some(total)
}

fn field_score(pattern: &[u16], text: &[u16]) -> Option<f64> {
    if pattern == text {
        return Some(0.0);
    }
    if pattern.len() <= 32 {
        let (found, score) = bitap(text, pattern, 0);
        return found.then_some(score);
    }
    let mut chunks = Vec::new();
    let end = pattern.len() - pattern.len() % 32;
    for start in (0..end).step_by(32) {
        chunks.push((&pattern[start..start + 32], start));
    }
    if pattern.len() % 32 != 0 {
        let start = pattern.len() - 32;
        chunks.push((&pattern[start..], start));
    }
    // Bitap clamps every offset beyond this field to text.len(). Only there can
    // identical chunks share a result; earlier offsets must be evaluated separately.
    // Keep this cache local to the field and include unmatched results as well.
    let mut clamped_results: Vec<(&[u16], (bool, f64))> = Vec::new();
    let mut matched = false;
    let mut total = 0.0;
    for &(chunk, offset) in &chunks {
        let (found, score) = if offset < text.len() {
            bitap(text, chunk, offset)
        } else if let Some((_, result)) = clamped_results.iter().find(|(key, _)| *key == chunk) {
            *result
        } else {
            let result = bitap(text, chunk, offset);
            clamped_results.push((chunk, result));
            result
        };
        matched |= found;
        // Preserve every addition and the original denominator, even on cache hits.
        total += score;
    }
    matched.then_some(total / chunks.len() as f64)
}

fn accuracy(errors: usize, current: usize, expected: usize, length: usize) -> f64 {
    errors as f64 / length as f64 + current.abs_diff(expected) as f64 / 100.0
}

fn bitap(text: &[u16], pattern: &[u16], location: usize) -> (bool, f64) {
    let length = pattern.len();
    let expected = location.min(text.len());
    let mut threshold: f64 = 0.6;
    let mut from = expected;
    while from + length <= text.len() {
        let Some(relative) = text[from..].windows(length).position(|w| w == pattern) else {
            break;
        };
        let index = from + relative;
        threshold = threshold.min(accuracy(0, index, expected, length));
        from = index + length;
    }
    let mut alphabet = HashMap::new();
    for (i, &ch) in pattern.iter().enumerate() {
        *alphabet.entry(ch).or_insert(0u32) |= 1u32 << (length - i - 1);
    }
    let mut best = None;
    let mut last: Vec<u32> = Vec::new();
    let mut final_score: f64 = 1.0;
    let mut bin_max = length + text.len();
    let mask = 1u32 << (length - 1);
    for errors in 0..length {
        let mut bin_min = 0;
        let mut bin_mid = bin_max;
        while bin_min < bin_mid {
            if accuracy(errors, expected + bin_mid, expected, length) <= threshold {
                bin_min = bin_mid;
            } else {
                bin_max = bin_mid;
            }
            bin_mid = (bin_max - bin_min) / 2 + bin_min;
        }
        bin_max = bin_mid;
        let mut start = (expected as isize - bin_mid as isize + 1).max(1) as usize;
        let finish = (expected + bin_mid).min(text.len()) + length;
        let mut bits = vec![0u32; finish + 2];
        bits[finish + 1] = (1u32 << errors).wrapping_sub(1);
        let mut j = finish;
        while j >= start {
            let current = j - 1;
            let char_match = text
                .get(current)
                .and_then(|ch| alphabet.get(ch))
                .copied()
                .unwrap_or(0);
            bits[j] = ((bits[j + 1] << 1) | 1) & char_match;
            if errors > 0 {
                let next = last.get(j + 1).copied().unwrap_or(0);
                let here = last.get(j).copied().unwrap_or(0);
                bits[j] |= ((next | here) << 1) | 1 | next;
            }
            if bits[j] & mask != 0 {
                final_score = accuracy(errors, current, expected, length);
                if final_score <= threshold {
                    threshold = final_score;
                    best = Some(current);
                    if current <= expected {
                        break;
                    }
                    start = (2 * expected).saturating_sub(current).max(1);
                }
            }
            j -= 1;
        }
        if accuracy(errors + 1, expected, expected, length) > threshold {
            break;
        }
        last = bits;
    }
    (best.is_some(), final_score.max(0.001))
}

#[cfg(test)]
mod tests {
    use super::{bitap, field_score, units};

    // Original, uncached Fuse chunk evaluation: independent of the cache path.
    fn uncached_field_score(pattern: &[u16], text: &[u16]) -> Option<f64> {
        if pattern == text {
            return Some(0.0);
        }
        let mut chunks = Vec::new();
        if pattern.len() <= 32 {
            chunks.push((pattern, 0));
        } else {
            let end = pattern.len() - pattern.len() % 32;
            for start in (0..end).step_by(32) {
                chunks.push((&pattern[start..start + 32], start));
            }
            if pattern.len() % 32 != 0 {
                let start = pattern.len() - 32;
                chunks.push((&pattern[start..], start));
            }
        }
        let mut matched = false;
        let mut total = 0.0;
        for (chunk, offset) in &chunks {
            let (found, score) = bitap(text, chunk, *offset);
            matched |= found;
            total += score;
        }
        matched.then_some(total / chunks.len() as f64)
    }

    fn assert_exact(query: &str, field: &str) {
        let pattern = units(query);
        let text = units(field);
        assert_eq!(
            field_score(&pattern, &text).map(f64::to_bits),
            uncached_field_score(&pattern, &text).map(f64::to_bits),
            "query UTF-16 length {}, field {field:?}",
            pattern.len(),
        );
    }

    #[test]
    fn short_and_utf16_scores_are_bit_exact() {
        for query in [
            "a", "alpha", "ALPHA", "\\sum", "∑", "İ", "😀", "a😀B", "notfound",
        ] {
            for field in [
                "",
                "alpha",
                "\\alpha",
                "sum; summation",
                "İ a😀b ∑",
                "a\tb\nc",
            ] {
                assert_exact(query, field);
            }
        }
        assert_exact("", ""); // Nonempty fields never receive an empty query in production.
    }

    #[test]
    fn overlapping_tail_and_offsets_are_bit_exact() {
        for length in [32, 33, 63, 64, 65, 95, 96, 97] {
            let repeated = "a".repeat(length);
            let mixed: String = (0..length)
                .map(|i| if i % 3 == 0 { 'b' } else { 'a' })
                .collect();
            for query in [&repeated, &mixed] {
                for field in [
                    "".to_string(),
                    "a".repeat(31),
                    "a".repeat(32),
                    "a".repeat(33),
                    "a".repeat(64),
                    "ab ".repeat(40),
                    query.to_string(),
                ] {
                    assert_exact(query, &field);
                }
            }
        }
        assert_exact(&"😀".repeat(33), "a😀 a😀 a😀"); // 66 UTF-16 units, not 33.
    }

    #[test]
    fn unmatched_chunks_still_contribute_to_total_and_denominator() {
        let query = format!("{}{}{}", "a".repeat(32), "z".repeat(32), "a".repeat(32));
        let text = units(&"a".repeat(32));
        assert!(bitap(&text, &units(&"a".repeat(32)), 32).0);
        assert!(!bitap(&text, &units(&"z".repeat(32)), 32).0);
        assert_exact(&query, "a");
        assert_exact(&query, &"a".repeat(32));
    }

    #[test]
    fn repeated_and_mixed_4096_units_are_bit_exact() {
        for query in [
            "a".repeat(4096),
            "z".repeat(4096),
            "😀".repeat(2048),
            format!("{}{}", "a".repeat(32), "z".repeat(32)).repeat(64),
            "alpha summation infinity omega "
                .repeat(137)
                .chars()
                .take(4096)
                .collect(),
        ] {
            assert_eq!(query.encode_utf16().count(), 4096);
            for field in ["a", "\\alpha", "sum; summation", "😀 alpha infinity"] {
                assert_exact(&query, field);
            }
        }
    }
}
