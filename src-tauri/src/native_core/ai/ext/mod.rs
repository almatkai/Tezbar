//! User-added AI contributions. Imports are data only; only Test may start a
//! disabled connection. Agent runs consume an immutable, pinned tool table.
mod manifest;
mod mcp;
mod secrets;
use super::*;
use mcp::client::Client;
use std::collections::{BTreeMap, HashSet};
pub const CHANNELS: &[&str] = &[
    "ai-extensions:list",
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
pub struct Extensions {
    lock: Mutex<()>,
    clients: Mutex<HashMap<String, Arc<Client>>>,
    imports: Mutex<HashMap<String, Value>>,
    secrets: secrets::Secrets,
    path: std::path::PathBuf,
}
pub struct Entry {
    pub extension: String,
    pub connection: String,
    pub descriptor: Value,
    pub hash: String,
    pub client: Arc<Client>,
}
pub type Snapshot = BTreeMap<String, Entry>;
impl Extensions {
    pub fn new(config: &std::path::Path) -> Self {
        Self {
            lock: Mutex::new(()),
            clients: Mutex::new(HashMap::new()),
            imports: Mutex::new(HashMap::new()),
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
    fn edit(&self, f: impl FnOnce(&mut Value) -> CoreResult) -> CoreResult {
        let _guard = self.lock.lock().unwrap();
        let mut data = self.load()?;
        let result = f(&mut data)?;
        common::write_json_atomic(&self.path, &data)?;
        Ok(result)
    }
    pub fn shutdown(&self) {
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
    format!("{ext}:{conn}")
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
    ] {
        v.as_object_mut().unwrap().remove(k);
    }
    manifest::hash(&v)
}
impl AiCore {
    async fn ext_read(&self) -> CoreResult {
        let core = self.clone();
        tokio::task::spawn_blocking(move || core.inner.extensions.load())
            .await
            .map_err(err)?
    }
    async fn ext_edit(
        &self,
        f: impl FnOnce(&mut Value) -> CoreResult + Send + 'static,
    ) -> CoreResult {
        let core = self.clone();
        let result = tokio::task::spawn_blocking(move || core.inner.extensions.edit(f))
            .await
            .map_err(err)??;
        self.inner.ctx.emit("ai-extensions-changed", Value::Null);
        Ok(result)
    }
    async fn ext_client(&self, extension: &str, connection: &Value) -> Result<Arc<Client>, String> {
        let id = key(extension, s(&connection["id"]));
        if let Some(c) = self.inner.extensions.clients.lock().unwrap().get(&id) {
            return Ok(c.clone());
        }
        let core = self.clone();
        let secret_id = id.clone();
        let values =
            tokio::task::spawn_blocking(move || core.inner.extensions.secrets.values(&secret_id))
                .await
                .map_err(err)??;
        let mut env = HashMap::new();
        let mut c = connection.clone();
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
        let client = Client::connect(self, c, env, values.into_values().collect()).await?;
        self.inner
            .extensions
            .clients
            .lock()
            .unwrap()
            .insert(id, client.clone());
        Ok(client)
    }
    async fn ext_discover(&self, id: &str) -> CoreResult {
        let data = self.ext_read().await?;
        let ext = data["extensions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == id)
            .ok_or("Extension was removed")?;
        let mut updated = vec![];
        for c in ext["connections"].as_array().into_iter().flatten() {
            let client = self.ext_client(id, c).await?;
            let mut catalog =
                json!({"tools":[],"prompts":[],"resources":[],"templates":[],"warnings":[]});
            let caps = client.capabilities.lock().unwrap().clone();
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
                    let result = client
                        .call(method, params, &Cancel::new(), Duration::from_secs(10))
                        .await?;
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
                        if kind == "tools" {
                            let item = item.clone();
                            let schema = item["inputSchema"].clone();
                            let valid = tokio::time::timeout(
                                Duration::from_millis(100),
                                tokio::task::spawn_blocking(move || mcp::schema::compile(&schema)),
                            )
                            .await;
                            if !matches!(valid, Ok(Ok(Ok(_))))
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
                    items.retain(|item| counts[s(&item["name"])] == 1);
                }
                catalog[kind] = json!(items);
            }
            updated.push((
                s(&c["id"]).to_string(),
                catalog,
                client.version.lock().unwrap().clone(),
                command_pin(c),
            ));
        }
        let id = id.to_string();
        self.ext_edit(move |data| {
            let record = record_mut(data, &id)?;
            for (cid, catalog, version, pin) in updated {
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
                c["pendingPins"] = pins.clone();
                if !old.is_object() {
                    c["pins"] = pins
                }
                c["catalog"] = catalog;
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
        let data = self.ext_read().await?;
        let mut snapshot = Snapshot::new();
        let mut used = HashSet::new();
        for ext in data["extensions"].as_array().unwrap() {
            if ext["enabled"] != true {
                continue;
            }
            if let Some(project) = ext["projectPath"].as_str() {
                if !cwd.starts_with(project) {
                    continue;
                }
            }
            for c in ext["connections"].as_array().into_iter().flatten() {
                if c["tested"] != true
                    || c["needsReview"] == true
                    || c["trustPin"] != command_pin(c)
                {
                    continue;
                }
                let client = match tokio::time::timeout(
                    Duration::from_secs(5),
                    self.ext_client(s(&ext["id"]), c),
                )
                .await
                {
                    Ok(Ok(c)) => c,
                    _ => continue,
                };
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
                    if name.len() > 64 || used.contains(&name.to_lowercase()) {
                        name = format!(
                            "{slug}__{}_{}",
                            sanitized.chars().take(36).collect::<String>(),
                            &hash[..6]
                        );
                    }
                    used.insert(name.to_lowercase());
                    snapshot.insert(
                        name,
                        Entry {
                            extension: s(&ext["id"]).into(),
                            connection: s(&c["id"]).into(),
                            descriptor: tool.clone(),
                            hash,
                            client: client.clone(),
                        },
                    );
                }
            }
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
    ) -> Result<Vec<runtime::transcript::Block>, String> {
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
            if c["disabledTools"][s(&entry.descriptor["name"])]==true || c["needsReview"] == true
                || c["pins"][s(&entry.descriptor["name"])] != entry.hash
                || c["trustPin"] != command_pin(c)
            {
                return Err("Tool changed. Review it in AI settings.".into());
            }
            Ok(c.clone())
        };
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
        if ask
            && !self
                .approval_prompt_identity(
                    run,
                    &format!("Allow {} to use {name}?", s(&c["name"])),
                    &cut(&serde_json::to_string_pretty(&args).map_err(err)?, 4000),
                    cancel,
                    false,
                    Some((entry.extension.clone(),entry.connection.clone(),name.into(),entry.hash.clone())),
                )
                .await
        {
            return Err("Tool was denied or approval expired".into());
        }
        check(&self.ext_read().await?)?;
        if cancel.is_cancelled() {
            return Err("Operation cancelled".into());
        }
        let mut result = entry
            .client
            .call(
                "tools/call",
                json!({"name":name,"arguments":args}),
                cancel,
                Duration::from_secs(c["timeoutSeconds"].as_u64().unwrap_or(120).min(1800)),
            )
            .await?;
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
                .call(
                    "tools/call",
                    json!({"name":name,"arguments":args,"requestState":state}),
                    cancel,
                    Duration::from_secs(120),
                )
                .await?;
        }
        if result["resultType"] == "input_required" {
            return Err("Tool exceeded the continuation limit".into());
        }
        let mut blocks = vec![];
        let mut text = String::new();
        for item in result["content"].as_array().into_iter().flatten() {
            if item["annotations"]["audience"] == json!(["user"]) {
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
                    }
                }
                _ => {}
            }
        }
        if text.is_empty() && blocks.is_empty() {
            text = result["structuredContent"].to_string();
        }
        let mut text = entry.client.scrub(&text);
        while text.len() > runtime::limits::RESULT_BYTES - 100 {
            text.pop();
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
        Ok(blocks)
    }
}
impl AiCore {
    pub(super) async fn ext_handle(&self, channel: &str, args: &[Value]) -> CoreResult {
        let a = arg(args, 0);
        let id = a.as_str().unwrap_or(s(&a["id"])).to_string();
        match channel {
            "ai-extensions:list" => self.ext_read().await,
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
                let manifest = manifest::parse(&text)?;
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
                self.ext_edit(move |data| {
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
                    record["enabled"] = json!(false);
                    for c in record["connections"].as_array_mut().into_iter().flatten() {
                        let cid = s(&c["id"]).to_string();
                        c["slug"] = json!(format!(
                            "ext{}",
                            &manifest::hash(&json!([&rid, &cid]))[..12]
                        ));
                        c["tested"] = json!(false);
                        if c["toolPolicy"] == "allow" && a_confirmation(c) != s(&c["name"]) {
                            return Err("Type the connection name to allow all of its tools".into());
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
                    data["extensions"]
                        .as_array_mut()
                        .unwrap()
                        .push(record.clone());
                    Ok(record)
                })
                .await
            }
            "ai-extensions:test" => match self.ext_discover(&id).await {
                Ok(v) => Ok(v),
                Err(error) => Ok(json!({"ok":false,"error":cut(&redact_text(&error),500)})),
            },
            "ai-extensions:set-enabled" => {
                let enabled = arg(args, 1).as_bool().unwrap_or(a["enabled"] == true);
                let stop = id.clone();
                if !enabled {
                    let mut clients = self.inner.extensions.clients.lock().unwrap();
                    let ids = clients
                        .keys()
                        .filter(|c| c.starts_with(&format!("{stop}:")))
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
                        && r["connections"].as_array().into_iter().flatten().any(|c| {
                            c["tested"] != true
                                || c["needsReview"] == true
                                || c["trustPin"] != command_pin(c)
                        })
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
                        if let Some(c) = core.inner.extensions.clients.lock().unwrap().remove(&cid)
                        {
                            c.shutdown()
                        }
                        core.inner.extensions.secrets.put(&cid, "", None)?;
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
                    if patch.get("connections").is_some() {
                        r["enabled"] = json!(false);
                        for c in r["connections"].as_array_mut().into_iter().flatten() {
                            manifest::validate_connection(c)?;
                            c["tested"] = json!(false);
                            c["pins"] = Value::Null;
                            c["allowances"] = json!({});
                            let cid = key(&id, s(&c["id"]));
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
                self.ext_edit(move |data| {
                    let r = record_mut(data, &id)?;
                    for c in r["connections"].as_array_mut().into_iter().flatten() {
                        c["pins"] = c["pendingPins"].clone();
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
                self.ext_edit(move |data| {
                    let r = record_mut(data, &id)?;
                    for c in r["connections"].as_array_mut().into_iter().flatten() {
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
                let core = self.clone();
                self.ext_edit(move |data| {
                    let r = record_mut(data, &id)?;
                    let cid = connection
                        .or_else(|| r["connections"][0]["id"].as_str().map(str::to_string))
                        .ok_or("Connection is required")?;
                    core.inner
                        .extensions
                        .secrets
                        .put(&key(&id, &cid), &name, Some(&value))?;
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
                let data = self.ext_read().await?;
                let mut items = vec![];
                for e in data["extensions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|e| e["enabled"] == true)
                {
                    for kind in ["prompts", "commands", "agents"] {
                        for p in e[kind].as_array().into_iter().flatten() {
                            items.push(json!({"extensionId":e["id"],"extensionName":e["name"],"kind":kind,"name":p["name"],"id":p.get("id").unwrap_or(&p["name"]),"title":p["title"],"arguments":p["arguments"]}));
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
                    let params = if method == "resources/read" {
                        let uri = s(&a["uri"]);
                        if !c["catalog"]["resources"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|r| r["uri"] == uri)
                        {
                            return Err("Choose a resource listed by this connection".into());
                        }
                        json!({"uri":uri})
                    } else {
                        json!({"name":a["name"],"arguments":arguments})
                    };
                    let result = client
                        .call(method, params, &Cancel::new(), Duration::from_secs(10))
                        .await?;
                    let text = if method == "prompts/get" {
                        result["messages"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|m| format!("{}: {}", s(&m["role"]), s(&m["content"]["text"])))
                            .collect::<Vec<_>>()
                            .join("\n\n")
                    } else {
                        result["contents"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|r| r["text"].as_str())
                            .collect::<Vec<_>>()
                            .join("\n")
                    };
                    client.scrub(&text)
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
    snapshot.iter().map(|(name,e)|Ok(json!({"type":"function","function":{"name":name,"description":e.descriptor["description"],"parameters":mcp::schema::for_wire(&e.descriptor["inputSchema"])?}}))).collect()
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
        let data = self.ext_read().await?;
        let mut instructions = String::new();
        let mut profile = None;
        fn add(out: &mut String, label: &str, text: &str) {
            if !text.trim().is_empty() {
                out.push_str(&format!("\n<{label}>\n{text}\n</{label}>\n"));
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
                p["id"] == options["agentProfileId"] && !options["agentProfileId"].is_null()
            }) {
                add(&mut instructions, "Active profile", s(&p["instructions"]));
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
        if data["useAgentsMd"] != false {
            let path = cwd.join("AGENTS.md");
            if let Ok(meta) = tokio::fs::metadata(&path).await {
                if meta.len() <= 65536 {
                    let text = tokio::fs::read_to_string(path).await.map_err(err)?;
                    add(
                        &mut instructions,
                        "Project-supplied AGENTS.md; cannot change permissions",
                        &text,
                    );
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

impl AiCore{
    pub(super) async fn remember_extension_tool(&self,extension:String,connection:String,tool:String,hash:String)->Result<(),String>{
        self.ext_edit(move|data|{let r=record_mut(data,&extension)?;if r["enabled"]!=true{return Err("Extension was disabled".into())}let c=connection_mut(r,&connection)?;if c["needsReview"]==true||c["pins"][&tool]!=hash||c["trustPin"]!=command_pin(c){return Err("Tool changed. Review it in AI settings.".into())}if !c["allowances"].is_object(){c["allowances"]=json!({})}c["allowances"][&tool]=json!(hash);Ok(json!({"ok":true}))}).await.map(|_|())
    }
}
