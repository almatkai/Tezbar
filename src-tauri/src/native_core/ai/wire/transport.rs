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
        let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
        let mut route = tokio::select! {biased;_=cancel.cancelled()=>return Err("Operation cancelled".into()),r=self.native_route(cfg,messages)=>r?};
        let budget = route.request_budget;
        let mut view = messages.to_vec();
        if limits::project_images_budget(&route.origin.protocol, &mut view, budget) {
            if let Some(run) = cfg["runId"].as_str() {
                self.agent_event(run,json!({"type":"log","source":"native","line":"Earlier images omitted to fit the selected provider limits. Original history images are retained."}));
            }
        }
        let messages = view.as_slice();
        limits::validate_images_budget(&route.origin.protocol, messages, budget)?;
        let mut encoded = encode(
            &route.origin,
            messages,
            tools,
            route.max_tokens,
            route.thinking,
        )?;
        if route.origin.protocol == Protocol::OpenaiChat && route.requires_reasoning_content {
            for m in encoded["messages"].as_array_mut().into_iter().flatten() {
                if m["role"] == "assistant" && m["reasoning_content"].is_null() {
                    m["reasoning_content"] = json!("");
                }
            }
        }
        if route.origin.protocol == Protocol::OpenaiChat && route.official_openai {
            if let Some(tokens) = encoded.as_object_mut().unwrap().remove("max_tokens") {
                encoded["max_completion_tokens"] = tokens;
            }
        }
        if let Some(project) = &route.project {
            let mut options = cfg.clone();
            options["_requestIndex"] =
                json!(messages.iter().filter(|m| m.role == "assistant").count());
            encoded = providers::antigravity::envelope(encoded, &options, project)
        }
        let mut body = serde_json::to_vec(&encoded).map_err(err)?;
        if body.len() > budget {
            return Err(format!(
                "Native limit: {:?} request exceeds {} bytes including images",
                route.origin.protocol, budget
            ));
        }
        let ag = cfg["provider"] == "antigravity";
        let ag_bases = if ag {
            self.antigravity_bases(cfg)?
        } else {
            vec![]
        };
        let mut ag_endpoint = 0;
        let mut tried_accounts = std::collections::HashSet::new();
        if ag {
            for account in self
                .antigravity_accounts()
                .await?
                .as_array()
                .into_iter()
                .flatten()
                .filter(|a| a["active"] == true)
            {
                tried_accounts.insert(s(&account["accountId"]).to_string());
            }
        }
        let mut visible = false;
        let run = cfg["runId"].as_str();
        let mut retry_attempt = 0;
        let mut fallback_attempt = 0;
        let mut account_attempt = 0;
        let mut copilot_exchanged = false;
        let mut include_fallback = false;
        for _request in 0..24 {
            let attempt = retry_attempt;
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
            let mut thinking_stage = None;
            let secrets = route.secrets.clone();
            let mut redactor = super::redact::Redactor::new(secrets);

            let result = async {
                let response = tokio::time::timeout(header_wait(cfg, &route.url), req.send())
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
                        .and_then(|v| {
                            v["error"]["message"]
                                .as_str()
                                .or(v["error"].as_str())
                                .or(v["message"].as_str())
                                .or(v["detail"].as_str())
                                .map(str::to_string)
                        })
                        .unwrap_or_default();
                    let mut detail = redact_text(&detail);
                    for secret in &route.secrets {
                        if !secret.is_empty() {
                            detail = detail.replace(secret, "[redacted]");
                        }
                    }
                    return Err((
                        format!(
                            "Native HTTP: status {status}{}",
                            if detail.is_empty() {
                                String::new()
                            } else {
                                format!(
                                    ": {}{}",
                                    cut(&provider::format_error(&detail, "Native"), 500),
                                    if cfg["provider"] == "copilot"
                                        && regex::Regex::new(
                                            "(?i)not.enabled|not.supported|unavailable"
                                        )
                                        .unwrap()
                                        .is_match(&detail)
                                    {
                                        ". Check model access in your Copilot settings."
                                    } else {
                                        ""
                                    }
                                )
                            }
                        ),
                        matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504 | 529)
                            || ag && matches!(status, 403 | 404),
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
                let namespace = cfg["_callNamespace"]
                    .as_str()
                    .map(|namespace| format!("{namespace}_{turn}"))
                    .unwrap_or_else(|| turn.to_string());
                let mut parser = Decoder::new(route.origin.clone(), namespace);
                let mut bytes = vec![];
                let mut on = |text: &str| {
                    let clean = redactor.push(text, false);
                    if !clean.is_empty() {
                        visible = true;
                        delta(&clean)
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
                                parser.feed(&b, false, &mut on).map_err(|e| {
                                    let retry =
                                        e.starts_with("Native model temporarily unavailable");
                                    (e, retry, None)
                                })?;
                                if parser.has_thinking() && thinking_stage.is_none() {
                                    if let Some(run) = run {
                                        let index = self.stage_index(run);
                                        self.agent_event(run,json!({"type":"stage","stage":{"index":index,"label":"Thinking…","status":"running"}}));
                                        thinking_stage = Some(index);
                                    }
                                }
                            }
                        }
                        None => {
                            if json_body {
                                let v = serde_json::from_slice(&bytes)
                                    .map_err(|_| (failure("invalid JSON response"), false, None))?;
                                parser.json(v, &mut on).map_err(|e| {
                                    let retry =
                                        e.starts_with("Native model temporarily unavailable");
                                    (e, retry, None)
                                })?;
                            } else {
                                parser.feed(&[], true, &mut on).map_err(|e| {
                                    let retry =
                                        e.starts_with("Native model temporarily unavailable");
                                    (e, retry, None)
                                })?;
                            }
                            break;
                        }
                    }
                }
                let mut turn = parser.finish().map_err(|e| {
                    let retry = e.contains("before terminal signal")
                        || ag && e == "Native model: empty final answer";
                    (e, retry, None)
                })?;
                drop(on);
                if turn
                    .calls
                    .iter()
                    .any(|c| redactor.contains_secret(s(&c["function"]["arguments"])))
                {
                    return Err(("Native validation: tool arguments contained a provider credential; no tools executed".into(),false,None));
                }
                if turn
                    .message
                    .opaque
                    .as_ref()
                    .is_some_and(|o| opaque_has_secret(&o.value, "", &redactor))
                {
                    return Err(("Native validation: provider returned credentials in opaque history; the response was not saved or replayed".into(),false,None));
                }
                let tail = redactor.push("", true);
                if !tail.is_empty() {
                    visible = true;
                    delta(&tail)
                }
                turn.text = redactor.clean(&turn.text);
                for block in &mut turn.message.blocks {
                    if let runtime::transcript::Block::Text { text } = block {
                        *text = redactor.clean(text)
                    }
                }
                Ok(turn)
            };
            let result = tokio::select! {biased;_=cancel.cancelled()=>return Err("Operation cancelled".into()),r=tokio::time::timeout_at(deadline,result)=>r.unwrap_or_else(|_|Err(("Native network: turn exceeded 600s".into(),true,None)))};
            if let (Some(run), Some(index)) = (run, thinking_stage) {
                self.agent_event(run,json!({"type":"stage","stage":{"index":index,"label":"Thinking…","status":if result.is_ok(){"done"}else{"failed"}}}));
            }
            match result {
                Ok(t) => return Ok(t),
                Err((e, retry, delay)) => {
                    if !visible
                        && !include_fallback
                        && route.origin.protocol == Protocol::OpenaiResponses
                        && encoded.get("include").is_some()
                        && e.starts_with("Native HTTP: status 400")
                        && e.to_ascii_lowercase().contains("include")
                        && regex::Regex::new(
                            "(?i)unsupported|not supported|unknown|unrecognized|not allowed",
                        )
                        .unwrap()
                        .is_match(&e)
                    {
                        include_fallback = true;
                        encoded.as_object_mut().unwrap().remove("include");
                        body = serde_json::to_vec(&encoded).map_err(err)?;
                        continue;
                    }
                    if cfg["provider"] == "opencode" && e.starts_with("Native HTTP: status 403") {
                        return Err(format!("{e}. Free-tier chat models require OpenCode. Choose a supported paid model or check your OpenCode/Go access in AI settings."));
                    }
                    if ag && !visible {
                        if e.starts_with("Native HTTP: status 429")
                            && providers::antigravity::quota_wall(&e)
                            && account_attempt < 2
                        {
                            account_attempt += 1;
                            let account = self.antigravity_next_account(&tried_accounts).await?;
                            tried_accounts.insert(s(&account["accountId"]).to_string());
                            route.secrets.push(s(&account["access"]).to_string());
                            route.headers.insert(
                                reqwest::header::AUTHORIZATION,
                                reqwest::header::HeaderValue::from_str(&format!(
                                    "Bearer {}",
                                    s(&account["access"])
                                ))
                                .map_err(|_| "Invalid Antigravity account credential")?,
                            );
                            encoded["project"] = account["projectId"].clone();
                            body = serde_json::to_vec(&encoded).map_err(err)?;
                            continue;
                        }
                        if (e.starts_with("Native HTTP: status 404")
                            || e.starts_with("Native HTTP: status 400"))
                            && fallback_attempt < 2
                            && regex::Regex::new(
                                "(?i)model.*(?:unavailable|not found|unsupported|not supported)",
                            )
                            .unwrap()
                            .is_match(&e)
                        {
                            if let Some(model) = providers::antigravity::fallback_model(
                                &route.origin.model,
                                cfg["reasoningEffort"].as_str().unwrap_or("off"),
                            ) {
                                fallback_attempt += 1;
                                ag_endpoint = 0;
                                route.url = reqwest::Url::parse(&format!(
                                    "{}/v1internal:streamGenerateContent?alt=sse",
                                    ag_bases[0]
                                ))
                                .map_err(|_| "Invalid Antigravity endpoint")?;
                                route.origin.model = model.clone();
                                let mut options = cfg.clone();
                                options["model"] = json!(model);
                                options["_requestIndex"] = json!(messages
                                    .iter()
                                    .filter(|m| m.role == "assistant")
                                    .count());
                                encoded = providers::antigravity::envelope(
                                    encode(
                                        &route.origin,
                                        messages,
                                        tools,
                                        route.max_tokens,
                                        route.thinking,
                                    )?,
                                    &options,
                                    s(&encoded["project"]),
                                );
                                body = serde_json::to_vec(&encoded).map_err(err)?;
                                continue;
                            }
                        }
                        if [403, 404, 429, 500, 502, 503, 504]
                            .iter()
                            .any(|status| e.starts_with(&format!("Native HTTP: status {status}")))
                            && ag_endpoint + 1 < ag_bases.len()
                        {
                            ag_endpoint += 1;
                            route.url = reqwest::Url::parse(&format!(
                                "{}/v1internal:streamGenerateContent?alt=sse",
                                ag_bases[ag_endpoint]
                            ))
                            .map_err(|_| "Invalid Antigravity endpoint")?;
                            continue;
                        }
                    }
                    if cfg["provider"] == "copilot"
                        && (e.starts_with("Native HTTP: status 401")
                            || e.starts_with("Native HTTP: status 403"))
                    {
                        let github = self.copilot_token().await?;
                        self.clear_copilot_session(&github).await;
                    }
                    if !visible
                        && !copilot_exchanged
                        && cfg["provider"] == "copilot"
                        && (e.starts_with("Native HTTP: status 401")
                            || e.starts_with("Native HTTP: status 403"))
                    {
                        copilot_exchanged = true;
                        let github = self.copilot_token().await?;
                        self.clear_copilot_session(&github).await;
                        let session = self.exchange_copilot_session(&github).await?;
                        let mut url = reqwest::Url::parse(if cfg["_explicitBaseURL"] == true {
                            route.url.as_str()
                        } else {
                            &session.base
                        })
                        .map_err(|_| "Copilot returned an invalid API base")?;
                        url.set_path(route.url.path());
                        url.set_query(route.url.query());
                        route.url = url;
                        route.secrets.push(session.token.clone());
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
                    let mut e = redact_text(&e);
                    for secret in &route.secrets {
                        if !secret.is_empty() {
                            e = e.replace(secret, "[redacted]");
                        }
                    }
                    if retry_attempt == 2 || !retry || visible {
                        return Err(e);
                    }
                    let stage=run.map(|run|{let index=self.stage_index(run);self.agent_event(run,json!({"type":"stage","stage":{"index":index,"label":format!("Retrying ({}/2)",attempt+1),"status":"running"}}));index});
                    retry_attempt += 1;
                    tokio::select! {biased;_=cancel.cancelled()=>return Err("Operation cancelled".into()),_=tokio::time::sleep_until(deadline)=>return Err("Native network: turn exceeded 600s".into()),_=tokio::time::sleep(Duration::from_secs(delay.unwrap_or(if attempt==0{1}else{3})))=>{}}
                    if let (Some(run), Some(index)) = (run, stage) {
                        self.agent_event(run,json!({"type":"stage","stage":{"index":index,"label":format!("Retrying ({}/2)",attempt+1),"status":"done"}}));
                    }
                }
            }
        }
        Err("Native limit: exhausted provider fallback budget".into())
    }
}

fn opaque_has_secret(value: &Value, key: &str, redactor: &super::redact::Redactor) -> bool {
    match value {
        Value::String(text) => redactor.contains_secret(text),
        Value::Array(items) => items.iter().any(|v| opaque_has_secret(v, key, redactor)),
        Value::Object(fields) => fields
            .iter()
            .any(|(key, value)| opaque_has_secret(value, key, redactor)),
        _ => false,
    }
}

fn header_wait(cfg: &Value, url: &reqwest::Url) -> Duration {
    let local = cfg["provider"] == "ollama"
        || matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    Duration::from_secs(
        cfg["headerTimeoutSeconds"]
            .as_u64()
            .filter(|n| *n > 0)
            .unwrap_or(if local { 120 } else { 30 })
            .min(300),
    )
}

#[cfg(test)]
mod repair_tests {
    use super::*;
    #[tokio::test(start_paused = true)]
    async fn local_header_wait_defaults_to_120_seconds_and_is_bounded() {
        let local = reqwest::Url::parse("http://127.0.0.1:1234/chat").unwrap();
        let remote = reqwest::Url::parse("https://example.invalid/chat").unwrap();
        assert_eq!(header_wait(&json!({}), &remote).as_secs(), 30);
        assert_eq!(
            header_wait(&json!({"provider":"ollama"}), &remote).as_secs(),
            120
        );
        assert_eq!(
            header_wait(&json!({"headerTimeoutSeconds":999}), &local).as_secs(),
            300
        );
        let timer =
            tokio::spawn(async move { tokio::time::sleep(header_wait(&json!({}), &local)).await });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(119)).await;
        assert!(!timer.is_finished());
        tokio::time::advance(Duration::from_secs(1)).await;
        timer.await.unwrap();
    }
}
