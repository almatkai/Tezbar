pub mod ai;
pub mod assets;
pub mod common;
pub mod extension_worker;
pub mod extensions;
mod extension_feedback;
mod fts_rank;
#[cfg(feature = "native-ui-instant")]
pub mod instant;
pub mod knowledge;
pub mod launcher;
pub mod platform;
pub mod system;
pub mod terminal_bridge;
pub mod vault;

#[cfg(test)]
mod performance;
#[cfg(test)]
mod performance_alloc;
#[cfg(test)]
mod startup_demand_tests;

use common::{CoreContext, CoreResult};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

/// Optional background startup is explicit so an isolated native preview can
/// become searchable without scanning Knowledge, restoring extension processes
/// or registering the production shortcut. Explicit service requests remain lazy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartupOptions {
    pub initialize_shortcuts: bool,
    pub restore_background_extensions: bool,
    pub background_knowledge: bool,
}
impl StartupOptions {
    /// Combine with a Host whose desktop_services() is false. That also keeps
    /// real clipboard observation, app discovery and filesystem watchers off.
    pub const ISOLATED_PREVIEW: Self = Self {
        initialize_shortcuts: false,
        restore_background_extensions: false,
        background_knowledge: false,
    };
}
impl Default for StartupOptions {
    fn default() -> Self {
        Self {
            initialize_shortcuts: true,
            restore_background_extensions: true,
            background_knowledge: true,
        }
    }
}

/// Application services live in Rust. Only explicit extension operations can
/// reach the compatibility process; there is no general sidecar fallback.
pub struct NativeCore {
    pub ctx: CoreContext,
    pub assets: assets::AssetService,
    pub ai: ai::AiCore,
    pub knowledge: knowledge::KnowledgeCore,
    pub launcher: launcher::LauncherCore,
    pub extensions: extensions::ExtensionsCore,
    pub platform: platform::PlatformCore,
    pub system: system::SystemCore,
    pub vault: vault::VaultService,
    extension_feedback: extension_feedback::Feedback,
    initialized: AtomicBool,
    ready: tokio::sync::Notify,
    closing: AtomicBool,
    lifecycle: tokio::sync::Mutex<()>,
    knowledge_start: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl NativeCore {
    pub fn new(ctx: CoreContext) -> Result<Arc<Self>, String> {
        Self::configured(ctx, None)
    }
    /// Construct the same routed core with a host-selected isolated worker bundle.
    pub fn new_with_extension_worker(
        ctx: CoreContext,
        launch: extension_worker::WorkerLaunch,
    ) -> Result<Arc<Self>, String> {
        Self::configured(ctx, Some(launch))
    }
    fn configured(
        ctx: CoreContext,
        launch: Option<extension_worker::WorkerLaunch>,
    ) -> Result<Arc<Self>, String> {
        let extensions = match launch {
            Some(launch) => extensions::ExtensionsCore::with_worker_launch(ctx.clone(), launch),
            None => extensions::ExtensionsCore::new(ctx.clone()),
        };
        let core = Arc::new(Self {
            assets: assets::AssetService::default(),
            ai: ai::AiCore::new(ctx.clone()),
            knowledge: knowledge::KnowledgeCore::new(ctx.clone()),
            launcher: launcher::LauncherCore::new(ctx.clone()),
            extensions,
            platform: platform::PlatformCore::new(ctx.clone()),
            system: system::SystemCore::new(ctx.clone()),
            vault: vault::VaultService::new(ctx.paths.data_dir.clone(), Some(ctx.host.clone())),
            ctx: ctx.clone(),
            extension_feedback: Default::default(),
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
        self.start_with_options(StartupOptions::default()).await
    }

    pub async fn start_with_options(&self, options: StartupOptions) -> Result<(), String> {
        let _lifecycle = self.lifecycle.lock().await;
        if self.closing.load(Ordering::Acquire) {
            return Err("Native core has shut down".into());
        }
        let shortcuts = if options.initialize_shortcuts {
            self.platform.initialize_shortcuts()
        } else {
            Ok(())
        };
        let (launcher, ()) = tokio::join!(self.launcher.start(), self.ai.start());
        if options.restore_background_extensions {
            self.extensions.start()?;
        }
        // A legacy Knowledge database may need a large one-time FTS migration.
        // Resume it in the background without holding basic launcher IPC hostage.
        // Explicit Knowledge requests still await the service's serialized start.
        let mut pending = self.knowledge_start.lock().map_err(|e| e.to_string())?;
        if options.background_knowledge
            && pending
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
        let settings_id = command.strip_prefix("extcmd:").and_then(|key| key.split_once(':'));
        if let Some(extension) = commands
            .as_array()
            .into_iter()
            .flatten()
            .find(|row| {
                row["id"].as_str() == Some(command) || settings_id.is_some_and(|(extension, name)| {
                    if extension.starts_with("com.tezbar.native.") {
                        return row["extensionId"] == extension && row["cmdName"] == name;
                    }
                    let js_slug = row["extensionId"].as_str().and_then(|id| id.strip_prefix("raycast."));
                    let legacy_slug = extension.strip_prefix("com.tezbar.")
                        .filter(|_| !extension.starts_with("com.tezbar.native."));
                    row["cmdName"] == name && (row["extensionId"] == extension || row["extName"] == extension
                        || legacy_slug.is_some() && legacy_slug == js_slug
                        || row["extensionId"].as_str().is_some_and(|id| {
                            crate::extensions::command_hotkey_aliases(&format!("extcmd:{id}:{name}"))
                                .iter().any(|alias| alias == command)
                        }))
                })
            })
        {
            self.ctx
                .request("core:platform:visibility", vec![json!(true)])
                .await?;
            let extension_id = extension["extensionId"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    format!("raycast.{}", extension["extName"].as_str().unwrap_or(""))
                });
            self.ctx.emit(
                "run-extension-command-from-hotkey",
                json!({"extensionId":extension_id,"commandName":extension["cmdName"]}),
            );
            return Ok(json!({"ok":true}));
        }
        if settings_id.is_some() { return Err(format!("Extension command unavailable: {command}")); }
        let id = command
            .strip_prefix("command:")
            .or_else(|| command.strip_prefix("native:"))
            .unwrap_or(command);
        if let Some(surface) = native_command_surface(id) {
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

    /// Releases idle in-memory caches, SQLite buffers, and requests OS memory relief.
    pub async fn trim_idle_memory(&self) {
        let _ = self.knowledge.shrink_memory().await;
        trim_os_process_memory();
    }

    async fn dispatch(&self, channel: &str, args: &[Value]) -> CoreResult {
        if self.closing.load(Ordering::Acquire) && channel != "core:terminal:exit" {
            return Err("Tezbar is shutting down".into());
        }
        if channel == "core:extension-status-refresh" {
            return self
                .extensions
                .refresh_sdk_status(args.first().cloned().unwrap_or(Value::Null))
                .await;
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
            if result.is_ok()
                && channel == "toggle-command-enabled"
                && args.get(1) == Some(&Value::Bool(false))
            {
                if let Some(id) = args.first().and_then(Value::as_str) {
                    self.extensions.command_disabled(id).await;
                }
            }
            return result;
        }
        if let Some(mut result) = self.extensions.handle(channel, args).await {
            if matches!(channel, "extension:run-command" | "extension:invoke-action") {
                extension_feedback::strip_native_feedback(&mut result);
                if let Err(error) = self.extension_feedback.present(
                    channel, self.ctx.host.launcher_hidden(), &mut result,
                    |message| self.ctx.host.notify(host::NotificationSpec::ExtensionFeedback(message)),
                ).await {
                    log::debug!("Extension feedback unavailable: {error}");
                }
            }
            return result;
        }
        if let Some(result) = self.platform.handle(channel, args).await {
            return result;
        }
        if let Some(result) = self.system.handle(channel, args).await {
            return result;
        }
        if let Some(result) = self.vault.handle(channel, args).await {
            // Keep vault names in global search in step with the vault.
            if result.is_ok()
                && matches!(
                    channel,
                    "vault:unlock" | "vault:save" | "vault:delete" | "vault:reset"
                )
            {
                if let Err(error) = self.launcher.local_refresh().await {
                    log::warn!("vault search refresh failed: {error}");
                }
            }
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
        let terminal = self.ctx.terminal.clone();
        let _ = tokio::task::spawn_blocking(move || terminal::pty::shutdown_all(&terminal)).await;
        self.vault.lock();
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

/// Trims native process allocations and requests the host OS to reclaim unused memory pages.
pub fn trim_os_process_memory() {
    #[cfg(target_os = "macos")]
    unsafe {
        extern "C" {
            fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
        }
        let _ = malloc_zone_pressure_relief(std::ptr::null_mut(), 0);
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
        let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    #[cfg(target_env = "gnu")]
    unsafe {
        extern "C" {
            fn malloc_trim(pad: usize) -> i32;
        }
        let _ = malloc_trim(0);
    }
}

#[cfg(test)]
mod tests;

pub mod bun;
pub mod child_supervisor;
pub mod host;
/// Native command ids served by a built-in Rust/GPUI surface on every platform.
pub fn native_command_surface(id: &str) -> Option<&'static str> {
    match id {
        "open-indexing" => Some("indexing"),
        "open-extensions" => Some("extensions"),
        "open-snippets" => Some("snippets"),
        "open-notes" | "open-quick-notes" => Some("notes"),
        "open-vault" => Some("vault"),
        "open-clipboard-history" => Some("clipboard"),
        "open-emoji-picker" => Some("emoji-picker"),
        "show-system-monitor"
        | "show-cpu-info"
        | "show-macos-version"
        | "show-memory-info"
        | "show-disk-usage"
        | "show-battery-status" => Some("system-monitor"),
        "start-onboarding" => Some("onboarding"),
        "open-settings" | "check-for-updates" => Some("settings"),
        "open-extensions-settings" => Some("settings-extensions"),
        _ => None,
    }
}
#[cfg(feature = "native-launcher-facade")]
pub mod launcher_facade;
pub mod native_v2_cli;
pub mod resource_directory;
pub mod terminal;
#[cfg(windows)]
pub mod windows_job;
#[cfg(windows)]
pub(crate) mod windows_shell;

#[cfg(test)]
mod desktop_effect_tests;
