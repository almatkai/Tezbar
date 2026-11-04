//! Ported credential selection, refresh and envelope rules from pi-antigravity
//! 0.11.0, MIT © Rahul Arya. See licenses/pi-antigravity-MIT.txt.
use super::super::*;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
pub const USER_AGENT:&str="antigravity/cli/1.4.0 (aidev_client; os_type=linux; arch=amd64; cl=982146307; auth_method=consumer)";
const CLIENT_ID: &str = "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
const CLIENT_SECRET: &str = "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";
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
        let mut a = current.clone();
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
    pub(in crate::native_core::ai) async fn antigravity_account(&self) -> CoreResult {
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
                ("client_id", CLIENT_ID),
                ("client_secret", CLIENT_SECRET),
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
            let id = account_id(&next);
            store["accounts"][&id] = next.clone();
            store["activeAccountId"] = json!(id);
            auth["antigravity"] = next;
            save(&dir, &store, &auth)
        })
        .await
        .map_err(err)??;
        Ok(account)
    }
    pub(in crate::native_core::ai) async fn antigravity_accounts(&self) -> CoreResult {
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
                    f["parameters"] = schema;
                }
            }
        }
    }
    json!({"project":project,"model":model,"request":request,"requestType":"agent","userAgent":"antigravity","requestId":format!("agent/{conversation}/{}/{trajectory}/{step}",now_ms())})
}
