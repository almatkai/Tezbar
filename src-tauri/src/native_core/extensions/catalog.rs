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
        let entries = if cfg!(target_os = "windows") {
            let values: Vec<Value> = serde_json::from_str(include_str!("loved_extensions.json"))
                .map_err(|e| e.to_string())?;
            values
                .into_iter()
                .filter(|v| query.is_empty() || v.to_string().to_lowercase().contains(&query))
                .collect::<Vec<_>>()
        } else {
            let mut scored: Vec<_> = self
                .entries()
                .await?
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
}
