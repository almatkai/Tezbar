//! End-to-end fixtures cross the same agent:run / agent:event seam as the UI.
//! All profiles, files, HTTP endpoints and Pi overrides are local test resources.
use super::tests::{fixture, next_event};
use super::*;
use std::collections::VecDeque;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::mpsc,
};
struct Reply {
    body: String,
    status: u16,
    hold_headers: bool,
    hold_body: bool,
}
impl Reply {
    fn sse(body: String) -> Self {
        Self {
            body,
            status: 200,
            hold_headers: false,
            hold_body: false,
        }
    }
    fn hold(headers: bool, body: String) -> Self {
        Self {
            body,
            status: 200,
            hold_headers: headers,
            hold_body: !headers,
        }
    }
}
struct Server {
    url: String,
    requests: mpsc::UnboundedReceiver<Value>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn server(replies: Vec<Reply>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        let mut queue: VecDeque<_> = replies.into();
        let mut clients = tokio::task::JoinSet::new();
        // Keep accepting after the scripted replies to detect unintended retries.
        loop {
            tokio::select! {
                accepted=listener.accept()=>{
                    let Ok((mut socket,_))=accepted else{break};
                    let reply=queue.pop_front().unwrap_or(Reply{body:String::new(),status:500,hold_headers:false,hold_body:false});
                    let tx=tx.clone();
                    clients.spawn(async move {
                        let mut data=Vec::new();let end;
                        loop {
                            let mut b=[0;4096];let n=socket.read(&mut b).await.unwrap_or(0);if n==0{return;}
                            data.extend_from_slice(&b[..n]);
                            if let Some(i)=data.windows(4).position(|w|w==b"\r\n\r\n"){end=i+4;break;}
                            assert!(data.len()<16384);
                        }
                        let header=String::from_utf8_lossy(&data[..end]);
                        assert!(header.starts_with("POST /v1/chat/completions "));
                        let length=header.lines().find_map(|l|l.to_lowercase().strip_prefix("content-length:").map(|v|v.trim().parse::<usize>().unwrap())).unwrap();
                        while data.len()-end<length {let mut b=[0;4096];let n=socket.read(&mut b).await.unwrap_or(0);if n==0{return;}data.extend_from_slice(&b[..n]);}
                        tx.send(serde_json::from_slice(&data[end..end+length]).unwrap()).unwrap();
                        if reply.hold_headers {std::future::pending::<()>().await;}
                        let length=if reply.hold_body{reply.body.len()+100}else{reply.body.len()};
                        let h=format!("HTTP/1.1 {} Fixture\r\nContent-Type: text/event-stream\r\nContent-Length: {length}\r\nRetry-After: 2\r\nConnection: close\r\n\r\n",reply.status);
                        let _=socket.write_all(h.as_bytes()).await;
                        let _=socket.write_all(reply.body.as_bytes()).await;
                        if reply.hold_body {std::future::pending::<()>().await;}
                    });
                },Some(_)=clients.join_next(),if !clients.is_empty()=>{}
            }
        }
    });
    Server {
        url,
        requests: rx,
        task,
    }
}
fn data(v: Value) -> String {
    format!("data: {v}\n\n")
}
fn text(t: &str) -> String {
    format!(
        "{}{}data: [DONE]\n\n",
        data(json!({"choices":[{"index":0,"delta":{"content":t}}]})),
        data(json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}))
    )
}
fn call(id: &str, name: &str, args: Value) -> Value {
    json!({"id":id,"type":"function","function":{"name":name,"arguments":args.to_string()}})
}
fn calls(calls: Vec<Value>) -> String {
    let indexed: Vec<_> = calls
        .into_iter()
        .enumerate()
        .map(|(i, mut c)| {
            c["index"] = json!(i);
            c
        })
        .collect();
    format!(
        "{}{}data: [DONE]\n\n",
        data(json!({"choices":[{"index":0,"delta":{"tool_calls":indexed}}]})),
        data(json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}))
    )
}
async fn configure(core: &AiCore, url: &str, patch: Value) {
    let mut cfg = json!({"provider":"openai-compatible","model":"fixture-model","openaiCompatibleBaseURL":url,"agentRuntime":"native"});
    for (k, v) in patch.as_object().unwrap() {
        cfg[k] = v.clone();
    }
    core.handle("llm-config-set", &[cfg])
        .await
        .unwrap()
        .unwrap();
    *core.inner.pi_override.lock().unwrap() =
        Some(core.inner.ctx.paths.temp_dir.join("PI-MUST-NOT-BE-INVOKED"));
}
async fn start(core: &AiCore, id: &str, cwd: &std::path::Path) {
    let result=core.handle("agent:run",&[json!({"runId":id,"task":"Prior conversation: User: remember 🦀\nNew user message: perform fixture task","cwd":cwd})]).await.unwrap().unwrap();
    assert_eq!(result["ok"], true);
}
async fn approve(core: &AiCore, event: &Value, decision: &str) -> Value {
    core.handle(
        "agent:approve",
        &[json!({"runId":event["runId"],"approvalId":event["approvalId"],"decision":decision})],
    )
    .await
    .unwrap()
    .unwrap()
}
async fn request(s: &mut Server) -> Value {
    tokio::time::timeout(Duration::from_secs(5), s.requests.recv())
        .await
        .unwrap()
        .unwrap()
}
async fn no_extra(s: &mut Server) {
    assert!(
        tokio::time::timeout(Duration::from_millis(80), s.requests.recv())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn native_write_read_answer_preserves_ids_schema_history_and_never_needs_pi() {
    let (_dir, core, mut events) = fixture();
    let root = core.inner.ctx.paths.home_dir.clone();
    let mut http = server(vec![
        Reply::sse(calls(vec![call(
            "write_1",
            "write",
            json!({"path":"note.txt","content":"hello 🦀"}),
        )])),
        Reply::sse(calls(vec![call(
            "read_2",
            "read",
            json!({"path":"note.txt"}),
        )])),
        Reply::sse(text("Saved and verified 🦀")),
    ])
    .await;
    configure(&core, &http.url, json!({"agentRuntime":"auto"})).await;
    start(&core, "agent-roundtrip", &root).await;
    let first = request(&mut http).await;
    assert_eq!(first["model"], "fixture-model");
    assert_eq!(first["stream"], true);
    assert!(s(&first["messages"][1]["content"]).contains("remember 🦀"));
    for t in first["tools"].as_array().unwrap() {
        assert_eq!(t["type"], "function");
        assert_eq!(t["function"]["parameters"]["additionalProperties"], false);
        assert!(t["function"]["parameters"]["required"].is_array());
    }
    let approval = next_event(&mut events, "approval").await;
    assert!(!root.join("note.txt").exists());
    assert_eq!(approve(&core, &approval, "once").await["ok"], true);
    let second = request(&mut http).await;
    assert_eq!(second["messages"][2]["role"], "assistant");
    assert_eq!(second["messages"][2]["tool_calls"][0]["id"], "write_1");
    assert_eq!(second["messages"][3]["tool_call_id"], "write_1");
    let third = request(&mut http).await;
    assert_eq!(third["messages"][4]["tool_calls"][0]["id"], "read_2");
    assert_eq!(third["messages"][5]["tool_call_id"], "read_2");
    assert!(s(&third["messages"][5]["content"]).contains("hello 🦀"));
    assert_eq!(
        next_event(&mut events, "answer").await["text"],
        "Saved and verified 🦀"
    );
    next_event(&mut events, "done").await;
    assert_eq!(
        std::fs::read_to_string(root.join("note.txt")).unwrap(),
        "hello 🦀"
    );
    assert!(core.inner.gateway.lock().await.is_none());
    no_extra(&mut http).await;
    core.shutdown().await;
}

#[tokio::test]
#[cfg(unix)]
async fn native_side_effect_is_one_when_followup_http_fails_and_duplicate_ids_never_repeat_it() {
    for duplicate in [false, true] {
        let (_dir, core, mut events) = fixture();
        let root = core.inner.ctx.paths.home_dir.clone();
        let c = call("counter", "bash", json!({"command":"printf x >> counter"}));
        let next = if duplicate {
            Reply::sse(calls(vec![c.clone()]))
        } else {
            Reply {
                body: "outage".into(),
                status: 503,
                hold_headers: false,
                hold_body: false,
            }
        };
        let mut http = server(vec![Reply::sse(calls(vec![c])), next]).await;
        configure(&core, &http.url, json!({})).await;
        start(&core, "agent-counter", &root).await;
        let a = next_event(&mut events, "approval").await;
        approve(&core, &a, "once").await;
        let error = next_event(&mut events, "error").await;
        assert!(s(&error["message"]).contains(if duplicate {
            "duplicate call ID"
        } else {
            "Native HTTP: status 503"
        }));
        next_event(&mut events, "done").await;
        assert_eq!(std::fs::read_to_string(root.join("counter")).unwrap(), "x");
        request(&mut http).await;
        let followup = request(&mut http).await;
        assert_eq!(followup["messages"][3]["tool_call_id"], "counter");
        no_extra(&mut http).await;
        core.shutdown().await;
    }
}

#[tokio::test]
async fn native_rejects_whole_malformed_batches_before_any_approval_or_effect() {
    let good = call(
        "good",
        "write",
        json!({"path":"must-not-exist","content":"bad"}),
    );
    let mut invalid_json = call("bad", "write", json!({}));
    invalid_json["function"]["arguments"] = json!("{\"path\":");
    let mut oversize = call("bad", "write", json!({}));
    oversize["function"]["arguments"] = json!("x".repeat(65537));
    let valid = calls(vec![good.clone()]);
    let cases = vec![
        calls(vec![good.clone(), call("bad", "unknown", json!({}))]),
        calls(vec![good.clone(), invalid_json]),
        calls(vec![good.clone(), oversize]),
        calls(vec![good.clone(), good.clone()]),
        calls(vec![
            good.clone(),
            call("bad", "read", json!({"path":"/etc/hosts"})),
        ]),
        calls(vec![
            good.clone(),
            call("bad", "write", json!({"path":"../outside","content":"bad"})),
        ]),
        calls(vec![
            good.clone(),
            call(
                "bad",
                "write",
                json!({"path":"b","content":"b","extra":true}),
            ),
        ]),
        valid.replace("data: [DONE]\n\n", ""),
        valid.replace(
            "data: [DONE]\n\n",
            &data(json!({"error":{"message":"failure"}})),
        ),
        format!(
            "{}data: [DONE]\n\n",
            data(
                json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"x","function":{"name":"write","arguments":"{}"}}]}}]})
            )
        ),
        data(
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"x","function":{"name":"write","arguments":"{"}}]}}]}),
        ),
    ];
    for body in cases {
        let (_dir, core, mut events) = fixture();
        let root = core.inner.ctx.paths.home_dir.clone();
        let mut http = server(vec![Reply::sse(body)]).await;
        configure(&core, &http.url, json!({})).await;
        start(&core, "agent-malformed", &root).await;
        let mut error = false;
        loop {
            let (_, e) = tokio::time::timeout(Duration::from_secs(5), events.recv())
                .await
                .unwrap()
                .unwrap();
            assert_ne!(e["type"], "approval");
            if e["type"] == "error" {
                error = true;
            }
            if e["type"] == "done" {
                break;
            }
        }
        assert!(error);
        assert!(!root.join("must-not-exist").exists());
        request(&mut http).await;
        no_extra(&mut http).await;
        core.shutdown().await;
    }
}

#[tokio::test]
async fn native_edit_ambiguity_rejects_an_earlier_write_in_same_batch() {
    let (_dir, core, mut events) = fixture();
    let root = core.inner.ctx.paths.home_dir.clone();
    std::fs::write(root.join("ambiguous"), "aaa").unwrap();
    let _http = server(vec![Reply::sse(calls(vec![
        call("w", "write", json!({"path":"new","content":"bad"})),
        call(
            "e",
            "edit",
            json!({"path":"ambiguous","oldText":"aa","newText":"b"}),
        ),
    ]))])
    .await;
    configure(&core, &_http.url, json!({})).await;
    start(&core, "agent-edit", &root).await;
    assert!(s(&next_event(&mut events, "error").await["message"]).contains("ambiguous"));
    next_event(&mut events, "done").await;
    assert!(!root.join("new").exists());
    assert_eq!(
        std::fs::read_to_string(root.join("ambiguous")).unwrap(),
        "aaa"
    );
    core.shutdown().await;
}

#[tokio::test]
async fn native_review_remembered_destructive_and_complex_commands_cannot_bypass_prompts() {
    for (mode, command) in [
        ("review", "pwd"),
        ("turbo", "rm -f victim"),
        ("auto", "pwd && rm -f victim"),
        ("default", "printf unknown"),
        ("turbo", "pwd; printf complex"),
    ] {
        let (_dir, core, mut events) = fixture();
        let root = core.inner.ctx.paths.home_dir.clone();
        std::fs::write(root.join("victim"), "keep").unwrap();
        let mut http = server(vec![
            Reply::sse(calls(vec![call("p", "bash", json!({"command":command}))])),
            Reply::sse(text("Denied")),
        ])
        .await;
        configure(&core,&http.url,json!({"agentApprovalMode":mode,"agentAlwaysAllowedExactCommands":[command],"agentAlwaysAllowedCommands":["pwd","rm","printf"]})).await;
        start(&core, "agent-policy", &root).await;
        let a = next_event(&mut events, "approval").await;
        assert_eq!(a["command"], command);
        approve(&core, &a, "deny").await;
        next_event(&mut events, "answer").await;
        next_event(&mut events, "done").await;
        request(&mut http).await;
        let r = request(&mut http).await;
        assert!(s(&r["messages"][3]["content"]).contains("Native permission"));
        assert!(root.join("victim").exists());
        core.shutdown().await;
    }
}

#[tokio::test]
async fn native_concurrent_approval_ownership_cancel_and_stale_responses() {
    let (_dir, core, mut events) = fixture();
    let aroot = core.inner.ctx.paths.home_dir.join("a");
    let broot = core.inner.ctx.paths.home_dir.join("b");
    std::fs::create_dir_all(&aroot).unwrap();
    std::fs::create_dir_all(&broot).unwrap();
    let c = calls(vec![call(
        "shared",
        "write",
        json!({"path":"owned","content":"second"}),
    )]);
    let mut http = server(vec![
        Reply::sse(c.clone()),
        Reply::sse(c),
        Reply::sse(text("second complete")),
    ])
    .await;
    configure(&core, &http.url, json!({})).await;
    start(&core, "agent-owner-a", &aroot).await;
    let a = next_event(&mut events, "approval").await;
    start(&core, "agent-owner-b", &broot).await;
    let b = next_event(&mut events, "approval").await;
    let mut wrong = a.clone();
    wrong["runId"] = b["runId"].clone();
    assert_eq!(approve(&core, &wrong, "once").await["ok"], false);
    core.handle("agent:cancel", &[json!("agent-owner-a")])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(approve(&core, &a, "always").await["ok"], false);
    assert_eq!(approve(&core, &b, "once").await["ok"], true);
    let answer = next_event(&mut events, "answer").await;
    assert_eq!(answer["runId"], "agent-owner-b");
    next_event(&mut events, "done").await;
    assert!(!aroot.join("owned").exists());
    assert!(broot.join("owned").exists());
    request(&mut http).await;
    request(&mut http).await;
    request(&mut http).await;
    core.shutdown().await;
}

#[tokio::test]
async fn native_cancellation_during_headers_body_and_pending_tools_has_zero_effects() {
    for headers in [true, false] {
        let (_dir, core, mut events) = fixture();
        let root = core.inner.ctx.paths.home_dir.clone();
        let partial = calls(vec![call(
            "x",
            "write",
            json!({"path":"cancelled","content":"bad"}),
        )])
        .replace("data: [DONE]\n\n", "");
        let mut http = server(vec![Reply::hold(headers, partial)]).await;
        configure(&core, &http.url, json!({})).await;
        start(&core, "agent-cancel", &root).await;
        request(&mut http).await;
        core.handle("agent:cancel", &[json!("agent-cancel")])
            .await
            .unwrap()
            .unwrap();
        next_event(&mut events, "done").await;
        assert!(!root.join("cancelled").exists());
        no_extra(&mut http).await;
        core.shutdown().await;
    }
}

#[tokio::test]
#[cfg(unix)]
async fn native_cancel_kills_shell_descendants_and_prevents_late_mutation() {
    let (_dir, core, mut events) = fixture();
    let root = core.inner.ctx.paths.home_dir.clone();
    let _http = server(vec![Reply::sse(calls(vec![call(
        "child",
        "bash",
        json!({"command":"(sleep 1; printf late > late) & printf started > started; wait"}),
    )]))])
    .await;
    configure(&core, &_http.url, json!({})).await;
    start(&core, "agent-child", &root).await;
    let a = next_event(&mut events, "approval").await;
    approve(&core, &a, "once").await;
    tokio::time::timeout(Duration::from_secs(3), async {
        while !root.join("started").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    core.handle("agent:cancel", &[json!("agent-child")])
        .await
        .unwrap()
        .unwrap();
    next_event(&mut events, "done").await;
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(!root.join("late").exists());
    core.shutdown().await;
}

#[tokio::test]
#[cfg(unix)]
async fn native_output_is_unicode_bounded_and_reported_to_model_and_stage() {
    let (_dir, core, mut events) = fixture();
    let root = core.inner.ctx.paths.home_dir.clone();
    let mut http = server(vec![
        Reply::sse(calls(vec![call(
            "large",
            "bash",
            json!({"command":"i=0; while [ $i -lt 9000 ]; do printf '🦀'; i=$((i+1)); done"}),
        )])),
        Reply::sse(text("bounded")),
    ])
    .await;
    configure(&core, &http.url, json!({})).await;
    start(&core, "agent-output", &root).await;
    let a = next_event(&mut events, "approval").await;
    approve(&core, &a, "once").await;
    let stage = next_event(&mut events, "stage").await;
    assert_eq!(stage["stage"]["outputTruncated"], true);
    assert_eq!(s(&stage["stage"]["output"]).chars().count(), 8000);
    assert!(stage["stage"]["durationMs"].is_number());
    request(&mut http).await;
    let r = request(&mut http).await;
    let out: Value = serde_json::from_str(s(&r["messages"][3]["content"])).unwrap();
    assert_eq!(out["outputTruncated"], true);
    assert_eq!(s(&out["output"]).chars().count(), 8000);
    next_event(&mut events, "done").await;
    core.shutdown().await;
}

#[tokio::test]
async fn native_routes_search_in_process_and_rejects_images_explicitly() {
    let (_dir, core, mut events) = fixture();
    let root = core.inner.ctx.paths.home_dir.clone();
    let routes = Arc::new(Mutex::new(Vec::new()));
    let captured = routes.clone();
    core.inner
        .ctx
        .set_router(Arc::new(move |route, args| {
            captured.lock().unwrap().push((route.clone(), args));
            let value=if route=="search:all" {json!([{"id":"fixture","title":"Fixture","icon":"data:image/png;base64,SHOULD_NOT_LEAK","action":{"type":"open-file","path":"fixture.txt"}}])}else{json!({"hit":"fixture-indexed-source"})};
            Box::pin(async move { Ok(value) })
        }))
        .unwrap();
    let mut http = server(vec![
        Reply::sse(calls(vec![
            call("search", "pc_search", json!({"query":"needle"})),
            call("read", "pc_read", json!({"resultId":"result-1"})),
            call("launcher", "launcher_search", json!({"query":"note"})),
        ])),
        Reply::sse(text("Indexed result")),
    ])
    .await;
    configure(&core, &http.url, json!({})).await;
    start(&core, "agent-search", &root).await;
    next_event(&mut events, "done").await;
    let calls = routes.lock().unwrap().clone();
    assert_eq!(
        calls.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(),
        vec!["core:knowledge:search", "core:knowledge:read", "search:all"]
    );
    assert!(core.inner.gateway.lock().await.is_none());
    request(&mut http).await;
    let followup = request(&mut http).await;
    assert!(followup.to_string().contains("fixture.txt"));
    assert!(!followup.to_string().contains("SHOULD_NOT_LEAK"));
    core.handle("agent:run",&[json!({"runId":"agent-image","task":"image","images":[{"mimeType":"image/png","data":"aGVsbG8="}]})]).await.unwrap().unwrap();
    assert!(s(&next_event(&mut events, "error").await["message"]).contains("text-only"));
    next_event(&mut events, "done").await;
    core.handle(
        "agent:run",
        &[json!({"runId":"agent-history-image","task":"continue","historyHasImages":true})],
    )
    .await
    .unwrap()
    .unwrap();
    assert!(s(&next_event(&mut events, "error").await["message"]).contains("image history"));
    next_event(&mut events, "done").await;
    no_extra(&mut http).await;
    core.shutdown().await;
}

#[test]
fn native_routing_is_narrow_and_explicit_mode_fails_closed() {
    for provider in config::PROVIDERS.iter().copied().chain(["custom:fixture"]) {
        let supported = matches!(provider, "openai" | "openai-compatible" | "custom:fixture");
        assert_eq!(
            native::selected(&json!({"provider":provider})).unwrap(),
            supported
        );
        assert!(!native::selected(&json!({"provider":provider,"agentRuntime":"pi"})).unwrap());
        assert_eq!(
            native::selected(&json!({"provider":provider,"agentRuntime":"native"})).is_ok(),
            supported
        );
    }
    assert!(native::selected(&json!({"provider":"openai","agentRuntime":"typo"})).is_err());
}

#[tokio::test]
#[cfg(unix)]
async fn native_explicit_unsupported_fails_closed_and_pi_mode_uses_existing_adapter() {
    let (_dir, core, mut events) = fixture();
    let root = core.inner.ctx.paths.home_dir.clone();
    core.patch_config(json!({"provider":"ollama","agentRuntime":"native"}))
        .await
        .unwrap();
    start(&core, "agent-unsupported", &root).await;
    assert!(s(&next_event(&mut events, "error").await["message"]).contains("unsupported"));
    next_event(&mut events, "done").await;
    super::tests::mock_pi(&core, "complete");
    for (runtime, provider) in [("auto", "ollama"), ("pi", "openai-compatible")] {
        core.patch_config(json!({"agentRuntime":runtime,"provider":provider}))
            .await
            .unwrap();
        start(&core, "agent-compatible", &root).await;
        assert!(s(&next_event(&mut events, "answer").await["text"]).starts_with("complete"));
        next_event(&mut events, "done").await;
        // done emission precedes cleanup by a few instructions; yield to that task.
        tokio::task::yield_now().await;
    }
    core.shutdown().await;
}

#[tokio::test]
async fn native_review_applies_to_reads_and_always_decision_never_persists_file_content() {
    let (_dir, core, mut events) = fixture();
    let root = core.inner.ctx.paths.home_dir.clone();
    std::fs::write(root.join("read"), "secret fixture").unwrap();
    let mut http = server(vec![
        Reply::sse(calls(vec![call("r", "read", json!({"path":"read"}))])),
        Reply::sse(calls(vec![call(
            "w",
            "write",
            json!({"path":"written","content":"private fixture content"}),
        )])),
        Reply::sse(text("complete")),
    ])
    .await;
    configure(&core, &http.url, json!({"agentApprovalMode":"review"})).await;
    start(&core, "agent-review", &root).await;
    let a = next_event(&mut events, "approval").await;
    assert!(s(&a["command"]).contains("read"));
    approve(&core, &a, "once").await;
    let b = next_event(&mut events, "approval").await;
    approve(&core, &b, "always").await;
    next_event(&mut events, "done").await;
    assert!(core
        .read_config()
        .await
        .unwrap()
        .get("agentAlwaysAllowedExactCommands")
        .is_none());
    request(&mut http).await;
    request(&mut http).await;
    request(&mut http).await;
    core.shutdown().await;
}

#[tokio::test]
async fn native_context_limit_duplicate_live_run_and_literal_project_search() {
    let (_dir, core, mut events) = fixture();
    let root = core.inner.ctx.paths.home_dir.clone();
    std::fs::write(root.join("code.rs"), "literal.* 🦀\nelse").unwrap();
    let mut http = server(vec![
        Reply::sse(calls(vec![call(
            "s",
            "project_search",
            json!({"query":"literal.*"}),
        )])),
        Reply::sse(text("found")),
    ])
    .await;
    configure(&core, &http.url, json!({})).await;
    start(&core, "agent-literal", &root).await;
    next_event(&mut events, "done").await;
    request(&mut http).await;
    let next = request(&mut http).await;
    assert!(s(&next["messages"][3]["content"]).contains("code.rs:1: literal.* 🦀"));
    core.handle("agent:run",&[json!({"runId":"agent-limit","task":"x".repeat(native_protocol::CONTEXT_LIMIT+1),"cwd":root})]).await.unwrap().unwrap();
    assert!(s(&next_event(&mut events, "error").await["message"]).contains("context exceeds"));
    next_event(&mut events, "done").await;
    no_extra(&mut http).await;
    let mut held = server(vec![Reply::hold(true, String::new())]).await;
    configure(&core, &held.url, json!({})).await;
    start(&core, "agent-live", &root).await;
    request(&mut held).await;
    assert!(core
        .handle(
            "agent:run",
            &[json!({"runId":"agent-live","task":"duplicate"})]
        )
        .await
        .unwrap()
        .unwrap_err()
        .contains("already active"));
    core.handle("agent:cancel", &[json!("agent-live")])
        .await
        .unwrap()
        .unwrap();
    next_event(&mut events, "done").await;
    core.shutdown().await;
}

#[tokio::test]
#[cfg(unix)]
async fn native_next_header_deadline_never_replays_completed_effects() {
    let (_dir, core, mut events) = fixture();
    let root = core.inner.ctx.paths.home_dir.clone();
    let mut http = server(vec![
        Reply::sse(calls(vec![call(
            "one",
            "bash",
            json!({"command":"printf x >> counter"}),
        )])),
        Reply::hold(true, String::new()),
    ])
    .await;
    configure(&core, &http.url, json!({})).await;
    start(&core, "agent-deadline", &root).await;
    let a = next_event(&mut events, "approval").await;
    approve(&core, &a, "once").await;
    request(&mut http).await;
    request(&mut http).await;
    let error = tokio::time::timeout(Duration::from_secs(35), async {
        loop {
            let (_, e) = events.recv().await.unwrap();
            if e["type"] == "error" {
                return e;
            }
        }
    })
    .await
    .unwrap();
    assert!(s(&error["message"]).contains("response headers timed out after 30s"));
    next_event(&mut events, "done").await;
    assert_eq!(std::fs::read_to_string(root.join("counter")).unwrap(), "x");
    no_extra(&mut http).await;
    core.shutdown().await;
}

#[tokio::test]
async fn native_429_is_reported_without_replay_and_invalid_urls_are_configuration_errors() {
    let (_dir, core, mut events) = fixture();
    let root = core.inner.ctx.paths.home_dir.clone();
    let mut http = server(vec![Reply {
        body: String::new(),
        status: 429,
        hold_headers: false,
        hold_body: false,
    }])
    .await;
    configure(&core, &http.url, json!({})).await;
    start(&core, "agent-rate-limit", &root).await;
    assert!(s(&next_event(&mut events, "error").await["message"])
        .contains("status 429; Retry-After 2s"));
    next_event(&mut events, "done").await;
    request(&mut http).await;
    no_extra(&mut http).await;
    configure(&core, "file:///fixture", json!({})).await;
    start(&core, "agent-bad-url", &root).await;
    assert!(s(&next_event(&mut events, "error").await["message"]).contains("Native configuration"));
    next_event(&mut events, "done").await;
    core.shutdown().await;
}
