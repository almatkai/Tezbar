//! Shared admission payload checks, independent of profiles and application launch.
use super::build_metadata::{self, BuildMetadata};
use std::path::Path;

/// Identity is verified by the production wrapper BEFORE these read-only checks.
/// Plist access is injected only to keep fixtures platform-independent.
pub fn verify_payload(
    bundle: &Path,
    version: &str,
    running: Option<&BuildMetadata>,
    mut plist: impl FnMut(&Path, &str) -> Result<String, String>,
) -> Result<(), String> {
    if bundle.file_name().and_then(|n| n.to_str()) != Some("Tezbar.app") {
        return Err("Production requires the established signed bundle".into());
    }
    let contents = bundle.join("Contents");
    for path in [
        bundle.to_path_buf(),
        contents.clone(),
        contents.join("MacOS"),
        contents.join("Resources"),
        contents.join("Info.plist"),
    ] {
        if std::fs::symlink_metadata(path)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Bundle symlink rejected".into());
        }
    }
    let (binary, metadata_sha256) = build_metadata::capture(&contents.join("MacOS/tezbar"))?;
    binary.production(version)?;
    if running.is_some_and(|expected| expected != &binary) {
        return Err("Running executable provenance mismatch".into());
    }
    verify_resources(
        &contents.join("Resources"),
        &binary,
        &metadata_sha256,
        MACOS_REQUIRED,
    )?;
    for (key, expected) in [
        ("CFBundleIdentifier", "com.tezbar.app"),
        ("CFBundleExecutable", "tezbar"),
        ("CFBundleShortVersionString", version),
    ] {
        if plist(&contents.join("Info.plist"), key)?.trim() != expected {
            return Err("Bundle metadata mismatch".into());
        }
    }
    Ok(())
}
const MACOS_REQUIRED: &[&str] = &[
    "dist-backend/extension-worker.js",
    "dist-backend/vec0",
    "dist-backend/libsqlite3.dylib",
    "trayIconTemplate@2x.png",
    "icon.icns",
    "embedding-runtime/embedding-runtime.zip",
    "licenses/raycast-extensions-MIT.txt",
    "native/axhelper/axhelper",
    "native/screenocr/screenocr-helper",
    "native/color-picker/color-picker-helper",
    "native/image-colors/image-colors-helper",
];
/// The Windows NSIS layout (scripts/native/release.mjs WINDOWS_ROOTS): no macOS helpers.
const WINDOWS_REQUIRED: &[&str] = &[
    "dist-backend/extension-worker.js",
    "dist-backend/vec0",
    "trayIconTemplate@2x.png",
    "icon.ico",
    "licenses/raycast-extensions-MIT.txt",
];
/// Installed Windows app: `<install>\tezbar.exe` with its resources and provenance beside it.
/// Identity (Authenticode) is verified by the production wrapper when a pin exists.
pub fn verify_windows_install(
    install: &Path,
    version: &str,
    running: Option<&BuildMetadata>,
) -> Result<(), String> {
    let executable = install.join("tezbar.exe");
    for path in [install, executable.as_path()] {
        if std::fs::symlink_metadata(path)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Installation reparse point rejected".into());
        }
    }
    let (binary, metadata_sha256) = build_metadata::capture(&executable)?;
    binary.production(version)?;
    if running.is_some_and(|expected| expected != &binary) {
        return Err("Running executable provenance mismatch".into());
    }
    verify_resources(install, &binary, &metadata_sha256, WINDOWS_REQUIRED)
}
fn verify_resources(
    root: &Path,
    binary: &BuildMetadata,
    metadata_sha256: &str,
    required: &[&str],
) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let manifest = root.join(".tezbar-native-production.json");
    let meta =
        std::fs::symlink_metadata(&manifest).map_err(|_| "Native resource provenance missing")?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 16 * 1024 * 1024 {
        return Err("Invalid resource provenance".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(manifest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if value["schema"] != 2
        || value["binaryMetadataSha256"] != metadata_sha256
        || value["version"] != binary.version
        || value["sourceCommit"] != binary.source_commit
        || value["benchControl"] != binary.bench_control
        || value["runtimeShaders"] != binary.runtime_shaders
        || value["gpuiRev"] != build_metadata::GPUI_REV
        || value["rustVersion"] != build_metadata::RUST_VERSION
        || value["dirty"] != false
    {
        return Err("Native resource provenance mismatch".into());
    }
    let files = value["files"]
        .as_array()
        .filter(|f| !f.is_empty() && f.len() <= 100000)
        .ok_or("Missing resource inventory")?;
    for key in required {
        if !files.iter().any(|f| f["destination"] == *key) {
            return Err("Required native resource missing".into());
        }
    }
    let mut names = std::collections::BTreeSet::new();
    for pin in files {
        let name = pin["destination"].as_str().ok_or("Invalid resource path")?;
        if !names.insert(name.to_lowercase())
            || name.contains(['\\', '\0'])
            || !Path::new(name)
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
        {
            return Err("Unsafe resource path".into());
        }
        let mut path = root.to_path_buf();
        for component in Path::new(name).components() {
            path.push(component);
            if std::fs::symlink_metadata(&path)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
            {
                return Err("Resource symlink rejected".into());
            }
        }
        let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len()
            != pin["bytes"].as_u64().ok_or("Missing resource size")?
        {
            return Err("Resource size mismatch".into());
        }
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        if format!("{:x}", hash.finalize())
            != pin["sha256"].as_str().ok_or("Missing resource hash")?
        {
            return Err("Resource hash mismatch".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    const VERSION: &str = "9.8.7-beta.1";
    const RESOURCES: &[&str] = &[
        "dist-backend/extension-worker.js",
        "dist-backend/vec0",
        "dist-backend/libsqlite3.dylib",
        "trayIconTemplate@2x.png",
        "icon.icns",
        "embedding-runtime/embedding-runtime.zip",
        "licenses/raycast-extensions-MIT.txt",
        "native/axhelper/axhelper",
        "native/screenocr/screenocr-helper",
        "native/color-picker/color-picker-helper",
        "native/image-colors/image-colors-helper",
    ];
    fn metadata() -> Value {
        json!({"schema":2,"version":VERSION,"sourceCommit":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "dirty":false,"benchControl":false,"runtimeShaders":true,
            "gpuiRev":build_metadata::GPUI_REV,"rustVersion":"1.98.1"})
    }
    fn fixture(bundle: &Path, binary: Value) {
        let contents = bundle.join("Contents");
        std::fs::create_dir_all(contents.join("MacOS")).unwrap();
        std::fs::write(contents.join("Info.plist"), "private plist fixture").unwrap();
        let record = binary.to_string();
        std::fs::write(
            contents.join("MacOS/tezbar"),
            format!("dummy\0TEZBAR_NATIVE_BUILD_V2\0{record}\0TEZBAR_NATIVE_BUILD_END\0"),
        )
        .unwrap();
        let files: Vec<_> = RESOURCES.iter().map(|name| {
            let path = contents.join("Resources").join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path,b"fixture").unwrap();
            json!({"destination":name,"bytes":7,"sha256":format!("{:x}",Sha256::digest(b"fixture"))})
        }).collect();
        let mut manifest = binary;
        manifest["binaryMetadataSha256"] =
            json!(format!("{:x}", Sha256::digest(record.as_bytes())));
        manifest["files"] = json!(files);
        std::fs::write(
            contents.join("Resources/.tezbar-native-production.json"),
            manifest.to_string(),
        )
        .unwrap();
    }
    fn plist(_: &Path, key: &str) -> Result<String, String> {
        Ok(match key {
            "CFBundleIdentifier" => "com.tezbar.app",
            "CFBundleExecutable" => "tezbar",
            _ => VERSION,
        }
        .into())
    }
    #[test]
    fn staged_new_release_uses_its_own_source_and_version_never_old_process_constants() {
        let temp = tempfile::tempdir().unwrap();
        let bundle = temp.path().join("Tezbar.app");
        fixture(&bundle, metadata());
        assert!(verify_payload(&bundle, VERSION, None, plist).is_ok());
        assert!(verify_payload(&bundle, "0.0.1", None, plist).is_err());
        let mut old: BuildMetadata = serde_json::from_value(metadata()).unwrap();
        old.source_commit = "a".repeat(40);
        assert!(verify_payload(&bundle, VERSION, Some(&old), plist).is_err());
        assert!(verify_payload(&bundle, VERSION, None, |_, _| Ok("wrong".into())).is_err());
    }
    #[test]
    fn dirty_bench_missing_record_and_resource_tamper_refuse_before_swap_preserving_old_and_backup()
    {
        for case in [
            "dirty",
            "benchControl",
            "sourceCommit",
            "gpuiRev",
            "rustVersion",
            "missing-record",
            "resource",
            "missing-resource",
            "manifest-hash",
            "unchecked-sidecar",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let bundle = temp.path().join("stage/Tezbar.app");
            let old = temp.path().join("installed/Tezbar.app");
            let rollback = temp.path().join("tezbar-previous-fixture/Tezbar.app");
            for path in [&old, &rollback] {
                std::fs::create_dir_all(path).unwrap();
                std::fs::write(path.join("sentinel"), b"keep old").unwrap();
            }
            let mut binary = metadata();
            match case {
                "dirty" | "benchControl" => binary[case] = json!(true),
                "sourceCommit" | "gpuiRev" | "rustVersion" => binary[case] = json!("wrong"),
                _ => (),
            }
            fixture(&bundle, binary);
            let resources = bundle.join("Contents/Resources");
            match case {
                "missing-record" => {
                    std::fs::write(bundle.join("Contents/MacOS/tezbar"), b"no metadata").unwrap()
                }
                "resource" => std::fs::write(resources.join(RESOURCES[0]), b"changed").unwrap(),
                "missing-resource" => std::fs::remove_file(resources.join(RESOURCES[0])).unwrap(),
                "manifest-hash" | "unchecked-sidecar" => {
                    let path = resources.join(".tezbar-native-production.json");
                    let mut manifest: Value =
                        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                    manifest[if case == "manifest-hash" {
                        "binaryMetadataSha256"
                    } else {
                        "sourceCommit"
                    }] = json!("a".repeat(64));
                    std::fs::write(path, manifest.to_string()).unwrap();
                }
                _ => (),
            }
            let destructive = std::cell::Cell::new(false);
            let result =
                verify_payload(&bundle, VERSION, None, plist).map(|_| destructive.set(true));
            assert!(result.is_err(), "{case}");
            assert!(!destructive.get(), "{case}");
            for path in [&old, &rollback] {
                assert_eq!(std::fs::read(path.join("sentinel")).unwrap(), b"keep old");
            }
        }
    }
    #[test]
    fn compiled_metal_production_is_valid_and_manifest_must_match_actual_shader_mode() {
        let temp = tempfile::tempdir().unwrap();
        let bundle = temp.path().join("Tezbar.app");
        let mut binary = metadata();
        binary["runtimeShaders"] = json!(false);
        fixture(&bundle, binary);
        assert!(verify_payload(&bundle, VERSION, None, plist).is_ok());
        let path = bundle.join("Contents/Resources/.tezbar-native-production.json");
        let mut manifest: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        manifest["runtimeShaders"] = json!(true);
        std::fs::write(path, manifest.to_string()).unwrap();
        assert!(verify_payload(&bundle, VERSION, None, plist).is_err());
    }
    fn windows_fixture(install: &Path, binary: Value) {
        std::fs::create_dir_all(install).unwrap();
        let record = binary.to_string();
        std::fs::write(
            install.join("tezbar.exe"),
            format!("MZ\0TEZBAR_NATIVE_BUILD_V2\0{record}\0TEZBAR_NATIVE_BUILD_END\0"),
        )
        .unwrap();
        let files: Vec<_> = super::WINDOWS_REQUIRED.iter().map(|name| {
            let path = install.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"fixture").unwrap();
            json!({"destination":name,"bytes":7,"sha256":format!("{:x}",Sha256::digest(b"fixture"))})
        }).collect();
        let mut manifest = binary;
        manifest["binaryMetadataSha256"] =
            json!(format!("{:x}", Sha256::digest(record.as_bytes())));
        manifest["files"] = json!(files);
        std::fs::write(
            install.join(".tezbar-native-production.json"),
            manifest.to_string(),
        )
        .unwrap();
    }
    #[test]
    fn windows_install_is_admitted_only_with_exact_provenance_and_resources() {
        let temp = tempfile::tempdir().unwrap();
        let install = temp.path().join("Tezbar");
        windows_fixture(&install, metadata());
        // The NSIS uninstaller and other unpinned files beside the app are not resources.
        std::fs::write(install.join("uninstall.exe"), b"MZ uninstaller").unwrap();
        assert!(verify_windows_install(&install, VERSION, None).is_ok());
        assert!(verify_windows_install(&install, "0.0.1", None).is_err());
        let mut old: BuildMetadata = serde_json::from_value(metadata()).unwrap();
        old.source_commit = "a".repeat(40);
        assert!(verify_windows_install(&install, VERSION, Some(&old)).is_err());
        for case in ["dirty", "resource", "missing-resource", "missing-exe"] {
            let temp = tempfile::tempdir().unwrap();
            let install = temp.path().join("Tezbar");
            let mut binary = metadata();
            if case == "dirty" {
                binary["dirty"] = json!(true);
            }
            windows_fixture(&install, binary);
            match case {
                "resource" => std::fs::write(install.join("dist-backend/vec0"), b"changed").unwrap(),
                "missing-resource" => std::fs::remove_file(install.join("icon.ico")).unwrap(),
                "missing-exe" => std::fs::remove_file(install.join("tezbar.exe")).unwrap(),
                _ => (),
            }
            assert!(verify_windows_install(&install, VERSION, None).is_err(), "{case}");
        }
    }
    #[test]
    fn ambiguous_and_truncated_binary_records_are_not_attested() {
        let temp = tempfile::tempdir().unwrap();
        let bundle = temp.path().join("Tezbar.app");
        fixture(&bundle, metadata());
        let exe = bundle.join("Contents/MacOS/tezbar");
        let bytes = std::fs::read(&exe).unwrap();
        std::fs::write(&exe, [bytes.clone(), bytes].concat()).unwrap();
        assert!(build_metadata::capture(&exe).is_err());
        std::fs::write(&exe, b"\0TEZBAR_NATIVE_BUILD_V2\0{\"schema\":2").unwrap();
        assert!(build_metadata::capture(&exe).is_err());
    }
}
