//! Bounded display invalidation only, not an integrity verdict. Never hash
//! executable/inventory bytes here. Legacy build trees are not traversed.
use super::*;
use crate::native_core::extensions::command_list::{Inputs, MAX_INPUTS};
use std::{collections::BTreeMap, fs, io::Read};

struct Probe(BTreeMap<PathBuf, String>);
impl Probe {
    fn path(&mut self, path: &Path) -> Result<(), String> {
        if self.0.contains_key(path) {
            return Ok(());
        }
        if self.0.len() >= MAX_INPUTS {
            return Err("Command-list input limit".into());
        }
        let mut identity = String::new();
        for metadata in [fs::symlink_metadata(path), fs::metadata(path)] {
            match metadata {
                Ok(m) => {
                    use std::fmt::Write;
                    write!(
                        identity,
                        "{:?}/{}/{:?}/{:?}/{};",
                        m.file_type(),
                        m.len(),
                        m.modified().ok(),
                        m.created().ok(),
                        m.permissions().readonly()
                    )
                    .unwrap();
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::MetadataExt;
                        write!(
                            identity,
                            "{}/{}/{}/{}/{}/{};",
                            m.dev(),
                            m.ino(),
                            m.mode(),
                            m.nlink(),
                            m.ctime(),
                            m.ctime_nsec()
                        )
                        .unwrap();
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                    ) =>
                {
                    identity.push_str("missing;")
                }
                Err(e) => return Err(e.to_string()),
            }
        }
        self.0.insert(path.into(), identity);
        Ok(())
    }
    fn json(&mut self, path: &Path) -> Result<Value, String> {
        self.path(path)?;
        let mut file = match fs::File::open(path) {
            Ok(file) => file,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                return Ok(Value::Null)
            }
            Err(e) => return Err(e.to_string()),
        };
        // Small manifests/active pointers only. Oversize input bypasses cache;
        // the original registry remains the authority for discovery behavior.
        let mut bytes = Vec::new();
        file.by_ref()
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("Command-list manifest limit".into());
        }
        self.0
            .get_mut(path)
            .unwrap()
            .push_str(&format!("{:x}", Sha256::digest(&bytes)));
        Ok(serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }
    fn icons(&mut self, extension: &Installed) -> Result<(), String> {
        let mut names = vec![fallback(&extension.manifest, "icon", "icon.png")];
        for cmd in extension.manifest["commands"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(name) = cmd["icon"].as_str().or_else(|| cmd["iconPath"].as_str()) {
                names.push(name);
            }
        }
        for name in names {
            if name.starts_with("http://") || name.starts_with("https://") {
                continue;
            }
            let relative = Path::new(name.strip_prefix("./").unwrap_or(name));
            if relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            {
                continue;
            }
            for candidate in icon_candidates(extension, relative) {
                self.path(&candidate)?;
                // Include missing candidate parents and fallback legacy roots,
                // not unrelated ancestors such as /tmp or the user home.
                let minimum = extension.path.components().count();
                for parent in candidate.ancestors().skip(1) {
                    if parent.components().count() < minimum {
                        break;
                    }
                    self.path(parent)?;
                }
            }
        }
        Ok(())
    }
}

pub(in crate::native_core::extensions) fn command_inputs(
    ctx: &CoreContext,
) -> Result<Inputs, String> {
    let roots = roots(ctx);
    let mut probe = Probe(BTreeMap::new());
    for root in &roots {
        probe.path(root)?;
        probe.path(&root.join("package.json"))?;
    }
    // This exactly follows the registry's root/direct-package discovery order.
    // Include even skipped/malformed manifests so repair can make them visible.
    let paths = discovery_paths(ctx);
    if paths.len() > MAX_INPUTS {
        return Err("Command-list discovery limit".into());
    }
    for path in paths {
        probe.path(&path)?;
        let mut manifest = probe.json(&path.join("package.json"))?;
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Ok(slug) = slug(name) else {
            continue;
        };
        let native = name.starts_with("com.tezbar.native.");
        if native {
            // resolve() always uses this managed root, even for a canonical
            // discovery entry found via SUPERCMD_EXTENSION_PATHS.
            let package = ctx.paths.data_dir.join("extensions/packages").join(name);
            probe.path(&ctx.paths.data_dir.join("extensions"))?;
            probe.path(&ctx.paths.data_dir.join("extensions/packages"))?;
            probe.path(&package)?;
            probe.path(&package.join("versions"))?;
            let managed = super::super::process_install::root(ctx, name).is_ok();
            let active = if managed {
                probe.json(&package.join("active.json"))?
            } else {
                Value::Null
            };
            if let Some(version) = active["directory"]
                .as_str()
                .filter(|v| super::super::process_install::component(v).is_ok())
            {
                let dir = package.join("versions").join(version);
                probe.path(&dir)?;
                let regular_dir = fs::symlink_metadata(&dir)
                    .is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink());
                let manifest_path = dir.join("extension.json");
                probe.path(&manifest_path)?;
                if regular_dir
                    && fs::symlink_metadata(&manifest_path).is_ok_and(|m| m.file_type().is_file())
                {
                    manifest = probe.json(&manifest_path)?;
                }
                // File/directory identity checks catch inventory modifications,
                // extra files, permission/link changes, and normal tampering.
                // No inventory contents are read or trusted for execution.
                if regular_dir {
                    for entry in walkdir::WalkDir::new(&dir)
                        .follow_links(false)
                        .follow_root_links(false)
                    {
                        let entry = entry.map_err(|e| e.to_string())?;
                        probe.path(entry.path())?;
                    }
                }
            }
        }
        let extension = Installed {
            id: if native {
                name.into()
            } else {
                format!("raycast.{slug}")
            },
            slug,
            path,
            manifest,
            data_dir: ctx.paths.data_dir.clone(),
        };
        probe.icons(&extension)?;
    }
    Ok(Inputs {
        roots,
        files: probe.0.into_iter().collect(),
    })
}
