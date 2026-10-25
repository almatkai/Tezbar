mod acquisition;
mod catalog;
mod launcher;
mod native;
mod process;
mod process_host;
mod process_install;
mod process_notifications;
mod registry;
mod v2_catalog;
mod v2_runtime;
// Domain implementations/assets live in standalone extension repositories.
// v2_runtime retains generic legacy identity compatibility only.

use super::common::{CoreContext, HandlerResult};
use super::extension_worker::{ExtensionWorker, CHANNELS as WORKER_CHANNELS};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

pub(crate) fn v2_cli_list(ctx: &CoreContext) -> super::common::CoreResult {
    v2_catalog::cli_list(ctx)
}

pub(crate) async fn v2_cli_install(ctx: &CoreContext, id: &str) -> super::common::CoreResult {
    v2_catalog::cli_install(ctx, id).await
}
pub(crate) async fn v3_cli_import(
    ctx: &CoreContext,
    manifest: std::path::PathBuf,
    zip: std::path::PathBuf,
) -> super::common::CoreResult {
    let manifest = super::common::read_json(&manifest)?;
    process_install::install(ctx, manifest, Some(zip)).await
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
    installs: Arc<tokio::sync::Mutex<()>>,
    #[cfg(test)]
    worker_override: Option<super::common::CoreRouter>,
}

impl ExtensionsCore {
    pub fn start(&self) -> Result<(), String> {
        self.native.start(&self.ctx)
    }

    pub fn new(ctx: CoreContext) -> Self {
        Self {
            worker: ExtensionWorker::new(ctx.clone()),
            catalog: catalog::Catalog::new(ctx.clone()),
            ctx,
            files: Arc::new(Mutex::new(())),
            native: native::NativeSessions::default(),
            installs: Arc::new(tokio::sync::Mutex::new(())),
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
        if matches!(
            channel,
            "extension:install" | "extensions:install" | "extensions:reinstall"
        ) {
            if let Some(id) = args.first().and_then(Value::as_str) {
                let _install = self.installs.lock().await;
                // Reject unsupported acquisition before disturbing a running
                // local package. The installer independently enforces this.
                if let Err(error) = acquisition::require_remote(acquisition::for_id(id)) {
                    return Some(Err(error));
                }
                if let Some(slug) = id.strip_prefix("com.tezbar.native.") {
                    self.native.clear_slug(slug).await;
                }
                match v2_catalog::install(&self.ctx, id).await {
                    Some(result) => {
                        // Resume only this package's durable ownership after an
                        // update (or failed update retaining its old activation).
                        if let Err(error)=self.native.restore_id(&self.ctx,id){log::warn!("Background restoration after install failed: {error}");}
                        if result.is_ok() {
                            self.ctx.emit("extensions-changed", json!({}));
                            let _ = self
                                .ctx
                                .request("core:launcher:reindex-extensions", vec![])
                                .await;
                        }
                        return Some(result);
                    }
                    None if id.starts_with("com.tezbar.native.") => return Some(Err("Unknown external Rust package; import a trusted local release manifest and ZIP".into())),
                    None => {}
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
                .and_then(|id| id.trim().strip_prefix("com.tezbar.native."))
                .map(str::to_owned)
        } else {
            None
        };
        let _install_guard = if uninstall {
            Some(self.installs.lock().await)
        } else {
            None
        };
        if let Some(slug) = &uninstall_slug {
            self.native.clear_slug(slug).await;
        }
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
