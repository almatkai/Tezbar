pub mod ai;
pub mod common;
pub mod extension_worker;
pub mod extensions;
pub mod knowledge;
pub mod launcher;
pub mod platform;
pub mod system;
pub mod terminal_bridge;

use common::{CoreContext, CoreResult};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

/// Application services live in Rust. Only explicit extension operations can
/// reach the compatibility process; there is no general sidecar fallback.
pub struct NativeCore {
    pub ctx: CoreContext,
    pub ai: ai::AiCore,
    pub knowledge: knowledge::KnowledgeCore,
    pub launcher: launcher::LauncherCore,
    pub extensions: extensions::ExtensionsCore,
    pub platform: platform::PlatformCore,
    pub system: system::SystemCore,
    initialized: AtomicBool,
    ready: tokio::sync::Notify,
    closing: AtomicBool,
    lifecycle: tokio::sync::Mutex<()>,
    knowledge_start: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl NativeCore {
    pub fn new(ctx: CoreContext) -> Result<Arc<Self>, String> {
        let core = Arc::new(Self {
            ai: ai::AiCore::new(ctx.clone()),
            knowledge: knowledge::KnowledgeCore::new(ctx.clone()),
            launcher: launcher::LauncherCore::new(ctx.clone()),
            extensions: extensions::ExtensionsCore::new(ctx.clone()),
            platform: platform::PlatformCore::new(ctx.clone()),
            system: system::SystemCore::new(ctx.clone()),
            ctx: ctx.clone(),
            initialized: AtomicBool::new(false),
            ready: tokio::sync::Notify::new(),
            closing: AtomicBool::new(false),
            lifecycle: tokio::sync::Mutex::new(()),
            knowledge_start: Mutex::new(None),
        });
        let weak = Arc::downgrade(&core);
        ctx.set_router(Arc::new(move |channel, args| {
            let weak = weak.clone();
            Box::pin(async move {
                weak.upgrade()
                    .ok_or("Native core has shut down")?
                    .dispatch(&channel, &args)
                    .await
            })
        }))?;
        Ok(core)
    }

    pub async fn start(&self) -> Result<(), String> {
        let _lifecycle = self.lifecycle.lock().await;
        if self.closing.load(Ordering::Acquire) {
            return Err("Native core has shut down".into());
        }
        let shortcuts = self.platform.initialize_shortcuts();
        let (launcher, ()) = tokio::join!(self.launcher.start(), self.ai.start());
        self.extensions.start()?;
        // A legacy Knowledge database may need a large one-time FTS migration.
        // Resume it in the background without holding basic launcher IPC hostage.
        // Explicit Knowledge requests still await the service's serialized start.
        let mut pending = self.knowledge_start.lock().map_err(|e| e.to_string())?;
        if pending
            .as_ref()
            .map(|task| task.is_finished())
            .unwrap_or(true)
        {
            let knowledge = self.knowledge.clone();
            let ctx = self.ctx.clone();
            *pending = Some(tokio::spawn(async move {
                if let Err(error) = knowledge.start().await {
                    log::warn!("Knowledge startup: {error}");
                    ctx.emit(
                        "knowledge:status",
                        json!({"state":"failed","backend":"local","progress":0,"error":error}),
                    );
                }
            }));
        }
        drop(pending);
        self.initialized.store(true, Ordering::Release);
        self.ready.notify_waiters();
        launcher.and(shortcuts)
    }

    pub async fn request(&self, channel: &str, args: &[Value]) -> CoreResult {
        if channel.starts_with("core:") {
            return Err("Internal channel is not exposed to the renderer".into());
        }
        if !self.initialized.load(Ordering::Acquire) {
            let ready = self.ready.notified();
            tokio::pin!(ready);
            ready.as_mut().enable();
            if !self.initialized.load(Ordering::Acquire) {
                tokio::time::timeout(std::time::Duration::from_secs(30), ready)
                    .await
                    .map_err(|_| "Native core is still initializing")?;
            }
        }
        // Keep the large service-dispatch future off renderer/test caller stacks.
        Box::pin(self.dispatch(channel, args)).await
    }

    pub async fn execute_command_shortcut(&self, command: &str) -> CoreResult {
        let commands = self.ctx.request("core:extensions:commands", vec![]).await?;
        if let Some(extension) = commands
            .as_array()
            .into_iter()
            .flatten()
            .find(|v| v["id"].as_str() == Some(command))
        {
            self.ctx
                .request("core:platform:visibility", vec![json!(true)])
                .await?;
            let extension_id = extension["extensionId"].as_str().map(str::to_owned)
                .unwrap_or_else(|| format!("raycast.{}", extension["extName"].as_str().unwrap_or("")));
            self.ctx.emit("run-extension-command-from-hotkey",json!({"extensionId":extension_id,"commandName":extension["cmdName"]}));
            return Ok(json!({"ok":true}));
        }
        let id = command
            .strip_prefix("command:")
            .or_else(|| command.strip_prefix("native:"))
            .unwrap_or(command);
        let surface = match id {
            "open-indexing" => Some("indexing"),
            "open-extensions" => Some("extensions"),
            "open-snippets" => Some("snippets"),
            "open-notes" | "open-quick-notes" => Some("notes"),
            "open-clipboard-history" => Some("clipboard"),
            "open-emoji-picker" => Some("emoji-picker"),
            "show-system-monitor" => Some("system-monitor"),
            "start-onboarding" => Some("onboarding"),
            "open-settings" | "check-for-updates" => Some("settings"),
            "open-extensions-settings" => Some("settings-extensions"),
            _ => None,
        };
        if let Some(surface) = surface {
            return self
                .ctx
                .request("core:platform:open-surface", vec![json!(surface)])
                .await;
        }
        if id == "quit-tezbar" {
            return self.ctx.request("app:confirm-quit", vec![]).await;
        }
        if command.starts_with("native:") {
            return self
                .ctx
                .request(
                    "search:execute",
                    vec![json!({"type":"run-native-command","commandId":id})],
                )
                .await;
        }
        self.ctx
            .request("core:commands:execute", vec![json!(id), Value::Null])
            .await
    }

    async fn dispatch(&self, channel: &str, args: &[Value]) -> CoreResult {
        if self.closing.load(Ordering::Acquire) && channel != "core:terminal:exit" {
            return Err("Tezbar is shutting down".into());
        }
        if channel == "search:all" || channel == "search:candidates" {
            self.ctx.note_foreground();
        }
        if channel == "core:launcher:reindex-extensions" {
            return self
                .launcher
                .handle("core:launcher:refresh", &[])
                .await
                .unwrap_or_else(|| Err("Launcher refresh unavailable".into()));
        }
        if channel == "core:terminal:metadata:delete" {
            return self
                .launcher
                .handle("terminal:delete", args)
                .await
                .unwrap_or_else(|| Err("Terminal metadata unavailable".into()));
        }
        if let Some(result) = terminal_bridge::handle(&self.ctx, channel, args).await {
            return result;
        }
        if let Some(result) = self.ai.handle(channel, args).await {
            return result;
        }
        if let Some(result) = self.knowledge.handle(channel, args).await {
            return result;
        }
        if let Some(result) = self.launcher.handle(channel, args).await {
            return result;
        }
        if let Some(result) = self.extensions.handle(channel, args).await {
            return result;
        }
        if let Some(result) = self.platform.handle(channel, args).await {
            return result;
        }
        if let Some(result) = self.system.handle(channel, args).await {
            return result;
        }
        if channel == "health:ping" {
            return Ok(json!({"ok":true,"runtime":"rust"}));
        }
        Err(format!("Unknown native core channel: {channel}"))
    }

    pub async fn shutdown(&self) {
        let _lifecycle = self.lifecycle.lock().await;
        if self.closing.swap(true, Ordering::AcqRel) {
            return;
        }
        let startup = self
            .knowledge_start
            .lock()
            .ok()
            .and_then(|mut task| task.take());
        if let Some(startup) = startup {
            let _ = startup.await;
        }
        let (_, knowledge, launcher, ()) = tokio::join!(
            self.ai.shutdown(),
            self.knowledge.shutdown(),
            self.launcher.shutdown(),
            self.extensions.shutdown()
        );
        self.system.shutdown().await;
        if let Err(error) = knowledge {
            log::warn!("Knowledge shutdown: {error}");
        }
        if let Err(error) = launcher {
            log::warn!("Launcher shutdown: {error}");
        }
    }
}

#[cfg(test)]
mod tests;
