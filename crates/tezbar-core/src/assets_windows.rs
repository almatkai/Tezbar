//! Locked handle walk. READ sharing only also excludes in-place reparse writes,
//! not just rename/delete races. No new crate edge; these are stable kernel32 APIs.
use super::FAILURE;
use std::{
    ffi::c_void,
    fs::File,
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Component, Path, PathBuf, Prefix},
};
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetFinalPathNameByHandleW(file: *mut c_void, path: *mut u16, length: u32, flags: u32)
        -> u32;
    fn GetFileType(file: *mut c_void) -> u32;
}
fn final_path(file: &File) -> Result<PathBuf, String> {
    use std::os::windows::ffi::OsStringExt;
    let mut buf = vec![0u16; 32768];
    let n = unsafe {
        GetFinalPathNameByHandleW(file.as_raw_handle(), buf.as_mut_ptr(), buf.len() as u32, 0)
    } as usize;
    if n == 0 || n >= buf.len() {
        return Err(FAILURE.into());
    }
    Ok(std::ffi::OsString::from_wide(&buf[..n]).into())
}
// Path::starts_with compares whole components, not the textual \\?\ marker.
// Match the parsed disk prefix so an already-verbatim path is never prefixed twice.
fn is_verbatim_disk(path: &Path) -> bool {
    matches!(path.components().next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::VerbatimDisk(_)))
}
fn safe_name(name: &std::ffi::OsStr) -> bool {
    let Some(s) = name.to_str() else {
        return false;
    };
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    !s.is_empty()
        && !s.contains([':', '~'])
        && !s.ends_with(['.', ' '])
        && !s.chars().any(|c| c.is_control())
        && !matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
        )
        && !["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        })
}
pub(super) fn canonical_spelling(path: &Path) -> Result<PathBuf, String> {
    let parts = path.components().collect::<Vec<_>>();
    if parts.len() < 3
        || !matches!(parts[0], Component::Prefix(p) if matches!(p.kind(), Prefix::VerbatimDisk(_) | Prefix::Disk(_)))
        || parts[1] != Component::RootDir
        || parts[2..]
            .iter()
            .any(|p| !matches!(p, Component::Normal(n) if safe_name(n)))
    {
        return Err(FAILURE.into());
    }
    Ok(if is_verbatim_disk(path) {
        path.to_owned()
    } else {
        // Rebuild from parsed components: `/` separates components here (for example
        // `root.join("assets/icon.png")`) but is a literal character after `\\?\`.
        let mut spelled = std::ffi::OsString::from(r"\\?\");
        spelled.push(parts[0].as_os_str());
        for part in &parts[2..] {
            spelled.push(r"\");
            spelled.push(part.as_os_str());
        }
        PathBuf::from(spelled)
    })
}
/// User-selected files carry explicit host authority. Follow normal disk aliases
/// (including cloud placeholders); extension scope traversal remains separate.
pub(super) fn open_chosen(path: &Path) -> Result<File, String> {
    let prefix = match path.components().next() {
        Some(Component::Prefix(prefix)) if path.has_root() => prefix.kind(),
        _ => return Err(FAILURE.into()),
    };
    match prefix {
        Prefix::Disk(_) | Prefix::VerbatimDisk(_) => {}
        Prefix::UNC(_, share) | Prefix::VerbatimUNC(_, share)
            if !share.to_string_lossy().eq_ignore_ascii_case("pipe") => {}
        _ => return Err(FAILURE.into()),
    }
    let file = File::open(path).map_err(|_| FAILURE)?;
    if !file.metadata().map_err(|_| FAILURE)?.is_file()
        || unsafe { GetFileType(file.as_raw_handle()) } != 1
    {
        return Err(FAILURE.into());
    }
    Ok(file)
}
/// Only canonical local disk paths are accepted. UNC/device namespaces fail closed.
/// Each opened ancestor remains locked until the final read finishes. A conflicting
/// writer/delete handle causes admission to fail rather than widening authority.
pub(super) fn open_locked(path: &Path) -> Result<(File, Vec<File>), String> {
    let parts = path.components().collect::<Vec<_>>();
    if parts.len() < 3
        || !matches!(parts[0], Component::Prefix(p) if matches!(p.kind(), Prefix::VerbatimDisk(_) | Prefix::Disk(_)))
        || parts[1] != Component::RootDir
        || parts[2..]
            .iter()
            .any(|p| !matches!(p, Component::Normal(n) if safe_name(n)))
    {
        return Err(FAILURE.into());
    }
    let mut current = PathBuf::from(parts[0].as_os_str());
    current.push(parts[1].as_os_str());
    let mut held = Vec::new();
    // Open from volume root; check metadata on HANDLE, never on a checked/reopened path.
    for index in 1..parts.len() {
        if index > 1 {
            current.push(parts[index].as_os_str());
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x02000000 | 0x00200000) // BACKUP_SEMANTICS | OPEN_REPARSE_POINT
            .open(&current)
            .map_err(|_| FAILURE)?;
        let meta = file.metadata().map_err(|_| FAILURE)?;
        if meta.file_attributes() & 0x400 != 0
            || unsafe { GetFileType(file.as_raw_handle()) } != 1
            || if index + 1 == parts.len() {
                !meta.is_file()
            } else {
                !meta.is_dir()
            }
        {
            return Err(FAILURE.into());
        }
        // Normalized handle path must match exactly, including the issued canonical
        // root / exact legacy grant; aliases or an unexpected mount cannot pass.
        let expected = if is_verbatim_disk(&current) {
            current.clone()
        } else {
            PathBuf::from(format!(r"\\?\{}", current.display()))
        };
        if final_path(&file)? != expected {
            return Err(FAILURE.into());
        }
        held.push(file);
    }
    let file = held.pop().ok_or(FAILURE)?;
    Ok((file, held))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{read_scoped, AssetScope};
    use std::collections::BTreeSet;
    fn junction(link: &Path, target: &Path) {
        let status = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .status()
            .unwrap();
        assert!(status.success(), "fixture junction creation failed");
    }
    #[test]
    fn windows_verbatim_disk_spelling_is_idempotent() {
        let canonical = Path::new(r"\\?\C:\fixture\image.png");
        assert!(is_verbatim_disk(canonical));
        assert!(!is_verbatim_disk(Path::new(r"C:\fixture\image.png")));
        assert_eq!(canonical_spelling(canonical).unwrap(), canonical);
        assert_eq!(
            canonical_spelling(Path::new(r"C:\fixture\image.png")).unwrap(),
            canonical
        );
        assert!(!is_verbatim_disk(Path::new(r"\\.\pipe\fixture")));
        // `/` from a relative asset join is a separator, never part of the verbatim name.
        assert_eq!(
            canonical_spelling(Path::new(r"C:\fixture\assets/icons/image.png")).unwrap(),
            Path::new(r"\\?\C:\fixture\assets\icons\image.png")
        );
    }
    #[tokio::test]
    async fn windows_scoped_read_accepts_forward_slash_relative_asset_joins() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("assets").join("icons")).unwrap();
        std::fs::write(root.join("assets").join("icons").join("image.png"), b"inside").unwrap();
        let scope = crate::assets::AssetScope::registered(root, BTreeSet::new());
        // Extension manifests name assets with `/`; the uncanonicalized join must load.
        let joined = d.path().join("assets/icons/image.png");
        assert_eq!(
            crate::assets::AssetService::default()
                .local(&scope, &joined, 20)
                .await
                .unwrap(),
            b"inside"
        );
    }
    #[tokio::test]
    async fn windows_chosen_files_follow_user_aliases_without_scope_authority() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        let tilde = root.join("icon~dark.png");
        std::fs::write(&tilde, b"chosen").unwrap();
        let service = crate::assets::AssetService::default();
        assert_eq!(service.chosen_image(&tilde).await.unwrap(), b"chosen");
        assert!(open_locked(&tilde).is_err());
        assert!(open_chosen(Path::new(r"\\.\pipe\fixture")).is_err());
        assert!(open_chosen(Path::new(r"\\server\pipe\fixture")).is_err());
        let target = root.join("target.png");
        std::fs::write(&target, b"user alias").unwrap();
        let link = root.join("chosen.png");
        std::os::windows::fs::symlink_file(&target, &link)
            .expect("Fixture requires Developer Mode or symlink privilege");
        assert_eq!(service.chosen_image(&link).await.unwrap(), b"user alias");
        assert!(open_locked(&link).is_err());
        std::fs::remove_file(link).unwrap();
    }
    #[test]
    fn windows_scope_paths_aliases_links_legacy_and_removal() {
        let d = tempfile::tempdir().unwrap();
        let o = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        let outside = o.path().canonicalize().unwrap();
        let path = root.join("image");
        std::fs::write(&path, b"inside").unwrap();
        let secret = outside.join("image");
        std::fs::write(&secret, b"outside").unwrap();
        let scope = AssetScope::registered(root.clone(), BTreeSet::new());
        assert_eq!(read_scoped(&scope, &path, 20).unwrap(), b"inside");
        for name in [
            "image:stream",
            "NUL",
            "CON.txt",
            "COM1",
            "LPT1",
            "LONGNA~1",
            "image.",
            "image ",
        ] {
            assert!(open_locked(&root.join(name)).is_err(), "{name}");
        }
        assert!(open_locked(Path::new(r"\\.\pipe\tezbar-fixture")).is_err());
        assert!(open_locked(Path::new(r"\\server\share\image")).is_err());
        let file_link = root.join("file-link");
        std::os::windows::fs::symlink_file(&secret, &file_link)
            .expect("Enable Developer Mode or run the fixture in an elevated disposable shell");
        assert!(open_locked(&file_link).is_err());
        std::fs::remove_file(&file_link).unwrap();
        let dir_link = root.join("dir-link");
        std::os::windows::fs::symlink_dir(&outside, &dir_link).unwrap();
        assert!(open_locked(&dir_link.join("image")).is_err());
        std::fs::remove_dir(dir_link).unwrap();
        let j = root.join("junction");
        junction(&j, &outside);
        assert!(open_locked(&j.join("image")).is_err());
        std::fs::remove_dir(j).unwrap();
        assert!(read_scoped(&scope, &secret, 20).is_err());
        let legacy = AssetScope::registered(root.clone(), [secret.clone()].into());
        assert_eq!(read_scoped(&legacy, &secret, 20).unwrap(), b"outside");
        std::fs::remove_file(&path).unwrap();
        assert!(read_scoped(&scope, &path, 20).is_err());
    }
    #[test]
    fn windows_ancestor_root_nested_junction_swap_and_handle_write_locks() {
        for level in ["ancestor", "root", "nested"] {
            let d = tempfile::tempdir().unwrap();
            let o = tempfile::tempdir().unwrap();
            let base = d.path().canonicalize().unwrap();
            let outside = o.path().canonicalize().unwrap();
            let root = base.join("ancestor/package");
            let path = root.join("nested/image");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"inside").unwrap();
            std::fs::create_dir_all(outside.join("package/nested")).unwrap();
            std::fs::create_dir_all(outside.join("nested")).unwrap();
            for p in [
                outside.join("package/nested/image"),
                outside.join("nested/image"),
                outside.join("image"),
            ] {
                std::fs::write(p, b"OUTSIDE").unwrap();
            }
            let scope = AssetScope::registered(root.clone(), BTreeSet::new());
            let legacy = AssetScope::registered(base.join("unrelated"), [path.clone()].into());
            let swapped = match level {
                "ancestor" => base.join("ancestor"),
                "root" => root.clone(),
                _ => root.join("nested"),
            };
            let parked = base.join("parked");
            // Stable HANDLE locks forbid both replacement and in-place reparse mutation
            // (requires a write handle); hold them through the final read.
            let (file, held) = open_locked(&path).unwrap();
            assert!(std::fs::rename(&swapped, &parked).is_err());
            assert!(std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(0x02000000 | 0x00200000)
                .open(&swapped)
                .is_err());
            drop(file);
            drop(held);
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let done = stop.clone();
            let a = swapped.clone();
            let b = parked.clone();
            let target = outside.clone();
            let swapper = std::thread::spawn(move || {
                for _ in 0..100 {
                    if done.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }
                    if std::fs::rename(&a, &b).is_ok() {
                        junction(&a, &target);
                        std::thread::yield_now();
                        std::fs::remove_dir(&a).unwrap();
                        std::fs::rename(&b, &a).unwrap();
                    }
                }
            });
            for _ in 0..300 {
                for grant in [&scope, &legacy] {
                    if let Ok(bytes) = read_scoped(grant, &path, 20) {
                        assert_eq!(bytes, b"inside");
                    }
                }
            }
            stop.store(true, std::sync::atomic::Ordering::SeqCst);
            swapper.join().unwrap();
            assert_eq!(read_scoped(&scope, &path, 20).unwrap(), b"inside");
            // Deterministic post-authorization replacement must fail too.
            std::fs::rename(&swapped, &parked).unwrap();
            junction(&swapped, &outside);
            assert!(read_scoped(&scope, &path, 20).is_err());
            assert!(read_scoped(&legacy, &path, 20).is_err());
            std::fs::remove_dir(&swapped).unwrap();
            std::fs::rename(parked, swapped).unwrap();
        }
    }
}
