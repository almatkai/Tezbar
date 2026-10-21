use crate::native_core::common::{now_ms, read_json, write_json_atomic, CoreContext, CoreResult};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

#[derive(Clone)]
pub struct Catalog {
    ctx: CoreContext,
    cache: Arc<tokio::sync::Mutex<Option<(u64, Vec<Value>)>>>,
}

impl Catalog {
    pub fn new(ctx: CoreContext) -> Self {
        Self {
            ctx,
            cache: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    async fn entries(&self) -> Result<Vec<Value>, String> {
        let mut cache = self.cache.lock().await;
        if let Some((time, entries)) = cache
            .as_ref()
            .filter(|(time, _)| now_ms().saturating_sub(*time) < 3_600_000)
        {
            let _ = time;
            return Ok(entries.clone());
        }
        let path = self.ctx.paths.data_dir.join("extension-catalog.json");
        let read_path = path.clone();
        let disk = tokio::task::spawn_blocking(move || read_json(&read_path).ok())
            .await
            .map_err(|e| e.to_string())?;
        if let Some(value) = disk.as_ref().filter(|v| {
            v["version"].as_u64().unwrap_or(0) >= 6
                && now_ms().saturating_sub(v["fetchedAt"].as_u64().unwrap_or(0)) < 3_600_000
        }) {
            let entries = normalize(&value["entries"]);
            if !entries.is_empty() {
                *cache = Some((now_ms(), entries.clone()));
                return Ok(entries);
            }
        }
        let base = std::env::var("RAYMES_EXTENSION_API_URL")
            .unwrap_or_else(|_| "https://api.supercmd.sh".into());
        let response = async {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .map_err(|e| e.to_string())?
                .get(format!("{}/extensions/catalog", base.trim_end_matches('/')))
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?
                .json::<Value>()
                .await
                .map_err(|e| e.to_string())
        }
        .await;
        let entries = match response {
            Ok(value) => {
                let entries = normalize(&value);
                let saved = json!({"version":6,"fetchedAt":now_ms(),"entries":entries});
                tokio::task::spawn_blocking(move || write_json_atomic(&path, &saved))
                    .await
                    .map_err(|e| e.to_string())??;
                entries
            }
            Err(error) => {
                let entries = disk
                    .as_ref()
                    .map(|v| normalize(&v["entries"]))
                    .unwrap_or_default();
                if entries.is_empty() {
                    return Err(format!("Extension catalog is unavailable: {error}"));
                }
                entries
            }
        };
        *cache = Some((now_ms(), entries.clone()));
        Ok(entries)
    }

    pub async fn search(&self, channel: &str, args: &[Value]) -> CoreResult {
        let body = args.first().cloned().unwrap_or(Value::Null);
        let options = if body.is_object() {
            &body
        } else {
            args.get(1).unwrap_or(&Value::Null)
        };
        let query = body
            .as_str()
            .or_else(|| body["query"].as_str())
            .unwrap_or("")
            .trim()
            .to_lowercase();
        let offset = options["offset"].as_u64().unwrap_or(0).min(1_000_000) as usize;
        let limit = options["limit"].as_u64().unwrap_or(25).clamp(1, 1000) as usize;
        let loved: Vec<Value> = serde_json::from_str(include_str!("loved_extensions.json"))
            .map_err(|e| e.to_string())?;
        let mut entries = if cfg!(target_os = "windows") {
            loved
                .clone()
                .into_iter()
                .filter(|v| query.is_empty() || v.to_string().to_lowercase().contains(&query))
                .collect::<Vec<_>>()
        } else {
            let mut scored: Vec<_> = self
                .entries()
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|v| {
                    let score = score(&v, &query);
                    (v, score)
                })
                .filter(|(_, score)| *score > 0.)
                .collect();
            scored.sort_by(|(a, sa), (b, sb)| {
                sb.total_cmp(sa)
                    .then_with(|| b["installCount"].as_u64().cmp(&a["installCount"].as_u64()))
                    .then_with(|| a["title"].as_str().cmp(&b["title"].as_str()))
            });
            scored.into_iter().map(|(v, _)| manifest(&v)).collect()
        };
        if !cfg!(target_os = "windows") {
            entries = merge_own(loved, entries, &query);
        }
        let entries = merge_native(entries, &query);
        if channel == "extensions:searchStore" {
            return Ok(json!(entries));
        }
        let total = entries.len();
        let items: Vec<_> = entries.into_iter().skip(offset).take(limit).collect();
        Ok(
            json!({"hasMore":offset + items.len() < total,"items":items,"total":total,"offset":offset,"limit":limit}),
        )
    }
}

// Keep curated repositories and command metadata, enriched with live store data.
fn merge_own(loved: Vec<Value>, mut entries: Vec<Value>, query: &str) -> Vec<Value> {
    let live: std::collections::HashMap<String, &Value> = entries
        .iter()
        .filter_map(|item| item["id"].as_str().map(|id| (id.to_owned(), item)))
        .collect();
    let mut own: Vec<Value> = loved
        .into_iter()
        .filter(|item| query.is_empty() || score_native(item, query) > 0.)
        .map(|mut item| {
            if let Some(source) = item["id"].as_str().and_then(|id| live.get(id)) {
                for field in [
                    "downloadCount",
                    "iconUrl",
                    "authorIconUrl",
                    "screenshotUrls",
                ] {
                    if !source[field].is_null() {
                        item[field] = source[field].clone();
                    }
                }
            }
            item
        })
        .collect();
    let ids: std::collections::HashSet<String> = own
        .iter()
        .filter_map(|item| item["id"].as_str().map(str::to_owned))
        .collect();
    entries.retain(|item| !ids.contains(item["id"].as_str().unwrap_or("")));
    own.extend(entries);
    own
}

// Merge before pagination; a complete replacement takes precedence, while a
// partial replacement leaves the V1 entry reachable for remaining commands.
fn merge_native(mut entries: Vec<Value>, query: &str) -> Vec<Value> {
    let mut native: Vec<_> = super::v2_catalog::manifests()
        .into_iter()
        .filter(|v| v["status"] != "unavailable")
        .filter(|v| query.is_empty() || score_native(v, &query) > 0.)
        .collect();
    native.sort_by(|a, b| {
        score_native(b, &query)
            .total_cmp(&score_native(a, &query))
            .then_with(|| a["name"].as_str().cmp(&b["name"].as_str()))
    });
    let replaced: std::collections::HashSet<String> = native
        .iter()
        .filter(|v| v["status"] == "complete")
        .flat_map(|v| {
            let mut ids: Vec<String> = v["replaces"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            if let Some(slug) = v["slug"].as_str() {
                ids.push(format!("com.tezbar.{slug}"));
            }
            ids
        })
        .collect();
    entries.retain(|v| !replaced.contains(v["id"].as_str().unwrap_or("")));
    let mut native: Vec<Value> = native
        .into_iter()
        .map(super::v2_catalog::store_item)
        .collect();
    native.extend(entries);
    let mut seen = std::collections::HashSet::new();
    native.retain(|item| {
        item["id"]
            .as_str()
            .is_some_and(|id| seen.insert(id.to_owned()))
    });
    native
}

fn score_native(v: &Value, query: &str) -> f64 {
    if query.is_empty() {
        return 1.;
    }
    let mut value = v.clone();
    value["title"] = v["name"].clone();
    score(&value, query)
}

fn normalize(value: &Value) -> Vec<Value> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter(|v| v["name"].as_str().is_some_and(|s| !s.is_empty()))
        .map(|v| {
            let mut entry = v.clone();
            for (camel, snake) in [
                ("iconUrl", "icon_url"),
                ("screenshotUrls", "screenshot_urls"),
                ("installCount", "install_count"),
            ] {
                if entry[camel].is_null() {
                    entry[camel] = v[snake].clone();
                }
            }
            entry
        })
        .collect()
}

fn manifest(v: &Value) -> Value {
    let name = v["name"].as_str().unwrap_or("");
    let author = v["author"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| v["contributors"][0].as_str())
        .unwrap_or("Raycast Community");
    json!({"id":format!("raycast.{name}"),"name":v["title"].as_str().filter(|s|!s.is_empty()).unwrap_or(name),
        "description":v["description"].as_str().unwrap_or(""),"author":author,"version":"latest",
        "repository":format!("https://github.com/raycast/extensions/tree/main/extensions/{name}"),
        "downloadCount":v["installCount"],"icon":v["icon"],"iconUrl":v["iconUrl"],
        "authorIconUrl":super::registry::avatar(&json!(author)),"screenshotUrls":v["screenshotUrls"],
        "categories":v["categories"],"commands":v["commands"],"owner":author})
}

fn token_score(value: &str, query: &str, exact: f64, prefix: f64) -> f64 {
    let tokens: Vec<_> = value
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|v| !v.is_empty())
        .collect();
    if tokens.contains(&query) {
        exact
    } else if tokens.iter().any(|t| t.starts_with(query)) {
        prefix
    } else {
        0.
    }
}

fn score(v: &Value, query: &str) -> f64 {
    if query.is_empty() {
        return 1.;
    }
    let name = v["name"].as_str().unwrap_or("").to_lowercase();
    let title = v["title"].as_str().unwrap_or("").to_lowercase();
    let description = v["description"].as_str().unwrap_or("").to_lowercase();
    let mut score = 0.;
    if name == query || title == query {
        score += 10000.;
    }
    if name.starts_with(query) || title.starts_with(query) {
        score += 9000.;
    }
    for (text, exact, prefix) in [
        (name.clone(), 8200., 7200.),
        (title.clone(), 8000., 7000.),
        (v["categories"].to_string().to_lowercase(), 3000., 2200.),
        (
            v["author"].as_str().unwrap_or("").to_lowercase(),
            2200.,
            1600.,
        ),
        (
            v["commands"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| {
                    format!(
                        "{} {}",
                        c["name"].as_str().unwrap_or(""),
                        c["title"].as_str().unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase(),
            1800.,
            1200.,
        ),
        (description.clone(), 900., 550.),
    ] {
        score += token_score(&text, query, exact, prefix);
    }
    if query.len() >= 4 {
        if name.contains(query) || title.contains(query) {
            score += 1400.;
        }
        if description.contains(query) {
            score += 350.;
        }
    }
    let installs = v["installCount"].as_f64().unwrap_or(0.);
    if score > 0. && installs > 0. {
        score += (installs.log10() * 80.).min(500.);
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn curated_replacements_keep_live_counts_and_artwork() {
        let items = merge_own(
            vec![
                json!({"id":"raycast.example","repository":"https://github.com/tezbar/example","commands":[{"name":"own"}]}),
                json!({"id":"com.tezbar.translate","repository":"https://github.com/tezbar/translate"}),
            ],
            vec![
                json!({"id":"raycast.example","repository":"upstream","downloadCount":42000,"iconUrl":"https://cdn/icon.png","commands":[{"name":"upstream"}]}),
                json!({"id":"raycast.other"}),
            ],
            "",
        );
        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["downloadCount"], 42000);
        assert_eq!(items[0]["iconUrl"], "https://cdn/icon.png");
        assert_eq!(items[0]["repository"], "https://github.com/tezbar/example");
        assert_eq!(items[0]["commands"][0]["name"], "own");
        assert_eq!(items[1]["id"], "com.tezbar.translate");
    }
    #[test]
    fn short_queries_do_not_match_unrelated_substrings() {
        assert_eq!(
            score(
                &json!({"name":"search","title":"Search","description":"search"}),
                "arc"
            ),
            0.
        );
        assert!(score(&json!({"name":"arc","title":"Arc"}), "arc") > 10000.);
    }
    #[test]
    fn own_v1_catalog_keeps_declared_ids_and_command_count() {
        let loved: Vec<Value> =
            serde_json::from_str(include_str!("loved_extensions.json")).unwrap();
        assert_eq!(loved.len(), 98);
        assert_eq!(
            loved
                .iter()
                .map(|v| v["commands"].as_array().unwrap().len())
                .sum::<usize>(),
            247
        );
        let ids: std::collections::HashSet<_> =
            loved.iter().filter_map(|v| v["id"].as_str()).collect();
        assert_eq!(ids.len(), loved.len());
        assert!(ids.contains("com.tezbar.google-translate"));
        assert!(ids.contains("com.tezbar.visual-studio-code"));
        assert_eq!(
            loved
                .iter()
                .find(|v| v["id"] == "com.tezbar.google-translate")
                .unwrap()["slug"],
            "translate"
        );
    }
    #[test]
    fn missing_raycast_catalog_still_offers_installable_native_item() {
        let items = merge_native(vec![], "perplexity");
        let native = items
            .iter()
            .find(|v| v["id"] == "com.tezbar.native.perplexity")
            .unwrap();
        assert_eq!(native["author"], "Tezbar");
        assert_eq!(native["runtime"], "rust");
        assert_eq!(native["nativeV2"]["schemaVersion"], 2);
        assert_eq!(native["nativeV2"]["commands"][0]["runtime"], "rust");
        assert_eq!(native["platforms"], json!(["macOS", "Windows"]));
        let temp = tempfile::tempdir().unwrap();
        let ctx = CoreContext::headless(crate::native_core::common::CorePaths {
            data_dir: temp.path().into(),
            config_dir: temp.path().join("config"),
            resource_dir: temp.path().join("resources"),
            temp_dir: temp.path().join("tmp"),
            home_dir: temp.path().into(),
        });
        let installed = super::super::v2_catalog::install(&ctx, native["id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(installed["nativeV2"]["id"], native["id"]);
        assert!(super::super::v2_catalog::installed(
            &ctx,
            native["id"].as_str().unwrap()
        ));
    }
    #[test]
    fn complete_native_item_deduplicates_legacy_before_pagination() {
        let entries = merge_native(
            vec![
                json!({"id":"raycast.perplexity","name":"Perplexity"}),
                json!({"id":"raycast.perplexity","name":"Duplicate"}),
            ],
            "perplexity",
        );
        assert_eq!(
            entries
                .iter()
                .filter(|v| v["id"] == "com.tezbar.native.perplexity")
                .count(),
            1
        );
        assert_eq!(
            entries
                .iter()
                .filter(|v| v["id"] == "raycast.perplexity")
                .count(),
            0
        );
    }
}
