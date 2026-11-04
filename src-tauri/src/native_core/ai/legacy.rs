use super::*;
use base64::Engine;
impl AiCore {
    pub(super) fn pi_dir(&self) -> std::path::PathBuf {
        #[cfg(test)]
        return self.inner.ctx.paths.home_dir.join(".pi/agent");
        #[cfg(not(test))]
        std::env::var_os("PI_CODING_AGENT_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| self.inner.ctx.paths.home_dir.join(".pi/agent"))
    }
    pub(super) async fn pi_check(&self) -> Value {
        json!({"installed":true,"version":"native"})
    }
    pub(super) async fn pi_install(&self) -> Value {
        json!({"ok":true,"method":"native","version":"native"})
    }
    pub(super) async fn pi_extensions(&self) -> CoreResult {
        let dir = self.pi_dir();
        tokio::task::spawn_blocking(move||{
            let settings=common::read_json(&dir.join("settings.json")).unwrap_or(Value::Null);let mut items=vec![];
            for pkg in settings["packages"].as_array().into_iter().flatten().filter_map(Value::as_str){
                items.push(json!({"id":pkg,"name":pkg.strip_prefix("npm:").unwrap_or(pkg),"version":"installed","description":"Not loaded. Tezbar no longer runs Pi extensions.","type":"package","location":pkg,"enabled":false,"features":[]}));
            }
            if let Ok(entries)=std::fs::read_dir(dir.join("extensions")){for entry in entries.flatten().take(512){let path=entry.path();if matches!(path.extension().and_then(|s|s.to_str()),Some("js"|"ts")){let name=path.file_stem().unwrap_or_default().to_string_lossy();items.push(json!({"id":name,"name":name,"version":"local","description":"Not loaded. Tezbar no longer runs Pi extensions.","type":"script","location":path,"enabled":false,"features":[]}));}}}
            Ok(json!(items))
        }).await.map_err(err)?
    }
    pub(super) async fn pi_query(&self, command: &Value, cwd: Option<&str>) -> CoreResult {
        let observation = self.pi_observe(cwd).await?;
        let data = match s(&command["type"]) {
            "get_state" => {
                json!({"cwd":observation["cwd"],"isStreaming":observation["isStreaming"],"messageCount":observation["messageCount"],"pendingMessageCount":0,"model":observation["model"]})
            }
            "get_session_stats" => json!({"toolCalls":observation["toolCalls"]}),
            _ => {
                return Err(
                    "Native observation supports get_state and get_session_stats only".into(),
                )
            }
        };
        Ok(json!({"type":"response","command":command["type"],"success":true,"data":data}))
    }
    pub(super) async fn pi_observe(&self, cwd: Option<&str>) -> CoreResult {
        let dir = cwd
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| self.inner.ctx.paths.home_dir.clone());
        let dir = tokio::fs::canonicalize(&dir).await.unwrap_or(dir);
        let key = dir.to_string_lossy().into_owned();
        if let Some(observation) = self.inner.state.lock().unwrap().observations.get(&key) {
            let mut observation = observation.clone();
            observation.as_object_mut().unwrap().remove("runId");
            return Ok(observation);
        }
        let cfg = self.cfg(Some("chat"), None).await?;
        Ok(
            json!({"cwd":cwd.map(str::to_string).unwrap_or_else(||self.inner.ctx.paths.home_dir.to_string_lossy().into_owned()),"isStreaming":false,"messageCount":0,"pendingMessageCount":0,"toolCalls":0,"model":{"id":cfg["model"],"provider":cfg["provider"]}}),
        )
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
        let mode = raw["agentApprovalMode"].as_str().unwrap_or("default");
        if mode != "review"
            && raw["agentAlwaysAllowedExactCommands"]
                .as_array()
                .is_some_and(|a| a.iter().any(|c| c.as_str() == Some(command.trim())))
        {
            return true;
        }
        if mode == "turbo" && !title.contains("Destructive") {
            return true;
        }
        self.approval_prompt(run_id, title, command, cancel, true)
            .await
    }
    // Raw prompt seam shared with native tools: no policy shortcuts.
    pub(super) async fn approval_prompt(&self,run_id:&str,title:&str,command:&str,cancel:&Cancel,remember_command:bool)->bool{
        self.approval_prompt_identity(run_id,title,command,cancel,remember_command,None).await
    }
    pub(super) async fn approval_prompt_identity(
        &self,
        run_id: &str,
        title: &str,
        command: &str,
        cancel: &Cancel,
        remember_command: bool,
        extension_identity:Option<(String,String,String,String)>,
    ) -> bool {
        if cancel.is_cancelled() {
            return false;
        }
        let id = format!("approval-{}", uuid::Uuid::new_v4());
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut state = self.inner.state.lock().unwrap();
            if !state
                .agent
                .as_ref()
                .is_some_and(|r| r.id == run_id && !r.cancel.is_cancelled())
                && !state
                    .agents
                    .get(run_id)
                    .is_some_and(|r| !r.cancel.is_cancelled())
            {
                return false;
            }
            state.approvals.insert(
                id.clone(),
                Approval {
                    run_id: run_id.into(),
                    command: command.into(),
                    remember_command,
                    extension_identity:extension_identity.clone(),
                    reply: tx,
                },
            );
        }
        let mut event = json!({"type":"approval","approvalId":id,"title":title,"command":command,"canRemember":remember_command||extension_identity.is_some()});
        if remember_command {
            if let Some(rule) = suggested_rule(command) {
                event["suggestedRule"] = json!(rule);
            }
        }
        self.agent_event(run_id, event);
        let answer = tokio::select! {r=rx=>r.unwrap_or(false),_=cancel.cancelled()=>false,_=tokio::time::sleep(Duration::from_secs(300))=>false};
        self.inner.state.lock().unwrap().approvals.remove(&id);
        answer && !cancel.is_cancelled()
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
        if decision=="always" {if let Some((extension,connection,tool,hash))=approval.extension_identity.clone(){
            if let Err(error)=self.remember_extension_tool(extension,connection,tool,hash).await {let _=approval.reply.send(false);return Err(error)}
        }}
        if decision == "always" && approval.remember_command {
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
        let active = {
            let state = self.inner.state.lock().unwrap();
            state
                .agent
                .as_ref()
                .is_some_and(|r| r.id == run && !r.cancel.is_cancelled())
                || state
                    .agents
                    .get(run)
                    .is_some_and(|r| !r.cancel.is_cancelled())
        };
        let _ = approval.reply.send(active && decision != "deny");
        Ok(json!({"ok":true}))
    }
    pub(super) async fn ocr_images(
        &self,
        images: &[Value],
        cancel: &Cancel,
    ) -> Result<String, String> {
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
            let format = image::guess_format(&bytes)
                .map_err(|_| "Agent image MIME does not match its bytes")?;
            let expected = match mime {
                "image/png" => image::ImageFormat::Png,
                "image/jpeg" => image::ImageFormat::Jpeg,
                _ => image::ImageFormat::WebP,
            };
            if format != expected {
                return Err("Agent image MIME does not match its bytes".into());
            }
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
