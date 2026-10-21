//! Native V2 port of josephschmitt's Raycast `gif-search` (MIT), based on the
//! installed package.json and .sc-build/{search,favorites,recents}.js.
//! Provider mapping, preferences and action semantics originate in that source.
//! This version uses Rust HTTP/animation/storage; no bundled JavaScript runs.
//! The installed revision uses the Raycast GIPHY/Klipy proxy (no embedded key).
use super::v2_sdk;
use crate::native_core::common::{now_ms, CoreContext, CoreResult};
use image::{AnimationDecoder, ImageDecoder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::oneshot;

const SLUG: &str = "gif-search";
const PROVIDERS: [&str; 4] = ["giphy", "giphy-clips", "klipy", "finergifs"];
const SERVICES: [&str; 6] = [
    "giphy",
    "giphy-clips",
    "klipy",
    "finergifs",
    "favorites",
    "recents",
];
const ITEMS: &str = "gifSearchItems";
const USAGE: &str = "gifSearchUsage";
const MAX_MEDIA: usize = 100 * 1024 * 1024;
const MAX_JSON: usize = 8 * 1024 * 1024;
const MORE: &str = "gif-search:load-more";
const SERVICE: &str = "gif-search:service";
const BACK: &str = "gif-search:back";
const OPEN_DOWNLOAD: &str = "gif-search:open-download";
const REVEAL_DOWNLOAD: &str = "gif-search:reveal-download";
const RETRY: &str = "gif-search:retry";
const MIGRATED: &str = "_gifSearchV2Imported";

pub fn commands() -> &'static [&'static str] {
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        &["search", "favorites", "recents"]
    } else {
        &[]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Gif {
    service: String,
    id: String,
    title: String,
    url: String,
    gif_url: String,
    download_url: String,
    download_name: String,
    preview: String,
    large_preview: String,
    metadata: Value,
}

impl Gif {
    fn key(&self) -> String {
        format!("{}:{}", self.service, self.id)
    }

    fn action(&self, action: &str) -> String {
        format!(
            "gif-search:{action}:{}",
            v2_sdk::encode_component(&self.key())
        )
    }
}

#[derive(Debug)]
struct Page {
    gifs: Vec<Gif>,
    snapshots: Vec<Gif>,
    next: Option<String>,
    has_more: bool,
    offset: usize,
}

struct Pending {
    receiver: oneshot::Receiver<Result<Page, String>>,
    abort: tokio::task::AbortHandle,
}

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);
static REQUESTS: OnceLock<Mutex<HashMap<u64, Pending>>> = OnceLock::new();

fn requests() -> &'static Mutex<HashMap<u64, Pending>> {
    REQUESTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cancel(state: &mut Value) -> Result<(), String> {
    if let Some(id) = state["request"].as_u64() {
        if let Some(job) = requests()
            .lock()
            .map_err(|_| "GIF request lock failed")?
            .remove(&id)
        {
            job.abort.abort();
        }
    }
    state["request"] = Value::Null;
    Ok(())
}

fn string<'a>(value: &'a Value, key: &str, fallback: &'a str) -> &'a str {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback)
}

fn title(service: &str) -> &str {
    match service {
        "giphy" => "GIPHY GIFs",
        "giphy-clips" => "GIPHY Clips",
        "klipy" => "Klipy",
        "finergifs" => "Finer Gifs Club",
        "favorites" => "Favorites",
        "recents" => "Recent",
        _ => "GIFs",
    }
}

fn page_limit(prefs: &Value) -> usize {
    prefs["maxResults"]
        .as_u64()
        .or_else(|| prefs["maxResults"].as_str()?.trim().parse().ok())
        .unwrap_or(20)
        .clamp(1, 50) as usize
}

fn is_local(service: &str) -> bool {
    matches!(service, "favorites" | "recents")
}

fn columns(state: &Value, prefs: &Value) -> usize {
    let trending = string(state, "service", "giphy") != "favorites"
        && string(state, "service", "giphy") != "recents"
        && string(state, "query", "").is_empty();
    match string(
        prefs,
        if trending {
            "gridTrendingItemSize"
        } else {
            "gridItemSize"
        },
        "medium",
    ) {
        "small" => 8,
        "large" => 3,
        _ => 5,
    }
}

fn safe_url(value: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value).map_err(|_| "Invalid GIF URL")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("GIF URLs must use HTTPS without embedded credentials".into());
    }
    Ok(url)
}

fn clean_url(value: &str) -> Result<String, String> {
    let mut url = safe_url(value)?;
    url.set_query(None);
    Ok(url.to_string())
}

fn request_url(
    service: &str,
    query: &str,
    prefs: &Value,
    offset: usize,
    next: Option<&str>,
) -> Result<reqwest::Url, String> {
    let base = match service {
        "giphy" | "giphy-clips" => "https://gif-search.raycast.com/api/giphy",
        "klipy" => "https://gif-search.raycast.com/api/klipy",
        "finergifs" => "https://api.thefinergifs.club/search",
        _ => return Err("Unknown GIF provider".into()),
    };
    let mut url = safe_url(base)?;
    let limit = page_limit(prefs).to_string();
    {
        let mut params = url.query_pairs_mut();
        match service {
            "giphy" | "giphy-clips" => {
                params
                    .append_pair("limit", &limit)
                    .append_pair("offset", &offset.to_string())
                    .append_pair("lang", string(prefs, "giphyLocale", "en"));
                if service == "giphy-clips" {
                    params.append_pair("type", "videos");
                }
            }
            "klipy" => {
                params
                    .append_pair("limit", &limit)
                    .append_pair("locale", string(prefs, "klipyLocale", "en"))
                    .append_pair("media_filter", "gif,nanogif,tinygif");
                if let Some(next) = next {
                    params.append_pair("pos", next);
                }
            }
            "finergifs" => {
                params
                    .append_pair("size", &limit)
                    .append_pair("q.parser", "simple")
                    .append_pair("sort", "_score desc");
                if offset > 0 {
                    params.append_pair("start", &offset.to_string());
                }
            }
            _ => unreachable!(),
        }
        if !query.is_empty() {
            params.append_pair("q", query);
        }
    }
    Ok(url)
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::limited(4))
        .build()
        .map_err(|_| "Could not initialize GIF HTTP client".into())
}

async fn response_bytes(mut response: reqwest::Response, cap: usize) -> Result<Vec<u8>, String> {
    if !response.status().is_success() {
        return Err(format!(
            "GIF provider request failed (HTTP {})",
            response.status().as_u16()
        ));
    }
    if response
        .content_length()
        .is_some_and(|len| len > cap as u64)
    {
        return Err("GIF response exceeds the download limit".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Could not read GIF response")?
    {
        if bytes.len().saturating_add(chunk.len()) > cap {
            return Err("GIF response exceeds the download limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

// Test clients may target a loopback HTTP fixture. Production callers use only
// the HTTPS-only client and fixed provider URLs, never a preference base URL.
async fn fetch_json(
    client: &reqwest::Client,
    url: reqwest::Url,
    provider: &str,
) -> Result<Value, String> {
    let response = client.get(url).send().await.map_err(|_| {
        format!(
            "Could not reach {}. Check the connection and try again.",
            title(provider)
        )
    })?;
    let bytes = response_bytes(response, MAX_JSON).await?;
    serde_json::from_slice(&bytes).map_err(|_| "GIF provider returned invalid JSON".into())
}

async fn fetch_page(
    service: String,
    query: String,
    prefs: Value,
    offset: usize,
    next: Option<String>,
) -> Result<Page, String> {
    if service == "finergifs" && query.is_empty() {
        // The original provider explicitly has no trending endpoint.
        return Ok(Page {
            gifs: vec![],
            snapshots: vec![],
            next: None,
            has_more: false,
            offset,
        });
    }
    let url = request_url(&service, &query, &prefs, offset, next.as_deref())?;
    let response = fetch_json(&client()?, url, &service).await?;
    parse_page(&service, &response, offset, page_limit(&prefs))
}

fn slugify(value: &str) -> String {
    let mut result = String::new();
    for c in value.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c);
        } else if (c.is_whitespace() || c == '_' || c == '-') && !result.ends_with('-') {
            result.push('-');
        }
        if result.len() >= 100 {
            break;
        }
    }
    let slug = result.trim_matches('-');
    let reserved = matches!(slug, "con" | "prn" | "aux" | "nul")
        || (slug.len() == 4
            && (slug.starts_with("com") || slug.starts_with("lpt"))
            && matches!(slug.as_bytes()[3], b'1'..=b'9'));
    if slug.is_empty() {
        "gif".into()
    } else if reserved {
        format!("gif-{slug}")
    } else {
        slug.to_string()
    }
}

fn scalar(value: &Value) -> String {
    match value {
        Value::String(v) => v.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn map_gif(service: &str, value: &Value) -> Result<Gif, String> {
    let (id, name, page, media, download, preview, large, metadata) = match service {
        "giphy" | "giphy-clips" => {
            let image = &value["images"]["original"];
            let media = string(image, "url", "");
            let video = &value["video"]["assets"];
            let download = if service == "giphy-clips" {
                ["1080p", "720p", "360p"]
                    .iter()
                    .find_map(|k| video[*k]["url"].as_str())
                    .unwrap_or(media)
            } else {
                media
            };
            (
                scalar(&value["id"]),
                string(value, "title", string(value, "slug", "GIF")).to_string(),
                string(value, "url", media).to_string(),
                media.to_string(),
                download.to_string(),
                string(&value["images"]["preview_gif"], "url", media).to_string(),
                string(&value["images"]["fixed_height_small"], "url", media).to_string(),
                json!({"width":image["width"],"height":image["height"],"size":image["size"],
                    "labels":[{"title":"Created","text":value["import_datetime"]},{"title":"User","text":value["username"]}],
                    "links":[{"title":"Source","text":value["source_tld"],"target":value["source"]}],"tags":value["tags"]}),
            )
        }
        "klipy" => {
            let media = string(&value["media_formats"]["gif"], "url", "");
            let format = &value["media_formats"]["gif"];
            (
                scalar(&value["id"]),
                string(value, "title", string(value, "content_description", "GIF")).to_string(),
                string(value, "itemurl", media).to_string(),
                media.to_string(),
                media.to_string(),
                string(&value["media_formats"]["nanogif"], "url", media).to_string(),
                string(&value["media_formats"]["tinygif"], "url", media).to_string(),
                json!({"width":format["dims"][0],"height":format["dims"][1],"size":format["size"],
                    "labels":[{"title":"Created","text":value["created"]}],"tags":value["tags"]}),
            )
        }
        "finergifs" => {
            let id = string(&value["fields"], "fileid", "");
            let media = format!(
                "https://media.thefinergifs.club/{}.gif",
                v2_sdk::encode_component(id)
            );
            let mut labels = vec![];
            let bytes = id.as_bytes();
            if bytes.len() >= 6
                && bytes[0..2].iter().all(u8::is_ascii_digit)
                && bytes[2] == b'x'
                && bytes[3..5].iter().all(u8::is_ascii_digit)
                && bytes[5] == b'-'
            {
                labels.push(json!({"title":"Season","text":&id[0..2]}));
                labels.push(json!({"title":"Episode","text":&id[3..5]}));
            }
            (
                id.to_string(),
                string(&value["fields"], "text", "GIF").to_string(),
                media.clone(),
                media.clone(),
                media.clone(),
                media.clone(),
                media,
                json!({"labels":labels}),
            )
        }
        _ => return Err("Unknown GIF provider".into()),
    };
    if id.is_empty() || id.len() > 1024 {
        return Err("GIF provider returned an invalid GIF identifier".into());
    }
    for url in [&page, &media, &download, &preview, &large] {
        safe_url(url)?;
    }
    let ext = if reqwest::Url::parse(&download)
        .ok()
        .is_some_and(|u| u.path().to_ascii_lowercase().ends_with(".mp4"))
    {
        "mp4"
    } else {
        "gif"
    };
    Ok(Gif {
        service: service.into(),
        id,
        title: name.clone(),
        url: page,
        gif_url: media,
        download_url: download,
        download_name: format!("{}.{}", slugify(&name), ext),
        preview,
        large_preview: large,
        metadata,
    })
}

fn parse_page(service: &str, value: &Value, offset: usize, limit: usize) -> Result<Page, String> {
    if let Some(status) = value["meta"]["status"].as_u64() {
        if status >= 400 {
            return Err(format!(
                "GIF provider rejected the request (status {status})"
            ));
        }
    }
    let key = if matches!(service, "giphy" | "giphy-clips") {
        "data"
    } else {
        "results"
    };
    let values = value[key]
        .as_array()
        .ok_or("GIF provider response is missing its results array")?;
    let mut gifs = vec![];
    let mut seen = HashSet::new();
    for value in values {
        let gif = map_gif(service, value)?;
        if seen.insert(gif.key()) {
            gifs.push(gif);
        }
    }
    let next = value["next"].as_str().map(str::to_string);
    let consumed = offset.saturating_add(values.len());
    let has_more = if let Some(next) = &next {
        !next.is_empty()
    } else if let Some(total) = value["pagination"]["total_count"].as_u64() {
        !values.is_empty() && consumed < total as usize
    } else {
        !values.is_empty() && values.len() == limit
    };
    Ok(Page {
        gifs,
        snapshots: vec![],
        next,
        has_more,
        offset: consumed,
    })
}

fn missing_snapshots(store: &Value, service: &str) -> Vec<(String, Vec<String>)> {
    let providers = if is_local(service) {
        PROVIDERS.to_vec()
    } else {
        vec![service]
    };
    providers
        .into_iter()
        .filter_map(|provider| {
            let mut seen = HashSet::new();
            let ids = ids(store, provider, false)
                .into_iter()
                .chain(ids(store, provider, true))
                .filter(|id| {
                    seen.insert(id.clone())
                        && serde_json::from_value::<Gif>(
                            store[ITEMS][format!("{provider}:{id}")].clone(),
                        )
                        .is_err()
                })
                .collect::<Vec<_>>();
            (!ids.is_empty()).then_some((provider.to_string(), ids))
        })
        .collect()
}

fn lookup_url(service: &str, ids: &[String]) -> Result<reqwest::Url, String> {
    let base = match service {
        "giphy" | "giphy-clips" => "https://gif-search.raycast.com/api/giphy",
        "klipy" => "https://gif-search.raycast.com/api/klipy",
        "finergifs" => "https://api.thefinergifs.club/search",
        _ => return Err("Unknown GIF provider".into()),
    };
    let mut url = safe_url(base)?;
    if service == "finergifs" {
        url.query_pairs_mut()
            .append_pair("q", &ids.join("|"))
            .append_pair("q.parser", "simple")
            .append_pair("q.options", r#"{"fields":["fileid"]}"#)
            .append_pair("size", "10");
    } else {
        url.query_pairs_mut().append_pair("ids", &ids.join(","));
    }
    Ok(url)
}

async fn fetch_with_snapshots(
    service: String,
    query: String,
    prefs: Value,
    offset: usize,
    next: Option<String>,
    missing: Vec<(String, Vec<String>)>,
) -> Result<Page, String> {
    let mut page = if is_local(&service) {
        Page {
            gifs: vec![],
            snapshots: vec![],
            next: None,
            has_more: false,
            offset: 0,
        }
    } else {
        fetch_page(service, query, prefs, offset, next).await?
    };
    for (provider, ids) in missing {
        for ids in ids.chunks(if provider == "finergifs" { 10 } else { 50 }) {
            let value = fetch_json(&client()?, lookup_url(&provider, ids)?, &provider).await?;
            let result = parse_page(&provider, &value, 0, ids.len())?;
            // Only accept the requested provider/IDs, preserving collection order.
            let resolved: HashSet<String> = result.gifs.iter().map(|gif| gif.id.clone()).collect();
            if ids.iter().any(|id| !resolved.contains(id.as_str())) {
                return Err(format!(
                    "Some saved GIFs are no longer available from {}",
                    title(&provider)
                ));
            }
            page.snapshots
                .extend(result.gifs.into_iter().filter(|gif| ids.contains(&gif.id)));
        }
    }
    Ok(page)
}

fn begin(state: &mut Value, prefs: &Value, store: &Value, append: bool) -> Result<(), String> {
    cancel(state)?;
    let service = string(state, "service", "giphy").to_string();
    state["error"] = Value::Null;
    let missing = missing_snapshots(store, &service);
    if is_local(&service) && missing.is_empty() {
        return Ok(());
    }
    let query = string(state, "query", "").to_string();
    let offset = if append {
        state["offset"].as_u64().unwrap_or(0) as usize
    } else {
        0
    };
    let next = if append {
        state["next"].as_str().map(str::to_string)
    } else {
        None
    };
    if !append {
        state["results"] = json!([]);
        state["offset"] = json!(0);
        state["hasMore"] = json!(false);
        state["next"] = Value::Null;
    }
    let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let prefs = prefs.clone();
    let (sender, receiver) = oneshot::channel();
    let task = tokio::spawn(async move {
        let _ =
            sender.send(fetch_with_snapshots(service, query, prefs, offset, next, missing).await);
    });
    requests()
        .lock()
        .map_err(|_| "GIF request lock failed")?
        .insert(
            id,
            Pending {
                receiver,
                abort: task.abort_handle(),
            },
        );
    state["request"] = json!(id);
    state["error"] = Value::Null;
    Ok(())
}

async fn poll(ctx: &CoreContext, state: &mut Value) -> Result<bool, String> {
    let Some(id) = state["request"].as_u64() else {
        return Ok(false);
    };
    let result = {
        let mut pending = requests().lock().map_err(|_| "GIF request lock failed")?;
        let Some(job) = pending.get_mut(&id) else {
            state["request"] = Value::Null;
            return Ok(true);
        };
        match job.receiver.try_recv() {
            Ok(result) => {
                pending.remove(&id);
                result
            }
            Err(oneshot::error::TryRecvError::Empty) => return Ok(false),
            Err(oneshot::error::TryRecvError::Closed) => {
                pending.remove(&id);
                Err("GIF request was cancelled".into())
            }
        }
    };
    state["request"] = Value::Null;
    match result {
        Ok(page) => {
            if !page.snapshots.is_empty() {
                v2_sdk::update_store(ctx, SLUG, move |store| {
                    if store[ITEMS].is_null() {
                        store[ITEMS] = json!({});
                    }
                    let items = store[ITEMS]
                        .as_object_mut()
                        .ok_or("GIF snapshots must be an object")?;
                    for gif in page.snapshots {
                        items.insert(gif.key(), json!(gif));
                    }
                    Ok(())
                })
                .await?;
            }
            let mut results: Vec<Gif> =
                serde_json::from_value(state["results"].clone()).unwrap_or_default();
            let mut seen: HashSet<String> = results.iter().map(Gif::key).collect();
            results.extend(page.gifs.into_iter().filter(|gif| seen.insert(gif.key())));
            state["results"] = json!(results);
            state["offset"] = json!(page.offset);
            state["hasMore"] = json!(page.has_more);
            state["next"] = json!(page.next);
        }
        Err(error) => {
            state["error"] = json!(error);
        }
    }
    Ok(true)
}

fn list_key(service: &str, recent: bool) -> String {
    format!("{service}-{}", if recent { "recent" } else { "favs" })
}

fn ids(store: &Value, service: &str, recent: bool) -> Vec<String> {
    let value = &store[list_key(service, recent)];
    // Source-compatible ID arrays; snapshots live separately, preserving unknown keys.
    let parsed = value
        .as_str()
        .and_then(|s| serde_json::from_str::<Value>(s).ok());
    parsed
        .as_ref()
        .unwrap_or(value)
        .as_array()
        .map(|array| {
            array
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn validate_store(store: &Value) -> Result<(), String> {
    if !store.is_object() {
        return Err("GIF store must be an object".into());
    }
    for key in [ITEMS, USAGE] {
        if !store[key].is_null() && !store[key].is_object() {
            return Err(format!("GIF store field {key} must be an object"));
        }
    }
    for service in PROVIDERS {
        for recent in [false, true] {
            let key = list_key(service, recent);
            let value = &store[&key];
            if value.is_null() {
                continue;
            }
            let decoded = value
                .as_str()
                .map(serde_json::from_str::<Value>)
                .transpose()
                .map_err(|_| format!("GIF collection {key} contains invalid JSON"))?;
            let values = decoded
                .as_ref()
                .unwrap_or(value)
                .as_array()
                .ok_or_else(|| format!("GIF collection {key} must be an array"))?;
            if values
                .iter()
                .any(|v| v.as_str().is_none_or(|s| s.is_empty() || s.len() > 1024))
            {
                return Err(format!("GIF collection {key} has an invalid identifier"));
            }
        }
    }
    Ok(())
}

fn select_legacy_keys(source: &Value) -> Result<Value, String> {
    let source = source
        .as_object()
        .ok_or("Legacy GIF storage must be an object")?;
    let mut selected = json!({});
    // These eight keys are the only collection keys used by localGifs.ts.
    // No preferences, tokens, arbitrary cached data, or other keys are copied.
    for service in PROVIDERS {
        for recent in [false, true] {
            let key = list_key(service, recent);
            let Some(raw) = source.get(&key) else {
                continue;
            };
            let decoded = raw
                .as_str()
                .map(serde_json::from_str::<Value>)
                .transpose()
                .map_err(|_| format!("Legacy GIF collection {key} contains invalid JSON"))?;
            let values = decoded
                .as_ref()
                .unwrap_or(raw)
                .as_array()
                .ok_or_else(|| format!("Legacy GIF collection {key} must be an array"))?;
            let mut seen = HashSet::new();
            let mut ids = vec![];
            for value in values {
                let id = value
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 1024)
                    .ok_or_else(|| {
                        format!("Legacy GIF collection {key} has an invalid identifier")
                    })?;
                if seen.insert(id.to_string()) {
                    ids.push(id.to_string());
                }
            }
            selected[&key] = json!(ids);
        }
    }
    if let Some(raw) = source.get("service").and_then(Value::as_str) {
        let parsed = serde_json::from_str::<String>(raw).ok();
        let service = parsed.as_deref().unwrap_or(raw);
        if SERVICES.contains(&service) {
            selected["service"] = json!(service);
        }
    }
    Ok(selected)
}

async fn store_with_migration(ctx: &CoreContext) -> CoreResult {
    let store = v2_sdk::read_store(ctx, SLUG).await?;
    validate_store(&store)?;
    if store[MIGRATED] == true {
        return Ok(store);
    }
    // Runtime-only, read-only legacy access. Tests construct both layouts under
    // a temporary CoreContext; implementation work never opens user storage.
    let paths = [
        ctx.paths
            .data_dir
            .join("extensions/gif-search/.tezbar-local-storage.json"),
        ctx.paths
            .data_dir
            .join("extensions/packages/raycast.gif-search/.tezbar-local-storage.json"),
    ];
    let imports = tokio::task::spawn_blocking(move || -> Result<Vec<Value>, String> {
        let mut imports = vec![];
        for path in paths {
            let file = match std::fs::File::open(&path) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => return Err("Could not read legacy GIF collections".into()),
            };
            let mut bytes = vec![];
            file.take(16 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "Could not read legacy GIF collections")?;
            if bytes.len() > 16 * 1024 * 1024 {
                return Err("Legacy GIF storage exceeds the import size limit".into());
            }
            let source: Value = serde_json::from_slice(&bytes)
                .map_err(|_| "Legacy GIF storage contains invalid JSON")?;
            imports.push(select_legacy_keys(&source)?);
        }
        Ok(imports)
    })
    .await
    .map_err(|_| "GIF migration task failed")??;
    v2_sdk::update_store(ctx, SLUG, move |target| {
        validate_store(target)?;
        if target[MIGRATED] == true {
            return Ok(());
        }
        for source in imports {
            for service in PROVIDERS {
                for recent in [false, true] {
                    let key = list_key(service, recent);
                    if source.get(&key).is_none() {
                        continue;
                    }
                    let mut values = ids(target, service, recent);
                    let mut seen: HashSet<String> = values.iter().cloned().collect();
                    values.extend(
                        ids(&source, service, recent)
                            .into_iter()
                            .filter(|id| seen.insert(id.clone())),
                    );
                    target[&key] = json!(values);
                }
            }
            if target.get("service").is_none() && source["service"].is_string() {
                target["service"] = source["service"].clone();
            }
        }
        target[MIGRATED] = json!(true);
        Ok(())
    })
    .await
}

fn contains(store: &Value, gif: &Gif, recent: bool) -> bool {
    ids(store, &gif.service, recent)
        .iter()
        .any(|id| id == &gif.id)
}

fn local_gifs(store: &Value, service: &str, recent: bool, query: &str) -> Vec<Gif> {
    let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut gifs: Vec<Gif> = ids(store, service, recent)
        .iter()
        .filter_map(|id| {
            let gif: Gif =
                serde_json::from_value(store[ITEMS][format!("{service}:{id}")].clone()).ok()?;
            let haystack = format!("{} {} {}", gif.title, gif.service, gif.metadata).to_lowercase();
            (gif.service == service
                && gif.id == *id
                && terms.iter().all(|term| haystack.contains(term)))
            .then_some(gif)
        })
        .collect();
    if !recent {
        // Local favorite ordering follows native visit history. Keep source list
        // order as a stable tie-breaker for entries that have never been used.
        let now = now_ms();
        let score = |gif: &Gif| {
            let usage = &store[USAGE][gif.key()];
            let age =
                now.saturating_sub(usage["lastUsed"].as_u64().unwrap_or(0)) as f64 / 86_400_000.0;
            usage["count"].as_u64().unwrap_or(0) as f64 / (1.0 + age)
        };
        gifs.sort_by(|a, b| score(b).total_cmp(&score(a)));
    }
    gifs
}

fn mutate_collection(
    store: &mut Value,
    gif: &Gif,
    recent: bool,
    remove: bool,
) -> Result<(), String> {
    validate_store(store)?;
    let mut values = ids(store, &gif.service, recent);
    values.retain(|id| id != &gif.id);
    if !remove {
        if recent {
            values.insert(0, gif.id.clone());
        } else {
            values.push(gif.id.clone());
        }
    }
    store[list_key(&gif.service, recent)] = json!(values);
    if store[ITEMS].is_null() {
        store[ITEMS] = json!({});
    }
    let items = store[ITEMS]
        .as_object_mut()
        .ok_or("GIF snapshots must be an object")?;
    items.insert(gif.key(), json!(gif));
    if recent && !remove {
        if store[USAGE].is_null() {
            store[USAGE] = json!({});
        }
        if !store[USAGE].is_object() {
            return Err("GIF usage store must be an object".into());
        }
        let count = store[USAGE][gif.key()]["count"]
            .as_u64()
            .unwrap_or(0)
            .saturating_add(1);
        store[USAGE][gif.key()] = json!({"count":count,"lastUsed":now_ms()});
    }
    Ok(())
}

fn action_definitions(gif: &Gif, store: &Value, prefs: &Value, details: bool) -> Vec<Value> {
    let favorite = contains(store, gif, false);
    let mut actions = vec![
        ("copyFile", "Copy GIF"),
        ("pasteFile", "Paste GIF"),
        ("copyGifUrl", "Copy GIF Link"),
        ("pasteGifUrl", "Paste GIF Link"),
        ("copyGifMarkdown", "Copy GIF Markdown"),
        ("pasteGifMarkdown", "Paste GIF Markdown"),
        ("copySquareGif", "Copy GIF Square"),
        ("pasteSquareGif", "Paste GIF Square"),
        (
            "toggleFav",
            if favorite {
                "Remove from Favorites"
            } else {
                "Add to Favorites"
            },
        ),
    ];
    if contains(store, gif, true) {
        actions.push(("removeRecent", "Remove from Recents"));
    }
    if !details {
        actions.push(("viewDetails", "View GIF Details"));
    }
    actions.extend([
        ("copyPageUrl", "Copy Page Link"),
        ("openUrlInBrowser", "Open in Browser"),
        ("downloadFile", "Download GIF"),
    ]);
    let default = string(prefs, "defaultAction", "copyFile");
    if let Some(index) = actions.iter().position(|(key, _)| *key == default) {
        let action = actions.remove(index);
        actions.insert(0, action);
    }
    actions.into_iter().map(|(key, title)| json!({"id":gif.action(key),"title":title,"kind":"action",
        "style":if key == "removeRecent" || (key == "toggleFav" && favorite) {"destructive"} else {"default"}})).collect()
}

fn visible_sections(state: &Value, store: &Value, prefs: &Value) -> Vec<(String, Vec<Gif>)> {
    let service = string(state, "service", "giphy");
    let query = string(state, "query", "");
    if is_local(service) {
        return PROVIDERS
            .iter()
            .map(|provider| {
                (
                    title(provider).to_string(),
                    local_gifs(store, provider, service == "recents", query),
                )
            })
            .collect();
    }
    let mut sections = vec![];
    if query.is_empty() {
        let count = columns(state, prefs) * 2;
        sections.push((
            "Favorites".into(),
            local_gifs(store, service, false, "")
                .into_iter()
                .take(count)
                .collect(),
        ));
        sections.push((
            "Recent".into(),
            local_gifs(store, service, true, "")
                .into_iter()
                .filter(|gif| !contains(store, gif, false))
                .take(count)
                .collect(),
        ));
    }
    let results: Vec<Gif> = serde_json::from_value(state["results"].clone()).unwrap_or_default();
    sections.push((
        if query.is_empty() {
            "Trending".into()
        } else {
            format!("GIF results for \"{query}\"")
        },
        results,
    ));
    sections
}

fn detail(gif: &Gif, store: &Value, prefs: &Value) -> Value {
    let mut metadata = vec![
        json!({"type":"Detail.Metadata.Label","props":{"title":"Title","text":gif.title}}),
        json!({"type":"Detail.Metadata.Label","props":{"title":"Provider","text":title(&gif.service)}}),
    ];
    for (field, label) in [
        ("width", "Width"),
        ("height", "Height"),
        ("size", "Size (bytes)"),
    ] {
        let text = scalar(&gif.metadata[field]);
        if !text.is_empty() {
            metadata
                .push(json!({"type":"Detail.Metadata.Label","props":{"title":label,"text":text}}));
        }
    }
    for label in gif.metadata["labels"].as_array().into_iter().flatten() {
        if !scalar(&label["text"]).is_empty() {
            metadata.push(json!({"type":"Detail.Metadata.Label","props":label}));
        }
    }
    for link in gif.metadata["links"].as_array().into_iter().flatten() {
        if link["target"].as_str().is_some_and(|s| safe_url(s).is_ok()) {
            metadata.push(json!({"type":"Detail.Metadata.Link","props":link}));
        }
    }
    if let Some(tags) = gif.metadata["tags"].as_array() {
        metadata.push(json!({"type":"Detail.Metadata.TagList","props":{"title":"Tags"},"children":tags.iter().filter_map(Value::as_str).map(|tag| json!({"type":"Detail.Metadata.TagList.Item","props":{"text":tag}})).collect::<Vec<_>>()}));
    }
    let mut actions = action_definitions(gif, store, prefs, true);
    actions.push(json!({"id":BACK,"title":"Back to GIFs","kind":"action"}));
    v2_sdk::view(
        &gif.title,
        json!({"type":"Detail","props":{"markdown":format!("![GIF]({})",gif.gif_url),"backActionId":BACK},
        "metadata":{"type":"Detail.Metadata","children":metadata}}),
        actions,
    )
}

fn render(state: &Value, store: &Value, prefs: &Value) -> Value {
    if let Ok(gif) = serde_json::from_value::<Gif>(state["detail"].clone()) {
        let mut view = detail(&gif, store, prefs);
        if state["download"]
            .as_str()
            .is_some_and(|s| Path::new(s).is_absolute())
        {
            view["actions"].as_array_mut().unwrap().extend([
                json!({"id":OPEN_DOWNLOAD,"title":"Open Downloaded GIF","kind":"action"}),
                json!({"id":REVEAL_DOWNLOAD,"title":"Show Downloaded GIF in Folder","kind":"action"}),
            ]);
        }
        return view;
    }
    let service = string(state, "service", "giphy");
    let mut actions = vec![];
    let mut children = vec![];
    let mut seen = HashSet::new();
    for (section_title, gifs) in visible_sections(state, store, prefs) {
        let mut items = vec![];
        for gif in gifs.into_iter().filter(|gif| seen.insert(gif.key())) {
            let item_actions = action_definitions(&gif, store, prefs, false);
            let action_ids: Vec<Value> = item_actions
                .iter()
                .map(|action| action["id"].clone())
                .collect();
            let image = if columns(state, prefs) == 3 {
                &gif.large_preview
            } else {
                &gif.preview
            };
            items.push(json!({"type":"Grid.Item","props":{"id":gif.key(),"title":gif.title,"subtitle":title(&gif.service),
                "content":{"source":image},"actionIds":action_ids}}));
            actions.extend(item_actions);
        }
        if !items.is_empty() {
            children.push(
                json!({"type":"Grid.Section","props":{"title":section_title},"children":items}),
            );
        }
    }
    let loading = state["request"].as_u64().is_some();
    if !loading && state["hasMore"].as_bool().unwrap_or(false) && !is_local(service) {
        actions.push(json!({"id":MORE,"title":"Load More GIFs","kind":"action"}));
        // GridRuntime currently has no automatic onLoadMore binding. A real item
        // keeps pagination reachable, in addition to the standard IPC operation.
        children.push(json!({"type":"Grid.Item","props":{"id":MORE,"title":"Load More GIFs","actionIds":[MORE]}}));
    }
    if let Some(path) = state["download"].as_str() {
        if Path::new(path).is_absolute() {
            actions.push(json!({"id":OPEN_DOWNLOAD,"title":"Open Downloaded GIF","kind":"action"}));
            actions.push(json!({"id":REVEAL_DOWNLOAD,"title":if cfg!(target_os="macos") {"Show Downloaded GIF in Finder"} else {"Show Downloaded GIF in Explorer"},"kind":"action"}));
            children.push(json!({"type":"Grid.Item","props":{"id":OPEN_DOWNLOAD,"title":"Downloaded GIF","subtitle":path,"actionIds":[OPEN_DOWNLOAD,REVEAL_DOWNLOAD]}}));
        }
    }
    if state["error"].is_string() && !loading {
        actions.push(json!({"id":RETRY,"title":"Retry Loading GIFs","kind":"action"}));
        children.push(json!({"type":"Grid.Item","props":{"id":RETRY,"title":"Retry Loading GIFs","subtitle":state["error"],"actionIds":[RETRY]}}));
    }
    let error = state["error"].as_str();
    let empty = error.unwrap_or(if loading {
        "Loading GIFs…"
    } else if service == "favorites" {
        "Add some GIFs to your Favorites first…"
    } else if service == "recents" {
        "Work with some GIFs first…"
    } else if service == "finergifs" && string(state, "query", "").is_empty() {
        "Search Finer Gifs Club to get started…"
    } else {
        "No GIFs were found."
    });
    children.push(json!({"type":"Grid.EmptyView","props":{"title":empty}}));
    let mut props = json!({"columns":columns(state,prefs),"searchText":string(state,"query",""),"filtering":false,
        "__hasServerSearch":true,"isLoading":loading,"pagination":{"hasMore":state["hasMore"],"isLoading":loading},
        "searchBarPlaceholder":if service == "favorites" { "Search favorites".into() } else if service == "recents" { "Search recents".into() } else { format!("Search for GIFs on {}",title(service)) }});
    if let Some(error) = error {
        props["error"] = json!(error);
    }
    if string(state, "command", "search") == "search" {
        props["searchBarAccessory"] = json!({"type":"List.Dropdown","props":{"value":service,"actionId":SERVICE},
            "children":SERVICES.iter().map(|value|json!({"type":"List.Dropdown.Item","props":{"title":title(value),"value":value}})).collect::<Vec<_>>()});
        actions.push(json!({"id":SERVICE,"title":"Change GIF Provider","kind":"action"}));
    }
    let view_title = if service == "favorites" {
        "Favorite GIFs"
    } else if service == "recents" {
        "Recent GIFs"
    } else {
        "Search for GIFs"
    };
    let mut view = v2_sdk::view(
        view_title,
        json!({"type":"Grid","props":props,"children":children}),
        actions,
    );
    if let Some(error) = error {
        view["message"] = json!(error);
    }
    view
}

fn find_action_gif(
    state: &Value,
    store: &Value,
    prefs: &Value,
    id: &str,
) -> Result<(Gif, String), String> {
    let mut gifs: Vec<Gif> = visible_sections(state, store, prefs)
        .into_iter()
        .flat_map(|(_, gifs)| gifs)
        .collect();
    if let Ok(gif) = serde_json::from_value(state["detail"].clone()) {
        gifs.push(gif);
    }
    for gif in gifs {
        for action in action_definitions(&gif, store, prefs, false) {
            if action["id"] == id {
                let name = id
                    .strip_prefix("gif-search:")
                    .and_then(|s| s.split(':').next())
                    .ok_or("Invalid GIF action")?;
                return Ok((gif, name.to_string()));
            }
        }
    }
    Err("This GIF action is stale. Refresh the results and try again.".into())
}

async fn store_change(ctx: &CoreContext, gif: &Gif, recent: bool, remove: bool) -> CoreResult {
    let gif = gif.clone();
    v2_sdk::update_store(ctx, SLUG, move |store| {
        mutate_collection(store, &gif, recent, remove)
    })
    .await
}

fn filename(gif: &Gif, prefs: &Value) -> String {
    let hidden =
        prefs["hideFilename"].as_bool().unwrap_or(false) || prefs["hideFilename"] == "true";
    let extension = if gif.download_name.ends_with(".mp4") {
        "mp4"
    } else {
        "gif"
    };
    if hidden {
        format!("gif.{extension}")
    } else {
        format!("{}.{}", slugify(&gif.title), extension)
    }
}

fn cache_dir(ctx: &CoreContext, gif: &Gif) -> PathBuf {
    let digest = Sha256::digest(format!("{}:{}", gif.key(), gif.download_url).as_bytes());
    ctx.paths
        .data_dir
        .join("native-v2")
        .join(SLUG)
        .join("cached-gifs")
        .join(format!("{digest:x}"))
}

async fn media_bytes(gif: &Gif) -> Result<Vec<u8>, String> {
    let response = client()?
        .get(safe_url(&gif.download_url)?)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|_| "Could not download GIF")?;
    let bytes = response_bytes(response, MAX_MEDIA).await?;
    let gif_header = bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a");
    let mp4_header = bytes.get(4..8) == Some(b"ftyp".as_slice());
    if !gif_header && !mp4_header {
        return Err("Provider download is neither GIF nor MP4 media".into());
    }
    Ok(bytes)
}

async fn downloaded_file(
    ctx: &CoreContext,
    gif: &Gif,
    prefs: &Value,
    favorite: bool,
    square: bool,
) -> Result<PathBuf, String> {
    let name = filename(gif, prefs);
    let cached = cache_dir(ctx, gif).join(&name);
    if !square && cached.is_file() {
        return Ok(cached);
    }
    let mut bytes = media_bytes(gif).await?;
    if square {
        bytes = tokio::task::spawn_blocking(move || square_gif(&bytes))
            .await
            .map_err(|_| "Square GIF conversion failed")??;
    }
    let base = if favorite && !square {
        cache_dir(ctx, gif)
    } else {
        ctx.paths
            .temp_dir
            .join("native-v2")
            .join(SLUG)
            .join(uuid::Uuid::new_v4().to_string())
    };
    tokio::fs::create_dir_all(&base)
        .await
        .map_err(|e| format!("Could not create GIF directory: {e}"))?;
    let path = base.join(if square {
        format!("{}.gif", slugify(&gif.title))
    } else {
        name
    });
    let output = path.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let parent = output.parent().ok_or("GIF file has no parent directory")?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)
            .map_err(|e| format!("Could not stage GIF: {e}"))?;
        temporary
            .write_all(&bytes)
            .map_err(|e| format!("Could not write GIF: {e}"))?;
        temporary
            .flush()
            .map_err(|e| format!("Could not flush GIF: {e}"))?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|e| format!("Could not sync GIF: {e}"))?;
        temporary
            .persist(&output)
            .map_err(|_| "Could not save GIF atomically")?;
        Ok(())
    })
    .await
    .map_err(|_| "GIF file write task failed")??;
    Ok(path)
}

fn gif_repeat(bytes: &[u8]) -> Option<u16> {
    // Read actual extension blocks, never search arbitrary compressed image data.
    if bytes.len() < 13 {
        return None;
    }
    let mut i = 13
        + if bytes[10] & 0x80 != 0 {
            3 * (1usize << ((bytes[10] & 7) + 1))
        } else {
            0
        };
    while i < bytes.len() {
        match bytes[i] {
            0x3b => return None,
            0x21 => {
                let label = *bytes.get(i + 1)?;
                i += 2;
                let size = *bytes.get(i)? as usize;
                if label == 0xff && size == 11 {
                    let name = bytes.get(i + 1..i + 12)?;
                    if name == b"NETSCAPE2.0" || name == b"ANIMEXTS1.0" {
                        let data = bytes.get(i + 12..i + 17)?;
                        if data[0] == 3 && data[1] == 1 {
                            return Some(u16::from_le_bytes([data[2], data[3]]));
                        }
                    }
                }
            }
            0x2c => {
                let packed = *bytes.get(i + 9)?;
                i += 10;
                if packed & 0x80 != 0 {
                    i += 3 * (1usize << ((packed & 7) + 1));
                }
                i += 1; // LZW minimum code size.
            }
            _ => return None,
        }
        loop {
            let size = *bytes.get(i)? as usize;
            i += 1;
            if size == 0 {
                break;
            }
            i = i.checked_add(size)?;
            if i > bytes.len() {
                return None;
            }
        }
    }
    None
}

fn square_gif(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
        .map_err(|_| "Square conversion requires a GIF file")?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(128 * 1024 * 1024);
    decoder
        .set_limits(limits)
        .map_err(|_| "GIF dimensions exceed square conversion limits")?;
    let (width, height) = decoder.dimensions();
    let edge = width.min(height);
    if edge == 0 {
        return Err("GIF has no image dimensions".into());
    }
    let mut result = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut result);
        if let Some(repeat) = gif_repeat(bytes) {
            encoder
                .set_repeat(if repeat == 0 {
                    image::codecs::gif::Repeat::Infinite
                } else {
                    image::codecs::gif::Repeat::Finite(repeat)
                })
                .map_err(|e| e.to_string())?;
        }
        let mut pixels = 0u64;
        let mut count = 0usize;
        for frame in decoder.into_frames() {
            let frame = frame.map_err(|e| format!("Could not decode GIF frame: {e}"))?;
            pixels += u64::from(width) * u64::from(height);
            count += 1;
            if pixels > 128_000_000 || count > 2000 {
                return Err("GIF animation exceeds square conversion limits".into());
            }
            let cropped = image::imageops::crop_imm(
                frame.buffer(),
                (width - edge) / 2,
                (height - edge) / 2,
                edge,
                edge,
            )
            .to_image();
            encoder
                .encode_frame(image::Frame::from_parts(cropped, 0, 0, frame.delay()))
                .map_err(|e| format!("Could not encode square GIF: {e}"))?;
        }
        if count == 0 {
            return Err("GIF has no animation frames".into());
        }
    }
    Ok(result)
}

async fn copy_file(path: &Path) -> Result<(), String> {
    // The general image clipboard route rasterizes GIFs. Use file references so
    // animation and MP4 clips survive copying on both supported desktops.
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = tokio::process::Command::new("/usr/bin/osascript");
        command
            .args([
                "-e",
                "on run argv\nset the clipboard to (POSIX file (item 1 of argv))\nend run",
            ])
            .arg(path);
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = tokio::process::Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-STA", "-Command", "$ErrorActionPreference='Stop'; Add-Type -AssemblyName System.Windows.Forms; $files=New-Object System.Collections.Specialized.StringCollection; [void]$files.Add($env:TEZBAR_GIF_CLIPBOARD_FILE); [System.Windows.Forms.Clipboard]::SetFileDropList($files)"])
            .env("TEZBAR_GIF_CLIPBOARD_FILE",path);
        command
    };
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let output =
            tokio::time::timeout(Duration::from_secs(10), command.kill_on_drop(true).output())
                .await
                .map_err(|_| "File clipboard helper timed out")?
                .map_err(|_| "File clipboard helper could not start")?;
        if !output.status.success() {
            return Err("Could not copy GIF file to the system clipboard".into());
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = path;
        Err("GIF file clipboard is supported on macOS and Windows".into())
    }
}

fn save_unique(directory: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    if !directory.is_absolute() {
        return Err("GIF download path must be absolute".into());
    }
    if !directory.is_dir() {
        return Err("GIF download directory does not exist".into());
    }
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Invalid GIF filename")?;
    let ext = Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("gif");
    let mut temporary = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
    temporary.write_all(bytes).map_err(|e| e.to_string())?;
    temporary.flush().map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    for n in 0..10_000 {
        let target = directory.join(if n == 0 {
            name.to_string()
        } else {
            format!("{stem} ({n}).{ext}")
        });
        match temporary.persist_noclobber(&target) {
            Ok(_) => return Ok(target),
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                temporary = error.file;
            }
            Err(error) => return Err(format!("Could not save GIF: {}", error.error)),
        }
    }
    Err("Too many GIFs with the same filename in the download directory".into())
}

async fn invoke(
    ctx: &CoreContext,
    state: &mut Value,
    prefs: &Value,
    payload: &Value,
    mut store: Value,
) -> CoreResult {
    let id = payload["actionId"]
        .as_str()
        .ok_or("GIF action identifier is required")?;
    if id == OPEN_DOWNLOAD || id == REVEAL_DOWNLOAD {
        let path = state["download"]
            .as_str()
            .filter(|s| Path::new(s).is_absolute())
            .ok_or("There is no downloaded GIF in this session")?;
        if id == OPEN_DOWNLOAD {
            v2_sdk::open(ctx, path).await?;
        } else {
            ctx.request("core:platform:reveal-file", vec![json!(path)])
                .await?;
        }
        return Ok(render(state, &store, prefs));
    }
    if id == RETRY {
        begin(state, prefs, &store, false)?;
        return Ok(render(state, &store, prefs));
    }
    if id == BACK {
        state["detail"] = Value::Null;
        return Ok(render(state, &store, prefs));
    }
    if id == SERVICE {
        if string(state, "command", "search") != "search" {
            return Err("Provider switching is available in GIF Search".into());
        }
        let service = payload["formValues"]["value"]
            .as_str()
            .ok_or("GIF provider is required")?;
        if !SERVICES.contains(&service) {
            return Err("Unknown GIF provider".into());
        }
        let service = service.to_string();
        v2_sdk::update_store(ctx, SLUG, {
            let service = service.clone();
            move |store| {
                store
                    .as_object_mut()
                    .ok_or("GIF store must be an object")?
                    .insert("service".into(), json!(service));
                Ok(())
            }
        })
        .await?;
        state["service"] = json!(service);
        state["detail"] = Value::Null;
        state["results"] = json!([]);
        state["hasMore"] = json!(false);
        state["error"] = Value::Null;
        begin(state, prefs, &store, false)?;
        return Ok(render(state, &store, prefs));
    }
    if id == MORE {
        if state["request"].as_u64().is_none() && state["hasMore"] == true {
            begin(state, prefs, &store, true)?;
        }
        return Ok(render(state, &store, prefs));
    }
    let (gif, action) = find_action_gif(state, &store, prefs, id)?;
    if action == "toggleFav" || action == "removeRecent" {
        let recent = action == "removeRecent";
        let remove = recent || contains(&store, &gif, false);
        store = store_change(ctx, &gif, recent, remove).await?;
        if !recent && remove {
            let cache = cache_dir(ctx, &gif);
            if cache.is_dir() {
                tokio::fs::remove_dir_all(cache)
                    .await
                    .map_err(|_| "Favorite removed, but its cached GIF could not be deleted")?;
            }
        }
        return Ok(render(state, &store, prefs));
    }
    if action == "viewDetails" {
        store = store_change(ctx, &gif, true, false).await?;
        state["detail"] = json!(gif);
        return Ok(render(state, &store, prefs));
    }
    let message = match action.as_str() {
        "copyGifUrl" | "copyPageUrl" | "copyGifMarkdown" | "pasteGifUrl" | "pasteGifMarkdown" => {
            let text = if action == "copyPageUrl" {
                gif.url.clone()
            } else if action.ends_with("Markdown") {
                format!(
                    "![{}]({})",
                    gif.title
                        .replace('\\', "\\\\")
                        .replace('[', "\\[")
                        .replace(']', "\\]"),
                    clean_url(&gif.gif_url)?
                )
            } else {
                clean_url(&gif.gif_url)?
            };
            if action.starts_with("paste") {
                ctx.request("core:platform:paste-text", vec![json!(text)])
                    .await?;
                "Pasted GIF link"
            } else {
                v2_sdk::copy(ctx, &text).await?;
                "Copied GIF link"
            }
        }
        "openUrlInBrowser" => {
            v2_sdk::open(ctx, safe_url(&gif.url)?.as_str()).await?;
            "Opened GIF in browser"
        }
        "copyFile" | "pasteFile" | "copySquareGif" | "pasteSquareGif" => {
            let square = action.contains("Square");
            let file =
                downloaded_file(ctx, &gif, prefs, contains(&store, &gif, false), square).await?;
            copy_file(&file).await?;
            if action.starts_with("paste") {
                ctx.request("core:platform:paste", vec![]).await?;
                "Pasted GIF"
            } else {
                "Copied GIF"
            }
        }
        "downloadFile" => {
            let cached = cache_dir(ctx, &gif).join(filename(&gif, prefs));
            let bytes = if cached.is_file() {
                tokio::fs::read(cached)
                    .await
                    .map_err(|_| "Could not read cached GIF")?
            } else {
                media_bytes(&gif).await?
            };
            let raw = string(prefs, "downloadPath", "");
            let directory = if raw.is_empty() {
                ctx.paths.home_dir.join("Downloads")
            } else if let Some(rest) = raw.strip_prefix("~/") {
                ctx.paths.home_dir.join(rest)
            } else {
                PathBuf::from(raw)
            };
            let name = filename(&gif, prefs);
            let file = tokio::task::spawn_blocking(move || save_unique(&directory, &name, &bytes))
                .await
                .map_err(|_| "GIF save task failed")??;
            state["download"] = json!(file.to_string_lossy());
            let mut result = render(state, &store, prefs);
            result["message"] = json!(format!("Downloaded GIF: {}", file.display()));
            result["effects"] = json!([{"kind":"toast","title":"Downloaded GIF","message":file.to_string_lossy(),"style":"success"}]);
            return Ok(result);
        }
        _ => return Err("Unsupported GIF action".into()),
    };
    store = store_change(ctx, &gif, true, false).await?;
    if action == "copyFile" || action == "copySquareGif" {
        ctx.request("window:hide", vec![]).await?;
    }
    let mut result = render(state, &store, prefs);
    result["message"] = json!(message);
    Ok(result)
}

pub async fn handle(
    ctx: &CoreContext,
    command: &str,
    operation: &str,
    payload: &Value,
    state: &mut Value,
) -> CoreResult {
    if !["search", "favorites", "recents"].contains(&command) {
        return Err("Unknown GIF Search command".into());
    }
    if operation == "extension:dispose-session" {
        cancel(state)?;
        *state = json!({});
        return Ok(v2_sdk::no_view(""));
    }
    let prefs = &payload["preferences"];
    let store = store_with_migration(ctx).await?;
    if operation == "extension:run-command" {
        cancel(state)?;
        let cached_service = string(&store, "service", "giphy");
        let service = if command == "search" {
            if SERVICES.contains(&cached_service) {
                cached_service
            } else {
                "giphy"
            }
        } else {
            command
        };
        *state =
            json!({"command":command,"service":service,"query":"","results":[],"hasMore":false});
        begin(state, prefs, &store, false)?;
        return Ok(render(state, &store, prefs));
    }
    if state["command"] != command {
        return Err("GIF session has not been initialized".into());
    }
    match operation {
        "extension:search-text-changed" => {
            let query = payload["searchText"].as_str().unwrap_or("");
            if query.len() > 4096 {
                return Err("GIF search text is too long".into());
            }
            state["query"] = json!(query);
            state["detail"] = Value::Null;
            begin(state, prefs, &store, false)?;
            Ok(render(state, &store, prefs))
        }
        "extension:refresh-session" => {
            let changed = poll(ctx, state).await?;
            let store = v2_sdk::read_store(ctx, SLUG).await?;
            let signature = serde_json::to_string(&(&store, prefs))
                .map_err(|_| "Could not inspect GIF store")?;
            let digest = format!("{:x}", Sha256::digest(signature.as_bytes()));
            if !changed && state["storeRevision"] == digest {
                return Ok(v2_sdk::unchanged());
            }
            state["storeRevision"] = json!(digest);
            Ok(render(state, &store, prefs))
        }
        "extension:load-more" => {
            poll(ctx, state).await?;
            let store = v2_sdk::read_store(ctx, SLUG).await?;
            if state["request"].as_u64().is_none() && state["hasMore"] == true {
                begin(state, prefs, &store, true)?;
            }
            Ok(render(state, &store, prefs))
        }
        "extension:invoke-action" => {
            poll(ctx, state).await?;
            let store = v2_sdk::read_store(ctx, SLUG).await?;
            invoke(ctx, state, prefs, payload, store).await
        }
        _ => Err("Unsupported GIF Search operation".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_core::common::{CorePaths, CoreRouter};
    use std::sync::Arc;

    fn fixture(id: &str) -> Value {
        json!({"id":id,"title":"Hello GIF","url":format!("https://giphy.com/gifs/{id}"),
            "images":{"original":{"url":format!("https://media.giphy.com/{id}.gif?tracking=1"),"width":"4","height":"2","size":"123"},
                "preview_gif":{"url":format!("https://media.giphy.com/{id}-preview.gif")},
                "fixed_height_small":{"url":format!("https://media.giphy.com/{id}-large.gif")}}})
    }

    fn gif(id: &str) -> Gif {
        map_gif("giphy", &fixture(id)).unwrap()
    }

    fn context() -> (
        tempfile::TempDir,
        CoreContext,
        Arc<Mutex<Vec<(String, Vec<Value>)>>>,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let ctx = CoreContext::headless(CorePaths {
            data_dir: root.join("data"),
            config_dir: root.join("config"),
            resource_dir: root.join("resources"),
            temp_dir: root.join("temp"),
            home_dir: root.join("home"),
        });
        let records = Arc::new(Mutex::new(vec![]));
        let recorded = records.clone();
        let router: CoreRouter = Arc::new(move |channel, args| {
            recorded.lock().unwrap().push((channel, args));
            Box::pin(async { Ok(json!({"ok":true})) })
        });
        ctx.set_router(router).unwrap();
        (temp, ctx, records)
    }

    #[test]
    fn provider_urls_match_installed_source_and_encode_user_input() {
        let url = request_url(
            "giphy-clips",
            "cats & dogs/Кот",
            &json!({"maxResults":"17","giphyLocale":"ru"}),
            34,
            None,
        )
        .unwrap();
        let pairs: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(url.path(), "/api/giphy");
        assert_eq!(pairs["type"], "videos");
        assert_eq!(pairs["q"], "cats & dogs/Кот");
        assert_eq!(pairs["offset"], "34");
        assert_eq!(pairs["limit"], "17");
        assert_eq!(pairs["lang"], "ru");
        let url = request_url("klipy", "", &json!({}), 0, Some("next+/=&")).unwrap();
        assert!(url
            .query_pairs()
            .any(|(k, v)| k == "pos" && v == "next+/=&"));
        let url = request_url("finergifs", "the office", &json!({}), 20, None).unwrap();
        assert!(url.query_pairs().any(|(k, v)| k == "start" && v == "20"));
        assert_eq!(page_limit(&json!({"maxResults":"bad"})), 20);
        assert_eq!(page_limit(&json!({"maxResults":"0"})), 1);
    }

    #[test]
    fn parser_deduplicates_and_pages_by_consumed_rows_not_unique_count() {
        let page = parse_page(
            "giphy",
            &json!({"data":[fixture("a"),fixture("a")],"pagination":{"total_count":8}}),
            2,
            2,
        )
        .unwrap();
        assert_eq!(page.gifs.len(), 1);
        assert_eq!(page.offset, 4);
        assert!(page.has_more);
        assert!(parse_page("giphy", &json!({"error":"quota"}), 0, 20).is_err());
        assert!(parse_page("giphy", &json!({"data":[],"meta":{"status":429}}), 0, 20).is_err());
        let mut broken = fixture("a");
        broken["images"]["original"]["url"] = json!("file:///secret");
        assert!(map_gif("giphy", &broken).is_err());
        assert_eq!(
            clean_url("https://media.giphy.com/a.gif?secret=never-log").unwrap(),
            "https://media.giphy.com/a.gif"
        );
    }

    #[test]
    fn clips_klipy_and_finer_preserve_download_formats() {
        let mut video = fixture("clip");
        video["video"] = json!({"assets":{"720p":{"url":"https://media.giphy.com/clip.mp4"}}});
        let clip = map_gif("giphy-clips", &video).unwrap();
        assert!(clip.download_name.ends_with(".mp4"));
        let page = parse_page("klipy",&json!({"results":[{"id":"k","content_description":"Cat","itemurl":"https://klipy.com/gifs/cat","media_formats":{"gif":{"url":"https://media.klipy.com/cat.gif","dims":[8,4]}}}],"next":"cursor"}),0,20).unwrap();
        assert_eq!(page.next.as_deref(), Some("cursor"));
        assert!(page.has_more);
        assert_eq!(page.gifs[0].preview, page.gifs[0].gif_url);
        let finer = map_gif(
            "finergifs",
            &json!({"fields":{"fileid":"02x03-happy","text":"Happy"}}),
        )
        .unwrap();
        assert_eq!(finer.metadata["labels"][0]["text"], "02");
    }

    #[test]
    fn native_collections_preserve_unknown_keys_order_and_provider_identity() {
        let mut store = json!({"unknown":{"keep":true}});
        let a = gif("a");
        let b = gif("b");
        mutate_collection(&mut store, &a, false, false).unwrap();
        mutate_collection(&mut store, &b, false, false).unwrap();
        mutate_collection(&mut store, &a, true, false).unwrap();
        mutate_collection(&mut store, &b, true, false).unwrap();
        mutate_collection(&mut store, &a, true, false).unwrap();
        assert_eq!(ids(&store, "giphy", false), vec!["a", "b"]);
        assert_eq!(ids(&store, "giphy", true), vec!["a", "b"]);
        assert_eq!(store[USAGE][a.key()]["count"], 2);
        assert_eq!(local_gifs(&store, "giphy", false, "hello gif").len(), 2);
        assert!(local_gifs(&store, "giphy", false, "absent").is_empty());
        mutate_collection(&mut store, &a, false, true).unwrap();
        assert_eq!(ids(&store, "giphy", false), vec!["b"]);
        assert_eq!(store["unknown"]["keep"], true);
        assert!(contains(&store, &a, true));
    }

    #[test]
    fn grid_item_action_ids_are_registered_and_default_preference_is_honored() {
        let item = gif("a");
        let state = json!({"command":"search","service":"giphy","query":"hello","results":[item],"hasMore":true});
        let view = render(&state, &json!({}), &json!({"defaultAction":"copyGifUrl"}));
        let props = &view["root"]["children"][0]["children"][0]["props"];
        assert_eq!(
            props["content"]["source"],
            "https://media.giphy.com/a-preview.gif"
        );
        assert_eq!(props["actionIds"][0], item.action("copyGifUrl"));
        for id in props["actionIds"].as_array().unwrap() {
            assert!(view["actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["id"] == *id));
        }
        assert_eq!(
            view["root"]["props"]["searchBarAccessory"]["type"],
            "List.Dropdown"
        );
        assert!(view["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == MORE));
        assert!(find_action_gif(
            &state,
            &json!({}),
            &json!({}),
            "gif-search:copyGifUrl:bogus"
        )
        .is_err());
    }

    #[tokio::test]
    async fn copy_link_and_favorites_use_recording_router_and_temporary_store() {
        let (_temp, ctx, recorded) = context();
        let item = gif("a");
        let mut state = json!({"command":"search","service":"giphy","query":"a","results":[item]});
        handle(
            &ctx,
            "search",
            "extension:invoke-action",
            &json!({"actionId":item.action("copyGifUrl")}),
            &mut state,
        )
        .await
        .unwrap();
        let calls = recorded.lock().unwrap().clone();
        assert!(calls
            .iter()
            .any(|(channel, args)| channel.contains("clipboard")
                && args.iter().any(|v| v == "https://media.giphy.com/a.gif")));
        let store = v2_sdk::read_store(&ctx, SLUG).await.unwrap();
        assert!(contains(&store, &item, true));
        handle(
            &ctx,
            "search",
            "extension:invoke-action",
            &json!({"actionId":item.action("toggleFav")}),
            &mut state,
        )
        .await
        .unwrap();
        let mut local = json!({});
        let view = handle(
            &ctx,
            "favorites",
            "extension:run-command",
            &json!({}),
            &mut local,
        )
        .await
        .unwrap();
        assert_eq!(
            view["root"]["children"][0]["children"][0]["props"]["id"],
            item.key()
        );
        handle(
            &ctx,
            "favorites",
            "extension:invoke-action",
            &json!({"actionId":item.action("toggleFav")}),
            &mut local,
        )
        .await
        .unwrap();
        assert!(!contains(
            &v2_sdk::read_store(&ctx, SLUG).await.unwrap(),
            &item,
            false
        ));
    }

    #[tokio::test]
    async fn cancellation_discards_completed_old_results_and_disposal_aborts_work() {
        let (sender, receiver) = oneshot::channel();
        let task = tokio::spawn(std::future::pending::<()>());
        let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        requests().lock().unwrap().insert(
            id,
            Pending {
                receiver,
                abort: task.abort_handle(),
            },
        );
        sender
            .send(Ok(Page {
                gifs: vec![gif("stale")],
                snapshots: vec![],
                next: None,
                has_more: false,
                offset: 1,
            }))
            .unwrap();
        let mut state = json!({"request":id,"results":[]});
        cancel(&mut state).unwrap();
        let (_temp, ctx, _) = context();
        assert!(!poll(&ctx, &mut state).await.unwrap());
        assert_eq!(state["results"], json!([]));
        assert!(!requests().lock().unwrap().contains_key(&id));
        let (_temp, ctx, _) = context();
        handle(
            &ctx,
            "search",
            "extension:dispose-session",
            &json!({}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(state, json!({}));
        let _ = task.await;
    }

    #[tokio::test]
    async fn latest_completion_merges_pages_persists_snapshots_and_dispose_aborts_active_job() {
        let (_temp, ctx, _) = context();
        let (sender, receiver) = oneshot::channel();
        let task = tokio::spawn(std::future::pending::<()>());
        let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        requests().lock().unwrap().insert(
            id,
            Pending {
                receiver,
                abort: task.abort_handle(),
            },
        );
        sender
            .send(Ok(Page {
                gifs: vec![gif("a"), gif("b")],
                snapshots: vec![gif("saved")],
                next: Some("cursor".into()),
                has_more: true,
                offset: 3,
            }))
            .unwrap();
        let mut state = json!({"command":"search","service":"giphy","query":"new","request":id,"results":[gif("a")]});
        assert!(poll(&ctx, &mut state).await.unwrap());
        assert_eq!(state["results"].as_array().unwrap().len(), 2);
        assert_eq!(state["results"][1]["id"], "b");
        assert_eq!(state["offset"], 3);
        assert_eq!(state["next"], "cursor");
        assert_eq!(state["hasMore"], true);
        assert_eq!(
            v2_sdk::read_store(&ctx, SLUG).await.unwrap()[ITEMS]["giphy:saved"]["id"],
            "saved"
        );
        task.abort();
        let _ = task.await;
        let (_sender, receiver) = oneshot::channel();
        let active = tokio::spawn(std::future::pending::<()>());
        let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        requests().lock().unwrap().insert(
            id,
            Pending {
                receiver,
                abort: active.abort_handle(),
            },
        );
        state["request"] = json!(id);
        handle(
            &ctx,
            "search",
            "extension:dispose-session",
            &json!({}),
            &mut state,
        )
        .await
        .unwrap();
        assert!(active.await.unwrap_err().is_cancelled());
        assert!(!requests().lock().unwrap().contains_key(&id));
    }

    #[test]
    fn square_conversion_preserves_frames_delays_and_repeat_without_swift() {
        let mut bytes = vec![];
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            encoder
                .set_repeat(image::codecs::gif::Repeat::Finite(3))
                .unwrap();
            for (color, delay) in [([255, 0, 0, 255], 100), ([0, 255, 0, 255], 200)] {
                encoder
                    .encode_frame(image::Frame::from_parts(
                        image::RgbaImage::from_pixel(4, 2, image::Rgba(color)),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(delay, 1),
                    ))
                    .unwrap();
            }
        }
        let square = square_gif(&bytes).unwrap();
        assert_eq!(gif_repeat(&square), Some(3));
        let decoder = image::codecs::gif::GifDecoder::new(Cursor::new(square)).unwrap();
        assert_eq!(decoder.dimensions(), (2, 2));
        let frames = decoder.into_frames().collect_frames().unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].delay().numer_denom_ms(), (100, 1));
        assert_eq!(frames[1].delay().numer_denom_ms(), (200, 1));
        assert!(square_gif(b"not a GIF").is_err());
    }

    #[test]
    fn download_names_are_safe_and_existing_files_are_never_overwritten() {
        let directory = tempfile::tempdir().unwrap();
        let item = gif("a");
        let name = filename(&item, &json!({}));
        assert!(!name.contains('/'));
        assert!(!name.contains('\\'));
        let a = save_unique(directory.path(), &name, b"first").unwrap();
        let b = save_unique(directory.path(), &name, b"second").unwrap();
        assert_ne!(a, b);
        assert_eq!(std::fs::read(a).unwrap(), b"first");
        assert_eq!(std::fs::read(b).unwrap(), b"second");
        assert_eq!(filename(&item, &json!({"hideFilename":true})), "gif.gif");
    }

    #[test]
    fn id_only_collections_use_provider_lookup_and_malformed_data_is_rejected() {
        let mut store = json!({"giphy-favs":"[\"old\"]","unknown":{"preserve":true}});
        assert_eq!(
            missing_snapshots(&store, "favorites"),
            vec![("giphy".into(), vec!["old".into()])]
        );
        let url = lookup_url("giphy", &["id with space".into(), "a&b".into()]).unwrap();
        assert!(url
            .query_pairs()
            .any(|(k, v)| k == "ids" && v == "id with space,a&b"));
        let finer = lookup_url("finergifs", &["02x03-hello".into()]).unwrap();
        assert!(finer
            .query_pairs()
            .any(|(k, v)| k == "q.options" && v == r#"{"fields":["fileid"]}"#));
        store["giphy-favs"] = json!("not-json");
        let before = store.clone();
        assert!(mutate_collection(&mut store, &gif("a"), false, false).is_err());
        assert_eq!(store, before);
        assert_eq!(slugify("../../CON"), "gif-con");
    }

    #[tokio::test]
    async fn http_fixture_exercises_json_and_failure_paths_without_external_network() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        async fn server(status: u16, body: String) -> (reqwest::Url, tokio::task::JoinHandle<()>) {
            let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let address = listener.local_addr().unwrap();
            let task = tokio::spawn(async move {
                let (mut stream, _) =
                    tokio::time::timeout(Duration::from_secs(3), listener.accept())
                        .await
                        .unwrap()
                        .unwrap();
                let mut request = [0u8; 4096];
                let _ = stream.read(&mut request).await.unwrap();
                let response = format!("HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
                stream.write_all(response.as_bytes()).await.unwrap();
            });
            (
                reqwest::Url::parse(&format!("http://{address}/?api_key=fixture-secret")).unwrap(),
                task,
            )
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        let (url, task) = server(200, json!({"data":[fixture("http")]}).to_string()).await;
        let value = fetch_json(&client, url, "giphy").await.unwrap();
        task.await.unwrap();
        assert_eq!(
            parse_page("giphy", &value, 0, 20).unwrap().gifs[0].id,
            "http"
        );
        for (status, body) in [(429, "quota"), (200, "{broken")] {
            let (url, task) = server(status, body.into()).await;
            let error = fetch_json(&client, url, "giphy").await.unwrap_err();
            task.await.unwrap();
            assert!(!error.contains("fixture-secret"));
            assert!(!error.contains("http://"));
        }
    }

    #[tokio::test]
    async fn legacy_migration_merges_only_known_keys_once_and_never_changes_source_files() {
        let (_temp, ctx, _) = context();
        let primary = ctx
            .paths
            .data_dir
            .join("extensions/gif-search/.tezbar-local-storage.json");
        let fallback = ctx
            .paths
            .data_dir
            .join("extensions/packages/raycast.gif-search/.tezbar-local-storage.json");
        let legacy = json!({
            "giphy-favs":"[\"old-favorite\",\"shared\",\"old-favorite\"]",
            "giphy-recent":"[\"old-recent\",\"shared\"]",
            "giphy-clips-favs":"[\"clip\"]","klipy-recent":"[\"klipy-old\"]",
            "finergifs-favs":"[\"02x03-hello\"]","service":"\"klipy\"",
            "apiKey":"fixture-do-not-copy","unknownSecret":{"token":"fixture-do-not-copy"},
            "gifSearchItems":{"secret":"fixture-do-not-copy"}
        });
        let original = serde_json::to_vec(&legacy).unwrap();
        let secondary = br#"{"giphy-favs":"[\"secondary\"]","unrelatedSecret":"fixture-ignore"}"#;
        std::fs::create_dir_all(primary.parent().unwrap()).unwrap();
        std::fs::create_dir_all(fallback.parent().unwrap()).unwrap();
        std::fs::write(&primary, &original).unwrap();
        std::fs::write(&fallback, secondary).unwrap();
        v2_sdk::update_store(&ctx,SLUG,|store| {
            *store = json!({"giphy-favs":["native-favorite","shared"],"giphy-recent":["native-recent","shared"],"nativeUnknown":{"keep":true}});
            Ok(())
        }).await.unwrap();
        let store = store_with_migration(&ctx).await.unwrap();
        assert_eq!(
            ids(&store, "giphy", false),
            vec!["native-favorite", "shared", "old-favorite", "secondary"]
        );
        assert_eq!(
            ids(&store, "giphy", true),
            vec!["native-recent", "shared", "old-recent"]
        );
        assert_eq!(ids(&store, "giphy-clips", false), vec!["clip"]);
        assert_eq!(ids(&store, "klipy", true), vec!["klipy-old"]);
        assert_eq!(ids(&store, "finergifs", false), vec!["02x03-hello"]);
        assert_eq!(store["service"], "klipy");
        assert_eq!(store["nativeUnknown"]["keep"], true);
        assert_eq!(store[MIGRATED], true);
        for key in [
            "apiKey",
            "unknownSecret",
            "gifSearchItems",
            "unrelatedSecret",
        ] {
            assert!(store.get(key).is_none());
        }
        assert!(!store.to_string().contains("fixture-do-not-copy"));
        assert_eq!(std::fs::read(&primary).unwrap(), original);
        assert_eq!(std::fs::read(&fallback).unwrap(), secondary);
        std::fs::write(&primary, br#"{"giphy-favs":"[\"later\"]"}"#).unwrap();
        assert_eq!(store_with_migration(&ctx).await.unwrap(), store);
        assert!(!missing_snapshots(&store, "favorites").is_empty());
    }

    #[tokio::test]
    async fn invalid_known_legacy_collection_never_marks_or_overwrites_native_store() {
        let (_temp, ctx, _) = context();
        let path = ctx
            .paths
            .data_dir
            .join("extensions/gif-search/.tezbar-local-storage.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let bytes = br#"{"giphy-favs":"not-json","unrelatedSecret":"never-include-in-error"}"#;
        std::fs::write(&path, bytes).unwrap();
        let error = store_with_migration(&ctx).await.unwrap_err();
        assert!(!error.contains("never-include-in-error"));
        assert_eq!(v2_sdk::read_store(&ctx, SLUG).await.unwrap(), json!({}));
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}
