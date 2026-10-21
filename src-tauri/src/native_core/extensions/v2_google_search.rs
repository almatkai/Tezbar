//! Native port of the installed Google Search extension (mblode et al., MIT).
//! Behavior transcribed from its installed `.sc-build/index.js` and
//! `.sc-build/searchSelectText.js`; no JavaScript bundle is executed.
use crate::native_core::common::{CoreContext, CoreResult};
use serde_json::{json, Value};
use std::time::Duration;

use super::v2_sdk;

const SLUG: &str = "google-search";
const SUGGEST_URL: &str = "https://suggestqueries.google.com/complete/search";

pub fn commands() -> &'static [&'static str] {
    &["index", "searchSelectText"]
}

fn preference(payload: &Value, name: &str, default: bool) -> bool {
    let value = &payload["preferences"][name];
    value
        .as_bool()
        .or_else(|| value.as_str().and_then(|v| v.parse().ok()))
        .unwrap_or(default)
}

fn search_url(query: &str) -> String {
    format!(
        "https://www.google.com/search?q={}",
        v2_sdk::encode_component(query)
    )
}

fn result(
    query: &str,
    description: String,
    url: String,
    history: bool,
    navigation: bool,
    id: String,
) -> Value {
    json!({"id":id,"query":query,"description":description,"url":url,"isHistory":history,"isNavigation":navigation})
}

fn history_items(store: &Value) -> Vec<Value> {
    store["history"].as_array().cloned().unwrap_or_default()
}

// Legacy Raycast LocalStorage shim stores the `history` key as a JSON string.
// Read it at runtime only, once, from the installed extension's own directory.
async fn store_with_migration(ctx: &CoreContext) -> CoreResult {
    let store = v2_sdk::read_store(ctx, SLUG).await?;
    if store.get("history").is_some() {
        return Ok(store);
    }
    let path = ctx
        .paths
        .data_dir
        .join("extensions")
        .join(SLUG)
        .join(".tezbar-local-storage.json");
    let legacy = tokio::task::spawn_blocking(move || -> Result<Option<Vec<Value>>, String> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        let object: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let Some(raw) = object.get("history").and_then(Value::as_str) else {
            return Ok(None);
        };
        let value: Value = serde_json::from_str(raw).map_err(|e| e.to_string())?;
        Ok(value.as_array().cloned())
    })
    .await
    .map_err(|e| e.to_string())??;
    v2_sdk::update_store(ctx, SLUG, move |data| {
        if data.get("history").is_none() {
            data["history"] = json!(legacy.unwrap_or_default());
        }
        Ok(())
    })
    .await
}

fn parse_suggestions(bytes: &[u8]) -> Result<Vec<Value>, String> {
    // iconv-lite's iso-8859-1 decoder maps every byte directly to U+00XX.
    let text: String = bytes.iter().map(|byte| char::from(*byte)).collect();
    let body: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let items = body
        .get(1)
        .and_then(Value::as_array)
        .ok_or("Google suggestions omitted items")?;
    let descriptions = body
        .get(2)
        .and_then(Value::as_array)
        .ok_or("Google suggestions omitted descriptions")?;
    let types = body
        .get(4)
        .and_then(|x| x.get("google:suggesttype"))
        .and_then(Value::as_array)
        .ok_or("Google suggestions omitted types")?;
    let mut output = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let Some(item) = item.as_str() else { continue };
        let kind = types.get(index).and_then(Value::as_str).unwrap_or("");
        let description = descriptions
            .get(index)
            .and_then(Value::as_str)
            .unwrap_or("");
        let id = format!("suggestion:{index}:{item}");
        match kind {
            "NAVIGATION" => output.push(result(
                if description.is_empty() {
                    item
                } else {
                    description
                },
                format!("Open URL for '{item}'"),
                item.to_owned(),
                false,
                true,
                id,
            )),
            "QUERY" => output.push(result(
                item,
                format!("Search Google for '{item}'"),
                search_url(item),
                false,
                false,
                id,
            )),
            _ => {}
        }
    }
    Ok(output)
}

async fn suggestions(query: &str) -> Result<Vec<Value>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!(
        "{SUGGEST_URL}?hl=en-us&output=chrome&q={}",
        v2_sdk::encode_component(query)
    );
    fetch_suggestions(&client, &url).await
}

async fn fetch_suggestions(client: &reqwest::Client, url: &str) -> Result<Vec<Value>, String> {
    let response = client
        .get(url)
        .header("Content-Type", "text/plain; charset=UTF-8")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    parse_suggestions(&bytes)
}

fn matching_history(history: &[Value], query: &str) -> Vec<Value> {
    let needle = query.to_lowercase();
    history
        .iter()
        .filter(|item| {
            item["query"]
                .as_str()
                .is_some_and(|text| text.to_lowercase().contains(&needle))
        })
        .cloned()
        .collect()
}

fn action_id(verb: &str, index: usize) -> String {
    format!("google-search:{verb}:{index}")
}

fn build_view(state: &mut Value) -> Value {
    let query = state["searchText"].as_str().unwrap_or("").to_owned();
    let history = state["history"].as_array().cloned().unwrap_or_default();
    let auto = state["suggestions"].as_array().cloned().unwrap_or_default();
    let mut results = Vec::new();
    if !query.is_empty() {
        results.push(result(
            &query,
            format!("Search Google for '{query}'"),
            search_url(&query),
            false,
            false,
            format!("static:{query}"),
        ));
    }
    results.extend(matching_history(&history, &query));
    results.extend(auto);
    // Legacy code deduplicates by ID, not by query or URL.
    let mut ids = std::collections::HashSet::new();
    results.retain(|item| ids.insert(item["id"].as_str().unwrap_or("").to_owned()));
    state["results"] = json!(results);
    let mut actions = Vec::new();
    let mut rows = Vec::new();
    for (index, item) in results.iter().enumerate() {
        let mut ids = Vec::new();
        for (verb, title, kind) in [
            ("open", "Open in Browser", "open"),
            ("copy-url", "Copy URL to Clipboard", "copy"),
            ("copy-suggestion", "Copy Suggestion to Clipboard", "copy"),
            ("set-search", "Set as Search Text", "action"),
        ] {
            let id = action_id(verb, index);
            actions.push(json!({"id":id,"title":title,"kind":kind}));
            ids.push(id);
        }
        if item["isHistory"] == true {
            let id = action_id("remove", index);
            actions.push(json!({"id":id,"title":"Remove from History","kind":"action"}));
            ids.push(id);
        }
        let id = action_id("clear", index);
        actions.push(json!({"id":id,"title":"Clear All History","kind":"action"}));
        ids.push(id);
        rows.push(json!({"type":"List.Item","props":{"id":item["id"],"title":item["query"],"subtitle":item["description"],"icon":if item["isHistory"] == true {"Icon.Clock"} else if item["isNavigation"] == true {"Icon.Link"} else {"Icon.MagnifyingGlass"},"actionIds":ids}}));
    }
    v2_sdk::view(
        "Google Search",
        json!({"type":"List","props":{"searchText":query,"searchBarPlaceholder":"Search Google or enter a URL...","__hasServerSearch":true},"children":[{"type":"List.Section","props":{"title":"Results","subtitle":results.len().to_string()},"children":rows}]}),
        actions,
    )
}

fn action_target<'a>(state: &'a Value, id: &str) -> Result<(&'a Value, String), String> {
    let parts: Vec<_> = id.split(':').collect();
    if parts.len() != 3 || parts[0] != "google-search" {
        return Err("Unknown Google Search action".into());
    }
    let index: usize = parts[2]
        .parse()
        .map_err(|_| "Invalid Google Search result index")?;
    let item = state["results"]
        .as_array()
        .and_then(|a| a.get(index))
        .ok_or("Google Search result is no longer available")?;
    Ok((item, parts[1].to_owned()))
}

async fn save_history(ctx: &CoreContext, item: Value) -> CoreResult {
    v2_sdk::update_store(ctx, SLUG, move |data| {
        let history = data["history"]
            .as_array_mut()
            .ok_or("Invalid Google Search history")?;
        let query = item["query"]
            .as_str()
            .ok_or("Invalid Google Search result")?
            .to_owned();
        if !history.iter().any(|entry| entry["query"] == query) {
            let mut item = item;
            item["isHistory"] = json!(true);
            history.insert(0, item);
        }
        Ok(())
    })
    .await
}

async fn selected_text(ctx: &CoreContext, fallback: bool) -> Result<String, String> {
    match ctx.request("core:platform:selected-text", vec![]).await {
        Ok(value) => value
            .as_str()
            .map(str::to_owned)
            .ok_or("Selected text was not a string".into()),
        Err(error) if fallback => {
            let text = ctx
                .request("clipboard:read", vec![])
                .await
                .map_err(|e| format!("Selected text failed ({error}); clipboard failed: {e}"))?;
            match text.as_str() {
                Some("") | None => Err("No text selected and clipboard is empty".into()),
                Some(text) => Ok(text.to_owned()),
            }
        }
        Err(_) => Err("No text selected".into()),
    }
}

pub async fn handle(
    ctx: &CoreContext,
    command: &str,
    operation: &str,
    payload: &Value,
    state: &mut Value,
) -> CoreResult {
    match (command, operation) {
        ("index", "extension:run-command") => {
            let store = store_with_migration(ctx).await?;
            state["searchText"] = json!("");
            state["history"] = if preference(payload, "rememberSearchHistory", true) {
                json!(history_items(&store))
            } else {
                json!([])
            };
            state["suggestions"] = json!([]);
            Ok(build_view(state))
        }
        ("index", "extension:search-text-changed") => {
            let query = payload["searchText"]
                .as_str()
                .ok_or("Missing search text")?
                .to_owned();
            state["searchText"] = json!(query);
            match if query.is_empty() {
                Ok(vec![])
            } else {
                suggestions(&query).await
            } {
                Ok(items) => {
                    state["suggestions"] = json!(items);
                    Ok(build_view(state))
                }
                Err(error) => {
                    state["suggestions"] = json!([]);
                    let mut view = build_view(state);
                    view["effects"] = json!([{"kind":"toast","style":"failure","title":"Could not perform search","message":error}]);
                    Ok(view)
                }
            }
        }
        ("index", "extension:invoke-action") => {
            let id = payload["actionId"].as_str().ok_or("Missing action ID")?;
            let (item, verb) = action_target(state, id)?;
            let item = item.clone();
            match verb.as_str() {
                "open" => {
                    let url = item["url"]
                        .as_str()
                        .ok_or("Google Search result has no URL")?
                        .to_owned();
                    if preference(payload, "rememberSearchHistory", true) {
                        let _ = store_with_migration(ctx).await?;
                        save_history(ctx, item).await?;
                    }
                    v2_sdk::open(ctx, &url).await?;
                    ctx.request("window:hide", vec![]).await?;
                    Ok(v2_sdk::no_view(""))
                }
                "copy-url" | "copy-suggestion" => {
                    let text = if verb == "copy-url" {
                        &item["url"]
                    } else {
                        &item["query"]
                    };
                    v2_sdk::copy(
                        ctx,
                        text.as_str().ok_or("Google Search result has no text")?,
                    )
                    .await?;
                    Ok(build_view(state))
                }
                "set-search" => {
                    let query = item["query"]
                        .as_str()
                        .ok_or("Google Search result has no query")?
                        .to_owned();
                    state["searchText"] = json!(query);
                    match suggestions(&query).await {
                        Ok(items) => {
                            state["suggestions"] = json!(items);
                            Ok(build_view(state))
                        }
                        Err(error) => {
                            state["suggestions"] = json!([]);
                            let mut view = build_view(state);
                            view["effects"] = json!([{"kind":"toast","style":"failure","title":"Could not perform search","message":error}]);
                            Ok(view)
                        }
                    }
                }
                "remove" if item["isHistory"] == true => {
                    let query = item["query"]
                        .as_str()
                        .ok_or("Google Search history item has no query")?
                        .to_owned();
                    let _ = store_with_migration(ctx).await?;
                    let store = v2_sdk::update_store(ctx, SLUG, move |data| {
                        let history = data["history"]
                            .as_array_mut()
                            .ok_or("Invalid Google Search history")?;
                        if let Some(index) =
                            history.iter().position(|entry| entry["query"] == query)
                        {
                            history.remove(index);
                        }
                        Ok(())
                    })
                    .await?;
                    state["history"] = json!(history_items(&store));
                    Ok(build_view(state))
                }
                "clear" => {
                    v2_sdk::update_store(ctx, SLUG, |data| {
                        data["history"] = json!([]);
                        Ok(())
                    })
                    .await?;
                    state["history"] = json!([]);
                    Ok(build_view(state))
                }
                _ => Err("Unknown Google Search action".into()),
            }
        }
        ("index", "extension:refresh-session") => Ok(build_view(state)),
        ("index", "extension:load-more") => Ok(v2_sdk::unchanged()),
        ("index", "extension:dispose-session") => Ok(v2_sdk::unchanged()),
        ("searchSelectText", "extension:run-command") => {
            let query =
                selected_text(ctx, preference(payload, "useClipboardFallback", false)).await?;
            let url = search_url(&query);
            v2_sdk::open(ctx, &url).await?;
            ctx.request("window:hide", vec![]).await?;
            // Installed source writes history unconditionally, even when hidden by preference.
            let _ = store_with_migration(ctx).await?;
            let item = result(
                &query,
                format!("Search Google for '{query}'"),
                url,
                false,
                false,
                format!("selected:{}", uuid::Uuid::new_v4()),
            );
            v2_sdk::update_store(ctx, SLUG, move |data| {
                data["history"]
                    .as_array_mut()
                    .ok_or("Invalid Google Search history")?
                    .insert(0, item);
                Ok(())
            })
            .await?;
            Ok(v2_sdk::no_view(""))
        }
        _ => Err("Unsupported Google Search operation".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_core::common::CorePaths;
    use std::sync::{Arc, Mutex};

    fn fixture() -> (tempfile::TempDir, CoreContext, Arc<Mutex<Vec<String>>>) {
        let temp = tempfile::tempdir().unwrap();
        let paths = CorePaths {
            data_dir: temp.path().join("data"),
            config_dir: temp.path().join("config"),
            resource_dir: temp.path().join("resources"),
            temp_dir: temp.path().join("tmp"),
            home_dir: temp.path().join("home"),
        };
        let ctx = CoreContext::headless(paths);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let record = calls.clone();
        ctx.set_router(Arc::new(move |channel, args| {
            let record = record.clone();
            Box::pin(async move {
                record.lock().unwrap().push(channel.clone());
                match channel.as_str() {
                    "core:platform:selected-text" => Err("selection unavailable".into()),
                    "clipboard:read" => Ok(json!("Café & tea")),
                    "shell:open" => {
                        assert_eq!(args[0], search_url("Café & tea"));
                        Ok(json!({"ok":true}))
                    }
                    "window:hide" => Ok(json!({"ok":true})),
                    _ => Err(format!("unexpected route: {channel}")),
                }
            })
        }))
        .unwrap();
        (temp, ctx, calls)
    }

    #[test]
    fn latin1_suggestions_and_navigation() {
        let mut input = br#"["cafe",["cafe","example.com"],["","Caf"#.to_vec();
        input.push(0xe9);
        input.extend_from_slice(br#""],[],{"google:suggesttype":["QUERY","NAVIGATION"]}]"#);
        let results = parse_suggestions(&input).unwrap();
        assert_eq!(results[0]["url"], search_url("cafe"));
        assert_eq!(results[1]["query"], "Café");
        assert_eq!(results[1]["url"], "example.com");
    }

    #[test]
    fn history_order_and_query_matching() {
        let history = vec![
            json!({"id":"one","query":"Cafe","url":"a"}),
            json!({"id":"two","query":"Tea","url":"b"}),
        ];
        assert_eq!(matching_history(&history, "CAF").len(), 1);
        let mut state = json!({"searchText":"ca","history":history,"suggestions":[]});
        let view = build_view(&mut state);
        assert_eq!(
            view["root"]["children"][0]["children"][0]["props"]["title"],
            "ca"
        );
        assert_eq!(
            view["root"]["children"][0]["children"][1]["props"]["title"],
            "Cafe"
        );
    }

    #[test]
    fn invalid_action_never_reaches_router() {
        let state = json!({"results":[{"query":"x","url":"https://example.com"}]});
        assert!(action_target(&state, "google-search:open:1").is_err());
        assert!(action_target(&state, "shell:open:0").is_err());
    }

    #[tokio::test]
    async fn selected_text_falls_back_only_when_enabled_and_writes_real_history() {
        let (_temp, ctx, calls) = fixture();
        let mut state = json!({});
        let denied = handle(
            &ctx,
            "searchSelectText",
            "extension:run-command",
            &json!({"preferences":{"useClipboardFallback":false}}),
            &mut state,
        )
        .await;
        assert!(denied.is_err());
        assert_eq!(
            calls.lock().unwrap().as_slice(),
            &["core:platform:selected-text"]
        );
        calls.lock().unwrap().clear();
        let result = handle(
            &ctx,
            "searchSelectText",
            "extension:run-command",
            &json!({"preferences":{"useClipboardFallback":true,"rememberSearchHistory":false}}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(result["mode"], "no-view");
        assert_eq!(
            calls.lock().unwrap().as_slice(),
            &[
                "core:platform:selected-text",
                "clipboard:read",
                "shell:open",
                "window:hide"
            ]
        );
        let store = v2_sdk::read_store(&ctx, SLUG).await.unwrap();
        assert_eq!(store["history"][0]["query"], "Café & tea");
    }

    #[tokio::test]
    async fn legacy_history_migrates_in_temporary_fixture() {
        let (_temp, ctx, _calls) = fixture();
        let path = ctx
            .paths
            .data_dir
            .join("extensions")
            .join(SLUG)
            .join(".tezbar-local-storage.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, json!({"history":"[{\"id\":\"old\",\"query\":\"legacy\",\"url\":\"https://example.com\"}]"}).to_string()).unwrap();
        let migrated = store_with_migration(&ctx).await.unwrap();
        assert_eq!(migrated["history"][0]["query"], "legacy");
        assert_eq!(
            v2_sdk::read_store(&ctx, SLUG).await.unwrap()["history"][0]["id"],
            "old"
        );
    }

    #[tokio::test]
    async fn suggestions_use_http_response_bytes() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 1024];
            let n = stream.read(&mut request).await.unwrap();
            assert!(String::from_utf8_lossy(&request[..n]).starts_with("GET /fixture?q=Caf%C3%A9"));
            let mut body =
                br#"["Cafe",["Cafe"],[""],[],{"google:suggesttype":["QUERY"]}]"#.to_vec();
            body.extend_from_slice(b" ");
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(header.as_bytes()).await.unwrap();
            stream.write_all(&body).await.unwrap();
        });
        let client = reqwest::Client::new();
        let results = fetch_suggestions(
            &client,
            &format!(
                "http://{address}/fixture?q={}",
                v2_sdk::encode_component("Café")
            ),
        )
        .await
        .unwrap();
        server.await.unwrap();
        assert_eq!(results[0]["url"], search_url("Cafe"));
    }
}
