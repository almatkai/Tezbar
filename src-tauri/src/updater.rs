// src-tauri/src/updater.rs
//
// GitHub Releases–based update tracking. Only stable (non-prerelease,
// non-draft) releases are surfaced: beta tags are skipped, and the client
// additionally ignores versions with a pre-release suffix (e.g. 0.1.0-beta.1)
// even if the release was not flagged as a prerelease on GitHub.

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// GitHub repository that publishes Tezbar releases ("owner/repo").
const GITHUB_REPO: &str = "almatkai/Raymes";

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

#[tauri::command]
pub async fn get_update_status(app: AppHandle) -> Result<AppUpdateStatus, String> {
    Ok(current_status(&app))
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<AppUpdateStatus, String> {
    set_status(&app, AppUpdateStatus::Checking);

    let updater = app
        .updater_builder()
        .version_comparator(|current, release| {
            // Only offer updates that are (a) strictly newer than the running
            // build and (b) stable releases — never beta/pre-release versions.
            release.version.pre.is_empty() && release.version > current
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
    let download_result = update
        .download_and_install(
            move |downloaded, total| {
                set_status(
                    &app_for_events,
                    AppUpdateStatus::Downloading {
                        version: version.clone(),
                        downloaded: downloaded as u64,
                        total: total.map(|t| t as u64),
                    },
                );
            },
            || {},
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
