//! App-owned runtime, independent of model/index identity. Only compiled-in pins
//! are trusted. No env/path override, network, mutable installed.json or fallback
//! to a modified payload. Old app versions keep using their old model runtime.
use super::runtime_integrity::{fingerprint, inventory, runtime_files, Fingerprint};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    schema: u32,
    target: String,
    id: String,
    archive_sha256: String,
    archive_bytes: u64,
    executable: String,
    files: Value,
}
#[derive(Clone)]
pub(super) struct Bundle {
    root: PathBuf,
    catalog: &'static str,
    archive: PathBuf,
}
#[derive(Clone)]
pub(super) struct VerifiedBundle {
    pub executable: PathBuf,
    fingerprint: Vec<Fingerprint>,
    directory: PathBuf,
}

fn archive_path(resources: &Path, source: &Path, unbundled_dev: bool) -> PathBuf {
    // CorePaths uses the source root only for an unbundled development app.
    // Packaged apps must resolve their own resource and never fall back to a
    // developer's staged files when a bundled resource is missing.
    if unbundled_dev && resources == source {
        resources.join("build/embedding-runtime/resources/embedding-runtime.zip")
    } else {
        resources.join("embedding-runtime/embedding-runtime.zip")
    }
}

impl VerifiedBundle {
    pub fn unchanged(&self) -> bool {
        fs::symlink_metadata(&self.directory)
            .map(|m| m.is_dir() && !m.file_type().is_symlink())
            .unwrap_or(false)
            && inventory(&self.directory)
                .map(|f| f == self.fingerprint)
                .unwrap_or(false)
    }
}
impl Bundle {
    #[cfg(test)]
    pub(super) fn fixture(data: &Path) -> Option<Self> {
        let mut bundle = Self::current(data, Path::new("fixture-app-resources"))?;
        bundle.archive = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../build/embedding-runtime/resources/embedding-runtime.zip");
        Some(bundle)
    }
    pub fn current(data: &Path, resources: &Path) -> Option<Self> {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        return Some(Self {
            root: data.join("knowledge/runtimes"),
            catalog: include_str!("../../../runtime/embedding-macos-arm64.json"),
            archive: archive_path(
                resources,
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."),
                cfg!(debug_assertions) && tauri::is_dev(),
            ),
        });
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            let _ = (data, resources);
            None
        }
    }
    pub fn ensure(
        &self,
        cached: Option<&VerifiedBundle>,
        check: &impl Fn() -> Result<(), String>,
    ) -> Result<VerifiedBundle, String> {
        // Q4 and Q8 share the payload. Serialize first-use/repair transactions,
        // not inference, and re-check cancellation after waiting for the lock.
        static INSTALL: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _install = INSTALL
            .lock()
            .map_err(|_| "Runtime install lock poisoned")?;
        check()?;
        let c: Catalog = serde_json::from_str(self.catalog).map_err(|e| e.to_string())?;
        if c.schema != 1
            || c.target != "aarch64-apple-darwin"
            || c.id.is_empty()
            || c.archive_sha256.len() != 64
            || !c.archive_sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || c.executable != "llama-server"
        {
            return Err("Invalid bundled runtime catalog".into());
        }
        // Content-addressed directory: upgrades and rollback never replace an old
        // payload under an active process, nor touch any model install/manifest.
        let destination = self.root.join(&c.archive_sha256);
        if let Some(cached) = cached {
            if cached.directory == destination && cached.unchanged() {
                return Ok(cached.clone());
            }
        }
        if destination.exists() {
            if let Ok(verified) = Self::verify(&destination, &c, check) {
                return Ok(verified);
            }
            // Invalid cache bytes never execute. Repair only from the embedded
            // authenticated archive, without downloading/replacing model weights.
            check()?;
        }
        let archive_bytes =
            fs::read(&self.archive).map_err(|e| format!("Cannot read bundled runtime: {e}"))?;
        if archive_bytes.len() as u64 != c.archive_bytes
            || format!("{:x}", Sha256::digest(&archive_bytes)) != c.archive_sha256
        {
            return Err("Bundled runtime archive failed SHA-256 verification".into());
        }
        fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        let staging = tempfile::Builder::new()
            .prefix(".runtime-stage-")
            .tempdir_in(&self.root)
            .map_err(|e| e.to_string())?;
        let mut archive =
            zip::ZipArchive::new(Cursor::new(archive_bytes)).map_err(|e| e.to_string())?;
        let files = c
            .files
            .as_object()
            .ok_or("Invalid bundled runtime file catalog")?;
        if archive.len() != files.len() {
            return Err("Bundled runtime entry count mismatch".into());
        }
        let mut names = std::collections::HashSet::new();
        for i in 0..archive.len() {
            check()?;
            let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = entry.name().to_owned();
            // Flat, exact catalog; prohibit links, directories, traversal,
            // duplicates and unbounded decompression even for a malformed pin.
            let expected = files.get(&name).ok_or("Uncatalogued runtime file")?;
            if name.contains(['/', '\\'])
                || name == "."
                || name == ".."
                || !names.insert(name.clone())
                || entry.is_dir()
                || entry
                    .unix_mode()
                    .map(|m| m & 0o170000 != 0o100000)
                    .unwrap_or(true)
                || expected["bytes"].as_u64() != Some(entry.size())
            {
                return Err("Unsafe bundled runtime entry".into());
            }
            let path = staging.path().join(name);
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
            let mut buffer = [0u8; 65536];
            loop {
                check()?;
                let n = entry.read(&mut buffer).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
            }
            file.sync_all().map_err(|e| e.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(
                    &path,
                    fs::Permissions::from_mode(entry.unix_mode().unwrap() & 0o777),
                )
                .map_err(|e| e.to_string())?;
            }
        }
        Self::verify(staging.path(), &c, check)?;
        check()?;
        let backup = self
            .root
            .join(format!(".runtime-backup-{}", uuid::Uuid::new_v4()));
        let had_old = publish(staging.path(), &destination, &backup)?;
        // The commit section has no cancellation point between the two renames.
        // Verification failure restores the previous payload before returning.
        match Self::verify(&destination, &c, &|| Ok(())) {
            Ok(verified) => {
                if had_old {
                    if backup.is_symlink() {
                        let _ = fs::remove_file(&backup);
                    } else {
                        let _ = fs::remove_dir_all(&backup);
                    }
                }
                Ok(verified)
            }
            Err(error) => {
                fs::remove_dir_all(&destination).map_err(|e| e.to_string())?;
                if had_old {
                    fs::rename(&backup, &destination).map_err(|e| e.to_string())?;
                }
                Err(error)
            }
        }
    }
    fn verify(
        directory: &Path,
        catalog: &Catalog,
        check: &impl Fn() -> Result<(), String>,
    ) -> Result<VerifiedBundle, String> {
        check()?;
        // Unlike legacy runtime inventories, a bundle must contain only regular
        // flat files. Symlinks are not a way to substitute another installation.
        if fs::symlink_metadata(directory)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Bundled runtime directory is a symlink".into());
        }
        for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
            if !entry
                .map_err(|e| e.to_string())?
                .file_type()
                .map_err(|e| e.to_string())?
                .is_file()
            {
                return Err("Bundled runtime contains a non-file entry".into());
            }
        }
        let before = inventory(directory)?;
        if runtime_files(directory, check)? != catalog.files {
            return Err("Bundled runtime failed pinned file integrity verification".into());
        }
        let executable = directory.join(&catalog.executable);
        fingerprint(&executable)?;
        if before != inventory(directory)? {
            return Err("Bundled runtime changed during verification".into());
        }
        check()?;
        Ok(VerifiedBundle {
            executable,
            fingerprint: before,
            directory: directory.to_owned(),
        })
    }
}

// The caller holds the install lock and has verified staging. Keep the two
// renames and rollback together: no cancellation/await can strand the backup.
fn publish(staging: &Path, destination: &Path, backup: &Path) -> Result<bool, String> {
    let had_old = fs::symlink_metadata(destination).is_ok();
    if had_old {
        fs::rename(destination, backup).map_err(|e| e.to_string())?;
    }
    if let Err(error) = fs::rename(staging, destination) {
        if had_old {
            fs::rename(backup, destination).map_err(|restore| {
                format!("Runtime install failed: {error}; rollback failed: {restore}")
            })?;
        }
        return Err(error.to_string());
    }
    Ok(had_old)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_runtime_resource_lookup_preserves_development_and_packaged_paths() {
        let source = Path::new("/worktree with spaces/src-tauri/..");
        let packaged = Path::new("/Applications/Tezbar.app/Contents/Resources");
        assert_eq!(
            archive_path(source, source, true),
            source.join("build/embedding-runtime/resources/embedding-runtime.zip")
        );
        for unbundled_dev in [false, true] {
            assert_eq!(
                archive_path(packaged, source, unbundled_dev),
                packaged.join("embedding-runtime/embedding-runtime.zip")
            );
        }
        assert_eq!(
            archive_path(source, source, false),
            source.join("embedding-runtime/embedding-runtime.zip")
        );
    }

    #[test]
    fn bundled_runtime_failed_publish_restores_previous_directory() {
        let temp = tempfile::tempdir().unwrap();
        let installed = temp.path().join("installed");
        let backup = temp.path().join("backup");
        fs::create_dir(&installed).unwrap();
        fs::write(installed.join("old"), b"old valid payload").unwrap();
        let old = fingerprint(&installed.join("old")).unwrap();
        assert!(publish(&temp.path().join("missing-stage"), &installed, &backup).is_err());
        assert_eq!(fingerprint(&installed.join("old")).unwrap(), old);
        assert!(!backup.exists());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }
    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn bundled_runtime_pin_install_reuse_tamper_cancel_and_rollback() {
        let temp = tempfile::tempdir().unwrap();
        let bundle = Bundle::fixture(temp.path()).unwrap();
        let cancelled = || Err("cancelled".to_string());
        assert!(bundle.ensure(None, &cancelled).is_err());
        assert!(!bundle.root.exists());
        let first = bundle.ensure(None, &|| Ok(())).unwrap();
        let inode = fingerprint(&first.executable).unwrap();
        let second = bundle.ensure(Some(&first), &|| Ok(())).unwrap();
        assert_eq!(inode, fingerprint(&second.executable).unwrap());
        assert!(first.unchanged());
        // Same-length corruption cannot acquire trust from a mutable manifest.
        let library = first.directory.join("libllama.0.6.0.dylib");
        let original = fs::read(&library).unwrap();
        let mut corrupt = original.clone();
        corrupt[100] ^= 1;
        fs::write(&library, corrupt).unwrap();
        assert!(!first.unchanged());
        let calls = std::sync::atomic::AtomicUsize::new(0);
        assert!(bundle
            .ensure(Some(&first), &|| {
                if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) > 30 {
                    Err("cancelled".into())
                } else {
                    Ok(())
                }
            })
            .is_err());
        assert_ne!(
            fs::read(&library).unwrap(),
            original,
            "cancel must not publish partial repair"
        );
        let repaired = bundle.ensure(Some(&first), &|| Ok(())).unwrap();
        assert!(repaired.unchanged());
        assert_eq!(fs::read(&library).unwrap(), original);
        // Failed new-package activation leaves the previous version intact.
        let mut bad_catalog: Value = serde_json::from_str(bundle.catalog).unwrap();
        bad_catalog["archiveSha256"] = serde_json::json!("0".repeat(64));
        let bad = Bundle {
            catalog: Box::leak(bad_catalog.to_string().into_boxed_str()),
            ..bundle.clone()
        };
        assert!(bad.ensure(None, &|| Ok(())).is_err());
        assert!(first.executable.exists());
        assert!(fs::read_dir(&bundle.root).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".runtime-stage-")));
    }
    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn bundled_runtime_mid_extraction_cancel_cleans_staging_and_retry_works() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let temp = tempfile::tempdir().unwrap();
        let bundle = Bundle::fixture(temp.path()).unwrap();
        let calls = AtomicUsize::new(0);
        let result = bundle.ensure(None, &|| {
            if calls.fetch_add(1, Ordering::SeqCst) >= 30 {
                Err("cancelled".into())
            } else {
                Ok(())
            }
        });
        assert!(matches!(result, Err(e) if e == "cancelled"));
        assert_eq!(fs::read_dir(&bundle.root).unwrap().count(), 0);
        let ready = bundle.ensure(None, &|| Ok(())).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            fs::remove_file(ready.directory.join("LICENSE")).unwrap();
            symlink(
                ready.directory.join("TEZBAR-PROVENANCE.json"),
                ready.directory.join("LICENSE"),
            )
            .unwrap();
            let repaired = bundle.ensure(Some(&ready), &|| Ok(())).unwrap();
            assert!(repaired.unchanged());
            assert!(!fs::symlink_metadata(ready.directory.join("LICENSE"))
                .unwrap()
                .file_type()
                .is_symlink());
        }
    }
}
