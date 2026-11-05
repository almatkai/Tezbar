//! Ported credential selection, refresh and envelope rules from pi-antigravity
//! 0.11.0, MIT © Rahul Arya. See licenses/pi-antigravity-MIT.txt.
use super::super::*;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
pub const USER_AGENT:&str="antigravity/cli/1.4.0 (aidev_client; os_type=linux; arch=amd64; cl=982146307; auth_method=consumer)";
const CLIENT_ID: &str = "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
const CLIENT_SECRET: &str = "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";
#[cfg(test)]
pub fn env(_name: &str) -> Option<String> {
    None
}
#[cfg(not(test))]
pub fn env(name: &str) -> Option<String> {
    std::env::var(format!("ANTIGRAVITY_{name}"))
        .or_else(|_| std::env::var(format!("NOAGY_{name}")))
        .ok()
        .filter(|v| !v.trim().is_empty())
}
fn client_id() -> String {
    env("CLIENT_ID").unwrap_or_else(|| CLIENT_ID.into())
}
fn client_secret() -> String {
    env("CLIENT_SECRET").unwrap_or_else(|| CLIENT_SECRET.into())
}
pub fn thinking(model: &str, effort: &str) -> Option<Value> {
    let off = effort.is_empty() || effort == "off";
    let high = matches!(effort, "high" | "xhigh");
    if (model.contains("argon") || model.starts_with("gemini-4")) && off {
        return None;
    }
    let budget = if off {
        0
    } else if model.starts_with("claude-") {
        1024
    } else if model.starts_with("gpt-oss-") {
        8192
    } else if model.starts_with("gemini-3.5-flash") || model == "gemini-3-flash-agent" {
        if high {
            10000
        } else if effort == "medium" {
            4000
        } else {
            1000
        }
    } else if model.starts_with("gemini-3.1-pro") || model == "gemini-pro-agent" {
        if high {
            10001
        } else {
            1001
        }
    } else if model.starts_with("gemini-") {
        if high {
            -1
        } else if effort == "medium" {
            4000
        } else {
            1000
        }
    } else {
        return None;
    };
    Some(json!({"includeThoughts":!off,"thinkingBudget":budget}))
}
pub fn fallback_model(model: &str, effort: &str) -> Option<String> {
    if model == "gemini-3.8-flash" {
        Some("gemini-3.7-flash-low".into())
    } else if model.starts_with("gemini-3.8-flash-") {
        Some(model.replacen("gemini-3.8-flash-", "gemini-3.7-flash-", 1))
    } else if model == "gemini-3.7-flash-tiered" {
        Some(runtime_model("gemini-3.6-flash", effort))
    } else if model == "gemini-3.7-flash" {
        Some("gemini-3.6-flash-low".into())
    } else if model.starts_with("gemini-3.7-flash-") {
        Some(model.replacen("gemini-3.7-flash-", "gemini-3.6-flash-", 1))
    } else {
        None
    }
}
pub fn quota_wall(text: &str) -> bool {
    regex::Regex::new("(?i)Individual quota reached|Resets? in ")
        .unwrap()
        .is_match(text)
        || (!regex::Regex::new("(?i)rate.?limit").unwrap().is_match(text)
            && regex::Regex::new("(?i)quota exceeded|exceeded your|daily limit")
                .unwrap()
                .is_match(text))
}
fn legacy_schema(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                match k.as_str() {
                    "type" => {
                        let t = v.as_str().map(str::to_string).or_else(|| {
                            v.as_array().and_then(|a| {
                                a.iter()
                                    .filter_map(Value::as_str)
                                    .find(|s| *s != "null")
                                    .map(str::to_string)
                            })
                        });
                        if let Some(t) = t {
                            out.insert(k.clone(), json!(t));
                        }
                    }
                    "properties" => {
                        let props = v
                            .as_object()
                            .into_iter()
                            .flatten()
                            .map(|(k, v)| (k.clone(), legacy_schema(v)))
                            .collect();
                        out.insert(k.clone(), Value::Object(props));
                    }
                    "enum" if v.as_array().is_some_and(|a| a.iter().all(Value::is_string)) => {
                        out.insert(k.clone(), v.clone());
                    }
                    "description" | "required" | "items" => {
                        out.insert(k.clone(), legacy_schema(v));
                    }
                    _ => {}
                }
            }
            Value::Object(out)
        }
        Value::Array(a) => json!(a.iter().map(legacy_schema).collect::<Vec<_>>()),
        _ => value.clone(),
    }
}
pub fn stable_id(seed: &str) -> String {
    use sha1::Sha1;
    let mut bytes = Sha1::digest(format!("antigravity:{seed}").as_bytes());
    bytes[6] = (bytes[6] & 15) | 80;
    bytes[8] = (bytes[8] & 63) | 128;
    let h = format!("{:x}", bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}
struct Lock(PathBuf);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn lock(dir: &Path) -> Result<Lock, String> {
    use std::fs::OpenOptions;
    let path = dir.join("antigravity-accounts.lock");
    std::fs::create_dir_all(dir).map_err(err)?;
    let started = std::time::Instant::now();
    loop {
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(Lock(path)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if path
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|m| m.elapsed().ok())
                    .is_some_and(|s| s > Duration::from_secs(60))
                {
                    std::fs::remove_file(&path).map_err(err)?;
                    continue;
                }
                if started.elapsed() > Duration::from_secs(5) {
                    return Err("Antigravity accounts are locked by another writer".into());
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => return Err(err(e)),
        }
    }
}
fn object(path: &Path, default: Value) -> CoreResult {
    if !path.exists() {
        return Ok(default);
    }
    let v = common::read_json(path)?;
    if !v.is_object() {
        return Err("Antigravity account store must be an object; no file was overwritten".into());
    }
    Ok(v)
}
fn stores(dir: &Path) -> Result<(Value, Value), String> {
    Ok((
        object(
            &dir.join("antigravity-accounts.json"),
            json!({"version":1,"accounts":{}}),
        )?,
        object(&dir.join("auth.json"), json!({}))?,
    ))
}
fn account_id(v: &Value) -> String {
    let email = s(&v["email"]).trim().to_lowercase();
    if !email.is_empty() {
        email
    } else {
        format!("account-{:x}", Sha256::digest(s(&v["refresh"]).as_bytes()))[..24].into()
    }
}
fn selected(store: &mut Value, auth: &Value) -> Result<Value, String> {
    if !store["accounts"].is_object() {
        return Err("Antigravity accounts are malformed".into());
    }
    let current = &auth["antigravity"];
    if !s(&current["refresh"]).is_empty() {
        let id = store["accounts"]
            .as_object()
            .unwrap()
            .iter()
            .find(|(_, a)| {
                a["refresh"] == current["refresh"]
                    || !s(&current["email"]).is_empty()
                        && s(&a["email"]).eq_ignore_ascii_case(s(&current["email"]))
            })
            .map(|(k, _)| k.clone())
            .unwrap_or_else(|| account_id(current));
        let mut a = store["accounts"][&id]
            .as_object()
            .cloned()
            .map(Value::Object)
            .unwrap_or(json!({}));
        for (k, v) in current
            .as_object()
            .ok_or("Antigravity auth entry is malformed")?
        {
            a[k] = v.clone();
        }
        a["accountId"] = json!(id);
        store["accounts"][&id] = a;
        store["activeAccountId"] = json!(id);
    }
    let active = s(&store["activeAccountId"]);
    let a = store["accounts"]
        .get(active)
        .or_else(|| store["accounts"].as_object().unwrap().values().next())
        .ok_or("Native auth: Sign in to Antigravity in AI settings")?;
    if s(&a["refresh"]).is_empty() {
        return Err(
            "Native auth: Antigravity account has no refresh credential; sign in again".into(),
        );
    }
    Ok(a.clone())
}
fn save(dir: &Path, store: &Value, auth: &Value) -> Result<(), String> {
    common::write_json_atomic(&dir.join("antigravity-accounts.json"), store)?;
    common::write_json_atomic(&dir.join("auth.json"), auth)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for p in ["antigravity-accounts.json", "auth.json"] {
            std::fs::set_permissions(dir.join(p), std::fs::Permissions::from_mode(0o600))
                .map_err(err)?;
        }
    }
    Ok(())
}
impl AiCore {
    pub(in crate::ai) async fn antigravity_account(&self) -> CoreResult {
        let _refresh = self.inner.token_refresh.lock().await;
        let dir = self.pi_dir();
        let d = dir.clone();
        let mut account = tokio::task::spawn_blocking(move || {
            let _lock = lock(&d)?;
            let (mut store, auth) = stores(&d)?;
            selected(&mut store, &auth)
        })
        .await
        .map_err(err)??;
        if s(&account["projectId"]).is_empty() {
            account["projectId"] = json!(stable_id(if s(&account["email"]).is_empty() {
                "antigravity-default"
            } else {
                s(&account["email"])
            }));
        }
        if !s(&account["access"]).is_empty()
            && account["expires"]
                .as_u64()
                .is_some_and(|at| now_ms() < at.saturating_sub(300000))
        {
            return Ok(account);
        }
        let previous = account.clone();
        let response = self
            .inner
            .http
            .post(self.endpoint("google-token", "https://oauth2.googleapis.com/token"))
            .form(&[
                ("client_id", client_id().as_str()),
                ("client_secret", client_secret().as_str()),
                ("grant_type", "refresh_token"),
                ("refresh_token", s(&account["refresh"])),
            ])
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|_| "Antigravity token refresh: network failed")?;
        if !response.status().is_success() {
            return Err(format!(
                "Antigravity token refresh failed ({}). Sign in again in AI settings.",
                response.status().as_u16()
            ));
        }
        let token: Value = response
            .json()
            .await
            .map_err(|_| "Antigravity token refresh: invalid response")?;
        let access = token["access_token"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or("Antigravity token refresh: missing access token")?;
        account["access"] = json!(access);
        account["expires"] = json!(now_ms() + token["expires_in"].as_u64().unwrap_or(3600) * 1000);
        if token["refresh_token"].is_string() {
            account["refresh"] = token["refresh_token"].clone()
        }
        if s(&account["projectId"]).is_empty() {
            account["projectId"] = json!(stable_id(if s(&account["email"]).is_empty() {
                "antigravity-default"
            } else {
                s(&account["email"])
            }));
        }
        let next = account.clone();
        tokio::task::spawn_blocking(move || {
            let _lock = lock(&dir)?;
            let (mut store, mut auth) = stores(&dir)?;
            let current = selected(&mut store, &auth)?;
            if current["refresh"] != previous["refresh"] {
                return Err(
                    "Antigravity account changed during refresh; retry the request".to_string(),
                );
            }
            let id = next["accountId"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| account_id(&next));
            store["accounts"][&id] = next.clone();
            store["activeAccountId"] = json!(id);
            auth["antigravity"] = next;
            save(&dir, &store, &auth)
        })
        .await
        .map_err(err)??;
        Ok(account)
    }
    pub(in crate::ai) async fn antigravity_accounts(&self) -> CoreResult {
        let dir = self.pi_dir();
        tokio::task::spawn_blocking(move||{let(mut store,auth)=stores(&dir)?;let _=selected(&mut store,&auth);let active=s(&store["activeAccountId"]);Ok(json!(store["accounts"].as_object().into_iter().flatten().map(|(id,a)|json!({"accountId":id,"email":a["email"],"active":id==active,"lastUsedAt":a["lastUsedAt"]})).collect::<Vec<_>>()))}).await.map_err(err)?
    }
}
pub fn runtime_model(model: &str, effort: &str) -> String {
    let catalog: Value =
        serde_json::from_str(include_str!("antigravity_routing.json")).expect("static routing");
    let m = &catalog[model];
    m["routing"][effort]
        .as_str()
        .or_else(|| {
            m[if effort == "off" {
                "off"
            } else {
                "defaultRequestId"
            }]
            .as_str()
        })
        .or_else(|| m["defaultRequestId"].as_str())
        .unwrap_or(model)
        .into()
}
pub fn envelope(mut request: Value, cfg: &Value, project: &str) -> Value {
    let session = cfg["sessionId"].as_str().unwrap_or("native");
    let trajectory = stable_id(session);
    let conversation = stable_id(&format!("conversation:{session}"));
    let step = request["contents"]
        .as_array()
        .map(Vec::len)
        .unwrap_or(1)
        .max(1);
    let model = runtime_model(
        s(&cfg["model"]),
        cfg["reasoningEffort"].as_str().unwrap_or("off"),
    );
    if let Some(config) = thinking(
        &model,
        cfg["reasoningEffort"]
            .as_str()
            .unwrap_or(if cfg["thinking"] == true {
                "medium"
            } else {
                "off"
            }),
    ) {
        request["generationConfig"]["thinkingConfig"] = config;
    }
    let metadata: Value = serde_json::from_str(include_str!("antigravity_metadata.json"))
        .expect("static Antigravity metadata");
    if let Some(max) = metadata["maxTokens"][&model].as_u64() {
        let requested = request["generationConfig"]["maxOutputTokens"]
            .as_u64()
            .unwrap_or(max);
        request["generationConfig"]["maxOutputTokens"] = json!(requested.min(max));
    }
    let claude = model.starts_with("claude-");
    let non_gemini = !model.starts_with("gemini-");
    request["sessionId"] = json!(format!(
        "{}",
        u64::from_str_radix(
            &format!("{:x}", Sha256::digest(session.as_bytes()))[..15],
            16
        )
        .unwrap()
    ));
    request["systemInstruction"]["role"] = json!("user");
    request["labels"] = json!({"last_step_index":(step-1).to_string(),"request_id":format!("{trajectory}-{}",step-1),"trajectory_id":trajectory,"used_claude":claude.to_string(),"used_claude_conservative":claude.to_string(),"used_non_gemini_model":non_gemini.to_string()});
    if let Some(value) = metadata["enums"][&model].as_str() {
        request["labels"]["model_enum"] = json!(value);
    }
    request["labels"]["request_id"] = json!(format!(
        "{trajectory}-{}",
        cfg["_requestIndex"].as_u64().unwrap_or(0)
    ));
    if step > 1 {
        request["labels"]["last_execution_id"] =
            json!(stable_id(&format!("exec:{trajectory}:{}", step - 1)));
    }
    if non_gemini {
        for tool in request["tools"].as_array_mut().into_iter().flatten() {
            for f in tool["functionDeclarations"]
                .as_array_mut()
                .into_iter()
                .flatten()
            {
                if let Some(schema) = f
                    .as_object_mut()
                    .and_then(|f| f.remove("parametersJsonSchema"))
                {
                    f["parameters"] = legacy_schema(&schema);
                }
            }
        }
    }
    json!({"project":project,"model":model,"request":request,"requestType":"agent","userAgent":"antigravity","requestId":format!("agent/{conversation}/{}/{trajectory}/{step}",now_ms())})
}
impl AiCore {
    pub(in crate::ai) async fn antigravity_sign_in(&self) -> CoreResult {
        self.oauth_start_native(oauth::Request {
            target: "antigravity".into(),
            issuer: "https://accounts.google.com".into(),
            authorize: self.endpoint(
                "google-authorize",
                "https://accounts.google.com/o/oauth2/v2/auth",
            ),
            token: self.endpoint("google-token", "https://oauth2.googleapis.com/token"),
            client_id: client_id(),
            client_secret: Some(client_secret()),
            resource: None,
            resource_policy: None,
            scopes: [
                "https://www.googleapis.com/auth/aicode",
                "https://www.googleapis.com/auth/cloud-platform",
                "https://www.googleapis.com/auth/userinfo.email",
                "https://www.googleapis.com/auth/userinfo.profile",
                "https://www.googleapis.com/auth/cclog",
                "https://www.googleapis.com/auth/experimentsandconfigs",
            ]
            .join(" "),
            port: 51121,
            callback_path: "/oauth-callback".into(),
            metadata: json!({}),
        })
        .await
    }
    pub(in crate::ai) async fn persist_antigravity_grant(
        &self,
        token: Value,
        cancel: &Cancel,
    ) -> Result<(), String> {
        let access = s(&token["access_token"]);
        let refresh = token["refresh_token"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or(
            "Google didn't return a refresh token. Start sign-in again and grant offline access.",
        )?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(err)?;
        let user = http
            .get(self.endpoint(
                "google-userinfo",
                "https://www.googleapis.com/oauth2/v1/userinfo?alt=json",
            ))
            .bearer_auth(access)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|_| "Couldn't read the Google account identity")?;
        if !user.status().is_success() {
            return Err("Couldn't read the Google account identity".into());
        }
        let user = oauth::bounded_json(user).await?;
        let email = user["email"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or("Google didn't return an account email")?;
        let mut project = None;
        for base in [
            "https://daily-cloudcode-pa.googleapis.com",
            "https://daily-cloudcode-pa.sandbox.googleapis.com",
            "https://cloudcode-pa.googleapis.com",
        ] {
            let base = self.endpoint("antigravity", base);
            let response = http
                .post(format!("{base}/v1internal:loadCodeAssist"))
                .header("User-Agent", USER_AGENT)
                .bearer_auth(access)
                .json(&json!({"metadata":{"ideType":"ANTIGRAVITY"}}))
                .timeout(Duration::from_secs(8))
                .send()
                .await;
            if let Ok(response) = response {
                if response.status().is_success() {
                    let v = oauth::bounded_json(response).await?;
                    project = [
                        "antigravityProjectId",
                        "projectId",
                        "backendProjectId",
                        "userDefinedCloudaicompanionProject",
                        "cloudaicompanionProject",
                        "project",
                    ]
                    .iter()
                    .find_map(|k| {
                        v[k].as_str()
                            .or_else(|| v[k]["id"].as_str())
                            .map(str::to_string)
                    });
                    if project.is_some() {
                        break;
                    }
                }
            }
        }
        let account = json!({"type":"oauth","email":email,"accountId":email.to_lowercase(),"access":access,"refresh":refresh,"expires":token["expiresAt"],"projectId":project.unwrap_or_else(||stable_id(email)),"addedAt":now_ms(),"lastUsedAt":now_ms()});
        let dir = self.pi_dir();
        let cancel = cancel.clone();
        tokio::task::spawn_blocking(move || {
            let _lock = lock(&dir)?;
            let (mut store, mut auth) = stores(&dir)?;
            let _ = selected(&mut store, &auth);
            if cancel.is_cancelled() {
                return Err("Sign-in was cancelled".into());
            }
            let id = account_id(&account);
            store["accounts"][&id] = account.clone();
            store["activeAccountId"] = json!(id);
            auth["antigravity"] = account;
            save(&dir, &store, &auth)
        })
        .await
        .map_err(err)?
    }
    pub(in crate::ai) async fn antigravity_select_account(
        &self,
        selector: &str,
        remove: bool,
    ) -> CoreResult {
        let dir = self.pi_dir();
        let selector = selector.to_string();
        tokio::task::spawn_blocking(move || {
            let _lock = lock(&dir)?;
            let (mut store, mut auth) = stores(&dir)?;
            let _ = selected(&mut store, &auth);
            let account = store["accounts"]
                .get(&selector)
                .ok_or("Antigravity account was not found")?
                .clone();
            if remove {
                store["accounts"].as_object_mut().unwrap().remove(&selector);
                if store["activeAccountId"] == selector {
                    if let Some((id, a)) = store["accounts"]
                        .as_object()
                        .unwrap()
                        .iter()
                        .next()
                        .map(|(k, v)| (k.clone(), v.clone()))
                    {
                        store["activeAccountId"] = json!(id);
                        auth["antigravity"] = a;
                    } else {
                        store["activeAccountId"] = Value::Null;
                        auth.as_object_mut().unwrap().remove("antigravity");
                    }
                }
            } else {
                store["activeAccountId"] = json!(selector);
                auth["antigravity"] = account;
            }
            save(&dir, &store, &auth)?;
            Ok(json!({"ok":true}))
        })
        .await
        .map_err(err)?
    }
}
impl AiCore {
    pub(in crate::ai) async fn antigravity_catalog(&self) -> CoreResult {
        let path = self
            .inner
            .ctx
            .paths
            .data_dir
            .join("llm/antigravity-models.json");
        let cache_path = path.clone();
        let cached = tokio::task::spawn_blocking(move || common::read_json(&cache_path))
            .await
            .map_err(err)?
            .unwrap_or(Value::Null);
        if cached["fetchedAt"]
            .as_u64()
            .is_some_and(|at| now_ms().saturating_sub(at) < 4 * 3600000)
        {
            if let Some(models) = cached["models"].as_object() {
                return Ok(json!(models.keys().cloned().collect::<Vec<_>>()));
            }
        }
        let mut models = serde_json::Map::new();
        if let Ok(account) = self.antigravity_account().await {
            let cfg = self.cfg(None, Some("antigravity")).await?;
            for base in self.antigravity_bases(&cfg)? {
                let response = self
                    .inner
                    .http
                    .post(format!("{base}/v1internal:fetchAvailableModels"))
                    .bearer_auth(s(&account["access"]))
                    .header("User-Agent", USER_AGENT)
                    .json(&json!({"project":account["projectId"]}))
                    .timeout(Duration::from_secs(8))
                    .send()
                    .await;
                if let Ok(response) = response {
                    if response.status().is_success() {
                        if let Ok(value) = oauth::bounded_json(response).await {
                            for (id, row) in value["models"].as_object().into_iter().flatten() {
                                if id.len() <= 160
                                    && row.is_object()
                                    && !id.contains(s(&account["access"]))
                                {
                                    models.insert(id.clone(), row.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        if !models.is_empty() {
            let value = json!({"fetchedAt":now_ms(),"models":models});
            let ids = value["models"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            tokio::task::spawn_blocking(move || common::write_json_atomic(&path, &value))
                .await
                .map_err(err)??;
            return Ok(json!(ids));
        }
        if let Some(models) = cached["models"].as_object() {
            return Ok(json!(models.keys().cloned().collect::<Vec<_>>()));
        }
        let seed = self.pi_dir().join("models-store.json");
        let seed = tokio::task::spawn_blocking(move || common::read_json(&seed))
            .await
            .map_err(err)?
            .unwrap_or(Value::Null);
        let mut ids = seed["antigravity"]["models"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m["id"].as_str())
            .map(str::to_string)
            .collect::<Vec<_>>();
        if ids.is_empty() {
            ids = config::defaults()["DEFAULT_PROVIDER_MODELS"]["antigravity"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|m| m["id"].as_str().map(str::to_string))
                .collect();
        }
        ids.sort();
        ids.dedup();
        Ok(json!(ids))
    }
}

impl AiCore {
    pub(in crate::ai) fn antigravity_bases(
        &self,
        cfg: &Value,
    ) -> Result<Vec<String>, String> {
        let mut bases = if cfg["_explicitBaseURL"] == true {
            vec![s(&cfg["baseURL"]).to_string()]
        } else if let Some(base) = env("BASE_URL") {
            vec![base]
        } else {
            vec![
                "https://daily-cloudcode-pa.googleapis.com".into(),
                "https://daily-cloudcode-pa.sandbox.googleapis.com".into(),
                "https://cloudcode-pa.googleapis.com".into(),
            ]
        };
        #[cfg(test)]
        {
            let endpoints = self.inner.endpoints.lock().unwrap();
            if let Some(base) = endpoints.get("antigravity") {
                bases = vec![base.clone()];
                for key in ["antigravity-fallback-1", "antigravity-fallback-2"] {
                    if let Some(base) = endpoints.get(key) {
                        bases.push(base.clone())
                    }
                }
            }
        }
        for base in &bases {
            let u = reqwest::Url::parse(base).map_err(|_| "Invalid Antigravity base URL")?;
            if !matches!(u.scheme(), "http" | "https")
                || u.password().is_some()
                || !u.username().is_empty()
            {
                return Err(
                    "Antigravity base URL must use http or https without credentials".into(),
                );
            }
        }
        Ok(bases
            .into_iter()
            .map(|b| b.trim_end_matches('/').into())
            .collect())
    }
    pub(in crate::ai) async fn antigravity_next_account(
        &self,
        tried: &std::collections::HashSet<String>,
    ) -> CoreResult {
        let accounts = self.antigravity_accounts().await?;
        let next=accounts.as_array().into_iter().flatten().find(|a|!tried.contains(s(&a["accountId"]))&&!a["active"].as_bool().unwrap_or(false)).ok_or("Antigravity quota reached on all linked accounts. Try later or add an account in AI settings.")?;
        self.antigravity_select_account(s(&next["accountId"]), false)
            .await?;
        self.antigravity_account().await
    }
}
