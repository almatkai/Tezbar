//! Child of registry: capabilities derive from its canonical installed identities,
//! never serialized extension props or a caller-provided legacy allowlist.
use super::*;
use crate::assets::AssetScope;
use std::collections::BTreeSet;

fn registered(extension: Installed) -> Result<AssetScope, String> {
    if !extension.manifest["healthError"].is_null() {
        return Err("Installed extension needs repair".into());
    }
    let root = extension
        .path
        .canonicalize()
        .map_err(|_| "Installed extension is unavailable")?;
    if !root.is_dir() {
        return Err("Installed extension is unavailable".into());
    }
    let mut legacy = BTreeSet::new();
    let names = std::iter::once(extension.manifest["icon"].as_str())
        .chain(
            extension.manifest["commands"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| c["icon"].as_str().or_else(|| c["iconPath"].as_str())),
        )
        .flatten();
    for name in names {
        let relative = Path::new(name.strip_prefix("./").unwrap_or(name));
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            continue;
        }
        for candidate in icon_candidates(&extension, relative) {
            // Legacy grants name exact registry icon files only. A legacy symlink
            // cannot turn a registry icon into arbitrary filesystem authority.
            if let Ok(canonical) = candidate.canonicalize() {
                if crate::assets::exact_file_spelling(&candidate, &canonical)
                    && canonical.is_file()
                    && !canonical.starts_with(&root)
                {
                    // The wire may use the ordinary drive spelling while the locked
                    // reader uses the verbatim path. Both name this exact file.
                    legacy.insert(candidate);
                    legacy.insert(canonical);
                }
            }
        }
    }
    Ok(AssetScope::registered(root, legacy))
}
pub(crate) fn scope(ctx: &CoreContext, id: &str) -> Result<AssetScope, String> {
    registered(installed(ctx, id)?)
}
pub(crate) fn scope_for_root(ctx: &CoreContext, requested: &Path) -> Result<AssetScope, String> {
    // Match an installed record again on every read. Uninstall/replacement or a
    // forged public UI scope does not preserve stale file authority.
    let extension = collect(ctx)
        .into_iter()
        .find(|e| e.path.canonicalize().is_ok_and(|p| p == requested))
        .ok_or("Image scope is not an installed extension")?;
    registered(extension)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, CoreContext, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::common::CorePaths {
            config_dir: root.path().join("config"),
            data_dir: root.path().join("data"),
            home_dir: root.path().join("home"),
            temp_dir: root.path().join("tmp"),
            resource_dir: root.path().join("resources"),
        };
        let package = paths.data_dir.join("extensions/scope-fixture");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(
            package.join("package.json"),
            br#"{"icon":"image.png","commands":[]}"#,
        )
        .unwrap();
        let ctx = CoreContext::new(
            paths,
            Some(std::sync::Arc::new(crate::host::TestHost::default())),
        );
        (root, ctx, package)
    }
    #[test]
    fn registry_issues_identity_and_uninstall_invalidates_it() {
        let (_root, ctx, package) = fixture();
        let grant = scope(&ctx, "raycast.scope-fixture").unwrap();
        assert_eq!(grant.root(), package.canonicalize().unwrap());
        assert!(grant.allowed_legacy().is_empty());
        assert!(scope_for_root(&ctx, grant.root()).is_ok());
        std::fs::remove_dir_all(&package).unwrap();
        assert!(scope_for_root(&ctx, grant.root()).is_err());
    }
    #[test]
    fn random_directory_and_unknown_identity_cannot_issue_grants() {
        let (root, ctx, _) = fixture();
        assert!(scope(&ctx, "raycast.missing").is_err());
        assert!(scope_for_root(&ctx, root.path()).is_err());
        assert!(scope(&ctx, "../../private").is_err());
    }
}
