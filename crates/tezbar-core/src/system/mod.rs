//! Rust-owned system services. OS operations run asynchronously; profile writes are serialized.
use super::common::{now_ms, CoreContext, CoreResult, HandlerResult};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    process::{Child, Command},
    sync::{Mutex, Semaphore},
};
mod assets;
mod background;
mod commands;
mod permissions;
mod safety;
mod stats;
#[cfg(test)]
mod tests;

pub const CHANNELS: &[&str] = &[
    "permissions:snapshot",
    "permissions:request",
    "safety:descriptors",
    "safety:log",
    "safety:log-clear",
    "safety:dry-run:get",
    "safety:dry-run:set",
    "native-commands:list",
    "core:native:execute",
    "core:commands:execute",
    "system-stats:get",
    "app-icon:data-url",
    "asset-icon:data-url",
    "currency:frankfurter-latest",
    "file:quick-look",
    "background-tasks:list",
    "core:safety:authorize",
    "core:safety:record",
];
#[derive(Clone)]
pub struct SystemCore {
    ctx: CoreContext,
    state: Arc<State>,
}
struct State {
    currency: Mutex<HashMap<String, (u64, Value)>>,
    audit: Mutex<()>,
    keep_awake: Mutex<Option<Child>>,
    icons: Mutex<HashMap<String, Value>>,
    icon_slots: Semaphore,
    stats: Mutex<stats::StatsState>,
}
impl SystemCore {
    pub fn new(ctx: CoreContext) -> Self {
        Self {
            ctx,
            state: Arc::new(State {
                currency: Mutex::new(HashMap::new()),
                audit: Mutex::new(()),
                keep_awake: Mutex::new(None),
                icons: Mutex::new(HashMap::new()),
                icon_slots: Semaphore::new(2),
                stats: Mutex::new(Default::default()),
            }),
        }
    }
    pub async fn shutdown(&self) {
        if let Some(mut child) = self.state.keep_awake.lock().await.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    }
    pub async fn handle(&self, channel: &str, args: &[Value]) -> HandlerResult {
        if !CHANNELS.contains(&channel) {
            return None;
        }
        Some(self.dispatch(channel, args).await)
    }
    async fn dispatch(&self, c: &str, a: &[Value]) -> CoreResult {
        let first = a.first().cloned().unwrap_or(Value::Null);
        match c {
            "permissions:snapshot" => self.permissions(None).await,
            "permissions:request" => self.permissions(Some(string(&first)?)).await,
            "safety:descriptors" => Ok(safety::descriptors()),
            "safety:log" => self.audit_log(false).await,
            "safety:log-clear" => {
                self.audit_log(true).await?;
                Ok(Value::Null)
            }
            "safety:dry-run:get" => Ok(json!(self.dry_run().await?)),
            "safety:dry-run:set" => {
                let ctx = self.ctx.clone();
                blocking(move || {
                    ctx.config.patch(&json!({"safetyDryRun":first==true}))?;
                    Ok(json!(first == true))
                })
                .await
            }
            "native-commands:list" => Ok(commands::descriptors()),
            "core:native:execute" => self.execute_native(&first).await,
            "core:commands:execute" => {
                self.execute_builtin(string(&first)?, a.get(1).unwrap_or(&Value::Null))
                    .await
            }
            "system-stats:get" => self.system_stats().await,
            "app-icon:data-url" => self.icon("application", first.as_str().unwrap_or("")).await,
            "asset-icon:data-url" => {
                self.icon(
                    first["kind"].as_str().unwrap_or(""),
                    first["path"].as_str().unwrap_or(""),
                )
                .await
            }
            "currency:frankfurter-latest" => self.currency(string(&first)?).await,
            "file:quick-look" => self.quick_look(&first).await,
            "background-tasks:list" => self.background_tasks().await,
            "core:safety:authorize" => {
                self.authorize(string(&first)?, a.get(1).cloned().unwrap_or(json!({})))
                    .await
            }
            "core:safety:record" => {
                self.record(
                    string(&first)?,
                    a.get(1).cloned().unwrap_or(json!({})),
                    a.get(2).cloned().unwrap_or(json!({})),
                )
                .await?;
                Ok(Value::Null)
            }
            _ => unreachable!(),
        }
    }
    async fn currency(&self, raw: &str) -> CoreResult {
        let base = raw.trim().to_ascii_uppercase();
        if base.len() != 3 || !base.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(format!("Invalid currency code: {raw}"));
        }
        let mut cache = self.state.currency.lock().await;
        if let Some((at, payload)) = cache.get(&base) {
            if now_ms().saturating_sub(*at) < 21_600_000 {
                return Ok(payload.clone());
            }
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?;
        let mut errors = Vec::new();
        for (url, primary) in [
            (format!("https://open.er-api.com/v6/latest/{base}"), true),
            (
                format!("https://api.frankfurter.app/latest?from={base}"),
                false,
            ),
        ] {
            let result = async {
                let v: Value = client
                    .get(url)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .await
                    .map_err(|e| e.to_string())?;
                currency_payload(&base, &v, primary)
            }
            .await;
            match result {
                Ok(v) => {
                    if cache.len() > 32 {
                        cache.clear();
                    }
                    cache.insert(base, (now_ms(), v.clone()));
                    return Ok(v);
                }
                Err(e) => errors.push(e),
            }
        }
        Err(errors.join("; "))
    }
}
fn currency_payload(base: &str, v: &Value, primary: bool) -> CoreResult {
    if primary && v["result"].as_str().is_some_and(|r| r != "success") {
        return Err("Exchange rate provider rejected request".into());
    }
    let rates = v["rates"]
        .as_object()
        .filter(|r| {
            !r.is_empty()
                && r.values()
                    .all(|x| x.as_f64().is_some_and(|n| n.is_finite() && n > 0.0))
        })
        .ok_or("Invalid exchange rates response")?;
    Ok(
        json!({"base":base,"date":v[if primary{"time_last_update_utc"}else{"date"}].as_str().unwrap_or(""),"rates":rates}),
    )
}
pub(super) async fn blocking<F>(f: F) -> CoreResult
where
    F: FnOnce() -> CoreResult + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}
pub(super) fn string(v: &Value) -> Result<&str, String> {
    v.as_str().ok_or_else(|| "Expected a string".into())
}
pub(super) fn result(ok: bool, message: impl Into<String>) -> Value {
    json!({"ok":ok,"message":message.into()})
}
pub(super) async fn output(mut command: Command, timeout: Duration) -> Result<String, String> {
    command.kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let out = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| "Native command timed out".to_string())?
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("{}", String::from_utf8_lossy(&out.stderr).trim())
            .trim()
            .to_owned()
            .chars()
            .take(2000)
            .collect::<String>()
            .pipe_nonempty(format!("Native command exited {}", out.status)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
trait Nonempty {
    fn pipe_nonempty(self, fallback: String) -> String;
}
impl Nonempty for String {
    fn pipe_nonempty(self, fallback: String) -> String {
        if self.is_empty() {
            fallback
        } else {
            self
        }
    }
}
pub(super) async fn run(file: &str, args: &[&str]) -> Result<String, String> {
    let mut c = Command::new(file);
    c.args(args);
    output(c, Duration::from_secs(15)).await
}
pub(super) async fn apple(script: &str) -> Result<String, String> {
    run("/usr/bin/osascript", &["-e", script]).await
}
pub(super) async fn shell(script: &str) -> Result<String, String> {
    run("/bin/bash", &["-lc", script]).await
}
pub(super) async fn ps(script: &str) -> Result<String, String> {
    run(
        "powershell.exe",
        &[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!("$ErrorActionPreference='Stop'; {script}"),
        ],
    )
    .await
}
