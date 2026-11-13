//! M11 pure production composition draft. Not exported from App or Preview CLI.
//! This is a plan, never permission to bypass release identity or preview isolation.
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tezbar_core::common::CorePaths;
use tezbar_host::paths::{self, Environment, Platform, Profile};

mod build_metadata;
mod bundle_validation;

/// Also consumed by --version: a live static-record reference independent of
/// production admission's dirty/bench refusal and linker reachability decisions.
pub fn current_build_metadata() -> serde_json::Value {
    serde_json::to_value(build_metadata::current()).expect("compile-time native metadata")
}

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
    // Accept a unique direct or EBWebView Default profile, never prefer one when
    // both exist. Stage O-LS must still prove the installed app's actual layout.
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
    // The macOS plan uses Unix-absolute fixture paths; `windows_tests` covers Windows.
    #[cfg(unix)]
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
        let running = build_metadata::current();
        running.production(env!("CARGO_PKG_VERSION"))?;
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
            verify_bundle(bundle, env!("CARGO_PKG_VERSION"), Some(&running))?;
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

/// Staged updates use the NEW release version and the staged binary's own static
/// record. No staged executable is launched; the old process's SHA is irrelevant.
#[cfg(target_os = "macos")]
pub(super) fn verify_staged(bundle: &Path, version: &str) -> Result<(), String> {
    verify_bundle(bundle, version, None)
}
#[cfg(target_os = "macos")]
fn verify_bundle(
    bundle: &Path,
    version: &str,
    running: Option<&build_metadata::BuildMetadata>,
) -> Result<(), String> {
    tezbar_update::native::verify_macos_bundle_identity(bundle)?;
    bundle_validation::verify_payload(bundle, version, running, |path, key| {
        let output = std::process::Command::new("/usr/libexec/PlistBuddy")
            .args(["-c", &format!("Print :{key}")])
            .arg(path)
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err("Bundle metadata unavailable".into());
        }
        String::from_utf8(output.stdout).map_err(|e| e.to_string())
    })
}
