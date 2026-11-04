//! Bounded, user-initiated authorization-code/PKCE flows. No token is returned
//! through IPC. The callback server and paste path share the same state check.
use super::*;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
#[derive(Clone)]
pub(super) struct Handle {
    pub cancel: Cancel,
    pub paste: tokio::sync::mpsc::Sender<String>,
    pub generation: String,
}
pub(super) struct Request {
    pub target: String,
    pub issuer: String,
    pub authorize: String,
    pub token: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub resource: Option<String>,
    pub resource_policy: Option<ResourcePolicy>,
    pub scopes: String,
    pub port: u16,
    pub callback_path: String,
    pub metadata: Value,
}
fn nonce() -> String {
    URL_SAFE_NO_PAD.encode(crate::native_core::vault::crypto::generate_random_bytes::<32>())
}
pub(super) fn secure_url(raw: &str) -> Result<reqwest::Url, String> {
    let u = reqwest::Url::parse(raw.trim()).map_err(|_| "Invalid authorization address")?;
    if u.scheme() != "https"
        && !(u.scheme() == "http"
            && matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
    {
        return Err("Authorization requires https or a localhost address".into());
    }
    if !u.username().is_empty() || u.password().is_some() {
        return Err("Authorization addresses cannot contain credentials".into());
    }
    Ok(u)
}
/// One authorization flow pins the resource's resolved trust zone and address.
/// Endpoints are DNS-pinned independently; redirects and proxies stay disabled.
#[derive(Clone)]
pub(super) struct ResourcePolicy {
    resource: reqwest::Url,
    addresses: Vec<std::net::SocketAddr>,
    pub private: bool,
}
fn private_address(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v) => {
            let [a, b, c, _] = v.octets();
            v.is_private()
                || v.is_loopback()
                || v.is_link_local()
                || v.is_multicast()
                || v.is_broadcast()
                || a == 0
                || a >= 240
                || (a == 100 && (64..=127).contains(&b))
                || (a == 198 && (b == 18 || b == 19))
                || (a == 192 && (b == 0 || b == 88 && c == 99))
                || (a == 198 && b == 51 && c == 100)
                || (a == 203 && b == 0 && c == 113)
        }
        std::net::IpAddr::V6(v) => {
            if let Some(ip) = v.to_ipv4_mapped() {
                return private_address(ip.into());
            }
            let octets = v.octets();
            if octets[..12] == [0, 0x64, 0xff, 0x9b, 0, 0, 0, 0, 0, 0, 0, 0] {
                return private_address(
                    std::net::Ipv4Addr::new(octets[12], octets[13], octets[14], octets[15]).into(),
                );
            }
            v.is_loopback()
                || v.is_unspecified()
                || v.is_unique_local()
                || v.is_unicast_link_local()
                || v.is_multicast()
                || v.to_ipv4_mapped()
                    .is_some_and(|v| private_address(v.into()))
                || (v.segments()[0] & 0xe000 != 0x2000)
                || (v.segments()[0] == 0x2001
                    && (v.segments()[1] < 0x200 || v.segments()[1] == 0xdb8))
                || v.segments()[0] == 0x2002
                || (v.segments()[0] == 0x3fff && v.segments()[1] < 0x1000)
        }
    }
}
async fn resolve_authorization(url: &reqwest::Url) -> Result<Vec<std::net::SocketAddr>, String> {
    let host = url
        .host_str()
        .ok_or("Missing authorization host")?
        .trim_matches(['[', ']']);
    let port = url.port_or_known_default().unwrap_or(443);
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return Ok(vec![(ip, port).into()]);
    }
    let addresses = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::lookup_host((host, port)),
    )
    .await
    .map_err(|_| "Authorization address lookup timed out")?
    .map_err(|_| "Authorization address lookup failed")?
    .collect::<Vec<_>>();
    if addresses.is_empty() {
        return Err("Authorization address lookup returned no addresses".into());
    }
    Ok(addresses)
}
impl ResourcePolicy {
    #[cfg(test)]
    pub(in crate::native_core::ai) fn from_fixture(resource: &reqwest::Url, address: &str) -> Self {
        Self::from_addresses(resource, vec![address.parse().unwrap()]).unwrap()
    }
    pub async fn new(resource: &reqwest::Url) -> Result<Self, String> {
        Self::from_addresses(resource, resolve_authorization(resource).await?)
    }
    fn from_addresses(
        resource: &reqwest::Url,
        addresses: Vec<std::net::SocketAddr>,
    ) -> Result<Self, String> {
        if addresses.is_empty() {
            return Err("Authorization address lookup returned no addresses".into());
        }
        let private = addresses.iter().all(|a| private_address(a.ip()));
        if !private && addresses.iter().any(|a| private_address(a.ip())) {
            return Err("Authorization resource mixes public and private addresses".into());
        }
        Ok(Self {
            resource: resource.clone(),
            addresses,
            private,
        })
    }
    pub async fn http(&self, endpoint: &reqwest::Url) -> Result<reqwest::Client, String> {
        let addresses = if endpoint.host_str() == self.resource.host_str() {
            self.addresses
                .iter()
                .map(|a| {
                    std::net::SocketAddr::new(
                        a.ip(),
                        endpoint.port_or_known_default().unwrap_or(443),
                    )
                })
                .collect()
        } else {
            resolve_authorization(endpoint).await?
        };
        self.pinned_http(endpoint, &addresses)
    }
    fn pinned_http(
        &self,
        endpoint: &reqwest::Url,
        addresses: &[std::net::SocketAddr],
    ) -> Result<reqwest::Client, String> {
        if addresses.is_empty()
            || (!self.private && addresses.iter().any(|a| private_address(a.ip())))
        {
            return Err("Remote sign-in metadata cannot access a private or local address".into());
        }
        let host = endpoint
            .host_str()
            .ok_or("Missing authorization host")?
            .trim_matches(['[', ']']);
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .resolve_to_addrs(host, addresses)
            .build()
            .map_err(err)
    }
}
impl AiCore {
    pub(super) async fn oauth_start_native(&self, request: Request) -> CoreResult {
        secure_url(&request.issuer)?;
        let mut authorize = secure_url(&request.authorize)?;
        secure_url(&request.token)?;
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, request.port))
            .await
            .map_err(|_| {
                if request.port == 51121 {
                    "Port 51121 is already in use. Close the program using it and try again."
                } else {
                    "Couldn't open an authorization callback port"
                }
            })?;
        let port = listener.local_addr().map_err(err)?.port();
        let redirect = format!(
            "http://{}:{port}{}",
            if request.target == "antigravity" {
                "localhost"
            } else {
                "127.0.0.1"
            },
            request.callback_path
        );
        let state = nonce();
        let verifier = nonce();
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        authorize.query_pairs_mut().extend_pairs([
            ("response_type", "code"),
            ("client_id", &request.client_id),
            ("redirect_uri", &redirect),
            ("state", &state),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
        ]);
        if !request.scopes.trim().is_empty() {
            authorize
                .query_pairs_mut()
                .append_pair("scope", request.scopes.trim());
        }
        if let Some(resource) = &request.resource {
            authorize
                .query_pairs_mut()
                .append_pair("resource", resource);
        }
        if request.target == "antigravity" {
            authorize
                .query_pairs_mut()
                .append_pair("access_type", "offline")
                .append_pair("prompt", "consent");
        }
        let cancel = Cancel::new();
        let (paste_tx, mut paste_rx) = tokio::sync::mpsc::channel(1);
        let generation = uuid::Uuid::new_v4().to_string();
        {
            let mut flows = self.inner.oauth_flows.lock().unwrap();
            if let Some(old) = flows.insert(
                request.target.clone(),
                Handle {
                    cancel: cancel.clone(),
                    paste: paste_tx,
                    generation: generation.clone(),
                },
            ) {
                old.cancel.cancel();
            }
        }
        self.oauth_status(&request.target, json!({"status":"pending"}));
        let core = self.clone();
        let target = request.target.clone();
        let browser_url = authorize.to_string();
        self.spawn(async move{
            let work=async{
                let callback=loop{
                    tokio::select!{biased;_=cancel.cancelled()=>return Err("Sign-in was cancelled".to_string()),
                        pasted=paste_rx.recv()=>{let pasted=pasted.ok_or("Sign-in was cancelled")?;break pasted},
                        accepted=listener.accept()=>{
                            let(mut socket,_)=accepted.map_err(|_|"Authorization callback failed")?;let mut bytes=vec![];
                            let read=async{loop{let mut b=[0;1024];let n=socket.read(&mut b).await.map_err(|_|"Authorization callback read failed")?;if n==0{return Err("Authorization callback closed")}bytes.extend_from_slice(&b[..n]);if bytes.len()>8192{return Err("Authorization callback was too large")}if bytes.windows(4).any(|w|w==b"\r\n\r\n"){break}}Ok::<(),&str>(())};
                            if tokio::time::timeout(Duration::from_secs(5),read).await.is_err(){continue}
                            let line=String::from_utf8_lossy(&bytes);let line=line.lines().next().unwrap_or("");let mut parts=line.split_whitespace();let method=parts.next().unwrap_or("");let path=parts.next().unwrap_or("");let url=format!("http://127.0.0.1:{port}{path}");
                            let matches=method=="GET"&&reqwest::Url::parse(&url).ok().is_some_and(|u|u.path()==request.callback_path);
                            let text="Return to Tezbar to see the authorization result.";
                            let response=if matches{format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{text}",text.len())}else{"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()};let _=socket.write_all(response.as_bytes()).await;
                            if matches{break url}
                        }
                    }
                };
                let callback=reqwest::Url::parse(&callback).map_err(|_|"Paste the complete callback URL")?;
                if callback.path()!=request.callback_path||!matches!(callback.host_str(),Some("localhost"|"127.0.0.1"|"[::1]"))||callback.port()!=Some(port){return Err("The callback URL doesn't match this sign-in".into())}
                let params=callback.query_pairs().collect::<HashMap<_,_>>();if params.get("state").map(|v|v.as_ref())!=Some(state.as_str()){return Err("OAuth state mismatch. Start sign-in again.".into())}
                if params.get("iss").is_some_and(|issuer|issuer.as_ref()!=request.issuer){return Err("OAuth issuer mismatch. Start sign-in again.".into())}
                if params.contains_key("error"){return Err("Authorization was denied. Start sign-in again when ready.".into())}
                let code=params.get("code").filter(|c|!c.is_empty()).ok_or("Authorization callback has no code")?;
                let mut form=vec![("grant_type","authorization_code".to_string()),("code",code.to_string()),("client_id",request.client_id.clone()),("redirect_uri",redirect.clone()),("code_verifier",verifier)];
                if let Some(secret)=&request.client_secret{form.push(("client_secret",secret.clone()));}if let Some(resource)=&request.resource{form.push(("resource",resource.clone()));}
                let endpoint=secure_url(&request.token)?;
                let http=if request.target.starts_with("mcp:") {request.resource_policy.as_ref().ok_or("Missing OAuth resource policy")?.http(&endpoint).await?} else {reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().map_err(err)?};
                let response=http.post(&request.token).form(&form).timeout(Duration::from_secs(30)).send().await.map_err(|_|"Authorization token request failed")?;
                if !response.status().is_success(){return Err(format!("Authorization token request was rejected ({}). Start sign-in again.",response.status().as_u16()))}
                let mut token:Value=bounded_json(response).await?;
                if s(&token["access_token"]).is_empty(){return Err("Authorization response has no access token".into())}
                if token["token_type"].as_str().is_some_and(|t|!t.eq_ignore_ascii_case("bearer")){return Err("Unsupported authorization token type".into())}
                token["expiresAt"]=json!(now_ms()+token["expires_in"].as_u64().unwrap_or(3600)*1000);
                if cancel.is_cancelled()||core.inner.oauth_flows.lock().unwrap().get(&target).map(|h|h.generation.as_str())!=Some(generation.as_str()){return Err("Sign-in was cancelled".into())}
                core.persist_oauth_grant(&request,token,&cancel).await?;Ok::<(),String>(())
            };
            let result=tokio::select!{biased;_=cancel.cancelled()=>Err("Sign-in was cancelled".into()),r=tokio::time::timeout(Duration::from_secs(300),work)=>r.unwrap_or_else(|_|Err("Sign-in timed out. Start again.".into()))};
            cancel.cancel();
            let current=core.inner.oauth_flows.lock().unwrap().get(&target).is_some_and(|h|h.generation==generation);
            if current{core.inner.oauth_flows.lock().unwrap().remove(&target);core.oauth_status(&target,match result{Ok(())=>json!({"status":"success"}),Err(e)=>json!({"status":"error","error":e})});}
        });
        Ok(json!({"url":browser_url,"status":"pending"}))
    }
    fn oauth_status(&self, target: &str, status: Value) {
        self.inner
            .oauth_results
            .lock()
            .unwrap()
            .insert(target.into(), status.clone());
        self.inner.ctx.emit("ai-extensions-changed", Value::Null);
        self.inner.ctx.emit(
            "ai-account-status",
            json!({"target":target,"result":status}),
        );
    }
    pub(super) fn cancel_oauth_native(&self, target: &str) -> Value {
        if let Some(flow) = self.inner.oauth_flows.lock().unwrap().remove(target) {
            flow.cancel.cancel();
        }
        self.oauth_status(target, json!({"status":"cancelled"}));
        json!({"ok":true})
    }
    pub(super) async fn paste_oauth_native(&self, target: &str, url: &str) -> CoreResult {
        let paste = self
            .inner
            .oauth_flows
            .lock()
            .unwrap()
            .get(target)
            .map(|h| h.paste.clone())
            .ok_or("Start sign-in before pasting a callback")?;
        paste
            .try_send(url.into())
            .map_err(|_| "A callback has already been submitted")?;
        Ok(json!({"ok":true}))
    }
}
pub(super) async fn bounded_json(response: reqwest::Response) -> CoreResult {
    use futures_util::StreamExt;
    let mut bytes = vec![];
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "Authorization metadata connection failed")?;
        if bytes.len() + chunk.len() > 256 * 1024 {
            return Err("Authorization response exceeds 256 KiB".into());
        }
        bytes.extend_from_slice(&chunk)
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid authorization JSON response".into())
}
impl AiCore {
    async fn persist_oauth_grant(
        &self,
        request: &Request,
        token: Value,
        cancel: &Cancel,
    ) -> Result<(), String> {
        if request.target == "antigravity" {
            self.persist_antigravity_grant(token, cancel).await
        } else {
            self.persist_mcp_oauth(request, token, cancel).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::fixture;
    use super::*;
    async fn status(
        events: &mut tokio::sync::mpsc::UnboundedReceiver<(String, Value)>,
        expected: &str,
    ) -> Value {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let (channel, event) = events.recv().await.unwrap();
                if channel == "ai-account-status" && event["result"]["status"] == expected {
                    return event;
                }
            }
        })
        .await
        .unwrap()
    }
    #[tokio::test]
    async fn pkce_paste_checks_state_issuer_cancel_and_encrypts_successful_grant() {
        let (_dir, core, mut events) = fixture();
        core.handle("ai-extensions:add",&[json!({"manifestVersion":1,"id":"oauth-fixture","name":"OAuth fixture","connections":[{"id":"web","name":"Web","transport":"http","url":"http://127.0.0.1/fixture-mcp","auth":{"type":"oauth"}}]})]).await.unwrap().unwrap();
        for scenario in ["state", "issuer", "cancel", "success"] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let (tx, rx) = tokio::sync::oneshot::channel();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = vec![];
                let end;
                loop {
                    let mut b = [0; 4096];
                    let n = socket.read(&mut b).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&b[..n]);
                    if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        end = i + 4;
                        break;
                    }
                }
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let len = headers
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap();
                while bytes.len() < end + len {
                    let mut b = [0; 4096];
                    let n = socket.read(&mut b).await.unwrap();
                    bytes.extend_from_slice(&b[..n]);
                }
                let _ = tx.send(String::from_utf8(bytes[end..end + len].to_vec()).unwrap());
                let body=json!({"access_token":"fixture-oauth-access","refresh_token":"fixture-oauth-refresh","token_type":"Bearer","expires_in":3600}).to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            });
            let result = core
                .oauth_start_native(Request {
                    target: "mcp:oauth-fixture:web".into(),
                    issuer: base.clone(),
                    authorize: format!("{base}/authorize"),
                    token: format!("{base}/token"),
                    client_id: "fixture-client".into(),
                    client_secret: None,
                    resource: Some("http://127.0.0.1/fixture-mcp".into()),
                    resource_policy: Some(
                        ResourcePolicy::new(&secure_url("http://127.0.0.1/fixture-mcp").unwrap())
                            .await
                            .unwrap(),
                    ),
                    scopes: if scenario == "cancel" {
                        String::new()
                    } else {
                        "tools".into()
                    },
                    port: 0,
                    callback_path: "/callback".into(),
                    metadata: json!({"extension":"oauth-fixture","connection":"web"}),
                })
                .await
                .unwrap();
            let url = reqwest::Url::parse(s(&result["url"])).unwrap();
            let params = url
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect::<HashMap<_, _>>();
            assert_eq!(params["code_challenge_method"], "S256");
            assert_eq!(params["resource"], "http://127.0.0.1/fixture-mcp");
            if scenario == "cancel" {
                assert!(!params.contains_key("scope"));
            }
            if scenario == "cancel" {
                core.cancel_oauth_native("mcp:oauth-fixture:web");
                assert!(core
                    .paste_oauth_native("mcp:oauth-fixture:web", "http://127.0.0.1:1/callback")
                    .await
                    .is_err());
                server.abort();
                continue;
            }
            let mut callback = reqwest::Url::parse(&params["redirect_uri"]).unwrap();
            callback
                .query_pairs_mut()
                .append_pair("code", "fixture-code")
                .append_pair(
                    "state",
                    if scenario == "state" {
                        "wrong"
                    } else {
                        &params["state"]
                    },
                );
            if scenario == "issuer" {
                callback
                    .query_pairs_mut()
                    .append_pair("iss", "https://wrong.example");
            }
            core.paste_oauth_native("mcp:oauth-fixture:web", callback.as_str())
                .await
                .unwrap();
            if scenario != "success" {
                let event = status(&mut events, "error").await;
                assert!(!event.to_string().contains("fixture-code"));
                assert!(!core
                    .inner
                    .ctx
                    .paths
                    .config_dir
                    .join("ai-extension-secrets.bin")
                    .exists());
                server.abort();
                continue;
            }
            let body = rx.await.unwrap();
            let form = reqwest::Url::parse(&format!("http://fixture/?{body}")).unwrap();
            let form = form.query_pairs().collect::<HashMap<_, _>>();
            assert_eq!(form["resource"], "http://127.0.0.1/fixture-mcp");
            assert_eq!(
                URL_SAFE_NO_PAD.encode(Sha256::digest(form["code_verifier"].as_bytes())),
                params["code_challenge"]
            );
            let event = status(&mut events, "success").await;
            assert!(!event.to_string().contains("fixture-oauth-access"));
            let encrypted = tokio::fs::read(
                core.inner
                    .ctx
                    .paths
                    .config_dir
                    .join("ai-extension-secrets.bin"),
            )
            .await
            .unwrap();
            assert!(!String::from_utf8_lossy(&encrypted).contains("fixture-oauth-access"));
            server.await.unwrap();
        }
        core.shutdown().await;
    }
}

#[cfg(test)]
mod resource_policy_tests {
    use super::*;
    #[tokio::test]
    async fn private_hostname_resource_is_pinned_and_public_metadata_cannot_rebind() {
        let mut fixture = crate::native_core::ai::fixture_http::server(|_| {
            crate::native_core::ai::fixture_http::Reply::json(json!({"ok":true}))
        })
        .await;
        let url = reqwest::Url::parse(&fixture.url).unwrap();
        let mut named = url.clone();
        named.set_host(Some("fixture.corp.example")).unwrap();
        // Resolver override at the DNS seam; every network byte goes to our loopback fixture.
        let policy =
            ResourcePolicy::from_fixture(&named, &format!("127.0.0.1:{}", url.port().unwrap()));
        assert!(policy.private);
        let result = policy
            .http(&named)
            .await
            .unwrap()
            .get(named.clone())
            .send()
            .await
            .unwrap();
        assert!(result.status().is_success());
        assert!(
            s(&fixture.requests.recv().await.unwrap()["headers"]["host"])
                .starts_with("fixture.corp.example")
        );
        let public = ResourcePolicy::from_fixture(
            &secure_url("https://public.example/mcp").unwrap(),
            "93.184.216.34:443",
        );
        for public_address in ["[64:ff9b::5db8:d822]:443", "[::ffff:93.184.216.34]:443"] {
            let translated = ResourcePolicy::from_fixture(
                &secure_url("https://public.example/mcp").unwrap(),
                public_address,
            );
            assert!(!translated.private);
            assert!(translated
                .http(&secure_url("http://127.0.0.1/token").unwrap())
                .await
                .is_err());
            assert!(translated
                .pinned_http(
                    &secure_url("https://metadata.example/token").unwrap(),
                    &["10.0.0.1:443".parse().unwrap()]
                )
                .is_err());
            assert!(ResourcePolicy::from_addresses(
                &named,
                vec![
                    public_address.parse().unwrap(),
                    "10.0.0.1:443".parse().unwrap()
                ]
            )
            .is_err());
        }
        for address in [
            "[64:ff9b::a00:1]:443",
            "[64:ff9b::7f00:1]:443",
            "[64:ff9b:1::5db8:d822]:443",
            "[::ffff:10.0.0.1]:443",
            "127.0.0.1:443",
            "10.0.0.1:443",
            "100.77.254.64:443",
            "198.18.0.1:443",
            "192.0.0.1:443",
            "240.1.1.1:443",
            "0.1.1.1:443",
            "[::1]:443",
            "[fc00::1]:443",
            "192.88.99.1:443",
            "[2001:2::1]:443",
            "[2002::1]:443",
            "[3fff::1]:443",
        ] {
            let endpoint = secure_url("https://metadata.example/token").unwrap();
            assert!(
                public
                    .pinned_http(&endpoint, &[address.parse().unwrap()])
                    .is_err(),
                "{address}"
            );
            assert!(
                policy
                    .pinned_http(&endpoint, &[address.parse().unwrap()])
                    .is_ok(),
                "{address}"
            );
        }
        assert!(ResourcePolicy::from_addresses(
            &named,
            vec![
                "10.0.0.1:443".parse().unwrap(),
                "93.184.216.34:443".parse().unwrap()
            ]
        )
        .is_err());
    }
}
