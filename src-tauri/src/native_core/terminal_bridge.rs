//! Legacy terminal IPC uses the same managed native PTYs as direct Tauri commands.
use crate::native_core::common::{CoreContext, HandlerResult};
use crate::native_terminal::*;
use serde_json::{json, Value};
use tauri::Manager;

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
    if !CHANNELS.contains(&channel) {
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
        let app = ctx.app.clone().ok_or("Native terminal requires the desktop application")?;
        let state = app.state::<NativeTerminalState>();
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
            "terminal:delete" => {
                native_terminal_kill_impl(app.state::<NativeTerminalState>(),id.clone())?;
                if !state.wait_stopped(&id) { return Err("Terminal is still stopping".into()); }
                tauri::async_runtime::block_on(ctx.request("core:terminal:metadata:delete", vec![json!({"sessionId":id})]))
            }
            _ => unreachable!(),
        }
    }).await.map_err(|e| e.to_string()).and_then(|result| result))
}

#[cfg(test)]
mod terminal_bridge_tests {
    use super::*;
    use crate::native_core::common::CorePaths;

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
