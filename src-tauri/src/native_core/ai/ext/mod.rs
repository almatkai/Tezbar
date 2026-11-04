//! User-added AI contributions. Imports are data only; only Test may start a
//! disabled connection. Agent runs consume an immutable, pinned tool table.
mod contributions;
mod manifest;
mod mcp;
mod project;
mod resources;
mod secrets;
use super::*;
use mcp::client::Client;
use std::collections::{BTreeMap, HashSet};
pub const CHANNELS: &[&str] = &[
    "ai-extensions:oauth-start",
    "ai-extensions:oauth-cancel",
    "ai-extensions:oauth-status",
    "ai-extensions:trust-project",
    "ai-extensions:logs",
    "ai-extensions:reset-secrets",
    "ai-extensions:clear-allowance",
    "ai-extensions:list",
    "ai-extensions:review-project",
    "ai-extensions:parse-import",
    "ai-extensions:add",
    "ai-extensions:update",
    "ai-extensions:remove",
    "ai-extensions:test",
    "ai-extensions:set-enabled",
    "ai-extensions:set-tool",
    "ai-extensions:accept-review",
    "ai-extensions:set-secret",
    "ai-extensions:composer-items",
    "ai-extensions:prompt-resolve",
    "ai-extensions:read-resource",
    "ai-extensions:set-instructions",
];
struct NewSecretGuard {
    core: AiCore,
    namespaces: Vec<String>,
    armed: Arc<std::sync::atomic::AtomicBool>,
}
impl Drop for NewSecretGuard {
    fn drop(&mut self) {
        if self.armed.load(std::sync::atomic::Ordering::SeqCst) {
            for namespace in &self.namespaces {
                let _ = self.core.inner.extensions.secrets.put(namespace, "", None);
            }
        }
    }
}
struct Starting {
    result: tokio::sync::watch::Receiver<Option<Result<Arc<Client>, String>>>,
    cancel: Cancel,
}
pub struct Extensions {
    starting: Mutex<HashMap<String, Starting>>,
    failures: Mutex<HashMap<String, (u8, u64, String)>>,
    lock: Mutex<()>,
    clients: Mutex<HashMap<String, Arc<Client>>>,
    imports: Mutex<HashMap<String, Value>>,
    review_cwd: Mutex<Option<String>>,
    secrets: secrets::Secrets,
    path: std::path::PathBuf,
}
#[derive(Clone, Copy)]
pub enum ResourceOp {
    List,
    Read,
}
pub struct Entry {
    pub label: String,
    pub resource: Option<ResourceOp>,
    pub extension: String,
    pub connection: String,
    pub descriptor: Value,
    pub hash: String,
    pub client: Arc<Client>,
}
#[derive(Default)]
pub struct Snapshot {
    entries: BTreeMap<String, Entry>,
    pub warnings: Vec<String>,
}
impl Snapshot {
    fn new() -> Self {
        Self::default()
    }
}
impl std::ops::Deref for Snapshot {
    type Target = BTreeMap<String, Entry>;
    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}
impl std::ops::DerefMut for Snapshot {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entries
    }
}
impl Extensions {
    pub fn new(config: &std::path::Path) -> Self {
        Self {
            lock: Mutex::new(()),
            starting: Mutex::new(HashMap::new()),
            failures: Mutex::new(HashMap::new()),
            clients: Mutex::new(HashMap::new()),
            imports: Mutex::new(HashMap::new()),
            review_cwd: Mutex::new(None),
            secrets: secrets::Secrets::new(config),
            path: config.join("ai-extensions.json"),
        }
    }
    fn load(&self) -> CoreResult {
        if self.path.exists() {
            let v = common::read_json(&self.path)?;
            if !v["extensions"].is_array() {
                return Err("AI extensions settings are malformed".into());
            }
            Ok(v)
        } else {
            Ok(json!({"version":1,"extensions":[],"globalInstructions":"","useAgentsMd":true}))
        }
    }
    fn edit(
        &self,
        f: impl FnOnce(&mut Value) -> CoreResult,
    ) -> Result<(Value, Value, Value), String> {
        let _guard = self.lock.lock().unwrap();
        let mut data = self.load()?;
        let before = data.clone();
        let result = f(&mut data)?;
        if data != before {
            common::write_json_atomic(&self.path, &data)?;
        }
        Ok((result, before, data))
    }
    pub fn shutdown(&self) {
        for s in self.starting.lock().unwrap().values() {
            s.cancel.cancel();
        }
        for c in self.clients.lock().unwrap().drain().map(|(_, c)| c) {
            c.shutdown();
        }
    }
}
fn record_mut<'a>(data: &'a mut Value, id: &str) -> Result<&'a mut Value, String> {
    data["extensions"]
        .as_array_mut()
        .ok_or("Invalid AI extension settings")?
        .iter_mut()
        .find(|v| v["id"] == id)
        .ok_or_else(|| "AI extension was removed".into())
}
fn connection_mut<'a>(record: &'a mut Value, id: &str) -> Result<&'a mut Value, String> {
    record["connections"]
        .as_array_mut()
        .ok_or("No connections")?
        .iter_mut()
        .find(|v| v["id"] == id)
        .ok_or_else(|| "Connection was removed".into())
}
fn key(ext: &str, conn: &str) -> String {
    format!("{}:{ext}:{conn}", ext.len())
}
fn provider_key(ext: &str, provider: &str) -> String {
    format!("provider{}:{ext}:{provider}", ext.len())
}
fn prefix(ext: &str) -> String {
    format!("{}:{ext}:", ext.len())
}
fn current_tool_hash(c: &Value, name: &str) -> Option<String> {
    c["catalog"]["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| t["name"] == name)
        .map(manifest::hash)
}
fn command_pin(c: &Value) -> String {
    let mut v = c.clone();
    for k in [
        "catalog",
        "tested",
        "enabled",
        "needsReview",
        "pins",
        "allowances",
        "error",
        "slug",
        "trustPin",
        "pendingPins",
        "version",
        "disabledTools",
        "allowAllConfirmation",
        "previousCatalog",
        "catalogAt",
        "needsAuth",
        "modelNames",
        "hasResources",
        "pinnedCatalog",
        "cachedEra",
        "requestedScopes",
        "ttlMs",
    ] {
        v.as_object_mut().unwrap().remove(k);
    }
    manifest::hash(&v)
}
fn profile_tool_names(c: &Value, resources: bool) -> Value {
    let mut counts = HashMap::<String, usize>::new();
    let tools = c["catalog"]["tools"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let sanitize = |name: &str| {
        name.chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                    ch
                } else {
                    '_'
                }
            })
            .collect::<String>()
    };
    for tool in &tools {
        *counts
            .entry(sanitize(s(&tool["name"])).to_lowercase())
            .or_default() += 1;
    }
    let mut names = json!({});
    let mut used = HashSet::new();
    let slug = s(&c["slug"]);
    for tool in &tools {
        let original = s(&tool["name"]);
        let sanitized = sanitize(original);
        let mut name = format!("{slug}__{sanitized}");
        if name.len() > 64
            || counts[&sanitized.to_lowercase()] > 1
            || resources && matches!(original, "list_resources" | "read_resource")
        {
            name = format!(
                "{slug}__{}_{}",
                sanitized.chars().take(36).collect::<String>(),
                &manifest::hash(&json!(original))[..6]
            );
        }
        let mut collision = 0;
        while used.contains(&name.to_lowercase()) {
            collision += 1;
            name = format!(
                "{slug}__{}_{}",
                sanitized.chars().take(36).collect::<String>(),
                &manifest::hash(&json!([original, collision]))[..6]
            );
        }
        used.insert(name.to_lowercase());
        names[original] = json!(name);
    }
    names
}
impl AiCore {
    pub(in crate::native_core::ai) async fn ext_read(&self) -> CoreResult {
        let core = self.clone();
        tokio::task::spawn_blocking(move || core.inner.extensions.load())
            .await
            .map_err(err)?
    }
    pub(in crate::native_core::ai) async fn ext_edit(
        &self,
        f: impl FnOnce(&mut Value) -> CoreResult + Send + 'static,
    ) -> CoreResult {
        let core = self.clone();
        let (result, before, data) =
            tokio::task::spawn_blocking(move || core.inner.extensions.edit(f))
                .await
                .map_err(err)??;
        // Persist disablement before cancelling starts, so a concurrent cold
        // start cannot observe an enabled record after the cancellation sweep.
        for record in data["extensions"].as_array().unwrap().iter().filter(|e| {
            e["enabled"] != true
                && before["extensions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|old| old["id"] == e["id"])
                    .is_some_and(|old| old["enabled"] == true)
        }) {
            let p = prefix(s(&record["id"]));
            for (k, start) in self.inner.extensions.starting.lock().unwrap().iter() {
                if k.starts_with(&p) {
                    start.cancel.cancel();
                }
            }
            for (k, client) in self.inner.extensions.clients.lock().unwrap().iter() {
                if k.starts_with(&p) {
                    client.shutdown();
                }
            }
        }
        if data != before {
            self.inner.ctx.emit("ai-extensions-changed", Value::Null);
        }
        Ok(result)
    }
    async fn ext_client(&self, extension: &str, connection: &Value) -> Result<Arc<Client>, String> {
        self.ext_client_mode(extension, connection, false).await
    }
    async fn ext_client_mode(
        &self,
        extension: &str,
        connection: &Value,
        explicit_test: bool,
    ) -> Result<Arc<Client>, String> {
        let id = key(extension, s(&connection["id"]));
        if let Some((count, at, error)) = self.inner.extensions.failures.lock().unwrap().get(&id) {
            if *count >= 4 || now_ms() < *at {
                return Err(format!(
                    "Connection unavailable: {error}. Test it again in Settings."
                ));
            }
        }
        let mut receiver = {
            let mut starts = self.inner.extensions.starting.lock().unwrap();
            if let Some(start) = starts.get(&id) {
                start.result.clone()
            } else {
                let (tx, rx) = tokio::sync::watch::channel(None);
                let cancel = Cancel::new();
                starts.insert(
                    id.clone(),
                    Starting {
                        result: rx.clone(),
                        cancel: cancel.clone(),
                    },
                );
                let core = self.clone();
                let eid = extension.to_string();
                let c = connection.clone();
                self.spawn(async move{let result=tokio::select!{biased;_=cancel.cancelled()=>Err("Connection was disabled".into()),r=core.ext_connect(&eid,&c,explicit_test)=>r};
                    if let Err(e)=&result {if !cancel.is_cancelled(){let mut failures=core.inner.extensions.failures.lock().unwrap();let count=failures.get(&id).map(|v|v.0+1).unwrap_or(1);let wait=[1,5,30][usize::from(count.saturating_sub(1)).min(2)];failures.insert(id.clone(),(count,now_ms()+wait*1000,cut(e,500)));}}
                    if result.is_ok() { core.inner.extensions.failures.lock().unwrap().remove(&id); }
                    let _=tx.send(Some(result));core.inner.extensions.starting.lock().unwrap().remove(&id);
                });
                rx
            }
        };
        loop {
            if let Some(result) = receiver.borrow().clone() {
                return result;
            }
            receiver
                .changed()
                .await
                .map_err(|_| "Connection startup was cancelled")?;
        }
    }
    async fn ext_connect(
        &self,
        extension: &str,
        connection: &Value,
        explicit_test: bool,
    ) -> Result<Arc<Client>, String> {
        let current = self.ext_read().await?;
        let record = current["extensions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| e["id"] == extension)
            .ok_or("Extension was removed")?;
        if !explicit_test && record["enabled"] != true {
            return Err("Extension was disabled".into());
        }
        let actual = record["connections"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|c| c["id"] == connection["id"])
            .ok_or("Connection was removed")?;
        if command_pin(actual) != command_pin(connection) {
            return Err("Connection settings changed. Test it again.".into());
        }
        let id = key(extension, s(&connection["id"]));
        let cached = {
            let mut clients = self.inner.extensions.clients.lock().unwrap();
            if clients.get(&id).is_some_and(|c| c.is_closed()) {
                if let Some(client) = clients.remove(&id) {
                    if client.crashed() {
                        return Err("The connection process exited. It will restart with backoff; test it again to reset failures.".into());
                    }
                }
            }
            clients.get(&id).cloned()
        };
        let values = self.mcp_oauth_values(extension, connection, false).await?;
        let mut env = HashMap::new();
        let mut c = connection.clone();
        if explicit_test {
            c.as_object_mut().unwrap().remove("cachedEra");
            if let Some(client) = cached.as_ref() {
                client.shutdown();
            }
        }
        let cached = if explicit_test { None } else { cached };
        for (k, v) in connection["env"].as_object().into_iter().flatten() {
            let value = if let Some(name) = v["secret"].as_str() {
                values
                    .get(name)
                    .cloned()
                    .ok_or_else(|| format!("Missing {}", v["label"].as_str().unwrap_or(name)))?
            } else {
                v.as_str()
                    .ok_or("Variables must be text or secret references")?
                    .into()
            };
            env.insert(k.clone(), value);
        }
        for (k, v) in connection["headers"].as_object().into_iter().flatten() {
            if let Some(name) = v["secret"].as_str() {
                c["headers"][k] = json!(values.get(name).ok_or("Missing access token")?);
            }
        }
        if c["auth"]["type"] == "token" {
            let name = c["auth"]["secret"].as_str().unwrap_or("access-token");
            let token = values.get(name).ok_or("Missing access token")?;
            let header = c["auth"]["header"]
                .as_str()
                .unwrap_or("Authorization")
                .to_string();
            if !c["headers"].is_object() {
                c["headers"] = json!({})
            }
            c["headers"][&header] = json!(if header.eq_ignore_ascii_case("authorization") {
                format!("Bearer {token}")
            } else {
                token.clone()
            });
        }
        let mut scrub_values = values.values().cloned().collect::<Vec<_>>();
        if c["auth"]["type"] == "oauth" {
            let grant = values.get("oauth-grant").ok_or("Sign in required")?;
            let grant: Value = serde_json::from_str(grant)
                .map_err(|_| "Saved sign-in is invalid. Sign in again.")?;
            if oauth::secure_url(s(&grant["resource"]))? != oauth::secure_url(s(&c["url"]).trim())?
            {
                return Err("Saved sign-in belongs to another resource. Sign in again.".into());
            }
            if grant["expiresAt"]
                .as_u64()
                .is_some_and(|t| now_ms() >= t.saturating_sub(60000))
            {
                return Err("Sign-in expired. Sign in again.".into());
            }
            let token = grant["access_token"].as_str().ok_or("Sign in required")?;
            scrub_values.push(token.into());
            for field in ["refresh_token", "clientSecret"] {
                if let Some(value) = grant[field].as_str() {
                    scrub_values.push(value.into());
                }
            }
            if !c["headers"].is_object() {
                c["headers"] = json!({})
            }
            c["headers"]["Authorization"] = json!(format!("Bearer {token}"));
        }
        if let Some(client) = cached {
            client.remember_secrets(&scrub_values);
            if c["auth"]["type"] == "oauth" {
                let grant: Value =
                    serde_json::from_str(values.get("oauth-grant").ok_or("Sign in required")?)
                        .map_err(|_| "Saved sign-in is invalid")?;
                client.set_http_token(s(&grant["access_token"]));
            }
            return Ok(client);
        }
        c["_extensionId"] = json!(extension);
        let client = Client::connect(self, c, env, scrub_values).await?;
        let current = self.ext_read().await?;
        if !current["extensions"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|e| {
                e["id"] == extension
                    && (explicit_test || e["enabled"] == true)
                    && e["connections"].as_array().into_iter().flatten().any(|c| {
                        c["id"] == connection["id"] && command_pin(c) == command_pin(connection)
                    })
            })
        {
            client.shutdown();
            return Err("Connection was disabled or changed".into());
        }
        self.inner
            .extensions
            .clients
            .lock()
            .unwrap()
            .insert(id, client.clone());
        Ok(client)
    }
    async fn ext_discover(&self, id: &str) -> CoreResult {
        self.ext_discover_mode(id, false).await
    }
    async fn ext_discover_mode(&self, id: &str, explicit_test: bool) -> CoreResult {
        let data = self.ext_read().await?;
        let ext = data["extensions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == id)
            .ok_or("Extension was removed")?;
        if !explicit_test && ext["enabled"] != true {
            return Err("Extension was disabled".into());
        }
        let mut updated = vec![];
        for c in ext["connections"].as_array().into_iter().flatten() {
            let client = self.ext_client_mode(id, c, explicit_test).await?;
            let mut catalog =
                json!({"tools":[],"prompts":[],"resources":[],"templates":[],"warnings":[]});
            let caps = client.capabilities.lock().unwrap().clone();
            let mut ttl = 60_000u64;
            for (method, kind, cap) in [
                ("tools/list", "tools", "tools"),
                ("prompts/list", "prompts", "prompts"),
                ("resources/list", "resources", "resources"),
                ("resources/templates/list", "templates", "resources"),
            ] {
                if caps.get(cap).is_none() {
                    continue;
                }
                let mut cursor = Value::Null;
                let mut items = vec![];
                let mut total = 0;
                for page in 0..20 {
                    let mut params = json!({});
                    if !cursor.is_null() {
                        params["cursor"] = cursor;
                    }
                    let mut result = client
                        .call(method, params, &Cancel::new(), Duration::from_secs(10))
                        .await?;
                    client.scrub_value(&mut result);
                    if let Some(value) = result["ttlMs"].as_u64() {
                        ttl = ttl.min(value.clamp(1000, 600_000));
                    }
                    let field = if kind == "templates" {
                        "resourceTemplates"
                    } else {
                        kind
                    };
                    let list = result[field]
                        .as_array()
                        .ok_or("Invalid connection catalog")?;
                    for item in list {
                        let len = item.to_string().len();
                        total += len;
                        if total > 4 * 1024 * 1024 || items.len() >= 512 {
                            return Err("Connection catalog exceeds its limit".into());
                        }
                        if len > 65536 {
                            catalog["warnings"]
                                .as_array_mut()
                                .unwrap()
                                .push(json!("A definition exceeded 64 KiB and was omitted"));
                            continue;
                        }
                        if s(&item["description"]).len() > 2048
                            || s(&item["title"]).len() > 2048
                            || (kind == "prompts"
                                && (!item["name"].is_string() || s(&item["name"]).len() > 128))
                            || (kind == "resources"
                                && (!item["uri"].is_string() || s(&item["uri"]).len() > 8192))
                            || (kind == "templates"
                                && (!item["uriTemplate"].is_string()
                                    || s(&item["uriTemplate"]).len() > 8192))
                        {
                            catalog["warnings"]
                                .as_array_mut()
                                .unwrap()
                                .push(json!("An invalid definition was omitted"));
                            continue;
                        }
                        if kind == "tools" {
                            let item = item.clone();
                            let schema = item["inputSchema"].clone();
                            let valid = if mcp::schema::for_wire(&schema).is_ok() {
                                mcp::schema::definition(schema).await
                            } else {
                                Err("Recursive or unresolved schema".into())
                            };
                            let output_valid = if let Some(output) = item.get("outputSchema") {
                                mcp::schema::definition(output.clone()).await.is_ok()
                            } else {
                                true
                            };
                            if !output_valid
                                || (c["transport"] == "http"
                                    && mcp::schema::mirrored_headers(&item["inputSchema"], None)
                                        .is_err())
                                || valid.is_err()
                                || !regex::Regex::new("^[A-Za-z0-9_.-]{1,128}$")
                                    .unwrap()
                                    .is_match(s(&item["name"]))
                                || s(&item["description"]).len() > 2048
                                || s(&item["title"]).len() > 2048
                            {
                                catalog["warnings"]
                                    .as_array_mut()
                                    .unwrap()
                                    .push(json!("An invalid tool definition was omitted"));
                                continue;
                            }
                        }
                        items.push(item.clone());
                    }
                    cursor = result["nextCursor"].clone();
                    if cursor.is_null() {
                        break;
                    }
                    if page == 19 {
                        return Err("Connection catalog exceeds 20 pages".into());
                    }
                }
                if kind == "tools" {
                    let mut counts = HashMap::new();
                    for item in &items {
                        *counts.entry(s(&item["name"]).to_string()).or_insert(0) += 1
                    }
                    if counts.values().any(|count| *count > 1) {
                        catalog["warnings"]
                            .as_array_mut()
                            .unwrap()
                            .push(json!("Duplicate tool names were omitted"));
                    }
                    items.retain(|item| counts[s(&item["name"])] == 1);
                }
                catalog[kind] = json!(items);
            }
            updated.push((
                s(&c["id"]).to_string(),
                catalog,
                client.version.lock().unwrap().clone(),
                command_pin(c),
                caps.get("resources").is_some(),
                ttl,
            ));
        }
        let id = id.to_string();
        self.ext_edit(move |data| {
            let record = record_mut(data, &id)?;
            record.as_object_mut().unwrap().remove("error");
            for (cid, catalog, version, pin, resources, ttl) in updated {
                let c = connection_mut(record, &cid)?;
                let old = c["pins"].clone();
                let mut pins = json!({});
                let mut changed = false;
                for tool in catalog["tools"].as_array().unwrap() {
                    let name = s(&tool["name"]);
                    let hash = manifest::hash(tool);
                    if old.is_object() && old[name] != hash {
                        changed = true;
                    }
                    pins[name] = json!(hash);
                }
                changed |= old.is_object() && old != pins;
                c["pendingPins"] = pins.clone();
                if !old.is_object() {
                    c["pins"] = pins;
                    c["pinnedCatalog"] = catalog.clone();
                }
                if changed {
                    c["previousCatalog"] = c
                        .get("pinnedCatalog")
                        .cloned()
                        .filter(Value::is_object)
                        .unwrap_or_else(|| c["catalog"].clone());
                }
                if c["pinnedCatalog"].is_null() {
                    c["pinnedCatalog"] = c["previousCatalog"].clone();
                }
                c["catalog"] = catalog;
                c.as_object_mut().unwrap().remove("error");
                c.as_object_mut().unwrap().remove("needsAuth");
                c["hasResources"] = json!(resources);
                c["modelNames"] = profile_tool_names(c, resources);
                c["catalogAt"] = json!(now_ms());
                c["ttlMs"] = json!(ttl);
                c["cachedEra"] = json!(version);
                c["version"] = json!(version);
                c["tested"] = json!(true);
                c["trustPin"] = json!(pin);
                c["needsReview"] = json!(changed);
                if changed {
                    c["allowances"] = json!({})
                }
            }
            Ok(json!({"ok":true,"extension":record}))
        })
        .await
    }
    pub(super) async fn extension_snapshot(
        &self,
        cwd: &std::path::Path,
    ) -> Result<Snapshot, String> {
        let cwd = tokio::fs::canonicalize(cwd).await.map_err(err)?;
        self.revoke_changed_projects(&cwd).await?;
        let initial = self.ext_read().await?;
        for ext in initial["extensions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["enabled"] == true)
        {
            if ext["projectPath"]
                .as_str()
                .is_some_and(|folder| !runtime::policy::within(&cwd, std::path::Path::new(folder)))
            {
                continue;
            }
            let notified = ext["connections"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|c| {
                    self.inner
                        .extensions
                        .clients
                        .lock()
                        .unwrap()
                        .get(&key(s(&ext["id"]), s(&c["id"])))
                        .is_some_and(|client| {
                            client.changed.load(std::sync::atomic::Ordering::SeqCst)
                        })
                });
            if notified {
                if matches!(
                    tokio::time::timeout(Duration::from_secs(5), self.ext_discover(s(&ext["id"])))
                        .await,
                    Ok(Ok(_))
                ) {
                    for c in ext["connections"].as_array().into_iter().flatten() {
                        if let Some(client) = self
                            .inner
                            .extensions
                            .clients
                            .lock()
                            .unwrap()
                            .get(&key(s(&ext["id"]), s(&c["id"])))
                        {
                            client
                                .changed
                                .store(false, std::sync::atomic::Ordering::SeqCst);
                        }
                    }
                }
                continue;
            }
            if ext["connections"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|c| {
                    c["tested"] == true
                        && (c["catalogAt"].as_u64().is_none_or(|at| {
                            now_ms().saturating_sub(at) > c["ttlMs"].as_u64().unwrap_or(60_000)
                        }) || self
                            .inner
                            .extensions
                            .clients
                            .lock()
                            .unwrap()
                            .get(&key(s(&ext["id"]), s(&c["id"])))
                            .is_some_and(|client| {
                                client.changed.load(std::sync::atomic::Ordering::SeqCst)
                            }))
                })
            {
                let core = self.clone();
                let id = s(&ext["id"]).to_string();
                self.spawn(async move {
                    let _ = core.ext_discover(&id).await;
                });
            }
        }
        let data = self.ext_read().await?;
        let mut snapshot = Snapshot::new();
        let mut used = HashSet::new();
        for ext in data["extensions"].as_array().unwrap() {
            if ext["enabled"] != true {
                continue;
            }
            if let Some(project) = ext["projectPath"].as_str() {
                if !runtime::policy::within(&cwd, std::path::Path::new(project)) {
                    continue;
                }
            }
            for c in ext["connections"].as_array().into_iter().flatten() {
                if c["tested"] != true
                    || c["needsReview"] == true
                    || c["trustPin"] != command_pin(c)
                {
                    snapshot.warnings.push(if c["needsReview"] == true {
                        format!(
                            "Tools changed in {}. Review them in AI settings.",
                            s(&c["name"])
                        )
                    } else {
                        format!("{} is unavailable; test it in AI settings.", s(&c["name"]))
                    });
                    continue;
                }
                let client = match tokio::time::timeout(
                    Duration::from_secs(5),
                    self.ext_client(s(&ext["id"]), c),
                )
                .await
                {
                    Ok(Ok(c)) => c,
                    _ => {
                        snapshot.warnings.push(format!(
                            "{} is unavailable for this run. Check AI settings.",
                            s(&c["name"])
                        ));
                        continue;
                    }
                };
                if c["needsReview"] == true {
                    snapshot.warnings.push(format!(
                        "{} has changed tools awaiting review in AI settings.",
                        s(&c["name"])
                    ));
                }
                let mut name_counts = HashMap::new();
                for tool in c["catalog"]["tools"].as_array().into_iter().flatten() {
                    let normalized = s(&tool["name"])
                        .chars()
                        .map(|ch| {
                            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                                ch.to_ascii_lowercase()
                            } else {
                                '_'
                            }
                        })
                        .collect::<String>();
                    *name_counts.entry(normalized).or_insert(0) += 1;
                }
                for tool in c["catalog"]["tools"].as_array().into_iter().flatten() {
                    let original = s(&tool["name"]);
                    let hash = manifest::hash(tool);
                    if c["pins"][original] != hash || c["disabledTools"][original] == true {
                        continue;
                    }
                    if snapshot.len() >= 128 {
                        break;
                    }
                    let slug = s(&c["slug"]);
                    let sanitized = original
                        .chars()
                        .map(|c| {
                            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                                c
                            } else {
                                '_'
                            }
                        })
                        .collect::<String>();
                    let mut name = format!("{slug}__{sanitized}");
                    if name.len() > 64
                        || name_counts
                            .get(&sanitized.to_ascii_lowercase())
                            .is_some_and(|count| *count > 1)
                        || matches!(original, "list_resources" | "read_resource")
                            && client
                                .capabilities
                                .lock()
                                .unwrap()
                                .get("resources")
                                .is_some()
                        || used.contains(&name.to_lowercase())
                    {
                        name = format!(
                            "{slug}__{}_{}",
                            sanitized.chars().take(36).collect::<String>(),
                            &manifest::hash(&json!(original))[..6]
                        );
                    }
                    let mut collision = 0;
                    while used.contains(&name.to_lowercase()) {
                        collision += 1;
                        name = format!(
                            "{slug}__{}_{}",
                            sanitized.chars().take(36).collect::<String>(),
                            &manifest::hash(&json!([original, collision]))[..6]
                        );
                    }
                    used.insert(name.to_lowercase());
                    snapshot.insert(
                        name,
                        Entry {
                            label: format!(
                                "{} · {}",
                                s(&c["name"]),
                                tool["title"].as_str().unwrap_or(original)
                            ),
                            resource: None,
                            extension: s(&ext["id"]).into(),
                            connection: s(&c["id"]).into(),
                            descriptor: tool.clone(),
                            hash,
                            client: client.clone(),
                        },
                    );
                }
                if client
                    .capabilities
                    .lock()
                    .unwrap()
                    .get("resources")
                    .is_some()
                {
                    for (operation,name,description,schema) in [
                        (ResourceOp::List,"list_resources","List resources and resource templates from this connection",json!({"type":"object","properties":{},"additionalProperties":false})),
                        (ResourceOp::Read,"read_resource","Read a listed resource or a URI matching a listed template through this connection",json!({"type":"object","properties":{"uri":{"type":"string","maxLength":8192}},"required":["uri"],"additionalProperties":false}))
                    ] {
                        if snapshot.len()>=128{break}
                        let mut model_name=format!("{}__{name}",s(&c["slug"]));
                        if used.contains(&model_name.to_lowercase()){model_name=format!("{}__{name}_{}",s(&c["slug"]),&manifest::hash(&json!(name))[..6]);}
                        used.insert(model_name.to_lowercase());
                        snapshot.insert(model_name,Entry{label:format!("{} · {name}",s(&c["name"])),resource:Some(operation),extension:s(&ext["id"]).into(),connection:s(&c["id"]).into(),descriptor:json!({"name":name,"description":description,"inputSchema":schema,"annotations":{"readOnlyHint":true}}),hash:resources::catalog_hash(c),client:client.clone()});
                    }
                }
            }
        }
        if snapshot.len() >= 128 {
            snapshot.warnings.push(
                "At most 128 extension tools are included in a run; later tools may be omitted."
                    .into(),
            );
        }
        Ok(snapshot)
    }
    pub(super) async fn extension_call(
        &self,
        entry: &Entry,
        args: Value,
        run: &str,
        cancel: &Cancel,
        mode: Option<&str>,
    ) -> Result<(Vec<runtime::transcript::Block>, bool), String> {
        mcp::schema::validate(entry.descriptor["inputSchema"].clone(), args.clone()).await?;
        let check = |data: &Value| -> Result<Value, String> {
            let ext = data["extensions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["id"] == entry.extension && v["enabled"] == true)
                .ok_or("Extension was disabled")?;
            let c = ext["connections"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|c| c["id"] == entry.connection)
                .ok_or("Connection was removed")?;
            if c["disabledTools"][s(&entry.descriptor["name"])] == true
                || if entry.resource.is_some() {
                    resources::catalog_hash(c) != entry.hash
                } else {
                    c["pins"][s(&entry.descriptor["name"])] != entry.hash
                        || current_tool_hash(c, s(&entry.descriptor["name"])).as_deref()
                            != Some(entry.hash.as_str())
                }
                || c["trustPin"] != command_pin(c)
            {
                return Err("Tool changed. Review it in AI settings.".into());
            }
            Ok(c.clone())
        };
        let data = self.ext_read().await?;
        if let Some(folder) = data["extensions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| e["id"] == entry.extension)
            .and_then(|e| e["projectPath"].as_str())
        {
            self.revoke_changed_projects(std::path::Path::new(folder))
                .await?;
        }
        let c = check(&self.ext_read().await?)?;
        let cfg = self.read_config().await?;
        let mode = mode
            .or_else(|| cfg["agentApprovalMode"].as_str())
            .unwrap_or("default");
        let name = s(&entry.descriptor["name"]);
        let remembered = c["allowances"][name] == entry.hash;
        let ask = mode == "review"
            || !(remembered
                || c["toolPolicy"] == "allow"
                || c["toolPolicy"] == "allowReadOnly"
                    && entry.descriptor["annotations"]["readOnlyHint"] == true);
        let display_args = entry
            .client
            .scrub(&serde_json::to_string_pretty(&args).map_err(err)?);
        let display_args = if display_args.chars().count() > 4000 {
            format!(
                "{}\n[Arguments truncated; more fields exist]",
                cut(&display_args, 4000)
            )
        } else {
            display_args
        };
        if ask
            && !self
                .approval_prompt_identity(
                    run,
                    &format!("Allow {} to use {name}?", s(&c["name"])),
                    &display_args,
                    cancel,
                    false,
                    Some((
                        entry.extension.clone(),
                        entry.connection.clone(),
                        name.into(),
                        entry.hash.clone(),
                    )),
                )
                .await
        {
            return Err("Tool was denied or approval expired".into());
        }
        if let Some(folder) = data["extensions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| e["id"] == entry.extension)
            .and_then(|e| e["projectPath"].as_str())
        {
            self.revoke_changed_projects(std::path::Path::new(folder))
                .await?;
        }
        check(&self.ext_read().await?)?;
        if cancel.is_cancelled() {
            return Err("Operation cancelled".into());
        }
        let timeout =
            Duration::from_secs(c["timeoutSeconds"].as_u64().unwrap_or(120).clamp(1, 1800));
        let first = if let Some(operation) = entry.resource {
            self.extension_resource(&entry.client, &c, operation, &args, cancel)
                .await
        } else {
            entry
                .client
                .call_tool(name, args.clone(), &entry.descriptor, cancel, timeout)
                .await
        };
        let mut result = match first {
            Err(error) if error.starts_with("Access denied.") || error == "Sign in required" => {
                let extension = entry.extension.clone();
                let connection = entry.connection.clone();
                let saved = error.clone();
                let requested = entry.client.requested_scopes.lock().unwrap().clone();
                self.ext_edit(move |data| {
                    let c = connection_mut(record_mut(data, &extension)?, &connection)?;
                    c["needsAuth"] = json!(true);
                    if let Some(scope) = requested {
                        c["requestedScopes"] = json!(scope);
                    }
                    c["error"] = json!(saved);
                    Ok(Value::Null)
                })
                .await?;
                return Err(error);
            }
            other => other?,
        };
        for _ in 0..3 {
            if result["resultType"] != "input_required" {
                break;
            }
            if result.get("inputRequests").is_some() {
                return Err(
                    "This tool needs interactive input, which Tezbar does not support yet.".into(),
                );
            }
            let state = result
                .get("requestState")
                .ok_or("Invalid interactive tool result")?
                .clone();
            result = entry
                .client
                .call_tool_state(
                    name,
                    args.clone(),
                    &entry.descriptor,
                    cancel,
                    timeout,
                    Some(state),
                )
                .await?;
        }
        if result["resultType"] == "input_required" {
            return Err("Tool exceeded the continuation limit".into());
        }
        let mut blocks = vec![];
        let mut text = String::new();
        for item in result["content"].as_array().into_iter().flatten() {
            if item["annotations"]["audience"].as_array().is_some_and(|a| {
                a.iter().any(|v| v == "user") && !a.iter().any(|v| v == "assistant")
            }) {
                let output = entry
                    .client
                    .scrub(item["text"].as_str().unwrap_or("[Content for you]"));
                self.agent_event(run,json!({"type":"stage","stage":{"index":self.stage_index(run),"label":format!("{}: {}",s(&c["name"]),name),"toolName":name,"status":"done","output":cut(&output,8000),"outputTruncated":output.chars().count()>8000}}));
                continue;
            }
            match s(&item["type"]) {
                "text" => text.push_str(s(&item["text"])),
                "audio" => text.push_str("[Audio omitted]"),
                "resource_link" => {
                    text.push_str(&format!("{}: {}", s(&item["name"]), s(&item["uri"])))
                }
                "resource" => text.push_str(
                    item["resource"]["text"]
                        .as_str()
                        .unwrap_or("[Binary resource omitted]"),
                ),
                "image" => {
                    if blocks.len() < 4 {
                        let image = json!({"mimeType":item["mimeType"],"data":item["data"]});
                        match legacy::normalize_images(&[image]) {
                            Ok(images) => blocks.push(runtime::transcript::Block::Image {
                                mime: s(&images[0]["mimeType"]).into(),
                                data: s(&images[0]["data"]).into(),
                            }),
                            Err(e) => text.push_str(&format!("[Image omitted: {e}]")),
                        }
                    } else {
                        text.push_str("[Image omitted: at most four images per result]");
                    }
                }
                _ => {}
            }
        }
        if result["content"].as_array().is_none_or(|a| a.is_empty()) {
            if let Some(structured) = result.get("structuredContent") {
                text = structured.to_string();
            }
        }
        if entry.descriptor.get("outputSchema").is_some()
            && result.get("structuredContent").is_some()
        {
            if mcp::schema::validate(
                entry.descriptor["outputSchema"].clone(),
                result["structuredContent"].clone(),
            )
            .await
            .is_err()
            {
                text.push_str("\n[Structured output doesn't match the tool definition]");
            }
        }
        if text.is_empty() && blocks.is_empty() {
            text = "[No model-facing content]".into();
        }
        let mut text = entry.client.scrub(&text);
        let before = text.len();
        while text.len() > runtime::limits::RESULT_BYTES - 100 {
            text.pop();
        }
        if text.len() < before {
            text.push_str("\n[Output truncated]")
        }
        blocks.insert(
            0,
            runtime::transcript::Block::Text {
                text: format!(
                    "[Untrusted tool result{}]\n{text}",
                    if result["isError"] == true {
                        "; error"
                    } else {
                        ""
                    }
                ),
            },
        );
        Ok((blocks, result["isError"] == true))
    }
}
impl AiCore {
    pub(super) async fn ext_handle(&self, channel: &str, args: &[Value]) -> CoreResult {
        let a = arg(args, 0);
        let id = a.as_str().unwrap_or(s(&a["id"])).to_string();
        match channel {
            "ai-extensions:oauth-start" => self.mcp_oauth_start(&id, arg(args, 1).as_str()).await,
            "ai-extensions:oauth-cancel" => Ok(self.cancel_oauth_native(&format!(
                "mcp:{}",
                key(&id, arg(args, 1).as_str().unwrap_or(""))
            ))),
            "ai-extensions:oauth-status" => Ok(self
                .inner
                .oauth_results
                .lock()
                .unwrap()
                .get(&format!(
                    "mcp:{}",
                    key(&id, arg(args, 1).as_str().unwrap_or(""))
                ))
                .cloned()
                .unwrap_or(json!({"status":"idle"}))),
            "ai-extensions:review-project" => {
                let cwd = tokio::fs::canonicalize(s(a)).await.map_err(err)?;
                self.project_status(&cwd).await?;
                *self.inner.extensions.review_cwd.lock().unwrap() =
                    Some(cwd.to_string_lossy().into());
                self.inner
                    .ctx
                    .emit("ai-extensions-changed", json!({"projectReview":true}));
                Ok(json!({"ok":true}))
            }
            "ai-extensions:list" => {
                let mut data = self.ext_read().await?;
                let core = self.clone();
                data["canResetSecrets"] = json!(tokio::task::spawn_blocking(move || core
                    .inner
                    .extensions
                    .secrets
                    .reset_available())
                .await
                .map_err(err)?
                .unwrap_or(false));
                data["defaultWorkingFolder"] = json!(self.inner.ctx.paths.home_dir);
                let queued = self.inner.extensions.review_cwd.lock().unwrap().clone();
                if let Some(cwd) = a.as_str().or(queued.as_deref()) {
                    data["project"] = self.project_status(std::path::Path::new(cwd)).await.unwrap_or_else(|_|json!({"folder":cwd,"error":"The project folder is no longer available. Choose an existing folder to review."}));
                }
                Ok(data)
            }
            "ai-extensions:reset-secrets" => {
                let core = self.clone();
                tokio::task::spawn_blocking(move || core.inner.extensions.secrets.reset_missing())
                    .await
                    .map_err(err)??;
                let core = self.clone();
                self.ext_edit(move |data| {
                    for ext in data["extensions"].as_array_mut().unwrap() {
                        let eid = s(&ext["id"]).to_string();
                        for c in ext["connections"].as_array_mut().into_iter().flatten() {
                            let secret_refs = ["env", "headers"].iter().any(|k| {
                                c[*k]
                                    .as_object()
                                    .is_some_and(|o| o.values().any(|v| v["secret"].is_string()))
                            });
                            if secret_refs || matches!(s(&c["auth"]["type"]), "token" | "oauth") {
                                c["tested"] = json!(false);
                                c["needsAuth"] = json!(true);
                                c["error"] =
                                    json!("Enter the secret again, then test this connection.");
                                let namespace = key(&eid, s(&c["id"]));
                                if let Some(start) = core
                                    .inner
                                    .extensions
                                    .starting
                                    .lock()
                                    .unwrap()
                                    .get(&namespace)
                                {
                                    start.cancel.cancel();
                                }
                                if let Some(client) = core
                                    .inner
                                    .extensions
                                    .clients
                                    .lock()
                                    .unwrap()
                                    .remove(&namespace)
                                {
                                    client.shutdown();
                                }
                            }
                        }
                    }
                    Ok(json!({"ok":true}))
                })
                .await
            }
            "ai-extensions:trust-project" => self.trust_project(s(a), s(arg(args, 1))).await,
            "ai-extensions:clear-allowance" => {
                let cid = s(arg(args, 1)).to_string();
                let tool = s(arg(args, 2)).to_string();
                self.ext_edit(move |data| {
                    let c = connection_mut(record_mut(data, &id)?, &cid)?;
                    if let Some(allowances) = c["allowances"].as_object_mut() {
                        allowances.remove(&tool);
                    }
                    Ok(json!({"ok":true}))
                })
                .await
            }
            "ai-extensions:logs" => {
                let mut lines = vec![];
                {
                    let clients = self.inner.extensions.clients.lock().unwrap();
                    for (k, client) in clients
                        .iter()
                        .filter(|(key, _)| key.starts_with(&prefix(&id)))
                    {
                        let _ = k;
                        lines.extend(client.logs.lock().unwrap().iter().cloned());
                    }
                }
                if lines.is_empty() {
                    for (key, (_, _, error)) in
                        self.inner.extensions.failures.lock().unwrap().iter()
                    {
                        if key.starts_with(&prefix(&id)) {
                            lines.extend(redact_text(error).lines().map(str::to_string));
                        }
                    }
                }
                if lines.is_empty() {
                    if let Some(record) = self.ext_read().await?["extensions"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|e| e["id"] == id)
                    {
                        lines.extend(s(&record["error"]).lines().map(str::to_string));
                    }
                }
                if lines.len() > 20 {
                    lines.drain(..lines.len() - 20);
                }
                Ok(json!(lines))
            }
            "ai-extensions:parse-import" => {
                let text = if let Some(text) = a.as_str().or_else(|| a["text"].as_str()) {
                    text.to_string()
                } else {
                    let path = a["filePath"]
                        .as_str()
                        .ok_or("Choose a file or paste configuration")?;
                    let meta = tokio::fs::metadata(path).await.map_err(err)?;
                    if meta.len() > 256 * 1024 {
                        return Err("Configuration exceeds 256 KiB".into());
                    }
                    tokio::fs::read_to_string(path).await.map_err(err)?
                };
                let mut manifest = manifest::parse(&text)?;
                for c in manifest["connections"].as_array_mut().into_iter().flatten() {
                    if c["transport"] == "stdio" && s(&c["cwd"]).is_empty() {
                        c["cwd"] = json!(self.inner.ctx.paths.home_dir);
                    }
                }
                let mut preview = manifest.clone();
                let mut count = 0;
                for c in preview["connections"].as_array_mut().into_iter().flatten() {
                    for section in ["env", "headers"] {
                        for (k, v) in c[section].as_object_mut().into_iter().flatten() {
                            if let Some(literal) = v.as_str() {
                                if manifest::secret_key(k, literal) {
                                    *v = json!({"secret":k,"label":k});
                                    count += 1;
                                }
                            }
                        }
                    }
                }
                for p in preview["providers"].as_array_mut().into_iter().flatten() {
                    for (k, v) in p["headers"].as_object_mut().into_iter().flatten() {
                        if let Some(literal) = v.as_str() {
                            if manifest::secret_key(k, literal) {
                                *v = json!({"secret":k,"label":k});
                                count += 1;
                            }
                        }
                    }
                }
                let request = uuid::Uuid::new_v4().to_string();
                let mut imports = self.inner.extensions.imports.lock().unwrap();
                if imports.len() >= 8 {
                    imports.clear();
                }
                imports.insert(request.clone(), manifest);
                Ok(json!({"previewId":request,"manifest":preview,"secretCount":count}))
            }
            "ai-extensions:add" => {
                let manifest = if let Some(id) = a["previewId"].as_str() {
                    self.inner
                        .extensions
                        .imports
                        .lock()
                        .unwrap()
                        .remove(id)
                        .ok_or("Import preview expired; preview it again")?
                } else {
                    manifest::parse(&a.to_string())?
                };
                let core = self.clone();
                let armed = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let guard = NewSecretGuard {
                    core: core.clone(),
                    namespaces: manifest["connections"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|c| key(s(&manifest["id"]), s(&c["id"])))
                        .chain(
                            manifest["providers"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .map(|p| provider_key(s(&manifest["id"]), s(&p["id"]))),
                        )
                        .collect(),
                    armed: armed.clone(),
                };
                let result = self
                    .ext_edit(move |data| {
                        let mut record = manifest;
                        let rid = s(&record["id"]).to_string();
                        if data["extensions"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|e| e["id"] == rid)
                        {
                            return Err("An extension with this ID already exists".into());
                        }
                        for c in record["connections"].as_array().into_iter().flatten() {
                            if c["toolPolicy"] == "allow" && a_confirmation(c) != s(&c["name"]) {
                                return Err(
                                    "Type the connection name to allow all of its tools".into()
                                );
                            }
                        }
                        armed.store(true, std::sync::atomic::Ordering::SeqCst);
                        record["enabled"] = json!(false);
                        for c in record["connections"].as_array_mut().into_iter().flatten() {
                            if c["transport"] == "http" {
                                c["url"] = json!(oauth::secure_url(s(&c["url"]).trim())?.as_str());
                            }
                            if c["transport"] == "stdio" && s(&c["cwd"]).is_empty() {
                                c["cwd"] = json!(core.inner.ctx.paths.home_dir);
                            }
                            let cid = s(&c["id"]).to_string();
                            c["slug"] = json!(format!(
                                "ext{}",
                                &manifest::hash(&json!([&rid, &cid]))[..12]
                            ));
                            c["tested"] = json!(false);
                            if c["toolPolicy"] == "allow" && a_confirmation(c) != s(&c["name"]) {
                                return Err(
                                    "Type the connection name to allow all of its tools".into()
                                );
                            }
                            for section in ["env", "headers"] {
                                for (k, v) in c[section].as_object_mut().into_iter().flatten() {
                                    if let Some(literal) = v.as_str() {
                                        if manifest::secret_key(k, literal) {
                                            core.inner.extensions.secrets.put(
                                                &key(&rid, &cid),
                                                k,
                                                Some(literal),
                                            )?;
                                            *v = json!({"secret":k,"label":k});
                                        }
                                    }
                                }
                            }
                        }
                        for p in record["providers"].as_array_mut().into_iter().flatten() {
                            let pid = s(&p["id"]).to_string();
                            for (k, v) in p["headers"].as_object_mut().into_iter().flatten() {
                                if let Some(literal) = v.as_str() {
                                    if manifest::secret_key(k, literal) {
                                        core.inner.extensions.secrets.put(
                                            &provider_key(&rid, &pid),
                                            k,
                                            Some(literal),
                                        )?;
                                        *v = json!({"secret":k,"label":k});
                                    }
                                }
                            }
                        }
                        data["extensions"]
                            .as_array_mut()
                            .unwrap()
                            .push(record.clone());
                        Ok(record)
                    })
                    .await;
                if result.is_ok() {
                    guard
                        .armed
                        .store(false, std::sync::atomic::Ordering::SeqCst);
                }
                result
            }
            "ai-extensions:test" => {
                self.inner
                    .extensions
                    .failures
                    .lock()
                    .unwrap()
                    .retain(|key, _| !key.starts_with(&prefix(&id)));
                match self.ext_discover_mode(&id, true).await {
                    Ok(v) => Ok(v),
                    Err(error) => {
                        let message = cut(&redact_text(&error), 500);
                        let saved = message.clone();
                        self.ext_edit(move |data| {
                            let r = record_mut(data, &id)?;
                            r["error"] = json!(saved);
                            Ok(Value::Null)
                        })
                        .await?;
                        Ok(json!({"ok":false,"error":message}))
                    }
                }
            }
            "ai-extensions:set-enabled" => {
                let enabled = arg(args, 1).as_bool().unwrap_or(a["enabled"] == true);
                let stop = id.clone();
                if !enabled {
                    for (k, start) in self.inner.extensions.starting.lock().unwrap().iter() {
                        if k.starts_with(&prefix(&stop)) {
                            start.cancel.cancel();
                        }
                    }
                    let mut clients = self.inner.extensions.clients.lock().unwrap();
                    let ids = clients
                        .keys()
                        .filter(|c| c.starts_with(&prefix(&stop)))
                        .cloned()
                        .collect::<Vec<_>>();
                    for id in ids {
                        if let Some(c) = clients.remove(&id) {
                            c.shutdown();
                        }
                    }
                }
                self.ext_edit(move |data| {
                    let r = record_mut(data, &id)?;
                    if enabled
                        && r["connections"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|c| c["tested"] != true || c["trustPin"] != command_pin(c))
                    {
                        return Err(
                            "Test each connection and review changed tools before enabling".into(),
                        );
                    }
                    r["enabled"] = json!(enabled);
                    Ok(r.clone())
                })
                .await
            }
            "ai-extensions:remove" => {
                let core = self.clone();
                self.ext_edit(move |data| {
                    let r = record_mut(data, &id)?;
                    for c in r["connections"].as_array().into_iter().flatten() {
                        let cid = key(&id, s(&c["id"]));
                        if let Some(start) =
                            core.inner.extensions.starting.lock().unwrap().get(&cid)
                        {
                            start.cancel.cancel();
                        }
                        if let Some(c) = core.inner.extensions.clients.lock().unwrap().remove(&cid)
                        {
                            c.shutdown()
                        }
                        core.inner.extensions.secrets.put(&cid, "", None)?;
                    }
                    for p in r["providers"].as_array().into_iter().flatten() {
                        core.inner.extensions.secrets.put(
                            &provider_key(&id, s(&p["id"])),
                            "",
                            None,
                        )?;
                    }
                    data["extensions"]
                        .as_array_mut()
                        .unwrap()
                        .retain(|r| r["id"] != id);
                    Ok(json!({"ok":true}))
                })
                .await
            }
            "ai-extensions:update" => {
                let patch = arg(args, 1).clone();
                let core = self.clone();
                self.ext_edit(move |data| {
                    let r = record_mut(data, &id)?;
                    let previous = r["connections"].clone();
                    for (k, v) in patch.as_object().ok_or("Invalid extension update")? {
                        if matches!(
                            k.as_str(),
                            "name"
                                | "instructions"
                                | "prompts"
                                | "commands"
                                | "agents"
                                | "connections"
                        ) {
                            r[k] = v.clone();
                        }
                    }
                    manifest::parse(&r.to_string())?;
                    if patch.get("connections").is_some() {
                        r["enabled"] = json!(false);
                        for c in r["connections"].as_array_mut().into_iter().flatten() {
                            manifest::validate_connection(c)?;
                            if c["transport"] == "http" {
                                c["url"] = json!(oauth::secure_url(s(&c["url"]).trim())?.as_str());
                            }
                            if c["toolPolicy"] == "allow" && a_confirmation(c) != s(&c["name"]) {
                                return Err("Type the connection name to allow all tools".into());
                            }
                            let connection_id = s(&c["id"]).to_string();
                            for section in ["env", "headers"] {
                                for (k, v) in c[section].as_object_mut().into_iter().flatten() {
                                    if let Some(literal) = v.as_str() {
                                        if manifest::secret_key(k, literal) {
                                            core.inner.extensions.secrets.put(
                                                &key(&id, &connection_id),
                                                k,
                                                Some(literal),
                                            )?;
                                            *v = json!({"secret":k,"label":k});
                                        }
                                    }
                                }
                            }
                            let old = previous
                                .as_array()
                                .into_iter()
                                .flatten()
                                .find(|old| old["id"] == c["id"]);
                            let changed = old.is_none_or(|old| command_pin(old) != command_pin(c));
                            for field in [
                                "slug",
                                "pins",
                                "pendingPins",
                                "pinnedCatalog",
                                "previousCatalog",
                                "catalog",
                                "needsReview",
                                "tested",
                                "trustPin",
                                "catalogAt",
                                "version",
                                "modelNames",
                                "hasResources",
                                "allowances",
                                "cachedEra",
                                "requestedScopes",
                                "error",
                                "needsAuth",
                            ] {
                                c.as_object_mut().unwrap().remove(field);
                                if let Some(value) = old.and_then(|old| old.get(field)) {
                                    c[field] = value.clone();
                                }
                            }
                            if !changed {
                                continue;
                            }
                            c.as_object_mut().unwrap().remove("cachedEra");
                            c["tested"] = json!(false);
                            c["allowances"] = json!({});
                            let cid = key(&id, s(&c["id"]));
                            if let Some(start) =
                                core.inner.extensions.starting.lock().unwrap().get(&cid)
                            {
                                start.cancel.cancel();
                            }
                            if let Some(client) =
                                core.inner.extensions.clients.lock().unwrap().remove(&cid)
                            {
                                client.shutdown();
                            }
                        }
                    }
                    Ok(r.clone())
                })
                .await
            }
            "ai-extensions:accept-review" => {
                let reviewed = arg(args, 1).clone();
                self.ext_edit(move |data| {
                    let r = record_mut(data, &id)?;
                    for c in r["connections"].as_array_mut().into_iter().flatten() {
                        if c["needsReview"] != true {
                            continue;
                        }
                        if reviewed[s(&c["id"])]
                            != json!({"pins":c["pendingPins"],"trustPin":c["trustPin"]})
                        {
                            return Err(
                                "The tools changed since this review. Review the latest changes."
                                    .into(),
                            );
                        }
                        c["pins"] = c["pendingPins"].clone();
                        c["pinnedCatalog"] = c["catalog"].clone();
                        c["previousCatalog"] = Value::Null;
                        c["needsReview"] = json!(false);
                        c["allowances"] = json!({});
                    }
                    Ok(r.clone())
                })
                .await
            }
            "ai-extensions:set-tool" => {
                let name = arg(args, 1).clone();
                let enabled = arg(args, 2).as_bool().unwrap_or(false);
                let cid = arg(args, 3).as_str().map(str::to_string);
                self.ext_edit(move |data| {
                    let r = record_mut(data, &id)?;
                    for c in r["connections"].as_array_mut().into_iter().flatten() {
                        if cid.as_deref().is_some_and(|id| c["id"] != id) {
                            continue;
                        }
                        if !c["disabledTools"].is_object() {
                            c["disabledTools"] = json!({})
                        }
                        c["disabledTools"][s(&name)] = json!(!enabled);
                    }
                    Ok(r.clone())
                })
                .await
            }
            "ai-extensions:set-secret" => {
                let name = arg(args, 1)
                    .as_str()
                    .ok_or("Secret name is required")?
                    .to_string();
                let value = arg(args, 2)
                    .as_str()
                    .ok_or("Secret value is required")?
                    .to_string();
                let connection = arg(args, 3).as_str().map(str::to_string);
                let provider = arg(args, 3)["providerId"].as_str().map(str::to_string);
                let core = self.clone();
                self.ext_edit(move |data| {
                    let r = record_mut(data, &id)?;
                    if name.len() > 200 || value.len() > 128 * 1024 {
                        return Err("Secret name or value exceeds its limit".into());
                    }
                    if let Some(provider) = provider {
                        let p = r["providers"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .find(|p| p["id"] == provider)
                            .ok_or("Provider was removed")?;
                        if !p["headers"]
                            .as_object()
                            .into_iter()
                            .flatten()
                            .any(|(_, value)| value["secret"] == name)
                        {
                            return Err("Provider secret is not declared".into());
                        }
                        core.inner.extensions.secrets.put(
                            &provider_key(&id, &provider),
                            &name,
                            Some(&value),
                        )?;
                        r["enabled"] = json!(false);
                        return Ok(json!({"hasValue":true}));
                    }
                    let cid = connection
                        .or_else(|| r["connections"][0]["id"].as_str().map(str::to_string))
                        .ok_or("Connection is required")?;
                    if name.len() > 200 || value.len() > 128 * 1024 {
                        return Err("Secret name or value exceeds its limit".into());
                    }
                    let c = connection_mut(r, &cid)?;
                    c["tested"] = json!(false);
                    c["allowances"] = json!({});
                    r["enabled"] = json!(false);
                    let namespace = key(&id, &cid);
                    if let Some(start) = core
                        .inner
                        .extensions
                        .starting
                        .lock()
                        .unwrap()
                        .get(&namespace)
                    {
                        start.cancel.cancel();
                    }
                    if let Some(client) = core
                        .inner
                        .extensions
                        .clients
                        .lock()
                        .unwrap()
                        .remove(&namespace)
                    {
                        client.shutdown();
                    }
                    core.inner
                        .extensions
                        .secrets
                        .put(&namespace, &name, Some(&value))?;
                    Ok(json!({"hasValue":true}))
                })
                .await
            }
            "ai-extensions:set-instructions" => {
                let text = a.as_str().ok_or("Instructions must be text")?.to_string();
                if text.len() > 128 * 1024 {
                    return Err("Instructions exceed 128 KiB".into());
                }
                let use_agents = arg(args, 1).as_bool().unwrap_or(true);
                self.ext_edit(move |data| {
                    data["globalInstructions"] = json!(text);
                    data["useAgentsMd"] = json!(use_agents);
                    Ok(json!({"ok":true}))
                })
                .await
            }
            "ai-extensions:composer-items" => {
                let cwd = a
                    .as_str()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| self.inner.ctx.paths.home_dir.clone());
                let cwd = tokio::fs::canonicalize(cwd).await.map_err(err)?;
                self.revoke_changed_projects(&cwd).await?;
                let data = self.ext_read().await?;
                let mut items = vec![];
                for e in data["extensions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|e| e["enabled"] == true)
                {
                    if e["projectPath"].as_str().is_some_and(|folder| {
                        !runtime::policy::within(&cwd, std::path::Path::new(folder))
                    }) {
                        continue;
                    }
                    for kind in ["prompts", "commands", "agents"] {
                        for p in e[kind].as_array().into_iter().flatten() {
                            if kind == "commands"
                                && p["name"] == p["prompt"]
                                && e["prompts"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .any(|prompt| prompt["name"] == p["name"])
                            {
                                continue;
                            }
                            let arguments = if kind == "commands" {
                                e["prompts"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .find(|prompt| prompt["name"] == p["prompt"])
                                    .map(|prompt| prompt["arguments"].clone())
                                    .unwrap_or(Value::Null)
                            } else {
                                p["arguments"].clone()
                            };
                            items.push(json!({"extensionId":e["id"],"extensionName":e["name"],"kind":kind,"name":p["name"],"id":if kind=="agents"{json!(format!("{}::{}",s(&e["id"]),s(&p["id"])))}else{p.get("id").unwrap_or(&p["name"]).clone()},"title":p["title"],"arguments":arguments}));
                        }
                    }
                    for c in e["connections"].as_array().into_iter().flatten() {
                        for kind in ["prompts", "resources"] {
                            for p in c["catalog"][kind].as_array().into_iter().flatten() {
                                items.push(json!({"extensionId":e["id"],"connectionId":c["id"],"extensionName":e["name"],"kind":kind,"name":p["name"],"uri":p["uri"],"title":p["title"],"arguments":p["arguments"]}));
                            }
                        }
                    }
                }
                Ok(json!(items))
            }
            "ai-extensions:prompt-resolve" | "ai-extensions:read-resource" => {
                let data = self.ext_read().await?;
                let eid = s(&a["extensionId"]);
                let record = data["extensions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|e| e["id"] == eid && e["enabled"] == true)
                    .ok_or("Extension is disabled")?;
                if let Some(folder) = record["projectPath"].as_str() {
                    let cwd = a["cwd"]
                        .as_str()
                        .ok_or("Choose the project's working folder first")?;
                    let cwd = tokio::fs::canonicalize(cwd).await.map_err(err)?;
                    self.revoke_changed_projects(&cwd).await?;
                    if !runtime::policy::within(&cwd, std::path::Path::new(folder)) {
                        return Err("This contribution belongs to another folder".into());
                    }
                    if !self.ext_read().await?["extensions"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|e| e["id"] == eid && e["enabled"] == true)
                    {
                        return Err(
                            "Project configuration changed. Review it in AI settings.".into()
                        );
                    }
                }
                let arguments = arg(args, 1);
                let text = if let Some(cid) = a["connectionId"].as_str() {
                    let c = record["connections"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|c| c["id"] == cid)
                        .ok_or("Connection was removed")?;
                    let client = self.ext_client(eid, c).await?;
                    let method = if channel.ends_with("read-resource") {
                        "resources/read"
                    } else {
                        "prompts/get"
                    };
                    if c["tested"] != true || c["trustPin"] != command_pin(c) {
                        return Err("Test this connection before using it".into());
                    }
                    let result = if method == "resources/read" {
                        self.extension_resource(
                            &client,
                            c,
                            ResourceOp::Read,
                            &json!({"uri":a["uri"]}),
                            &Cancel::new(),
                        )
                        .await?
                    } else {
                        let prompt = c["catalog"]["prompts"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .find(|p| p["name"] == a["name"])
                            .ok_or("Choose a prompt listed by this connection")?;
                        resources::validate_prompt_arguments(prompt, arguments)?;
                        client
                            .call(
                                method,
                                json!({"name":a["name"],"arguments":arguments}),
                                &Cancel::new(),
                                Duration::from_secs(10),
                            )
                            .await?
                    };
                    return resources::draft(&client, &result, method == "prompts/get");
                } else {
                    let name = s(&a["name"]);
                    let name = record["commands"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|c| c["name"] == name)
                        .and_then(|c| c["prompt"].as_str())
                        .unwrap_or(name);
                    let prompt = record["prompts"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|p| p["name"] == name)
                        .ok_or("Prompt was removed")?;
                    let mut template = s(&prompt["template"]).to_string();
                    for argument in prompt["arguments"].as_array().into_iter().flatten() {
                        let name = s(&argument["name"]);
                        let value = arguments[name].as_str().unwrap_or("");
                        if argument["required"] == true && value.trim().is_empty() {
                            return Err(format!("Enter {name}"));
                        }
                        template = template.replace(&format!("{{{{{name}}}}}"), value);
                    }
                    template
                };
                if text.len() > 65536 {
                    return Err("Prompt exceeds 64 KiB".into());
                }
                Ok(json!({"text":text,"images":[]}))
            }
            _ => Err("Unknown AI extension action".into()),
        }
    }
}
fn a_confirmation(c: &Value) -> &str {
    s(&c["allowAllConfirmation"])
}
pub(super) fn definitions(snapshot: &Snapshot) -> Result<Vec<Value>, String> {
    snapshot.iter().map(|(name,e)|Ok(json!({"type":"function","function":{"name":name,"description":e.descriptor["description"].as_str().or(e.descriptor["title"].as_str()).unwrap_or(""),"parameters":mcp::schema::for_wire(&e.descriptor["inputSchema"])?}}))).collect()
}
pub(super) async fn validate_call(entry: &Entry, args: Value) -> Result<(), String> {
    mcp::schema::validate(entry.descriptor["inputSchema"].clone(), args).await
}
impl AiCore {
    pub(super) async fn extension_instructions(
        &self,
        cwd: &std::path::Path,
        options: &Value,
    ) -> Result<(String, Option<HashSet<String>>), String> {
        self.revoke_changed_projects(cwd).await?;
        let data = self.ext_read().await?;
        let mut instructions = String::new();
        let mut profile = None;
        let mut profile_text = None;
        fn add(out: &mut String, label: &str, text: &str) {
            if !text.trim().is_empty() {
                let block = format!("\n<{label}>\n{text}\n</{label}>\n");
                if out.len() + block.len() <= 128 * 1024 {
                    out.push_str(&block);
                } else {
                    let notice=format!("\n[Instructions from {label} truncated to fit the combined 128 KiB limit]\n");
                    let remaining = (128 * 1024usize)
                        .saturating_sub(out.len() + notice.len() + label.len() * 2 + 8);
                    let mut text = text.to_string();
                    while text.len() > remaining {
                        text.pop();
                    }
                    out.push_str(&format!("\n<{label}>\n{text}\n</{label}>\n{notice}"));
                    while out.len() > 128 * 1024 {
                        out.pop();
                    }
                }
            }
        }
        add(
            &mut instructions,
            "Global custom instructions",
            s(&data["globalInstructions"]),
        );
        for e in data["extensions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["enabled"] == true)
        {
            if e["projectPath"]
                .as_str()
                .is_some_and(|folder| !runtime::policy::within(cwd, std::path::Path::new(folder)))
            {
                continue;
            }

            add(
                &mut instructions,
                &format!("Extension {}", s(&e["name"])),
                s(&e["instructions"]),
            );
            for c in e["connections"].as_array().into_iter().flatten() {
                if let Some(client) = self
                    .inner
                    .extensions
                    .clients
                    .lock()
                    .unwrap()
                    .get(&key(s(&e["id"]), s(&c["id"])))
                {
                    add(
                        &mut instructions,
                        "Unverified connection instructions",
                        &client.instructions.lock().unwrap(),
                    );
                }
            }
            if let Some(p) = e["agents"].as_array().into_iter().flatten().find(|p| {
                !options["agentProfileId"].is_null()
                    && (p["id"] == options["agentProfileId"]
                        || options["agentProfileId"] == format!("{}::{}", s(&e["id"]), s(&p["id"])))
            }) {
                if profile_text.is_some() {
                    return Err("Choose a profile from a specific extension".into());
                }
                profile_text = Some(s(&p["instructions"]).to_string());
                if let Some(tools) = p["tools"].as_array() {
                    profile = Some(
                        tools
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect(),
                    );
                }
            }
        }
        if options["agentProfileId"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
            && profile_text.is_none()
        {
            return Err("The selected agent profile is unavailable. Enable its extension or remove the profile from this conversation.".into());
        }
        if let Some(text) = profile_text {
            add(&mut instructions, "Active profile", &text);
        }
        if data["useAgentsMd"] != false {
            let path = cwd.join("AGENTS.md");
            if let Ok(real) = tokio::fs::canonicalize(&path).await {
                if runtime::policy::within(&real, cwd)
                    && !runtime::policy::sensitive(
                        &real,
                        &self.inner.ctx.paths.home_dir,
                        &self.inner.ctx.paths.config_dir,
                    )
                {
                    if let Ok(meta) = tokio::fs::metadata(&real).await {
                        if meta.len() <= 65536 {
                            let text = tokio::fs::read_to_string(real).await.map_err(err)?;
                            add(
                                &mut instructions,
                                "Project-supplied AGENTS.md; cannot change permissions",
                                &text,
                            );
                        }
                    }
                }
            }
        }
        add(
            &mut instructions,
            "Request instructions",
            s(&options["instructions"]),
        );
        if instructions.len() > 128 * 1024 {
            return Err("Native limit: instructions exceed 128 KiB".into());
        }
        Ok((instructions, profile))
    }
}

#[cfg(test)]
mod tests;

impl AiCore {
    pub(super) async fn remember_extension_tool(
        &self,
        extension: String,
        connection: String,
        tool: String,
        hash: String,
    ) -> Result<(), String> {
        self.ext_edit(move |data| {
            let r = record_mut(data, &extension)?;
            if r["enabled"] != true {
                return Err("Extension was disabled".into());
            }
            let c = connection_mut(r, &connection)?;
            if (c["pins"][&tool] != hash
                && !(matches!(tool.as_str(), "list_resources" | "read_resource")
                    && resources::catalog_hash(c) == hash))
                || c["trustPin"] != command_pin(c)
                || (matches!(tool.as_str(), "list_resources" | "read_resource")
                    && resources::catalog_hash(c) != hash)
                || (!matches!(tool.as_str(), "list_resources" | "read_resource")
                    && current_tool_hash(c, &tool).as_deref() != Some(hash.as_str()))
            {
                return Err("Tool changed. Review it in AI settings.".into());
            }
            if !c["allowances"].is_object() {
                c["allowances"] = json!({})
            }
            c["allowances"][&tool] = json!(hash);
            Ok(json!({"ok":true}))
        })
        .await
        .map(|_| ())
    }
}
