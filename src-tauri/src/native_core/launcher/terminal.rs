use super::stores::string;
use crate::native_core::common::{now_ms, CoreContext, CoreResult};
use serde_json::{json, Value};
use tauri::Manager;
#[derive(Clone)]
pub struct TerminalCore {
    ctx: CoreContext,
}
fn period(value: &str, keep: bool) -> Option<u64> {
    match (value, keep) {
        ("day", _) => Some(86_400_000),
        ("week", false) => Some(604_800_000),
        ("month", false) => Some(2_592_000_000),
        ("3h", true) => Some(10_800_000),
        ("8h", true) => Some(28_800_000),
        _ => None,
    }
}
fn valid(value: &str, keep: bool) -> bool {
    if keep {
        ["3h", "8h", "day", "until-stop"].contains(&value)
    } else {
        ["day", "week", "month", "forever"].contains(&value)
    }
}
fn deadline(v: &str, now: u64, keep: bool) -> Value {
    period(v, keep)
        .map(|d| json!(now + d))
        .unwrap_or(Value::Null)
}
fn command(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(4096)
        .collect()
}
fn default_name(cwd: &str, cmd: &str) -> String {
    let path = cwd.replace('\\', "/");
    let parts: Vec<_> = path.split('/').filter(|s| !s.is_empty()).collect();
    let dir = if parts.len() == 2 && parts[0] == "Users" {
        "~".into()
    } else if parts.len() == 3 && parts[0] == "Users" {
        format!("~/{}", parts[2])
    } else if parts.len() >= 4 && parts[0].ends_with(':') && parts[1] == "Users" {
        format!("~/{}", parts[3..].join("/"))
    } else if parts.is_empty() {
        "/".into()
    } else {
        parts[parts.len().saturating_sub(2)..].join("/")
    };
    let cmd = command(cmd.lines().next().unwrap_or(""));
    if cmd.is_empty() {
        dir
    } else {
        format!("{dir} · {cmd}")
    }
}
fn name(raw: Option<&str>, cwd: &str, cmd: &str) -> String {
    let s = raw.map(command).unwrap_or_default();
    if s.is_empty() {
        default_name(cwd, cmd)
    } else {
        s.chars().take(120).collect()
    }
}
fn strip_nulls(v: &mut Value) {
    if let Some(o) = v.as_object_mut() {
        o.retain(|_, v| !v.is_null());
    }
}
impl TerminalCore {
    pub fn new(ctx: CoreContext) -> Self {
        Self { ctx }
    }
    pub async fn initialize(&self) -> CoreResult {
        let ctx = self.ctx.clone();
        tokio::task::spawn_blocking(move || {
            let mut changed = false;
            ctx.config.update(|config| {
                let before = config["terminalSessions"].clone();
                if let Some(sessions) = config["terminalSessions"].as_array_mut() {
                    sessions.retain(|s| {
                        s["sessionId"].is_string() && s["name"].is_string() && s["cwd"].is_string()
                    });
                    for s in sessions.iter_mut() {
                        s["status"] = json!("exited");
                        if let Some(o) = s.as_object_mut() {
                            o.remove("pid");
                            o.remove("keepAliveUntil");
                        }
                    }
                }
                changed = config["terminalSessions"] != before;
                Ok(())
            })?;
            if changed {
                ctx.emit("terminal:sessions-changed", json!({}));
            }
            Ok(Value::Null)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    pub async fn tick(&self) -> Result<(), String> {
        let ctx = self.ctx.clone();
        let expired = tokio::task::spawn_blocking(move || {
            let config = ctx.config.read()?;
            Ok::<_, String>(
                config["terminalSessions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|s| {
                        s["status"] == "running"
                            && s["keepAliveUntil"].as_u64().is_some_and(|n| n <= now_ms())
                    })
                    .filter_map(|s| s["sessionId"].as_str().map(str::to_owned))
                    .collect::<Vec<_>>(),
            )
        })
        .await
        .map_err(|e| e.to_string())??;
        for id in expired {
            // The native PTY exit adapter owns the final exit code and generation.
            // A successful kill request is not itself evidence that the child exited.
            self.ctx
                .request("core:terminal:pty:kill", vec![json!({"sessionId":id})])
                .await?;
        }
        self.prune().await?;
        Ok(())
    }
    async fn prune(&self) -> CoreResult {
        let ctx = self.ctx.clone();
        tokio::task::spawn_blocking(move || {
            let mut expired = vec![];
            let mut changed = false;
            ctx.config.update(|config| {
                if let Some(items) = config["terminalSessions"].as_array_mut() {
                    items.retain(|s| {
                        let remove = s["status"] != "running"
                            && s["saveUntil"].as_u64().is_some_and(|n| n <= now_ms());
                        if remove {
                            changed = true;
                            if let Some(id) = s["sessionId"].as_str() {
                                expired.push(id.to_owned())
                            }
                        }
                        !remove
                    });
                }
                Ok(())
            })?;
            if changed {
                ctx.emit("terminal:sessions-changed", json!({}));
            }
            for id in expired {
                if valid_id(&id) {
                    for directory in history_dirs(&ctx) {
                        let _ = std::fs::remove_file(directory.join(format!("{id}.log")));
                    }
                }
            }
            Ok(Value::Null)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    pub async fn handle(&self, c: &str, a: &[Value]) -> CoreResult {
        if c == "terminal:get-prompt-info" {
            return tokio::task::spawn_blocking(move || {
                let user = std::env::var("USER")
                    .or_else(|_| std::env::var("USERNAME"))
                    .unwrap_or_default();
                let host = super::stores::interpolate("${hostname}", "");
                Ok(json!({"user":user,"host":host.split('.').next().unwrap_or(&host),"dir":"~"}))
            })
            .await
            .map_err(|e| e.to_string())?;
        }
        let c = if c == "terminal:native-detach" || c == "core:terminal:detach" {
            "terminal:detach"
        } else {
            c
        }
        .to_owned();
        let args = a.to_vec();
        let ctx = self.ctx.clone();
        tokio::task::spawn_blocking(move||{
 if c=="terminal:list"{let config=ctx.config.read()?;let mut rows=config["terminalSessions"].as_array().cloned().unwrap_or_default();rows.retain(|s|s["status"]=="running"||!s["saveUntil"].as_u64().is_some_and(|n|n<=now_ms()));rows.sort_by(|a,b|(a["status"]!="running").cmp(&(b["status"]!="running")).then_with(||b["updatedAt"].as_u64().cmp(&a["updatedAt"].as_u64())));return Ok(json!(rows))}
 let raw=args.first().cloned().unwrap_or(Value::Null);let id=raw["sessionId"].as_str().or_else(||raw.as_str()).ok_or("Terminal sessionId is required")?.to_owned();if !valid_id(&id){return Err("Invalid terminal sessionId".into())}let now=now_ms();let mut result=Value::Null;let mut changed=false;ctx.config.update(|config|{if config["terminalSessions"].is_null(){config["terminalSessions"]=json!([]);changed=true}let rows=config["terminalSessions"].as_array_mut().ok_or("Invalid terminalSessions configuration")?;
 if c=="terminal:delete"{let before=rows.len();rows.retain(|s|s["sessionId"]!=id);let deleted=before!=rows.len();changed|=deleted;result=json!(deleted);return Ok(())}
 if c=="terminal:native-create"{let shell=string(&raw,"shell")?;let cwd=resolve_cwd(&ctx,string(&raw,"cwd")?);let initial=raw["initialCommand"].as_str().unwrap_or("").trim();if initial.len()>16384{return Err("Initial terminal command exceeds 16 KiB".into())}let save=raw["saveFor"].as_str().filter(|s|valid(s,false)).unwrap_or("week");let keep=raw["keepAliveFor"].as_str().filter(|s|valid(s,true)).unwrap_or("3h");let mut summary=json!({"sessionId":id,"shell":shell,"cwd":cwd,"name":name(raw["name"].as_str(),&cwd,initial),"createdAt":now,"updatedAt":now,"lastActiveAt":now,"saveFor":save,"keepAliveFor":keep,"saveUntil":deadline(save,now,false),"status":"running"});if !initial.is_empty(){summary["initialCommand"]=json!(initial);summary["lastCommand"]=json!(command(initial))}strip_nulls(&mut summary);rows.retain(|s|s["sessionId"]!=id);rows.push(summary.clone());result=summary;changed=true;return Ok(())}
 let Some(summary)=rows.iter_mut().find(|s|s["sessionId"]==id)else{return Ok(())};let mut s=summary.clone();match c.as_str(){"terminal:native-attach"|"terminal:native-restore"=>{s["status"]=json!("running");for k in ["exitCode","signal","pid","keepAliveUntil"]{s.as_object_mut().unwrap().remove(k);}if c=="terminal:native-restore"{s["shell"]=json!(string(&raw,"shell")?);s["cwd"]=json!(resolve_cwd(&ctx,string(&raw,"cwd")?));s["saveUntil"]=deadline(s["saveFor"].as_str().unwrap_or("week"),now,false);}},
 "terminal:native-exit"|"core:terminal:exit"|"terminal:exit"=>{s["status"]=json!("exited");s["exitCode"]=json!(raw["exitCode"].as_i64().unwrap_or(1).max(0));s["signal"]=raw["signal"].clone();s["pid"]=Value::Null;s["keepAliveUntil"]=Value::Null;s["saveUntil"]=deadline(s["saveFor"].as_str().unwrap_or("week"),now,false);},
 "terminal:detach"=>s["keepAliveUntil"]=deadline(s["keepAliveFor"].as_str().unwrap_or("3h"),now,true),
 "terminal:update"=>{if let Some(n)=raw["name"].as_str(){s["name"]=json!(name(Some(n),s["cwd"].as_str().unwrap_or(""),s["initialCommand"].as_str().unwrap_or("")))}if let Some(cwd)=raw["cwd"].as_str(){let raw_path=std::path::PathBuf::from(cwd.trim());let path=if raw_path.is_absolute()&&raw_path.is_dir(){raw_path}else{super::providers::resolve(&ctx.paths.home_dir,cwd)};if path.is_dir(){s["cwd"]=json!(path.canonicalize().unwrap_or(path));}}if let Some(cmd)=raw["lastCommand"].as_str(){let cmd=command(cmd);s["lastCommand"]=if cmd.is_empty(){Value::Null}else{json!(cmd)}}if let Some(save)=raw["saveFor"].as_str(){if !valid(save,false){return Err("Invalid terminal saveFor".into())}s["saveFor"]=json!(save);s["saveUntil"]=deadline(save,now,false);}if let Some(keep)=raw["keepAliveFor"].as_str(){if !valid(keep,true){return Err("Invalid terminal keepAliveFor".into())}s["keepAliveFor"]=json!(keep);if s["keepAliveUntil"].is_number(){s["keepAliveUntil"]=deadline(keep,now,true);}}},_=>return Err(format!("Unknown terminal metadata channel: {c}"))}
 s["updatedAt"]=json!(now);s["lastActiveAt"]=json!(now);strip_nulls(&mut s);changed|=*summary!=s;*summary=s.clone();result=s;Ok(())})?;
 if changed{ctx.emit("terminal:sessions-changed",json!({}));}
 if c=="terminal:delete"&&result==true{for directory in history_dirs(&ctx){let path=directory.join(format!("{id}.log"));if path.exists(){std::fs::remove_file(path).map_err(|e|e.to_string())?;}}}
 if c=="terminal:detach"{result=json!(!result.is_null());}
 Ok(result)
 }).await.map_err(|e|e.to_string())?
    }
}
fn history_dirs(ctx: &CoreContext) -> Vec<std::path::PathBuf> {
    let mut directories = vec![
        ctx.paths.config_dir.join("terminal-history"),
        ctx.paths.data_dir.join("terminal-history"),
    ];
    if let Some(app) = &ctx.app {
        if let Ok(path) = app.path().app_data_dir() {
            directories.push(path.join("terminal-history"));
        }
    }
    directories.sort();
    directories.dedup();
    directories
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 200
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}
fn resolve_cwd(ctx: &CoreContext, s: &str) -> String {
    let raw = std::path::PathBuf::from(s.trim());
    let p = if raw.is_absolute() && raw.is_dir() {
        raw
    } else {
        super::providers::resolve(&ctx.paths.home_dir, s)
    };
    if p.is_dir() {
        p.canonicalize().unwrap_or(p).to_string_lossy().into()
    } else {
        ctx.paths.home_dir.to_string_lossy().into()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_core::common::CorePaths;
    #[tokio::test]
    async fn session_mutations_emit_after_successful_update() {
        let dir = tempfile::tempdir().unwrap();
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = events.clone();
        let ctx = CoreContext::headless(CorePaths {
            data_dir: dir.path().into(),
            config_dir: dir.path().join("config"),
            resource_dir: dir.path().into(),
            temp_dir: dir.path().into(),
            home_dir: dir.path().into(),
        })
        .with_event_sink(std::sync::Arc::new(move |channel, payload| {
            sink.lock().unwrap().push((channel.to_owned(), payload));
        }));
        let terminal = TerminalCore::new(ctx.clone());
        terminal
            .handle(
                "terminal:native-create",
                &[json!({"sessionId":"one","shell":"/bin/sh","cwd":dir.path()})],
            )
            .await
            .unwrap();
        terminal
            .handle("core:terminal:exit", &[json!({"sessionId":"one","exitCode":0})])
            .await
            .unwrap();
        terminal.handle("terminal:delete", &[json!("one")]).await.unwrap();
        terminal.handle("terminal:delete", &[json!("missing")]).await.unwrap();
        ctx.config.patch(&json!({"terminalSessions":[{"sessionId":"expired","status":"exited","saveUntil":1}]})).unwrap();
        terminal.tick().await.unwrap();
        assert_eq!(
            *events.lock().unwrap(),
            vec![
                ("terminal:sessions-changed".to_owned(), json!({})),
                ("terminal:sessions-changed".to_owned(), json!({})),
                ("terminal:sessions-changed".to_owned(), json!({})),
                ("terminal:sessions-changed".to_owned(), json!({})),
            ]
        );
    }
    #[tokio::test]
    async fn restore_and_exit_retention() {
        let d = tempfile::tempdir().unwrap();
        let ctx = CoreContext::headless(CorePaths {
            data_dir: d.path().into(),
            config_dir: d.path().join("config"),
            resource_dir: d.path().into(),
            temp_dir: d.path().into(),
            home_dir: d.path().into(),
        });
        ctx.config.patch(&json!({"unknown":42})).unwrap();
        let t = TerminalCore::new(ctx.clone());
        let v=t.handle("terminal:native-create",&[json!({"sessionId":"a-1","shell":"/bin/zsh","cwd":d.path(),"initialCommand":" echo hello ","saveFor":"forever"})]).await.unwrap();
        assert_eq!(v["lastCommand"], "echo hello");
        assert!(v["saveUntil"].is_null());
        let v = t
            .handle(
                "core:terminal:exit",
                &[json!({"sessionId":"a-1","exitCode":2})],
            )
            .await
            .unwrap();
        assert_eq!(v["status"], "exited");
        assert_eq!(v["exitCode"], 2);
        let v = t
            .handle(
                "terminal:native-restore",
                &[json!({"sessionId":"a-1","shell":"/bin/bash","cwd":d.path()})],
            )
            .await
            .unwrap();
        assert_eq!(v["status"], "running");
        assert!(v["exitCode"].is_null());
        assert_eq!(ctx.config.read().unwrap()["unknown"], 42);
        assert!(t
            .handle("terminal:delete", &[json!("../../evil")])
            .await
            .is_err());
    }
    #[tokio::test]
    async fn retention_prunes_both_legacy_and_native_history_without_renderer() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = CoreContext::headless(CorePaths {
            data_dir: dir.path().join("data"),
            config_dir: dir.path().join("config"),
            resource_dir: dir.path().into(),
            temp_dir: dir.path().into(),
            home_dir: dir.path().into(),
        });
        for base in [&ctx.paths.config_dir, &ctx.paths.data_dir] {
            std::fs::create_dir_all(base.join("terminal-history")).unwrap();
            std::fs::write(base.join("terminal-history/expired.log"), b"history").unwrap();
            std::fs::write(base.join("terminal-history/retained.log"), b"keep").unwrap();
        }
        ctx.config.patch(&json!({"terminalSessions":[{"sessionId":"expired","status":"exited","saveUntil":1},{"sessionId":"retained","status":"exited","saveFor":"forever"}],"future":true})).unwrap();
        let terminal = TerminalCore::new(ctx.clone());
        terminal.tick().await.unwrap();
        let config = ctx.config.read().unwrap();
        assert_eq!(config["terminalSessions"].as_array().unwrap().len(), 1);
        assert_eq!(config["future"], true);
        for base in [&ctx.paths.config_dir, &ctx.paths.data_dir] {
            assert!(!base.join("terminal-history/expired.log").exists());
            assert!(base.join("terminal-history/retained.log").exists());
        }
    }
}
