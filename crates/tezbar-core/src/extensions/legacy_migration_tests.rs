//! Compiled app resolver + real Host grants. Frozen process coverage is opt-in
//! because the third-party executable is deliberately not a repository fixture.
use super::*;
use crate::common::CorePaths;
use std::path::Path;

const ROOTS: [&str; 3] = [
    "extensions",
    "extensions/packages",
    "extension-registry/packages",
];
fn context(path: &Path) -> CoreContext {
    CoreContext::headless(CorePaths {
        data_dir: path.into(),
        config_dir: path.join("config"),
        resource_dir: path.join("resources"),
        temp_dir: path.join("tmp"),
        home_dir: path.into(),
    })
}
fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}
fn manifest() -> Value {
    let mut m = super::super::v2_catalog::source()
        .into_iter()
        .find(|m| m["slug"] == "perplexity")
        .unwrap();
    m["migrationPaths"] = json!({});
    for (r, root) in ROOTS.iter().enumerate() {
        m["migrationPaths"][format!("root{r}")] = json!(format!("{root}/package.json"));
        for (a, name) in ["perplexity", "raycast.perplexity"].iter().enumerate() {
            m["migrationPaths"][format!("manifest{r}{a}")] =
                json!(format!("{root}/{name}/package.json"));
            m["migrationPaths"][format!("store{r}{a}")] =
                json!(format!("{root}/{name}/.tezbar-local-storage.json"));
        }
    }
    m
}
fn owner(base: &Path, root: usize, name: &str, data: Option<&[u8]>) -> PathBuf {
    let path = base.join(ROOTS[root]).join(name);
    // Actual identity comes from the directory, not these misleading fields.
    write(
        &path.join("package.json"),
        br#"{"name":"wrong","id":"raycast.wrong"}"#,
    );
    if let Some(data) = data {
        write(&path.join(".tezbar-local-storage.json"), data);
    }
    path
}
#[test]
fn non_ascii_migration_paths_are_rejected_before_owner_discovery_and_at_grant_time() {
    for path in [
        "extenſions/perplexity/.tezbar-local-storage.json",
        "extension-regiſtry/packages/perplexity/package.json",
        "extensions/perplexity/café.json",
        "EXTENS~1/perplexity/.tezbar-local-storage.json",
        "EXTENS~2/packages/perplexity/package.json",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let mut declared = manifest();
        declared["migrationPaths"] = json!({"oldStore": path});
        assert!(
            Host::new(context(temp.path()), &declared).is_err(),
            "{path}"
        );

        // Defense in depth even if a trusted/previously validated value is
        // replaced before grants are computed. No filesystem existence needed.
        let mut host = Host::new(context(temp.path()), &manifest()).unwrap();
        host.manifest["migrationPaths"] = declared["migrationPaths"].clone();
        assert!(host.migration_paths().is_err(), "{path}");
        assert!(!temp.path().join("external-extensions").exists());
        assert!(!temp.path().join("tmp").exists());
    }
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("профиль");
    let selected = owner(&profile, 0, "perplexity", Some(b"7"));
    let paths = Host::new(context(&profile), &manifest())
        .unwrap()
        .migration_paths()
        .unwrap();
    assert_eq!(
        Path::new(paths["store00"].as_str().unwrap()),
            selected.join(".tezbar-local-storage.json")
    );
}

#[test]
fn exact_owner_intersection_precedes_existence_and_domain_parsing() {
    for root in 0..3 {
        for bytes in [
            None,
            Some(b"{}".as_slice()),
            Some(b"{broken"),
            Some(b"[]"),
            Some(br#"{"counter":"0"}"#),
            Some(br#"{"counter":"7"}"#),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let active = owner(temp.path(), root, "perplexity", bytes);
            owner(
                temp.path(),
                root,
                "raycast.perplexity",
                Some(br#"{"counter":"91"}"#),
            );
            let paths = Host::new(context(temp.path()), &manifest())
                .unwrap()
                .migration_paths()
                .unwrap();
            assert_eq!(
                paths.as_object().unwrap().len(),
                if bytes.is_some() { 2 } else { 1 }
            );
            assert_eq!(
                Path::new(paths[format!("manifest{root}0")].as_str().unwrap()),
            active.join("package.json")
            );
            assert!(paths.get(format!("store{root}1")).is_none());
            if let Some(bytes) = bytes {
                assert_eq!(
                    std::fs::read(active.join(".tezbar-local-storage.json")).unwrap(),
                    bytes
                );
            }
        }
    }
}
#[test]
fn malformed_root_masks_children_and_orphans_never_own_data() {
    for root_bytes in [b"{}".as_slice(), b"{broken", b"[]", b""] {
        let temp = tempfile::tempdir().unwrap();
        owner(temp.path(), 0, "perplexity", Some(b"hidden"));
        let selected = owner(temp.path(), 1, "raycast.perplexity", Some(b"selected"));
        write(&temp.path().join("extensions/package.json"), root_bytes);
        let paths = Host::new(context(temp.path()), &manifest())
            .unwrap()
            .migration_paths()
            .unwrap();
        assert_eq!(paths.as_object().unwrap().len(), 2);
        assert_eq!(
            Path::new(paths["store11"].as_str().unwrap()),
            selected.join(".tezbar-local-storage.json")
        );
        assert!(paths.get("root0").is_none());
    }
    let temp = tempfile::tempdir().unwrap();
    write(
        &temp
            .path()
            .join("extensions/perplexity/.tezbar-local-storage.json"),
        b"orphan",
    );
    assert_eq!(
        Host::new(context(temp.path()), &manifest())
            .unwrap()
            .migration_paths()
            .unwrap(),
        json!({})
    );
}
#[cfg(unix)]
#[test]
fn unsafe_evidence_denies_but_stateless_and_native_only_do_not_scan_js() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    for kind in [
        "owner",
        "manifest",
        "root",
        "store",
        "dangling",
        "directory",
        "permission",
        "root-permission",
        "owner-permission",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let selected = owner(temp.path(), 0, "perplexity", Some(b"7"));
        owner(temp.path(), 0, "raycast.perplexity", Some(b"91"));
        let outside = temp.path().join("source");
        write(&outside.join("package.json"), b"{}");
        write(&outside.join(".tezbar-local-storage.json"), b"7");
        let mut denied = None;
        match kind {
            "owner" => {
                std::fs::remove_dir_all(&selected).unwrap();
                symlink(&outside, &selected).unwrap();
            }
            "manifest" | "store" | "dangling" => {
                let file = if kind == "manifest" {
                    "package.json"
                } else {
                    ".tezbar-local-storage.json"
                };
                std::fs::remove_file(selected.join(file)).unwrap();
                symlink(
                    outside.join(if kind == "dangling" { "missing" } else { file }),
                    selected.join(file),
                )
                .unwrap();
            }
            "root" => symlink(
                outside.join("package.json"),
                temp.path().join("extensions/package.json"),
            )
            .unwrap(),
            "directory" => {
                let p = selected.join(".tezbar-local-storage.json");
                std::fs::remove_file(&p).unwrap();
                std::fs::create_dir(p).unwrap();
            }
            _ => {
                let p = match kind {
                    "root-permission" => {
                        let p = temp.path().join("extensions/package.json");
                        write(&p, b"{}");
                        p
                    }
                    "owner-permission" => selected.clone(),
                    _ => selected.join(".tezbar-local-storage.json"),
                };
                std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0)).unwrap();
                denied = Some(p);
            }
        }
        let ctx = context(temp.path());
        let error = Host::new(ctx.clone(), &manifest())
            .unwrap()
            .migration_paths()
            .unwrap_err();
        assert!(error.contains("Repair the source"), "{kind}: {error}");
        assert!(!temp.path().join("external-extensions").exists());
        let mut m = manifest();
        m["migrationPaths"] = json!({});
        assert_eq!(
            Host::new(ctx.clone(), &m)
                .unwrap()
                .migration_paths()
                .unwrap(),
            json!({})
        );
        write(
            &temp
                .path()
                .join("native-extensions-v2/perplexity/storage.json"),
            b"{}",
        );
        m["migrationPaths"] = json!({"native":"native-extensions-v2/perplexity/storage.json"});
        assert!(Host::new(ctx, &m).unwrap().migration_paths().unwrap()["native"].is_string());
        if let Some(p) = denied {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
}
#[test]
fn selected_undeclared_owner_is_error_without_path_substitution() {
    let temp = tempfile::tempdir().unwrap();
    owner(temp.path(), 1, "perplexity", Some(b"7"));
    owner(temp.path(), 1, "raycast.perplexity", Some(b"91"));
    let mut m = manifest();
    m["migrationPaths"]
        .as_object_mut()
        .unwrap()
        .remove("manifest10");
    m["migrationPaths"]
        .as_object_mut()
        .unwrap()
        .remove("store10");
    assert!(Host::new(context(temp.path()), &m)
        .unwrap()
        .migration_paths()
        .unwrap_err()
        .contains("exact declared"));
}

#[test]
fn structural_slots_keep_multiple_replacements_case_aliases_and_support_paths_exact() {
    let temp = tempfile::tempdir().unwrap();
    let selected = owner(temp.path(), 0, "Perplexity", Some(b"7"));
    owner(temp.path(), 0, "raycast.perplexity", Some(b"91"));
    let other = owner(temp.path(), 2, "other", Some(b"13"));
    owner(temp.path(), 2, "raycast.other", Some(b"91"));
    let mut m = manifest();
    m["replaces"] = json!(["raycast.perplexity", "raycast.Other"]);
    // Ordinary within-owner subdirectories are not additional discovery roots.
    let support = selected.join("packages/raycast.other/cache/shared.json");
    write(&support, b"support");
    let native = temp
        .path()
        .join("native-extensions-v2/perplexity/storage.json");
    let old_private = temp
        .path()
        .join("external-extensions/com.tezbar.native.perplexity/storage.json");
    write(&native, b"{}");
    write(&old_private, b"{\"unknown\":42}");
    m["migrationPaths"] = json!({
        "selected":"extensions/perplexity/package.json",
        "caseAlias":"Extensions/Perplexity/.tezbar-local-storage.json",
        "support":"extensions/Perplexity/packages/raycast.other/cache/shared.json",
        "inactive":"extensions/raycast.perplexity/.tezbar-local-storage.json",
        "other":"extension-registry/packages/other/.tezbar-local-storage.json",
        "otherInactive":"extension-registry/packages/raycast.other/.tezbar-local-storage.json",
        "native":"native-extensions-v2/perplexity/storage.json",
        "oldPrivate":"external-extensions/com.tezbar.native.perplexity/storage.json"
    });
    let aliases_resolve = temp
        .path()
        .join("extensions/perplexity/package.json")
        .exists()
        && temp
            .path()
            .join("Extensions/Perplexity/.tezbar-local-storage.json")
            .exists();
    let host = Host::new(context(temp.path()), &m).unwrap();
    if aliases_resolve {
        let grants = host.migration_paths().unwrap();
        assert_eq!(grants.as_object().unwrap().len(), 6);
        for (key, path) in m["migrationPaths"].as_object().unwrap() {
            if !["inactive", "otherInactive"].contains(&key.as_str()) {
                assert_eq!(grants[key], json!(temp.path().join(path.as_str().unwrap())));
            }
        }
    } else {
        // Support itself addresses the selected case-exact directory, so the
        // owner is representable even though the lowercase file is not granted.
        let grants = host.migration_paths().unwrap();
        assert!(grants.get("selected").is_none());
        assert!(grants.get("caseAlias").is_none());
        assert_eq!(grants["support"], json!(support));
        assert_eq!(
            grants["other"],
            json!(other.join(".tezbar-local-storage.json"))
        );
    }
    assert_eq!(std::fs::read(native).unwrap(), b"{}");
    assert_eq!(std::fs::read(old_private).unwrap(), b"{\"unknown\":42}");
}
