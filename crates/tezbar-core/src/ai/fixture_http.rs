//! Concurrent loopback fixture: no external addresses or user credentials.
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
pub struct Reply {
    pub status: u16,
    pub content_type: String,
    pub body: String,
    pub headers: Vec<(String, String)>,
    pub delay: Duration,
}
impl Reply {
    pub fn json(body: Value) -> Self {
        Self {
            status: 200,
            content_type: "application/json".into(),
            body: body.to_string(),
            headers: vec![],
            delay: Duration::ZERO,
        }
    }
    pub fn status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }
}
pub struct Server {
    pub url: String,
    pub requests: tokio::sync::mpsc::UnboundedReceiver<Value>,
    stop: Cancel,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.cancel();
        self.task.abort();
    }
}
pub async fn server(handler: impl Fn(&Value) -> Reply + Send + Sync + 'static) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let stop = Cancel::new();
    let cancel = stop.clone();
    let handler = Arc::new(handler);
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        loop {
            let socket =
                tokio::select! {_=cancel.cancelled()=>break,r=listener.accept()=>r.unwrap().0};
            let tx = tx.clone();
            let handler = handler.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move {
                let work = async move {
                    let mut socket = socket;
                    let mut bytes = vec![];
                    let end;
                    loop {
                        let mut b = [0; 8192];
                        let n = socket.read(&mut b).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        bytes.extend_from_slice(&b[..n]);
                        if bytes.len() > 17 * 1024 * 1024 {
                            return;
                        }
                        if let Some(i) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                            end = i + 4;
                            break;
                        }
                    }
                    let head = String::from_utf8_lossy(&bytes[..end]);
                    let request_line = head.lines().next().unwrap_or("").to_string();
                    let headers = head
                        .lines()
                        .skip(1)
                        .filter_map(|l| {
                            l.split_once(':')
                                .map(|(k, v)| (k.to_ascii_lowercase(), json!(v.trim())))
                        })
                        .collect::<serde_json::Map<_, _>>();
                    let length = headers
                        .get("content-length")
                        .and_then(Value::as_str)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    if length > 17 * 1024 * 1024 {
                        return;
                    }
                    while bytes.len() < end + length {
                        let mut b = [0; 8192];
                        let n = socket.read(&mut b).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        bytes.extend_from_slice(&b[..n]);
                    }
                    let raw = String::from_utf8_lossy(&bytes[end..end + length]).to_string();
                    let body = serde_json::from_str::<Value>(&raw).unwrap_or(json!({"raw":raw}));
                    let request =
                        json!({"line":request_line,"headers":headers,"body":body,"raw":raw});
                    let reply = handler(&request);
                    let _ = tx.send(request);
                    tokio::time::sleep(reply.delay).await;
                    let extras = reply
                        .headers
                        .iter()
                        .map(|(k, v)| format!("{k}: {v}\r\n"))
                        .collect::<String>();
                    let headers=format!("HTTP/1.1 {} Fixture\r\nContent-Type: {}\r\nContent-Length: {}\r\n{extras}Connection: close\r\n\r\n",reply.status,reply.content_type,reply.body.len());
                    if socket.write_all(headers.as_bytes()).await.is_err() {
                        return;
                    }
                    for chunk in reply.body.as_bytes().chunks(11) {
                        if socket.write_all(chunk).await.is_err() {
                            return;
                        }
                    }
                    let _ = socket.shutdown().await;
                };
                tokio::select! {_=cancel.cancelled()=>(),_=work=>()};
            });
        }
    });
    Server {
        url,
        requests: rx,
        stop,
        task,
    }
}
