// src-tauri/src/updater.rs
//
// GitHub Releases–based update tracking using tauri-plugin-updater.

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// GitHub repository that publishes Tezbar releases ("owner/repo").
const GITHUB_REPO: &str = "almatkai/Tezbar";

/// Mirrors the renderer's `AppUpdateStatus` union (`src/shared/updater.ts`).
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AppUpdateStatus {
    Idle,
    Checking,
    UpToDate {
        version: String,
    },
    Available {
        version: String,
        notes: String,
        #[serde(rename = "releaseUrl")]
        release_url: String,
    },
    Downloading {
        version: String,
        downloaded: u64,
        total: Option<u64>,
    },
    Ready {
        version: String,
    },
    Error {
        message: String,
    },
}

pub struct UpdaterState {
    status: Mutex<AppUpdateStatus>,
    pending: Mutex<Option<Update>>,
}

impl Default for UpdaterState {
    fn default() -> Self {
        Self {
            status: Mutex::new(AppUpdateStatus::Idle),
            pending: Mutex::new(None),
        }
    }
}

fn set_status(app: &AppHandle, status: AppUpdateStatus) {
    if let Some(state) = app.try_state::<UpdaterState>() {
        *state.status.lock().unwrap() = status.clone();
    }
    if let Err(error) = app.emit("update-status", status) {
        log::debug!("failed to emit update-status: {error}");
    }
}

fn current_status(app: &AppHandle) -> AppUpdateStatus {
    app.try_state::<UpdaterState>()
        .map(|state| state.status.lock().unwrap().clone())
        .unwrap_or(AppUpdateStatus::Idle)
}

fn github_release_url(tag: &str, draft_fallback: &str) -> String {
    if tag.is_empty() {
        draft_fallback.to_string()
    } else {
        format!("https://github.com/{GITHUB_REPO}/releases/tag/{tag}")
    }
}

fn is_platform_not_found_error(err: &tauri_plugin_updater::Error) -> bool {
    matches!(
        err,
        tauri_plugin_updater::Error::TargetNotFound(_)
            | tauri_plugin_updater::Error::TargetsNotFound(_)
    )
}

#[tauri::command]
pub async fn get_update_status(app: AppHandle) -> Result<AppUpdateStatus, String> {
    Ok(current_status(&app))
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<AppUpdateStatus, String> {
    set_status(&app, AppUpdateStatus::Checking);

    let os_info = if cfg!(target_os = "windows") {
        "Windows; Windows NT"
    } else if cfg!(target_os = "macos") {
        "Macintosh; Intel Mac OS X"
    } else {
        "Linux; X11"
    };
    let current_version = app.package_info().version.to_string();
    let user_agent = format!("Tezbar-App/{current_version} ({os_info})");

    let updater = app
        .updater_builder()
        .header("User-Agent", user_agent)
        .map_err(|e| e.to_string())?
        .version_comparator(|current, release| {
            // Offer updates that are strictly newer than the running build.
            release.version > current
        })
        .build()
        .map_err(|e| e.to_string())?;

    let result = updater.check().await;

    match result {
        Ok(Some(update)) => {
            let version = update.version.clone();
            let notes = update.body.clone().unwrap_or_default();
            // tag is embedded in the version string; release page URL is derived
            // from the configured endpoints' repository.
            let release_url = github_release_url(&format!("v{version}"), "");
            if let Some(state) = app.try_state::<UpdaterState>() {
                *state.pending.lock().unwrap() = Some(update);
            }
            let status = AppUpdateStatus::Available {
                version,
                notes,
                release_url,
            };
            set_status(&app, status.clone());
            Ok(status)
        }
        Ok(None) => {
            if let Some(state) = app.try_state::<UpdaterState>() {
                *state.pending.lock().unwrap() = None;
            }
            let version = app.package_info().version.to_string();
            let status = AppUpdateStatus::UpToDate { version };
            set_status(&app, status.clone());
            Ok(status)
        }
        Err(error) => {
            if is_platform_not_found_error(&error) {
                // The release manifest is valid, but doesn't have an updater artifact
                // for this target platform (e.g. a Windows-only release checked on macOS).
                // From the viewpoint of the running OS, this build is up to date.
                if let Some(state) = app.try_state::<UpdaterState>() {
                    *state.pending.lock().unwrap() = None;
                }
                let version = app.package_info().version.to_string();
                let status = AppUpdateStatus::UpToDate { version };
                set_status(&app, status.clone());
                return Ok(status);
            }

            let status = AppUpdateStatus::Error {
                message: error.to_string(),
            };
            set_status(&app, status.clone());
            Ok(status)
        }
    }
}

#[tauri::command]
pub async fn download_and_install_update(app: AppHandle) -> Result<AppUpdateStatus, String> {
    if let AppUpdateStatus::Downloading { .. } | AppUpdateStatus::Ready { .. } = current_status(&app) {
        return Ok(current_status(&app));
    }

    let update = {
        let state = app
            .try_state::<UpdaterState>()
            .ok_or_else(|| "updater state unavailable".to_string())?;
        let pending = state.pending.lock().unwrap().take();
        pending
    };

    let Some(update) = update else {
        return Err("No pending update. Run check_for_updates first.".to_string());
    };

    let version = update.version.clone();
    set_status(
        &app,
        AppUpdateStatus::Downloading {
            version: version.clone(),
            downloaded: 0,
            total: None,
        },
    );

    let app_for_events = app.clone();
    let downloaded_bytes = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let last_emitted = std::sync::Arc::new(std::sync::Mutex::new(std::time::Instant::now()));

    let dl_bytes_for_chunk = downloaded_bytes.clone();
    let last_emitted_for_chunk = last_emitted.clone();
    let v_for_chunk = version.clone();
    let app_for_finish = app.clone();
    let v_for_finish = version.clone();
    let dl_bytes_for_finish = downloaded_bytes.clone();

    let download_result = update
        .download_and_install(
            move |chunk_len, total| {
                let current = dl_bytes_for_chunk
                    .fetch_add(chunk_len as u64, std::sync::atomic::Ordering::Relaxed)
                    + chunk_len as u64;
                let now = std::time::Instant::now();
                let mut last = last_emitted_for_chunk.lock().unwrap();
                let is_done = total.map(|t| current >= t as u64).unwrap_or(false);
                if now.duration_since(*last).as_millis() >= 150 || is_done {
                    *last = now;
                    set_status(
                        &app_for_events,
                        AppUpdateStatus::Downloading {
                            version: v_for_chunk.clone(),
                            downloaded: current,
                            total: total.map(|t| t as u64),
                        },
                    );
                }
            },
            move || {
                let total_dl = dl_bytes_for_finish.load(std::sync::atomic::Ordering::Relaxed);
                set_status(
                    &app_for_finish,
                    AppUpdateStatus::Downloading {
                        version: v_for_finish,
                        downloaded: total_dl,
                        total: Some(total_dl),
                    },
                );
            },
        )
        .await;

    match download_result {
        Ok(()) => {
            let version = update.version.clone();
            let status = AppUpdateStatus::Ready { version };
            set_status(&app, status.clone());
            Ok(status)
        }
        Err(error) => {
            let status = AppUpdateStatus::Error {
                message: error.to_string(),
            };
            set_status(&app, status.clone());
            Ok(status)
        }
    }
}

#[tauri::command]
pub fn restart_app(app: AppHandle) {
    app.restart();
}

#[tauri::command]
pub fn open_release_page(url: String) -> Result<(), String> {
    open::that_detached(&url).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::is_platform_not_found_error;
    use tauri_plugin_updater::{Error, RemoteRelease};

    #[test]
    fn skips_releases_missing_macos_installer_and_architecture_fallbacks() {
        for arch in ["aarch64", "x86_64"] {
            let targets = vec![format!("darwin-{arch}-app"), format!("darwin-{arch}")];
            assert!(is_platform_not_found_error(&Error::TargetsNotFound(targets)));
        }
    }

    #[test]
    fn windows_only_manifest_has_no_macos_update() {
        let release: RemoteRelease = serde_json::from_value(serde_json::json!({
            "version": "0.2.6",
            "platforms": {
                "windows-x86_64-nsis": {
                    "url": "https://example.com/Tezbar_0.2.6_x64-setup.exe",
                    "signature": "windows-signature"
                }
            }
        }))
        .unwrap();

        assert!(release.download_url("windows-x86_64-nsis").is_ok());
        for target in ["darwin-aarch64-app", "darwin-aarch64", "darwin-x86_64"] {
            let error = release.download_url(target).unwrap_err();
            assert!(is_platform_not_found_error(&error));
        }
    }

    #[test]
    fn keeps_real_update_failures_visible() {
        for error in [
            Error::ReleaseNotFound,
            Error::UnsupportedArch,
            Error::UnsupportedOs,
            Error::Network("connection timed out".into()),
            Error::Network("response `platforms` object could not be fetched".into()),
            Error::Serialization(serde_json::from_str::<serde_json::Value>("{").unwrap_err()),
            Error::SignatureUtf8("invalid signature".into()),
        ] {
            assert!(!is_platform_not_found_error(&error), "{error}");
        }
    }
}
