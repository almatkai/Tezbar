//! Verified schema-3 binary packages. No scripts, package managers or build tools.
use crate::native_core::common::{read_json, write_json_atomic, CoreContext, CoreResult};
use futures_util::StreamExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) const MAX_ARCHIVE: u64 = 128 * 1024 * 1024;
const MAX_EXPANDED: u64 = 512 * 1024 * 1024;
const MAX_FILES: usize = 4096;
const CAPABILITY_GROUPS: &[&str] = &[
    "clipboard",
    "shell",
    "selection",
    "window",
    "dialog",
    "screen",
    "apps",
    "settings",
    "ai",
    "storage",
    "background",
    "tray",
    "notifications",
    "safety",
    "launcher",
];

/// Closed service allowlist shared by manifest validation and runtime grants.
pub(super) const METHODS: &[&str] = &[
    "host.clipboard.read",
    "host.clipboard.write",
    "host.clipboard.writeImage",
    "host.clipboard.writeFiles",
    "host.clipboard.paste",
    "host.clipboard.pasteText",
    "host.shell.open",
    "host.shell.openWithApp",
    "host.shell.reveal",
    "host.selection.read",
    "host.window.visibility",
    "host.window.suppressBlur",
    "host.dialog.confirm",
    "host.screen.capture",
    "host.apps.list",
    "host.settings.open",
    "host.ai.ask",
    "host.background.retain",
    "host.background.release",
    "host.tray.set",
    "host.tray.remove",
    "host.notifications.show",
    "host.notifications.remove",
    "host.safety.authorize",
    "host.safety.record",
    "host.launcher.publish",
];
fn valid_capability(cap: &str) -> bool {
    CAPABILITY_GROUPS.contains(&cap)
        || METHODS
            .iter()
            .any(|m| *m == cap || m.strip_prefix("host.") == Some(cap))
}
// Shared by installer, uninstall, and startup bookkeeping across UI/CLI callers.
pub(super) static PACKAGE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Cross-process metadata serialization also covers a CLI import while the UI is
/// running. The OS releases this lease after an interrupted installer.
pub(super) fn metadata_lock(ctx: &CoreContext) -> Result<std::fs::File, String> {
    let directory = ctx.paths.data_dir.join("extensions");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    if std::fs::symlink_metadata(&directory)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Extensions directory symlink rejected".into());
    }
    let path = directory.join(".package-metadata.lock");
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    let file = {
        #[cfg(windows)]
        {
            let started = std::time::Instant::now();
            loop {
                match options.open(&path) {
                    Ok(f) => break f,
                    Err(e)
                        if e.raw_os_error() == Some(32)
                            && started.elapsed() < Duration::from_secs(5) =>
                    {
                        std::thread::sleep(Duration::from_millis(20))
                    }
                    Err(e) => return Err(e.to_string()),
                }
            }
        }
        #[cfg(not(windows))]
        {
            options.open(path).map_err(|e| e.to_string())?
        }
    };
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let started = std::time::Instant::now();
        loop {
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                break;
            }
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::WouldBlock
                && started.elapsed() < Duration::from_secs(5)
            {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
            if error.kind() != std::io::ErrorKind::Interrupted {
                return Err(format!("Package metadata lock unavailable: {error}"));
            }
        }
    }
    Ok(file)
}

pub(super) fn target() -> &'static str {
    target_for(std::env::consts::OS, std::env::consts::ARCH)
}
fn target_for(os: &str, arch: &str) -> &'static str {
    match (os, arch) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        _ => "unsupported",
    }
}
pub(super) fn component(s: &str) -> Result<(), String> {
    if s.is_empty()
        || s.len() > 200
        || !s.as_bytes()[0].is_ascii_alphanumeric()
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        return Err("Invalid package component".into());
    }
    Ok(())
}
pub(super) fn relative(s: &str) -> Result<PathBuf, String> {
    // Reject cross-platform path syntax even when verifying on Unix.
    if s.is_empty()
        || s.len() > 1024
        || s.contains(['\\', ':', '\0'])
        || s.chars().any(|c| c.is_control())
        || s.starts_with('/')
        || s.split('/')
            .any(|p| p.is_empty() || p == "." || p == ".." || p.ends_with([' ', '.']))
    {
        return Err(format!("Unsafe package path: {s}"));
    }
    for part in s.split('/') {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        if [
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ]
        .contains(&stem.as_str())
        {
            return Err("Reserved package filename".into());
        }
    }
    Ok(PathBuf::from(s))
}
fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
pub(super) fn validate(manifest: &Value) -> Result<(), String> {
    if manifest["schemaVersion"] != 3
        || manifest["runtime"] != "rust"
        || manifest["execution"] != "process"
        || manifest["protocolVersion"] != 1
    {
        return Err("Extension needs a schema-3 process package (protocol 1); schema-2 markers need upgrade".into());
    }
    let slug = manifest["slug"].as_str().ok_or("Missing slug")?;
    component(slug)?;
    if manifest["id"] != format!("com.tezbar.native.{slug}") {
        return Err("Package identity mismatch".into());
    }
    let version = manifest["version"].as_str().ok_or("Missing version")?;
    component(version)?;
    if let Some(min) = manifest["minHostVersion"].as_str() {
        let min = semver::Version::parse(min).map_err(|_| "Invalid minimum host version")?;
        let host = semver::Version::parse(env!("CARGO_PKG_VERSION")).map_err(|e| e.to_string())?;
        if host < min {
            return Err("Package requires a newer Tezbar host".into());
        }
    }
    let caps = manifest["capabilities"]
        .as_array()
        .ok_or("Missing capabilities")?;
    if caps
        .iter()
        .any(|c| !c.as_str().is_some_and(|s| valid_capability(s)))
    {
        return Err("Unknown capability".into());
    }
    let commands = manifest["commands"].as_array().ok_or("Missing commands")?;
    let mut names = HashSet::new();
    for c in commands {
        let name = c["name"].as_str().ok_or("Missing command name")?;
        component(name)?;
        if !names.insert(name) {
            return Err("Duplicate command".into());
        }
        if c.get("concurrency")
            .is_some_and(|v| v != "serialize" && v != "reject-while-busy")
        {
            return Err("Invalid concurrency policy".into());
        }
        if c.get("timeoutMs")
            .is_some_and(|v| !v.as_u64().is_some_and(|n| (1..=300000).contains(&n)))
        {
            return Err("Invalid command deadline".into());
        }
        if c.get("refreshMs")
            .is_some_and(|v| !v.is_null() && !v.as_u64().is_some_and(|n| n >= 500))
        {
            return Err("Invalid refresh interval".into());
        }
    }
    super::launcher::validate(manifest)?;
    if let Some(paths) = manifest.get("migrationPaths") {
        let paths = paths.as_object().ok_or("Invalid migration paths")?;
        if paths.len() > 32 {
            return Err("Too many migration paths".into());
        }
        for (key, path) in paths {
            component(key)?;
            relative(path.as_str().ok_or("Invalid migration path")?)?;
        }
    }
    if let Some(probe) = manifest.get("startupProbe") {
        component(probe["key"].as_str().ok_or("Startup probe requires key")?)?;
        let paths = probe["migrationPaths"]
            .as_array()
            .ok_or("Startup probe requires migration path keys")?;
        if paths.is_empty()
            || paths.iter().any(|p| {
                p.as_str()
                    .is_none_or(|k| manifest["migrationPaths"].get(k).is_none())
            })
        {
            return Err("Startup probe references undeclared migration path".into());
        }
        if !caps
            .iter()
            .any(|v| v == "background" || v == "background.retain" || v == "host.background.retain")
        {
            return Err("Startup probe requires background capability".into());
        }
    }
    Ok(())
}
pub(super) fn artifact(manifest: &Value) -> Result<&Value, String> {
    validate(manifest)?;
    if target() == "unsupported" {
        return Err("Unsupported extension target architecture".into());
    }
    artifact_for(manifest, target())
}
fn artifact_for<'a>(manifest: &'a Value, target: &str) -> Result<&'a Value, String> {
    let a = manifest["artifacts"]
        .get(target)
        .ok_or("No verified release artifact for this target; publication/pins required")?;
    if !a["sha256"].as_str().is_some_and(digest)
        || !a["executableSha256"].as_str().is_some_and(digest)
    {
        return Err("Invalid artifact digest".into());
    }
    let exe = a["executable"].as_str().ok_or("Missing executable")?;
    relative(exe)?;
    if !exe.starts_with("bin/") {
        return Err("Executable must be inside bin/".into());
    }
    Ok(a)
}
pub(super) fn root(ctx: &CoreContext, id: &str) -> Result<PathBuf, String> {
    component(id)?;
    let root = ctx.paths.data_dir.join("extensions/packages").join(id);
    // Data-root itself is user configured. Managed descendants may never redirect
    // installation/launch to a different tree via symlinks.
    let mut path = ctx.paths.data_dir.clone();
    for part in ["extensions", "packages", id, "versions"] {
        path.push(part);
        if let Ok(meta) = std::fs::symlink_metadata(&path) {
            if meta.file_type().is_symlink() || !meta.is_dir() {
                return Err("Managed package directory is not a regular directory".into());
            }
        }
    }
    Ok(root)
}
fn hash_file(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut b = [0u8; 65536];
    loop {
        let n = f.read(&mut b).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}
fn regular(root: &Path, name: &str) -> Result<PathBuf, String> {
    let rel = relative(name)?;
    let mut path = root.to_path_buf();
    for part in rel.components() {
        path.push(part);
        let m = std::fs::symlink_metadata(&path)
            .map_err(|e| format!("Missing package file {name}: {e}"))?;
        if m.file_type().is_symlink() {
            return Err("Package symlink rejected".into());
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if std::fs::metadata(&path).map_err(|e| e.to_string())?.nlink() != 1 {
            return Err("Package hardlink rejected".into());
        }
    }
    if !path.is_file() {
        return Err("Package member is not a regular file".into());
    }
    Ok(path)
}
fn verify_files(dir: &Path, m: &Value, a: &Value) -> Result<PathBuf, String> {
    let files = m["files"]
        .as_object()
        .ok_or("Package requires file SHA256 inventory")?;
    if files.len() > MAX_FILES {
        return Err("Package inventory too large".into());
    }
    for (name, hash) in files {
        let path = regular(dir, name)?;
        if !hash.as_str().is_some_and(digest) || hash_file(&path)? != hash.as_str().unwrap_or("") {
            return Err(format!("Tampered package file: {name}"));
        }
    }
    let name = a["executable"].as_str().ok_or("Missing executable")?;
    if files.get(name) != Some(&a["executableSha256"]) {
        return Err("Executable missing from inventory".into());
    }
    let path = regular(dir, name)?;
    if hash_file(&path)? != a["executableSha256"].as_str().unwrap_or("") {
        return Err("Executable digest mismatch".into());
    }
    Ok(path)
}
#[derive(Clone)]
pub(super) struct Package {
    pub manifest: Value,
    pub dir: PathBuf,
    pub executable: PathBuf,
}
pub(super) fn resolve(ctx: &CoreContext, id: &str) -> Result<Package, String> {
    let root = root(ctx, id)?;
    let active = read_json(&root.join("active.json")).map_err(|_| {
        "Binary package not installed; existing schema-2 package needs upgrade".to_string()
    })?;
    let version = active["directory"]
        .as_str()
        .ok_or("Invalid active package pointer")?;
    component(version)?;
    let dir = root.join("versions").join(version);
    if std::fs::symlink_metadata(&dir)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Package directory symlink rejected".into());
    }
    let manifest = read_json(&regular(&dir, "extension.json")?)?;
    if manifest["id"] != id || active["manifestSha256"] != hash_file(&dir.join("extension.json"))? {
        return Err("Active package identity/digest mismatch".into());
    }
    let a = artifact(&manifest)?;
    let executable = verify_files(&dir, &manifest, a)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(&executable)
            .map_err(|e| e.to_string())?
            .permissions()
            .mode()
            & 0o111
            == 0
        {
            return Err("Package executable is not executable".into());
        }
    }
    for entry in walkdir::WalkDir::new(&dir) {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().is_symlink() {
            return Err("Package symlink rejected".into());
        }
        if entry.file_type().is_file() {
            let name = entry
                .path()
                .strip_prefix(&dir)
                .map_err(|e| e.to_string())?
                .to_str()
                .ok_or("Non-UTF8 file")?
                .replace('\\', "/");
            if name != "extension.json" && manifest["files"].get(&name).is_none() {
                return Err("Uninventoried package file".into());
            }
        }
    }
    Ok(Package {
        manifest,
        dir,
        executable,
    })
}

/// Trusted catalog manifest, or an explicitly user-selected local manifest + ZIP.
/// Local import grants the selected publisher code execution; it is not a sandbox.
pub(super) async fn install(
    ctx: &CoreContext,
    manifest: Value,
    local: Option<PathBuf>,
) -> CoreResult {
    install_inner(ctx, manifest, local, false).await
}
async fn install_inner(
    ctx: &CoreContext,
    manifest: Value,
    local: Option<PathBuf>,
    test_loopback: bool,
) -> CoreResult {
    let rule = super::acquisition::for_id(manifest["id"].as_str().unwrap_or(""));
    install_with_rule(ctx, manifest, local, test_loopback, rule).await
}
async fn install_with_rule(
    ctx: &CoreContext,
    manifest: Value,
    local: Option<PathBuf>,
    test_loopback: bool,
    rule: Result<super::acquisition::Rule, String>,
) -> CoreResult {
    if local.is_none() {
        super::acquisition::require_remote(rule)?;
    }
    let a = artifact(&manifest)?.clone();
    let bytes = if let Some(path) = local {
        if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > MAX_ARCHIVE {
            return Err("Archive exceeds size limit".into());
        }
        use tokio::io::AsyncReadExt;
        let mut bytes = Vec::new();
        tokio::fs::File::open(path)
            .await
            .map_err(|e| e.to_string())?
            .take(MAX_ARCHIVE + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_ARCHIVE {
            return Err("Archive exceeds size limit".into());
        }
        bytes
    } else {
        let url = reqwest::Url::parse(a["url"].as_str().ok_or("Missing artifact URL")?)
            .map_err(|e| e.to_string())?;
        let loopback = cfg!(test)
            && test_loopback
            && url.scheme() == "http"
            && url
                .host_str()
                .and_then(|h| h.parse::<std::net::IpAddr>().ok())
                .is_some_and(|ip| ip.is_loopback());
        if (url.scheme() != "https" && !loopback)
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("Artifact URL must be credential-free HTTPS".into());
        }
        let client = reqwest::Client::builder()
            .https_only(!loopback)
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= 5 {
                    return attempt.error("Too many artifact redirects");
                }
                let next = attempt.url();
                if !next.username().is_empty()
                    || next.password().is_some()
                    || (!loopback && next.scheme() != "https")
                {
                    attempt.error("Unsafe artifact redirect")
                } else {
                    attempt.follow()
                }
            }))
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?;
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| {
                format!(
                    "Artifact download failed (private repositories require local ZIP import): {e}"
                )
            })?;
        if response.content_length().is_some_and(|n| n > MAX_ARCHIVE) {
            return Err("Archive exceeds size limit".into());
        }
        let mut data = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| e.to_string())?;
            if data.len() as u64 + chunk.len() as u64 > MAX_ARCHIVE {
                return Err("Archive exceeds size limit".into());
            }
            data.extend_from_slice(&chunk);
        }
        data
    };
    let ctx = ctx.clone();
    tokio::task::spawn_blocking(move || install_bytes(&ctx, &manifest, &a, &bytes))
        .await
        .map_err(|e| e.to_string())?
}
fn extract(bytes: &[u8], stage: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("Invalid ZIP: {e}"))?;
    if zip.len() > MAX_FILES {
        return Err("Too many archive entries".into());
    }
    let mut seen = HashSet::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().trim_end_matches('/').to_string();
        let rel = relative(&name)?;
        if !seen.insert(name.to_lowercase()) {
            return Err("Duplicate/case-colliding ZIP member".into());
        }
        let mode = entry.unix_mode().unwrap_or(0);
        let kind = mode & 0o170000;
        if kind != 0 && kind != 0o100000 && kind != 0o040000 {
            return Err("ZIP links/devices are forbidden".into());
        }
        total = total
            .checked_add(entry.size())
            .ok_or("Archive expanded size overflow")?;
        if total > MAX_EXPANDED
            || (entry.size() > 1024 * 1024 && entry.size() / entry.compressed_size().max(1) > 1000)
        {
            return Err("Archive expansion limit".into());
        }
        let path = stage.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
            continue;
        }
        std::fs::create_dir_all(path.parent().ok_or("Missing ZIP parent")?)
            .map_err(|e| e.to_string())?;
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        let copied = std::io::copy(&mut (&mut entry).take(MAX_EXPANDED + 1), &mut out)
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if mode & 0o111 != 0 {
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                    .map_err(|e| e.to_string())?;
            }
        }
        if copied != entry.size() {
            return Err("ZIP member size mismatch".into());
        }
        out.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn install_bytes(ctx: &CoreContext, m: &Value, a: &Value, bytes: &[u8]) -> CoreResult {
    if bytes.len() as u64 > MAX_ARCHIVE
        || format!("{:x}", Sha256::digest(bytes)) != a["sha256"].as_str().unwrap_or("")
    {
        return Err("Archive SHA256 mismatch".into());
    }
    validate(m)?;
    let _guard = PACKAGE_LOCK.lock().map_err(|e| e.to_string())?;
    let id = m["id"].as_str().ok_or("Missing id")?;
    let root = root(ctx, id)?;
    let _metadata_lease = metadata_lock(ctx)?;
    std::fs::create_dir_all(root.join("versions")).map_err(|e| e.to_string())?;
    let stage = tempfile::Builder::new()
        .prefix(".stage-")
        .tempdir_in(root.join("versions"))
        .map_err(|e| e.to_string())?;
    extract(bytes, stage.path())?;
    let packaged = read_json(&stage.path().join("extension.json"))?;
    // A combined release reference may omit files or leave it empty because each target ZIP
    // contains a different executable. The archive SHA has ALREADY been verified;
    // derive its exact inventory, while matching every identity/policy field.
    // An explicit trusted inventory is stricter and must match byte-for-byte.
    let mut resolved = m.clone();
    let empty = json!({});
    let trusted_files = m
        .get("files")
        .unwrap_or(&empty)
        .as_object()
        .ok_or("Trusted manifest files must be an object")?;
    if trusted_files.is_empty() {
        let inventory = packaged["files"]
            .as_object()
            .filter(|v| !v.is_empty())
            .ok_or("Archive must contain a complete file inventory")?;
        resolved["files"] = json!(inventory);
    }
    let m = &resolved;
    // Archive hashes cannot be embedded inside the hashed ZIP itself.
    let mut expected = m.clone();
    expected
        .as_object_mut()
        .ok_or("Invalid manifest")?
        .remove("artifacts");
    let mut actual = packaged.clone();
    actual
        .as_object_mut()
        .ok_or("Invalid packaged manifest")?
        .remove("artifacts");
    if expected != actual {
        return Err("Packaged manifest differs from trusted manifest".into());
    }
    write_json_atomic(&stage.path().join("extension.json"), m)?;
    let exe = verify_files(stage.path(), m, a)?;
    for entry in walkdir::WalkDir::new(stage.path()) {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().is_file() {
            let name = entry
                .path()
                .strip_prefix(stage.path())
                .map_err(|e| e.to_string())?
                .to_str()
                .ok_or("Non-UTF8 package member")?
                .replace('\\', "/");
            if name != "extension.json" && m["files"].get(&name).is_none() {
                return Err(format!("Uninventoried file: {name}"));
            }
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(exe, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    let _ = exe;
    let directory = format!(
        "{}-{}",
        m["version"].as_str().unwrap_or(""),
        uuid::Uuid::new_v4()
    );
    let destination = root.join("versions").join(&directory);
    // Commit DB before the atomic active-pointer publication, rolling it back if
    // publication fails. The previous version is never modified or deleted.
    let db_path = ctx.paths.data_dir.join("extensions/installed.json");
    let previous_db = if db_path.exists() {
        Some(read_json(&db_path)?)
    } else {
        None
    };
    let mut db = previous_db.clone().unwrap_or(json!({"installed":[]}));
    let entries = db["installed"]
        .as_array_mut()
        .ok_or("Invalid installed database")?;
    let mut item = super::v2_catalog::store_item(m.clone());
    item["installedAt"] = entries
        .iter()
        .find(|e| e["id"] == id)
        .map(|e| e["installedAt"].clone())
        .unwrap_or(json!(crate::native_core::common::now_ms()));
    entries.retain(|e| e["id"] != id);
    entries.push(item.clone());
    std::fs::rename(stage.path(), &destination).map_err(|e| e.to_string())?;
    let pointer = json!({"directory":directory,"manifestSha256":hash_file(&destination.join("extension.json"))?});
    let commit = (|| {
        write_json_atomic(&db_path, &db)?;
        write_json_atomic(&root.join("active.json"), &pointer)
    })();
    if let Err(error) = commit {
        if let Some(old) = previous_db {
            let _ = write_json_atomic(&db_path, &old);
        } else {
            let _ = std::fs::remove_file(db_path);
        }
        let _ = std::fs::remove_dir_all(destination);
        return Err(error);
    }
    // Discovery copy is not launch authority. Preferences at package root and old
    // native-v2 stores remain untouched across upgrades.
    if let Err(e) = write_json_atomic(&root.join("package.json"), m) {
        log::warn!("Package discovery metadata update failed: {e}");
    }
    Ok(item)
}

#[cfg(test)]
pub(super) fn test_install(ctx: &CoreContext, mode: &str) -> Value {
    test_install_with(ctx, mode, |_| {})
}
#[cfg(test)]
pub(super) fn test_install_with(
    ctx: &CoreContext,
    mode: &str,
    edit: impl FnOnce(&mut Value),
) -> Value {
    static FIXTURE: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let binary = FIXTURE.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("fixture.rs");
        std::fs::write(&source, include_str!("process_fixture.rs.txt")).unwrap();
        let exe = dir.path().join(if cfg!(windows) {
            "fixture.exe"
        } else {
            "fixture"
        });
        let output = std::process::Command::new("rustc")
            .arg("--edition=2021")
            .arg(&source)
            .arg("-o")
            .arg(&exe)
            .output()
            .expect(
                "rustc is required to build the real test fixture, not for installed extensions",
            );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::read(exe).unwrap()
    });
    let exe = format!(
        "bin/fixture-{mode}{}",
        if cfg!(windows) { ".exe" } else { "" }
    );
    let hash = format!("{:x}", Sha256::digest(binary));
    let mut m = json!({"schemaVersion":3,"id":"com.tezbar.native.perplexity","slug":"perplexity","name":"Perplexity","version":"1.0.0","runtime":"rust","execution":"process","protocolVersion":1,"repository":"https://example.com/fixture","capabilities":["shell","background","window"],"platforms":["macOS","Windows","Linux"],"preferences":[],"commands":[{"name":"ask-perplexity","runtime":"rust","platforms":["macOS","Windows","Linux"],"concurrency":"reject-while-busy","refreshMs":null}],"files":{exe.clone():hash.clone()},"artifacts":{}});
    edit(&mut m);
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    z.start_file("extension.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(serde_json::to_string(&m).unwrap().as_bytes())
        .unwrap();
    z.start_file(
        &exe,
        zip::write::SimpleFileOptions::default().unix_permissions(0o755),
    )
    .unwrap();
    z.write_all(binary).unwrap();
    let bytes = z.finish().unwrap().into_inner();
    m["artifacts"][target()] = json!({"url":"https://example.com/fixture.zip","sha256":format!("{:x}",Sha256::digest(&bytes)),"executable":exe,"executableSha256":hash});
    install_bytes(ctx, &m, &m["artifacts"][target()], &bytes).unwrap();
    m
}

#[cfg(test)]
pub(super) fn test_import_target(
    ctx: &CoreContext,
    manifest: &Value,
    target: &str,
    archive: &Path,
) -> CoreResult {
    validate(manifest)?;
    let artifact = artifact_for(manifest, target)?;
    install_bytes(
        ctx,
        manifest,
        artifact,
        &std::fs::read(archive).map_err(|e| e.to_string())?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn host_target_mapping_includes_intel_mac_without_aliasing_architectures() {
        for (os, arch, expected) in [
            ("macos", "aarch64", "aarch64-apple-darwin"),
            ("macos", "x86_64", "x86_64-apple-darwin"),
            ("windows", "x86_64", "x86_64-pc-windows-msvc"),
            ("windows", "aarch64", "unsupported"),
            ("linux", "x86_64", "unsupported"),
        ] {
            assert_eq!(target_for(os, arch), expected, "{os}/{arch}");
        }
        assert_eq!(
            target(),
            target_for(std::env::consts::OS, std::env::consts::ARCH)
        );
    }
    #[test]
    fn intel_mac_requires_its_own_artifact_and_has_no_public_catalog_pins() {
        let intel = "x86_64-apple-darwin";
        for manifest in super::super::v2_catalog::source() {
            assert!(manifest["artifacts"].get(intel).is_none());
            assert!(artifact_for(&manifest, intel)
                .unwrap_err()
                .contains("publication/pins required"));
            if target() == intel {
                let described = super::super::v2_catalog::describe(manifest);
                assert_eq!(described["status"], "unavailable");
                assert!(described["commands"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|c| c["runtime"] == "unavailable"));
            }
        }
        let local = json!({"schemaVersion":3,"runtime":"rust","execution":"process","protocolVersion":1,
            "id":"com.tezbar.native.synthetic","slug":"synthetic","version":"1.0.0",
            "capabilities":[],"commands":[{"name":"run"}],
            "artifacts":{intel:{"sha256":"0".repeat(64),"executable":"bin/synthetic","executableSha256":"1".repeat(64)}}});
        validate(&local).unwrap();
        assert!(artifact_for(&local, intel).is_ok());
        assert!(artifact_for(&local, "aarch64-apple-darwin").is_err());
        if target() == intel {
            assert!(artifact(&local).is_ok());
        }
    }
    #[test]
    fn rejects_cross_platform_paths() {
        for p in [
            "../bad",
            "/bad",
            "C:/bad",
            "\\\\server\\bad",
            "a\\b",
            "a//b",
            "a/../b",
            "bin/CON.exe",
            "a.",
            "a ",
        ] {
            assert!(relative(p).is_err(), "{p}");
        }
        assert!(relative("resources/colors.json").is_ok());
    }
    #[test]
    fn zip_security_and_inventory() {
        fn archive(name: &str, mode: u32, content: &[u8]) -> Vec<u8> {
            let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            z.start_file(
                name,
                zip::write::SimpleFileOptions::default().unix_permissions(mode),
            )
            .unwrap();
            z.write_all(content).unwrap();
            z.finish().unwrap().into_inner()
        }
        let temp = tempfile::tempdir().unwrap();
        assert!(extract(&archive("../escape", 0o644, b"x"), temp.path()).is_err());
        assert!(extract(b"not a zip", temp.path()).is_err());
        assert!(extract(&archive("resources/ok", 0o644, b"ok"), temp.path()).is_ok());
        assert!(verify_files(
            temp.path(),
            &json!({"files":{"resources/ok":"0".repeat(64)}}),
            &json!({"executable":"resources/ok"})
        )
        .is_err());
    }
}

#[cfg(test)]
mod activation_tests {
    use super::*;
    use crate::native_core::common::CorePaths;
    fn context(path: &Path) -> CoreContext {
        CoreContext::headless(CorePaths {
            data_dir: path.into(),
            config_dir: path.join("config"),
            resource_dir: path.join("resources"),
            temp_dir: path.join("tmp"),
            home_dir: path.into(),
        })
    }
    fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, bytes) in files {
            zip.start_file(
                *name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated)
                    .unix_permissions(0o755),
            )
            .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
    fn reference(bytes: &[u8]) -> (Value, Vec<u8>) {
        let executable = "bin/test";
        let sha = format!("{:x}", Sha256::digest(bytes));
        let mut m = json!({"schemaVersion":3,"runtime":"rust","execution":"process","protocolVersion":1,"id":"com.tezbar.native.example","slug":"example","name":"Example","version":"1.0.0","repository":"https://example.com","capabilities":[],"commands":[{"name":"run"}],"preferences":[],"replaces":[],"artifacts":{},"files":{executable:sha.clone()}});
        let metadata = serde_json::to_vec(&m).unwrap();
        let archive = zip(&[("extension.json", &metadata), (executable, bytes)]);
        m["artifacts"][target()] = json!({"url":"https://example.com/package.zip","sha256":format!("{:x}",Sha256::digest(&archive)),"executable":executable,"executableSha256":sha});
        (m, archive)
    }
    #[test]
    fn reject_invalid_package_without_changing_active_or_preferences() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let (m, bytes) = reference(b"an executable fixture");
        let a = &m["artifacts"][target()];
        install_bytes(&ctx, &m, a, &bytes).unwrap();
        let root = root(&ctx, m["id"].as_str().unwrap()).unwrap();
        let pointer = std::fs::read(root.join("active.json")).unwrap();
        std::fs::write(root.join("preferences.json"), b"{\"saved\":true}").unwrap();
        std::fs::create_dir_all(temp.path().join("native-extensions-v2/example")).unwrap();
        std::fs::write(
            temp.path()
                .join("native-extensions-v2/example/storage.json"),
            b"{\"history\":1}",
        )
        .unwrap();
        assert!(install_bytes(&ctx, &m, a, b"invalid archive").is_err());
        let mut bad = m.clone();
        bad["protocolVersion"] = json!(2);
        assert!(artifact(&bad).is_err());
        bad = m.clone();
        bad["schemaVersion"] = json!(2);
        assert!(artifact(&bad).is_err());
        bad = m.clone();
        bad["artifacts"] = json!({});
        assert!(artifact(&bad).is_err());
        let badzip = b"not zip";
        bad = m.clone();
        bad["artifacts"][target()]["sha256"] = json!(format!("{:x}", Sha256::digest(badzip)));
        assert!(
            install_bytes(&ctx, &bad, &bad["artifacts"][target()], badzip)
                .unwrap_err()
                .contains("ZIP")
        );
        assert_eq!(std::fs::read(root.join("active.json")).unwrap(), pointer);
        assert_eq!(
            read_json(&root.join("preferences.json")).unwrap()["saved"],
            true
        );
        assert_eq!(
            read_json(
                &temp
                    .path()
                    .join("native-extensions-v2/example/storage.json")
            )
            .unwrap()["history"],
            1
        );
        let package = resolve(&ctx, m["id"].as_str().unwrap()).unwrap();
        std::fs::write(&package.executable, b"tampered").unwrap();
        assert!(resolve(&ctx, m["id"].as_str().unwrap())
            .unwrap_err_string()
            .contains("Tampered"));
    }
    #[test]
    fn combined_reference_derives_only_hash_verified_target_inventory() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let (mut trusted, bytes) = reference(b"actual target executable");
        let pinned = trusted["files"].clone();
        trusted["files"] = json!({});
        install_bytes(&ctx, &trusted, &trusted["artifacts"][target()], &bytes).unwrap();
        let installed = resolve(&ctx, trusted["id"].as_str().unwrap()).unwrap();
        assert_eq!(installed.manifest["files"], pinned);
        let root = root(&ctx, trusted["id"].as_str().unwrap()).unwrap();
        let pointer = std::fs::read(root.join("active.json")).unwrap();
        std::fs::write(root.join("preferences.json"), b"{\"saved\":true}").unwrap();
        for invalid in [Value::Null, json!("files"), json!([])] {
            let mut malformed = trusted.clone();
            malformed["files"] = invalid;
            assert!(
                install_bytes(&ctx, &malformed, &malformed["artifacts"][target()], &bytes)
                    .unwrap_err()
                    .contains("files must be an object")
            );
            assert_eq!(std::fs::read(root.join("active.json")).unwrap(), pointer);
        }
        let mut mismatch = trusted.clone();
        mismatch["files"] = json!({"bin/test":"0".repeat(64)});
        assert!(
            install_bytes(&ctx, &mismatch, &mismatch["artifacts"][target()], &bytes)
                .unwrap_err()
                .contains("differs")
        );
        assert_eq!(std::fs::read(root.join("active.json")).unwrap(), pointer);
        assert_eq!(
            read_json(&root.join("preferences.json")).unwrap()["saved"],
            true
        );
        let mut omitted = trusted.clone();
        omitted.as_object_mut().unwrap().remove("files");
        install_bytes(&ctx, &omitted, &omitted["artifacts"][target()], &bytes).unwrap();
        assert_eq!(
            resolve(&ctx, trusted["id"].as_str().unwrap())
                .unwrap()
                .manifest["files"],
            pinned
        );
        let mut wrong_hash = trusted.clone();
        wrong_hash["artifacts"][target()]["sha256"] = json!("0".repeat(64));
        wrong_hash["files"] = Value::Null;
        assert!(install_bytes(
            &ctx,
            &wrong_hash,
            &wrong_hash["artifacts"][target()],
            &bytes
        )
        .unwrap_err()
        .contains("SHA256"));
        let active = resolve(&ctx, trusted["id"].as_str().unwrap()).unwrap();
        std::fs::write(active.executable, b"after install tamper").unwrap();
        assert!(resolve(&ctx, trusted["id"].as_str().unwrap()).is_err());
    }
    #[tokio::test]
    async fn local_only_import_is_offline_discoverable_and_preserves_active_on_bad_hash() {
        use super::super::acquisition::Rule;
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let (mut m, bytes) = reference(b"local-only executable fixture");
        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        m["artifacts"][target()]["url"] = json!(format!(
            "http://{}/never-requested.zip",
            server.local_addr().unwrap()
        ));
        let archive = temp.path().join("local.zip");
        std::fs::write(&archive, &bytes).unwrap();
        let error = install_with_rule(&ctx, m.clone(), None, true, Ok(Rule::LocalImportOnly))
            .await
            .unwrap_err();
        assert!(error.contains("--extensions-v3-import"));
        assert!(!root(&ctx, m["id"].as_str().unwrap()).unwrap().exists());
        assert!(install_with_rule(
            &ctx,
            m.clone(),
            None,
            true,
            Err("unsupported acquisition".into())
        )
        .await
        .unwrap_err()
        .contains("unsupported acquisition"));
        install_with_rule(
            &ctx,
            m.clone(),
            Some(archive.clone()),
            true,
            Ok(Rule::LocalImportOnly),
        )
        .await
        .unwrap();
        let id = m["id"].as_str().unwrap().to_owned();
        let installed =
            super::super::registry::handle(&ctx, "extensions:listInstalled", &[]).unwrap();
        assert_eq!(installed[0]["id"], id);
        assert_eq!(installed[0]["installationStatus"], "installed");
        assert_eq!(installed[0]["nativeV2"]["commands"][0]["name"], "run");
        let root = root(&ctx, &id).unwrap();
        let active = std::fs::read(root.join("active.json")).unwrap();
        std::fs::write(root.join("preferences.json"), b"{\"saved\":true}").unwrap();
        let mut bad = m.clone();
        bad["artifacts"][target()]["sha256"] = json!("0".repeat(64));
        assert!(
            install_with_rule(&ctx, bad, Some(archive), true, Ok(Rule::LocalImportOnly))
                .await
                .unwrap_err()
                .contains("SHA256")
        );
        // Reinstall remains blocked even after a valid package is installed.
        assert!(
            install_with_rule(&ctx, m, None, true, Ok(Rule::LocalImportOnly))
                .await
                .is_err()
        );
        assert_eq!(std::fs::read(root.join("active.json")).unwrap(), active);
        assert_eq!(
            read_json(&root.join("preferences.json")).unwrap()["saved"],
            true
        );
        assert!(resolve(&ctx, &id).is_ok());
        assert!(
            tokio::time::timeout(Duration::from_millis(30), server.accept())
                .await
                .is_err(),
            "No remote request is permitted for local-only acquisition/import"
        );
    }

    #[test]
    fn published_combined_reference_omitted_files_installs_and_mismatch_rolls_back() {
        let Some(path) = std::env::var_os("TEZBAR_HOST_TEST_CALENDAR_MANIFEST") else {
            eprintln!("Published omitted-files fixture not supplied");
            return;
        };
        let m = read_json(Path::new(&path)).unwrap();
        assert!(m.get("files").is_none(), "Use the raw published reference");
        let archive =
            std::env::var_os("TEZBAR_HOST_TEST_CALENDAR_ZIP").expect("Published ZIP required");
        let bytes = std::fs::read(archive).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let a = artifact(&m).unwrap();
        install_bytes(&ctx, &m, a, &bytes).unwrap();
        let id = m["id"].as_str().unwrap();
        let installed = resolve(&ctx, id).unwrap();
        assert!(!installed.manifest["files"].as_object().unwrap().is_empty());
        let root = root(&ctx, id).unwrap();
        let pointer = std::fs::read(root.join("active.json")).unwrap();
        let db = std::fs::read(ctx.paths.data_dir.join("extensions/installed.json")).unwrap();
        std::fs::write(root.join("preferences.json"), b"{\"saved\":true}").unwrap();
        for files in [
            Value::Null,
            json!("invalid"),
            json!([]),
            json!({"bin/mismatch":"0".repeat(64)}),
        ] {
            let mut bad = m.clone();
            bad["files"] = files;
            assert!(install_bytes(&ctx, &bad, a, &bytes).is_err());
            assert_eq!(std::fs::read(root.join("active.json")).unwrap(), pointer);
            assert_eq!(
                std::fs::read(ctx.paths.data_dir.join("extensions/installed.json")).unwrap(),
                db
            );
            assert_eq!(
                read_json(&root.join("preferences.json")).unwrap()["saved"],
                true
            );
            assert!(resolve(&ctx, id).is_ok());
        }
        eprintln!("Published omitted-files reference: {} installed with exact derived inventory; malformed/mismatched inventory rollback verified",Path::new(&path).display());
    }
    #[cfg(unix)]
    #[test]
    fn publication_failure_rolls_back_database_and_previous_version() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let (m, bytes) = reference(b"old version");
        install_bytes(&ctx, &m, &m["artifacts"][target()], &bytes).unwrap();
        let root = root(&ctx, m["id"].as_str().unwrap()).unwrap();
        let active = std::fs::read(root.join("active.json")).unwrap();
        let db = std::fs::read(temp.path().join("extensions/installed.json")).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o500)).unwrap();
        let (new, bytes) = reference(b"new version");
        let result = install_bytes(&ctx, &new, &new["artifacts"][target()], &bytes);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_err());
        assert_eq!(std::fs::read(root.join("active.json")).unwrap(), active);
        assert_eq!(
            std::fs::read(temp.path().join("extensions/installed.json")).unwrap(),
            db
        );
        assert!(resolve(&ctx, m["id"].as_str().unwrap()).is_ok());
    }
    #[cfg(unix)]
    #[test]
    fn symlink_hardlink_extra_files_and_missing_executable_never_launch() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let (m, bytes) = reference(b"binary");
        install_bytes(&ctx, &m, &m["artifacts"][target()], &bytes).unwrap();
        let p = resolve(&ctx, m["id"].as_str().unwrap()).unwrap();
        let elsewhere = temp.path().join("outside");
        std::fs::write(&elsewhere, b"binary").unwrap();
        std::fs::remove_file(&p.executable).unwrap();
        symlink(&elsewhere, &p.executable).unwrap();
        assert!(resolve(&ctx, m["id"].as_str().unwrap()).is_err());
        std::fs::remove_file(&p.executable).unwrap();
        std::fs::hard_link(&elsewhere, &p.executable).unwrap();
        assert!(resolve(&ctx, m["id"].as_str().unwrap()).is_err());
        std::fs::remove_file(&p.executable).unwrap();
        assert!(resolve(&ctx, m["id"].as_str().unwrap()).is_err());
        let temp2 = tempfile::tempdir().unwrap();
        let ctx = context(temp2.path());
        install_bytes(&ctx, &m, &m["artifacts"][target()], &bytes).unwrap();
        let p = resolve(&ctx, m["id"].as_str().unwrap()).unwrap();
        std::fs::write(p.dir.join("untrusted-helper"), b"unknown").unwrap();
        assert!(resolve(&ctx, m["id"].as_str().unwrap()).is_err());
        let temp3 = tempfile::tempdir().unwrap();
        let ctx = context(temp3.path());
        std::fs::create_dir_all(temp3.path().join("extensions/packages")).unwrap();
        symlink(
            temp.path(),
            temp3
                .path()
                .join("extensions/packages/com.tezbar.native.example"),
        )
        .unwrap();
        assert!(install_bytes(&ctx, &m, &m["artifacts"][target()], &bytes).is_err());
    }
    #[test]
    fn reject_zip_symlinks_case_collisions_and_bombs() {
        let temp = tempfile::tempdir().unwrap();
        let bomb = vec![0u8; 2 * 1024 * 1024];
        assert!(extract(&zip(&[("resources/bomb", &bomb)]), temp.path()).is_err());
        assert!(extract(&zip(&[("A", b"a"), ("a", b"b")]), temp.path()).is_err());
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        z.add_symlink(
            "link",
            "/tmp/outside",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        let bytes = z.finish().unwrap().into_inner();
        assert!(extract(&bytes, temp.path()).is_err());
    }
    #[test]
    fn concurrent_reinstall_uninstall_serializes_without_losing_other_records() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let (m, bytes) = reference(b"a target executable");
        install_bytes(&ctx, &m, &m["artifacts"][target()], &bytes).unwrap();
        let database = ctx.paths.data_dir.join("extensions/installed.json");
        let mut db = read_json(&database).unwrap();
        db["installed"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"raycast.unrelated","name":"Unrelated"}));
        write_json_atomic(&database, &db).unwrap();
        let context1 = ctx.clone();
        let manifest = m.clone();
        let archive = bytes.clone();
        let installer = std::thread::spawn(move || {
            install_bytes(
                &context1,
                &manifest,
                &manifest["artifacts"][target()],
                &archive,
            )
        });
        let context2 = ctx.clone();
        let uninstaller = std::thread::spawn(move || {
            super::super::registry::handle(
                &context2,
                "extension:uninstall",
                &[json!("com.tezbar.native.example")],
            )
        });
        installer.join().unwrap().unwrap();
        uninstaller.join().unwrap().unwrap();
        let db = read_json(&database).unwrap();
        let installed = db["installed"].as_array().unwrap();
        assert_eq!(
            installed
                .iter()
                .filter(|item| item["id"] == "raycast.unrelated")
                .count(),
            1
        );
        let selected = installed
            .iter()
            .filter(|item| item["id"] == "com.tezbar.native.example")
            .count();
        assert!(selected <= 1);
        if selected == 1 {
            assert!(resolve(&ctx, "com.tezbar.native.example").is_ok());
        } else {
            assert!(!root(&ctx, "com.tezbar.native.example")
                .unwrap()
                .join("active.json")
                .exists());
        }
    }
    #[tokio::test]
    async fn async_http_fixture_installs_and_production_rejects_http() {
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::TcpListener,
        };
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let (mut m, bytes) = reference(b"downloaded executable");
        let server = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = server.local_addr().unwrap();
        m["artifacts"][target()]["url"] = json!(format!("http://{address}/package.zip"));
        assert!(install(&ctx, m.clone(), None)
            .await
            .unwrap_err()
            .contains("HTTPS"));
        let task = tokio::spawn(async move {
            let (mut socket, _) = server.accept().await.unwrap();
            let mut request = [0u8; 8192];
            let _ = socket.read(&mut request).await.unwrap();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        bytes.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            for chunk in bytes.chunks(73) {
                socket.write_all(chunk).await.unwrap();
            }
        });
        install_inner(&ctx, m.clone(), None, true).await.unwrap();
        task.await.unwrap();
        assert!(resolve(&ctx, m["id"].as_str().unwrap()).is_ok());
        let server = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = server.local_addr().unwrap();
        m["artifacts"][target()]["url"] = json!(format!("http://{address}/private.zip"));
        let task = tokio::spawn(async move {
            let (mut socket, _) = server.accept().await.unwrap();
            let mut request = [0u8; 8192];
            let _ = socket.read(&mut request).await.unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
        });
        assert!(install_inner(&ctx, m, None, true)
            .await
            .unwrap_err()
            .contains("local ZIP import"));
        task.await.unwrap();
    }
    trait ErrorString {
        fn unwrap_err_string(self) -> String;
    }
    impl ErrorString for Result<Package, String> {
        fn unwrap_err_string(self) -> String {
            match self {
                Err(e) => e,
                Ok(_) => panic!("tampered package accepted"),
            }
        }
    }
}
