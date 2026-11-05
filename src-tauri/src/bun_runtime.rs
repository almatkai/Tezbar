//! Bun discovery and on-demand provisioning for the JavaScript extension
//! compatibility worker.
//!
//! Tezbar core services run natively in Rust. Only legacy JavaScript
//! extensions need Bun, so the runtime is provisioned lazily the first time
//! the Extension Worker is launched. The download is pinned, checksum
//! verified, and cached inside the app data directory.

use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

pub(crate) const BUN_VERSION: &str = "1.2.5";

const DOWNLOAD_ROOT: &str = "https://github.com/oven-sh/bun/releases/download";
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_DOWNLOAD_BYTES: u64 = 128 * 1024 * 1024;

/// Serializes provisioning so concurrent extension requests cannot download
/// the same archive twice.
static PROVISION: Mutex<()> = Mutex::new(());

pub(crate) fn executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "bun.exe"
    } else {
        "bun"
    }
}

pub(crate) fn platform_arch() -> Result<(&'static str, &'static str), String> {
    let platform = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        return Err(
            "Automatic JavaScript runtime installation is not supported on this platform. Install Bun from https://bun.sh and restart Tezbar."
                .to_string(),
        );
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        other => {
            return Err(format!(
                "Automatic JavaScript runtime installation is not supported on {other}. Install Bun from https://bun.sh and restart Tezbar."
            ))
        }
    };
    // Bun 1.2.5 does not publish a Windows arm64 archive.
    if platform == "windows" && arch == "aarch64" {
        return Err(
            "Automatic JavaScript runtime installation is not available for Windows on ARM. Install Bun from https://bun.sh and restart Tezbar."
                .to_string(),
        );
    }
    Ok((platform, arch))
}

pub(crate) fn archive_name(platform: &str, arch: &str) -> String {
    format!("bun-{platform}-{arch}.zip")
}

pub(crate) fn download_url(platform: &str, arch: &str) -> String {
    format!(
        "{DOWNLOAD_ROOT}/bun-v{BUN_VERSION}/{}",
        archive_name(platform, arch)
    )
}

fn checksum_url() -> String {
    format!("{DOWNLOAD_ROOT}/bun-v{BUN_VERSION}/SHASUMS256.txt")
}

pub(crate) fn profile_bun() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .map(|home| home.join(".bun").join("bin").join(executable_name()))
}

fn common_bun_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    #[cfg(target_os = "macos")]
    {
        paths.push(PathBuf::from("/opt/homebrew/bin").join(executable_name()));
        paths.push(PathBuf::from("/usr/local/bin").join(executable_name()));
    }
    #[cfg(target_os = "linux")]
    {
        paths.push(PathBuf::from("/usr/local/bin").join(executable_name()));
        paths.push(PathBuf::from("/usr/bin").join(executable_name()));
    }
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        paths.push(
            PathBuf::from(home)
                .join(".local")
                .join("bin")
                .join(executable_name()),
        );
    }
    paths
}

fn path_bun() -> Option<PathBuf> {
    // Only accept an executable that actually runs.
    Command::new(executable_name())
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()
        .filter(|status| status.success())
        .map(|_| PathBuf::from(executable_name()))
}

pub(crate) fn cached_bun(data_dir: &Path) -> Option<PathBuf> {
    let cached = data_dir.join("bun").join(executable_name());
    cached.is_file().then_some(cached)
}

fn bun_runs(path: &Path) -> bool {
    Command::new(path)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Find an already available Bun without downloading anything.
pub(crate) fn locate_bun(data_dir: &Path) -> Result<PathBuf, String> {
    if let Some(cached) = cached_bun(data_dir) {
        return Ok(cached);
    }
    if let Some(user_bun) = profile_bun().filter(|path| path.is_file()) {
        return Ok(user_bun);
    }
    if let Some(common) = common_bun_paths().into_iter().find(|path| path.is_file()) {
        return Ok(common);
    }
    if let Some(path_bun) = path_bun() {
        return Ok(path_bun);
    }
    Err(
        "Bun runtime is not installed on this system. Tezbar core features and Native V2 extensions run natively in Rust without Bun."
            .to_string(),
    )
}

/// Return a usable Bun, installing a pinned copy into the app data directory
/// when no local runtime is available.
pub(crate) fn ensure_bun(
    data_dir: &Path,
    progress: &dyn Fn(&str, Option<u64>),
) -> Result<PathBuf, String> {
    if let Ok(path) = locate_bun(data_dir) {
        if bun_runs(&path) {
            return Ok(path);
        }
    }

    let _guard = PROVISION
        .lock()
        .map_err(|_| "JavaScript runtime installer lock is poisoned".to_string())?;
    // Another request may have finished provisioning while this one waited.
    if let Ok(path) = locate_bun(data_dir) {
        if bun_runs(&path) {
            return Ok(path);
        }
    }

    log::info!("No usable local Bun runtime found; installing Bun v{BUN_VERSION} into app data");
    install_bun(data_dir, progress)
}

fn install_bun(data_dir: &Path, progress: &dyn Fn(&str, Option<u64>)) -> Result<PathBuf, String> {
    let (platform, arch) = platform_arch()?;
    let archive = archive_name(platform, arch);
    let url = download_url(platform, arch);
    let directory = data_dir.join("bun");
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not create the JavaScript runtime directory: {error}"))?;

    let client = reqwest::blocking::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .user_agent(format!("Tezbar/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("Could not prepare the JavaScript runtime download: {error}"))?;

    // Fetch the checksum first: a download without integrity verification is
    // not acceptable for an executable that will run extension code.
    let expected = fetch_checksum(&client, &archive)?;

    progress("runtime-downloading", Some(0));
    let mut response = client
        .get(&url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| {
            format!(
                "Could not download the JavaScript runtime: {error}. Check your connection or install Bun from https://bun.sh and restart Tezbar."
            )
        })?;

    let total = response.content_length();
    if total.is_some_and(|length| length > MAX_DOWNLOAD_BYTES) {
        return Err("The JavaScript runtime download is unexpectedly large; refusing it".into());
    }

    let mut archive_file = tempfile::Builder::new()
        .prefix(".bun-download-")
        .suffix(".zip")
        .tempfile_in(&directory)
        .map_err(|error| format!("Could not create a temporary download file: {error}"))?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|error| format!("Could not read the JavaScript runtime download: {error}"))?;
        if read == 0 {
            break;
        }
        downloaded += read as u64;
        if downloaded > MAX_DOWNLOAD_BYTES {
            return Err("The JavaScript runtime download exceeded the size limit".into());
        }
        hasher.update(&buffer[..read]);
        archive_file
            .write_all(&buffer[..read])
            .map_err(|error| format!("Could not save the JavaScript runtime download: {error}"))?;
        if let Some(total) = total.filter(|total| *total > 0) {
            let percent = downloaded.saturating_mul(100) / total;
            progress("runtime-downloading", Some(percent.min(100)));
        }
    }
    archive_file
        .flush()
        .map_err(|error| format!("Could not finish the JavaScript runtime download: {error}"))?;

    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        return Err(format!(
            "The downloaded JavaScript runtime failed checksum verification (expected {expected}, got {actual}). Try again or install Bun from https://bun.sh and restart Tezbar."
        ));
    }

    progress("runtime-installing", None);
    let executable = extract_executable(archive_file.path(), &directory)?;
    let version = Command::new(&executable)
        .arg("--version")
        .output()
        .map_err(|error| format!("The downloaded JavaScript runtime could not run: {error}"))?;
    if !version.status.success() {
        let _ = std::fs::remove_file(&executable);
        return Err(format!(
            "The downloaded JavaScript runtime failed to start: {}. Install Bun from https://bun.sh and restart Tezbar.",
            String::from_utf8_lossy(&version.stderr).trim()
        ));
    }

    progress("runtime-ready", None);
    Ok(executable)
}

fn fetch_checksum(client: &reqwest::blocking::Client, archive: &str) -> Result<String, String> {
    let body = client
        .get(checksum_url())
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .and_then(reqwest::blocking::Response::text)
        .map_err(|error| {
            format!(
                "Could not download the JavaScript runtime checksum: {error}. Check your connection or install Bun from https://bun.sh and restart Tezbar."
            )
        })?;
    parse_checksum(&body, archive)
        .ok_or_else(|| format!("The JavaScript runtime checksum list did not contain {archive}"))
}

fn parse_checksum(body: &str, archive: &str) -> Option<String> {
    body.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let digest = fields.next()?;
        let name = fields.next()?.trim_start_matches('*');
        (name == archive
            && digest.len() == 64
            && digest
                .chars()
                .all(|character| character.is_ascii_hexdigit()))
        .then(|| digest.to_ascii_lowercase())
    })
}

fn extract_executable(archive_path: &Path, directory: &Path) -> Result<PathBuf, String> {
    let file = std::fs::File::open(archive_path)
        .map_err(|error| format!("Could not open the JavaScript runtime archive: {error}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("Invalid JavaScript runtime archive: {error}"))?;

    let name = executable_name();
    let mut member = None;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|error| {
            format!("Could not inspect the JavaScript runtime archive: {error}")
        })?;
        if entry.is_dir() {
            continue;
        }
        let file_name = Path::new(entry.name())
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if file_name == name {
            member = Some(entry.name().to_string());
            break;
        }
    }
    let member = member.ok_or_else(|| {
        "The downloaded JavaScript runtime archive did not contain a Bun executable".to_string()
    })?;

    let mut entry = archive
        .by_name(&member)
        .map_err(|error| format!("Could not read the Bun executable from the archive: {error}"))?;
    let temporary = directory.join(format!(".{name}.download-{}", std::process::id()));
    {
        let mut output = std::fs::File::create(&temporary)
            .map_err(|error| format!("Could not create the Bun executable: {error}"))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|error| format!("Could not extract the Bun executable: {error}"))?;
        output
            .flush()
            .map_err(|error| format!("Could not finish the Bun executable: {error}"))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o755))
            .map_err(|error| format!("Could not make the Bun executable runnable: {error}"))?;
    }

    let destination = directory.join(name);
    // Windows cannot rename over an existing file, and a stale/corrupt cache
    // entry is exactly why provisioning was started.
    let _ = std::fs::remove_file(&destination);
    std::fs::rename(&temporary, &destination)
        .map_err(|error| format!("Could not install the Bun executable: {error}"))?;
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_parser_matches_only_the_requested_archive() {
        let body = "55e6520a172e22a37d4822f23afde90b26f36880ce3eaad348699418dbf788bb  bun-darwin-aarch64.zip\n\
                    969f03626233168f3cdc17e9af9e7c9de32200073831fc53ab9416035de7ea61  bun-darwin-x64.zip\n";
        assert_eq!(
            parse_checksum(body, "bun-darwin-aarch64.zip").as_deref(),
            Some("55e6520a172e22a37d4822f23afde90b26f36880ce3eaad348699418dbf788bb")
        );
        assert_eq!(parse_checksum(body, "bun-windows-x64.zip"), None);
    }

    #[test]
    fn download_url_is_pinned() {
        assert_eq!(
            download_url("darwin", "aarch64"),
            "https://github.com/oven-sh/bun/releases/download/bun-v1.2.5/bun-darwin-aarch64.zip"
        );
    }

    #[test]
    fn cached_bun_is_found_without_a_download() {
        let directory = tempfile::tempdir().unwrap();
        let cache = directory.path().join("bun");
        std::fs::create_dir_all(&cache).unwrap();
        let executable = cache.join(executable_name());
        std::fs::write(&executable, b"not-a-real-runtime").unwrap();
        assert_eq!(cached_bun(directory.path()), Some(executable));
    }

    #[test]
    #[ignore = "downloads Bun from GitHub"]
    fn installs_and_runs_pinned_bun() {
        let directory = tempfile::tempdir().unwrap();
        let executable = install_bun(directory.path(), &|phase, percent| {
            eprintln!("{phase} {percent:?}");
        })
        .unwrap();
        assert!(executable.is_file());
        assert!(bun_runs(&executable));
    }
}
