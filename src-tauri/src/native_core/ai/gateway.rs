//! Small bounded, authenticated loopback HTTP adapter for the external Pi policy.
use super::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
pub struct Gateway {
    pub endpoint: String,
    pub token: String,
    pub cancel: process::Cancel,
    pub task: tokio::task::JoinHandle<()>,
}
impl Gateway {
    pub async fn start(ctx: CoreContext) -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0").await.map_err(err)?;
        let endpoint = format!("http://{}/search", listener.local_addr().map_err(err)?);
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let cancel = process::Cancel::new();
        let stop = cancel.clone();
        let auth = token.clone();
        let task = tokio::spawn(async move {
            let semaphore = Arc::new(tokio::sync::Semaphore::new(8));
            let mut clients = tokio::task::JoinSet::new();
            loop {
                tokio::select! {_=stop.cancelled()=>break,client=listener.accept()=>{let Ok((socket,_))=client else{break};let Ok(permit)=semaphore.clone().try_acquire_owned()else{drop(socket);continue};let ctx=ctx.clone();let auth=auth.clone();clients.spawn(async move{let _permit=permit;let _=tokio::time::timeout(Duration::from_secs(30),serve(socket,ctx,&auth)).await;});},Some(_)=clients.join_next(),if !clients.is_empty()=>{}}
            }
            clients.abort_all();
            while clients.join_next().await.is_some() {}
        });
        Ok(Self {
            endpoint,
            token,
            cancel,
            task,
        })
    }
}
async fn serve(mut socket: TcpStream, ctx: CoreContext, token: &str) -> Result<(), String> {
    let mut data = Vec::new();
    let header_end;
    loop {
        let mut buf = [0u8; 4096];
        let n = socket.read(&mut buf).await.map_err(err)?;
        if n == 0 {
            return Err("Incomplete HTTP request".into());
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(i) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            header_end = i + 4;
            break;
        }
        if data.len() > 8192 {
            return reply(
                &mut socket,
                413,
                json!({"error":"Request headers too large"}),
            )
            .await;
        }
    }
    let header = std::str::from_utf8(&data[..header_end]).map_err(err)?;
    let mut lines = header.split("\r\n");
    let first = lines.next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("").to_string();
    let mut auth = false;
    let mut length = None;
    let mut chunked = false;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            if k.eq_ignore_ascii_case("authorization") {
                auth = v.trim() == format!("Bearer {token}");
            }
            if k.eq_ignore_ascii_case("content-length") {
                if length.is_some() {
                    return reply(
                        &mut socket,
                        400,
                        json!({"error":"Duplicate content length"}),
                    )
                    .await;
                }
                length = v.trim().parse::<usize>().ok();
            }
            if k.eq_ignore_ascii_case("transfer-encoding") {
                chunked = true;
            }
        }
    }
    if method != "POST" || !auth {
        return reply(&mut socket, 404, json!({"error":"Not found"})).await;
    }
    let Some(length) = length else {
        return reply(&mut socket, 400, json!({"error":"Content length required"})).await;
    };
    if length > 65536 || chunked {
        return reply(
            &mut socket,
            413,
            json!({"error":"Request is too large or unsupported framing"}),
        )
        .await;
    }
    while data.len() - header_end < length {
        let mut buf = [0u8; 4096];
        let remaining = length - (data.len() - header_end);
        let n = socket
            .read(&mut buf[..remaining.min(4096)])
            .await
            .map_err(err)?;
        if n == 0 {
            return Err("Incomplete HTTP body".into());
        }
        data.extend_from_slice(&buf[..n]);
    }
    let result: CoreResult = async {
        let v: Value =
            serde_json::from_slice(&data[header_end..header_end + length]).map_err(err)?;
        match path.as_str() {
            "/search" => {
                let query = s(&v["query"]).trim();
                if query.is_empty() {
                    return Err("query is required".into());
                }
                let hits = ctx
                    .request(
                        "core:knowledge:search",
                        vec![
                            json!(query),
                            json!(v["limit"].as_u64().unwrap_or(8).clamp(1, 20)),
                        ],
                    )
                    .await?;
                Ok(json!({"hits":hits}))
            }
            "/read" => {
                let id = s(&v["resultId"]).trim();
                if id.is_empty() {
                    return Err("resultId is required".into());
                }
                let result = ctx
                    .request(
                        "core:knowledge:read",
                        vec![
                            json!(id),
                            json!(v["maxChars"].as_u64().unwrap_or(12000).clamp(500, 50000)),
                        ],
                    )
                    .await?;
                Ok(json!({"result":result}))
            }
            "/launcher-search" => {
                let query = s(&v["query"]).trim();
                if query.is_empty() {
                    return Err("query is required".into());
                }
                let raw = ctx.request("search:all", vec![json!(query)]).await?;
                let results = raw
                    .as_array()
                    .ok_or("Invalid launcher search response")?
                    .iter()
                    .filter(|r| {
                        !s(&r["id"]).starts_with("deep-search:")
                            && !s(&r["id"]).starts_with("note-add:")
                    })
                    .take(v["limit"].as_u64().unwrap_or(10).clamp(1, 20) as usize)
                    .map(|r| {
                        let mut result = json!({});
                        for key in ["id", "title", "subtitle", "category", "score"] {
                            if let Some(v) = r.get(key) {
                                result[key] = v.clone();
                            }
                        }
                        let action = &r["action"];
                        let target = match s(&action["type"]) {
                            "open-file" | "open-with-app" => action["path"].clone(),
                            "open-app" => {
                                action.get("appPath").unwrap_or(&action["appName"]).clone()
                            }
                            "open-url" => action["url"].clone(),
                            "run-extension-command" => json!(format!(
                                "{}/{}",
                                s(&action["extensionId"]),
                                s(&action["commandName"])
                            )),
                            "invoke-command" | "run-native-command" => action["commandId"].clone(),
                            "run-shell" => action["command"].clone(),
                            _ => Value::Null,
                        };
                        if !target.is_null() {
                            result["target"] = target;
                        }
                        result
                    })
                    .collect::<Vec<_>>();
                Ok(json!({"results":results}))
            }
            _ => Err("Not found".into()),
        }
    }
    .await;
    match result {
        Ok(v) if path == "/read" && v["result"].is_null() => {
            reply(
                &mut socket,
                404,
                json!({"error":"Knowledge result was not found"}),
            )
            .await
        }
        Ok(v) => reply(&mut socket, 200, v).await,
        Err(e) => {
            reply(
                &mut socket,
                if e == "Not found" { 404 } else { 400 },
                json!({"error":e}),
            )
            .await
        }
    }
}
async fn reply(socket: &mut TcpStream, status: u16, value: Value) -> Result<(), String> {
    let body = value.to_string();
    let header=format!("HTTP/1.1 {status} {}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",match status{200=>"OK",404=>"Not Found",413=>"Payload Too Large",_=>"Bad Request"},body.len());
    socket.write_all(header.as_bytes()).await.map_err(err)?;
    socket.write_all(body.as_bytes()).await.map_err(err)?;
    socket.shutdown().await.map_err(err)
}
