//! Correlated MCP connections. tools/call is issued once; broken streams return
//! an unknown-outcome error rather than replaying an effect.
use super::super::super::{
    process::{GroupChild, Lines},
    *,
};
use base64::Engine;
use std::collections::VecDeque;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
const MODERN: &str = "2026-07-28";
type Pending = Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<CoreResult>>>>;
pub struct Client {
    connection: Value,
    core: std::sync::Weak<Inner>,
    http: reqwest::Client,
    input: Option<Arc<tokio::sync::Mutex<Option<tokio::process::ChildStdin>>>>,
    pending: Pending,
    stop: Cancel,
    pub changed: Arc<std::sync::atomic::AtomicBool>,
    crashed: Arc<std::sync::atomic::AtomicBool>,
    negotiation: Arc<Mutex<Option<Value>>>,
    legacy_expired: std::sync::atomic::AtomicBool,
    legacy_init: tokio::sync::Mutex<()>,
    last_used: std::sync::atomic::AtomicU64,
    active: std::sync::atomic::AtomicUsize,
    pub version: Mutex<String>,
    pub session: Mutex<Option<String>>,
    pub capabilities: Mutex<Value>,
    pub instructions: Mutex<String>,
    pub logs: Arc<Mutex<VecDeque<String>>>,
    secrets: Vec<String>,
    http_token: Mutex<Option<String>>,
    pub requested_scopes: Mutex<Option<String>>,
    refreshed_secrets: Mutex<Vec<String>>,
}
impl Client {
    pub async fn connect(
        core: &AiCore,
        c: Value,
        env: HashMap<String, String>,
        secrets: Vec<String>,
    ) -> Result<Arc<Self>, String> {
        let stop = Cancel::new();
        let mut startup = StopGuard {
            cancel: stop.clone(),
            armed: true,
        };
        let crashed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let logs = Arc::new(Mutex::new(VecDeque::new()));
        let changed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let negotiation = Arc::new(Mutex::new(None));
        let mut input = None;
        if c["transport"] == "stdio" {
            let command = c["command"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("Command is required")?;
            let mut cmd = core.command(command).await;
            let path = cmd
                .as_std()
                .get_envs()
                .find(|(k, _)| *k == "PATH")
                .and_then(|(_, v)| v)
                .map(|v| v.to_os_string());
            #[cfg(windows)]
            {
                let effective = env
                    .get("PATH")
                    .map(std::ffi::OsStr::new)
                    .or(path.as_deref())
                    .unwrap_or(std::ffi::OsStr::new(""));
                let args = c["args"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_string)
                            .ok_or("Arguments must be text")
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                cmd = process::windows_mcp_command(
                    command,
                    effective,
                    &env.get("PATHEXT").cloned().unwrap_or_else(|| {
                        std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
                    }),
                    &args,
                )?;
            }
            cmd.env_clear();
            if let Some(path) = path {
                cmd.env("PATH", path);
            }
            for key in [
                "HOME",
                "USERPROFILE",
                "APPDATA",
                "LOCALAPPDATA",
                "TEMP",
                "TMP",
                "TMPDIR",
                "SystemRoot",
                "PATHEXT",
                "ComSpec",
                "ProgramData",
                "LANG",
                "LC_ALL",
            ] {
                if let Some(value) = std::env::var_os(key) {
                    cmd.env(key, value);
                }
            }
            #[cfg(test)]
            {
                cmd.env("HOME", &core.inner.ctx.paths.home_dir)
                    .env("USERPROFILE", &core.inner.ctx.paths.home_dir)
                    .env("APPDATA", &core.inner.ctx.paths.config_dir)
                    .env("LOCALAPPDATA", &core.inner.ctx.paths.config_dir)
                    .env("TEMP", &core.inner.ctx.paths.temp_dir)
                    .env("TMP", &core.inner.ctx.paths.temp_dir)
                    .env("TMPDIR", &core.inner.ctx.paths.temp_dir);
            }
            #[cfg(not(windows))]
            for arg in c["args"].as_array().into_iter().flatten() {
                cmd.arg(arg.as_str().ok_or("Arguments must be text")?);
            }
            cmd.current_dir(
                c["cwd"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(std::path::Path::new)
                    .unwrap_or(&core.inner.ctx.paths.home_dir),
            );
            cmd.envs(env).stdin(std::process::Stdio::piped());
            let mut group = GroupChild::spawn(&mut cmd)?;
            let writer = Arc::new(tokio::sync::Mutex::new(Some(
                group.child.stdin.take().ok_or("Missing extension input")?,
            )));
            input = Some(writer.clone());
            let mut stdout = group
                .child
                .stdout
                .take()
                .ok_or("Missing extension output")?;
            let mut stderr = group.child.stderr.take().ok_or("Missing extension log")?;
            let killed = stop.clone();
            let waiters = pending.clone();
            let log = logs.clone();
            let scrubs = secrets.clone();
            let crash = crashed.clone();
            let change = changed.clone();
            let errors = negotiation.clone();
            core.spawn(async move{
                let mut frame=Lines::default();let mut buffer=[0u8;8192];let mut nonjson=0;
                loop {let read=tokio::select!{biased;_=killed.cancelled()=>break,r=stdout.read(&mut buffer)=>r};let Ok(n)=read else{break};if n==0{break}
                    let Ok(lines)=frame.feed(&buffer[..n],false)else{break};let mut failed=false;
                    for line in lines {let Ok(v)=serde_json::from_str::<Value>(&line)else{nonjson+=1;push_log(&log,&line,&scrubs);if nonjson>64{failed=true;break}continue};nonjson=0;
                        if let Some(method)=v["method"].as_str(){if method.ends_with("/list_changed"){change.store(true,std::sync::atomic::Ordering::SeqCst);}if !v["id"].is_null(){let reply=if method=="ping"{json!({"jsonrpc":"2.0","id":v["id"],"result":{}})}else{json!({"jsonrpc":"2.0","id":v["id"],"error":{"code":-32601,"message":"Tezbar does not support this server request"}})};let _=write(&writer,&reply).await;}continue}
                        if let Some(id)=v["id"].as_str(){if let Some(reply)=waiters.lock().unwrap().remove(id){if v.get("error").is_some(){*errors.lock().unwrap()=Some(v["error"].clone());}let _=reply.send(rpc_result(v));}}
                    }if failed{break}
                }
                if !killed.is_cancelled(){crash.store(true,std::sync::atomic::Ordering::SeqCst);}killed.cancel();for(_,reply)in waiters.lock().unwrap().drain(){let _=reply.send(Err("Connection lost. The outcome of this call is unknown.".into()));}
                writer.lock().await.take();
                if tokio::time::timeout(Duration::from_secs(2),group.child.wait()).await.is_err(){group.terminate().await;}
                // Always terminate the owned tree, even if its root exited first.
                group.terminate().await;
            });
            let killed = stop.clone();
            let log = logs.clone();
            let scrubs = secrets.clone();
            core.spawn(async move{let mut frame=Lines::default();let mut b=[0;4096];loop{let n=tokio::select!{_=killed.cancelled()=>break,r=stderr.read(&mut b)=>match r{Ok(n)if n>0=>n,_=>break}};match frame.feed(&b[..n],false){Ok(lines)=>for line in lines{push_log(&log,&line,&scrubs)},Err(_)=>frame=Lines::default()}}});
        }
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(err)?;
        let http_token = if c["auth"]["type"] == "oauth" {
            c["headers"]["Authorization"]
                .as_str()
                .and_then(|v| v.strip_prefix("Bearer "))
                .map(str::to_string)
        } else {
            None
        };
        let client = Arc::new(Self {
            requested_scopes: Mutex::new(None),
            http_token: Mutex::new(http_token),
            refreshed_secrets: Mutex::new(Vec::new()),
            changed,
            crashed,
            negotiation,
            legacy_expired: std::sync::atomic::AtomicBool::new(false),
            legacy_init: tokio::sync::Mutex::new(()),
            last_used: std::sync::atomic::AtomicU64::new(now_ms()),
            active: std::sync::atomic::AtomicUsize::new(0),
            connection: c.clone(),
            core: Arc::downgrade(&core.inner),
            http,
            input,
            pending,
            stop,
            version: Mutex::new(MODERN.into()),
            session: Mutex::new(None),
            capabilities: Mutex::new(json!({})),
            instructions: Mutex::new(String::new()),
            logs,
            secrets,
        });
        let mut discover = if c["cachedEra"]
            .as_str()
            .is_some_and(|v| v.starts_with("2025-"))
        {
            Err("Use previously negotiated legacy era".into())
        } else {
            client
                .call(
                    "server/discover",
                    json!({}),
                    &Cancel::new(),
                    Duration::from_secs(5),
                )
                .await
        };
        let negotiation = client.negotiation.lock().unwrap().clone();
        let mut legacy_version = c["cachedEra"]
            .as_str()
            .filter(|v| matches!(*v, "2025-11-25" | "2025-06-18" | "2025-03-26"))
            .unwrap_or("2025-11-25");
        let mut modern = match &discover {
            Ok(_) => true,
            Err(_) => negotiation
                .as_ref()
                .is_some_and(|e| matches!(e["code"].as_i64(), Some(-32020 | -32021 | -32022))),
        };
        if let Some(error) = negotiation.filter(|e| e["code"] == -32022) {
            if error["data"]["supported"]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v == MODERN))
            {
                discover = client
                    .call(
                        "server/discover",
                        json!({}),
                        &Cancel::new(),
                        Duration::from_secs(5),
                    )
                    .await;
            } else if let Some(version) = ["2025-11-25", "2025-06-18", "2025-03-26"]
                .into_iter()
                .find(|version| {
                    error["data"]["supported"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|v| v == version))
                })
            {
                modern = false;
                legacy_version = version;
            } else {
                return Err(
                    "This connection doesn't support any protocol version supported by Tezbar"
                        .into(),
                );
            }
        }
        if client.crashed() {
            return Err(format!(
                "The program exited during startup. {}",
                client
                    .logs
                    .lock()
                    .unwrap()
                    .iter()
                    .rev()
                    .take(20)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
        if modern {
            if let Err(error) = &discover {
                return Err(error.clone());
            }
            if let Ok(v) = discover {
                *client.capabilities.lock().unwrap() = v["capabilities"].clone();
            }
        } else {
            *client.version.lock().unwrap() = legacy_version.into();
            let init=client.call("initialize",json!({"protocolVersion":legacy_version,"capabilities":{},"clientInfo":{"name":"Tezbar","version":env!("CARGO_PKG_VERSION")}}),&Cancel::new(),Duration::from_secs(10)).await?;
            let version = s(&init["protocolVersion"]);
            if !matches!(version, "2025-11-25" | "2025-06-18" | "2025-03-26") {
                return Err(
                    "Unsupported connection protocol version; legacy HTTP+SSE is not supported"
                        .into(),
                );
            }
            *client.version.lock().unwrap() = version.into();
            *client.capabilities.lock().unwrap() = init["capabilities"].clone();
            *client.instructions.lock().unwrap() =
                client.scrub(&cut(s(&init["instructions"]), 8192));
            client
                .notify("notifications/initialized", json!({}))
                .await?;
        }
        // This watcher owns no strong reference. Active calls keep the connection
        // alive, while subscriptions do not postpone the ten-minute idle stop.
        let weak = Arc::downgrade(&client);
        let stopped = client.stop.clone();
        core.spawn(async move { loop { tokio::select!{_=stopped.cancelled()=>break,_=tokio::time::sleep(Duration::from_secs(30))=>{}} let Some(c)=weak.upgrade() else{break}; if c.stop.is_cancelled(){break} if c.active.load(std::sync::atomic::Ordering::SeqCst)==0&&now_ms().saturating_sub(c.last_used.load(std::sync::atomic::Ordering::SeqCst))>=600_000{c.shutdown();break} } });
        let weak = Arc::downgrade(&client);
        core.spawn(async move { if let Some(c)=weak.upgrade(){if *c.version.lock().unwrap()==MODERN{let _=c.call_prepared("subscriptions/listen",json!({"notifications":{"toolsListChanged":true,"promptsListChanged":true,"resourcesListChanged":true}}),&Cancel::new(),Duration::from_secs(86400),vec![]).await;}else if c.input.is_none(){let _=c.legacy_events().await;}} });
        startup.armed = false;
        Ok(client)
    }
    pub fn crashed(&self) -> bool {
        self.crashed.load(std::sync::atomic::Ordering::SeqCst)
    }
    pub fn is_closed(&self) -> bool {
        self.stop.is_cancelled()
    }
    pub fn shutdown(&self) {
        self.stop.cancel()
    }
    pub fn scrub(&self, text: &str) -> String {
        scrub(
            &scrub(text, &self.secrets),
            &self.refreshed_secrets.lock().unwrap(),
        )
    }
    pub fn remember_secrets(&self, values: &[String]) {
        let mut secrets = self.refreshed_secrets.lock().unwrap();
        for value in values {
            if !value.is_empty() && !secrets.contains(value) {
                secrets.push(value.clone());
            }
        }
        while secrets.len() > 128 {
            secrets.remove(0);
        }
    }
    pub fn scrub_value(&self, value: &mut Value) {
        match value {
            Value::String(text) => *text = self.scrub(text),
            Value::Array(array) => {
                for value in array {
                    self.scrub_value(value)
                }
            }
            Value::Object(map) => {
                let original = std::mem::take(map);
                for (key, mut value) in original {
                    self.scrub_value(&mut value);
                    map.insert(self.scrub(&key), value);
                }
            }
            _ => {}
        }
    }
    pub fn set_http_token(&self, token: &str) {
        let mut current = self.http_token.lock().unwrap();
        if current.as_deref() != Some(token) {
            *current = Some(token.into());
            let mut secrets = self.refreshed_secrets.lock().unwrap();
            if secrets.len() > 16 {
                secrets.remove(0);
            }
            secrets.push(token.into());
        }
    }

    fn request(&self, id: Option<&str>, method: &str, mut params: Value) -> Value {
        if *self.version.lock().unwrap() == MODERN {
            params["_meta"] = json!({"io.modelcontextprotocol/protocolVersion":MODERN,"io.modelcontextprotocol/clientInfo":{"name":"Tezbar","version":env!("CARGO_PKG_VERSION")},"io.modelcontextprotocol/clientCapabilities":{}})
        }
        let mut v = json!({"jsonrpc":"2.0","method":method,"params":params});
        if let Some(id) = id {
            v["id"] = json!(id)
        }
        v
    }
    async fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        let v = self.request(None, method, params);
        if let Some(w) = &self.input {
            return write(w, &v).await;
        }
        self.http_request(&v, None, &[]).await.map(|_| ())
    }
    async fn http_request(
        &self,
        v: &Value,
        id: Option<&str>,
        mirrors: &[(String, String)],
    ) -> CoreResult {
        let listening = v["method"] == "subscriptions/listen";
        let url = s(&self.connection["url"]);
        let version = self.version.lock().unwrap().clone();
        let session = self.session.lock().unwrap().clone();
        let mut req = self
            .http
            .post(url)
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", &version)
            .header("Mcp-Method", s(&v["method"]))
            .json(v);
        if version == MODERN {
            if let Some(name) = v["params"]["name"]
                .as_str()
                .or_else(|| v["params"]["uri"].as_str())
            {
                let name = if name.is_ascii() && !name.bytes().any(|b| b < 32 || b == 127) {
                    name.into()
                } else {
                    format!(
                        "=?base64?{}?=",
                        base64::engine::general_purpose::STANDARD.encode(name)
                    )
                };
                req = req.header("Mcp-Name", name);
            }
        }
        if let Some(session) = session.filter(|_| version != MODERN) {
            req = req.header("MCP-Session-Id", session)
        }
        let dynamic_token = self.http_token.lock().unwrap().clone();
        for (k, value) in self.connection["headers"].as_object().into_iter().flatten() {
            if dynamic_token.is_some() && k.eq_ignore_ascii_case("authorization") {
                continue;
            }
            if let Some(value) = value.as_str() {
                req = req.header(k, value)
            }
        }
        if let Some(token) = dynamic_token {
            req = req.bearer_auth(token);
        }
        for (name, value) in mirrors {
            req = req.header(name, value);
        }
        let response = req
            .timeout(Duration::from_secs(if listening {
                86400
            } else {
                self.connection["timeoutSeconds"]
                    .as_u64()
                    .unwrap_or(120)
                    .clamp(1, 1800)
            }))
            .send()
            .await
            .map_err(|_| "Connection lost. The outcome of this call is unknown.")?;
        let status = response.status();
        if status.is_redirection() {
            return Err("Connection refused a redirect. Check the web address.".into());
        }
        if let Some(session) = response
            .headers()
            .get("MCP-Session-Id")
            .and_then(|v| v.to_str().ok())
        {
            if session.len() <= 1024 {
                *self.session.lock().unwrap() = Some(session.into());
            }
        }
        if matches!(status.as_u16(), 401 | 403) {
            if let Some(scope) = response
                .headers()
                .get(reqwest::header::WWW_AUTHENTICATE)
                .and_then(|v| v.to_str().ok())
                .and_then(super::oauth::challenge_scope)
            {
                *self.requested_scopes.lock().unwrap() = Some(scope);
            }
        }
        if status.as_u16() == 401 {
            return Err("Sign in required".into());
        }
        if status.as_u16() == 403 {
            let requested = response
                .headers()
                .get(reqwest::header::WWW_AUTHENTICATE)
                .and_then(|v| v.to_str().ok())
                .map(|s| cut(s, 200))
                .unwrap_or_default();
            return Err(format!(
                "Access denied. Review the requested access and sign in again. {requested}"
            ));
        }
        if status.as_u16() == 404 && self.session.lock().unwrap().is_none() {
            return Err(
                "No MCP endpoint at this address (or it only supports the old HTTP+SSE transport)."
                    .into(),
            );
        }
        if status.as_u16() == 404 && version != MODERN {
            *self.session.lock().unwrap() = None;
            self.legacy_expired
                .store(true, std::sync::atomic::Ordering::SeqCst);
            return Err("Legacy session expired (404). Test the connection again; the call was not replayed.".into());
        }
        if id.is_none() && status.is_success() {
            return Ok(Value::Null);
        }
        let sse = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("text/event-stream"));
        use futures_util::StreamExt;
        let mut stream = response.bytes_stream();
        let mut bytes = vec![];
        let mut frame = Lines::default();
        let mut data = vec![];
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|_| "Connection lost. The outcome of this call is unknown.")?;
            if bytes.len() + chunk.len() > 16 * 1024 * 1024 {
                return Err("Connection response exceeded 16 MiB".into());
            }
            bytes.extend_from_slice(&chunk);
            if sse {
                for line in frame.feed(&chunk, false)? {
                    if let Some(s) = line.strip_prefix("data:") {
                        data.push(s.trim_start().to_string())
                    } else if line.is_empty() && !data.is_empty() {
                        let v: Value = serde_json::from_str(&std::mem::take(&mut data).join("\n"))
                            .map_err(|_| "Invalid connection response")?;
                        if let Some(method) = v["method"].as_str() {
                            if method.ends_with("/list_changed") {
                                self.changed
                                    .store(true, std::sync::atomic::Ordering::SeqCst);
                            }
                            if method == "notifications/message" {
                                push_log(&self.logs, s(&v["params"]["data"]), &self.secrets);
                            }
                            if !v["id"].is_null() {
                                if version == MODERN {
                                    return Err(
                                        "Unsupported server request on a modern response stream"
                                            .into(),
                                    );
                                }
                                Box::pin(self.legacy_reply(&v)).await?;
                            }
                        }
                        if v["id"].as_str() == id && v.get("method").is_none() {
                            if v.get("error").is_some() {
                                *self.negotiation.lock().unwrap() = Some(v["error"].clone());
                            }
                            if listening && v.get("error").is_none() {
                                continue;
                            }
                            return rpc_result(v);
                        }
                    }
                }
            }
        }
        if sse {
            return Err("Connection lost. The outcome of this call is unknown.".into());
        }
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| "Invalid connection response")?;
        if value["id"].as_str() != id && !(value["id"].is_null() && value.get("error").is_some()) {
            return Err("Connection response has an unrelated request ID".into());
        }
        if value.get("error").is_some() {
            *self.negotiation.lock().unwrap() = Some(value["error"].clone());
        }
        rpc_result(value)
    }
    pub async fn call_tool(
        &self,
        name: &str,
        args: Value,
        descriptor: &Value,
        cancel: &Cancel,
        timeout: Duration,
    ) -> CoreResult {
        self.call_tool_state(name, args, descriptor, cancel, timeout, None)
            .await
    }
    pub async fn call_tool_state(
        &self,
        name: &str,
        args: Value,
        descriptor: &Value,
        cancel: &Cancel,
        timeout: Duration,
        state: Option<Value>,
    ) -> CoreResult {
        let mirrors = if *self.version.lock().unwrap() == MODERN && self.input.is_none() {
            super::schema::mirrored_headers(&descriptor["inputSchema"], Some(&args))?
        } else {
            vec![]
        };
        let mut params = json!({"name":name,"arguments":args});
        if let Some(state) = state {
            params["requestState"] = state;
        }
        self.authorized("tools/call", params, cancel, timeout, mirrors)
            .await
    }
    pub async fn call(
        &self,
        method: &str,
        params: Value,
        cancel: &Cancel,
        timeout: Duration,
    ) -> CoreResult {
        let result = self
            .authorized(method, params.clone(), cancel, timeout, vec![])
            .await;
        if matches!(
            method,
            "tools/list" | "prompts/list" | "resources/list" | "resources/templates/list"
        ) && result
            .as_ref()
            .is_err_and(|e| e.starts_with("Legacy session expired"))
        {
            self.authorized(method, params, cancel, timeout, vec![])
                .await
        } else {
            result
        }
    }
    async fn authorized(
        &self,
        method: &str,
        params: Value,
        cancel: &Cancel,
        timeout: Duration,
        mirrors: Vec<(String, String)>,
    ) -> CoreResult {
        let result = self
            .call_prepared(method, params.clone(), cancel, timeout, mirrors.clone())
            .await;
        if result.as_ref().is_err_and(|e| e == "Sign in required")
            && self.connection["auth"]["type"] == "oauth"
            && !cancel.is_cancelled()
        {
            let core = AiCore {
                inner: self.core.upgrade().ok_or("Tezbar is closing")?,
            };
            let values = core
                .mcp_oauth_values(s(&self.connection["_extensionId"]), &self.connection, true)
                .await?;
            let grant: Value =
                serde_json::from_str(values.get("oauth-grant").ok_or("Sign in required")?)
                    .map_err(|_| "Invalid saved sign-in")?;
            self.set_http_token(s(&grant["access_token"]));
            self.remember_secrets(&[
                s(&grant["refresh_token"]).into(),
                s(&grant["clientSecret"]).into(),
            ]);
            self.call_prepared(method, params, cancel, timeout, mirrors)
                .await
        } else {
            result
        }
    }
    async fn call_prepared(
        &self,
        method: &str,
        params: Value,
        cancel: &Cancel,
        timeout: Duration,
        mirrors: Vec<(String, String)>,
    ) -> CoreResult {
        if self.stop.is_cancelled() || cancel.is_cancelled() {
            return Err("Connection was disabled or cancelled".into());
        }
        if method != "initialize"
            && self.input.is_none()
            && self
                .legacy_expired
                .load(std::sync::atomic::Ordering::SeqCst)
        {
            Box::pin(self.reinitialize()).await?;
        }
        let activity = Activity(self, method != "subscriptions/listen");
        if activity.1 {
            self.active
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.last_used
                .store(now_ms(), std::sync::atomic::Ordering::SeqCst);
        }
        let id = uuid::Uuid::new_v4().to_string();
        let v = self.request(Some(&id), method, params);
        let work = async {
            if let Some(w) = &self.input {
                let (tx, rx) = tokio::sync::oneshot::channel();
                self.pending.lock().unwrap().insert(id.clone(), tx);
                write(w, &v).await?;
                rx.await.map_err(|_| {
                    "Connection lost. The outcome of this call is unknown.".to_string()
                })?
            } else {
                self.http_request(&v, Some(&id), &mirrors).await
            }
        };
        let result = tokio::select! {biased;_=self.stop.cancelled()=>Err(if self.crashed.load(std::sync::atomic::Ordering::SeqCst){"Connection lost. The outcome of this call is unknown.".into()}else{"Connection was disabled".into()}),_=cancel.cancelled()=>Err("Operation cancelled".into()),r=tokio::time::timeout(timeout,work)=>r.map_err(|_|"Connection timed out. The outcome of this call is unknown.".to_string()).and_then(|v|v)};
        self.pending.lock().unwrap().remove(&id);
        if result.is_err() && (self.input.is_some() || *self.version.lock().unwrap() != MODERN) {
            let _ = tokio::time::timeout(
                Duration::from_secs(1),
                self.notify("notifications/cancelled", json!({"requestId":id})),
            )
            .await;
        }
        if result
            .as_ref()
            .is_err_and(|e| e.starts_with("Connection error -32022"))
            && method != "server/discover"
        {
            self.shutdown();
        }
        result.map_err(|e| self.scrub(&e))
    }
}
struct Activity<'a>(&'a Client, bool);
impl Drop for Activity<'_> {
    fn drop(&mut self) {
        if self.1 {
            self.0
                .last_used
                .store(now_ms(), std::sync::atomic::Ordering::SeqCst);
            self.0
                .active
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
}
impl Client {
    async fn reinitialize(&self) -> Result<(), String> {
        let _guard = self.legacy_init.lock().await;
        if !self
            .legacy_expired
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Ok(());
        }
        let version = self.version.lock().unwrap().clone();
        let id = uuid::Uuid::new_v4().to_string();
        let request=self.request(Some(&id),"initialize",json!({"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"Tezbar","version":env!("CARGO_PKG_VERSION")}}));
        let init = self.http_request(&request, Some(&id), &[]).await?;
        if init["protocolVersion"] != version {
            return Err("Connection changed protocol version. Test it again.".into());
        }
        self.legacy_expired
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.notify("notifications/initialized", json!({})).await?;
        Ok(())
    }
    async fn legacy_reply(&self, v: &Value) -> Result<(), String> {
        let reply = if v["method"] == "ping" {
            json!({"jsonrpc":"2.0","id":v["id"],"result":{}})
        } else {
            json!({"jsonrpc":"2.0","id":v["id"],"error":{"code":-32601,"message":"Tezbar does not support this server request"}})
        };
        self.http_request(&reply, None, &[]).await.map(|_| ())
    }
    async fn legacy_events(&self) -> Result<(), String> {
        use futures_util::StreamExt;
        let mut last = None::<String>;
        for attempt in 0..3 {
            if self.stop.is_cancelled() {
                break;
            }
            let mut req = self
                .http
                .get(s(&self.connection["url"]))
                .header("Accept", "text/event-stream")
                .header("MCP-Protocol-Version", self.version.lock().unwrap().clone());
            if let Some(s) = self.session.lock().unwrap().clone() {
                req = req.header("MCP-Session-Id", s)
            }
            if let Some(s) = &last {
                req = req.header("Last-Event-ID", s)
            }
            for (k, v) in self.connection["headers"].as_object().into_iter().flatten() {
                if let Some(v) = v.as_str() {
                    req = req.header(k, v)
                }
            }
            if let Some(token) = self.http_token.lock().unwrap().clone() {
                req = req.bearer_auth(token)
            }
            let response = tokio::select! {_=self.stop.cancelled()=>return Ok(()),r=req.timeout(Duration::from_secs(600)).send()=>r.map_err(|_|"Connection notification stream closed")?};
            if !response.status().is_success() {
                return Ok(());
            }
            let mut stream = response.bytes_stream();
            let mut frame = Lines::default();
            let mut data = vec![];
            loop {
                let chunk = tokio::select! {_=self.stop.cancelled()=>return Ok(()),r=stream.next()=>match r{Some(Ok(c))=>c,_=>break}};
                for line in frame.feed(&chunk, false)? {
                    if let Some(id) = line.strip_prefix("id:") {
                        if id.len() <= 1024 {
                            last = Some(id.trim().into())
                        }
                    } else if let Some(d) = line.strip_prefix("data:") {
                        data.push(d.trim_start().to_string())
                    } else if line.is_empty() && !data.is_empty() {
                        if let Ok(v) =
                            serde_json::from_str::<Value>(&std::mem::take(&mut data).join("\n"))
                        {
                            if v["method"]
                                .as_str()
                                .is_some_and(|m| m.ends_with("/list_changed"))
                            {
                                self.changed
                                    .store(true, std::sync::atomic::Ordering::SeqCst);
                            }
                            if !v["id"].is_null() && v.get("method").is_some() {
                                Box::pin(self.legacy_reply(&v)).await?;
                            }
                        }
                    }
                }
            }
            tokio::select! {_=self.stop.cancelled()=>break,_=tokio::time::sleep(Duration::from_secs([1,5,30][attempt]))=>()};
        }
        Ok(())
    }
}

struct StopGuard {
    cancel: Cancel,
    armed: bool,
}
impl Drop for StopGuard {
    fn drop(&mut self) {
        if self.armed {
            self.cancel.cancel();
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}
async fn write(
    w: &tokio::sync::Mutex<Option<tokio::process::ChildStdin>>,
    v: &Value,
) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(v).map_err(err)?;
    bytes.push(b'\n');
    w.lock()
        .await
        .as_mut()
        .ok_or("Connection input closed")?
        .write_all(&bytes)
        .await
        .map_err(|_| "Connection input closed".into())
}
fn rpc_result(v: Value) -> CoreResult {
    if v["jsonrpc"] != "2.0" {
        return Err("Invalid connection protocol response".into());
    }
    if let Some(e) = v.get("error") {
        return Err(format!(
            "Connection error {}: {}",
            e["code"],
            cut(s(&e["message"]), 500)
        ));
    }
    let result = v.get("result").ok_or("Connection response has no result")?;
    if !result.is_object() {
        return Err("Connection result must be an object".into());
    }
    match result["resultType"].as_str() {
        None | Some("complete" | "input_required") => Ok(result.clone()),
        _ => Err("Unsupported connection result type".into()),
    }
}
fn scrub(text: &str, secrets: &[String]) -> String {
    let mut text = redact_text(text);
    for secret in secrets {
        if !secret.is_empty() {
            text = text.replace(secret, "[redacted]")
        }
    }
    text
}
fn push_log(log: &Mutex<VecDeque<String>>, line: &str, secrets: &[String]) {
    let mut log = log.lock().unwrap();
    if log.len() == 200 {
        log.pop_front();
    }
    log.push_back(cut(&scrub(line, secrets), 2000));
}
