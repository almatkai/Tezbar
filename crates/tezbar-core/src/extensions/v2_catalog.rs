//! Bundled trusted release references. Metadata is never proof of installation.
use super::{acquisition, process_install};
use crate::common::{CoreContext, CoreResult};
use serde_json::{json, Value};

pub(super) fn source() -> Vec<Value> {
    serde_json::from_str(include_str!("v2_catalog.json")).expect("bundled extension catalog")
}
pub(super) fn slug(id: &str) -> Option<&str> {
    let slug = id.strip_prefix("com.tezbar.native.")?;
    source().iter().any(|m| m["slug"] == slug).then_some(slug)
}
pub(super) fn manifest(slug: &str) -> Option<Value> {
    Some(describe_for(
        source().into_iter().find(|m| m["slug"] == slug)?,
        false,
    ))
}
// Installed local packages retain their normal coverage. Uninstalled local-only
// references must not suppress a working legacy package before import.
fn describe_for(item: Value, installed: bool) -> Value {
    let rule = acquisition::for_id(item["id"].as_str().unwrap_or(""));
    describe_with_route(item, installed, rule)
}
pub(super) fn describe_with_route(
    mut item: Value,
    installed: bool,
    rule: Result<acquisition::Rule, String>,
) -> Value {
    if !installed {
        if let Err(guidance) = acquisition::require_remote(rule) {
            if let Some(commands) = item["commands"].as_array_mut() {
                for command in commands {
                    command["runtime"] = json!("unavailable");
                    command["gap"] = json!(guidance);
                }
            }
        }
    }
    describe(item)
}
pub(super) fn describe(mut item: Value) -> Value {
    // Old source metadata stays visible, but cannot authorize process execution
    // until the publisher provides real schema-3 release references.
    let available = process_install::artifact(&item).is_ok();
    let platform = if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Linux"
    };
    if !item["commands"].is_array() {
        item["commands"] = json!([]);
    }
    let commands = item["commands"]
        .as_array_mut()
        .expect("normalized commands");
    let mut covered = 0;
    for command in commands.iter_mut() {
        let supported = command["platforms"]
            .as_array()
            .is_none_or(|a| a.iter().any(|v| v == platform));
        if available && supported && command["runtime"] == "rust" {
            covered += 1;
        } else {
            command["runtime"] = json!("unavailable");
            if !supported {
                command["gap"] = json!("No declared platform support");
            } else if !available {
                command["gap"] = json!("Verified executable artifact not published/pinned");
            } else if !command["gap"].is_string() {
                command["gap"] = json!("Command is not implemented by this executable");
            }
        }
    }
    item["totalCommands"] = json!(commands.len());
    item["title"] = item["name"].clone();
    item["status"] = json!(if covered == 0 {
        "unavailable"
    } else if covered == item["totalCommands"].as_u64().unwrap_or(0) {
        "complete"
    } else {
        "partial"
    });
    item
}
pub(super) fn manifests(ctx: &CoreContext) -> Vec<Value> {
    source()
        .into_iter()
        .map(|m| {
            let id = m["id"].as_str().unwrap_or("");
            let rule = acquisition::for_id(id);
            let installed = rule != Ok(acquisition::Rule::Remote) && installed(ctx, id);
            describe_with_route(m, installed, rule)
        })
        .collect()
}
/// raycast/extensions commit whose assets the native packages copied; pinned so store
/// artwork cannot change underneath a release.
const RAYCAST_ASSETS_COMMIT: &str = "28bf9eb29c72c2df9b4b1d17e3f7b51001de6c0d";
/// Store artwork for a native package that is not installed yet: its icon lives in the
/// release ZIP, so show the same file from the Raycast extension it replaces.
fn store_icon_url(native: &Value) -> Option<String> {
    let icon = native["icon"].as_str()?;
    if icon.starts_with("https://") {
        return Some(icon.to_owned());
    }
    // Packages keep the replaced extension's `assets/<file>` under resources/ (or resources/assets/).
    let file = icon
        .strip_prefix("resources/assets/")
        .or_else(|| icon.strip_prefix("resources/"))?;
    let slug = native["replaces"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .find_map(|id| id.strip_prefix("raycast."))?;
    let safe = |s: &str| {
        !s.is_empty()
            && !s.starts_with('.')
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    };
    (safe(slug) && safe(file)).then(|| {
        format!("https://raw.githubusercontent.com/raycast/extensions/{RAYCAST_ASSETS_COMMIT}/extensions/{slug}/assets/{file}")
    })
}
pub(super) fn store_item(native: Value) -> Value {
    let native = describe(native);
    let acquisition =
        acquisition::metadata(acquisition::for_id(native["id"].as_str().unwrap_or("")));
    json!({"id":native["id"],"name":native["name"],"description":native["description"],"version":native["version"],"author":"Tezbar","owner":"Tezbar","runtime":"rust","icon":native["icon"],"iconUrl":store_icon_url(&native),"platforms":native["platforms"],"commands":native["commands"],"nativeV2":native,"acquisition":acquisition})
}
#[cfg(test)]
mod store_icon_tests {
    use super::*;
    #[test]
    fn every_native_store_item_has_pinned_upstream_artwork() {
        let prefix = format!(
            "https://raw.githubusercontent.com/raycast/extensions/{RAYCAST_ASSETS_COMMIT}/extensions/"
        );
        for native in source() {
            let url = store_icon_url(&native).unwrap_or_else(|| panic!("{}", native["id"]));
            assert!(url.starts_with(&prefix), "{url}");
        }
        let traversal = json!({"icon":"resources/assets/../x.png","replaces":["raycast.a"]});
        assert_eq!(store_icon_url(&traversal), None);
        let direct = json!({"icon":"https://example.com/i.png"});
        assert_eq!(store_icon_url(&direct).as_deref(), Some("https://example.com/i.png"));
    }
}
pub(super) fn installed(ctx: &CoreContext, id: &str) -> bool {
    process_install::resolve(ctx, id).is_ok()
}
pub(super) async fn install(ctx: &CoreContext, id: &str) -> Option<CoreResult> {
    let slug = slug(id)?;
    let item = source().into_iter().find(|m| m["slug"] == slug)?;
    Some(process_install::install(ctx, item, None).await)
}
pub(crate) fn cli_list(ctx: &CoreContext) -> CoreResult {
    Ok(json!(manifests(ctx)
        .into_iter()
        .map(|mut m| {
            let id = m["id"].as_str().unwrap_or("");
            let root = process_install::root(ctx, id).ok();
            let status = match process_install::resolve(ctx, id) {
                Ok(_) => "installed",
                Err(_) => {
                    if root.is_some_and(|p| p.exists()) {
                        "needs-upgrade-or-repair"
                    } else {
                        "not-installed"
                    }
                }
            };
            m["installed"] = json!(status == "installed");
            m["installationStatus"] = json!(status);
            m["acquisition"] =
                acquisition::metadata(acquisition::for_id(m["id"].as_str().unwrap_or("")));
            m
        })
        .collect::<Vec<_>>()))
}
pub(crate) async fn cli_install(ctx: &CoreContext, id: &str) -> CoreResult {
    install(ctx, id)
        .await
        .ok_or_else(|| format!("Unknown Rust extension: {id}"))?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_without_pins_never_claims_installability() {
        for m in source().into_iter().map(describe) {
            if process_install::artifact(&m).is_err() {
                assert_eq!(m["status"], "unavailable");
                assert!(m["commands"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|c| c["runtime"] != "rust"));
            }
        }
    }
}
