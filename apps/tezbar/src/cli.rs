//! Preview identity is mandatory until the M11 cutover.
use std::path::{Path, PathBuf};
#[derive(Clone, Debug)]
pub struct Preview {
    pub root: PathBuf,
    pub resources: PathBuf,
    pub show: bool,
    pub allow_desktop_integration: bool,
    /// Second, separate opt-in (requires desktop integration): clipboard history
    /// observation and app/file discovery for this private profile. Off by default.
    pub allow_desktop_services: bool,
    /// Explicit preview key; does not enable scans, clipboard observation or command hotkeys.
    pub launcher_shortcut: Option<String>,
    pub theme: String,
    pub home: Option<PathBuf>,
    #[cfg(feature = "bench-control")]
    pub control_token: Option<String>,
}
/// The signed-in user's home: `HOME` where the platform sets it, `USERPROFILE` on Windows
/// (which does not set `HOME`), so desktop-integrated previews see real user folders.
fn user_home() -> Option<PathBuf> {
    let var = |name| std::env::var_os(name).filter(|v| !v.is_empty());
    #[cfg(windows)]
    let home = var("USERPROFILE").or_else(|| var("HOME"));
    #[cfg(not(windows))]
    let home = var("HOME");
    home.map(PathBuf::from)
}
fn real(path: &Path) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(path.is_absolute(), "absolute path required");
    anyhow::ensure!(
        !std::fs::symlink_metadata(path)?.file_type().is_symlink(),
        "symlink rejected"
    );
    Ok(path.canonicalize()?)
}
fn safe_tree(path: &Path) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let metadata = std::fs::symlink_metadata(entry.path())?;
        anyhow::ensure!(
            !metadata.file_type().is_symlink(),
            "symlink in preview profile: {}",
            entry.path().display()
        );
        if metadata.is_dir() {
            safe_tree(&entry.path())?;
        }
    }
    Ok(())
}
impl Preview {
    pub fn parse(args: &[String]) -> anyhow::Result<Self> {
        let mut root = None;
        let mut resources = None;
        let mut show = false;
        let mut allow_desktop_integration = false;
        let mut allow_desktop_services = false;
        let mut launcher_shortcut = None;
        let mut theme = "system".to_string();
        let mut home = None;
        #[cfg(feature = "bench-control")]
        let mut token_file = None;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                #[cfg(feature = "bench-control")]
                "--control-token-file" => {
                    i += 1;
                    token_file = Some(PathBuf::from(
                        args.get(i)
                            .ok_or_else(|| anyhow::anyhow!("token file required"))?,
                    ));
                }
                "--profile" | "--resources" | "--theme" | "--launcher-shortcut" | "--home" => {
                    let flag = &args[i];
                    i += 1;
                    let value = args
                        .get(i)
                        .ok_or_else(|| anyhow::anyhow!("value required for {flag}"))?;
                    match flag.as_str() {
                        "--profile" => root = Some(PathBuf::from(value)),
                        "--resources" => resources = Some(PathBuf::from(value)),
                        "--home" => home = Some(PathBuf::from(value)),
                        "--launcher-shortcut" => {
                            let shortcut = tezbar_host::desktop::parse_shortcut(value)
                                .map_err(anyhow::Error::msg)?;
                            launcher_shortcut = Some(shortcut.accelerator);
                        }
                        _ => theme = value.clone(),
                    }
                }
                "--show" => show = true,
                "--allow-desktop-integration" => allow_desktop_integration = true,
                "--allow-desktop-services" => allow_desktop_services = true,
                _ => anyhow::bail!("unknown preview argument: {}", args[i]),
            }
            i += 1;
        }
        anyhow::ensure!(
            ["system", "light", "white", "dark"].contains(&theme.as_str()),
            "theme must be system, light or dark"
        );
        let mut preview = Self::open(
            &root.ok_or_else(|| anyhow::anyhow!("--profile required"))?,
            &resources.ok_or_else(|| anyhow::anyhow!("--resources required"))?,
            show,
            theme,
        )?;
        anyhow::ensure!(
            !allow_desktop_services || allow_desktop_integration,
            "--allow-desktop-services requires --allow-desktop-integration"
        );
        preview.allow_desktop_integration = allow_desktop_integration;
        preview.allow_desktop_services = allow_desktop_services;
        preview.launcher_shortcut = launcher_shortcut;
        preview.home = home.or_else(|| {
            if allow_desktop_integration || std::env::var_os("TEZBAR_PREVIEW_USER_HOME").is_some() {
                user_home()
            } else {
                None
            }
        });
        #[cfg(feature = "bench-control")]
        if let Some(path) = token_file {
            let path = real(&path)?;
            anyhow::ensure!(
                path.starts_with(&preview.root),
                "control token must be profile-local"
            );
            preview.control_token = Some(std::fs::read_to_string(path)?.trim().to_string());
        }
        Ok(preview)
    }
    pub fn open(root: &Path, resources: &Path, show: bool, theme: String) -> anyhow::Result<Self> {
        let root = real(root)?;
        let temp = std::env::temp_dir().canonicalize()?;
        anyhow::ensure!(
            root.starts_with(&temp) && root != temp,
            "profile must be child of OS temp"
        );
        safe_tree(&root)?;
        anyhow::ensure!(
            std::fs::read_to_string(root.join(".tezbar-native-preview"))?
                == "com.tezbar.preview.native\n",
            "preview marker required"
        );
        let resources = real(resources)?;
        anyhow::ensure!(
            !resources.starts_with(&root),
            "resources must be separate read-only inputs"
        );
        let manifest = resources.join(".tezbar-native-source");
        anyhow::ensure!(
            !std::fs::symlink_metadata(&manifest)?
                .file_type()
                .is_symlink(),
            "symlink resource manifest rejected"
        );
        anyhow::ensure!(
            std::fs::read_to_string(manifest)?.trim() == env!("TEZBAR_NATIVE_GIT_SHA"),
            "resources source differs from binary"
        );
        for name in ["data", "config", "home", "temp", "logs"] {
            std::fs::create_dir_all(root.join(name))?;
        }
        for sub in ["Desktop", "Documents", "Downloads", "Pictures"] {
            let _ = std::fs::create_dir_all(root.join("home").join(sub));
        }
        Ok(Self {
            root,
            resources,
            show,
            allow_desktop_integration: false,
            allow_desktop_services: false,
            launcher_shortcut: None,
            theme,
            home: None,
            #[cfg(feature = "bench-control")]
            control_token: None,
        })
    }
    pub fn core_paths(&self) -> tezbar_core::common::CorePaths {
        let home_dir = self
            .home
            .clone()
            .unwrap_or_else(|| self.root.join("home"));
        tezbar_core::common::CorePaths {
            data_dir: self.root.join("data"),
            config_dir: self.root.join("config"),
            home_dir,
            temp_dir: self.root.join("temp"),
            resource_dir: self.resources.clone(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, tempfile::TempDir) {
        let p = tempfile::tempdir().unwrap();
        let r = tempfile::tempdir().unwrap();
        std::fs::write(
            p.path().join(".tezbar-native-preview"),
            "com.tezbar.preview.native\n",
        )
        .unwrap();
        std::fs::write(
            r.path().join(".tezbar-native-source"),
            env!("TEZBAR_NATIVE_GIT_SHA"),
        )
        .unwrap();
        (p, r)
    }
    #[::core::prelude::v1::test]
    fn explicit_paths_no_production_defaults() {
        assert!(Preview::parse(&[]).is_err());
        let (p, r) = fixture();
        let p = Preview::open(p.path(), r.path(), false, "system".into()).unwrap();
        let paths = p.core_paths();
        assert!(paths.home_dir.starts_with(&p.root));
        assert_eq!(paths.resource_dir, p.resources);
    }
    #[::core::prelude::v1::test]
    fn desktop_integration_requires_explicit_opt_in() {
        let (p, r) = fixture();
        let mut args = vec![
            "--profile".into(),
            p.path().display().to_string(),
            "--resources".into(),
            r.path().display().to_string(),
        ];
        let preview = Preview::parse(&args).unwrap();
        assert!(!preview.allow_desktop_integration);
        assert!(preview.launcher_shortcut.is_none());
        args.push("--allow-desktop-integration".into());
        let p_desktop = Preview::parse(&args).unwrap();
        assert!(p_desktop.allow_desktop_integration);
        if let Some(user_home) = user_home() {
            assert_eq!(p_desktop.core_paths().home_dir, user_home);
        }
        // Integration alone never enables observers; services are a separate opt-in
        // that is refused without integration.
        assert!(!p_desktop.allow_desktop_services);
        let mut services = args.clone();
        services.push("--allow-desktop-services".into());
        assert!(Preview::parse(&services).unwrap().allow_desktop_services);
        let without_integration: Vec<String> = services
            .into_iter()
            .filter(|a| a != "--allow-desktop-integration")
            .collect();
        assert!(Preview::parse(&without_integration).is_err());
    }
    #[::core::prelude::v1::test]
    fn custom_home_flag_supported() {
        let (p, r) = fixture();
        let custom = tempfile::tempdir().unwrap();
        let args = vec![
            "--profile".into(),
            p.path().display().to_string(),
            "--resources".into(),
            r.path().display().to_string(),
            "--home".into(),
            custom.path().display().to_string(),
        ];
        let preview = Preview::parse(&args).unwrap();
        assert_eq!(preview.core_paths().home_dir, custom.path());
    }
    #[::core::prelude::v1::test]
    fn launcher_hotkey_can_be_enabled_without_desktop_services() {
        let (p, r) = fixture();
        let args = vec![
            "--profile".into(),
            p.path().display().to_string(),
            "--resources".into(),
            r.path().display().to_string(),
            "--launcher-shortcut".into(),
            "Alt+Space".into(),
        ];
        let preview = Preview::parse(&args).unwrap();
        assert!(!preview.allow_desktop_integration);
        assert!(
            preview
                .core_paths()
                .home_dir
                .starts_with(p.path().canonicalize().unwrap())
        );
        assert_eq!(
            tezbar_host::desktop::parse_shortcut(preview.launcher_shortcut.as_ref().unwrap())
                .unwrap()
                .id,
            tezbar_host::desktop::parse_shortcut("Alt+Space")
                .unwrap()
                .id,
        );
        let mut invalid = args;
        *invalid.last_mut().unwrap() = "not-an-accelerator".into();
        assert!(Preview::parse(&invalid).is_err());
    }
    #[cfg(unix)]
    #[::core::prelude::v1::test]
    fn nested_profile_symlink_rejected() {
        let (p, r) = fixture();
        std::fs::create_dir(p.path().join("data")).unwrap();
        std::os::unix::fs::symlink(r.path(), p.path().join("data/escape")).unwrap();
        assert!(Preview::open(p.path(), r.path(), false, "system".into()).is_err());
    }
    #[::core::prelude::v1::test]
    fn resources_source_must_match() {
        let (p, r) = fixture();
        std::fs::write(r.path().join(".tezbar-native-source"), "other").unwrap();
        assert!(Preview::open(p.path(), r.path(), false, "system".into()).is_err());
    }
}
