//! M11 pure production composition draft. Not exported from App or Preview CLI.
//! This is a plan, never permission to bypass release identity or preview isolation.
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tezbar_core::common::CorePaths;
use tezbar_host::paths::{self, Environment, Platform, Profile};

pub const IDENTITY: &str = "com.tezbar.app";
pub const SHUTDOWN_LIMIT: Duration = Duration::from_secs(8);
#[derive(Clone)]
pub struct LaunchPlan {
    pub profile: Profile,
    pub core: CorePaths,
    pub ui_state: PathBuf,
    pub layouts: PathBuf,
    pub legacy_webkit_root: Option<PathBuf>,
    pub legacy_webview2_root: Option<PathBuf>,
    pub instance_identity: &'static str,
    pub log_name: &'static str,
}
/// Identity admission is deliberately not representable by an env flag, IPC or CLI argument.
/// Parent must verify the bundle using the existing signing policy before consuming a plan.
/// Pure planning can be used in fixtures with no identity or filesystem side effects.
pub fn plan(environment: Environment, executable: &Path) -> Result<LaunchPlan, &'static str> {
    let home = environment.home.clone();
    let platform = environment.platform;
    if !executable.is_absolute()
        || executable.file_name().and_then(|n| n.to_str())
            != Some(match platform {
                Platform::Mac => "tezbar",
                Platform::Windows => "tezbar.exe",
            })
    {
        return Err("Production executable must have the established product name");
    }
    let resources = match platform {
        Platform::Mac => {
            let macos = executable.parent().ok_or("Missing bundle")?;
            let contents = macos.parent().ok_or("Missing bundle")?;
            let bundle = contents.parent().ok_or("Missing bundle")?;
            if macos.file_name().and_then(|n| n.to_str()) != Some("MacOS")
                || contents.file_name().and_then(|n| n.to_str()) != Some("Contents")
                || bundle.file_name().and_then(|n| n.to_str()) != Some("Tezbar.app")
            {
                return Err("Production bundle layout changed");
            }
            contents.join("Resources")
        }
        Platform::Windows => executable
            .parent()
            .ok_or("Missing installation directory")?
            .to_path_buf(),
    };
    if environment.resources != resources {
        return Err("Production resources must belong to the executable installation");
    }
    let legacy_webkit_root = matches!(platform, Platform::Mac).then(|| {
        home.join("Library/WebKit")
            .join(IDENTITY)
            .join("WebsiteData/Default")
    });
    let profile = paths::resolve(environment, IDENTITY).map_err(|_| "Profile resolution failed")?;
    // Tauri manager/webview.rs supplies app_local_data_dir as WebView2 user data folder.
    // In that case Default lives directly under data; EBWebView is only a fallback candidate,
    // never silently preferred. Stage O-LS must prove the old installed app's actual layout.
    let legacy_webview2_root = matches!(platform, Platform::Windows).then(|| profile.data.clone());
    let core = CorePaths {
        data_dir: profile.data.clone(),
        config_dir: profile.config.clone(),
        resource_dir: profile.resources.clone(),
        temp_dir: profile.temp.clone(),
        home_dir: home,
    };
    Ok(LaunchPlan {
        ui_state: profile.config.join("ui-state-v1.json"),
        layouts: profile.config.join("launcher-layouts-v1.json"),
        profile,
        core,
        legacy_webkit_root,
        legacy_webview2_root,
        instance_identity: IDENTITY,
        log_name: "tezbar",
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    PrivateChildSupervisorDispatch,
    NativeV2CliDispatch,
    VerifyReleaseIdentityAndResources,
    ClaimSingleInstance,
    OpenExistingLogger,
    BindExistingRuntimeToGpui,
    PrepareLegacyStateBeforeHydration,
    CommitThroughExistingUiOwner,
    StartCore,
    OpenHiddenLauncher,
    RegisterPrimaryTrayAndConfiguredHotkey,
    RestoreMainWindow,
    StartOneUpdaterActor,
}
pub const ORDER: [Step; 13] = [
    Step::PrivateChildSupervisorDispatch,
    Step::NativeV2CliDispatch,
    Step::VerifyReleaseIdentityAndResources,
    Step::ClaimSingleInstance,
    Step::OpenExistingLogger,
    Step::BindExistingRuntimeToGpui,
    Step::PrepareLegacyStateBeforeHydration,
    Step::CommitThroughExistingUiOwner,
    Step::StartCore,
    Step::OpenHiddenLauncher,
    Step::RegisterPrimaryTrayAndConfiguredHotkey,
    Step::RestoreMainWindow,
    Step::StartOneUpdaterActor,
];
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn production_paths_and_shutdown_are_exact_without_reading_a_profile() {
        let root = PathBuf::from("/fixture");
        let result = plan(
            Environment {
                platform: Platform::Mac,
                home: root.join("home"),
                local_data: root.join("local"),
                cache: root.join("home/Library/Caches"),
                resources: root.join("Tezbar.app/Contents/Resources"),
                temp: root.join("temp"),
                data_override: None,
                config_override: None,
            },
            &root.join("Tezbar.app/Contents/MacOS/tezbar"),
        )
        .unwrap();
        assert_eq!(result.ui_state, root.join("home/.openray/ui-state-v1.json"));
        assert_eq!(
            result.profile.data,
            root.join("home/Library/Application Support/com.tezbar.app")
        );
        assert_eq!(result.instance_identity, "com.tezbar.app");
        assert_eq!(SHUTDOWN_LIMIT.as_secs(), 8);
    }
}

#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::*;
    #[test]
    fn windows_local_data_cache_logs_resources_and_config_are_byte_identical() {
        let home = PathBuf::from(r"C:\fixture\user");
        let local = home.join(r"AppData\Local");
        let install = local.join("Tezbar");
        let plan = plan(
            Environment {
                platform: Platform::Windows,
                home: home.clone(),
                local_data: local.clone(),
                cache: local.clone(),
                resources: install.clone(),
                temp: local.join("Temp"),
                data_override: None,
                config_override: None,
            },
            &install.join("tezbar.exe"),
        )
        .unwrap();
        assert_eq!(plan.core.data_dir, local.join("com.tezbar.app"));
        assert_eq!(plan.profile.cache, local.join("com.tezbar.app"));
        assert_eq!(plan.profile.logs, local.join(r"com.tezbar.app\logs"));
        assert_eq!(plan.ui_state, home.join(r".openray\ui-state-v1.json"));
        assert_eq!(plan.profile.resources, install);
    }
}
/// Opaque capability: no env, IPC, or feature can manufacture admission.
pub struct Admission {
    executable: PathBuf,
}
impl Admission {
    pub fn current() -> Result<Self, String> {
        if env!("TEZBAR_NATIVE_DIRTY") != "false" {
            return Err("Production requires a clean source build".into());
        }
        if cfg!(feature = "bench-control") {
            return Err("Production cannot enable bench control".into());
        }
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        #[cfg(target_os = "macos")]
        {
            let contents = executable
                .parent()
                .and_then(Path::parent)
                .ok_or("Missing app bundle")?;
            let bundle = contents.parent().ok_or("Missing app bundle")?;
            if executable.file_name().and_then(|n| n.to_str()) != Some("tezbar")
                || bundle.file_name().and_then(|n| n.to_str()) != Some("Tezbar.app")
            {
                return Err("Production requires the established signed bundle".into());
            }
            tezbar_update::native::verify_macos_bundle_identity(bundle)?;
            verify_resources(&contents.join("Resources"))?;
            for (key, expected) in [
                ("CFBundleIdentifier", IDENTITY),
                ("CFBundleExecutable", "tezbar"),
                ("CFBundleShortVersionString", env!("CARGO_PKG_VERSION")),
            ] {
                let output = std::process::Command::new("/usr/libexec/PlistBuddy")
                    .args(["-c", &format!("Print :{key}")])
                    .arg(contents.join("Info.plist"))
                    .output()
                    .map_err(|e| e.to_string())?;
                if !output.status.success()
                    || String::from_utf8_lossy(&output.stdout).trim() != expected
                {
                    return Err("Bundle metadata mismatch".into());
                }
            }
            Ok(Self { executable })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = executable;
            Err("Production admission awaits Windows installer and storage validation".into())
        }
    }
    pub(super) fn plan(self) -> anyhow::Result<LaunchPlan> {
        // Consult production environment only after signature/resource admission.
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("Home unavailable"))?;
        let resources = self
            .executable
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| anyhow::anyhow!("Missing bundle"))?
            .join("Resources");
        plan(
            Environment {
                platform: Platform::Mac,
                local_data: home.join("Library/Application Support"),
                cache: home.join("Library/Caches"),
                home,
                resources,
                temp: std::env::temp_dir(),
                data_override: std::env::var_os("TEZBAR_DATA_DIR").map(PathBuf::from),
                config_override: std::env::var_os("TEZBAR_CONFIG_DIR").map(PathBuf::from),
            },
            &self.executable,
        )
        .map_err(anyhow::Error::msg)
    }
}
fn verify_resources(root: &Path) -> Result<(), String> {
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
    if value["sourceCommit"] != env!("TEZBAR_NATIVE_GIT_SHA")
        || value["gpuiRev"] != "c84af5b99cbc366d675064e52b23664a015b6e35"
        || value["rustVersion"] != "1.98.1"
        || value["dirty"] != false
    {
        return Err("Native resource provenance mismatch".into());
    }
    let files = value["files"]
        .as_array()
        .filter(|f| !f.is_empty() && f.len() <= 100000)
        .ok_or("Missing resource inventory")?;
    for key in [
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
    ] {
        if !files.iter().any(|f| f["destination"] == key) {
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
mod admission_tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn production_resource_inventory_rejects_missing_hash_tamper_and_preview_provenance() {
        let d = tempfile::tempdir().unwrap();
        assert!(verify_resources(d.path()).is_err());
        let names = [
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
        let files:Vec<_>=names.iter().map(|n| {let path=d.path().join(n);std::fs::create_dir_all(path.parent().unwrap()).unwrap();std::fs::write(path,b"fixture").unwrap();serde_json::json!({"destination":n,"bytes":7,"sha256":format!("{:x}",Sha256::digest(b"fixture"))})}).collect();
        let mut manifest = serde_json::json!({"sourceCommit":env!("TEZBAR_NATIVE_GIT_SHA"),"gpuiRev":"c84af5b99cbc366d675064e52b23664a015b6e35","rustVersion":"1.98.1","dirty":false,"files":files});
        let path = d.path().join(".tezbar-native-production.json");
        std::fs::write(&path, manifest.to_string()).unwrap();
        assert!(verify_resources(d.path()).is_ok());
        std::fs::write(d.path().join(names[0]), b"tampered").unwrap();
        assert!(verify_resources(d.path()).is_err());
        std::fs::write(d.path().join(names[0]), b"fixture").unwrap();
        manifest["dirty"] = serde_json::json!(true);
        std::fs::write(path, manifest.to_string()).unwrap();
        assert!(verify_resources(d.path()).is_err());
    }
}
