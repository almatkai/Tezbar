//! Portable updater contracts and policy; no window runtime or installer side effects at startup.
pub mod manual_releases;
pub mod release_channels;
#[cfg(unix)]
pub mod updater_temp;
use serde::{Deserialize, Serialize};
const GITHUB_REPO: &str = "almatkai/Tezbar";
/// Mirrors the renderer's `AppUpdateStatus` union (`src/shared/updater.ts`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
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
    ManualAvailable {
        version: String,
        notes: String,
        #[serde(rename = "releaseUrl")]
        release_url: String,
        #[serde(rename = "installerUrl")]
        installer_url: String,
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

pub fn validate_download_url(version: &semver::Version, url: &reqwest::Url) -> Result<(), String> {
    let prefix = format!("/{GITHUB_REPO}/releases/download/v{version}/");
    let filename = url.path().strip_prefix(&prefix).unwrap_or("");
    let product = "Tezbar";
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || filename.contains('/')
        || filename.to_ascii_lowercase().contains("%2f")
        || (!filename.starts_with(&format!("{product}_"))
            && filename != format!("{product}.app.tar.gz"))
    {
        return Err(
            "Update must reference an immutable release asset for this Tezbar installation".into(),
        );
    }
    Ok(())
}

pub mod native;
pub fn github_release_url(tag: &str, draft_fallback: &str) -> String {
    if tag.is_empty() {
        draft_fallback.to_string()
    } else {
        format!("https://github.com/{GITHUB_REPO}/releases/tag/{tag}")
    }
}

pub fn manual_status(release: &crate::manual_releases::ManualRelease) -> AppUpdateStatus {
    AppUpdateStatus::ManualAvailable {
        version: release.version.to_string(),
        notes: release.notes.clone(),
        release_url: release.release_url.clone(),
        installer_url: release.installer_url.clone(),
    }
}

pub fn published_release_status(
    published: Result<Option<crate::manual_releases::ManualRelease>, String>,
    current: &str,
    signed_error: Option<String>,
) -> AppUpdateStatus {
    match published {
        Ok(Some(release)) => manual_status(&release),
        Ok(None) => match signed_error {
            Some(message) => AppUpdateStatus::Error { message },
            None => AppUpdateStatus::UpToDate {
                version: current.into(),
            },
        },
        Err(message) => AppUpdateStatus::Error { message },
    }
}
