//! Owned project emoji table from renderer/emoji/emojiData.ts; no external corpus.
use serde::Deserialize;
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct EmojiEntry {
    pub char: String,
    pub name: String,
    pub category: String,
    pub keywords: Vec<String>,
}
pub static DATA: LazyLock<Vec<EmojiEntry>> =
    LazyLock::new(|| serde_json::from_str(include_str!("emoji_data.json")).unwrap());
fn normalize(s: &str) -> String {
    s.to_lowercase()
        .nfd()
        .filter(|c| !('\u{0300}'..='\u{036f}').contains(c))
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect()
}
fn stem(s: &str) -> &str {
    for (suffix, min) in [("ing", 5), ("ed", 4), ("es", 4), ("s", 3)] {
        if s.chars().count() > min && s.ends_with(suffix) {
            return &s[..s.len() - suffix.len()];
        }
    }
    s
}
fn related(s: &str) -> &'static [&'static str] {
    match s {
        "angry" => &["mad", "furious", "rage", "annoyed"],
        "happy" => &["joy", "smile", "glad", "cheerful"],
        "sad" => &["cry", "upset", "down", "depressed"],
        "love" => &["heart", "romance", "affection", "crush"],
        "cool" => &["awesome", "nice", "fire", "dope"],
        "sus" => &["suspicious", "doubt", "weird", "sketchy"],
        "laugh" => &["lol", "haha", "lmao", "funny"],
        "party" => &["celebrate", "birthday", "festive", "hype"],
        "tired" => &["sleepy", "exhausted", "drained", "fatigue"],
        "scared" => &["afraid", "fear", "anxious", "panic"],
        "fire" => &["hot", "lit", "flame", "trending"],
        _ => &[],
    }
}
pub fn search(query: &str, category: Option<&str>, limit: usize) -> Vec<&'static EmojiEntry> {
    if query.len() > 4096 {
        return vec![];
    }
    let q = normalize(query);
    let mut terms = vec![];
    for t in q.split_whitespace().take(32) {
        if ["the", "a", "an", "of", "to", "and", "for", "is", "it"].contains(&t) {
            continue;
        }
        for word in [t, stem(t)]
            .into_iter()
            .chain(related(t).iter().copied())
            .chain(
                if related(t).is_empty() {
                    related(stem(t))
                } else {
                    &[]
                }
                .iter()
                .copied(),
            )
        {
            if !terms.contains(&word) {
                terms.push(word);
            }
        }
    }
    let assembled = terms.join(" ");
    let q = q.trim();
    let mut hits: Vec<_> = DATA
        .iter()
        .enumerate()
        .filter(|(_, e)| category.is_none_or(|c| c == "All" || e.category == c))
        .filter_map(|(i, e)| {
            if query.trim().is_empty() || assembled.is_empty() {
                return Some((0., i, e));
            }
            let mut matched = false;
            let mut score = 1.;
            for (field, weight) in std::iter::once((e.name.as_str(), 0.35))
                .chain(e.keywords.iter().map(|s| (s.as_str(), 0.4)))
                .chain([(e.category.as_str(), 0.15), (e.char.as_str(), 0.1)])
            {
                if let Some(s) = super::fuzzy::score(field, &assembled, 0.28, 1) {
                    matched = true;
                    score *= if s == 0. { f64::EPSILON } else { s }
                        .powf(weight * super::fuzzy::field_norm(field));
                }
            }
            if !matched {
                return None;
            }
            let name = normalize(&e.name);
            if name == q {
                score *= 0.2;
            } else if name.starts_with(q) {
                score *= 0.4;
            }
            if e.keywords.iter().any(|k| normalize(k) == q) {
                score *= 0.5;
            }
            Some((score, i, e))
        })
        .collect();
    hits.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    hits.into_iter()
        .take(limit.min(DATA.len()))
        .map(|(_, _, e)| e)
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn table_and_aliases() {
        assert!(DATA.len() > 200);
        assert_eq!(search("", None, 1)[0].char, "😀");
        assert!(search("happy", Some("Smileys"), 20).is_empty());
        assert!(search("happy", Some("Flags"), 20).is_empty());
        assert_eq!(search("the", None, 20).len(), 20);
    }
}
#[cfg(test)]
mod parity {
    use super::*;
    #[test]
    fn ts_top_twenty_fixtures() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("emoji_fixtures.json")).unwrap();
        for c in cases.as_array().unwrap() {
            let q = c["query"].as_str().unwrap();
            let cat = c["category"].as_str().unwrap();
            let got: Vec<_> = search(q, Some(cat), 20)
                .iter()
                .map(|e| e.char.as_str())
                .collect();
            let expected: Vec<_> = c["expected"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap())
                .collect();
            assert_eq!(got, expected, "query {q:?}, category {cat}");
        }
    }
}
