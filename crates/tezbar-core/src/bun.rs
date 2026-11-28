//! Bun discovery and on-demand provisioning for the JavaScript extension worker.
//!
//! Tezbar core services run natively in Rust; only legacy JavaScript extensions need
//! Bun. When no local runtime exists, a pinned, checksum-verified copy is installed into
//! the app data directory the first time the worker launches (beta.8 behaviour).
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

pub const BUN_VERSION: &str = "1.2.5";
const DOWNLOAD_ROOT: &str = "https://github.com/oven-sh/bun/releases/download";
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_DOWNLOAD_BYTES: u64 = 128 * 1024 * 1024;
const MANUAL: &str = "install Bun from https://bun.sh and restart Tezbar";

/// Serializes provisioning so concurrent extension requests cannot download twice.
static PROVISION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub fn bun_executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "bun.exe"
    } else {
        "bun"
    }
}

fn bun_in_user_profile() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .map(|home| home.join(".bun").join("bin").join(bun_executable_name()))
}

fn common_bun_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    #[cfg(target_os = "macos")]
    {
        paths.push(PathBuf::from("/opt/homebrew/bin").join(bun_executable_name()));
        paths.push(PathBuf::from("/usr/local/bin").join(bun_executable_name()));
    }
    #[cfg(target_os = "linux")]
    {
        paths.push(PathBuf::from("/usr/local/bin").join(bun_executable_name()));
        paths.push(PathBuf::from("/usr/bin").join(bun_executable_name()));
    }
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        paths.push(PathBuf::from(home).join(".local").join("bin").join(bun_executable_name()));
    }
    paths
}

fn bun_runs(path: &Path) -> bool {
    let mut command = Command::new(path);
    command.arg("--version").stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.status().is_ok_and(|status| status.success())
}

/// Find an already available Bun without downloading anything.
pub fn locate_bun(app_local_data: &Path) -> Result<PathBuf, String> {
    let cached = app_local_data.join("bun").join(bun_executable_name());
    if cached.is_file() {
        return Ok(cached);
    }
    if let Some(user_bun) = bun_in_user_profile().filter(|path| path.is_file()) {
        return Ok(user_bun);
    }
    if let Some(common) = common_bun_paths().into_iter().find(|path| path.is_file()) {
        return Ok(common);
    }
    if bun_runs(Path::new(bun_executable_name())) {
        return Ok(PathBuf::from(bun_executable_name()));
    }
    Err("Bun runtime is not installed on this system. Tezbar core features and Native V2 extensions run natively in Rust without Bun.".to_string())
}

/// A usable Bun, installing the pinned copy into app data when none runs locally.
pub async fn ensure_bun(
    data_dir: &Path,
    progress: impl Fn(&str, Option<u64>) + Send + Sync,
) -> Result<PathBuf, String> {
    let usable = |dir: PathBuf| async move {
        tokio::task::spawn_blocking(move || locate_bun(&dir).ok().filter(|p| bun_runs(p)))
            .await
            .ok()
            .flatten()
    };
    if let Some(path) = usable(data_dir.to_path_buf()).await {
        return Ok(path);
    }
    let _guard = PROVISION.lock().await;
    // Another request may have finished provisioning while this one waited.
    if let Some(path) = usable(data_dir.to_path_buf()).await {
        return Ok(path);
    }
    log::info!("No usable local Bun runtime found; installing Bun v{BUN_VERSION} into app data");
    install_bun(data_dir, &progress).await
}

fn platform_arch() -> Result<(&'static str, &'static str), String> {
    let platform = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        return Err(format!("Automatic JavaScript runtime installation is not supported on this platform; {MANUAL}."));
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        other => return Err(format!("Automatic JavaScript runtime installation is not supported on {other}; {MANUAL}.")),
    };
    // Bun 1.2.5 publishes no Windows arm64 archive.
    if platform == "windows" && arch == "aarch64" {
        return Err(format!("Automatic JavaScript runtime installation is not available for Windows on ARM; {MANUAL}."));
    }
    Ok((platform, arch))
}
fn archive_name(platform: &str, arch: &str) -> String {
    format!("bun-{platform}-{arch}.zip")
}
fn download_url(platform: &str, arch: &str) -> String {
    format!("{DOWNLOAD_ROOT}/bun-v{BUN_VERSION}/{}", archive_name(platform, arch))
}
fn checksum_url() -> String {
    format!("{DOWNLOAD_ROOT}/bun-v{BUN_VERSION}/SHASUMS256.txt")
}
fn parse_checksum(body: &str, archive: &str) -> Option<String> {
    body.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let digest = fields.next()?;
        let name = fields.next()?.trim_start_matches('*');
        (name == archive && digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| digest.to_ascii_lowercase())
    })
}

async fn install_bun(data_dir: &Path, progress: &(impl Fn(&str, Option<u64>) + Sync)) -> Result<PathBuf, String> {
    let (platform, arch) = platform_arch()?;
    let archive = archive_name(platform, arch);
    let directory = data_dir.join("bun");
    std::fs::create_dir_all(&directory)
        .map_err(|e| format!("Could not create the JavaScript runtime directory: {e}"))?;
    let client = reqwest::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .user_agent(format!("Tezbar/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("Could not prepare the JavaScript runtime download: {e}"))?;
    // The checksum comes first: an executable that runs extension code is never unverified.
    let checksums = client
        .get(checksum_url())
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("Could not download the JavaScript runtime checksum: {e}. Check your connection or {MANUAL}."))?
        .text()
        .await
        .map_err(|e| format!("Could not download the JavaScript runtime checksum: {e}"))?;
    let expected = parse_checksum(&checksums, &archive)
        .ok_or_else(|| format!("The JavaScript runtime checksum list did not contain {archive}"))?;

    progress("runtime-downloading", Some(0));
    let mut response = client
        .get(download_url(platform, arch))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("Could not download the JavaScript runtime: {e}. Check your connection or {MANUAL}."))?;
    let total = response.content_length();
    if total.is_some_and(|length| length > MAX_DOWNLOAD_BYTES) {
        return Err("The JavaScript runtime download is unexpectedly large; refusing it".into());
    }
    let mut file = tempfile::Builder::new()
        .prefix(".bun-download-")
        .suffix(".zip")
        .tempfile_in(&directory)
        .map_err(|e| format!("Could not create a temporary download file: {e}"))?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Could not read the JavaScript runtime download: {e}"))?
    {
        downloaded += chunk.len() as u64;
        if downloaded > MAX_DOWNLOAD_BYTES {
            return Err("The JavaScript runtime download exceeded the size limit".into());
        }
        hasher.update(&chunk);
        file.write_all(&chunk)
            .map_err(|e| format!("Could not save the JavaScript runtime download: {e}"))?;
        if let Some(total) = total.filter(|t| *t > 0) {
            progress("runtime-downloading", Some((downloaded.saturating_mul(100) / total).min(100)));
        }
    }
    file.flush().map_err(|e| format!("Could not finish the JavaScript runtime download: {e}"))?;
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        return Err(format!(
            "The downloaded JavaScript runtime failed checksum verification (expected {expected}, got {actual}). Try again or {MANUAL}."
        ));
    }

    progress("runtime-installing", None);
    let (archive_path, target) = (file.path().to_path_buf(), directory.clone());
    let executable = tokio::task::spawn_blocking(move || extract_executable(&archive_path, &target))
        .await
        .map_err(|e| e.to_string())??;
    drop(file);
    let check = executable.clone();
    if !tokio::task::spawn_blocking(move || bun_runs(&check)).await.unwrap_or(false) {
        let _ = std::fs::remove_file(&executable);
        return Err(format!("The downloaded JavaScript runtime failed to start; {MANUAL}."));
    }
    progress("runtime-ready", None);
    Ok(executable)
}

fn extract_executable(archive_path: &Path, directory: &Path) -> Result<PathBuf, String> {
    let file = std::fs::File::open(archive_path)
        .map_err(|e| format!("Could not open the JavaScript runtime archive: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Invalid JavaScript runtime archive: {e}"))?;
    let name = bun_executable_name();
    let member = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().filter(|e| !e.is_dir()).map(|e| e.name().to_owned()))
        .find(|entry| Path::new(entry).file_name().and_then(|f| f.to_str()) == Some(name))
        .ok_or("The downloaded JavaScript runtime archive did not contain a Bun executable")?;
    let mut entry = archive
        .by_name(&member)
        .map_err(|e| format!("Could not read the Bun executable from the archive: {e}"))?;
    let temporary = directory.join(format!(".{name}.download-{}", std::process::id()));
    {
        let mut output = std::fs::File::create(&temporary).map_err(|e| format!("Could not create the Bun executable: {e}"))?;
        std::io::copy(&mut entry, &mut output).map_err(|e| format!("Could not extract the Bun executable: {e}"))?;
        output.flush().map_err(|e| format!("Could not finish the Bun executable: {e}"))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Could not make the Bun executable runnable: {e}"))?;
    }
    let destination = directory.join(name);
    // Windows cannot rename over a file, and a stale cache entry is why we are here.
    let _ = std::fs::remove_file(&destination);
    std::fs::rename(&temporary, &destination).map_err(|e| format!("Could not install the Bun executable: {e}"))?;
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
            download_url("windows", "x64"),
            "https://github.com/oven-sh/bun/releases/download/bun-v1.2.5/bun-windows-x64.zip"
        );
    }
    #[test]
    fn extracts_only_the_bun_executable() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("bun.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("bun-windows-x64/README.md", options).unwrap();
        zip.write_all(b"readme").unwrap();
        zip.start_file(format!("bun-windows-x64/{}", bun_executable_name()), options).unwrap();
        zip.write_all(b"binary").unwrap();
        zip.finish().unwrap();
        let installed = extract_executable(&archive, dir.path()).unwrap();
        assert_eq!(installed, dir.path().join(bun_executable_name()));
        assert_eq!(std::fs::read(installed).unwrap(), b"binary");
    }
    #[tokio::test]
    #[ignore = "downloads Bun from GitHub"]
    async fn installs_and_runs_pinned_bun() {
        let dir = tempfile::tempdir().unwrap();
        let executable = install_bun(dir.path(), &|phase: &str, percent: Option<u64>| eprintln!("{phase} {percent:?}"))
            .await
            .unwrap();
        assert!(bun_runs(&executable));
    }
}
