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
