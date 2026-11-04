//! MCP authorization discovery and native-client registration. No hosted CIMD
//! document is invented. Pre-registered clients take precedence over DCR.
use super::super::super::*;
use super::super::{key, record_mut};
use oauth::{bounded_json, secure_url, Request};
impl AiCore {
    pub(in crate::native_core::ai) async fn mcp_oauth_start(
        &self,
        extension: &str,
        connection: Option<&str>,
    ) -> CoreResult {
        let data = self.ext_read().await?;
        let record = data["extensions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == extension)
            .ok_or("Connection was removed")?;
        let c = record["connections"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|c| connection.map_or(true, |id| c["id"] == id))
            .ok_or("Connection was removed")?;
        if c["transport"] != "http" {
            return Err("Browser sign-in is only available for web connections".into());
        }
        let resource = secure_url(s(&c["url"]))?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(err)?;
        let probe = http
            .get(resource.clone())
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|_| "Couldn't discover the connection's sign-in settings")?;
        let challenge = probe
            .headers()
            .get(reqwest::header::WWW_AUTHENTICATE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let advertised = regex::Regex::new(r#"(?i)resource_metadata\s*=\s*"([^"]+)""#)
            .unwrap()
            .captures(challenge)
            .map(|c| c[1].to_string());
        let mut candidates = vec![];
        if let Some(url) = advertised {
            candidates.push(secure_url(&url)?);
        }
        let mut scoped = resource.clone();
        scoped.set_query(None);
        scoped.set_path(&format!(
            "/.well-known/oauth-protected-resource{}",
            resource.path()
        ));
        candidates.push(scoped);
        let mut root = resource.clone();
        root.set_query(None);
        root.set_path("/.well-known/oauth-protected-resource");
        candidates.push(root);
        let mut protected = None;
        for url in candidates {
            if let Ok(response) = http.get(url).timeout(Duration::from_secs(10)).send().await {
                if response.status().is_success() {
                    protected = Some(bounded_json(response).await?);
                    break;
                }
            }
        }
        let protected=protected.ok_or("This service doesn't publish OAuth protected-resource metadata. Use an access token or contact its administrator.")?;
        if let Some(declared) = protected["resource"].as_str() {
            if declared != resource.as_str() {
                return Err("Authorization metadata describes a different resource".into());
            }
        }
        let issuer = protected["authorization_servers"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(Value::as_str)
            .ok_or("No authorization server is listed for this service")?;
        let issuer_url = secure_url(issuer)?;
        let mut rfc = issuer_url.clone();
        rfc.set_path(&format!(
            "/.well-known/oauth-authorization-server{}",
            issuer_url.path().trim_end_matches('/')
        ));
        let mut oidc = issuer_url.clone();
        oidc.set_path(&format!(
            "{}/.well-known/openid-configuration",
            issuer_url.path().trim_end_matches('/')
        ));
        let mut metadata = None;
        for url in [rfc, oidc] {
            if let Ok(response) = http.get(url).timeout(Duration::from_secs(10)).send().await {
                if response.status().is_success() {
                    metadata = Some(bounded_json(response).await?);
                    break;
                }
            }
        }
        let metadata = metadata
            .ok_or("The authorization server doesn't publish supported sign-in metadata")?;
        if metadata["issuer"] != issuer {
            return Err("Authorization server issuer mismatch".into());
        }
        if !metadata["code_challenge_methods_supported"]
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v == "S256"))
        {
            return Err("The authorization server must advertise PKCE S256 support".into());
        }
        let authorize = secure_url(s(&metadata["authorization_endpoint"]))?.to_string();
        let token = secure_url(s(&metadata["token_endpoint"]))?.to_string();
        let mut client_id = s(&c["auth"]["clientId"]).to_string();
        let mut client_secret = None;
        let cid = s(&c["id"]).to_string();
        let target = format!("mcp:{}", key(extension, &cid));
        // Registration needs the exact redirect URI. A preselected loopback port
        // is reserved by the common flow; native clients may register a dynamic
        // loopback redirect on authorization servers that support RFC 8252.
        let port = c["auth"]["callbackPort"]
            .as_u64()
            .unwrap_or(51947)
            .clamp(1024, 65535) as u16;
        let redirect = format!("http://127.0.0.1:{port}/callback");
        if client_id.is_empty() {
            let sid = key(extension, &cid);
            let core = self.clone();
            let saved =
                tokio::task::spawn_blocking(move || core.inner.extensions.secrets.values(&sid))
                    .await
                    .map_err(err)??;
            if let Some(grant) = saved
                .get("oauth-grant")
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
            {
                if grant["issuer"] == issuer && grant["resource"] == resource.as_str() {
                    client_id = s(&grant["clientId"]).to_string();
                    client_secret = grant["clientSecret"].as_str().map(str::to_string);
                }
            }
        }
        if client_id.is_empty() {
            let registration = secure_url(s(&metadata["registration_endpoint"]))?;
            let response=http.post(registration).json(&json!({"client_name":"Tezbar","application_type":"native","redirect_uris":[redirect],"grant_types":["authorization_code","refresh_token"],"response_types":["code"],"token_endpoint_auth_method":"none"})).timeout(Duration::from_secs(30)).send().await.map_err(|_|"Native client registration failed")?;
            if !response.status().is_success() {
                return Err(
                    "This service requires a pre-registered Client ID. Enter it in Advanced."
                        .into(),
                );
            }
            let registration = bounded_json(response).await?;
            client_id = registration["client_id"]
                .as_str()
                .filter(|v| !v.is_empty() && v.len() < 4096)
                .ok_or("Invalid client registration response")?
                .into();
            client_secret = registration["client_secret"].as_str().map(str::to_string);
        }
        self.oauth_start_native(Request{target,issuer:issuer.into(),authorize,token,client_id,client_secret,resource:Some(resource.to_string()),scopes:c["auth"]["scopes"].as_str().unwrap_or("").into(),port,callback_path:"/callback".into(),metadata:json!({"extension":extension,"connection":cid,"issuer":issuer,"resource":resource.as_str()})}).await
    }
    pub(in crate::native_core::ai) async fn persist_mcp_oauth(
        &self,
        request: &Request,
        mut token: Value,
        cancel: &Cancel,
    ) -> Result<(), String> {
        token["issuer"] = json!(request.issuer);
        token["resource"] = json!(request.resource);
        token["tokenEndpoint"] = json!(request.token);
        token["clientId"] = json!(request.client_id);
        if let Some(secret) = &request.client_secret {
            token["clientSecret"] = json!(secret)
        }
        let extension = s(&request.metadata["extension"]).to_string();
        let connection = s(&request.metadata["connection"]).to_string();
        let core = self.clone();
        let cancel = cancel.clone();
        self.ext_edit(move |data| {
            let _record = record_mut(data, &extension)?;
            if cancel.is_cancelled() {
                return Err("Sign-in was cancelled".into());
            }
            core.inner.extensions.secrets.put(
                &key(&extension, &connection),
                "oauth-grant",
                Some(&token.to_string()),
            )?;
            if let Some(client) = core
                .inner
                .extensions
                .clients
                .lock()
                .unwrap()
                .remove(&key(&extension, &connection))
            {
                client.shutdown();
            }
            Ok(json!({"ok":true}))
        })
        .await
        .map(|_| ())
    }
}
impl AiCore {
    pub(in crate::native_core::ai) async fn mcp_oauth_values(
        &self,
        extension: &str,
        c: &Value,
        force: bool,
    ) -> Result<HashMap<String, String>, String> {
        let _refresh = self.inner.token_refresh.lock().await;
        let id = key(extension, s(&c["id"]));
        let core = self.clone();
        let sid = id.clone();
        let mut values =
            tokio::task::spawn_blocking(move || core.inner.extensions.secrets.values(&sid))
                .await
                .map_err(err)??;
        if c["auth"]["type"] != "oauth" {
            return Ok(values);
        }
        let mut grant: Value =
            serde_json::from_str(values.get("oauth-grant").ok_or("Sign in required")?)
                .map_err(|_| "Saved sign-in is invalid. Sign in again.")?;
        if grant["resource"] != secure_url(s(&c["url"]))?.as_str() {
            return Err("Saved sign-in belongs to another resource. Sign in again.".into());
        }
        if !force
            && grant["expiresAt"]
                .as_u64()
                .is_some_and(|at| now_ms() < at.saturating_sub(60000))
        {
            return Ok(values);
        }
        let refresh = grant["refresh_token"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or("Sign-in expired. Sign in again.")?;
        let endpoint = secure_url(s(&grant["tokenEndpoint"]))?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(err)?;
        let mut form = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
            ("client_id", s(&grant["clientId"])),
            ("resource", s(&grant["resource"])),
        ];
        if let Some(secret) = grant["clientSecret"].as_str() {
            form.push(("client_secret", secret));
        }
        let response = http
            .post(endpoint)
            .form(&form)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|_| "Couldn't refresh this sign-in. Try again.")?;
        if !response.status().is_success() {
            return Err("Sign-in expired or needs more access. Sign in again.".into());
        }
        let token = bounded_json(response).await?;
        if s(&token["access_token"]).is_empty() {
            return Err("Sign-in refresh returned no access token".into());
        }
        grant["access_token"] = token["access_token"].clone();
        if token["refresh_token"].is_string() {
            grant["refresh_token"] = token["refresh_token"].clone()
        }
        grant["expiresAt"] = json!(now_ms() + token["expires_in"].as_u64().unwrap_or(3600) * 1000);
        let raw = grant.to_string();
        let core = self.clone();
        let save = raw.clone();
        tokio::task::spawn_blocking(move || {
            core.inner
                .extensions
                .secrets
                .put(&id, "oauth-grant", Some(&save))
        })
        .await
        .map_err(err)??;
        values.insert("oauth-grant".into(), raw);
        Ok(values)
    }
}
