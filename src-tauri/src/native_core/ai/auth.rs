use super::*;
const CLIENT: &str = "Iv1.b507a08c87ecfe98";
impl AiCore {
    async fn oauth(&self, path: &str, params: &[(&str, &str)]) -> CoreResult {
        let response = self
            .inner
            .http
            .post(format!("https://github.com/login/{path}"))
            .header("Accept", "application/json")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(
                params
                    .iter()
                    .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
                    .collect::<Vec<_>>()
                    .join("&"),
            )
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(err)?;
        let status = response.status();
        let value = response.json::<Value>().await.map_err(err)?;
        if !status.is_success() {
            return Err(format!("GitHub OAuth failed ({status}): {value}"));
        }
        Ok(value)
    }
    pub(super) async fn device_start(&self, client: Option<&str>) -> CoreResult {
        let client = client
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .unwrap_or(CLIENT);
        let generation = {
            let mut state = self.inner.state.lock().unwrap();
            state.device_generation += 1;
            state.device = None;
            state.device_generation
        };
        let v = self
            .oauth(
                "device/code",
                &[("client_id", client), ("scope", "repo workflow")],
            )
            .await?;
        for k in ["device_code", "user_code", "verification_uri"] {
            if s(&v[k]).is_empty() {
                return Err("GitHub device code: malformed response".into());
            }
        }
        let mut state = self.inner.state.lock().unwrap();
        if state.device_generation == generation {
            state.device = Some(
                json!({"device_code":v["device_code"],"client_id":client,"generation":generation}),
            );
        }
        Ok(v)
    }
    pub(super) async fn device_poll(&self) -> CoreResult {
        let session = self.inner.state.lock().unwrap().device.clone();
        let Some(session) = session else {
            return Ok(json!({"status":"error","error":"No device session. Start sign-in again."}));
        };
        let v = self
            .oauth(
                "oauth/access_token",
                &[
                    ("client_id", s(&session["client_id"])),
                    ("device_code", s(&session["device_code"])),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ],
            )
            .await?;
        {
            let state = self.inner.state.lock().unwrap();
            if state.device.as_ref() != Some(&session) {
                return Ok(json!({"status":"error","error":"Device session was cancelled"}));
            }
        }
        let error = s(&v["error"]);
        if matches!(error, "authorization_pending" | "slow_down") {
            return Ok(json!({"status":error}));
        }
        if !error.is_empty() {
            self.inner.state.lock().unwrap().device = None;
            return Ok(
                json!({"status":"error","error":v["error_description"].as_str().unwrap_or(error)}),
            );
        }
        if s(&v["access_token"]).is_empty() {
            return Ok(json!({"status":"error","error":"No access_token in response"}));
        }
        // Serialize the session validity check and durable update on the blocking pool:
        // cancellation can win before this transition, but cannot resurrect a stale session.
        let core = self.clone();
        let token = v.clone();
        let expected = session.clone();
        tokio::task::spawn_blocking(move || {
            let mut state = core.inner.state.lock().unwrap();
            if state.device.as_ref() != Some(&expected) {
                return Err("Device session was cancelled".to_string());
            }
            state.device = None;
            drop(state);
            core.persist_tokens(&token, s(&expected["client_id"]))
        })
        .await
        .map_err(err)??;
        let mut result = v;
        result["status"] = json!("success");
        result["client_id"] = session["client_id"].clone();
        Ok(result)
    }
    fn persist_tokens(&self, token: &Value, client: &str) -> Result<(), String> {
        self.inner.ctx.config.update(|raw| {
            let mut entry = raw["providerConfigs"]["copilot"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            for (source, target) in [
                ("access_token", "copilotGithubToken"),
                ("refresh_token", "copilotRefreshToken"),
            ] {
                if let Some(v) = token[source].as_str() {
                    raw[target] = json!(v);
                    entry.insert(target.into(), json!(v));
                }
            }
            if let Some(sec) = token["expires_in"].as_u64() {
                let expires = now_ms() + sec * 1000;
                raw["copilotExpiresAt"] = json!(expires);
                entry.insert("copilotExpiresAt".into(), json!(expires));
            }
            if !client.is_empty() {
                raw["githubOAuthClientId"] = json!(client);
                entry.insert("githubOAuthClientId".into(), json!(client));
            }
            if !raw["providerConfigs"].is_object() {
                raw["providerConfigs"] = json!({});
            }
            raw["providerConfigs"]["copilot"] = json!(entry);
            Ok(())
        })?;
        Ok(())
    }
    pub(super) async fn copilot_token(&self) -> Result<String, String> {
        let _guard = self.inner.token_refresh.lock().await;
        let raw = self.read_config().await?;
        let cfg = config::selected(&raw, None, Some("copilot"));
        let access = cfg["copilotGithubToken"]
            .as_str()
            .or_else(|| raw["copilotGithubToken"].as_str())
            .unwrap_or(s(&raw["apiKey"]));
        let refresh = s(&raw["copilotRefreshToken"]);
        if refresh.is_empty()
            || raw["copilotExpiresAt"]
                .as_u64()
                .map_or(true, |at| now_ms() < at.saturating_sub(120000))
        {
            return Ok(access.into());
        }
        let client = raw["githubOAuthClientId"]
            .as_str()
            .filter(|c| !c.is_empty())
            .unwrap_or(CLIENT);
        let token = self
            .oauth(
                "oauth/access_token",
                &[
                    ("client_id", client),
                    ("grant_type", "refresh_token"),
                    ("refresh_token", refresh),
                ],
            )
            .await?;
        let access = token["access_token"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Token refresh: missing access_token")?
            .to_string();
        let core = self.clone();
        let client = client.to_string();
        tokio::task::spawn_blocking(move || core.persist_tokens(&token, &client))
            .await
            .map_err(err)??;
        Ok(access)
    }
}
fn url_encode(v: &str) -> String {
    v.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
