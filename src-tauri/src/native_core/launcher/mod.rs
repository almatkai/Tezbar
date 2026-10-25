//! Native launcher search, local content, clipboard and terminal metadata.
mod clipboard;
mod clipboard_native;
mod db;
mod providers;
mod storage;
mod stores;
mod terminal;
mod text;
use crate::native_core::common::{now_ms, CoreContext, CoreResult, HandlerResult};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use stores::{string, text_arg};
pub const CHANNELS: &[&str] = &[
    "search:all",
    "search:candidates",
    "search:execute",
    "search:record-usage",
    "search:benchmark:run",
    "search:benchmark:history",
    "path:complete",
    "path:resolve-directory",
    "directory-visit:record",
    "clipboard:list",
    "clipboard:restore",
    "clipboard:delete",
    "clipboard:toggle-pin",
    "clipboard:reveal",
    "clipboard:image",
    "clipboard:clear",
    "clipboard:read",
    "clipboard:write",
    "clipboard:write-png",
    "snippets:list",
    "snippets:copy",
    "snippets:add",
    "snippets:update",
    "snippets:delete",
    "notes:list",
    "notes:append",
    "notes:update",
    "notes:delete",
    "get-settings",
    "save-settings",
    "toggle-command-enabled",
    "update-command-hotkey",
    "storage:breakdown",
    "storage:clipboard-config:get",
    "storage:clipboard-config:set",
    "storage:clear-clipboard-images",
    "storage:vacuum-search-db",
    "storage:clear-chromium-cache",
    "terminal:list",
    "terminal:update",
    "terminal:delete",
    "terminal:detach",
    "terminal:get-prompt-info",
    "terminal:native-create",
    "terminal:native-attach",
    "terminal:native-restore",
    "terminal:native-exit",
    "terminal:native-detach",
    "core:terminal:detach",
    "terminal:exit",
    "core:terminal:exit",
    "core:apps:list",
    "core:apps:bundle-id",
    "core:launcher:documents",
    "core:launcher:refresh",
    "core:launcher:status",
    "core:launcher:query-intent",
    "core:clipboard:cleanup",
    "core:clipboard:capture-text",
];
#[derive(Clone)]
pub struct LauncherCore {
    inner: Arc<Inner>,
}
struct Inner {
    ctx: CoreContext,
    db: db::Database,
    stores: stores::Stores,
    clipboard: clipboard::ClipboardCore,
    terminal: terminal::TerminalCore,
    apps: tokio::sync::RwLock<Vec<Value>>,
    started: AtomicBool,
    lifecycle: tokio::sync::Mutex<()>,
    stop: Arc<AtomicBool>,
    wake: tokio::sync::Notify,
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    refresh: tokio::sync::Mutex<()>,
    shortcuts: tokio::sync::Mutex<()>,
    scanning: Arc<tokio::sync::Mutex<()>>,
    status: Mutex<Value>,
}
impl LauncherCore {
    pub fn new(ctx: CoreContext) -> Self {
        Self {
            inner: Arc::new(Inner {
                db: db::Database::new(ctx.paths.data_dir.join("search/index.sqlite3")),
                stores: stores::Stores::new(ctx.paths.data_dir.clone()),
                clipboard: clipboard::ClipboardCore::new(ctx.clone()),
                terminal: terminal::TerminalCore::new(ctx.clone()),
                ctx,
                apps: tokio::sync::RwLock::new(vec![]),
                started: AtomicBool::new(false),
                lifecycle: tokio::sync::Mutex::new(()),
                stop: Arc::new(AtomicBool::new(false)),
                wake: tokio::sync::Notify::new(),
                tasks: Mutex::new(vec![]),
                refresh: tokio::sync::Mutex::new(()),
                shortcuts: tokio::sync::Mutex::new(()),
                scanning: Arc::new(tokio::sync::Mutex::new(())),
                status: Mutex::new(json!({"status":"idle","indexedFiles":0})),
            }),
        }
    }
    pub async fn start(&self) -> Result<(), String> {
        let _lifecycle = self.inner.lifecycle.lock().await;
        if self.inner.stop.load(Ordering::Acquire) {
            return Err("Launcher core has shut down".into());
        }
        if self.inner.started.load(Ordering::Acquire) {
            return Ok(());
        }
        self.inner.db.call(|_| Ok(Value::Null)).await?;
        self.inner.terminal.initialize().await?;
        self.inner.clipboard.start(self.inner.ctx.app.is_some());
        let this = self.clone();
        let refresh = tokio::spawn(async move {
            let _ = this
                .inner
                .clipboard
                .handle("core:clipboard:cleanup", &[])
                .await;
            let mut tick = 0;
            loop {
                if this.inner.stop.load(Ordering::Relaxed) {
                    break;
                }
                if let Err(e) = this.refresh(tick % 6 == 0).await {
                    log::warn!("Launcher refresh: {e}");
                }
                let _ = this.inner.terminal.tick().await;
                tick += 1;
                tokio::select! {_=this.inner.wake.notified()=>break,_=tokio::time::sleep(std::time::Duration::from_secs(10))=>{}}
            }
        });
        self.inner
            .tasks
            .lock()
            .map_err(|e| e.to_string())?
            .push(refresh);
        if self.inner.ctx.app.is_some() {
            let this = self.clone();
            let scan = tokio::spawn(async move {
                tokio::select! {_=this.inner.wake.notified()=>return,_=tokio::time::sleep(std::time::Duration::from_secs(5))=>{}}
                let _guard = this.inner.scanning.lock().await;
                this.set_status(json!({"status":"indexing"}));
                let result = providers::scan(
                    this.inner.ctx.clone(),
                    this.inner.db.clone(),
                    this.inner.stop.clone(),
                )
                .await;
                this.set_status(match result {
                    Ok(()) => json!({"status":"ready"}),
                    Err(e) => json!({"status":"error","error":e}),
                });
            });
            let ctx = self.inner.ctx.clone();
            let db = self.inner.db.clone();
            let stop = self.inner.stop.clone();
            let scanning = self.inner.scanning.clone();
            let watcher =
                tokio::spawn(async move { providers::watch(ctx, db, stop, scanning).await });
            self.inner
                .tasks
                .lock()
                .map_err(|e| e.to_string())?
                .extend([scan, watcher]);
        }
        self.inner.started.store(true, Ordering::Release);
        Ok(())
    }
    async fn update_command_hotkey(&self, args: Vec<Value>) -> CoreResult {
        // Keep registration and persistence ordered across concurrent updates.
        let _guard = self.inner.shortcuts.lock().await;
        let id = text_arg(&args, 0)?.to_owned();
        let hotkey = text_arg(&args, 1)?.to_owned();
        let previous = self.config().await?["commandHotkeys"][&id]
            .as_str()
            .unwrap_or("")
            .to_owned();
        let result = self
            .inner
            .ctx
            .request(
                "core:shortcut:update-command",
                vec![json!(id), json!(hotkey)],
            )
            .await?;
        if result["ok"] != true {
            return Ok(result);
        }
        let previous = result["previousAccelerator"]
            .as_str()
            .unwrap_or(&previous)
            .to_owned();
        let hotkey = result["accelerator"]
            .as_str()
            .ok_or("Shortcut registration omitted accelerator")?
            .to_owned();
        let config = self.inner.ctx.config.clone();
        let saved_id = id.clone();
        let persisted = tokio::task::spawn_blocking(move || {
            let id = saved_id;
            config.update(|v| {
                if !v["commandHotkeys"].is_object() {
                    v["commandHotkeys"] = json!({})
                }
                if hotkey.trim().is_empty() {
                    v["commandHotkeys"].as_object_mut().unwrap().remove(&id);
                } else {
                    v["commandHotkeys"][id] = json!(hotkey)
                }
                Ok(())
            })
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|v| v);
        if let Err(error) = persisted {
            let rollback = self
                .inner
                .ctx
                .request(
                    "core:shortcut:update-command",
                    vec![json!(id), json!(previous)],
                )
                .await;
            return Err(match rollback {
                Ok(v) if v["ok"] == true => error,
                other => format!("{error}; shortcut rollback failed: {other:?}"),
            });
        }
        self.inner.ctx.emit("llm-config-changed", json!({}));
        Ok(result)
    }

    pub async fn shutdown(&self) -> Result<(), String> {
        let _lifecycle = self.inner.lifecycle.lock().await;
        if self.inner.stop.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        self.inner.wake.notify_waiters();
        let tasks = std::mem::take(&mut *self.inner.tasks.lock().map_err(|e| e.to_string())?);
        for mut task in tasks {
            if tokio::time::timeout(std::time::Duration::from_secs(3), &mut task)
                .await
                .is_err()
            {
                task.abort();
                let _ = task.await;
            }
        }
        self.inner.clipboard.shutdown().await;
        let checkpoint = self
            .inner
            .db
            .call(|c| {
                c.execute_batch("PRAGMA wal_checkpoint(PASSIVE)")
                    .map_err(|e| e.to_string())?;
                Ok(Value::Null)
            })
            .await;
        self.inner.db.shutdown().await?;
        checkpoint?;
        Ok(())
    }
    fn set_status(&self, v: Value) {
        if let Ok(mut s) = self.inner.status.lock() {
            *s = v;
        }
    }
    async fn local_refresh(&self) -> CoreResult {
        let docs = self.inner.stores.documents().await?;
        let docs = docs.as_array().cloned().ok_or("Invalid local documents")?;
        self.inner
            .db
            .upsert(
                docs,
                vec![
                    "quick-notes".into(),
                    "snippets".into(),
                    "quick-links".into(),
                    "clipboard".into(),
                ],
            )
            .await
    }
    async fn refresh(&self, apps: bool) -> CoreResult {
        let _guard = self.inner.refresh.lock().await;
        self.local_refresh().await?;
        let native = self.inner.ctx.request("native-commands:list", vec![]).await;
        match native {
            Ok(v) => {
                let docs = providers::command_documents(
                    v.as_array().ok_or("Invalid native command descriptors")?,
                );
                self.inner
                    .db
                    .upsert(docs, vec!["native-command".into(), "commands".into()])
                    .await?;
            }
            Err(e) => {
                self.inner
                    .db
                    .upsert(providers::command_documents(&[]), vec!["commands".into()])
                    .await?;
                log::debug!("Native command descriptors: {e}");
            }
        }
        match self
            .inner
            .ctx
            .request("core:extensions:commands", vec![])
            .await
        {
            Ok(v) => {
                let config = self.config().await?;
                self.inner
                    .db
                    .upsert(
                        providers::extension_documents(
                            v.as_array()
                                .ok_or("Invalid extension command descriptors")?,
                            &config,
                        ),
                        vec!["extensions".into()],
                    )
                    .await?;
            }
            Err(e) => log::debug!("Extension command descriptors: {e}"),
        };
        if apps && self.inner.ctx.app.is_some() {
            let applications = providers::applications(&self.inner.ctx).await?;
            let applications = applications
                .as_array()
                .cloned()
                .ok_or("Invalid application enumeration")?;
            self.inner
                .db
                .upsert(
                    providers::app_documents(&applications),
                    vec!["applications".into()],
                )
                .await?;
            *self.inner.apps.write().await = applications;
        }
        Ok(Value::Null)
    }
    async fn config(&self) -> CoreResult {
        let config = self.inner.ctx.config.clone();
        tokio::task::spawn_blocking(move || config.read())
            .await
            .map_err(|e| e.to_string())?
    }
    async fn patch(&self, v: Value) -> CoreResult {
        let ctx = self.inner.ctx.clone();
        let result = tokio::task::spawn_blocking(move || ctx.config.patch(&v))
            .await
            .map_err(|e| e.to_string())??;
        self.inner.ctx.emit("llm-config-changed", json!({}));
        Ok(result)
    }
    pub async fn handle(&self, channel: &str, args: &[Value]) -> HandlerResult {
        if !CHANNELS.contains(&channel) {
            return None;
        }
        Some(self.dispatch(channel, args).await)
    }
    async fn dispatch(&self, c: &str, a: &[Value]) -> CoreResult {
        if c.starts_with("notes:")
            || c.starts_with("snippets:") && c != "snippets:copy"
            || c == "directory-visit:record"
        {
            let result = self.inner.stores.handle(c, a).await?;
            if (c.starts_with("notes:") || c.starts_with("snippets:")) && !c.ends_with(":list") {
                self.local_refresh().await?;
            }
            return Ok(result);
        }
        if c.starts_with("clipboard:") && c != "clipboard:reveal"
            || c.starts_with("core:clipboard:")
            || c == "storage:clear-clipboard-images"
        {
            // Clipboard contents are deliberately excluded from global search.
            // Clipboard writes must not rebuild unrelated notes/snippets/links.
            return self.inner.clipboard.handle(c, a).await;
        }
        if c.starts_with("terminal:") || c == "core:terminal:exit" || c == "core:terminal:detach" {
            return self.inner.terminal.handle(c, a).await;
        }
        match c {
            "get-settings" => {
                let v = self.config().await?;
                Ok(
                    json!({"commandHotkeys":v["commandHotkeys"].as_object().cloned().unwrap_or_default(),"commandAliases":v["commandAliases"].as_object().cloned().unwrap_or_default(),"disabledCommands":v["disabledCommands"].as_object().cloned().unwrap_or_default()}),
                )
            }
            "save-settings" => {
                let v = a.first().ok_or("Settings patch required")?;
                if !v.is_object() {
                    return Err("Settings patch must be an object".into());
                }
                if let Some(aliases) = v.get("commandAliases") {
                    if !aliases.is_object()
                        || aliases
                            .as_object()
                            .unwrap()
                            .values()
                            .any(|v| !v.is_string())
                    {
                        return Err("Command aliases must map IDs to strings".into());
                    }
                    self.patch(json!({"commandAliases":aliases})).await?;
                }
                self.refresh(false).await?;
                Ok(json!({"ok":true}))
            }
            "toggle-command-enabled" => {
                let id = text_arg(a, 0)?.to_owned();
                let enabled = a
                    .get(1)
                    .and_then(Value::as_bool)
                    .ok_or("Enabled must be a boolean")?;
                let config = self.inner.ctx.config.clone();
                tokio::task::spawn_blocking(move || {
                    config.update(|v| {
                        if !v["disabledCommands"].is_object() {
                            v["disabledCommands"] = json!({})
                        }
                        if enabled {
                            v["disabledCommands"].as_object_mut().unwrap().remove(&id);
                        } else {
                            v["disabledCommands"][id] = json!(true)
                        }
                        Ok(())
                    })
                })
                .await
                .map_err(|e| e.to_string())??;
                self.inner.ctx.emit("llm-config-changed", json!({}));
                self.refresh(false).await?;
                Ok(json!(true))
            }
            "update-command-hotkey" => {
                let this = self.clone();
                let args = a.to_vec();
                // Finish registration/save/rollback even if the renderer cancels IPC.
                tokio::spawn(async move { this.update_command_hotkey(args).await })
                    .await
                    .map_err(|error| error.to_string())?
            }

            "storage:breakdown" => storage::breakdown(self.inner.ctx.clone()).await,
            "storage:clipboard-config:get" => Ok(storage::clipboard_config(&self.config().await?)),
            "storage:clipboard-config:set" => {
                let p = a.first().ok_or("Clipboard configuration required")?;
                if !p.is_object() {
                    return Err("Clipboard configuration must be an object".into());
                }
                let mut patch = json!({});
                for (k, config_key) in [
                    ("watchEnabled", "clipboardWatchEnabled"),
                    ("captureImages", "clipboardCaptureImages"),
                ] {
                    if let Some(v) = p.get(k) {
                        if !v.is_boolean() {
                            return Err(format!("{k} must be a boolean"));
                        }
                        patch[config_key] = v.clone()
                    }
                }
                if let Some(v) = p.get("maxImageMegapixels") {
                    if !v.as_f64().is_some_and(|n| n > 0.) {
                        return Err("maxImageMegapixels must be positive".into());
                    }
                    patch["clipboardMaxImageMegapixels"] = v.clone();
                }
                self.patch(patch).await?;
                Ok(Value::Null)
            }
            "storage:vacuum-search-db" => {
                storage::vacuum(self.inner.db.clone(), self.inner.ctx.clone()).await
            }
            "storage:clear-chromium-cache" => {
                self.inner
                    .ctx
                    .request("core:platform:clear-webview-cache", vec![])
                    .await
            }
            "core:apps:list" => {
                let mut apps = self.inner.apps.read().await.clone();
                if apps.is_empty() {
                    let v = providers::applications(&self.inner.ctx).await?;
                    apps = v.as_array().cloned().unwrap_or_default();
                    *self.inner.apps.write().await = apps.clone();
                }
                Ok(json!(apps))
            }
            "core:apps:bundle-id" => Ok(providers::bundle_id(text_arg(a, 0)?).await),
            "core:launcher:documents" => self.inner.stores.documents().await,
            "core:launcher:refresh" => self.refresh(true).await,
            "core:launcher:query-intent" => Ok(text::intent(text_arg(a, 0)?)),
            "core:launcher:status" => self
                .inner
                .status
                .lock()
                .map(|s| s.clone())
                .map_err(|e| e.to_string()),
            "path:resolve-directory" => {
                let home = self.inner.ctx.paths.home_dir.clone();
                let raw = text_arg(a, 0)?.to_owned();
                tokio::task::spawn_blocking(move || {
                    let p = providers::resolve(&home, &raw);
                    Ok(if p.is_dir() {
                        json!(p.canonicalize().unwrap_or(p))
                    } else {
                        Value::Null
                    })
                })
                .await
                .map_err(|e| e.to_string())?
            }
            "path:complete" => {
                let ctx = self.inner.ctx.clone();
                let q = text_arg(a, 0)?.to_owned();
                let apps = self.inner.apps.read().await.clone();
                tokio::task::spawn_blocking(move || providers::complete(&ctx, &q, &apps))
                    .await
                    .map_err(|e| e.to_string())?
            }
            "clipboard:reveal" => {
                let list = self.inner.clipboard.handle("clipboard:list", &[]).await?;
                let id = text_arg(a, 0)?;
                let path = list
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|i| i["id"] == id)
                    .and_then(|i| i["imagePath"].as_str().or_else(|| i["paths"][0].as_str()));
                match path {
                    Some(p) => {
                        self.inner
                            .ctx
                            .request("core:platform:reveal-file", vec![json!(p)])
                            .await?;
                        Ok(json!(true))
                    }
                    None => Ok(json!(false)),
                }
            }
            "snippets:copy" => self.copy_snippet(text_arg(a, 0)?).await,
            "search:all" => self.search(text_arg(a, 0)?).await,
            "search:candidates" => self.inner.db.search(String::new(), true).await,
            "search:execute" => {
                let payload = a.first().ok_or("Search action required")?;
                let action = payload.get("action").unwrap_or(payload);
                let context = payload.get("context").cloned().unwrap_or(json!({}));
                let result = if action["type"] == "copy-text"
                    && context["resultId"]
                        .as_str()
                        .is_some_and(|id| id.starts_with("snippet:"))
                {
                    self.copy_snippet(context["resultId"].as_str().unwrap())
                        .await
                } else {
                    self.execute(action).await
                };
                let success = result.as_ref().is_ok_and(|v| v["ok"] == true);
                if success && action["type"] == "open-with-app" {
                    if let (Some(query), Some(name)) =
                        (context["query"].as_str(), action["appName"].as_str())
                    {
                        if let Err(error) = self.record_alias(query, name).await {
                            log::warn!("Open-with alias persistence: {error}");
                        }
                    }
                }
                self.inner
                    .db
                    .record(action_id(action, &context)?, context, success)
                    .await?;
                match result {
                    Ok(v) => Ok(v),
                    Err(e) => Ok(json!({"ok":false,"message":e})),
                }
            }
            "search:record-usage" => {
                let payload = a.first().ok_or("Search usage required")?;
                let action = payload
                    .get("action")
                    .filter(|a| a.is_object())
                    .ok_or("Search usage requires action envelope")?;
                let context = payload.get("context").cloned().unwrap_or(json!({}));
                self.inner
                    .db
                    .record(action_id(action, &context)?, context, true)
                    .await
            }
            "search:benchmark:history" => self.inner.db.read(benchmark_history).await,
            "search:benchmark:run" => {
                let mut p5 = 0.;
                let mut p10 = 0.;
                for (q, category, prefix) in [
                    ("open safari", "applications", ""),
                    ("wifi off", "commands", "command:wifi-off"),
                    ("quick note", "quick-notes", ""),
                    ("clipboard history", "clipboard", ""),
                    ("snippet date", "snippets", ""),
                ] {
                    let r = self.search(q).await?;
                    let r = r.as_array().ok_or("Invalid benchmark search")?;
                    for (i, d) in r.iter().take(10).enumerate() {
                        if d["category"] == category
                            && d["id"].as_str().unwrap_or("").starts_with(prefix)
                        {
                            p10 += 0.1 / 6.;
                            if i < 5 {
                                p5 += 0.2 / 6.;
                            }
                        }
                    }
                }
                self.inner.db.call(move|c|{let click:f64=c.query_row("SELECT coalesce(avg(rank),0) FROM click_events WHERE success=1",[],|r|r.get(0)).map_err(|e|e.to_string())?;c.execute("INSERT INTO benchmark_snapshots(created_at,precision_at_5,precision_at_10,avg_click_rank,benchmark_size) VALUES(?,?,?,?,6)",rusqlite::params![now_ms(),p5,p10,click]).map_err(|e|e.to_string())?;Ok(Value::Null)}).await
            }
            _ => Err(format!("Unhandled launcher channel: {c}")),
        }
    }
    async fn search(&self, input: &str) -> CoreResult {
        self.inner.ctx.note_foreground();
        let deep = input.starts_with('!');
        let q = if deep {
            input[1..].trim()
        } else {
            input.trim()
        };
        let contributions = match self
            .inner
            .ctx
            .request(
                "core:extensions:launcher-search",
                vec![json!({"text":q,"surface":"query","deep":deep})],
            )
            .await
        {
            Ok(rows) => rows.as_array().cloned().unwrap_or_default(),
            Err(error) => {
                log::debug!("Launcher contributions unavailable: {error}");
                vec![]
            }
        };
        if deep && q.chars().count() < 3 {
            return Ok(
                json!([{"id":if q.is_empty(){"deep-search:prompt"}else{"deep-search:keep-typing"},"title":if q.is_empty(){"Deep Search"}else{"Keep typing to search file contents"},"subtitle":if q.is_empty(){"Type after ! to search inside the contents of indexed files"}else{"Deep Search starts after 3 characters"},"category":"knowledge","score":10000,"action":{"type":"invoke-command","commandId":"activate-deep-search","payload":{"query":q}}}]),
            );
        }
        let mut rows = self
            .inner
            .db
            .search(q.to_owned(), false)
            .await?
            .as_array()
            .cloned()
            .unwrap_or_default();
        if deep {
            let hits = self
                .inner
                .ctx
                .request("core:knowledge:search-sources", vec![json!(q), json!(16)])
                .await?;
            rows.retain(|r| r["category"] == "files");
            match self
                .inner
                .ctx
                .request("core:knowledge:search-metadata", vec![json!(q), json!(24)])
                .await
            {
                Ok(metadata) => {
                    for (index, hit) in metadata
                        .as_array()
                        .ok_or("Invalid knowledge metadata search response")?
                        .iter()
                        .enumerate()
                    {
                        let path = string(hit, "path")?;
                        let id = format!("file:{path}");
                        if !rows.iter().any(|r| r["id"] == id) {
                            rows.push(json!({"id":id,"title":hit["title"],"subtitle":path,"category":"files","score":760.+(hit["score"].as_f64().unwrap_or(0.)*160.).round()-index as f64,"action":{"type":"open-file","path":path}}));
                        }
                    }
                }
                Err(error) if error.starts_with("Unknown native core channel:") => {
                    log::debug!("Knowledge metadata route is unavailable: {error}")
                }
                Err(error) => return Err(error),
            }
            let mut seen = std::collections::HashSet::new();
            let mut matched_paths = std::collections::HashSet::new();
            for h in hits.as_array().ok_or("Invalid knowledge search response")? {
                if !seen.insert(h["sourceId"].to_string()) {
                    continue;
                }
                if let Some(path) = h["path"].as_str() {
                    matched_paths.insert(path.to_owned());
                }
                let text = h["text"]
                    .as_str()
                    .unwrap_or("")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                let location = h["pageNumber"]
                    .as_u64()
                    .map(|n| format!(" · page {n}"))
                    .unwrap_or_default();
                rows.push(json!({"id":format!("knowledge:{}",h["chunkId"].as_str().unwrap_or("")),"title":h["title"],"subtitle":format!("Knowledge{location} · {}",text.chars().take(180).collect::<String>()),"category":"knowledge","score":520.+(h["score"].as_f64().unwrap_or(0.)*430.).round(),"action":{"type":"open-file","path":h["path"]}}));
            }
            rows.retain(|r| {
                r["category"] != "files"
                    || !r["action"]["path"]
                        .as_str()
                        .is_some_and(|p| matched_paths.contains(p))
            });
        } else if !q.is_empty() {
            rows.extend(contributions);
            if q.chars().count() >= 3 && !good_match(q, &rows) {
                let max = rows
                    .iter()
                    .filter_map(|r| r["score"].as_f64())
                    .fold(0., f64::max);
                rows.push(json!({"id":format!("deep-search:{}",percent(&q.to_lowercase())),"title":format!("Deep Search “{}”",q.chars().take(72).collect::<String>()),"subtitle":"No strong metadata match · Search inside indexed file contents","category":"knowledge","score":1200f64.max(max+1.),"action":{"type":"invoke-command","commandId":"activate-deep-search","payload":{"query":q}}}));
            }
            rows.push(json!({"id":format!("note-add:{q}"),"title":format!("Add quick note: {}",q.chars().take(64).collect::<String>()),"subtitle":"Quick notes","category":"quick-notes","score":if regex::Regex::new(r"(?i)\bnotes?\b|quick note").unwrap().is_match(q){780}else{120},"action":{"type":"add-note","text":q}}));
        }
        rows.sort_by(|a, b| {
            b["score"]
                .as_f64()
                .unwrap_or(0.)
                .total_cmp(&a["score"].as_f64().unwrap_or(0.))
        });
        // Contributions may intentionally strengthen an existing command ID.
        // Keep the highest-ranked row once, preserving normal ranking order.
        let mut ids = std::collections::HashSet::new();
        rows.retain(|row| ids.insert(row["id"].to_string()));
        rows.truncate(80);
        Ok(json!(rows))
    }
    async fn execute(&self, action: &Value) -> CoreResult {
        let kind = string(action, "type")?;
        let ctx = &self.inner.ctx;
        match kind {
            "open-app" => {
                let name = string(action, "appName")?;
                let mut request = action.clone();
                if self.inner.apps.read().await.is_empty()
                    && ctx.app.is_some()
                    && request["appPath"].as_str().map_or(true, |p| p.is_empty())
                {
                    let discovered = providers::applications(ctx).await?;
                    *self.inner.apps.write().await = discovered
                        .as_array()
                        .cloned()
                        .ok_or("Invalid applications list")?;
                }
                if request["appPath"].as_str().map_or(true, |p| p.is_empty()) {
                    let apps = self.inner.apps.read().await;
                    let found = apps
                        .iter()
                        .find(|app| {
                            app["name"]
                                .as_str()
                                .is_some_and(|n| n.eq_ignore_ascii_case(name))
                        })
                        .cloned();
                    drop(apps);
                    if let Some(app) = found {
                        request["appPath"] = app["path"].clone();
                    }
                }
                ctx.request("core:platform:open-app", vec![request]).await
            }
            "open-file" => {
                let p = providers::resolve(&ctx.paths.home_dir, string(action, "path")?);
                ctx.request("core:platform:open-file", vec![json!(p)]).await
            }
            "open-with-app" => {
                let path = providers::resolve(&ctx.paths.home_dir, string(action, "path")?);
                let name = action["appName"].as_str().unwrap_or("");
                let apps = self.inner.apps.read().await;
                let app = apps
                    .iter()
                    .find(|a| {
                        a["name"]
                            .as_str()
                            .is_some_and(|n| n.eq_ignore_ascii_case(name))
                    })
                    .cloned();
                drop(apps);
                let r = if name.is_empty()
                    || (cfg!(windows)
                        && app
                            .as_ref()
                            .and_then(|a| a["path"].as_str())
                            .map_or(true, |p| p.starts_with("shell:AppsFolder\\")))
                {
                    ctx.request("core:platform:open-file", vec![json!(path)])
                        .await?
                } else {
                    ctx.request("core:platform:open-with-app",vec![json!({"path":path,"appPath":app.as_ref().map(|a|a["path"].clone()).unwrap_or(json!(name)),"appName":name})]).await?
                };
                if r["ok"] != false && !name.is_empty() {
                    if let Err(error) = self.record_open_with(path, name.into()).await {
                        log::warn!("Open-with usage persistence: {error}");
                    }
                }
                Ok(r)
            }
            "copy-text" => {
                let text = string(action, "text")?.to_owned();
                self.inner
                    .clipboard
                    .handle("clipboard:write", &[json!(text)])
                    .await?;
                Ok(json!({"ok":true,"message":"Copied to clipboard"}))
            }
            "copy-and-paste-text" => {
                ctx.request(
                    "core:platform:paste-text",
                    vec![json!(string(action, "text")?)],
                )
                .await
            }
            "add-note" => {
                let v = self
                    .inner
                    .stores
                    .handle("notes:append", &[json!(string(action, "text")?)])
                    .await?;
                self.local_refresh().await?;
                Ok(
                    json!({"ok":!v.is_null(),"message":if v.is_null(){"Could not save quick note"}else{"Saved to Quick Notes"}}),
                )
            }
            "open-url" => {
                ctx.request(
                    "core:platform:open-url",
                    vec![json!(string(action, "url")?)],
                )
                .await
            }
            "install-extension" => {
                let id = string(action, "extensionId")?;
                let context = json!({"extensionId":id});
                let decision = ctx
                    .request(
                        "core:safety:authorize",
                        vec![json!("extension.install"), context.clone()],
                    )
                    .await?;
                let accepted = decision["accepted"]
                    .as_bool()
                    .ok_or("Invalid safety authorization response")?;
                if !accepted {
                    return Ok(json!({"ok":false,"message":"Cancelled"}));
                }
                if decision["dryRun"] == true {
                    return Ok(
                        json!({"ok":true,"message":"Dry run: would have install extension."}),
                    );
                }
                let result = match ctx.request("extension:install", vec![json!(id)]).await {
                    Ok(v) if v["ok"] == false => v,
                    Ok(_) => json!({"ok":true,"message":format!("Installing {id}")}),
                    Err(error) => json!({"ok":false,"message":error}),
                };
                ctx.request(
                    "core:safety:record",
                    vec![json!("extension.install"), context, result.clone()],
                )
                .await?;
                Ok(result)
            }
            "run-extension-command" => {
                let id = string(action, "extensionId")?;
                let command = string(action, "commandName")?;
                let mut values = action["argumentValues"]
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                if let (Some(name), Some(value)) = (
                    action["argumentName"].as_str(),
                    action["argumentValue"].as_str(),
                ) {
                    values.entry(name.to_owned()).or_insert(json!(value));
                }
                ctx.request(
                    "extension:run-command",
                    vec![json!({"extensionId":id,"commandName":command,"argumentValues":values})],
                )
                .await
            }
            "invoke-command" => {
                ctx.request(
                    "core:commands:execute",
                    vec![
                        json!(string(action, "commandId")?),
                        action["payload"].clone(),
                    ],
                )
                .await
            }
            "run-shell" => {
                let command = string(action, "command")?.trim();
                if command.is_empty() {
                    return Ok(json!({"ok":false,"message":"Empty shell command"}));
                }
                if command.chars().any(|c| ";|&`$(){}[]\n\r<>\\".contains(c)) {
                    return Ok(
                        json!({"ok":false,"message":"Shell metacharacters are not allowed in run-shell commands."}),
                    );
                }
                ctx.request("core:native:execute", vec![action.clone()])
                    .await
            }
            "run-native-command" => {
                string(action, "commandId")?;
                ctx.request("core:native:execute", vec![action.clone()])
                    .await
            }
            _ => Err(format!("Unknown search action type: {kind}")),
        }
    }
    async fn copy_snippet(&self, id: &str) -> CoreResult {
        let id = id.to_owned();
        let data = self
            .inner
            .stores
            .run(move |root| {
                let v = stores::load_snippets(root)?;
                Ok(v["snippets"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|s| s["id"] == id)
                    .cloned()
                    .unwrap_or(Value::Null))
            })
            .await?;
        if data.is_null() {
            return Ok(json!({"ok":false,"message":"Snippet not found"}));
        }
        let body = string(&data, "body")?;
        let clipboard = if body.contains("${clipboard}") {
            self.inner
                .clipboard
                .handle("clipboard:read", &[])
                .await?
                .as_str()
                .unwrap_or("")
                .to_owned()
        } else {
            String::new()
        };
        let text = stores::interpolate(body, &clipboard);
        self.inner
            .clipboard
            .handle("clipboard:write", &[json!(text)])
            .await?;
        self.inner
            .clipboard
            .handle("core:clipboard:capture-text", &[json!(text)])
            .await?;
        Ok(json!({"ok":true,"message":"Copied to clipboard"}))
    }
    async fn record_alias(&self, query: &str, name: &str) -> CoreResult {
        let mut term = None;
        for (i, c) in query.char_indices().rev() {
            if c == ' ' {
                let p = providers::resolve(&self.inner.ctx.paths.home_dir, &query[..i]);
                if p.exists() {
                    let t = providers::app_term(&query[i + 1..]);
                    if !t.is_empty() {
                        term = Some(t)
                    }
                    break;
                }
            }
        }
        let Some(term) = term else {
            return Ok(Value::Null);
        };
        let name = name.to_owned();
        self.inner
            .stores
            .run(move |root| {
                let file = root.join("open-with-usage.json");
                let mut v = stores::read_or(&file, json!({"version":1,"keys":{},"aliases":{}}))?;
                if !v.is_object() || !v["keys"].is_object() {
                    return Err("Invalid open-with usage store".into());
                }
                if !v["aliases"].is_object() {
                    v["aliases"] = json!({})
                }
                if !v["aliases"][&term].is_null() && !v["aliases"][&term].is_object() {
                    return Err("Invalid open-with alias bucket".into());
                }
                let n = v["aliases"][&term][&name]["count"].as_u64().unwrap_or(0) + 1;
                v["aliases"][term][name] = json!({"count":n,"lastUsedAt":now_ms()});
                crate::native_core::common::write_json_atomic(&file, &v)?;
                Ok(Value::Null)
            })
            .await
    }
    async fn record_open_with(&self, path: PathBuf, name: String) -> CoreResult {
        self.inner
            .stores
            .run(move |root| {
                let file = root.join("open-with-usage.json");
                let mut v = stores::read_or(&file, json!({"version":1,"keys":{},"aliases":{}}))?;
                if !v.is_object() || !v["keys"].is_object() {
                    return Err("Invalid open-with usage store".into());
                }
                for key in providers::usage_keys(&path) {
                    if !v["keys"][&key].is_null() && !v["keys"][&key].is_object() {
                        return Err("Invalid open-with usage bucket".into());
                    }
                    let n = v["keys"][&key][&name]["count"].as_u64().unwrap_or(0) + 1;
                    v["keys"][key][&name] = json!({"count":n,"lastUsedAt":now_ms()});
                }
                crate::native_core::common::write_json_atomic(&file, &v)?;
                Ok(Value::Null)
            })
            .await
    }
}
fn percent(s: &str) -> String {
    s.as_bytes()
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(b) {
                (*b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn good_match(q: &str, rows: &[Value]) -> bool {
    let variants = text::variants(q);
    rows.iter().take(10).any(|r| {
        let title = r["title"].as_str().unwrap_or("");
        let metadata = format!("{title} {}", r["subtitle"].as_str().unwrap_or("")).to_lowercase();
        variants.iter().any(|v| {
            if r["category"] == "applications" && text::app_match(title, v) > 0. {
                return true;
            }
            if metadata.contains(v) {
                return true;
            }
            let terms = text::tokens(v);
            !terms.is_empty()
                && terms.iter().all(|q| {
                    text::tokens(&metadata).iter().any(|m| {
                        m.starts_with(q)
                            || (q.starts_with(m)
                                && m.chars().count()
                                    >= 3.max((q.chars().count() as f64 * 0.7).ceil() as usize))
                    })
                })
                && (terms.len() > 1 || r["score"].as_f64().unwrap_or(0.) >= 650.)
        })
    })
}
fn action_id(action: &Value, context: &Value) -> Result<String, String> {
    if let Some(id) = context["resultId"].as_str() {
        return Ok(id.into());
    }
    let kind = string(action, "type")?;
    Ok(match kind {
        "open-app" => format!("open-app:{}", string(action, "appName")?),
        "open-file" => format!("open-file:{}", string(action, "path")?),
        "open-with-app" => format!(
            "open-with-app:{}:{}",
            action["appName"].as_str().unwrap_or("default"),
            string(action, "path")?
        ),
        "copy-text" | "copy-and-paste-text" | "add-note" => {
            format!("{kind}:{}", text::utf16_prefix(string(action, "text")?, 64))
        }
        "open-url" => format!("open-url:{}", string(action, "url")?),
        "install-extension" => format!("install-extension:{}", string(action, "extensionId")?),
        "run-extension-command" => format!(
            "extcmd:{}:{}",
            string(action, "extensionId")?,
            string(action, "commandName")?
        ),
        "run-shell" => format!("run-shell:{}", string(action, "command")?),
        "invoke-command" => format!("command:{}", string(action, "commandId")?),
        "run-native-command" => format!("native:{}", string(action, "commandId")?),
        _ => return Err(format!("Invalid search action type: {kind}")),
    })
}
fn benchmark_history(db: &mut rusqlite::Connection) -> CoreResult {
    let mut stmt=db.prepare("SELECT created_at,precision_at_5,precision_at_10,avg_click_rank,benchmark_size FROM benchmark_snapshots ORDER BY created_at DESC LIMIT 50").map_err(|e|e.to_string())?;
    let rows=stmt.query_map([],|r|Ok(json!({"generatedAt":r.get::<_,u64>(0)?,"precisionAt5":r.get::<_,f64>(1)?,"precisionAt10":r.get::<_,f64>(2)?,"clickThroughRank":r.get::<_,f64>(3)?,"benchmarkSize":r.get::<_,u64>(4)?}))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
    Ok(json!(rows))
}

#[cfg(test)]
mod tests;
