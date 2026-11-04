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
    pub(in crate::native_core::ai) async fn cached_copilot_session(
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
    pub(in crate::native_core::ai) async fn exchange_copilot_session(
        &self,
        github: &str,
    ) -> Result<Session, String> {
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
