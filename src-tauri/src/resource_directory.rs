use std::path::{Path, PathBuf};

/// Keep resource lookup lazy so selecting a bundle never probes the source tree.
pub(super) fn resolve<E>(
    debug: bool,
    dev: bool,
    executable: &Path,
    source: &Path,
    bundled: impl FnOnce() -> Result<PathBuf, E>,
) -> Result<PathBuf, E> {
    // Tauri's production protocol also covers packaged debug builds on Windows
    // and Linux. Recognize macOS bundles independently: a dev-protocol binary
    // can be copied into an .app (including the QA wrapper's renamed binary).
    let macos_bundle = executable
        .parent()
        .filter(|p| p.ends_with("Contents/MacOS"))
        .and_then(|p| p.parent()?.parent()?.extension())
        .is_some_and(|extension| extension == "app");
    if debug && dev && !macos_bundle {
        Ok(source.to_path_buf())
    } else {
        bundled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(debug: bool, dev: bool, executable: &Path, bundle: &Path) -> PathBuf {
        resolve::<()>(
            debug,
            dev,
            executable,
            Path::new("/source on Desktop"),
            || Ok(bundle.to_path_buf()),
        )
        .unwrap()
    }

    #[test]
    fn packaged_debug_app_with_spaces_uses_bundled_resources_even_with_dev_protocol() {
        let root = tempfile::tempdir().unwrap();
        let contents = root.path().join("Tezbar QA.app/Contents");
        let resources = contents.join("Resources");
        std::fs::create_dir_all(resources.join("dist-backend")).unwrap();
        std::fs::write(resources.join("dist-backend/vec0"), b"bundled vec").unwrap();
        let actual = resolved(
            true,
            true,
            &contents.join("MacOS/tezbar-native"),
            &resources,
        );
        assert_eq!(
            std::fs::read(actual.join("dist-backend/vec0")).unwrap(),
            b"bundled vec"
        );
        assert_eq!(actual, resources);
    }

    #[test]
    fn missing_packaged_vec_does_not_fall_back_to_source() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("Desktop/source");
        std::fs::create_dir_all(source.join("dist-backend")).unwrap();
        std::fs::write(source.join("dist-backend/vec0"), b"source vec").unwrap();
        let contents = root.path().join("Tezbar QA.app/Contents");
        let resources = contents.join("Resources");
        std::fs::create_dir_all(&resources).unwrap();
        let actual = resolve::<()>(
            true,
            true,
            &contents.join("MacOS/tezbar-native"),
            &source,
            || Ok(resources.clone()),
        )
        .unwrap();
        assert_eq!(actual, resources);
        assert!(!actual.join("dist-backend/vec0").exists());
    }

    #[test]
    fn missing_bundle_directory_propagates_resolution_error() {
        let actual = resolve(
            true,
            true,
            Path::new("/tmp/Tezbar QA.app/Contents/MacOS/tezbar-native"),
            Path::new("/source on Desktop"),
            || Err("missing Resources"),
        );
        assert_eq!(actual, Err("missing Resources"));
    }

    #[test]
    fn unbundled_debug_dev_preserves_source_and_does_not_resolve_bundle() {
        for executable in [
            "/repo/src-tauri/target/debug/tezbar",
            "/repo/custom target/debug/tezbar",
            "/repo/Examples.app/src-tauri/target/debug/tezbar",
            "/repo/Tezbar.app/Contents/Other/tezbar",
            "/repo/NotAnApp/Contents/MacOS/tezbar",
        ] {
            let actual = resolve::<()>(
                true,
                true,
                Path::new(executable),
                Path::new("/source on Desktop"),
                || panic!("unbundled dev must not resolve bundle resources"),
            );
            assert_eq!(actual.unwrap(), Path::new("/source on Desktop"));
        }
    }

    #[test]
    fn packaged_debug_and_release_use_platform_resource_resolver() {
        // Tauri's production protocol applies to packaged debug builds too.
        // Paths use the host's syntax; Windows native syntax is covered below.
        for (executable, resources) in [
            (
                "/Applications/Tezbar.app/Contents/MacOS/tezbar",
                "/Applications/Tezbar.app/Contents/Resources",
            ),
            ("/Program Files/Tezbar/tezbar.exe", "/Program Files/Tezbar"),
            ("/usr/bin/tezbar", "/usr/lib/Tezbar"),
            (
                "/tmp/.mount_Tezbar/usr/bin/tezbar",
                "/tmp/.mount_Tezbar/usr/lib/Tezbar",
            ),
        ] {
            for (debug, dev) in [(true, false), (false, false), (false, true)] {
                assert_eq!(
                    resolved(debug, dev, Path::new(executable), Path::new(resources)),
                    Path::new(resources)
                );
            }
        }
    }

    #[test]
    fn release_preserves_resource_errors() {
        for dev in [false, true] {
            assert_eq!(
                resolve(false, dev, Path::new(""), Path::new("/source"), || {
                    Err("platform resource error")
                }),
                Err("platform resource error")
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_native_paths() {
        let resources = Path::new(r"C:\Program Files\Tezbar");
        assert_eq!(
            resolved(true, false, &resources.join("tezbar.exe"), resources),
            resources
        );
        assert_eq!(
            resolved(
                true,
                true,
                Path::new(r"C:\repo\target\debug\tezbar.exe"),
                resources
            ),
            Path::new("/source on Desktop")
        );
    }
}
