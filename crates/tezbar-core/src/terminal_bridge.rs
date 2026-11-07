//! Legacy terminal IPC uses the same managed native PTYs as direct Tauri commands.
use crate::common::{CoreContext, HandlerResult};
use crate::terminal::pty::*;
use serde_json::{json, Value};

pub const CHANNELS: &[&str] = &[
    "terminal:create",
    "terminal:attach",
    "terminal:detach",
    "terminal:write",
    "terminal:resize",
    "terminal:get-cwd",
    "terminal:kill",
    "terminal:delete",
    "core:terminal:pty:kill",
    "core:terminal:pty:cwd",
    // Compatibility notifications are snapshots only: native lifecycle owns writes.
    "terminal:native-create",
    "terminal:native-attach",
    "terminal:native-restore",
    "terminal:native-exit",
    "terminal:exit",
    "core:terminal:exit",
];

pub async fn handle(ctx: &CoreContext, channel: &str, args: &[Value]) -> HandlerResult {
    if matches!(channel, "core:terminal:stream-attach" | "core:terminal:stream-detach" | "core:terminal:replay-read" | "core:terminal:replay-release" | "core:terminal:input") {
        let ctx=ctx.clone(); let channel=channel.to_owned(); let raw=args.first().cloned().unwrap_or(Value::Null);
        if channel == "core:terminal:stream-attach" {native_terminal_replay_reaper(&ctx.terminal);}
        return Some(tokio::task::spawn_blocking(move || {
            if !ctx.runs_desktop_services() { return Err("Native terminal requires the desktop application".into()); }
            let state=&*ctx.terminal;
            let text=|key: &str| raw[key].as_str().ok_or_else(|| format!("Terminal {key} is required"));
            match channel.as_str() {
                "core:terminal:stream-attach" => native_terminal_stream_attach(ctx.clone(),state,serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?),
                "core:terminal:replay-read" => native_terminal_stream_read(state,text("replay")?,raw["position"].as_u64().ok_or("Replay position required")?,raw["limit"].as_u64().ok_or("Replay limit required")?.try_into().map_err(|_| "Replay limit too large")?),
                "core:terminal:replay-release" => Ok(json!(native_terminal_stream_release(state,text("replay")?))),
                "core:terminal:stream-detach" => Ok(json!(native_terminal_stream_detach(ctx.clone(),state,text("sessionId")?.into(),text("generation")?)?)),
                "core:terminal:input" => Ok(json!(native_terminal_binary_input(state,text("sessionId")?,text("generation")?,text("bytes")?)?)),
                _ => unreachable!(),
            }
        }).await.map_err(|e| e.to_string()).and_then(|r|r));
    }
    let channel = match channel {
        "core:terminal:create" => "terminal:create",
        "core:terminal:attach" => "terminal:attach",
        "core:terminal:detach" => "terminal:detach",
        "core:terminal:write" => "terminal:write",
        "core:terminal:resize" => "terminal:resize",
        "core:terminal:cwd" => "terminal:get-cwd",
        "core:terminal:kill" => "terminal:kill",
        "core:terminal:delete" => "terminal:delete",
        "core:terminal:delete-history" => "terminal:delete-history",
        "core:terminal:prune-history" => "terminal:prune-history",
        other => other,
    };
    if !CHANNELS.contains(&channel)
        && !["terminal:delete-history", "terminal:prune-history"].contains(&channel)
    {
        return None;
    }
    let ctx = ctx.clone();
    let channel = channel.to_owned();
    let raw = args.first().cloned().unwrap_or(Value::Null);
    Some(tokio::task::spawn_blocking(move || {
        let id = raw["sessionId"].as_str().or_else(|| raw.as_str()).unwrap_or("").to_owned();
        if channel.starts_with("terminal:native-") || channel == "terminal:exit" || channel == "core:terminal:exit" {
            let config = ctx.config.read()?;
            return Ok(config["terminalSessions"].as_array().into_iter().flatten().find(|s| s["sessionId"] == id).cloned().unwrap_or(Value::Null));
        }
        if !ctx.runs_desktop_services() { return Err("Native terminal requires the desktop application".into()); }
        let app = ctx.clone();
        let state = &*ctx.terminal;
        let dimension = |key: &str, max: u64| -> Result<u16, String> {
            raw[key].as_f64().filter(|n| n.is_finite()).map(|n| n.floor().clamp(2.0,max as f64) as u16).ok_or_else(|| format!("Terminal {key} is required"))
        };
        match channel.as_str() {
            "terminal:create" => {
                let mut request = raw.clone();
                request["cols"] = json!(dimension("cols",500)?);
                request["rows"] = json!(dimension("rows",300)?);
                serde_json::to_value(native_terminal_create_impl(app.clone(), state, serde_json::from_value(request).map_err(|e| e.to_string())?)?).map_err(|e| e.to_string())
            }
            "terminal:attach" => {
                let request = json!({"sessionId":id,"cols":dimension("cols",500)?,"rows":dimension("rows",300)?});
                serde_json::to_value(native_terminal_attach_impl(app.clone(), state, serde_json::from_value(request).map_err(|e| e.to_string())?)?).map_err(|e| e.to_string())
            }
            "terminal:detach" => Ok(json!(native_terminal_detach_impl(app.clone(),state,id)?)),
            "terminal:write" => Ok(json!(native_terminal_write_impl(state,id,raw["data"].as_str().ok_or("Terminal data is required")?.into())?)),
            "terminal:resize" => Ok(json!(native_terminal_resize_impl(state,id,dimension("cols",500)?,dimension("rows",300)?)?)),
            "terminal:get-cwd" | "core:terminal:pty:cwd" => Ok(json!(native_terminal_cwd_impl(app.clone(),state,id)?)),
            "terminal:kill" | "core:terminal:pty:kill" => Ok(json!(native_terminal_kill_impl(state,id)?)),
            "terminal:delete-history" => Ok(json!(native_terminal_delete_history_impl(app.clone(), state, id)?)),
            "terminal:prune-history" => Ok(json!(native_terminal_prune_history_impl(app.clone(), state, serde_json::from_value(raw["sessionIds"].clone()).map_err(|e|e.to_string())?)?)),
            "terminal:delete" => {
                native_terminal_kill_impl(state,id.clone())?;
                if !state.wait_stopped(&id) { return Err("Terminal is still stopping".into()); }
                crate::launcher::terminal::TerminalCore::metadata(&ctx, "terminal:delete", &[json!({"sessionId":id})])
            }
            _ => unreachable!(),
        }
    }).await.map_err(|e| e.to_string()).and_then(|result| result))
}

#[cfg(test)]
mod terminal_bridge_tests {
    use super::*;
    use crate::common::CorePaths;

    #[tokio::test]
    async fn terminal_late_renderer_notifications_cannot_overwrite_new_generation() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = CoreContext::headless(CorePaths {
            data_dir: dir.path().into(),
            config_dir: dir.path().join("profile"),
            resource_dir: dir.path().into(),
            temp_dir: dir.path().into(),
            home_dir: dir.path().into(),
        });
        let summary = json!({"sessionId":"same-id","name":"User's name","status":"running","shell":"new-shell","createdAt":123,"saveFor":"forever"});
        ctx.config
            .patch(&json!({"terminalSessions":[summary.clone()],"unrelated":true}))
            .unwrap();
        let before = std::fs::read(ctx.config.path()).unwrap();
        for channel in [
            "terminal:native-create",
            "terminal:native-attach",
            "terminal:native-restore",
            "terminal:native-exit",
            "terminal:exit",
            "core:terminal:exit",
        ] {
            let actual = handle(
                &ctx,
                channel,
                &[json!({"sessionId":"same-id","exitCode":7,"shell":"old-shell"})],
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(actual, summary, "{channel}");
        }
        assert_eq!(before, std::fs::read(ctx.config.path()).unwrap());
        assert!(handle(&ctx, "not:a:terminal:route", &[]).await.is_none());
    }
}
