mod catalog;
mod native;
mod perplexity;
mod registry;

use super::common::{CoreContext, HandlerResult};
use super::extension_worker::{ExtensionWorker, CHANNELS as WORKER_CHANNELS};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

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
        let uninstall_native = uninstall
            && matches!(
                args.first().and_then(Value::as_str).map(str::trim),
                Some("raycast.perplexity" | "perplexity")
            );
        let result = tokio::task::spawn_blocking(move || {
            let _guard = this.files.lock().map_err(|e| e.to_string())?;
            registry::handle(&this.ctx, &channel, &args)
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|v| v);
        if uninstall && result.is_ok() {
            if uninstall_native {
                self.native.clear();
            }
            let _ = self
                .ctx
                .request("core:launcher:reindex-extensions", vec![])
                .await;
        }
        Some(result)
    }

    pub async fn shutdown(&self) {
        self.native.clear();
        self.worker.shutdown().await;
    }
    pub async fn restart_worker(&self) {
        self.native.clear();
        self.worker.shutdown().await;
    }

    #[cfg(test)]
    pub async fn worker_running(&self) -> bool {
        self.worker.is_running().await
    }
}
