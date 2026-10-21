//! Bundled manifests describe source commands; compiled dispatch decides coverage.
use super::v2_runtime;
use crate::native_core::common::{now_ms, read_json, write_json_atomic, CoreContext, CoreResult};
use serde_json::{json, Value};
use std::path::PathBuf;

fn source() -> Vec<Value> {
    serde_json::from_str(include_str!("v2_catalog.json")).expect("bundled Native V2 catalog")
}

pub(super) fn slug(id: &str) -> Option<&str> {
    let slug = id.strip_prefix("com.tezbar.native.")?;
    source()
        .iter()
        .any(|item| item["slug"] == slug)
        .then_some(slug)
}

pub(super) fn manifest(slug: &str) -> Option<Value> {
    let mut item = source().into_iter().find(|item| item["slug"] == slug)?;
    let compiled = v2_runtime::commands(slug);
    let platform = if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "Linux"
    };
    let commands = item["commands"].as_array_mut()?;
    let mut covered = 0;
    for command in commands.iter_mut() {
        let name = command["name"].as_str().unwrap_or("");
        let platform_ok = command["platforms"]
            .as_array()
            .is_some_and(|p| p.iter().any(|v| v == platform));
        if compiled.contains(&name) && platform_ok {
            command["runtime"] = json!("rust");
            command.as_object_mut().unwrap().remove("gap");
            covered += 1;
        } else {
            command["runtime"] = json!("unavailable");
            command["gap"] = json!(if !platform_ok {
                format!("No declared {platform} replacement for this command")
            } else {
                "No compiled Rust handler for this command".into()
            });
        }
    }
    let total = commands.len();
    item["totalCommands"] = json!(total);
    item["title"] = item["name"].clone();
    item["status"] = json!(if covered == total {
        "complete"
    } else if covered > 0 {
        "partial"
    } else {
        "unavailable"
    });
    Some(item)
}

pub(super) fn manifests() -> Vec<Value> {
    source()
        .iter()
        .filter_map(|item| manifest(item["slug"].as_str()?))
        .collect()
}

pub(super) fn store_item(native: Value) -> Value {
    json!({
        "id": native["id"],
        "name": native["name"],
        "description": native["description"],
        "version": native["version"],
        "author": "Tezbar",
        "owner": "Tezbar",
        "runtime": "rust",
        "icon": native["icon"],
        "iconUrl": native["icon"],
        "platforms": native["platforms"],
        "commands": native["commands"],
        "nativeV2": native
    })
}

fn package_root(ctx: &CoreContext, id: &str) -> PathBuf {
    ctx.paths.data_dir.join("extensions/packages").join(id)
}

pub(super) fn installed(ctx: &CoreContext, id: &str) -> bool {
    let Some(slug) = slug(id) else { return false };
    if manifest(slug).is_none_or(|v| v["status"] == "unavailable") {
        return false;
    }
    let root = package_root(ctx, id);
    read_json(&root.join("native-v2.json"))
        .is_ok_and(|v| v["id"] == id && v["schemaVersion"] == 2 && v["runtime"] == "rust")
        && read_json(&root.join("package.json"))
            .is_ok_and(|v| v["id"] == id && v["schemaVersion"] == 2)
}

/// Installs only a compiled-in adapter. Existing Raycast packages remain intact.
pub(super) fn install(ctx: &CoreContext, id: &str) -> Option<CoreResult> {
    let slug = slug(id)?;
    let item = manifest(slug)?;
    if item["status"] == "unavailable" {
        return Some(Err(format!(
            "No compiled native commands are available for {id} on this platform"
        )));
    }
    let id = item["id"].as_str()?;
    let root = package_root(ctx, id);
    let marker = json!({"schemaVersion":2,"id":id,"runtime":"rust","version":item["version"]});
    let result = (|| {
        if root.exists() && !root.is_dir() {
            return Err("Native extension install path is not a directory".into());
        }
        if root.join("package.json").is_file() {
            let existing = read_json(&root.join("package.json"))?;
            if existing["id"] != id || existing["schemaVersion"] != 2 {
                return Err("Native extension install path contains another package".into());
            }
        }
        write_json_atomic(&root.join("package.json"), &item)?;
        write_json_atomic(&root.join("native-v2.json"), &marker)?;
        let db_path = ctx.paths.data_dir.join("extensions/installed.json");
        let mut db = if db_path.is_file() {
            read_json(&db_path)?
        } else {
            json!({"installed":[]})
        };
        let entries = db["installed"]
            .as_array_mut()
            .ok_or("Invalid installed extensions database")?;
        let installed_at = entries
            .iter()
            .find(|v| v["id"] == id)
            .and_then(|v| v["installedAt"].as_u64())
            .unwrap_or_else(now_ms);
        let mut entry = store_item(item.clone());
        entry["installedAt"] = json!(installed_at);
        entries.retain(|v| v["id"] != id);
        entries.push(entry.clone());
        write_json_atomic(&db_path, &db)?;
        ctx.emit("extensions-changed", json!({}));
        Ok(entry)
    })();
    Some(result)
}

pub(crate) fn cli_list(ctx: &CoreContext) -> CoreResult {
    Ok(json!(manifests()
        .into_iter()
        .map(|mut item| {
            let id = item["id"].as_str().unwrap_or("").to_string();
            item["installed"] = json!(installed(ctx, &id));
            item
        })
        .collect::<Vec<_>>()))
}

pub(crate) fn cli_install(ctx: &CoreContext, id: &str) -> CoreResult {
    install(ctx, id).ok_or_else(|| format!("Unknown Native V2 extension: {id}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_ids_and_coverage_are_consistent() {
        let items = manifests();
        assert_eq!(items.len(), 11);
        for item in items {
            let slug = item["slug"].as_str().unwrap();
            assert_eq!(item["id"], v2_runtime::canonical_id(slug));
            assert_eq!(item["replaces"][0], format!("raycast.{slug}"));
            assert_eq!(
                item["totalCommands"],
                item["commands"].as_array().unwrap().len()
            );
            for command in item["commands"].as_array().unwrap() {
                if command["runtime"] == "rust" {
                    assert!(v2_runtime::supports(
                        item["id"].as_str().unwrap(),
                        command["name"].as_str().unwrap()
                    ));
                }
            }
            assert!(item["verification"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| v["state"] == "not-run"));
        }
    }
}
