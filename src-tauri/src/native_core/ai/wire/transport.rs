//! The only completion transport. Retries are scoped to one immutable request;
//! no tools execute here and visible text irrevocably disables retry.
use super::super::*;
use super::*;
use futures_util::StreamExt;
impl AiCore {
    pub(in crate::native_core::ai) async fn canonical_turn(
        &self,
        cfg: &Value,
        messages: &[Message],
        tools: &[Value],
        turn: usize,
        cancel: &Cancel,
        mut delta: impl FnMut(&str) + Send,
    ) -> Result<Turn, String> {
        let mut route = self.native_route(cfg, messages).await?;
        let mut encoded = encode(
            &route.origin,
            messages,
            tools,
            route.max_tokens,
            route.thinking,
        )?;
        if let Some(project) = &route.project {
            encoded = providers::antigravity::envelope(encoded, cfg, project)
        }
        let body = serde_json::to_vec(&encoded).map_err(err)?;
        let mut visible = false;
        for attempt in 0..=2 {
            if cancel.is_cancelled() {
                return Err("Operation cancelled".into());
            }
            let req = self
                .inner
                .http
                .post(route.url.clone())
                .headers(route.headers.clone())
                .header("Content-Type", "application/json")
                .body(body.clone());
            let result = async {
                let response = tokio::time::timeout(Duration::from_secs(30), req.send())
                    .await
                    .map_err(|_| ("Native network: header timeout".to_string(), true, None))?
                    .map_err(|e| {
                        (
                            format!(
                                "Native network: {}",
                                if e.is_connect() {
                                    "connection failed"
                                } else {
                                    "request failed"
                                }
                            ),
                            e.is_connect(),
                            None,
                        )
                    })?;
                let status = response.status().as_u16();
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .map(|s| s.min(60));
                if !response.status().is_success() {
                    let mut stream = response.bytes_stream();
                    let mut bytes = vec![];
                    while let Some(b) = tokio::time::timeout(Duration::from_secs(5), stream.next())
                        .await
                        .ok()
                        .flatten()
                    {
                        let Ok(b) = b else { break };
                        if bytes.len() + b.len() > 65536 {
                            break;
                        }
                        bytes.extend_from_slice(&b)
                    }
                    let detail = serde_json::from_slice::<Value>(&bytes)
                        .ok()
                        .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
                        .unwrap_or_default();
                    let mut detail = redact_text(&detail);
                    for name in ["authorization", "x-api-key", "x-goog-api-key"] {
                        if let Some(value) = route.headers.get(name).and_then(|v| v.to_str().ok()) {
                            let secret = value.strip_prefix("Bearer ").unwrap_or(value);
                            if !secret.is_empty() {
                                detail = detail.replace(secret, "[redacted]");
                            }
                        }
                    }
                    return Err((
                        format!(
                            "Native HTTP: status {status}{}",
                            if detail.is_empty() {
                                String::new()
                            } else {
                                format!(": {}", cut(&detail, 500))
                            }
                        ),
                        matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504),
                        retry_after,
                    ));
                }
                let json_body = response
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|v| v.contains("application/json"))
                    && route.origin.protocol != Protocol::Ollama;
                let mut stream = response.bytes_stream();
                let mut parser = Decoder::new(route.origin.clone(), turn);
                let mut bytes = vec![];
                let mut on = |t: &str| {
                    if !t.is_empty() {
                        visible = true;
                        delta(t)
                    }
                };
                loop {
                    match tokio::time::timeout(Duration::from_secs(90), stream.next())
                        .await
                        .map_err(|_| ("Native network: stream silent for 90s".into(), true, None))?
                    {
                        Some(b) => {
                            let b = b.map_err(|_| {
                                ("Native network: stream disconnected".into(), true, None)
                            })?;
                            if json_body {
                                if bytes.len() + b.len() > limits::STREAM_BYTES {
                                    return Err((failure("turn exceeds 8 MiB"), false, None));
                                }
                                bytes.extend_from_slice(&b)
                            } else {
                                parser
                                    .feed(&b, false, &mut on)
                                    .map_err(|e| (e, false, None))?;
                            }
                        }
                        None => {
                            if json_body {
                                let v = serde_json::from_slice(&bytes)
                                    .map_err(|_| (failure("invalid JSON response"), false, None))?;
                                parser.json(v, &mut on).map_err(|e| (e, false, None))?;
                            } else {
                                parser
                                    .feed(&[], true, &mut on)
                                    .map_err(|e| (e, false, None))?;
                            }
                            break;
                        }
                    }
                }
                parser.finish().map_err(|e| {
                    let retry = e.contains("before terminal signal");
                    (e, retry, None)
                })
            };
            let result = tokio::select! {biased;_=cancel.cancelled()=>return Err("Operation cancelled".into()),r=tokio::time::timeout(Duration::from_secs(600),result)=>r.unwrap_or_else(|_|Err(("Native network: turn exceeded 600s".into(),true,None)))};
            match result {
                Ok(t) => return Ok(t),
                Err((e, retry, delay)) => {
                    if !visible
                        && attempt == 0
                        && cfg["provider"] == "copilot"
                        && (e.starts_with("Native HTTP: status 401")
                            || e.starts_with("Native HTTP: status 403"))
                    {
                        let github = self.copilot_token().await?;
                        let session = self.exchange_copilot_session(&github).await?;
                        let mut url = reqwest::Url::parse(&session.base)
                            .map_err(|_| "Copilot returned an invalid API base")?;
                        url.set_path(route.url.path());
                        url.set_query(route.url.query());
                        route.url = url;
                        route.headers.insert(
                            reqwest::header::AUTHORIZATION,
                            reqwest::header::HeaderValue::from_str(&format!(
                                "Bearer {}",
                                session.token
                            ))
                            .map_err(|_| "Copilot returned an invalid session token")?,
                        );
                        continue;
                    }
                    if attempt == 2 || !retry || visible {
                        return Err(e);
                    }
                    tokio::select! {biased;_=cancel.cancelled()=>return Err("Operation cancelled".into()),_=tokio::time::sleep(Duration::from_secs(delay.unwrap_or(if attempt==0{1}else{3})))=>{}}
                }
            }
        }
        unreachable!()
    }
}
