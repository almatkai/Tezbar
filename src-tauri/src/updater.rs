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
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
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
    log::info!("Updater status set to: {status:?}");
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

/// Returns true if the updater error indicates that the current platform was not
/// found in the release manifest's platforms map.
pub fn is_platform_not_found_error(error: &tauri_plugin_updater::Error) -> bool {
    matches!(
        error,
        tauri_plugin_updater::Error::TargetNotFound(_)
            | tauri_plugin_updater::Error::TargetsNotFound(_)
    )
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateMetadata {
    pub version: String,
    pub notes: String,
    pub release_url: String,
}

#[allow(dead_code)]
pub fn resolve_update_status(
    check_result: Result<Option<UpdateMetadata>, &tauri_plugin_updater::Error>,
    current_version: &str,
) -> AppUpdateStatus {
    match check_result {
        Ok(Some(meta)) => AppUpdateStatus::Available {
            version: meta.version,
            notes: meta.notes,
            release_url: meta.release_url,
        },
        Ok(None) => AppUpdateStatus::UpToDate {
            version: current_version.to_string(),
        },
        Err(err) if is_platform_not_found_error(err) => AppUpdateStatus::UpToDate {
            version: current_version.to_string(),
        },
        Err(err) => AppUpdateStatus::Error {
            message: err.to_string(),
        },
    }
}

#[tauri::command]
pub async fn get_update_status(app: AppHandle) -> Result<AppUpdateStatus, String> {
    Ok(current_status(&app))
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<AppUpdateStatus, String> {
    log::info!("Updater: check_for_updates requested");
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
        Err(ref error) if is_platform_not_found_error(error) => {
            log::info!("Updater: platform not found for this architecture ({error:?}). Setting UpToDate.");
            if let Some(state) = app.try_state::<UpdaterState>() {
                *state.pending.lock().unwrap() = None;
            }
            let version = app.package_info().version.to_string();
            let status = AppUpdateStatus::UpToDate { version };
            set_status(&app, status.clone());
            Ok(status)
        }
        Err(error) => {
            log::error!("Updater: check error: {error:?}");
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
    if let AppUpdateStatus::Downloading { .. } | AppUpdateStatus::Ready { .. } =
        current_status(&app)
    {
        return Ok(current_status(&app));
    }

    #[cfg(target_os = "macos")]
    {
        let preparation = app
            .path()
            .app_cache_dir()
            .map_err(|error| format!("Cannot locate update cache directory: {error}"))
            .and_then(|cache| crate::updater_temp::prepare_temp_dir(&cache.join("updater-temp")));
        if let Err(message) = preparation {
            let status = AppUpdateStatus::Error { message };
            set_status(&app, status.clone());
            return Ok(status);
        }
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
    use super::*;

    const WINDOWS_ONLY_MANIFEST: &str = r#"{
      "version": "0.2.6",
      "notes": "Tezbar Windows release notes",
      "pub_date": "2026-10-07T07:49:10.768Z",
      "platforms": {
        "windows-x86_64-nsis": {
          "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZQ==",
          "url": "https://github.com/almatkai/Tezbar/releases/download/v0.2.6/Tezbar_0.2.6_x64-setup.exe"
        },
        "windows-x86_64": {
          "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZQ==",
          "url": "https://github.com/almatkai/Tezbar/releases/download/v0.2.6/Tezbar_0.2.6_x64-setup.exe"
        }
      }
    }"#;

    const MACOS_AND_WINDOWS_MANIFEST: &str = r#"{
      "version": "0.2.6",
      "notes": "Cross platform release",
      "pub_date": "2026-10-07T07:49:10.768Z",
      "platforms": {
        "darwin-aarch64": {
          "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZQ==",
          "url": "https://github.com/almatkai/Tezbar/releases/download/v0.2.6/Tezbar_aarch64.app.tar.gz"
        },
        "darwin-x86_64": {
          "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZQ==",
          "url": "https://github.com/almatkai/Tezbar/releases/download/v0.2.6/Tezbar_x64.app.tar.gz"
        },
        "windows-x86_64": {
          "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZQ==",
          "url": "https://github.com/almatkai/Tezbar/releases/download/v0.2.6/Tezbar_0.2.6_x64-setup.exe"
        }
      }
    }"#;

    /// Helper mirroring Tauri updater's fallback platform lookup logic.
    fn resolve_platform<'a>(
        release: &'a tauri_plugin_updater::RemoteRelease,
        targets: &[&str],
    ) -> Result<(&'a tauri::Url, &'a String), tauri_plugin_updater::Error> {
        for target in targets {
            if let (Ok(url), Ok(sig)) = (release.download_url(target), release.signature(target)) {
                return Ok((url, sig));
            }
        }
        if targets.len() == 1 {
            Err(tauri_plugin_updater::Error::TargetNotFound(targets[0].to_string()))
        } else {
            Err(tauri_plugin_updater::Error::TargetsNotFound(
                targets.iter().map(|s| s.to_string()).collect(),
            ))
        }
    }

    #[test]
    fn test_windows_only_manifest_macos_apple_silicon_targets_not_found() {
        let release: tauri_plugin_updater::RemoteRelease =
            serde_json::from_str(WINDOWS_ONLY_MANIFEST).expect("valid manifest JSON");

        // Tauri searches ["darwin-aarch64-app", "darwin-aarch64"] on Apple Silicon
        let targets = ["darwin-aarch64-app", "darwin-aarch64"];
        let result = resolve_platform(&release, &targets);

        match result {
            Err(ref err @ tauri_plugin_updater::Error::TargetsNotFound(ref missing)) => {
                assert_eq!(missing, &["darwin-aarch64-app", "darwin-aarch64"]);
                assert!(is_platform_not_found_error(err));
                let status = resolve_update_status(Err(err), "0.2.0-beta.11");
                assert_eq!(
                    status,
                    AppUpdateStatus::UpToDate {
                        version: "0.2.0-beta.11".to_string()
                    }
                );
            }
            other => panic!("expected TargetsNotFound error, got {:?}", other),
        }
    }

    #[test]
    fn test_windows_only_manifest_macos_intel_targets_not_found() {
        let release: tauri_plugin_updater::RemoteRelease =
            serde_json::from_str(WINDOWS_ONLY_MANIFEST).expect("valid manifest JSON");

        // Tauri searches ["darwin-x86_64-app", "darwin-x86_64"] on Intel Mac
        let targets = ["darwin-x86_64-app", "darwin-x86_64"];
        let result = resolve_platform(&release, &targets);

        match result {
            Err(ref err @ tauri_plugin_updater::Error::TargetsNotFound(ref missing)) => {
                assert_eq!(missing, &["darwin-x86_64-app", "darwin-x86_64"]);
                assert!(is_platform_not_found_error(err));
                let status = resolve_update_status(Err(err), "0.2.0-beta.11");
                assert_eq!(
                    status,
                    AppUpdateStatus::UpToDate {
                        version: "0.2.0-beta.11".to_string()
                    }
                );
            }
            other => panic!("expected TargetsNotFound error, got {:?}", other),
        }
    }

    #[test]
    fn test_single_target_not_found_error_treated_as_up_to_date() {
        let err_apple_silicon =
            tauri_plugin_updater::Error::TargetNotFound("darwin-aarch64".to_string());
        assert!(is_platform_not_found_error(&err_apple_silicon));
        assert_eq!(
            resolve_update_status(Err(&err_apple_silicon), "0.2.0-beta.11"),
            AppUpdateStatus::UpToDate {
                version: "0.2.0-beta.11".to_string()
            }
        );

        let err_intel =
            tauri_plugin_updater::Error::TargetNotFound("darwin-x86_64".to_string());
        assert!(is_platform_not_found_error(&err_intel));
        assert_eq!(
            resolve_update_status(Err(&err_intel), "0.2.0-beta.11"),
            AppUpdateStatus::UpToDate {
                version: "0.2.0-beta.11".to_string()
            }
        );
    }

    #[test]
    fn test_real_network_and_signature_errors_remain_errors() {
        let network_err = tauri_plugin_updater::Error::Network("connection refused".to_string());
        assert!(!is_platform_not_found_error(&network_err));
        assert_eq!(
            resolve_update_status(Err(&network_err), "0.2.0-beta.11"),
            AppUpdateStatus::Error {
                message: network_err.to_string()
            }
        );

        let release_not_found = tauri_plugin_updater::Error::ReleaseNotFound;
        assert!(!is_platform_not_found_error(&release_not_found));
        match resolve_update_status(Err(&release_not_found), "0.2.0-beta.11") {
            AppUpdateStatus::Error { message } => {
                assert!(message.contains("Could not fetch a valid release JSON"));
            }
            other => panic!("expected Error status, got {:?}", other),
        }

        let empty_endpoints = tauri_plugin_updater::Error::EmptyEndpoints;
        assert!(!is_platform_not_found_error(&empty_endpoints));
        assert_eq!(
            resolve_update_status(Err(&empty_endpoints), "0.2.0-beta.11"),
            AppUpdateStatus::Error {
                message: empty_endpoints.to_string()
            }
        );
    }

    #[test]
    fn test_macos_manifest_with_platform_available() {
        let release: tauri_plugin_updater::RemoteRelease =
            serde_json::from_str(MACOS_AND_WINDOWS_MANIFEST).expect("valid manifest JSON");

        let targets = ["darwin-aarch64-app", "darwin-aarch64"];
        let result = resolve_platform(&release, &targets);
        assert!(result.is_ok());

        let targets_intel = ["darwin-x86_64-app", "darwin-x86_64"];
        let result_intel = resolve_platform(&release, &targets_intel);
        assert!(result_intel.is_ok());

        let meta = UpdateMetadata {
            version: "0.2.6".to_string(),
            notes: "Cross platform release".to_string(),
            release_url: "https://github.com/almatkai/Tezbar/releases/tag/v0.2.6".to_string(),
        };
        assert_eq!(
            resolve_update_status(Ok(Some(meta)), "0.2.0-beta.11"),
            AppUpdateStatus::Available {
                version: "0.2.6".to_string(),
                notes: "Cross platform release".to_string(),
                release_url: "https://github.com/almatkai/Tezbar/releases/tag/v0.2.6".to_string(),
            }
        );
    }
}
