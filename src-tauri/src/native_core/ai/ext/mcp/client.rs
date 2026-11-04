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
    http: reqwest::Client,
    input: Option<Arc<tokio::sync::Mutex<tokio::process::ChildStdin>>>,
    pending: Pending,
    stop: Cancel,
    pub version: Mutex<String>,
    pub session: Mutex<Option<String>>,
    pub capabilities: Mutex<Value>,
    pub instructions: Mutex<String>,
    pub logs: Arc<Mutex<VecDeque<String>>>,
    secrets: Vec<String>,
}
impl Client {
    pub async fn connect(
        core: &AiCore,
        c: Value,
        env: HashMap<String, String>,
        secrets: Vec<String>,
    ) -> Result<Arc<Self>, String> {
        let stop = Cancel::new();
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let logs = Arc::new(Mutex::new(VecDeque::new()));
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
                "LANG",
                "LC_ALL",
            ] {
                if let Some(value) = std::env::var_os(key) {
                    cmd.env(key, value);
                }
            }
            for arg in c["args"].as_array().into_iter().flatten() {
                cmd.arg(arg.as_str().ok_or("Arguments must be text")?);
            }
            if let Some(cwd) = c["cwd"].as_str().filter(|s| !s.is_empty()) {
                cmd.current_dir(cwd);
            }
            cmd.envs(env).stdin(std::process::Stdio::piped());
            let mut group = GroupChild::spawn(&mut cmd)?;
            let writer = Arc::new(tokio::sync::Mutex::new(
                group.child.stdin.take().ok_or("Missing extension input")?,
            ));
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
            core.spawn(async move{
                let mut frame=Lines::default();let mut buffer=[0u8;8192];let mut nonjson=0;
                loop {let read=tokio::select!{biased;_=killed.cancelled()=>break,r=stdout.read(&mut buffer)=>r};let Ok(n)=read else{break};if n==0{break}
                    let Ok(lines)=frame.feed(&buffer[..n],false)else{break};let mut failed=false;
                    for line in lines {let Ok(v)=serde_json::from_str::<Value>(&line)else{nonjson+=1;push_log(&log,&line,&scrubs);if nonjson>64{failed=true;break}continue};nonjson=0;
                        if let Some(method)=v["method"].as_str(){if !v["id"].is_null(){let reply=if method=="ping"{json!({"jsonrpc":"2.0","id":v["id"],"result":{}})}else{json!({"jsonrpc":"2.0","id":v["id"],"error":{"code":-32601,"message":"Tezbar does not support this server request"}})};let _=write(&writer,&reply).await;}continue}
                        if let Some(id)=v["id"].as_str(){if let Some(reply)=waiters.lock().unwrap().remove(id){let _=reply.send(rpc_result(v));}}
                    }if failed{break}
                }
                killed.cancel();for(_,reply)in waiters.lock().unwrap().drain(){let _=reply.send(Err("Connection lost. The outcome of this call is unknown.".into()));}
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
        let client = Arc::new(Self {
            connection: c,
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
        let discover = client
            .call(
                "server/discover",
                json!({}),
                &Cancel::new(),
                Duration::from_secs(5),
            )
            .await;
        let modern = match &discover {
            Ok(_) => true,
            Err(e) => e.contains("protocolVersion") || e.contains("supported"),
        };
        if modern {
            if let Ok(v) = discover {
                *client.capabilities.lock().unwrap() = v["capabilities"].clone();
            }
        } else {
            *client.version.lock().unwrap() = "2025-11-25".into();
            let init=client.call("initialize",json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"Tezbar","version":env!("CARGO_PKG_VERSION")}}),&Cancel::new(),Duration::from_secs(10)).await?;
            let version = s(&init["protocolVersion"]);
            if !matches!(version, "2025-11-25" | "2025-06-18" | "2025-03-26") {
                return Err(
                    "Unsupported connection protocol version; legacy HTTP+SSE is not supported"
                        .into(),
                );
            }
            *client.version.lock().unwrap() = version.into();
            *client.capabilities.lock().unwrap() = init["capabilities"].clone();
            *client.instructions.lock().unwrap() = cut(s(&init["instructions"]), 8192);
            client
                .notify("notifications/initialized", json!({}))
                .await?;
        }
        Ok(client)
    }
    pub fn shutdown(&self) {
        self.stop.cancel()
    }
    pub fn scrub(&self, text: &str) -> String {
        scrub(text, &self.secrets)
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
        self.http_request(&v, None).await.map(|_| ())
    }
    async fn http_request(&self, v: &Value, id: Option<&str>) -> CoreResult {
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
            if let Some(name) = v["params"]["name"].as_str() {
                let name = if name.is_ascii() {
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
        if let Some(session) = session {
            req = req.header("MCP-Session-Id", session)
        }
        for (k, value) in self.connection["headers"].as_object().into_iter().flatten() {
            if let Some(value) = value.as_str() {
                req = req.header(k, value)
            }
        }
        let response = req
            .timeout(Duration::from_secs(
                self.connection["timeoutSeconds"]
                    .as_u64()
                    .unwrap_or(120)
                    .min(1800),
            ))
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
        if status.as_u16() == 401 {
            return Err("Sign in required".into());
        }
        if status.as_u16() == 403 {
            return Err("Access denied. Review the requested access and sign in again.".into());
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
                        if v["id"].as_str() == id {
                            return rpc_result(v);
                        }
                    }
                }
            }
        }
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| "Invalid connection response")?;
        if value["id"].as_str() != id && !value["id"].is_null() {
            return Err("Connection response has an unrelated request ID".into());
        }
        rpc_result(value)
    }
    pub async fn call(
        &self,
        method: &str,
        params: Value,
        cancel: &Cancel,
        timeout: Duration,
    ) -> CoreResult {
        if self.stop.is_cancelled() || cancel.is_cancelled() {
            return Err("Connection was disabled or cancelled".into());
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
                self.http_request(&v, Some(&id)).await
            }
        };
        let result = tokio::select! {biased;_=self.stop.cancelled()=>Err("Connection was disabled".into()),_=cancel.cancelled()=>Err("Operation cancelled".into()),r=tokio::time::timeout(timeout,work)=>r.map_err(|_|"Connection timed out. The outcome of this call is unknown.".to_string()).and_then(|v|v)};
        self.pending.lock().unwrap().remove(&id);
        if result.is_err() {
            let _ = tokio::time::timeout(
                Duration::from_secs(1),
                self.notify("notifications/cancelled", json!({"requestId":id})),
            )
            .await;
        }
        result.map_err(|e| self.scrub(&e))
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}
async fn write(
    w: &tokio::sync::Mutex<tokio::process::ChildStdin>,
    v: &Value,
) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(v).map_err(err)?;
    bytes.push(b'\n');
    w.lock()
        .await
        .write_all(&bytes)
        .await
        .map_err(|_| "Connection input closed".into())
}
fn rpc_result(v: Value) -> CoreResult {
    if let Some(e) = v.get("error") {
        return Err(format!(
            "Connection error {}: {}",
            e["code"],
            cut(s(&e["message"]), 500)
        ));
    }
    let result = v.get("result").ok_or("Connection response has no result")?;
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
