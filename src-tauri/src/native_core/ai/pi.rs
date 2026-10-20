use super::*;
use base64::Engine;
use process::{Cancel, GroupChild, Lines};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
impl AiCore {
    pub(super) fn pi_dir(&self) -> std::path::PathBuf {
        std::env::var_os("PI_CODING_AGENT_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| self.inner.ctx.paths.home_dir.join(".pi/agent"))
    }
    pub(super) async fn pi_binary(&self) -> String {
        #[cfg(test)]
        if let Some(path) = self.inner.pi_override.lock().unwrap().clone() {
            return path.to_string_lossy().into_owned();
        }
        if let Ok(bin) = std::env::var("RAYMES_PI_BIN") {
            if !bin.trim().is_empty() {
                return bin.trim().into();
            }
        }
        let home = self.inner.ctx.paths.home_dir.clone();
        tokio::task::spawn_blocking(move || {
            let mut candidates = Vec::new();
            if let Some(p) = std::env::var_os("PNPM_HOME") {
                candidates.push(std::path::PathBuf::from(p).join(if cfg!(windows) {
                    "pi.cmd"
                } else {
                    "pi"
                }));
            }
            for p in [
                "Library/pnpm/bin/pi",
                ".local/bin/pi",
                "Library/pnpm/pi",
                ".local/share/pnpm/pi",
            ] {
                candidates.push(home.join(p));
            }
            candidates
                .into_iter()
                .find(|p| p.is_file())
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or("pi".into())
        })
        .await
        .unwrap_or("pi".into())
    }
    pub(super) async fn pi_check(&self) -> Value {
        match process::output(
            self.command(self.pi_binary().await).await.arg("--version"),
            Duration::from_secs(8),
            &Cancel::new(),
        )
        .await
        {
            Ok(version) => json!({"installed":true,"version":version.trim()}),
            Err(_) => json!({"installed":false}),
        }
    }
    pub(super) async fn pi_install(&self) -> Value {
        let url = "https://www.npmjs.com/package/@mariozechner/pi-coding-agent";
        for (method, args) in [
            (
                "npm",
                vec!["install", "-g", "@mariozechner/pi-coding-agent"],
            ),
            ("pnpm", vec!["add", "-g", "@mariozechner/pi-coding-agent"]),
        ] {
            if process::output(
                self.command(method).await.arg("--version"),
                Duration::from_secs(8),
                &Cancel::new(),
            )
            .await
            .is_err()
            {
                continue;
            }
            let result = process::output(
                self.command(method).await.args(args),
                Duration::from_secs(180),
                &self.inner.stop,
            )
            .await;
            return match result {
                Err(error) => {
                    json!({"ok":false,"reason":"install-failed","officialUrl":url,"error":error})
                }
                Ok(_) => {
                    let check = self.pi_check().await;
                    if check["installed"] == true {
                        json!({"ok":true,"method":method,"version":check["version"]})
                    } else {
                        json!({"ok":false,"reason":"install-failed","officialUrl":url,"error":format!("{method} finished, but Tezbar can't find the pi command on your PATH.")})
                    }
                }
            };
        }
        json!({"ok":false,"reason":"no-package-manager","officialUrl":url})
    }
    pub(super) async fn pi_extensions(&self) -> CoreResult {
        let dir = self.pi_dir();
        tokio::task::spawn_blocking(move||{let mut results=Vec::new();let mut seen=std::collections::HashSet::new();let settings=common::read_json(&dir.join("settings.json")).unwrap_or(Value::Null);if let Some(packages)=settings["packages"].as_array(){for pkg in packages{let Some(pkg)=pkg.as_str()else{continue};let name=pkg.strip_prefix("npm:").unwrap_or(pkg);if !seen.insert(name.to_string()){continue}let location=dir.join("npm/node_modules").join(name);let meta=common::read_json(&location.join("package.json")).unwrap_or(Value::Null);let features=match name{"pi-antigravity"=>json!(["Google Cloud Code Provider","Image generation (generate_image)","Claude & Gemini models"]),"pi-agents"=>json!(["Multi-agent orchestration","Parallel & sequence workflows","Subagent spawning","Mermaid diagrams"]),"pi-multi-account"=>json!(["Multi-account rotation","Automatic rate-limit failover","OAuth account cycling"]),"pi-notify"=>json!(["OSC / native desktop notifications","Long-running turn alerts"]),"pi-reset-usage-limit"=>json!(["Codex usage limit credit resets"]),_=>if s(&meta["description"]).to_lowercase().contains("provider"){json!(["Custom AI provider"])}else{json!([])}};results.push(json!({"id":name,"name":meta["name"].as_str().unwrap_or(name),"version":meta["version"].as_str().map(|v|format!("v{v}")).unwrap_or("installed".into()),"description":meta["description"].as_str().unwrap_or("Pi agent package"),"type":"package","location":if meta.is_object(){location.to_string_lossy().into_owned()}else{pkg.to_string()},"enabled":true,"features":features}));}}
        if let Ok(files)=std::fs::read_dir(dir.join("extensions")){for entry in files.flatten(){let path=entry.path();if !matches!(path.extension().and_then(|s|s.to_str()),Some("ts"|"js")){continue}let id=path.file_stem().unwrap_or_default().to_string_lossy().into_owned();if !seen.insert(id.clone()){continue}let name=id.split(['-','_']).map(|word|{let mut chars=word.chars();match chars.next(){Some(c)=>c.to_uppercase().collect::<String>()+chars.as_str(),None=>String::new()}}).collect::<Vec<_>>().join(" ");results.push(json!({"id":id,"name":name,"version":"local","description":format!("Custom extension script ({})",path.file_name().unwrap_or_default().to_string_lossy()),"type":"script","location":path,"enabled":true,"features":if id.contains("modal"){json!(["Modal GLM-5.3-Flash provider","Serverless inference endpoint"])}else{json!(["Custom extension hooks and tools"])}}));}}Ok(json!(results))}).await.map_err(err)?
    }
    pub(super) async fn approval(
        &self,
        run_id: &str,
        title: &str,
        command: &str,
        cancel: &Cancel,
    ) -> bool {
        if cancel.is_cancelled() {
            return false;
        }
        let raw = match self.read_config().await {
            Ok(v) => v,
            Err(_) => return false,
        };
        if raw["agentAlwaysAllowedExactCommands"]
            .as_array()
            .is_some_and(|a| a.iter().any(|c| c.as_str() == Some(command.trim())))
        {
            return true;
        }
        let id = format!("approval-{}", uuid::Uuid::new_v4());
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut state = self.inner.state.lock().unwrap();
            if !state
                .agent
                .as_ref()
                .is_some_and(|r| r.id == run_id && !r.cancel.is_cancelled())
            {
                return false;
            }
            state.approvals.insert(
                id.clone(),
                Approval {
                    run_id: run_id.into(),
                    command: command.into(),
                    reply: tx,
                },
            );
        }
        let mut event = json!({"type":"approval","approvalId":id,"title":title,"command":command});
        if let Some(rule) = suggested_rule(command) {
            event["suggestedRule"] = json!(rule);
        }
        self.agent_event(run_id, event);
        let answer = tokio::select! {r=rx=>r.unwrap_or(false),_=cancel.cancelled()=>false,_=tokio::time::sleep(Duration::from_secs(300))=>false};
        self.inner.state.lock().unwrap().approvals.remove(&id);
        answer
    }
    pub(super) async fn approve(&self, v: &Value) -> CoreResult {
        let run = s(&v["runId"]);
        let id = s(&v["approvalId"]);
        let decision = s(&v["decision"]);
        if run.is_empty() || id.is_empty() || !matches!(decision, "deny" | "once" | "always") {
            return Ok(json!({"ok":false,"error":"Invalid approval"}));
        }
        let approval = {
            let mut state = self.inner.state.lock().unwrap();
            if !state.approvals.get(id).is_some_and(|a| a.run_id == run) {
                return Ok(json!({"ok":false,"error":"This approval is no longer active"}));
            }
            state.approvals.remove(id).unwrap()
        };
        if decision == "always" {
            let ctx = self.inner.ctx.clone();
            let command = approval.command.trim().to_string();
            if command.is_empty() || command.chars().count() > 16384 || command.contains('\0') {
                let _ = approval.reply.send(false);
                return Err("Invalid exact command approval".into());
            }
            let result = tokio::task::spawn_blocking(move || {
                ctx.config.update(|raw| {
                    let mut commands = raw["agentAlwaysAllowedExactCommands"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    if !commands.iter().any(|c| c.as_str() == Some(&command)) {
                        commands.push(json!(command));
                    }
                    raw["agentAlwaysAllowedExactCommands"] = json!(commands);
                    Ok(())
                })
            })
            .await
            .map_err(err)?;
            if let Err(error) = result {
                let _ = approval.reply.send(false);
                return Err(error);
            }
        }
        // A cancellation that raced with persistence must still deny execution.
        let active = self
            .inner
            .state
            .lock()
            .unwrap()
            .agent
            .as_ref()
            .is_some_and(|r| r.id == run && !r.cancel.is_cancelled());
        let _ = approval.reply.send(active && decision != "deny");
        Ok(json!({"ok":true}))
    }
    pub(super) async fn pi_run(
        &self,
        task: &str,
        images: &[Value],
        cwd: Option<&str>,
        cfg: &Value,
        cancel: &Cancel,
        run_id: Option<&str>,
    ) -> Result<String, String> {
        let images = normalize_images(images)?;
        let (pattern, provider_json, vision) = config::pi_bridge(cfg);
        let mut task = task.to_string();
        let mut images = images;
        if !images.is_empty() && !vision {
            let text = self.ocr_images(&images, cancel).await?;
            if text.trim().is_empty() {
                return Err("The selected model cannot read images, and no text could be extracted from the attached screen. Choose a vision model or remove the attachment.".into());
            }
            task.push_str("\n\nText extracted locally from the attached screen:\n\n");
            task.push_str(&text);
            images.clear();
        }
        let policy = self.policy_path().await?;
        let gateway = self.ensure_gateway().await?;
        let mut cmd = self.command(self.pi_binary().await).await;
        cmd.args([
            "--mode",
            "rpc",
            "--no-session",
            "--model",
            &pattern,
            "--extension",
            &policy.to_string_lossy(),
        ]);
        cmd.current_dir(
            cwd.map(std::path::PathBuf::from)
                .unwrap_or_else(|| self.inner.ctx.paths.home_dir.clone()),
        );
        cmd.stdin(std::process::Stdio::piped());
        if let Some(json) = provider_json {
            cmd.env("RAYMES_PI_PROVIDER_JSON", json);
        }
        let raw = self.read_config().await?;
        cmd.env(
            "RAYMES_PI_ALWAYS_ALLOW_JSON",
            raw["agentAlwaysAllowedCommands"]
                .as_array()
                .map(|a| json!(a))
                .unwrap_or(json!([]))
                .to_string(),
        )
        .env(
            "RAYMES_PI_ALWAYS_ALLOW_EXACT_JSON",
            raw["agentAlwaysAllowedExactCommands"]
                .as_array()
                .map(|a| json!(a))
                .unwrap_or(json!([]))
                .to_string(),
        )
        .env("TEZBAR_KNOWLEDGE_ENDPOINT", gateway.0)
        .env("TEZBAR_KNOWLEDGE_TOKEN", gateway.1);
        if pattern.starts_with("opencode/") {
            let extension = self.pi_dir().join("extensions/opencode/index.ts");
            if tokio::fs::try_exists(&extension).await.unwrap_or(false) {
                cmd.args(["--extension", &extension.to_string_lossy()]);
            }
        }
        let mut group = GroupChild::spawn(&mut cmd)?;
        let mut input = group.child.stdin.take().ok_or("Missing Pi stdin")?;
        let mut stdout = group.child.stdout.take().ok_or("Missing Pi stdout")?;
        let mut stderr = group.child.stderr.take().ok_or("Missing Pi stderr")?;
        let (ui_tx, mut ui_rx) = tokio::sync::mpsc::channel::<Value>(32);
        let mut ui_tasks = tokio::task::JoinSet::new();
        let mut driver = pi_loop::Loop::default();
        let mut framing = Lines::default();
        let mut err_framing = Lines::default();
        let mut tail = std::collections::VecDeque::new();
        let mut out_buf = [0u8; 8192];
        let mut err_buf = [0u8; 4096];
        let mut stderr_closed = false;
        let model_id = uuid::Uuid::new_v4().to_string();
        let prompt_id = uuid::Uuid::new_v4().to_string();
        let mut prompt = json!({"type":"prompt","id":prompt_id,"message":task});
        if !images.is_empty() {
            prompt["images"] = json!(images);
        }
        let mut pending = Some((
            model_id.clone(),
            tokio::time::Instant::now() + Duration::from_secs(15),
        ));
        let (provider, model) = pattern.split_once('/').ok_or("Invalid Pi model pattern")?;
        let work = async {
            write(
                &mut input,
                &json!({"type":"set_model","id":model_id,"provider":provider,"modelId":model}),
            )
            .await?;
            let deadline = tokio::time::Instant::now() + Duration::from_secs(900);
            let mut started = false;
            loop {
                tokio::select! {
                    _=cancel.cancelled()=>return Err("Agent run aborted".into()),
                    _=self.inner.stop.cancelled()=>return Err("Agent service shut down".into()),
                    _=tokio::time::sleep_until(deadline)=>return Err("Agent run timed out after 900000ms".into()),
                    _=tokio::time::sleep_until(pending.as_ref().map(|p|p.1).unwrap_or(deadline)),if pending.is_some()=>return Err("Pi RPC command timed out".into()),
                    Some(response)=ui_rx.recv()=>write(&mut input,&response).await?,
                    Some(_)=ui_tasks.join_next(),if !ui_tasks.is_empty()=>{},
                    n=stderr.read(&mut err_buf),if !stderr_closed=>{let n=n.map_err(err)?;stderr_closed=n==0;for line in err_framing.feed(&err_buf[..n],n==0)?{if line.starts_with("Warning: No models match pattern "){continue}if tail.len()>=50{tail.pop_front();}tail.push_back(line.clone());if let Some(run)=run_id{self.agent_event(run,json!({"type":"log","source":"stderr","line":line}));}}},
                    n=stdout.read(&mut out_buf)=>{let n=n.map_err(err)?;for line in framing.feed(&out_buf[..n],n==0)?{let Ok(v)=serde_json::from_str::<Value>(&line)else{continue};let kind=s(&v["type"]);if kind=="response"{let id=s(&v["id"]);if pending.as_ref().is_some_and(|p|p.0==id){if v["success"]==false{return Err(v["error"].as_str().unwrap_or("Pi RPC request failed").into())}if id==model_id{write(&mut input,&prompt).await?;pending=Some((prompt_id.clone(),tokio::time::Instant::now()+Duration::from_secs(if images.is_empty(){30}else{60})));}else{pending=None;if v["data"]["disposition"]=="handled"&&!started{return Err("The prompt was handled by an extension without starting an agent run.".into())}}}continue}
                        if kind=="extension_ui_request"{let id=s(&v["id"]).to_string();if id.is_empty(){continue}match s(&v["method"]){"confirm"=>{let core=self.clone();let run=run_id.map(str::to_string);let stop=cancel.clone();let tx=ui_tx.clone();ui_tasks.spawn(async move{let confirmed=if let Some(run)=run{core.approval(&run,v["title"].as_str().unwrap_or("Allow command?"),s(&v["message"]),&stop).await}else{false};let _=tx.send(json!({"type":"extension_ui_response","id":id,"confirmed":confirmed})).await;});},"notify"|"setStatus"=>{if let Some(run)=run_id{let line=v["message"].as_str().or_else(||v["statusText"].as_str()).unwrap_or("");if !line.is_empty(){self.agent_event(run,json!({"type":"log","source":"stderr","line":line}));}}},"setWidget"|"setTitle"|"set_editor_text"=>{},_=>write(&mut input,&json!({"type":"extension_ui_response","id":id,"cancelled":true})).await?}continue}
                        if kind=="agent_start"{started=true;}for event in driver.event(&v){if s(&event["type"])!="error"{if let Some(run)=run_id{self.agent_event(run,event);}}}if driver.finished{if pending.as_ref().is_some_and(|p|p.0==model_id){return Err("Pi emitted output before model selection was acknowledged".into())}return driver.result()}
                    }if n==0{return Err("pi exited before finishing".into())}}
                }
            }
        };
        let work = tokio::select! {
            _=cancel.cancelled()=>Err("Agent run aborted".to_string()),
            _=self.inner.stop.cancelled()=>Err("Agent service shut down".to_string()),
            result=tokio::time::timeout(Duration::from_secs(900),work)=>result.map_err(|_|"Agent run timed out after 900000ms".to_string()).and_then(|v|v)
        };
        ui_tasks.abort_all();
        while ui_tasks.join_next().await.is_some() {}
        let _ = tokio::time::timeout(
            Duration::from_millis(200),
            write(
                &mut input,
                &json!({"type":"abort","id":uuid::Uuid::new_v4().to_string()}),
            ),
        )
        .await;
        drop(input);
        group.terminate().await;
        work.map_err(|message: String| {
            let detail = tail
                .iter()
                .filter(|line| !message.contains(line.as_str()))
                .rev()
                .take(6)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            if detail.is_empty() {
                message
            } else {
                format!("{message}\n{detail}")
            }
        })
    }
    /// Private RPC adapter for native callers; every request has its own ephemeral process.
    pub(super) async fn pi_query(&self, command: &Value, cwd: Option<&str>) -> CoreResult {
        if !command.is_object() || !command["type"].is_string() {
            return Err("Pi query must include a command type".into());
        }
        let mut cmd = self.command(self.pi_binary().await).await;
        cmd.args(["--mode", "rpc", "--no-session"])
            .stdin(std::process::Stdio::piped());
        cmd.current_dir(
            cwd.map(std::path::PathBuf::from)
                .unwrap_or_else(|| self.inner.ctx.paths.home_dir.clone()),
        );
        let mut group = GroupChild::spawn(&mut cmd)?;
        let mut input = group.child.stdin.take().ok_or("Missing Pi stdin")?;
        let mut output = group.child.stdout.take().ok_or("Missing Pi stdout")?;
        let mut stderr = group.child.stderr.take().ok_or("Missing Pi stderr")?;
        let id = uuid::Uuid::new_v4().to_string();
        let mut command = command.clone();
        command["id"] = json!(id);
        let mut frame = Lines::default();
        let mut error_frame = Lines::default();
        let mut tail = std::collections::VecDeque::new();
        let mut buf = [0u8; 8192];
        let mut errors = [0u8; 4096];
        let mut error_closed = false;
        let work = async {
            write(&mut input, &command).await?;
            loop {
                tokio::select! {
                    read=output.read(&mut buf)=>{let n=read.map_err(err)?;for line in frame.feed(&buf[..n],n==0)?{let Ok(v)=serde_json::from_str::<Value>(&line)else{continue};if v["type"]=="response"&&v["id"]==id{if v["success"]==false{return Err(v["error"].as_str().unwrap_or("Pi RPC request failed").to_string())}return Ok(v)}}if n==0{return Err("pi rpc session closed before response".into())}},
                    read=stderr.read(&mut errors),if !error_closed=>{let n=read.map_err(err)?;error_closed=n==0;for line in error_frame.feed(&errors[..n],n==0)?{if tail.len()==50{tail.pop_front();}tail.push_back(line);}}
                }
            }
        };
        let result = tokio::select! {_=self.inner.stop.cancelled()=>Err("AI service shut down".into()),result=tokio::time::timeout(Duration::from_secs(10),work)=>result.map_err(|_|"Pi RPC query timed out".to_string()).and_then(|v|v)};
        drop(input);
        group.terminate().await;
        result.map_err(|message: String| {
            let tail = tail.into_iter().collect::<Vec<_>>().join("\n");
            if tail.is_empty() {
                message
            } else {
                format!("{message}\n{tail}")
            }
        })
    }
    pub(super) async fn pi_observe(&self, cwd: Option<&str>) -> CoreResult {
        let state_command = json!({"type":"get_state"});
        let stats_command = json!({"type":"get_session_stats"});
        let (state, stats) = tokio::join!(
            self.pi_query(&state_command, cwd),
            self.pi_query(&stats_command, cwd)
        );
        let state = state.unwrap_or(Value::Null);
        let stats = stats.unwrap_or(Value::Null);
        let state = &state["data"];
        let stats = &stats["data"];
        let mut result = json!({"cwd":cwd.map(str::to_string).unwrap_or_else(||self.inner.ctx.paths.home_dir.to_string_lossy().into_owned()),"isStreaming":state["isStreaming"]==true,"messageCount":state["messageCount"].as_u64().unwrap_or(0),"pendingMessageCount":state["pendingMessageCount"].as_u64().unwrap_or(0),"toolCalls":stats["toolCalls"].as_u64().unwrap_or(0)});
        if let Some(id) = state["model"]["id"].as_str() {
            result["model"] = json!(
                if let Some(provider) = state["model"]["provider"].as_str() {
                    format!("{provider}/{id}")
                } else {
                    id.into()
                }
            );
        }
        if let Some(path) = state["sessionFile"]
            .as_str()
            .or_else(|| stats["sessionFile"].as_str())
        {
            result["sessionFile"] = json!(path);
        }
        for key in ["thinkingLevel"] {
            if let Some(v) = state.get(key) {
                result[key] = v.clone();
            }
        }
        if let Some(v) = stats.get("contextUsage") {
            result["contextUsage"] = v.clone();
        }
        Ok(result)
    }
    async fn policy_path(&self) -> Result<std::path::PathBuf, String> {
        let mut paths = Vec::new();
        if let Some(p) = std::env::var_os("RAYMES_PI_EXTENSION") {
            paths.push(p.into());
        }
        paths.push(
            self.inner
                .ctx
                .paths
                .resource_dir
                .join("agent/raymes-pi-policy.ts"),
        );
        paths.push(
            self.inner
                .ctx
                .paths
                .resource_dir
                .join("dist-backend/agent/raymes-pi-policy.ts"),
        );
        if cfg!(debug_assertions) {
            paths.push(
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../src/main/agent/raymes-pi-policy.ts"),
            );
        }
        for path in paths {
            if tokio::fs::try_exists(&path).await.unwrap_or(false) {
                return Ok(path);
            }
        }
        Err("Pi policy resource is missing: agent/raymes-pi-policy.ts".into())
    }
    async fn ocr_images(&self, images: &[Value], cancel: &Cancel) -> Result<String, String> {
        if !cfg!(target_os = "macos") {
            return Ok(String::new());
        }
        let mut candidates = vec![
            self.inner
                .ctx
                .paths
                .resource_dir
                .join("native/screenocr/screenocr-helper"),
            self.inner
                .ctx
                .paths
                .resource_dir
                .join("app.asar.unpacked/native/screenocr/screenocr-helper"),
        ];
        if let Some(path) = std::env::var_os("SCREENOCR_HELPER_PATH") {
            candidates.insert(0, path.into());
        }
        if cfg!(debug_assertions) {
            candidates.push(
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../native/screenocr/screenocr-helper"),
            );
        }
        let mut helper = None;
        for candidate in candidates {
            if tokio::fs::try_exists(&candidate).await.unwrap_or(false) {
                helper = Some(candidate);
                break;
            }
        }
        let Some(helper) = helper else {
            return Ok(String::new());
        };
        let root = self.inner.ctx.paths.temp_dir.clone();
        let temp = tokio::task::spawn_blocking(move || {
            tempfile::Builder::new()
                .prefix("tezbar-agent-image-")
                .tempdir_in(root)
                .map_err(err)
        })
        .await
        .map_err(err)??;
        let mut texts = Vec::new();
        for (i, image) in images.iter().enumerate() {
            let ext = match s(&image["mimeType"]) {
                "image/jpeg" => "jpg",
                "image/webp" => "webp",
                _ => "png",
            };
            let path = temp.path().join(format!("attachment-{i}.{ext}"));
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(s(&image["data"]))
                .map_err(err)?;
            tokio::fs::write(&path, bytes).await.map_err(err)?;
            let args=json!({"imagePath":path,"fast":false,"languageCorrection":true,"ignoreLineBreaks":false}).to_string();
            let output = process::output(
                process::command(&helper).args(["recognize-text", &args]),
                Duration::from_secs(45),
                cancel,
            )
            .await?;
            let v: Value = serde_json::from_str(output.trim()).map_err(err)?;
            if v["ok"] != true {
                return Err(v["error"]
                    .as_str()
                    .unwrap_or("Local screen text extraction failed")
                    .into());
            }
            if !s(&v["value"]).trim().is_empty() {
                texts.push(s(&v["value"]).trim().to_string());
            }
        }
        let text = cut(&texts.join("\n\n"), 40000);
        tokio::task::spawn_blocking(move || drop(temp))
            .await
            .map_err(err)?;
        Ok(text)
    }
}
async fn write(input: &mut tokio::process::ChildStdin, v: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(v).map_err(err)?;
    bytes.push(b'\n');
    input.write_all(&bytes).await.map_err(err)
}
pub fn normalize_images(images: &[Value]) -> Result<Vec<Value>, String> {
    if images.len() > 4 {
        return Err("Agent accepts at most 4 images per prompt".into());
    }
    images
        .iter()
        .map(|image| {
            let mime = s(&image["mimeType"]);
            if !matches!(mime, "image/png" | "image/jpeg" | "image/webp") {
                return Err(format!("Unsupported agent image type: {mime}"));
            }
            let mut data = s(&image["data"]).trim();
            if data.starts_with("data:image/") {
                data = data
                    .split_once(',')
                    .map(|(_, data)| data)
                    .ok_or("Agent image is not valid base64 data")?;
            }
            if data.is_empty() {
                return Err("Agent image is not valid base64 data".into());
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|_| "Agent image is not valid base64 data")?;
            if bytes.len() > 8 * 1024 * 1024 {
                return Err("Agent image exceeds the 8 MB limit".into());
            }
            Ok(json!({"type":"image","data":data,"mimeType":mime}))
        })
        .collect()
}
fn suggested_rule(command: &str) -> Option<String> {
    if command.contains([';', '<', '>', '`', '\n'])
        || command.contains("$(")
        || command.contains("||")
    {
        return None;
    }
    command
        .split("&&")
        .flat_map(|part| part.split('|'))
        .filter_map(|part| part.split_whitespace().next())
        .map(|name| name.rsplit('/').next().unwrap_or(name).to_lowercase())
        .find(|name| matches!(name.as_str(), "grep" | "rg"))
}
