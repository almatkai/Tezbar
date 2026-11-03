//! Native AI services. No renderer or JavaScript backend participates in dispatch.
mod auth;
mod chat;
mod config;
mod gateway;
mod memory;
mod pi;
mod pi_loop;
mod process;
mod provider;
#[cfg(test)]
mod tests;
mod voice;
use super::common::{self, now_ms, CoreContext, CoreResult, HandlerResult};
use process::Cancel;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

pub const CHANNELS: &[&str] = &[
    "llm-config-get",
    "llm-config-set",
    "llm-provider-statuses",
    "llm-list-models",
    "query",
    "cancel",
    "agent:run",
    "agent:cancel",
    "agent:approve",
    "agent:capture-active-screen",
    "chat:run",
    "chat:list",
    "chat:get",
    "chat:append",
    "chat:update-title",
    "chat:generate-meta",
    "chat:delete-turn",
    "chat:delete",
    "chat:clear",
    "github-device-start",
    "github-device-poll",
    "github-device-cancel",
    "pi:check",
    "pi:install",
    "pi-extensions:list",
    "ai:action",
    "voice:tts:speak",
    "voice:tts:stop",
    "voice:stt:modes",
    "voice:stt:transcribe",
    "voice:models:list",
    "voice:model:download",
    "voice:model:delete",
    "voice:model:get-selected",
    "voice:model:set-selected",
    "core:ai:extension-ask",
    "core:ai:pi-query",
    "core:ai:pi-observe",
];
/// Required parent routes: core:platform:screenshot, core:platform:update-launcher-hotkey,
/// core:knowledge:search, core:knowledge:read, search:all. All are in-process requests.
#[derive(Clone)]
pub struct AiCore {
    inner: Arc<Inner>,
}
struct Inner {
    #[cfg(test)]
    pi_override: Mutex<Option<std::path::PathBuf>>,
    ctx: CoreContext,
    http: reqwest::Client,
    state: Mutex<State>,
    gateway: tokio::sync::Mutex<Option<gateway::Gateway>>,
    token_refresh: tokio::sync::Mutex<()>,
    login_path: tokio::sync::OnceCell<String>,
    voice: voice::Voice,
    stop: Cancel,
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}
#[derive(Default)]
struct State {
    agent: Option<Run>,
    answer: Option<Run>,
    approvals: HashMap<String, Approval>,
    device: Option<Value>,
    device_generation: u64,
}
struct Run {
    id: String,
    cancel: Cancel,
    emitted: bool,
}
struct Approval {
    run_id: String,
    command: String,
    reply: tokio::sync::oneshot::Sender<bool>,
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn cut(v: &str, n: usize) -> String {
    v.chars().take(n).collect()
}
fn arg(args: &[Value], n: usize) -> &Value {
    args.get(n).unwrap_or(&Value::Null)
}
impl AiCore {
    pub fn new(ctx: CoreContext) -> Self {
        let voice = voice::Voice::new(ctx.clone());
        Self {
            inner: Arc::new(Inner {
                #[cfg(test)]
                pi_override: Mutex::new(None),
                ctx,
                http: reqwest::Client::new(),
                state: Mutex::new(State::default()),
                gateway: tokio::sync::Mutex::new(None),
                token_refresh: tokio::sync::Mutex::new(()),
                login_path: tokio::sync::OnceCell::new(),
                voice,
                stop: Cancel::new(),
                tasks: Mutex::new(Vec::new()),
            }),
        }
    }
    /// Processes, runtime probes, and the loopback listener are demand-launched.
    /// Construction is sufficient to accept requests; startup never signs in or calls a model.
    pub async fn start(&self) {}
    pub async fn shutdown(&self) {
        self.inner.stop.cancel();
        self.cancel_run(None, true);
        {
            let mut state = self.inner.state.lock().unwrap();
            if let Some(run) = state.answer.take() {
                run.cancel.cancel();
            }
            state.device = None;
            state.device_generation += 1;
        }
        self.inner.voice.shutdown().await;
        let tasks = std::mem::take(&mut *self.inner.tasks.lock().unwrap());
        for mut task in tasks {
            if tokio::time::timeout(Duration::from_secs(5), &mut task)
                .await
                .is_err()
            {
                task.abort();
                let _ = task.await;
            }
        }
        if let Some(gateway) = self.inner.gateway.lock().await.take() {
            gateway.cancel.cancel();
            let _ = gateway.task.await;
        }
    }
    fn spawn(&self, f: impl std::future::Future<Output = ()> + Send + 'static) {
        let task = tokio::spawn(f);
        let mut tasks = self.inner.tasks.lock().unwrap();
        tasks.retain(|t| !t.is_finished());
        tasks.push(task);
    }
    async fn command(&self, binary: impl AsRef<std::ffi::OsStr>) -> tokio::process::Command {
        let path = self
            .inner
            .login_path
            .get_or_init(|| process::login_path(&self.inner.ctx.paths.home_dir, &self.inner.stop))
            .await;
        let mut cmd = process::command(binary);
        cmd.env("PATH", path);
        cmd
    }
    async fn read_config(&self) -> CoreResult {
        let ctx = self.inner.ctx.clone();
        tokio::task::spawn_blocking(move || ctx.config.read())
            .await
            .map_err(err)?
    }
    async fn patch_config(&self, patch: Value) -> CoreResult {
        let ctx = self.inner.ctx.clone();
        tokio::task::spawn_blocking(move || ctx.config.patch(&patch))
            .await
            .map_err(err)?
    }
    async fn ensure_gateway(&self) -> Result<(String, String), String> {
        let mut slot = self.inner.gateway.lock().await;
        if slot.is_none() {
            if self.inner.stop.is_cancelled() {
                return Err("AI service is shut down".into());
            }
            *slot = Some(gateway::Gateway::start(self.inner.ctx.clone()).await?);
        }
        let gateway = slot.as_ref().unwrap();
        Ok((gateway.endpoint.clone(), gateway.token.clone()))
    }
    fn agent_event(&self, id: &str, mut event: Value) {
        if matches!(
            s(&event["type"]),
            "stage" | "message" | "answer" | "approval"
        ) {
            let mut state = self.inner.state.lock().unwrap();
            if let Some(run) = state.agent.as_mut().filter(|r| r.id == id) {
                run.emitted = true;
            }
        }
        event["runId"] = json!(id);
        self.inner.ctx.emit("agent:event", event);
    }
    fn cancel_run(&self, expected: Option<&str>, take: bool) {
        let mut state = self.inner.state.lock().unwrap();
        if expected.is_some_and(|id| !state.agent.as_ref().is_some_and(|r| r.id == id)) {
            return;
        }
        if let Some(run) = &state.agent {
            run.cancel.cancel();
        }
        state.approvals.clear();
        if take {
            state.agent = None;
        }
    }
    fn finish_run(&self, id: &str) {
        let mut state = self.inner.state.lock().unwrap();
        state.approvals.retain(|_, a| a.run_id != id);
        if state.agent.as_ref().is_some_and(|r| r.id == id) {
            state.agent = None;
        }
    }
    fn retry_allowed(&self, id: &str, error: &str, cancel: &Cancel) -> bool {
        !cancel.is_cancelled()
            && !regex::Regex::new(
                "(?i)aborted|cancelled|task is empty|invalid base64|unsupported agent image",
            )
            .unwrap()
            .is_match(error)
            && self
                .inner
                .state
                .lock()
                .unwrap()
                .agent
                .as_ref()
                .is_some_and(|r| r.id == id && !r.emitted)
    }
    async fn generate_chat_meta(&self, prompt: &str, answer: &str) -> CoreResult {
        if let Ok(cfg) = self.cfg(Some("chat"), None).await {
            let cancel = Cancel::new();
            let mut prompt_content = format!("User prompt: {prompt}");
            if !answer.is_empty() {
                let snippet = if answer.len() > 300 { &answer[..300] } else { answer };
                prompt_content.push_str(&format!("\nAssistant response snippet: {snippet}"));
            }
            let system_prompt = "You are a concise title and icon generator for a launcher chat assistant.\nGenerate a very short, descriptive conversation title (2-5 words, max 35 characters) and a single fitting theme icon identifier.\nRules:\n1. Output strictly valid JSON matching this schema: {\"title\": \"...\", \"icon\": \"...\"}\n2. \"title\" must be concise and descriptive in the language of the prompt (Russian if Russian, English if English). Do not use quotes or boilerplate questions like 'How to' or 'What is'.\n3. \"icon\" must be one of these theme keys: \"snippets\" (coding/dev/MCP), \"key\" (API keys/auth/passwords), \"cpu\" (system/hardware/CPU), \"trophy\" (sports/Messi/achievements), \"edit\" (writing/summaries/notes), \"ai\" (AI/LLM/prompts), \"search\" (research/lookup), \"weather\" (forecast), \"audio\" (music), \"terminal\" (bash/cli), or \"chat\" (general questions).\n4. Return ONLY the raw JSON object, no markdown, no backticks, no explanations.";
            let messages = vec![
                json!({"role": "system", "content": system_prompt}),
                json!({"role": "user", "content": prompt_content}),
            ];
            let stream_req = self.stream(&cfg, messages, &cancel, |_| {});
            if let Ok(Ok(raw)) = tokio::time::timeout(Duration::from_secs(8), stream_req).await {
                cancel.cancel();
                if let Some(parsed) = parse_meta_json(&raw) {
                    return Ok(parsed);
                }
            }
            cancel.cancel();
        }
        Ok(heuristic_meta(prompt, answer))
    }
    async fn launch(
        &self,
        task: String,
        turns: Option<Vec<Value>>,
        images: Vec<Value>,
        cwd: Option<String>,
        requested: Option<String>,
    ) -> CoreResult {
        let id = requested.unwrap_or_else(|| {
            format!(
                "{}-{}",
                if turns.is_some() { "chat" } else { "agent" },
                uuid::Uuid::new_v4()
            )
        });
        let cancel = Cancel::new();
        self.cancel_run(None, true);
        self.inner.state.lock().unwrap().agent = Some(Run {
            id: id.clone(),
            cancel: cancel.clone(),
            emitted: false,
        });
        self.agent_event(&id, json!({"type":"start","task":task}));
        let core = self.clone();
        let run = id.clone();
        self.spawn(async move{let result=async{let cfg=core.cfg(Some("chat"),None).await?;let mut attempts=0;loop{let result=if let Some(turns)=&turns{let mut messages=vec![json!({"role":"system","content":"You are Tezbar, a helpful assistant. Answer clearly and concisely unless the user asks for more detail."})];messages.extend(turns.iter().skip(turns.len().saturating_sub(16)).map(|t|json!({"role":t["role"],"content":t["text"]})));core.stream(&cfg,messages,&cancel,|delta|{if let Some(text)=delta["text"].as_str(){if !text.is_empty(){core.agent_event(&run,json!({"type":"message","delta":text}));}}}).await.and_then(|text|if text.trim().is_empty(){Err("The model returned an empty response. Please try again.".into())}else{core.agent_event(&run,json!({"type":"answer","text":text}));Ok(text)})}else{core.pi_run(&task,&images,cwd.as_deref(),&cfg,&cancel,Some(&run)).await};match result{Ok(_)=>return Ok::<(),String>(()),Err(error) if attempts==0&&core.retry_allowed(&run,&error,&cancel)=>{attempts+=1;core.agent_event(&run,json!({"type":"log","source":"stderr","line":"Provider did not start cleanly. Retrying once..."}));tokio::select!{_=cancel.cancelled()=>return Err("Agent run aborted".into()),_=tokio::time::sleep(Duration::from_millis(350))=>{}}},Err(error)=>return Err(error)}}}.await;
            if let Err(message)=result{if !cancel.is_cancelled(){core.agent_event(&run,json!({"type":"error","message":message}));}}core.agent_event(&run,json!({"type":"done"}));core.finish_run(&run);
        });
        Ok(json!({"ok":true,"runId":id}))
    }
    pub async fn handle(&self, channel: &str, args: &[Value]) -> HandlerResult {
        if !CHANNELS.contains(&channel) {
            return None;
        }
        if self.inner.stop.is_cancelled() {
            return Some(Err("AI service is shut down".into()));
        }
        Some(self.dispatch(channel, args).await)
    }
    async fn dispatch(&self, channel: &str, args: &[Value]) -> CoreResult {
        let a = arg(args, 0);
        match channel {
            "llm-config-get" => {
                let raw = self.read_config().await?;
                let mut value = config::selected(&raw, None, None);
                for (k, v) in raw.as_object().ok_or("Configuration must be an object")? {
                    value[k] = v.clone();
                }
                for (k, default) in [
                    ("uiStateRetentionMs", 60000),
                    ("extensionRuntimeTimeoutMs", 300000),
                    ("aiModeTimeoutMs", 300000),
                    ("terminalModeTimeoutMs", 300000),
                ] {
                    if !value[k].as_f64().is_some_and(|n| n >= 0.0) {
                        value[k] = json!(default);
                    }
                }
                let hotkey = s(&value["raymesHotkey"]);
                if hotkey.trim().is_empty()
                    || cfg!(windows) && hotkey.eq_ignore_ascii_case("alt+space")
                {
                    value["raymesHotkey"] = json!(if cfg!(windows) {
                        "Control+Space"
                    } else {
                        "Alt+Space"
                    });
                }
                Ok(value)
            }
            "llm-config-set" => {
                let mut patch = a
                    .as_object()
                    .ok_or("LLM configuration patch must be an object")?
                    .clone();
                let mut result = Value::Null;
                if let Some(hotkey) = patch.remove("raymesHotkey") {
                    let hotkey = hotkey.as_str().ok_or("raymesHotkey must be a string")?;
                    result = self
                        .inner
                        .ctx
                        .request("core:platform:update-launcher-hotkey", vec![json!(hotkey)])
                        .await?;
                    if result["ok"] != true {
                        return Ok(result);
                    }
                    // The platform route persists the registered, normalized accelerator.
                    // Do not overwrite it with the caller's original spelling.
                }
                self.patch_config(json!(patch)).await?;
                self.inner.ctx.emit("llm-config-changed", json!({}));
                Ok(result)
            }
            "llm-provider-statuses" => {
                let raw = self.read_config().await?;
                let ids = config::ids(&raw);
                let requests = ids.iter().map(|id| async {
                    let cfg = config::selected(&raw, None, Some(id));
                    (id.clone(), json!(self.available(&cfg).await))
                });
                let mut statuses = serde_json::Map::new();
                for (id, status) in futures_util::future::join_all(requests).await {
                    statuses.insert(id, status);
                }
                Ok(json!(statuses))
            }
            "llm-list-models" => {
                let id = a.as_str().ok_or("Provider id must be a string")?;
                if !config::ids(&self.read_config().await?)
                    .iter()
                    .any(|p| p == id)
                    && !id.starts_with("custom:")
                {
                    return Err("Unknown AI provider".into());
                }
                self.models(id, arg(args, 1).as_str(), arg(args, 2).as_str())
                    .await
            }
            "github-device-start" => self.device_start(a.as_str()).await,
            "github-device-poll" => self.device_poll().await,
            "github-device-cancel" => {
                let mut state = self.inner.state.lock().unwrap();
                state.device = None;
                state.device_generation += 1;
                Ok(Value::Null)
            }
            "pi:check" => Ok(self.pi_check().await),
            "pi:install" => Ok(self.pi_install().await),
            "pi-extensions:list" => self.pi_extensions().await,
            "agent:run" => {
                let owned;
                if a.is_string() {
                    owned = json!({"task":a});
                } else {
                    owned = a.clone();
                }
                let task = s(&owned["task"]);
                if task.trim().is_empty() {
                    return Ok(json!({"ok":false,"error":"Task is empty"}));
                }
                if !owned["cwd"].is_null() && !owned["cwd"].is_string() {
                    return Ok(json!({"ok":false,"error":"Invalid agent working directory"}));
                }
                let requested = owned["runId"].as_str();
                if !owned["runId"].is_null()
                    && !requested.is_some_and(|id| {
                        regex::Regex::new("^agent-[a-zA-Z0-9-]{1,100}$")
                            .unwrap()
                            .is_match(id)
                    })
                {
                    return Ok(json!({"ok":false,"error":"Invalid agent run id"}));
                }
                let cwd = owned["cwd"]
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let cwd = if let Some(cwd) = cwd {
                    let expanded = if cwd == "~" {
                        self.inner.ctx.paths.home_dir.clone()
                    } else if let Some(rest) = cwd.strip_prefix("~/") {
                        self.inner.ctx.paths.home_dir.join(rest)
                    } else {
                        std::path::PathBuf::from(cwd)
                    };
                    if !tokio::fs::metadata(&expanded)
                        .await
                        .is_ok_and(|m| m.is_dir())
                    {
                        return Ok(
                            json!({"ok":false,"error":format!("Directory does not exist: {cwd}")}),
                        );
                    }
                    Some(expanded.to_string_lossy().into_owned())
                } else {
                    None
                };
                let images = owned["images"].as_array().cloned().unwrap_or_default();
                if let Err(error) = pi::normalize_images(&images) {
                    return Ok(json!({"ok":false,"error":error}));
                }
                self.launch(
                    task.into(),
                    None,
                    images,
                    cwd,
                    requested.map(str::to_string),
                )
                .await
            }
            "agent:cancel" => {
                self.cancel_run(a.as_str(), false);
                Ok(json!({"ok":true}))
            }
            "cancel" => {
                self.cancel_run(None, false);
                if let Some(run) = &self.inner.state.lock().unwrap().answer {
                    run.cancel.cancel();
                }
                Ok(Value::Null)
            }
            "agent:approve" => self.approve(a).await,
            "agent:capture-active-screen" => {
                self.inner
                    .ctx
                    .request(
                        "core:platform:screenshot",
                        vec![json!({"maxWidth":1600,"hideLauncher":true})],
                    )
                    .await
            }
            "chat:run" => {
                let Some(turns) = a.as_array().filter(|t| !t.is_empty()) else {
                    return Ok(json!({"ok":false,"error":"Invalid chat run payload"}));
                };
                if turns.iter().any(|t| {
                    !t["id"].is_string()
                        || !matches!(s(&t["role"]), "user" | "assistant")
                        || !t["text"].is_string()
                        || !t["createdAt"].is_number()
                }) {
                    return Ok(json!({"ok":false,"error":"Invalid chat run payload"}));
                }
                self.launch(
                    s(&turns.last().unwrap()["text"]).into(),
                    Some(turns.clone()),
                    Vec::new(),
                    None,
                    None,
                )
                .await
            }
            "chat:list" | "chat:get" | "chat:append" | "chat:update-title" | "chat:delete-turn"
            | "chat:delete" | "chat:clear" => chat::handle(&self.inner.ctx, channel, args).await,
            "chat:generate-meta" => {
                let id = a["id"].as_str().ok_or("Session id must be a string")?;
                let prompt = a["prompt"].as_str().unwrap_or("").trim();
                let answer = a["answer"].as_str().unwrap_or("").trim();
                if prompt.is_empty() {
                    return Err("Prompt is required".into());
                }
                let meta = self.generate_chat_meta(prompt, answer).await?;
                let title = meta["title"].as_str().unwrap_or("New chat");
                let icon = meta["icon"].as_str().unwrap_or("💬");
                let _ = chat::update_title_and_icon(&self.inner.ctx, id, title, icon).await;
                Ok(meta)
            }
            "query" => {
                let text = a.as_str().ok_or("Query must be a string")?.trim();
                let intent = classify(text);
                match s(&intent["type"]) {
                    "agent" => {
                        self.launch(text.into(), None, Vec::new(), None, None)
                            .await?;
                    }
                    "ai" | "answer" => self.answer(text.to_string()),
                    _ => {}
                }
                Ok(intent)
            }
            "core:ai:pi-query" => self.pi_query(a, arg(args, 1).as_str()).await,
            "core:ai:pi-observe" => self.pi_observe(a.as_str()).await,
            "core:ai:extension-ask" => {
                let prompt = a
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or("AI prompt is required")?;
                if !arg(args, 1).is_null() && !arg(args, 1).is_object() {
                    return Err("Extension AI options must be an object".into());
                }
                let cfg = self.cfg(Some("action"), None).await?;
                let cancel = Cancel::new();
                let request=self.stream(&cfg,vec![json!({"role":"system","content":"You are answering an AI request from a Raycast-compatible extension. Return only the requested result, without commentary or tool use."}),json!({"role":"user","content":prompt})],&cancel,|_|{});
                let result = tokio::select! {_=self.inner.stop.cancelled()=>Err("AI service shut down".into()),result=tokio::time::timeout(Duration::from_secs(60),request)=>result.map_err(|_|"Extension AI request timed out".to_string())?};
                cancel.cancel();
                Ok(json!(result?.trim()))
            }
            "ai:action" => {
                let instruction = a["instruction"]
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or("AI action instruction is required")?;
                let cfg = self.cfg(Some("action"), None).await?;
                if cfg["aiActionRequirePermission"] != false && a["allowAutomation"] != true {
                    return Ok(
                        json!({"ok":false,"output":"Action mode requires explicit permission. Retry with allowAutomation=true."}),
                    );
                }
                let redact = a["redactSensitive"]
                    .as_bool()
                    .unwrap_or(cfg["aiActionRedactionEnabled"] != false);
                let context = |key| {
                    if redact {
                        redact_text(s(&a[key]))
                    } else {
                        s(&a[key]).to_string()
                    }
                };
                let selected = context("selectedText");
                let app = context("appContext");
                let prompt=format!("Instruction: {instruction}\n\nAutomation permission: {}\n\nSelected text:\n{}\n\nApp context:\n{}",if a["allowAutomation"]==true{"granted"}else{"denied"},if selected.is_empty(){"(none)"}else{&selected},if app.is_empty(){"(none)"}else{&app});
                let output=self.stream(&cfg,vec![json!({"role":"system","content":"You are Tezbar Action Mode. Produce concise, executable steps and concrete output. Never execute system actions unless explicitly allowed."}),json!({"role":"user","content":prompt})],&self.inner.stop,|_|{}).await?;
                Ok(json!({"ok":true,"output":output.trim()}))
            }
            c if c.starts_with("voice:") => self.inner.voice.handle(c, args).await,
            _ => Err(format!("Unknown AI channel: {channel}")),
        }
    }
}
fn redact_text(input: &str) -> String {
    let mut text = input.to_string();
    for (pattern, replacement) in [
        (
            "(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----",
            "[REDACTED_PRIVATE_KEY]",
        ),
        (r"\bsk-[A-Za-z0-9_-]{12,}\b", "[REDACTED_API_KEY]"),
        (r"\bgh[pousr]_[A-Za-z0-9_]{12,}\b", "[REDACTED_TOKEN]"),
        (r"\bAKIA[0-9A-Z]{16}\b", "[REDACTED_AWS_ACCESS_KEY]"),
        (
            r"(?i)\b(bearer\s+)[A-Za-z0-9._~+/-]{16,}",
            "${1}[REDACTED_TOKEN]",
        ),
        (
            r"(?i)\b(password|passwd|api[_-]?key|secret|token)\s*[=:]\s*([^\s,;]+)",
            "${1}=[REDACTED]",
        ),
    ] {
        text = regex::Regex::new(pattern)
            .unwrap()
            .replace_all(&text, replacement)
            .into_owned();
    }
    text
}
fn classify(input: &str) -> Value {
    let lower = input.to_lowercase();
    let test = |pattern: &str| regex::Regex::new(pattern).unwrap().is_match(&lower);
    if let Some(body) = input.strip_prefix('/') {
        let body = body.trim();
        let (name, args) = body.split_once(char::is_whitespace).unwrap_or((body, ""));
        return json!({"type":"extension","name":name,"args":args.trim()});
    }
    for (pattern, action) in [
        (
            r"\b(disable|turn off|switch off)\b.*\b(wi-?fi|wifi)\b",
            "wifi-off",
        ),
        (
            r"\b(turn|switch|set|enable)\b.*\b(wi-?fi|wifi)\b",
            "wifi-on",
        ),
        (
            r"\b(enable|turn on|set)\b.*\b(dark mode|dark)\b",
            "dark-mode-on",
        ),
        (
            r"\b(disable|turn off)\b.*\b(dark mode|dark)\b",
            "dark-mode-off",
        ),
        (r"\b(mute|silence)\b", "mute-on"),
        (r"\b(unmute|sound on)\b", "mute-off"),
    ] {
        if test(pattern) {
            return json!({"type":"system","action":action});
        }
    }
    if matches!(lower.as_str(), "quit" | "calculator") {
        return json!({"type":"system","action":lower});
    }
    if lower.starts_with("open ") {
        return json!({"type":"application","target":input[5..].trim().trim_end_matches('?').trim()});
    }
    if test(r"\b(file|folder|path|directory|finder|desktop|documents|downloads)\b") {
        return json!({"type":"file","query":input});
    }
    if test(r"\b(open|launch|start)\s+") {
        let target = regex::Regex::new(r"(?i)^\s*(open|launch|start)\s+")
            .unwrap()
            .replace(input, "")
            .trim()
            .to_string();
        if !target.is_empty() {
            return json!({"type":"application","target":target});
        }
    }
    if input.len() < 120
        && (input.ends_with('?') || test(r"^(what|why|how|who|when|is|are|can|does)\b"))
        || test(r"\b(explain|summarize|write|draft|generate)\b")
    {
        return json!({"type":"ai","input":input});
    }
    if test(r"\b(run|execute|toggle|enable|disable|kill|stop|start|turn)\b") {
        return json!({"type":"command","command":input,"confidence":0.62});
    }
    json!({"type":"agent","input":input})
}

fn clean_prompt_snippet(prompt: &str) -> String {
    let first_line = prompt.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let mut cleaned = first_line.trim().to_string();
    let prefixes = [
        "can you please", "could you please", "please tell me", "tell me",
        "can you", "could you", "who is", "who was", "what is", "what was",
        "what are", "where is", "when is", "when did", "when was",
        "how to", "how do i", "how does", "why is", "why does", "explain",
        "help me with", "i need to", "look up",
        "подскажи пожалуйста", "расскажи пожалуйста", "подскажи", "расскажи",
        "пожалуйста", "кто такой", "кто такая", "что такое", "что за",
        "как сделать", "как настроить", "как вообще", "как", "почему", "зачем",
        "когда вышел", "когда выйдет", "когда", "объясни", "найди",
    ];
    let mut changed = true;
    while changed {
        changed = false;
        let lower = cleaned.to_lowercase();
        for p in &prefixes {
            if lower.starts_with(p) && (lower.len() == p.len() || lower[p.len()..].starts_with(' ')) {
                cleaned = cleaned[p.len()..].trim_start().to_string();
                changed = true;
                break;
            }
        }
    }

    while cleaned.ends_with('?') || cleaned.ends_with('!') || cleaned.ends_with('.') || cleaned.ends_with(':') {
        cleaned.pop();
    }
    cleaned = cleaned.trim().to_string();

    let suffixes = [
        "when it came out", "when did it come out", "when was it released",
        "когда вышел", "когда выйдет", "как вообще генерить", "как генерить",
        "как сделать", "как настроить", "как использовать",
    ];
    let mut changed_s = true;
    while changed_s {
        changed_s = false;
        let lower = cleaned.to_lowercase();
        for s in &suffixes {
            if lower.ends_with(s) && (lower.len() == s.len() || lower[..lower.len() - s.len()].ends_with(' ')) {
                cleaned = cleaned[..cleaned.len() - s.len()].trim_end().to_string();
                changed_s = true;
                break;
            }
        }
    }

    while cleaned.ends_with('?') || cleaned.ends_with('!') || cleaned.ends_with('.') || cleaned.ends_with(':') {
        cleaned.pop();
    }
    cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() {
        return "New chat".to_string();
    }
    let mut chars = cleaned.chars();
    let first = chars.next().unwrap().to_uppercase().to_string();
    let rest: String = chars.collect();
    let capitalized = format!("{first}{rest}");
    if capitalized.chars().count() > 36 {
        let truncated: String = capitalized.chars().take(36).collect();
        format!("{truncated}…")
    } else {
        capitalized
    }
}

pub fn heuristic_meta(prompt: &str, answer: &str) -> Value {
    let combined = format!("{prompt} {answer}").to_lowercase();

    if combined.contains("messi") || combined.contains("месси") {
        return json!({"title": "Lionel Messi", "icon": "trophy"});
    }
    if combined.contains("ronaldo") || combined.contains("роналду") || combined.contains("криштиану") {
        return json!({"title": "Cristiano Ronaldo", "icon": "trophy"});
    }
    if combined.contains("cpu") || combined.contains("processor") || combined.contains("процессор") || combined.contains("нагрузк") {
        if combined.contains("compiler") || combined.contains("build") || combined.contains("idle") || combined.contains("compil") {
            return json!({"title": "CPU Usage & Compiler Memory", "icon": "cpu"});
        }
        return json!({"title": "CPU & Process Diagnostics", "icon": "cpu"});
    }
    if combined.contains("api key") || combined.contains("api-key") || combined.contains("apikey") || combined.contains("токен") || combined.contains("ключ") {
        let mut title = clean_prompt_snippet(prompt);
        if title == "New chat" {
            title = "API Key".into();
        }
        return json!({"title": title, "icon": "key"});
    }
    if combined.contains("mcp") {
        let mut title = clean_prompt_snippet(prompt);
        if title == "New chat" {
            title = "MCP Protocol".into();
        } else {
            title = title.replace("version", "Version");
        }
        return json!({"title": title, "icon": "snippets"});
    }

    let icon = if combined.contains("football") || combined.contains("soccer") || combined.contains("fifa") || combined.contains("футбол") {
        "trophy"
    } else if combined.contains("basketball") || combined.contains("nba") || combined.contains("баскетбол") {
        "sports"
    } else if combined.contains("tennis") || combined.contains("теннис") {
        "sports"
    } else if combined.contains("rust") || combined.contains("python") || combined.contains("typescript") || combined.contains("javascript") || combined.contains("golang") || combined.contains("react") || combined.contains("code") || combined.contains("кодинг") || combined.contains("баг") || combined.contains("ошибк") {
        "snippets"
    } else if combined.contains("mac") || combined.contains("terminal") || combined.contains("bash") || combined.contains("zsh") || combined.contains("linux") || combined.contains("windows") || combined.contains("диск") || combined.contains("ssd") || combined.contains("памят") || combined.contains("процесс") {
        "cpu"
    } else if combined.contains("weather") || combined.contains("rain") || combined.contains("snow") || combined.contains("погода") || combined.contains("дождь") || combined.contains("температур") {
        "weather"
    } else if combined.contains("music") || combined.contains("song") || combined.contains("spotify") || combined.contains("песн") || combined.contains("музык") || combined.contains("трек") {
        "audio"
    } else if combined.contains("crypto") || combined.contains("bitcoin") || combined.contains("btc") || combined.contains("eth") || combined.contains("валют") || combined.contains("деньг") || combined.contains("биткоин") || combined.contains("курс") {
        "brew"
    } else if combined.contains("ai") || combined.contains("gpt") || combined.contains("llm") || combined.contains("claude") || combined.contains("gemini") || combined.contains("нейросет") || combined.contains("модел") {
        "ai"
    } else if combined.contains("auth") || combined.contains("password") || combined.contains("пароль") || combined.contains("логин") || combined.contains("secret") {
        "key"
    } else if combined.contains("movie") || combined.contains("film") || combined.contains("фильм") || combined.contains("кино") || combined.contains("сериал") {
        "picture"
    } else if combined.contains("food") || combined.contains("coffee") || combined.contains("еда") || combined.contains("кофе") || combined.contains("рецепт") {
        "coffee"
    } else if combined.contains("write") || combined.contains("draft") || combined.contains("note") || combined.contains("стать") || combined.contains("письм") {
        "edit"
    } else {
        "chat"
    };

    let title = clean_prompt_snippet(prompt);
    json!({"title": title, "icon": icon})
}

fn parse_meta_json(raw: &str) -> Option<Value> {
    let trimmed = raw.trim();
    let unquoted = if let Some(stripped) = trimmed.strip_prefix("```json") {
        stripped.strip_suffix("```").unwrap_or(stripped).trim()
    } else if let Some(stripped) = trimmed.strip_prefix("```") {
        stripped.strip_suffix("```").unwrap_or(stripped).trim()
    } else {
        trimmed
    };
    let start = unquoted.find('{')?;
    let end = unquoted.rfind('}')?;
    if end <= start {
        return None;
    }
    let slice = &unquoted[start..=end];
    let val: Value = serde_json::from_str(slice).ok()?;
    let title = val["title"].as_str()?.trim();
    let icon = val["icon"].as_str()?.trim();
    if title.is_empty() || icon.is_empty() {
        return None;
    }
    Some(json!({"title": title, "icon": icon}))
}
