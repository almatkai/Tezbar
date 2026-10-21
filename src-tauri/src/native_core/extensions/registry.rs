use crate::native_core::common::{
    now_ms, read_json, write_json_atomic, ConfigStore, CoreContext, CoreResult,
};
use base64::Engine;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

pub(super) struct Installed {
    id: String,
    slug: String,
    path: PathBuf,
    manifest: Value,
}

fn slug(id: &str) -> Result<String, String> {
    let name = id
        .trim()
        .strip_prefix("com.tezbar.native.")
        .or_else(|| id.trim().strip_prefix("raycast."))
        .unwrap_or(id.trim());
    if name.is_empty()
        || name.len() > 200
        || !name.as_bytes()[0].is_ascii_alphanumeric()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err("Invalid extension id".into());
    }
    Ok(name.into())
}

fn managed_roots(ctx: &CoreContext) -> Vec<PathBuf> {
    vec![
        ctx.paths.data_dir.join("extensions"),
        ctx.paths.data_dir.join("extensions/packages"),
        ctx.paths.data_dir.join("extension-registry/packages"),
    ]
}

fn roots(ctx: &CoreContext) -> Vec<PathBuf> {
    let mut result = managed_roots(ctx);
    if let Some(paths) = std::env::var_os("SUPERCMD_EXTENSION_PATHS") {
        result.extend(std::env::split_paths(&paths));
    }
    result
}

fn collect(ctx: &CoreContext) -> Vec<Installed> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for root in roots(ctx) {
        let mut paths = if root.join("package.json").is_file() {
            vec![root]
        } else {
            std::fs::read_dir(root)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|e| e.path())
                .collect()
        };
        paths.sort();
        for path in paths {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Ok(slug) = slug(name) else { continue };
            let Ok(manifest) = read_json(&path.join("package.json")) else {
                continue;
            };
            let native = name.starts_with("com.tezbar.native.")
                && super::v2_catalog::installed(ctx, name)
                && manifest["id"] == name
                && manifest["schemaVersion"] == 2;
            let manifest = if native {
                super::v2_catalog::manifest(&slug).unwrap_or(manifest)
            } else {
                manifest
            };
            let id = if native {
                name.to_string()
            } else {
                format!("raycast.{slug}")
            };
            if !manifest.is_object() || !seen.insert(id.to_lowercase()) {
                continue;
            }
            found.push(Installed {
                id,
                slug,
                path,
                manifest,
            });
        }
    }
    found
}

fn installed(ctx: &CoreContext, id: &str) -> Result<Installed, String> {
    let wanted = slug(id)?;
    let canonical = id.trim().starts_with("com.tezbar.native.");
    collect(ctx)
        .into_iter()
        .find(|entry| {
            entry.slug == wanted && entry.id.starts_with("com.tezbar.native.") == canonical
        })
        .ok_or_else(|| format!("Extension is not installed: {id}"))
}

/// Native implementations use the same discovery and platform rules as the launcher.
pub(super) fn installed_command(ctx: &CoreContext, id: &str, command: &str) -> Option<Installed> {
    let native = if id.starts_with("raycast.") {
        let slug = slug(id).ok()?;
        let canonical = super::v2_runtime::canonical_id(&slug);
        installed(ctx, &canonical).ok()
    } else if id.starts_with("com.tezbar.native.") {
        installed(ctx, id).ok()
    } else {
        None
    };
    if let Some(extension) = native {
        let active = super::v2_runtime::supports(&extension.id, command)
            && super::v2_catalog::manifest(&extension.slug)?["commands"]
                .as_array()?
                .iter()
                .any(|c| c["name"] == command && c["runtime"] == "rust");
        if active {
            return Some(extension);
        }
        if id.starts_with("com.tezbar.native.") {
            return None;
        }
    }
    let extension = installed(ctx, id).ok()?;
    (compatible(&extension.manifest, false)
        && extension.manifest["commands"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|cmd| cmd["name"].as_str() == Some(command) && compatible(cmd, true)))
    .then_some(extension)
}

fn platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else {
        "macOS"
    }
}

fn compatible(manifest: &Value, command: bool) -> bool {
    if command && manifest.get("platforms").is_none() {
        return true;
    }
    let Some(platforms) = manifest["platforms"]
        .as_array()
        .or_else(|| manifest["tezbar"]["platforms"].as_array())
    else {
        return platform() == "macOS";
    };
    platforms
        .iter()
        .filter_map(Value::as_str)
        .any(|value| match value.to_lowercase().as_str() {
            "macos" | "darwin" | "mac" => platform() == "macOS",
            "windows" | "win32" | "win" => platform() == "Windows",
            "linux" => platform() == "Linux",
            _ => false,
        })
}

fn person(value: &Value) -> String {
    value
        .as_str()
        .or_else(|| value["name"].as_str())
        .or_else(|| value["handle"].as_str())
        .unwrap_or("")
        .to_string()
}

pub(super) fn avatar(value: &Value) -> Value {
    let raw = person(value);
    let handle = raw
        .split(['<', '('])
        .next()
        .unwrap_or("")
        .trim()
        .trim_start_matches('@');
    if handle.is_empty()
        || handle.len() > 39
        || !handle
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || !handle.as_bytes()[0].is_ascii_alphanumeric()
        || !handle.as_bytes()[handle.len() - 1].is_ascii_alphanumeric()
    {
        return Value::Null;
    }
    json!(format!("https://github.com/{handle}.png?size=96"))
}

fn fallback<'a>(value: &'a Value, key: &str, default: &'a str) -> &'a str {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(default)
}

fn icon_path(extension: &Installed) -> Option<String> {
    let name = fallback(&extension.manifest, "icon", "icon.png");
    if name.starts_with("https://") || name.starts_with("http://") || name.starts_with("/extensions/") {
        return Some(name.into());
    }
    let relative = Path::new(name.strip_prefix("./").unwrap_or(name));
    if relative
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return None;
    }
    let mut candidates = vec![
        extension.path.join(relative),
        extension.path.join("assets").join(relative),
    ];
    if extension.id.starts_with("com.tezbar.native.") {
        if let Some(packages) = extension.path.parent() {
            if let Some(extensions) = packages.parent() {
                let legacy = extensions.join(&extension.slug);
                candidates.push(legacy.join(relative));
                candidates.push(legacy.join("assets").join(relative));
            }
            let legacy = packages.join(format!("raycast.{}", extension.slug));
            candidates.push(legacy.join(relative));
            candidates.push(legacy.join("assets").join(relative));
        }
    }
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into())
}

fn icon_data(extension: &Installed) -> Value {
    let Some(path) = icon_path(extension) else {
        return Value::Null;
    };
    if path.starts_with("http") || path.starts_with("/extensions/") {
        return json!(path);
    }
    let Ok(bytes) = std::fs::read(&path) else {
        return Value::Null;
    };
    if bytes.len() > 2_000_000 {
        return Value::Null;
    }
    let mime = match Path::new(&path)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
    {
        "svg" => "image/svg+xml",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/png",
    };
    json!(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn command_values(extension: &Installed) -> Vec<Value> {
    if !compatible(&extension.manifest, false) {
        return vec![];
    }
    let pkg = &extension.manifest;
    let icon = icon_data(extension);
    let owner = person(pkg.get("owner").unwrap_or(&pkg["author"]));
    pkg["commands"].as_array().into_iter().flatten().filter(|c| c["name"].is_string()&&compatible(c,true) && (extension.id.starts_with("raycast.") || c["runtime"] == "rust")).map(|cmd| {
        let name=cmd["name"].as_str().unwrap_or("");
        let title=fallback(cmd,"title",name);
        let extension_title=fallback(pkg,"title",&extension.slug);
        let description=fallback(cmd,"description","");
        json!({"id":format!("ext-{}-{name}",extension.slug),"title":title,"extensionTitle":extension_title,
            "extName":extension.slug,"extensionId":extension.id,"cmdName":name,"owner":owner,"description":description,"mode":fallback(cmd,"mode","view"),
            "interval":cmd["interval"],"disabledByDefault":cmd["disabledByDefault"].as_bool().unwrap_or(false),
            "keywords":[extension.slug.to_lowercase(),extension_title.to_lowercase(),name.to_lowercase(),title.to_lowercase(),description.to_lowercase()],
            "iconDataUrl":icon,"commandArgumentDefinitions":cmd["arguments"].as_array().cloned().unwrap_or_default()})
    }).collect()
}

fn list(ctx: &CoreContext) -> Value {
    let mut values:Vec<_>=collect(ctx).iter().map(|extension| {
        let pkg=&extension.manifest;
        let author=person(pkg.get("author").unwrap_or(&pkg["owner"]));
        let owner=person(pkg.get("owner").unwrap_or(&pkg["author"]));
        let commands:Vec<_>=command_values(extension).iter().map(|c|json!({"name":c["cmdName"],"title":c["title"],
            "subtitle":fallback(c,"description",c["extensionTitle"].as_str().unwrap_or("")),"description":c["description"],
            "mode":c["mode"],"argumentDefinitions":c["commandArgumentDefinitions"]})).collect();
        let installed_at=std::fs::metadata(&extension.path).and_then(|m|m.modified()).ok().and_then(|t|t.duration_since(UNIX_EPOCH).ok()).map(|d|d.as_millis() as u64).unwrap_or_else(now_ms);
        let mut value=json!({"id":extension.id,"slug":extension.slug,"name":fallback(pkg,"title",&extension.slug),
            "version":fallback(pkg,"version","1.0.0"),"description":fallback(pkg,"description",""),"author":author,"owner":owner,
            "authorIconUrl":avatar(&json!(author)),"iconPath":icon_path(extension),"packageJsonPath":extension.path.join("package.json"),
            "extensionPath":extension.path,"commands":commands,"installedAt":installed_at});
        if extension.id.starts_with("com.tezbar.native.") { value["author"]=json!("Tezbar"); value["owner"]=json!("Tezbar"); value["runtime"]=json!("rust"); value["nativeV2"]=super::v2_catalog::manifest(&extension.slug).unwrap_or(Value::Null); value["platforms"]=json!(["macOS","Windows"]); }
        value
    }).collect();
    values.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    json!(values)
}

fn platform_default(value: &Value) -> Value {
    if value.get("macOS").is_some()
        || value.get("Windows").is_some()
        || value.get("Linux").is_some()
    {
        value
            .get(platform())
            .or_else(|| value.get("macOS"))
            .or_else(|| value.get("Windows"))
            .cloned()
            .unwrap_or(Value::Null)
    } else {
        value.clone()
    }
}

fn preference_schemas(extension: &Installed, command: Option<&str>) -> Vec<Value> {
    let mut prefs = extension.manifest["preferences"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if let Some(name) = command {
        if let Some(cmd) = extension.manifest["commands"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|c| c["name"].as_str() == Some(name))
        {
            for pref in cmd["preferences"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|v| v.is_object())
            {
                let mut pref = pref.clone();
                pref["commandName"] = json!(name);
                pref["commandTitle"] = json!(fallback(cmd, "title", name));
                prefs.push(pref);
            }
        }
    }
    prefs
}

pub(super) fn preferences(extension: &Installed, command: Option<&str>) -> CoreResult {
    let mut values = Map::new();
    for pref in preference_schemas(extension, command) {
        let Some(name) = pref["name"].as_str() else {
            continue;
        };
        let value = if pref.get("default").is_some() {
            platform_default(&pref["default"])
        } else if pref["type"] == "checkbox" {
            json!(false)
        } else if pref["type"] == "dropdown" {
            pref["data"][0].get("value").cloned().unwrap_or(json!(""))
        } else if pref["type"] == "appPicker" && name == "uninstaller_app" {
            default_uninstaller()
        } else {
            json!("")
        };
        values.insert(name.into(), value);
    }
    let saved = ConfigStore::new(preferences_path(extension)).read()?;
    if let Some(saved) = saved.as_object() {
        values.extend(saved.clone());
    }
    if let Some(command) = command {
        if let Some(saved) = saved["commands"][command].as_object() {
            values.extend(saved.clone());
        }
    }
    Ok(Value::Object(values))
}

fn preferences_path(extension: &Installed) -> PathBuf {
    let own = extension.path.join("preferences.json");
    if own.is_file() || !extension.id.starts_with("com.tezbar.native.") {
        return own;
    }
    let packages = extension.path.parent().unwrap_or(&extension.path);
    let extensions = packages.parent().unwrap_or(packages);
    for legacy in [
        extensions.join(&extension.slug).join("preferences.json"),
        packages
            .join(format!("raycast.{}", extension.slug))
            .join("preferences.json"),
    ] {
        if legacy.is_file() {
            return legacy;
        }
    }
    own
}

fn default_uninstaller() -> Value {
    for (name, path) in [
        ("AppCleaner", "/Applications/AppCleaner.app"),
        ("Pearcleaner", "/Applications/PearCleaner.app"),
        ("TrashMe 3", "/Applications/TrashMe 3.app"),
        ("App Cleaner 8", "/Applications/App Cleaner 8.app"),
    ] {
        if Path::new(path).is_dir() {
            let id = std::process::Command::new("/usr/bin/plutil")
                .args(["-extract", "CFBundleIdentifier", "raw", "-o", "-"])
                .arg(Path::new(path).join("Contents/Info.plist"))
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            return json!({"name":name,"path":path,"bundleId":id});
        }
    }
    json!("")
}

fn settings(ctx: &CoreContext) -> Value {
    fn schemas(values: &Value, scope: &str) -> Vec<Value> {
        values
            .as_array()
            .into_iter()
            .flatten()
            .filter(|v| v["name"].is_string())
            .map(|v| {
                let mut v = v.clone();
                v["scope"] = json!(scope);
                v["default"] = platform_default(&v["default"]);
                v["required"] = json!(v["required"].as_bool().unwrap_or(false));
                v
            })
            .collect()
    }
    let mut result:Vec<_>=collect(ctx).iter().filter(|e|compatible(&e.manifest,false)).map(|e| {
        let pkg=&e.manifest;
        let commands:Vec<_>=pkg["commands"].as_array().into_iter().flatten().filter(|v|v["name"].is_string()&&compatible(v,true) && (e.id.starts_with("raycast.") || v["runtime"] == "rust")).map(|cmd| {
            json!({"name":cmd["name"],"title":fallback(cmd,"title",cmd["name"].as_str().unwrap_or("")),"description":fallback(cmd,"description",""),
                "mode":fallback(cmd,"mode","view"),"interval":cmd["interval"],"disabledByDefault":cmd["disabledByDefault"].as_bool().unwrap_or(false),"preferences":schemas(&cmd["preferences"],"command")})
        }).collect();
        json!({"extName":e.slug,"extensionId":e.id,"title":fallback(pkg,"title",&e.slug),"description":fallback(pkg,"description",""),
            "owner":person(pkg.get("owner").unwrap_or(&pkg["author"])),"iconDataUrl":icon_data(e),"preferences":schemas(&pkg["preferences"],"extension"),"commands":commands})
    }).collect();
    result.sort_by(|a, b| a["title"].as_str().cmp(&b["title"].as_str()));
    json!(result)
}

fn integrity(ctx: &CoreContext, id: &str) -> CoreResult {
    if id.starts_with("com.tezbar.native.") && super::v2_catalog::slug(id).is_none() {
        return Err("Unknown native extension".into());
    }
    if let Some(slug) = super::v2_catalog::slug(id) {
        let manifest = super::v2_catalog::manifest(slug).ok_or("Unknown native extension")?;
        let installed = super::v2_catalog::installed(ctx, id);
        let root = ctx.paths.data_dir.join("extensions/packages").join(id);
        let valid = installed
            && read_json(&root.join("package.json"))
                .is_ok_and(|v| v["id"] == id && v["schemaVersion"] == 2);
        let unavailable: Vec<_> = manifest["commands"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|v| v["runtime"] != "rust")
            .map(|v| v["name"].clone())
            .collect();
        return Ok(
            json!({"extensionId":id,"installed":installed,"healthy":valid,"runtime":"rust","nativeV2":manifest,"missingScripts":[],"tamperedScripts":[],"unavailableCommands":unavailable}),
        );
    }
    let slug = slug(id)?;
    let id = format!("raycast.{slug}");
    let root = ctx.paths.data_dir.join("extensions/packages").join(&id);
    let meta = if root.join("meta.json").is_file() {
        read_json(&root.join("meta.json"))?
    } else {
        return Ok(
            json!({"extensionId":id,"installed":false,"missingScripts":[],"tamperedScripts":[],"healthy":false}),
        );
    };
    let mut missing = meta["missingScripts"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut tampered = vec![];
    for name in meta["commandNames"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if missing.contains(&json!(name)) {
            continue;
        }
        // Command names are filenames, never paths supplied by an extension.
        if slug_component(name).is_err() {
            missing.push(json!(name));
            continue;
        }
        match std::fs::read(root.join(".sc-build").join(format!("{name}.js"))) {
            Ok(bytes) => {
                if let Some(expected) = meta["scriptHashes"][name].as_str() {
                    if format!("{:x}", Sha256::digest(bytes)) != expected {
                        tampered.push(json!(name));
                    }
                }
            }
            Err(_) => missing.push(json!(name)),
        }
    }
    Ok(
        json!({"extensionId":id,"installed":true,"commitRef":meta["commitRef"],"healthy":missing.is_empty()&&tampered.is_empty(),"missingScripts":missing,"tamperedScripts":tampered,"lastError":meta["lastError"]}),
    )
}

fn slug_component(value: &str) -> Result<(), String> {
    slug(value).map(|_| ())
}

fn uninstall(ctx: &CoreContext, id: &str) -> CoreResult {
    if id.starts_with("com.tezbar.native.") && super::v2_catalog::slug(id).is_none() {
        return Err("Unknown native extension".into());
    }
    let slug = slug(id)?;
    let canonical = id.starts_with("com.tezbar.native.");
    for root in managed_roots(ctx) {
        let names = if canonical {
            vec![format!("com.tezbar.native.{slug}")]
        } else {
            vec![
                slug.clone(),
                format!("raycast.{slug}"),
                format!("raycast.{slug}.backup"),
            ]
        };
        for name in names {
            let path = root.join(name);
            if let Ok(meta) = std::fs::symlink_metadata(&path) {
                if meta.file_type().is_symlink() {
                    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
                } else if meta.is_dir() {
                    std::fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
                }
            }
        }
    }
    let path = ctx.paths.data_dir.join("extensions/installed.json");
    if path.is_file() {
        let mut db = read_json(&path)?;
        if let Some(items) = db["installed"].as_array_mut() {
            let target = if canonical {
                format!("com.tezbar.native.{slug}")
            } else {
                format!("raycast.{slug}")
            };
            items.retain(|v| v["id"].as_str() != Some(&target));
        }
        write_json_atomic(&path, &db)?;
    }
    ctx.emit("extensions-changed", json!({}));
    Ok(json!(true))
}

pub(super) fn handle(ctx: &CoreContext, channel: &str, args: &[Value]) -> CoreResult {
    let body = args.first().cloned().unwrap_or(Value::Null);
    match channel {
        "extension:list" => Ok(list(ctx)),
        "core:extensions:commands" => {
            let mut extensions = collect(ctx);
            // Prefer each native command once; keep uncovered legacy commands
            // and both installed packages available for settings/uninstallation.
            extensions.sort_by_key(|entry| !entry.id.starts_with("com.tezbar.native."));
            let mut seen = HashSet::new();
            Ok(json!(extensions.iter().flat_map(command_values).filter(|command| {
                seen.insert(command["id"].as_str().unwrap_or("").to_owned())
            }).collect::<Vec<_>>()))
        },
        "get-installed-extensions-settings-schema" => Ok(settings(ctx)),
        "get-extensions" | "extensions:listInstalled" => {
            let path = ctx.paths.data_dir.join("extensions/installed.json");
            let mut items = if path.exists() {
                read_json(&path)?["installed"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            } else {
                vec![]
            };
            for item in &mut items {
                if let Some(slug) = item["id"].as_str().and_then(super::v2_catalog::slug) {
                    if let Some(meta) = super::v2_catalog::manifest(slug) {
                        item["runtime"] = json!("rust");
                        item["author"] = json!("Tezbar");
                        item["owner"] = json!("Tezbar");
                        item["nativeV2"] = meta;
                        item["platforms"] = json!(["macOS", "Windows"]);
                    }
                }
            }
            items.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
            Ok(json!(items))
        }
        "extension:uninstall" | "extensions:uninstall" => {
            uninstall(ctx, body.as_str().ok_or("Extension id required")?)
        }
        "extensions:integrity" => integrity(ctx, body.as_str().ok_or("Extension id required")?),
        "extensions:install-error" => {
            let Some(id) = body.as_str() else {
                return Ok(Value::Null);
            };
            Ok(integrity(ctx, id)?["lastError"].clone())
        }
        "preferences:get" | "preferences:setup" | "preferences:set" => {
            let Some(id) = body["extensionId"].as_str() else {
                return Ok(if channel == "preferences:setup" {
                    Value::Null
                } else {
                    json!({})
                });
            };
            let extension = installed(ctx, id)?;
            let command = body["commandName"].as_str();
            if channel == "preferences:set" {
                let values = body["values"].as_object().cloned().unwrap_or_default();
                ConfigStore::new(preferences_path(&extension)).update(|current| {
                    if let Some(command) = command {
                        if !current["commands"].is_object() {
                            current["commands"] = json!({});
                        }
                        if !current["commands"][command].is_object() {
                            current["commands"][command] = json!({});
                        }
                        current["commands"][command]
                            .as_object_mut()
                            .unwrap()
                            .extend(values);
                    } else {
                        current.as_object_mut().unwrap().extend(values);
                    }
                    Ok(())
                })?;
            }
            let values = preferences(&extension, command)?;
            if channel == "preferences:setup" {
                Ok(json!({"extensionId":extension.id,"commandName":command,
                    "title":fallback(&extension.manifest,"title",&extension.slug),"iconPath":icon_path(&extension),
                    "preferences":preference_schemas(&extension,command),"values":values,"hasSavedPreferences":preferences_path(&extension).is_file()}))
            } else {
                Ok(values)
            }
        }
        _ => Err(format!("Unknown extension metadata channel: {channel}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context(path: &Path) -> CoreContext {
        CoreContext::headless(crate::native_core::common::CorePaths {
            data_dir: path.into(),
            config_dir: path.join("config"),
            resource_dir: path.join("resources"),
            temp_dir: path.join("tmp"),
            home_dir: path.into(),
        })
    }
    #[test]
    fn legacy_preferences_preserve_scopes_and_unknown_keys() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let path = temp.path().join("extensions/packages/raycast.example");
        write_json_atomic(&path.join("package.json"),&json!({"title":"Example","platforms":[platform()],"preferences":[{"name":"enabled","type":"checkbox"}],"commands":[{"name":"run","preferences":[{"name":"language","default":{"macOS":"ru","Windows":"kk","Linux":"en"}}]}]})).unwrap();
        write_json_atomic(
            &path.join("preferences.json"),
            &json!({"future":42,"commands":{"other":{"key":"keep"}}}),
        )
        .unwrap();
        let result=handle(&ctx,"preferences:set",&[json!({"extensionId":"raycast.example","commandName":"run","values":{"language":"Қазақша"}})]).unwrap();
        assert_eq!(result["future"], 42);
        assert_eq!(result["enabled"], false);
        assert_eq!(result["language"], "Қазақша");
        assert_eq!(
            read_json(&path.join("preferences.json")).unwrap()["commands"]["other"]["key"],
            "keep"
        );
        assert_eq!(
            handle(&ctx, "core:extensions:commands", &[]).unwrap()[0]["id"],
            "ext-example-run"
        );
    }
    #[test]
    fn traversal_cannot_read_or_remove_external_data() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        for id in ["../outside", "raycast.../outside", "/tmp/example", "", ".."] {
            assert!(uninstall(&ctx, id).is_err());
            assert!(installed(&ctx, id).is_err());
        }
    }
    #[test]
    fn detects_missing_and_tampered_legacy_scripts() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let root = temp.path().join("extensions/packages/raycast.example");
        write_json_atomic(&root.join("meta.json"),&json!({"commandNames":["present","missing"],"scriptHashes":{"present":"wrong"},"missingScripts":[]})).unwrap();
        std::fs::create_dir(root.join(".sc-build")).unwrap();
        std::fs::write(root.join(".sc-build/present.js"), "export default 1").unwrap();
        let result = integrity(&ctx, "raycast.example").unwrap();
        assert_eq!(result["healthy"], false);
        assert_eq!(result["missingScripts"], json!(["missing"]));
        assert_eq!(result["tamperedScripts"], json!(["present"]));
    }
    #[test]
    fn native_install_reads_legacy_preferences_and_needs_no_script() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let legacy = temp.path().join("extensions/perplexity");
        write_json_atomic(
            &legacy.join("package.json"),
            &json!({"name":"perplexity","commands":[{"name":"ask-perplexity"}]}),
        )
        .unwrap();
        write_json_atomic(
            &legacy.join("preferences.json"),
            &json!({"commands":{"ask-perplexity":{"perplexityApp":true}},"unknown":{"keep":1}}),
        )
        .unwrap();
        let id = "com.tezbar.native.perplexity";
        super::super::v2_catalog::install(&ctx, id)
            .unwrap()
            .unwrap();
        let values = handle(
            &ctx,
            "preferences:get",
            &[json!({"extensionId":id,"commandName":"ask-perplexity"})],
        )
        .unwrap();
        assert_eq!(values["perplexityApp"], true);
        assert_eq!(values["unknown"]["keep"], 1);
        let report = integrity(&ctx, id).unwrap();
        assert_eq!(report["healthy"], true);
        assert_eq!(report["missingScripts"], json!([]));
        assert!(legacy.join("package.json").is_file());
        assert!(legacy.join("preferences.json").is_file());
        assert!(installed_command(&ctx, id, "ask-perplexity").is_some());
        assert_eq!(
            installed_command(&ctx, "raycast.perplexity", "ask-perplexity")
                .unwrap()
                .id,
            id
        );
        let unknown = temp
            .path()
            .join("extensions/packages/com.tezbar.native.perplexity/unknown.bin");
        std::fs::write(&unknown, b"keep").unwrap();
        super::super::v2_catalog::install(&ctx, id)
            .unwrap()
            .unwrap();
        assert_eq!(std::fs::read(unknown).unwrap(), b"keep");
    }
    #[test]
    fn launcher_prefers_native_commands_and_keeps_unported_legacy_commands() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let legacy = temp.path().join("extensions/perplexity");
        write_json_atomic(&legacy.join("package.json"), &json!({
            "name":"perplexity", "platforms":[platform()],
            "commands":[{"name":"ask-perplexity"},{"name":"legacy-only"}]
        })).unwrap();
        let id = "com.tezbar.native.perplexity";
        super::super::v2_catalog::install(&ctx, id).unwrap().unwrap();
        let commands = handle(&ctx, "core:extensions:commands", &[]).unwrap();
        let commands = commands.as_array().unwrap();
        assert_eq!(commands.len(), 2);
        assert_eq!(commands.iter().find(|c| c["cmdName"] == "ask-perplexity").unwrap()["extensionId"], id);
        assert_eq!(commands.iter().find(|c| c["cmdName"] == "legacy-only").unwrap()["extensionId"], "raycast.perplexity");
        assert_eq!(list(&ctx).as_array().unwrap().len(), 2);
        uninstall(&ctx, id).unwrap();
        let fallback = handle(&ctx, "core:extensions:commands", &[]).unwrap();
        assert_eq!(fallback.as_array().unwrap().len(), 2);
        assert!(fallback.as_array().unwrap().iter().all(|c| c["extensionId"] == "raycast.perplexity"));
    }
    #[test]
    fn canonical_uninstall_does_not_remove_legacy_package() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let legacy = temp.path().join("extensions/perplexity");
        write_json_atomic(&legacy.join("package.json"), &json!({"name":"perplexity"})).unwrap();
        super::super::v2_catalog::install(&ctx, "com.tezbar.native.perplexity")
            .unwrap()
            .unwrap();
        uninstall(&ctx, "com.tezbar.native.perplexity").unwrap();
        assert!(legacy.join("package.json").is_file());
        assert!(!super::super::v2_catalog::installed(
            &ctx,
            "com.tezbar.native.perplexity"
        ));
    }
}
