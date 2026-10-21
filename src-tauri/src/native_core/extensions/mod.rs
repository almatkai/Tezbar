mod catalog;
mod native;
mod perplexity;
mod registry;
mod v2_amphetamine;
mod v2_audio_device;
mod v2_catalog;
mod v2_cleanshotx;
mod v2_color_picker;
mod v2_gif_search;
mod v2_google_calendar;
mod v2_google_search;
mod v2_perplexity;
mod v2_port_manager;
mod v2_runtime;
mod v2_sdk;
mod v2_speedtest;
mod v2_timers;

use super::common::{CoreContext, HandlerResult};
use super::extension_worker::{ExtensionWorker, CHANNELS as WORKER_CHANNELS};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

pub(crate) fn v2_cli_list(ctx: &CoreContext) -> super::common::CoreResult {
    v2_catalog::cli_list(ctx)
}

pub(crate) fn v2_cli_install(ctx: &CoreContext, id: &str) -> super::common::CoreResult {
    v2_catalog::cli_install(ctx, id)
}

pub const CHANNELS: &[&str] = &[
    "get-extensions",
    "extensions:listInstalled",
    "extensions:searchStore",
    "extensions:uninstall",
    "extensions:integrity",
    "extensions:install-error",
    "extension:list",
    "extension:search-store",
    "extension:uninstall",
    "preferences:get",
    "preferences:setup",
    "preferences:set",
    "get-installed-extensions-settings-schema",
    "core:extensions:commands",
];

#[derive(Clone)]
pub struct ExtensionsCore {
    ctx: CoreContext,
    worker: ExtensionWorker,
    // Serialize metadata mutations without blocking the async runtime.
    files: Arc<Mutex<()>>,
    catalog: catalog::Catalog,
    native: native::NativeSessions,
    #[cfg(test)]
    worker_override: Option<super::common::CoreRouter>,
}

impl ExtensionsCore {
    pub fn start(&self) -> Result<(), String> {
        // Restore durable timers without starting a polling worker for profiles
        // that have never used the native extension.
        if self.ctx.paths.data_dir.join("native-extensions-v2/timers/storage.json").is_file() {
            v2_timers::start(&self.ctx)?;
        }
        Ok(())
    }

    pub fn new(ctx: CoreContext) -> Self {
        Self {
            worker: ExtensionWorker::new(ctx.clone()),
            catalog: catalog::Catalog::new(ctx.clone()),
            ctx,
            files: Arc::new(Mutex::new(())),
            native: native::NativeSessions::default(),
            #[cfg(test)]
            worker_override: None,
        }
    }

    pub async fn handle(&self, channel: &str, args: &[Value]) -> HandlerResult {
        if let Some(result) = self
            .native
            .handle(&self.ctx, &self.files, channel, args)
            .await
        {
            return Some(result);
        }
        if matches!(channel, "extension:install" | "extensions:install") {
            if let Some(id) = args.first().and_then(Value::as_str) {
                let (ctx, files, id) = (self.ctx.clone(), self.files.clone(), id.to_owned());
                let native_install = tokio::task::spawn_blocking(move || {
                    let _guard = files.lock().map_err(|e| e.to_string())?;
                    Ok::<_, String>(v2_catalog::install(&ctx, &id))
                })
                .await;
                match native_install {
                    Ok(Ok(Some(result))) => {
                        if result.is_ok() {
                            self.ctx.emit("extensions-changed", json!({}));
                            let _ = self
                                .ctx
                                .request("core:launcher:reindex-extensions", vec![])
                                .await;
                        }
                        return Some(result);
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => return Some(Err(error)),
                    Err(error) => return Some(Err(error.to_string())),
                }
            }
        }
        if WORKER_CHANNELS.contains(&channel) {
            #[cfg(test)]
            if let Some(router) = &self.worker_override {
                return Some(router(channel.to_string(), args.to_vec()).await);
            }
            let result = self.worker.request(channel, args.to_vec()).await;
            if result.is_ok() && channel.ends_with("install") {
                self.ctx.emit("extensions-changed", json!({}));
                // The launcher refreshes its native extension documents on this request.
                let _ = self
                    .ctx
                    .request("core:launcher:reindex-extensions", vec![])
                    .await;
            }
            return Some(result);
        }
        if !CHANNELS.contains(&channel) {
            return None;
        }
        if channel == "extension:search-store" || channel == "extensions:searchStore" {
            return Some(self.catalog.search(channel, args).await);
        }
        let this = self.clone();
        let channel = channel.to_string();
        let args = args.to_vec();
        let uninstall = channel == "extension:uninstall" || channel == "extensions:uninstall";
        let uninstall_slug = if uninstall {
            args.first()
                .and_then(Value::as_str)
                .and_then(|id| v2_runtime::slug(id.trim()))
                .map(str::to_owned)
        } else {
            None
        };
        let result = tokio::task::spawn_blocking(move || {
            let _guard = this.files.lock().map_err(|e| e.to_string())?;
            registry::handle(&this.ctx, &channel, &args)
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|v| v);
        if uninstall && result.is_ok() {
            if let Some(slug) = uninstall_slug {
                self.native.clear_slug(&slug).await;
            }
            let _ = self
                .ctx
                .request("core:launcher:reindex-extensions", vec![])
                .await;
        }
        Some(result)
    }

    pub async fn shutdown(&self) {
        self.native.clear().await;
        v2_timers::shutdown(&self.ctx).await;
        tokio::task::spawn_blocking(v2_amphetamine::shutdown).await.ok();
        self.worker.shutdown().await;
    }
    pub async fn restart_worker(&self) {
        self.native.clear().await;
        self.worker.shutdown().await;
    }

    #[cfg(test)]
    pub async fn worker_running(&self) -> bool {
        self.worker.is_running().await
    }
}
