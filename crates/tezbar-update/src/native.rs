//! Lazy native update transport and verified staging. Construction performs no I/O.
use crate::{
    release_channels::{self, ReleaseChannel},
    validate_download_url, AppUpdateStatus,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::StreamExt;
use serde_json::Value;
use std::{
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::io::AsyncWriteExt;

#[derive(Clone, Debug)]
pub struct Release {
    pub version: semver::Version,
    pub notes: String,
    pub url: reqwest::Url,
    pub signature: String,
}
/// The original platform fallback order is significant (NSIS/app suffix first).
pub fn select_release(
    document: &Value,
    current: &semver::Version,
    channel: ReleaseChannel,
    identifier: &str,
    targets: &[&str],
) -> Result<Option<Release>, String> {
    let version = semver::Version::parse(
        document["version"]
            .as_str()
            .ok_or("Update version missing")?
            .trim_start_matches('v'),
    )
    .map_err(|e| e.to_string())?;
    if version <= *current || !channel.accepts(&version) {
        return Ok(None);
    }
    release_channels::validate_manifest_identity(channel, identifier, &version, document)?;
    let platform = targets
        .iter()
        .find_map(|target| document["platforms"].get(*target));
    let Some(platform) = platform else {
        return Ok(None);
    };
    let url = reqwest::Url::parse(platform["url"].as_str().ok_or("Update URL missing")?)
        .map_err(|e| e.to_string())?;
    validate_download_url(&version, &url)?;
    let signature = platform["signature"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("Update signature missing")?
        .to_owned();
    Ok(Some(Release {
        version,
        notes: document["notes"].as_str().unwrap_or("").into(),
        url,
        signature,
    }))
}
#[derive(Clone)]
pub struct Config {
    pub current: semver::Version,
    pub identifier: String,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub public_key: String,
    pub targets: Vec<String>,
}
#[derive(Debug)]
enum CheckError {
    Validation(String),
    Transport(String),
}
struct DownloadGuard<'a> {
    updater: &'a Updater,
    armed: bool,
}
impl Drop for DownloadGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.updater.publish(AppUpdateStatus::Error {
                message: "Update download cancelled".into(),
            });
        }
    }
}
struct State {
    status: AppUpdateStatus,
    pending: Option<Release>,
}
pub struct Updater {
    config: Config,
    operation: tokio::sync::Mutex<()>,
    state: Mutex<State>,
    events: Arc<dyn Fn(AppUpdateStatus) + Send + Sync>,
    client: std::sync::OnceLock<Result<reqwest::Client, String>>,
}
impl Updater {
    pub fn new(config: Config, events: Arc<dyn Fn(AppUpdateStatus) + Send + Sync>) -> Self {
        Self {
            config,
            client: std::sync::OnceLock::new(),
            operation: tokio::sync::Mutex::new(()),
            state: Mutex::new(State {
                status: AppUpdateStatus::Idle,
                pending: None,
            }),
            events,
        }
    }
    fn http(&self) -> Result<&reqwest::Client, String> {
        self.client
            .get_or_init(|| {
                reqwest::Client::builder()
                    .timeout(Duration::from_secs(15))
                    .user_agent(format!("Tezbar-App/{}", self.config.current))
                    .build()
                    .map_err(|e| e.to_string())
            })
            .as_ref()
            .map_err(Clone::clone)
    }
    /// Owner has dropped all old-generation work/artifacts before resetting.
    pub fn reset(&self, failed: bool) {
        self.state.lock().unwrap().pending = None;
        self.publish(if failed {
            AppUpdateStatus::Error {
                message: "Update interrupted. Check again to retry.".into(),
            }
        } else {
            AppUpdateStatus::Idle
        });
    }
    pub fn status(&self) -> AppUpdateStatus {
        self.state.lock().unwrap().status.clone()
    }
    fn publish(&self, status: AppUpdateStatus) -> AppUpdateStatus {
        self.state.lock().unwrap().status = status.clone();
        (self.events)(status.clone());
        status
    }
    pub fn channel(&self) -> Result<ReleaseChannel, String> {
        release_channels::read_channel(
            &self.config.data_dir,
            &self.config.identifier,
            &self.config.current,
        )
    }
    pub async fn set_channel(&self, channel: ReleaseChannel) -> Result<(), String> {
        let _op = self.operation.lock().await;
        if matches!(
            self.status(),
            AppUpdateStatus::Downloading { .. } | AppUpdateStatus::Ready { .. }
        ) {
            return Err("Finish the current update before changing channels".into());
        }
        if self.config.identifier != release_channels::NORMAL_APP_ID {
            return Err("This installation has an unsupported updater identity".into());
        }
        release_channels::write_channel(&self.config.data_dir, channel)?;
        self.state.lock().unwrap().pending = None;
        self.publish(AppUpdateStatus::Idle);
        Ok(())
    }
    /// Network is reached only on explicit check, never from constructor or UI render.
    pub async fn check(&self) -> Result<AppUpdateStatus, String> {
        let _op = self.operation.lock().await;
        if matches!(
            self.status(),
            AppUpdateStatus::Downloading { .. } | AppUpdateStatus::Ready { .. }
        ) {
            return Ok(self.status());
        }
        self.state.lock().unwrap().pending = None;
        let channel = self.channel()?;
        self.publish(AppUpdateStatus::Checking);
        let client = self.http()?;
        let signed = async {
            let mut error = None;
            for endpoint in channel.endpoints() {
                match bounded_json(&client, &endpoint).await {
                    Ok(Some(doc)) => {
                        return select_release(
                            &doc,
                            &self.config.current,
                            channel,
                            &self.config.identifier,
                            &self
                                .config
                                .targets
                                .iter()
                                .map(String::as_str)
                                .collect::<Vec<_>>(),
                        )
                        .map_err(CheckError::Validation)
                    }
                    Ok(None) => {}
                    Err(e) => error = Some(e),
                }
            }
            match error {
                Some(e) => Err(CheckError::Transport(e)),
                None => Ok(None),
            }
        };
        let (signed, published) = tokio::join!(
            signed,
            crate::manual_releases::check_with_client(client, &self.config.current, channel)
        );
        Ok(self.apply_check(signed, published))
    }
    fn apply_check(
        &self,
        signed: Result<Option<Release>, CheckError>,
        published: Result<Option<crate::manual_releases::ManualRelease>, String>,
    ) -> AppUpdateStatus {
        if let Err(CheckError::Validation(message)) = &signed {
            return self.publish(AppUpdateStatus::Error {
                message: message.clone(),
            });
        }
        if let Ok(Some(release)) = &signed {
            if !matches!(&published,Ok(Some(p)) if p.version>release.version) {
                self.state.lock().unwrap().pending = Some(release.clone());
                return self.publish(AppUpdateStatus::Available {
                    version: release.version.to_string(),
                    notes: release.notes.clone(),
                    release_url: format!(
                        "https://github.com/almatkai/Tezbar/releases/tag/v{}",
                        release.version
                    ),
                });
            }
        }
        self.publish(crate::published_release_status(
            published,
            &self.config.current.to_string(),
            match signed {
                Err(CheckError::Transport(message)) | Err(CheckError::Validation(message)) => {
                    Some(message)
                }
                _ => None,
            },
        ))
    }
    /// Download to a private file. Progress is throttled; no artifact-sized heap buffer.
    pub async fn download(&self) -> Result<VerifiedArtifact, String> {
        let _op = self.operation.lock().await;
        let release = self
            .state
            .lock()
            .unwrap()
            .pending
            .take()
            .ok_or("No pending update. Run check_for_updates first.")?;
        // Revalidate the selected channel immediately before retrieving any bytes.
        if !self.channel()?.accepts(&release.version) {
            return Err("Update channel changed".into());
        }
        let mut guard = DownloadGuard {
            updater: self,
            armed: true,
        };
        let result = async {
            std::fs::create_dir_all(&self.config.cache_dir).map_err(|e| e.to_string())?;
            let file = tempfile::Builder::new()
                .prefix("tezbar-update-")
                .tempfile_in(&self.config.cache_dir)
                .map_err(|e| e.to_string())?;
            let mut writer =
                tokio::fs::File::from_std(file.as_file().try_clone().map_err(|e| e.to_string())?);
            let response = self
                .http()?
                .get(release.url.clone())
                .timeout(Duration::from_secs(900))
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?;
            let total = response.content_length();
            let mut stream = response.bytes_stream();
            let mut count = 0u64;
            let mut last = Instant::now();
            self.publish(AppUpdateStatus::Downloading {
                version: release.version.to_string(),
                downloaded: 0,
                total,
            });
            while let Some(bytes) = stream.next().await {
                let bytes = bytes.map_err(|e| e.to_string())?;
                writer.write_all(&bytes).await.map_err(|e| e.to_string())?;
                count = count
                    .checked_add(bytes.len() as u64)
                    .ok_or("Update size overflow")?;
                if last.elapsed() >= Duration::from_millis(150) || total == Some(count) {
                    self.publish(AppUpdateStatus::Downloading {
                        version: release.version.to_string(),
                        downloaded: count,
                        total,
                    });
                    last = Instant::now();
                }
            }
            writer.flush().await.map_err(|e| e.to_string())?;
            drop(writer);
            if total.is_some_and(|n| n != count) {
                return Err("Incomplete update download".into());
            }
            let key = self.config.public_key.clone();
            let release = release.clone();
            tokio::task::spawn_blocking(move || VerifiedArtifact::verify(file, release, &key))
                .await
                .map_err(|e| e.to_string())?
        }
        .await;
        guard.armed = false;
        match &result {
            Err(message) => {
                self.publish(AppUpdateStatus::Error {
                    message: message.clone(),
                });
            }
            Ok(artifact) => {
                self.publish(AppUpdateStatus::Ready {
                    version: artifact.version().to_string(),
                });
            }
        }
        result
    }
}
async fn bounded_json(client: &reqwest::Client, url: &str) -> Result<Option<Value>, String> {
    let response = client.get(url).send().await.map_err(|e| e.to_string())?;
    if response.status() == reqwest::StatusCode::NO_CONTENT
        || response.status() == reqwest::StatusCode::NOT_FOUND
    {
        return Ok(None);
    }
    let mut response = response
        .error_for_status()
        .map_err(|e| e.to_string())?
        .bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        if bytes.len() + chunk.len() > 1024 * 1024 {
            return Err("Update manifest exceeds 1 MiB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| e.to_string())
}
/// A verified, privately owned file, not an arbitrary path or an unchecked byte vector.
pub struct VerifiedArtifact {
    file: std::fs::File,
    release: Release,
}
impl VerifiedArtifact {
    pub fn verify(
        mut source: tempfile::NamedTempFile,
        release: Release,
        public_key: &str,
    ) -> Result<Self, String> {
        // Anonymous private snapshot: callers cannot retain a mutable file handle/path
        // to the bytes subsequently verified, mapped or extracted.
        let mut file =
            tempfile::tempfile_in(source.path().parent().ok_or("Update file has no parent")?)
                .map_err(|e| e.to_string())?;
        source
            .as_file_mut()
            .seek(SeekFrom::Start(0))
            .map_err(|e| e.to_string())?;
        std::io::copy(&mut source, &mut file).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(source);
        let decode = |s: &str| -> Result<String, String> {
            String::from_utf8(STANDARD.decode(s).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())
        };
        let key =
            minisign_verify::PublicKey::decode(&decode(public_key)?).map_err(|e| e.to_string())?;
        let signature = minisign_verify::Signature::decode(&decode(&release.signature)?)
            .map_err(|e| e.to_string())?;
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        match key.verify_stream(&signature) {
            Ok(mut verifier) => {
                let mut buffer = [0u8; 64 * 1024];
                loop {
                    let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    verifier.update(&buffer[..n]);
                }
                verifier.finalize().map_err(|e| e.to_string())?;
            }
            Err(minisign_verify::Error::UnsupportedLegacyMode) => {
                verify_legacy(&file, &key, &signature)?
            }
            Err(e) => return Err(e.to_string()),
        }
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        Ok(Self { file, release })
    }
    pub fn version(&self) -> &semver::Version {
        &self.release.version
    }
    #[cfg(unix)]
    pub fn stage_macos(mut self, cache: &Path) -> Result<StagedMac, String> {
        self.prepare_macos(cache)
    }
    /// Stage without consuming the verified download, so a pre-quit admission
    /// refusal or cancelled quit can return the same artifact to the updater.
    #[cfg(unix)]
    pub fn prepare_macos(&mut self, cache: &Path) -> Result<StagedMac, String> {
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|e| e.to_string())?;
        let directory = tempfile::Builder::new()
            .prefix("tezbar-stage-")
            .tempdir_in(cache)
            .map_err(|e| e.to_string())?;
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(&mut self.file));
        for entry in archive.entries().map_err(|e| e.to_string())? {
            let mut entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path().map_err(|e| e.to_string())?.into_owned();
            if !safe_bundle_path(&path) {
                return Err("Update archive contains a path outside Tezbar.app".into());
            }
            if !entry
                .unpack_in(directory.path())
                .map_err(|e| e.to_string())?
            {
                return Err("Unsafe update archive entry".into());
            }
        }
        let bundle = directory.path().join("Tezbar.app");
        if !bundle.join("Contents/Info.plist").is_file()
            || !bundle.join("Contents/MacOS/tezbar").is_file()
        {
            return Err("Update does not contain a complete Tezbar.app".into());
        }
        Ok(StagedMac { directory, bundle })
    }
    pub fn stage_windows(mut self, cache: &Path) -> Result<StagedWindows, String> {
        let directory = tempfile::Builder::new()
            .prefix("tezbar-stage-")
            .tempdir_in(cache)
            .map_err(|e| e.to_string())?;
        let installer = directory.path().join("Tezbar-setup.exe");
        let mut target =
            tempfile::NamedTempFile::new_in(directory.path()).map_err(|e| e.to_string())?;
        std::io::copy(&mut self.file, &mut target).map_err(|e| e.to_string())?;
        target.as_file().sync_all().map_err(|e| e.to_string())?;
        target.persist(&installer).map_err(|e| e.to_string())?;
        Ok(StagedWindows {
            directory,
            installer,
        })
    }
}
fn safe_bundle_path(path: &Path) -> bool {
    let mut parts = path.components();
    parts.next() == Some(std::path::Component::Normal("Tezbar.app".as_ref()))
        && parts.all(|p| matches!(p, std::path::Component::Normal(_)))
}
#[cfg(unix)]
pub struct StagedMac {
    directory: tempfile::TempDir,
    pub bundle: PathBuf,
}
#[cfg(unix)]
impl StagedMac {
    /// The production verifier is mandatory; install is never called by construction/check/download.
    pub fn install_over(self, installed: &Path) -> Result<InstalledMac, String> {
        if installed.file_name() != Some("Tezbar.app".as_ref())
            || std::fs::symlink_metadata(installed)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
        {
            return Err("Invalid installed bundle path".into());
        }
        verify_macos_bundle_identity(&self.bundle)?;
        let result = swap_bundle(&self.bundle, installed);
        let _ = self.directory;
        result
    }
}
/// Shared identity verifier for admission and staged installs.
#[cfg(unix)]
pub fn verify_macos_bundle_identity(bundle: &Path) -> Result<(), String> {
    let pin: Value = serde_json::from_str(include_str!("../../../packaging/macos-signing.json"))
        .map_err(|e| e.to_string())?;
    let requirement = pin["designatedRequirement"]
        .as_str()
        .ok_or("Pinned designated requirement missing")?;
    for args in mac_verification_arguments(requirement) {
        let out = std::process::Command::new("/usr/bin/codesign")
            .args(args)
            .arg(bundle)
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).into_owned());
        }
    }
    let outer = std::process::Command::new("/usr/bin/codesign")
        .args(["-dv", "--verbose=4"])
        .arg(bundle)
        .output()
        .map_err(|e| e.to_string())?;
    if !outer.status.success()
        || !pinned_team(
            &String::from_utf8_lossy(&outer.stderr),
            pin["teamIdentifier"]
                .as_str()
                .ok_or("Pinned team missing")?,
        )
    {
        return Err("Outer app Team ID does not match the pin".into());
    }
    for helper in pin["nativeHelpers"]
        .as_array()
        .ok_or("Pinned helper list missing")?
    {
        let path = bundle
            .join("Contents/Resources")
            .join(helper.as_str().ok_or("Invalid pinned helper")?);
        let output = std::process::Command::new("/usr/bin/codesign")
            .args(["--verify", "--strict"])
            .arg(&path)
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err("Update helper signature verification failed".into());
        }
        let output = std::process::Command::new("/usr/bin/codesign")
            .args(["-dv", "--verbose=4"])
            .arg(&path)
            .output()
            .map_err(|e| e.to_string())?;
        let expected = format!(
            "TeamIdentifier={}",
            pin["teamIdentifier"]
                .as_str()
                .ok_or("Pinned team missing")?
        );
        if !output.status.success()
            || !String::from_utf8_lossy(&output.stderr)
                .lines()
                .any(|line| line == expected)
        {
            return Err("Update helper Team ID does not match the pin".into());
        }
    }

    let output = std::process::Command::new("/usr/bin/codesign")
        .args(["-d", "-r-"])
        .arg(bundle)
        .output()
        .map_err(|e| e.to_string())?;
    let observed = String::from_utf8_lossy(&output.stderr);
    if !output.status.success()
        || observed
            .lines()
            .find_map(|l| l.strip_prefix("designated => "))
            != Some(requirement)
    {
        return Err("Designated requirement differs from the committed pin".into());
    }
    Ok(())
}
#[cfg(unix)]
fn pinned_team(detail: &str, team: &str) -> bool {
    let teams: Vec<_> = detail
        .lines()
        .filter_map(|l| l.strip_prefix("TeamIdentifier="))
        .collect();
    teams == [team] && !detail.lines().any(|l| l == "Signature=adhoc")
}
/// Refuse an unwritable installation before the application quiesces or quits.
/// The probe owns and removes only a newly created private directory.
#[cfg(unix)]
pub fn preflight_macos_install(installed: &Path) -> Result<(), String> {
    use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
    const REFUSAL: &str = "Tezbar cannot update in this folder. Move it to a writable Applications folder, then retry. The verified download has been kept.";
    let meta = std::fs::symlink_metadata(installed).map_err(|_| REFUSAL)?;
    if installed.file_name().and_then(|v| v.to_str()) != Some("Tezbar.app") || !meta.is_dir() {
        return Err("The installed Tezbar bundle is unavailable".into());
    }
    let directory = installed.parent().ok_or("Missing installation directory")?;
    let parent = std::fs::metadata(directory).map_err(|_| REFUSAL)?;
    let bundle_path =
        std::ffi::CString::new(installed.as_os_str().as_bytes()).map_err(|_| REFUSAL)?;
    // access() includes the current user's ACLs; mode bits alone miss MDM and
    // cross-user installations. A sticky parent also requires bundle ownership.
    let writable = unsafe { libc::access(bundle_path.as_ptr(), libc::W_OK) == 0 };
    if !writable
        || parent.mode() & u32::from(libc::S_ISVTX) != 0 && meta.uid() != unsafe { libc::geteuid() }
    {
        return Err(REFUSAL.into());
    }
    tempfile::Builder::new()
        .prefix(".tezbar-install-check-")
        .tempdir_in(directory)
        .and_then(|probe| probe.close())
        .map_err(|_| REFUSAL.into())
}
#[cfg(unix)]
fn swap_bundle(staged: &Path, installed: &Path) -> Result<InstalledMac, String> {
    let backup = tempfile::Builder::new()
        .prefix("tezbar-previous-")
        .tempdir_in(installed.parent().ok_or("Bundle has no parent")?)
        .map_err(|e| e.to_string())?;
    let previous = backup.path().join("Tezbar.app");
    std::fs::rename(installed, &previous).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(staged, installed) {
        if let Err(restore) = std::fs::rename(&previous, installed) {
            let retained = backup.keep();
            return Err(format!(
                "{e}; rollback failed: {restore}; backup retained at {}",
                retained.display()
            ));
        }
        return Err(e.to_string());
    }
    if let Err(error) = std::fs::File::open(installed)
        .and_then(|file| file.set_modified(std::time::SystemTime::now()))
    {
        let restore =
            std::fs::rename(installed, staged).and_then(|_| std::fs::rename(&previous, installed));
        if let Err(restore) = restore {
            let path = backup.keep();
            return Err(format!(
                "{error}; rollback failed: {restore}; backup retained at {}",
                path.display()
            ));
        }
        return Err(error.to_string());
    }
    Ok(InstalledMac {
        backup: backup.path().into(),
        installed: installed.into(),
        directory: backup,
    })
}
/// Keep the previous bundle until the relaunch coordinator explicitly confirms success.
#[cfg(unix)]
pub struct InstalledMac {
    pub backup: PathBuf,
    pub installed: PathBuf,
    directory: tempfile::TempDir,
}
#[cfg(unix)]
impl InstalledMac {
    /// End rollback ownership after the coordinator successfully spawned the new
    /// executable (the same success boundary as the legacy updater). This is not
    /// a claim that the new GUI has reached readiness.
    pub fn confirm_relaunch(self) -> Result<(), String> {
        self.directory.close().map_err(|e| e.to_string())
    }
    pub fn rollback(self) -> Result<(), String> {
        let failed = self.backup.join("failed.app");
        let result = std::fs::rename(&self.installed, &failed)
            .and_then(|_| std::fs::rename(self.backup.join("Tezbar.app"), &self.installed));
        if let Err(error) = result {
            let path = self.directory.keep();
            return Err(format!(
                "{error}; rollback files preserved at {}",
                path.display()
            ));
        }
        Ok(())
    }
}
pub struct StagedWindows {
    directory: tempfile::TempDir,
    pub installer: PathBuf,
}
impl StagedWindows {
    /// Raw NSIS parameter fragments, joined once for ShellExecuteW. `/R` keeps
    /// the passive install's relaunch contract from tauri-plugin-updater 2.10.1.
    pub fn arguments(current_args: &[String]) -> Vec<String> {
        ["/P", "/R", "/UPDATE", "/ARGS"]
            .into_iter()
            .map(String::from)
            .chain(current_args.iter().map(|arg| escape_nsis_argument(arg)))
            .collect()
    }
    #[cfg(windows)]
    pub fn launch(self, current_args: &[String]) -> Result<(), String> {
        self.launch_os(
            &current_args
                .iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>(),
        )
    }
    #[cfg(windows)]
    pub fn launch_os(self, current_args: &[std::ffi::OsString]) -> Result<(), String> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOW};
        let args = current_args
            .iter()
            .map(|s| s.encode_wide().collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let parameters = nsis_parameters_wide(&args)?;
        let file: Vec<u16> = self
            .installer
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        if file[..file.len() - 1].contains(&0) || parameters.contains(&0) {
            return Err("Installer arguments contain a null character".into());
        }
        let parameters: Vec<u16> = parameters.into_iter().chain(Some(0)).collect();
        let verb: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
        // ShellExecute preserves the installer's elevation behavior and consumes
        // the already-escaped NSIS command line without a second quoting pass.
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                parameters.as_ptr(),
                std::ptr::null(),
                SW_SHOW,
            )
        } as isize;
        if result <= 32 {
            return Err(format!(
                "Installer launch failed with ShellExecuteW code {result}"
            ));
        }
        self.directory.keep();
        Ok(())
    }
    pub fn staging_root(&self) -> &Path {
        self.directory.path()
    }
}
#[cfg(unix)]
fn mac_verification_arguments(requirement: &str) -> [Vec<String>; 2] {
    [
        ["--verify", "--deep", "--strict"]
            .map(String::from)
            .to_vec(),
        vec![
            "--verify".into(),
            "--strict".into(),
            format!("-R={requirement}"),
        ],
    ]
}

/// Port of the locked updater's NSIS escaping. In addition to Windows quoting,
/// slash-containing arguments must be quoted for NSIS GetOptions.
fn escape_nsis_argument(argument: &str) -> String {
    String::from_utf16(&escape_nsis_wide(
        &argument.encode_utf16().collect::<Vec<_>>(),
    ))
    .expect("valid Unicode remains valid")
}
fn escape_nsis_wide(argument: &[u16]) -> Vec<u16> {
    let quote = argument.is_empty() || argument.iter().any(|c| matches!(*c, 32 | 9 | 47));
    let mut escaped = Vec::new();
    if quote {
        escaped.push(34);
    }
    let mut backslashes = 0;
    for &c in argument {
        if c == 92 {
            backslashes += 1;
        } else {
            if c == 34 {
                escaped.extend(std::iter::repeat(92).take(backslashes + 1));
            }
            backslashes = 0;
        }
        escaped.push(c);
    }
    if quote {
        escaped.extend(std::iter::repeat(92).take(backslashes));
        escaped.push(34);
    }
    escaped
}
#[cfg(any(windows, test))]
fn nsis_parameters_wide(args: &[Vec<u16>]) -> Result<Vec<u16>, String> {
    let mut result = "/P /R /UPDATE /ARGS".encode_utf16().collect::<Vec<_>>();
    for arg in args {
        result.push(32);
        result.extend(escape_nsis_wide(arg));
    }
    if result.contains(&0) || result.len() > 32767 {
        return Err("Installer arguments contain null or exceed the Windows limit".into());
    }
    Ok(result)
}

#[cfg(unix)]
fn verify_legacy(
    file: &std::fs::File,
    key: &minisign_verify::PublicKey,
    signature: &minisign_verify::Signature,
) -> Result<(), String> {
    use std::os::fd::AsRawFd;
    let len = usize::try_from(file.metadata().map_err(|e| e.to_string())?.len())
        .map_err(|e| e.to_string())?;
    if len == 0 {
        return key.verify(&[], signature, true).map_err(|e| e.to_string());
    }
    // SAFETY: private immutable file remains open for the mapping's entire lifetime.
    let map = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            len,
            libc::PROT_READ,
            libc::MAP_PRIVATE,
            file.as_raw_fd(),
            0,
        )
    };
    if map == libc::MAP_FAILED {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let result = key
        .verify(
            unsafe { std::slice::from_raw_parts(map.cast(), len) },
            signature,
            true,
        )
        .map_err(|e| e.to_string());
    unsafe { libc::munmap(map, len) };
    result
}
#[cfg(windows)]
fn verify_legacy(
    file: &std::fs::File,
    key: &minisign_verify::PublicKey,
    signature: &minisign_verify::Signature,
) -> Result<(), String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{Foundation::CloseHandle, System::Memory::*};
    let len = usize::try_from(file.metadata().map_err(|e| e.to_string())?.len())
        .map_err(|e| e.to_string())?;
    if len == 0 {
        return key.verify(&[], signature, true).map_err(|e| e.to_string());
    }
    unsafe {
        let handle = CreateFileMappingW(
            file.as_raw_handle() as _,
            std::ptr::null(),
            PAGE_READONLY,
            0,
            0,
            std::ptr::null(),
        );
        if handle.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let map = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, len);
        if map.Value.is_null() {
            CloseHandle(handle);
            return Err(std::io::Error::last_os_error().to_string());
        }
        let result = key
            .verify(
                std::slice::from_raw_parts(map.Value.cast(), len),
                signature,
                true,
            )
            .map_err(|e| e.to_string());
        UnmapViewOfFile(map);
        CloseHandle(handle);
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn release() -> Release {
        Release {version:"0.3.0-beta.8".parse().unwrap(),notes:"fixture".into(),url:"https://github.com/almatkai/Tezbar/releases/download/v0.3.0-beta.8/Tezbar.app.tar.gz".parse().unwrap(),signature:STANDARD.encode(include_bytes!("../tests/fixtures/fixture.tar.gz.minisig"))}
    }
    fn artifact(bytes: &[u8]) -> Result<VerifiedArtifact, String> {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(bytes).unwrap();
        VerifiedArtifact::verify(
            f,
            release(),
            &STANDARD.encode(include_bytes!("../tests/fixtures/fixture.pub")),
        )
    }
    #[test]
    fn actual_minisign_fixture_rejects_tamper_before_archive_extraction() {
        let bytes = include_bytes!("../tests/fixtures/fixture.tar.gz");
        assert!(artifact(bytes).is_ok());
        let mut tamper = bytes.to_vec();
        tamper[10] ^= 1;
        assert!(artifact(&tamper).is_err());
        assert!(artifact(b"").is_err());
    }
    #[test]
    #[cfg(unix)]
    fn verified_fixture_stages_only_within_owned_temp_and_never_installs() {
        let d = tempfile::tempdir().unwrap();
        let staged = artifact(include_bytes!("../tests/fixtures/fixture.tar.gz"))
            .unwrap()
            .stage_macos(d.path())
            .unwrap();
        assert!(staged.bundle.starts_with(d.path()));
        assert_eq!(
            std::fs::read(staged.bundle.join("Contents/MacOS/tezbar")).unwrap(),
            b"fixture executable, never run"
        );
        drop(staged);
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
    }
    #[test]
    #[cfg(unix)]
    fn prepare_keeps_verified_download_for_cancel_and_retry() {
        let root = tempfile::tempdir().unwrap();
        let mut download = artifact(include_bytes!("../tests/fixtures/fixture.tar.gz")).unwrap();
        for _ in 0..2 {
            let staged = download.prepare_macos(root.path()).unwrap();
            assert_eq!(
                std::fs::read(staged.bundle.join("Contents/MacOS/tezbar")).unwrap(),
                b"fixture executable, never run"
            );
            drop(staged);
            assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
        }
    }
    #[test]
    fn bundle_paths_reject_traversal_absolute_and_other_products() {
        for path in [
            "../Tezbar.app/file",
            "/Tezbar.app/file",
            "Other.app/file",
            "Tezbar.app/../../escape",
        ] {
            assert!(!safe_bundle_path(Path::new(path)), "{path}");
        }
        assert!(safe_bundle_path(Path::new(
            "Tezbar.app/Contents/MacOS/tezbar"
        )));
    }
    #[test]
    fn feed_keeps_identity_channels_and_platform_fallback() {
        let current = "0.3.0-beta.7".parse().unwrap();
        let mut document = serde_json::json!({"version":"0.3.0-beta.8","appIdentifier":"com.tezbar.app","channel":"beta","platforms":{"darwin-aarch64":{"url":release().url.to_string(),"signature":release().signature}}});
        assert!(select_release(
            &document,
            &current,
            ReleaseChannel::Beta,
            "com.tezbar.app",
            &["darwin-aarch64-app", "darwin-aarch64"]
        )
        .unwrap()
        .is_some());
        assert!(select_release(
            &document,
            &current,
            ReleaseChannel::Stable,
            "com.tezbar.app",
            &["darwin-aarch64"]
        )
        .unwrap()
        .is_none());
        document["appIdentifier"] = serde_json::json!("evil.example");
        assert!(select_release(
            &document,
            &current,
            ReleaseChannel::Beta,
            "com.tezbar.app",
            &["darwin-aarch64"]
        )
        .is_err());
    }
    #[test]
    fn constructor_is_inert_and_installer_arguments_preserve_required_flags() {
        let d = tempfile::tempdir().unwrap();
        let missing = d.path().join("not-created");
        let updater = Updater::new(
            Config {
                current: "0.3.0".parse().unwrap(),
                identifier: "com.tezbar.app".into(),
                data_dir: missing.clone(),
                cache_dir: missing.clone(),
                public_key: String::new(),
                targets: vec![],
            },
            Arc::new(|_| panic!("constructor must not emit")),
        );
        assert_eq!(updater.status(), AppUpdateStatus::Idle);
        assert!(!missing.exists());
        let args =
            StagedWindows::arguments(&["--profile".into(), "C:\\fixture with spaces".into()]);
        assert_eq!(&args[..4], ["/P", "/R", "/UPDATE", "/ARGS"]);
        assert_eq!(&args[4..], ["--profile", "\"C:\\fixture with spaces\""]);
        assert_eq!(
            StagedWindows::arguments(&[]).join(" "),
            "/P /R /UPDATE /ARGS"
        );
    }

    #[test]
    fn nsis_relaunch_arguments_preserve_spaces_slashes_quotes_and_trailing_backslashes() {
        let cases = [
            ("something", "something"),
            ("--flag", "--flag"),
            ("some space", "\"some space\""),
            ("--arg=\"wrapped\"", "--arg=\\\"wrapped\\\""),
            ("--arg=\"wrapped space\"", "\"--arg=\\\"wrapped space\\\"\""),
            ("/profile", "\"/profile\""),
            ("C:\\dir with spaces\\", "\"C:\\dir with spaces\\\\\""),
            ("C:\\dir\\", "C:\\dir\\"),
            ("a\\\"b", "a\\\\\\\"b"),
            ("", "\"\""),
            ("қазақша 😀", "\"қазақша 😀\""),
        ];
        for (input, expected) in cases {
            assert_eq!(escape_nsis_argument(input), expected, "{input:?}");
        }
        let inputs: Vec<String> = cases.iter().map(|(input, _)| (*input).into()).collect();
        let arguments = StagedWindows::arguments(&inputs);
        assert_eq!(arguments.len(), inputs.len() + 4);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn inline_mac_requirement_is_enforced_by_codesign() {
        for (requirement, succeeds) in [("anchor apple", true), ("identifier \"com.wrong\"", false)]
        {
            let args = mac_verification_arguments(requirement);
            assert_eq!(args[1][2], format!("-R={requirement}"));
            let output = std::process::Command::new("/usr/bin/codesign")
                .args(&args[1])
                .arg("/bin/ls")
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                succeeds,
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    fn updater(root: &Path) -> Updater {
        Updater::new(
            Config {
                current: "0.3.0-beta.7".parse().unwrap(),
                identifier: "com.tezbar.app".into(),
                data_dir: root.into(),
                cache_dir: root.into(),
                public_key: String::new(),
                targets: vec!["darwin-aarch64".into()],
            },
            Arc::new(|_| {}),
        )
    }
    #[test]
    fn invalid_signed_identity_is_not_hidden_by_manual_release_fallback() {
        let d = tempfile::tempdir().unwrap();
        let updater = updater(d.path());
        let manual = crate::manual_releases::ManualRelease {
            version: "0.3.0-beta.9".parse().unwrap(),
            notes: "".into(),
            release_url: "".into(),
            installer_url: "".into(),
        };
        assert_eq!(
            updater.apply_check(
                Err(CheckError::Validation("bad identity".into())),
                Ok(Some(manual))
            ),
            AppUpdateStatus::Error {
                message: "bad identity".into()
            }
        );
    }
    #[test]
    fn failed_public_discovery_never_claims_up_to_date_and_network_error_is_retained() {
        let d = tempfile::tempdir().unwrap();
        let updater = updater(d.path());
        assert_eq!(
            updater.apply_check(Ok(None), Err("GitHub unavailable".into())),
            AppUpdateStatus::Error {
                message: "GitHub unavailable".into()
            }
        );
        assert_eq!(
            updater.apply_check(Err(CheckError::Transport("network".into())), Ok(None)),
            AppUpdateStatus::Error {
                message: "network".into()
            }
        );
    }
    #[tokio::test]
    async fn in_flight_channel_changes_are_rejected_without_changing_preferences() {
        let d = tempfile::tempdir().unwrap();
        let updater = updater(d.path());
        release_channels::write_channel(d.path(), ReleaseChannel::Beta).unwrap();
        updater.publish(AppUpdateStatus::Downloading {
            version: "1".into(),
            downloaded: 0,
            total: None,
        });
        assert!(updater.set_channel(ReleaseChannel::Alpha).await.is_err());
        assert_eq!(updater.channel().unwrap(), ReleaseChannel::Beta);
        updater.publish(AppUpdateStatus::Idle);
        updater.set_channel(ReleaseChannel::Alpha).await.unwrap();
        assert_eq!(updater.channel().unwrap(), ReleaseChannel::Alpha);
    }
    #[test]
    fn download_cancellation_guard_clears_the_busy_state() {
        let d = tempfile::tempdir().unwrap();
        let updater = updater(d.path());
        updater.publish(AppUpdateStatus::Downloading {
            version: "1".into(),
            downloaded: 0,
            total: None,
        });
        drop(DownloadGuard {
            updater: &updater,
            armed: true,
        });
        assert!(matches!(updater.status(), AppUpdateStatus::Error { .. }));
    }
    #[test]
    fn review_m11_nsis_wide_arguments_preserve_unpaired_surrogates() {
        let arg = vec![0xd800, 32, 92];
        let escaped = escape_nsis_wide(&arg);
        assert_eq!(escaped, vec![34, 0xd800, 32, 92, 92, 34]);
        let line = nsis_parameters_wide(&[arg]).unwrap();
        assert!(line.contains(&0xd800));
        assert!(nsis_parameters_wide(&[vec![0]]).is_err());
        assert!(nsis_parameters_wide(&[vec![1; 32768]]).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn review_m11_recheck_unwritable_bundle_is_refused_before_quit() {
        use std::os::unix::{ffi::OsStrExt, fs::PermissionsExt};
        let d = tempfile::tempdir().unwrap();
        let installed = d.path().join("Tezbar.app");
        std::fs::create_dir(&installed).unwrap();
        std::fs::write(installed.join("identity"), b"old").unwrap();
        std::fs::set_permissions(&installed, std::fs::Permissions::from_mode(0o555)).unwrap();
        let path = std::ffi::CString::new(installed.as_os_str().as_bytes()).unwrap();
        let allowed_by_os = unsafe { libc::access(path.as_ptr(), libc::W_OK) == 0 };
        let refused = preflight_macos_install(&installed);
        // Restore before assertions so even a failing regression cleans its own fixture.
        std::fs::set_permissions(&installed, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(refused.is_ok(), allowed_by_os);
        if !allowed_by_os {
            assert!(refused
                .unwrap_err()
                .contains("verified download has been kept"));
        }
        preflight_macos_install(&installed).unwrap();
        assert_eq!(std::fs::read(installed.join("identity")).unwrap(), b"old");
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
    }
    #[cfg(unix)]
    #[test]
    fn review_m11_successful_relaunch_removes_old_signed_bundle_directory() {
        let d = tempfile::tempdir().unwrap();
        let old = d.path().join("Tezbar.app");
        let new = d.path().join("staged.app");
        std::fs::create_dir(&old).unwrap();
        std::fs::create_dir(&new).unwrap();
        std::fs::write(old.join("identity"), b"old").unwrap();
        std::fs::write(new.join("identity"), b"new").unwrap();
        preflight_macos_install(&old).unwrap();
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 2);
        let result = swap_bundle(&new, &old).unwrap();
        let backup = result.backup.clone();
        result.confirm_relaunch().unwrap();
        assert!(!backup.exists());
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
        assert_eq!(std::fs::read(old.join("identity")).unwrap(), b"new");
        assert!(preflight_macos_install(&d.path().join("missing/Tezbar.app")).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn atomic_swap_keeps_old_bundle_for_relaunch_and_rolls_back_failures() {
        let d = tempfile::tempdir().unwrap();
        let installed = d.path().join("Tezbar.app");
        std::fs::create_dir(&installed).unwrap();
        std::fs::write(installed.join("identity"), b"old").unwrap();
        assert!(swap_bundle(&d.path().join("missing"), &installed).is_err());
        assert_eq!(std::fs::read(installed.join("identity")).unwrap(), b"old");
        let staged = d.path().join("staged.app");
        std::fs::create_dir(&staged).unwrap();
        std::fs::write(staged.join("identity"), b"new").unwrap();
        let result = swap_bundle(&staged, &installed).unwrap();
        assert_eq!(std::fs::read(installed.join("identity")).unwrap(), b"new");
        assert_eq!(
            std::fs::read(result.backup.join("Tezbar.app/identity")).unwrap(),
            b"old"
        );
        result.rollback().unwrap();
        assert_eq!(std::fs::read(installed.join("identity")).unwrap(), b"old");
    }
}

#[cfg(test)]
mod m11_tests {
    use super::*;
    fn updater(root: &Path) -> Updater {
        Updater::new(
            Config {
                current: "0.3.0-beta.7".parse().unwrap(),
                identifier: "com.tezbar.app".into(),
                data_dir: root.into(),
                cache_dir: root.into(),
                public_key: STANDARD.encode(include_bytes!("../tests/fixtures/fixture.pub")),
                targets: vec![],
            },
            Arc::new(|_| {}),
        )
    }
    #[tokio::test]
    async fn one_lazy_http_client_captured_local_download_ready_reset_and_retry() {
        use std::io::{Read, Write};
        let root = tempfile::tempdir().unwrap();
        let updater = updater(root.path());
        assert!(updater.client.get().is_none());
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut request = [0; 4096];
                        let n = stream.read(&mut request).unwrap();
                        assert!(String::from_utf8_lossy(&request[..n]).starts_with("GET /fixture "));
                        let body = include_bytes!("../tests/fixtures/fixture.tar.gz");
                        write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .unwrap();
                        stream.write_all(body).unwrap();
                        break;
                    }
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("Owned local HTTP fixture failed: {e}"),
                }
            }
        });
        let release = Release {
            version: "0.3.0-beta.8".parse().unwrap(),
            notes: String::new(),
            url: format!("http://{address}/fixture").parse().unwrap(),
            signature: STANDARD.encode(include_bytes!("../tests/fixtures/fixture.tar.gz.minisig")),
        };
        // Private test seam selects a captured release; production URL validation is unchanged.
        updater.apply_check(Ok(Some(release)), Ok(None));
        let artifact = updater.download().await.unwrap();
        server.join().unwrap();
        assert!(matches!(updater.status(), AppUpdateStatus::Ready { .. }));
        let first = updater.http().unwrap() as *const _;
        assert_eq!(first, updater.http().unwrap() as *const _);
        drop(artifact);
        updater.reset(true);
        assert!(matches!(updater.status(), AppUpdateStatus::Error { .. }));
        updater.set_channel(ReleaseChannel::Alpha).await.unwrap();
        assert_eq!(updater.status(), AppUpdateStatus::Idle);
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1); // only update-channel preference
    }
}
#[cfg(all(test, unix))]
mod team_tests {
    use super::*;
    #[test]
    fn exact_outer_team_rejects_ad_hoc_missing_different_and_duplicate_team_records() {
        assert!(pinned_team(
            "Authority=fixture\nTeamIdentifier=PIN\n",
            "PIN"
        ));
        for detail in [
            "TeamIdentifier=OTHER\n",
            "TeamIdentifier=PIN\nSignature=adhoc\n",
            "TeamIdentifier=PIN\nTeamIdentifier=OTHER\n",
            "TeamIdentifier=PIN-extra\n",
            "",
        ] {
            assert!(!pinned_team(detail, "PIN"));
        }
    }
}
