use std::path::Path;

/// Tauri's macOS installer creates backup/extraction directories using tempfile's
/// process default. A launcher can inherit a short-lived TMPDIR which disappears
/// while Tezbar is running. Use an app-owned cache directory instead. Do not change
/// environment variables in a multithreaded application.
pub fn prepare_temp_dir(directory: &Path) -> Result<(), String> {
    use std::os::unix::fs::DirBuilderExt;

    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)
        .map_err(|error| format!("Cannot create update temporary directory: {error}"))?;
    // Check writability before downloading. Recreate/probe on every attempt in
    // case a cache cleaner removed the directory since the previous update.
    tempfile::Builder::new()
        .prefix("tezbar_update_probe")
        .tempdir_in(directory)
        .map_err(|error| format!("Cannot write to update temporary directory: {error}"))?;

    match tempfile::env::override_temp_dir(directory) {
        Ok(()) => Ok(()),
        Err(existing) if existing == directory => Ok(()),
        Err(existing) => Err(format!(
            "Update temporary directory was already configured as {}",
            existing.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::prepare_temp_dir;
    use std::path::Path;

    #[test]
    fn updater_handles_missing_inherited_temp_dir() {
        run_isolated_check(false);
    }

    #[test]
    fn updater_handles_temp_dir_deleted_after_launch() {
        run_isolated_check(true);
    }

    fn run_isolated_check(remove_after_launch: bool) {
        let fixture = tempfile::tempdir().unwrap();
        let inherited = fixture.path().join("agent-run");
        if remove_after_launch {
            std::fs::create_dir(&inherited).unwrap();
        }
        // The tempfile override is process-global. Run in a fresh process so tests
        // do not mutate the test runner's environment or depend on execution order.
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "updater_temp::tests::isolated_temp_dir_check",
                "--nocapture",
            ])
            .env("TMPDIR", &inherited)
            .env("TEZBAR_TEST_UPDATE_CACHE", fixture.path().join("cache"))
            .env(
                "TEZBAR_TEST_REMOVE_TMPDIR",
                if remove_after_launch { "1" } else { "0" },
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn unavailable_cache_returns_an_error_without_changing_temp_directory() {
        let fixture = tempfile::tempdir().unwrap();
        let blocker = fixture.path().join("not-a-directory");
        std::fs::write(&blocker, "keep").unwrap();
        let previous = tempfile::env::temp_dir();
        assert!(prepare_temp_dir(&blocker.join("updater-temp")).is_err());
        assert_eq!(tempfile::env::temp_dir(), previous);
        assert_eq!(std::fs::read_to_string(blocker).unwrap(), "keep");
    }

    #[test]
    fn isolated_temp_dir_check() {
        let Some(cache) = std::env::var_os("TEZBAR_TEST_UPDATE_CACHE") else {
            return;
        };
        if std::env::var("TEZBAR_TEST_REMOVE_TMPDIR").as_deref() == Ok("1") {
            std::fs::remove_dir_all(std::env::temp_dir()).unwrap();
        }
        // These are the exact constructors used by Tauri's macOS installer.
        let error = tempfile::Builder::new()
            .prefix("tauri_current_app")
            .tempdir()
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        println!("Reproduced installer failure: {error}");

        let inherited = std::env::var_os("TMPDIR");
        prepare_temp_dir(Path::new(&cache)).unwrap();
        assert_eq!(std::env::var_os("TMPDIR"), inherited);
        assert_ne!(std::env::temp_dir(), tempfile::env::temp_dir());
        for prefix in ["tauri_current_app", "tauri_updated_app"] {
            let directory = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
            assert!(directory.path().starts_with(Path::new(&cache)));
        }
        // Repeated installs must also work if the cache has been cleared.
        std::fs::remove_dir_all(&cache).unwrap();
        prepare_temp_dir(Path::new(&cache)).unwrap();
        assert!(tempfile::tempdir()
            .unwrap()
            .path()
            .starts_with(Path::new(&cache)));
    }
}
