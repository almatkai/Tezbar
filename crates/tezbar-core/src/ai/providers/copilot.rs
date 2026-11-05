//! Copilot's unpublished session exchange follows MIT pi-ai's OAuth adapter
//! (© Mario Zechner, github.com/earendil-works/pi, packages/ai).
use super::super::*;
use sha2::{Digest, Sha256};
#[derive(Clone)]
pub struct Session {
    pub token: String,
    pub base: String,
    pub expires: u64,
}
fn hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}
impl AiCore {
    pub(in crate::ai) async fn clear_copilot_session(&self, github: &str) {
        self.inner
            .copilot_sessions
            .lock()
            .await
            .remove(&hash(github));
    }
    pub(in crate::ai) async fn cached_copilot_session(
        &self,
        github: &str,
    ) -> Option<Session> {
        self.inner
            .copilot_sessions
            .lock()
            .await
            .get(&hash(github))
            .filter(|s| now_ms() < s.expires.saturating_sub(300000))
            .cloned()
    }
    pub(in crate::ai) async fn exchange_copilot_session(
        &self,
        github: &str,
    ) -> Result<Session, String> {
        if github.trim().is_empty() {
            return Err("Sign in to Copilot in AI settings.".into());
        }
        let mut sessions = self.inner.copilot_sessions.lock().await;
        let key = hash(github);
        if let Some(s) = sessions
            .get(&key)
            .filter(|s| now_ms() < s.expires.saturating_sub(300000))
        {
            return Ok(s.clone());
        }
        let response = self
            .inner
            .http
            .get(self.endpoint(
                "copilot-session",
                "https://api.github.com/copilot_internal/v2/token",
            ))
            .header("User-Agent", "GitHubCopilotChat/0.35.0")
            .header("Editor-Version", "vscode/1.107.0")
            .header("Editor-Plugin-Version", "copilot-chat/0.35.0")
            .bearer_auth(github)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|_| "Copilot session exchange failed: network")?;
        if !response.status().is_success() {
            return Err(format!(
                "Copilot sign-in failed ({}). Check your Copilot access in GitHub settings.",
                response.status().as_u16()
            ));
        }
        let v: Value = response
            .json()
            .await
            .map_err(|_| "Invalid Copilot session response")?;
        let token = v["token"]
            .as_str()
            .filter(|t| !t.is_empty())
            .ok_or("Copilot session response has no token")?;
        let proxy = token.split(';').find_map(|p| p.strip_prefix("proxy-ep="));
        let mut base = "https://api.individual.githubcopilot.com".to_string();
        if let Some(proxy) = proxy {
            let host = proxy
                .strip_prefix("proxy.")
                .ok_or("Copilot returned an invalid proxy host")?;
            if !host.ends_with(".githubcopilot.com") {
                return Err("Copilot returned an untrusted proxy host".into());
            }
            base = format!("https://api.{host}");
        }
        #[cfg(test)]
        if let Some(url) = self.inner.endpoints.lock().unwrap().get("copilot-proxy") {
            base = url.clone();
        }
        let session = Session {
            token: token.into(),
            base,
            expires: v["expires_at"]
                .as_u64()
                .ok_or("Copilot session has no expiry")?
                * 1000,
        };
        if sessions.len() > 16 {
            sessions.clear()
        }
        sessions.insert(key, session.clone());
        Ok(session)
    }
}

impl AiCore {
    pub(in crate::ai) async fn copilot_models(&self) -> CoreResult {
        #[cfg(test)]
        if !self
            .inner
            .endpoints
            .lock()
            .unwrap()
            .contains_key("copilot-models")
        {
            return Ok(json!([]));
        }
        let token = self.copilot_token().await?;
        if token.trim().is_empty() {
            return Err("Sign in to Copilot in AI settings.".into());
        }
        let catalog_key = format!("copilot:{}", hash(&token));
        if let Some((_, value)) = self
            .inner
            .provider_catalogs
            .lock()
            .unwrap()
            .get(&catalog_key)
            .filter(|(at, _)| now_ms().saturating_sub(*at) < 300_000)
        {
            return Ok(value.clone());
        }
        let session = self.cached_copilot_session(&token).await;
        let base = session
            .as_ref()
            .map(|s| s.base.as_str())
            .unwrap_or("https://api.individual.githubcopilot.com");
        let key = session.as_ref().map(|s| s.token.as_str()).unwrap_or(&token);
        let fetch = |base: &str, key: &str| {
            self.inner
                .http
                .get(self.endpoint("copilot-models", &format!("{base}/models")))
                .bearer_auth(key)
                .header("User-Agent", "GitHubCopilotChat/0.35.0")
                .header("Editor-Version", "vscode/1.107.0")
                .header("Editor-Plugin-Version", "copilot-chat/0.35.0")
                .header("Copilot-Integration-Id", "vscode-chat")
                .timeout(Duration::from_secs(8))
                .send()
        };
        let mut response = fetch(base, key)
            .await
            .map_err(|_| "Copilot model catalog: network failed")?;
        if matches!(response.status().as_u16(), 401 | 403) {
            self.clear_copilot_session(&token).await;
            let session = self.exchange_copilot_session(&token).await?;
            response = fetch(&session.base, &session.token)
                .await
                .map_err(|_| "Copilot model catalog: network failed")?;
        }
        if !response.status().is_success() {
            return Err(format!(
                "Copilot model catalog: HTTP {}",
                response.status().as_u16()
            ));
        }
        let value = oauth::bounded_json(response).await?;
        let models = value["data"]
            .as_array()
            .ok_or("Copilot catalog has no model list")?
            .iter()
            .filter(|m| {
                m["id"].is_string()
                    && m["capabilities"]["type"]
                        .as_str()
                        .is_none_or(|t| t == "chat")
                    && !matches!(
                        s(&m["capabilities"]["family"]),
                        "search-agent" | "exec-agent" | "trajectory-compaction"
                    )
                    && !regex::Regex::new("(?i)(^|[-_/])(exec-)?agent[-_]?[a-z0-9]*$")
                        .unwrap()
                        .is_match(s(&m["id"]))
            })
            .take(512)
            .cloned()
            .collect::<Vec<_>>();
        let models = json!(models);
        self.inner
            .provider_catalogs
            .lock()
            .unwrap()
            .insert(catalog_key, (now_ms(), models.clone()));
        Ok(models)
    }
}
pub fn model_protocol(model: &Value) -> Option<runtime::transcript::Protocol> {
    use runtime::transcript::Protocol;
    let endpoints = model["supported_endpoints"]
        .as_array()
        .or_else(|| model["capabilities"]["supported_endpoints"].as_array())?;
    for (endpoint, protocol) in [
        ("/v1/messages", Protocol::Anthropic),
        ("/responses", Protocol::OpenaiResponses),
        ("/chat/completions", Protocol::OpenaiChat),
    ] {
        if endpoints.iter().any(|e| {
            e.as_str().is_some_and(|e| {
                e == endpoint
                    || e.trim_start_matches('/').trim_start_matches("v1/")
                        == endpoint.trim_start_matches('/').trim_start_matches("v1/")
            })
        }) {
            return Some(protocol);
        }
    }
    None
}
