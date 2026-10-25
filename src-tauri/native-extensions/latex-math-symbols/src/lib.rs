//! Offline catalogue and Fuse.js 6.6.2 search port. No OS or network effects.
mod search;

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

pub const SLUG: &str = "latex-math-symbols";
pub const ID: &str = "com.tezbar.native.latex-math-symbols";
pub const MAX_QUERY_UNITS: usize = 4096;
pub const RECORD_COUNT: usize = 131;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Variant {
    pub command: String,
    pub example: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Symbol {
    pub command: String,
    pub example: String,
    pub descriptions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variants: Option<Vec<Variant>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Item {
    pub id: usize,
    pub title: String,
    pub example: String,
    pub subtitle: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Hit {
    pub item: Item,
    /// Empty search bypasses Fuse in the source and has no score.
    pub score: Option<f64>,
}

pub fn symbols() -> Result<&'static [Symbol], String> {
    static DATA: OnceLock<Result<Vec<Symbol>, String>> = OnceLock::new();
    DATA.get_or_init(|| {
        let data: Vec<Symbol> = serde_json::from_str(include_str!("../data/commands.json"))
            .map_err(|_| "Bundled symbol data is invalid".to_string())?;
        if data.len() != RECORD_COUNT
            || data.iter().any(|s| {
                s.command.is_empty()
                    || s.example.is_empty()
                    || s.descriptions.is_empty()
                    || s.descriptions.iter().any(String::is_empty)
            })
        {
            return Err("Bundled symbol catalogue is incomplete".into());
        }
        Ok(data)
    })
    .as_ref()
    .map(Vec::as_slice)
    .map_err(Clone::clone)
}

pub fn items() -> Result<Vec<Item>, String> {
    Ok(symbols()?
        .iter()
        .enumerate()
        .map(|(id, s)| Item {
            id,
            title: s.command.clone(),
            example: s.example.clone(),
            subtitle: s.descriptions.join("; "),
        })
        .collect())
}

pub fn search(query: &str) -> Result<Vec<Hit>, String> {
    if query.encode_utf16().count() > MAX_QUERY_UNITS {
        return Err(format!(
            "Search exceeds {MAX_QUERY_UNITS} UTF-16 units; shorten the query"
        ));
    }
    let items = items()?;
    if query.is_empty() {
        return Ok(items
            .into_iter()
            .map(|item| Hit { item, score: None })
            .collect());
    }
    let mut hits: Vec<_> = items
        .into_iter()
        .filter_map(|item| {
            search::score(query, &item).map(|score| Hit {
                item,
                score: Some(score),
            })
        })
        .collect();
    hits.sort_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.item.id.cmp(&b.item.id))
    });
    Ok(hits)
}

/// Process nonce plus monotonic generations ensure discarded staged renders cannot revive IDs.
pub fn fresh_token() -> Result<String, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NONCE: OnceLock<String> = OnceLock::new();
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let nonce = NONCE.get_or_init(|| uuid::Uuid::new_v4().to_string());
    let generation = SEQUENCE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| "Session capability sequence exhausted; restart Tezbar".to_string())?;
    Ok(format!("{nonce}:{generation}"))
}
