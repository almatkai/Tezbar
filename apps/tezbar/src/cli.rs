//! Preview identity is mandatory until the M11 cutover.
use std::path::{Path, PathBuf};
#[derive(Clone, Debug)]
pub struct Preview {
    pub root: PathBuf,
    pub resources: PathBuf,
    pub show: bool,
    pub theme: String,
    #[cfg(feature = "bench-control")]
    pub control_token: Option<String>,
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
        let mut theme = "system".to_string();
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
                "--profile" | "--resources" | "--theme" => {
                    let flag = &args[i];
                    i += 1;
                    let value = args
                        .get(i)
                        .ok_or_else(|| anyhow::anyhow!("value required for {flag}"))?;
                    match flag.as_str() {
                        "--profile" => root = Some(PathBuf::from(value)),
                        "--resources" => resources = Some(PathBuf::from(value)),
                        _ => theme = value.clone(),
                    }
                }
                "--show" => show = true,
                _ => anyhow::bail!("unknown preview argument: {}", args[i]),
            }
            i += 1;
        }
        anyhow::ensure!(
            ["system", "light", "white", "dark"].contains(&theme.as_str()),
            "theme must be system, light or dark"
        );
        let preview = Self::open(
            &root.ok_or_else(|| anyhow::anyhow!("--profile required"))?,
            &resources.ok_or_else(|| anyhow::anyhow!("--resources required"))?,
            show,
            theme,
        )?;
        #[cfg(feature = "bench-control")]
        let mut preview = preview;
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
        Ok(Self {
            root,
            resources,
            show,
            theme,
            #[cfg(feature = "bench-control")]
            control_token: None,
        })
    }
    pub fn core_paths(&self) -> tezbar_core::common::CorePaths {
        tezbar_core::common::CorePaths {
            data_dir: self.root.join("data"),
            config_dir: self.root.join("config"),
            home_dir: self.root.join("home"),
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
