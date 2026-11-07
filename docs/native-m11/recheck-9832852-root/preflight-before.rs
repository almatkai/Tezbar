pub fn preflight_macos_install(installed: &Path) -> Result<(), String> {
    if installed.file_name().and_then(|v| v.to_str()) != Some("Tezbar.app") || !installed.is_dir() {
        return Err("The installed Tezbar bundle is unavailable".into());
    }
    tempfile::Builder::new().prefix(".tezbar-install-check-")
        .tempdir_in(installed.parent().ok_or("Missing installation directory")?)
        .and_then(|probe| probe.close())
        .map_err(|_| "Tezbar cannot update in this folder. Move it to a writable Applications folder, then retry. The verified download has been kept.".into())
}
