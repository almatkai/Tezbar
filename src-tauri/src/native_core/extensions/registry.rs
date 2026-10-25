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
    data_dir: PathBuf,
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
            let native = name.starts_with("com.tezbar.native.");
            let mut manifest = if native {
                match super::process_install::resolve(ctx, name) {
                    Ok(package) => package.manifest,
                    Err(error) => {
                        // A damaged canonical directory is NEVER a Raycast package.
                        let mut value = read_json(&path.join("package.json"))
                            .ok()
                            .filter(Value::is_object)
                            .unwrap_or(json!({"id":name,"title":slug}));
                        value["healthError"] = json!(error);
                        value["commands"] = json!([]);
                        value
                    }
                }
            } else {
                let Ok(value) = read_json(&path.join("package.json")) else {
                    continue;
                };
                value
            };
            if native {
                manifest["id"] = json!(name);
            }
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
                data_dir: ctx.paths.data_dir.clone(),
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
        let active = extension.manifest["healthError"].is_null()
            && extension.manifest["commands"]
                .as_array()?
                .iter()
                .any(|c| c["name"] == command && c["runtime"] == "rust" && compatible(c, true));
        if active {
            return Some(extension);
        }
        if id.starts_with("com.tezbar.native.") {
            return None;
        }
    }
    if id.starts_with("com.tezbar.native.") {
        return None;
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
    if name.starts_with("https://") || name.starts_with("http://") {
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
    if path.starts_with("http") {
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

fn command_icon_path(extension: &Installed, cmd: &Value) -> Option<String> {
    let name = cmd["icon"].as_str().or_else(|| cmd["iconPath"].as_str())?;
    if name.starts_with("https://") || name.starts_with("http://") {
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

fn command_icon_data(path_str: &str) -> Value {
    let Ok(bytes) = std::fs::read(path_str) else {
        return Value::Null;
    };
    if bytes.len() > 2_000_000 {
        return Value::Null;
    }
    let mime = match Path::new(path_str)
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
    let ext_icon_path = icon_path(extension);
    let owner = person(pkg.get("owner").unwrap_or(&pkg["author"]));
    pkg["commands"].as_array().into_iter().flatten().filter(|c| c["name"].is_string()&&compatible(c,true) && (extension.id.starts_with("raycast.") || c["runtime"] == "rust")).map(|cmd| {
        let name=cmd["name"].as_str().unwrap_or("");
        let title=fallback(cmd,"title",name);
        let extension_title=fallback(pkg,"title",&extension.slug);
        let description=fallback(cmd,"description","");
        let cmd_icon_path = command_icon_path(extension, cmd).or_else(|| ext_icon_path.clone());
        let cmd_icon_data = cmd_icon_path.as_deref().map(command_icon_data).unwrap_or(Value::Null);
        let icon_val = if !cmd_icon_data.is_null() { cmd_icon_data } else { icon.clone() };
        json!({"id":format!("ext-{}-{name}",extension.slug),"title":title,"extensionTitle":extension_title,
            "extName":extension.slug,"extensionId":extension.id,"cmdName":name,"owner":owner,"description":description,"mode":fallback(cmd,"mode","view"),
            "launcher":cmd["launcher"],"interval":cmd["interval"],"disabledByDefault":cmd["disabledByDefault"].as_bool().unwrap_or(false),
            "keywords":[extension.slug.to_lowercase(),extension_title.to_lowercase(),name.to_lowercase(),title.to_lowercase(),description.to_lowercase()],
            "iconPath":cmd_icon_path,"iconDataUrl":icon_val,"commandArgumentDefinitions":cmd["arguments"].as_array().cloned().unwrap_or_default()})
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
        if extension.id.starts_with("com.tezbar.native.") { value["author"]=json!("Tezbar"); value["owner"]=json!("Tezbar"); value["runtime"]=json!("rust"); value["nativeV2"]=super::v2_catalog::describe(pkg.clone()); value["platforms"]=pkg["platforms"].clone(); value["healthError"]=pkg["healthError"].clone(); }
        let rule=super::acquisition::for_id(&extension.id);
        if extension.id.starts_with("com.tezbar.native.") || rule != Ok(super::acquisition::Rule::Remote) { value["acquisition"]=super::acquisition::metadata(rule); }
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
    let saved = ConfigStore::new(preferences_path(extension)?).read()?;
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

// The configured profile root is trusted; its managed descendants cannot redirect
// preference reads/retention/removal through symlinks. Missing paths are distinct
// from unreadable, malformed, or non-regular files, which must fail closed.
fn preference_metadata(base: &Path, path: &Path) -> Result<Option<std::fs::Metadata>, String> {
    let relative = path.strip_prefix(base).map_err(|e| e.to_string())?;
    let mut current = base.to_path_buf();
    let mut parts = relative.components().peekable();
    let mut result = None;
    while let Some(part) = parts.next() {
        if !matches!(part, Component::Normal(_)) {
            return Err("Unsafe preference path".into());
        }
        current.push(part);
        let metadata = match std::fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        if metadata.file_type().is_symlink() || (parts.peek().is_some() && !metadata.is_dir()) {
            return Err("Preference path symlink/non-directory rejected".into());
        }
        result = Some(metadata);
    }
    Ok(result)
}

fn preference_file(base: &Path, path: &Path) -> Result<bool, String> {
    match preference_metadata(base, path)? {
        Some(metadata) if !metadata.is_file() => Err("Preference file is not regular".into()),
        Some(_) => Ok(true),
        None => Ok(false),
    }
}

fn retained_preferences_path(extension: &Installed) -> PathBuf {
    extension
        .data_dir
        .join("extension-preferences")
        .join(format!("{}.json", extension.id))
}

fn preferences_path(extension: &Installed) -> Result<PathBuf, String> {
    let own = extension.path.join("preferences.json");
    if !extension.id.starts_with("com.tezbar.native.") {
        return Ok(own);
    }
    // Explicit external search roots keep their existing boundary. Managed
    // packages additionally validate every descendant of the profile root.
    let base = if extension.path.starts_with(&extension.data_dir) {
        &extension.data_dir
    } else {
        extension.path.parent().unwrap_or(&extension.path)
    };
    if preference_file(base, &own)? {
        return Ok(own);
    }
    let retained = retained_preferences_path(extension);
    if preference_file(&extension.data_dir, &retained)? {
        return Ok(retained);
    }
    let packages = extension.path.parent().unwrap_or(&extension.path);
    let extensions = packages.parent().unwrap_or(packages);
    for legacy in [
        extensions.join(&extension.slug).join("preferences.json"),
        packages
            .join(format!("raycast.{}", extension.slug))
            .join("preferences.json"),
    ] {
        let base = if legacy.starts_with(&extension.data_dir) {
            &extension.data_dir
        } else {
            extensions
        };
        if preference_file(base, &legacy)? {
            return Ok(legacy);
        }
    }
    Ok(own)
}

fn retain_preferences(ctx: &CoreContext, id: &str) -> Result<(), String> {
    // Caller holds PACKAGE_LOCK and the cross-process metadata lease. Use the
    // same managed-root precedence as discovery, including old schema-2 roots.
    let mut selected = None;
    for root in managed_roots(ctx) {
        let path = root.join(id);
        if let Some(metadata) = preference_metadata(&ctx.paths.data_dir, &path)? {
            if !metadata.is_dir() {
                return Err("Canonical package root is not a directory".into());
            }
            if selected.is_none() {
                selected = Some(path);
            }
        }
    }
    let Some(path) = selected else { return Ok(()) };
    let extension = Installed {
        id: id.into(),
        slug: slug(id)?,
        path,
        manifest: Value::Null,
        data_dir: ctx.paths.data_dir.clone(),
    };
    let source = preferences_path(&extension)?;
    if !preference_file(&extension.data_dir, &source)? {
        return Ok(());
    }
    // Validate the selected object even when it already lives outside the
    // activation tree. Never treat an unreadable/corrupt source as defaults.
    let values = ConfigStore::new(source.clone()).read()?;
    let retained = retained_preferences_path(&extension);
    if source == retained {
        return Ok(()); // This is already the current persistent canonical store.
    }
    preference_file(&extension.data_dir, &retained)?;
    // Atomic replacement must finish successfully before uninstall deletes any
    // original files. Overwrite an older retained copy with the current values.
    // Snapshot legacy fallback too: a reimport may use a different managed root
    // with different relative legacy candidates. Never modify the legacy file.
    write_json_atomic(&retained, &values)
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
    if id.starts_with("com.tezbar.native.")
        && super::v2_catalog::slug(id).is_none()
        && !super::process_install::root(ctx, id)?.exists()
    {
        return Err("Unknown native extension".into());
    }
    if id.starts_with("com.tezbar.native.") {
        let root = super::process_install::root(ctx, id)?;
        return Ok(match super::process_install::resolve(ctx, id) {
            Ok(package) => {
                json!({"extensionId":id,"installed":true,"healthy":true,"runtime":"rust","nativeV2":package.manifest,"missingScripts":[],"tamperedScripts":[]})
            }
            Err(error) => {
                json!({"extensionId":id,"installed":root.exists(),"healthy":false,"runtime":"rust","lastError":error,"installationStatus":"needs-upgrade-or-repair","missingScripts":[],"tamperedScripts":[]})
            }
        });
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
    let _package_guard = super::process_install::PACKAGE_LOCK
        .lock()
        .map_err(|e| e.to_string())?;
    if id.starts_with("com.tezbar.native.")
        && super::v2_catalog::slug(id).is_none()
        && !super::process_install::root(ctx, id)?.exists()
    {
        return Err("Unknown native extension".into());
    }
    let slug = slug(id)?;
    let _metadata_lease = super::process_install::metadata_lock(ctx)?;
    let canonical = id.starts_with("com.tezbar.native.");
    if canonical {
        retain_preferences(ctx, &format!("com.tezbar.native.{slug}"))?;
        super::process_host::forget_background(ctx, id)?;
        super::launcher::purge(ctx, id)?;
    }
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
            Ok(json!(extensions
                .iter()
                .flat_map(command_values)
                .filter(|command| { seen.insert(command["id"].as_str().unwrap_or("").to_owned()) })
                .collect::<Vec<_>>()))
        }
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
                let id = item["id"].as_str().unwrap_or("").to_owned();
                let rule = super::acquisition::for_id(&id);
                if id.starts_with("com.tezbar.native.")
                    || rule != Ok(super::acquisition::Rule::Remote)
                {
                    item["acquisition"] = super::acquisition::metadata(rule);
                }
                if id.starts_with("com.tezbar.native.") {
                    match super::process_install::resolve(ctx, &id) {
                        Ok(package) => {
                            item["nativeV2"] =
                                super::v2_catalog::describe(package.manifest.clone());
                            item["commands"] = package.manifest["commands"].clone();
                            item["platforms"] = package.manifest["platforms"].clone();
                            item["installationStatus"] = json!("installed");
                        }
                        Err(error) => {
                            item["installationStatus"] = json!("needs-upgrade-or-repair");
                            item["healthError"] = json!(error);
                            item["commands"] = json!([]);
                            if let Some(meta) =
                                super::v2_runtime::slug(&id).and_then(super::v2_catalog::manifest)
                            {
                                item["nativeV2"] = meta;
                            }
                        }
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
            // Match install/uninstall lock order. A UI/CLI writer cannot race
            // retention, lose a newer value, or recreate an uninstalled root.
            let _package_guard = super::process_install::PACKAGE_LOCK
                .lock()
                .map_err(|e| e.to_string())?;
            let _metadata_lease = super::process_install::metadata_lock(ctx)?;
            let extension = installed(ctx, id)?;
            let command = body["commandName"].as_str();
            if channel == "preferences:set" {
                let values = body["values"].as_object().cloned().unwrap_or_default();
                ConfigStore::new(preferences_path(&extension)?).update(|current| {
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
                    "preferences":preference_schemas(&extension,command),"values":values,"hasSavedPreferences":preferences_path(&extension)?.is_file()}))
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
        super::super::process_install::test_install(&ctx, "normal");
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
        super::super::process_install::test_install(&ctx, "normal");
        assert_eq!(std::fs::read(unknown).unwrap(), b"keep");
    }
    #[test]
    fn launcher_prefers_native_commands_and_keeps_unported_legacy_commands() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let legacy = temp.path().join("extensions/perplexity");
        write_json_atomic(
            &legacy.join("package.json"),
            &json!({
                "name":"perplexity", "platforms":[platform()],
                "commands":[{"name":"ask-perplexity"},{"name":"legacy-only"}]
            }),
        )
        .unwrap();
        let id = "com.tezbar.native.perplexity";
        super::super::process_install::test_install(&ctx, "normal");
        let commands = handle(&ctx, "core:extensions:commands", &[]).unwrap();
        let commands = commands.as_array().unwrap();
        assert_eq!(commands.len(), 2);
        assert_eq!(
            commands
                .iter()
                .find(|c| c["cmdName"] == "ask-perplexity")
                .unwrap()["extensionId"],
            id
        );
        assert_eq!(
            commands
                .iter()
                .find(|c| c["cmdName"] == "legacy-only")
                .unwrap()["extensionId"],
            "raycast.perplexity"
        );
        assert_eq!(list(&ctx).as_array().unwrap().len(), 2);
        uninstall(&ctx, id).unwrap();
        let fallback = handle(&ctx, "core:extensions:commands", &[]).unwrap();
        assert_eq!(fallback.as_array().unwrap().len(), 2);
        assert!(fallback
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["extensionId"] == "raycast.perplexity"));
    }
    #[test]
    fn canonical_uninstall_does_not_remove_legacy_package() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let legacy = temp.path().join("extensions/perplexity");
        write_json_atomic(&legacy.join("package.json"), &json!({"name":"perplexity"})).unwrap();
        super::super::process_install::test_install(&ctx, "normal");
        uninstall(&ctx, "com.tezbar.native.perplexity").unwrap();
        assert!(legacy.join("package.json").is_file());
        assert!(!super::super::v2_catalog::installed(
            &ctx,
            "com.tezbar.native.perplexity"
        ));
    }
    // Start with the existing real rustc fixture installer, then package its
    // verified executable in an owned ZIP for repeated imports of identical bytes.
    fn preference_package(ctx: &CoreContext) -> (Value, PathBuf) {
        use std::io::Write;
        let mut manifest = super::super::process_install::test_install_with(ctx, "normal", |m| {
            m["preferences"] = json!([
                {"name":"theme","default":"default"},
                {"name":"newGlobal","default":17}
            ]);
            m["commands"][0]["preferences"] = json!([
                {"name":"language","default":"en"},
                {"name":"newCommand","default":"command-default"}
            ]);
        });
        let package =
            super::super::process_install::resolve(ctx, manifest["id"].as_str().unwrap()).unwrap();
        let executable = manifest["artifacts"][super::super::process_install::target()]
            ["executable"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut packaged = manifest.clone();
        packaged["artifacts"] = json!({});
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file("extension.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&serde_json::to_vec(&packaged).unwrap())
            .unwrap();
        zip.start_file(
            &executable,
            zip::write::SimpleFileOptions::default().unix_permissions(0o755),
        )
        .unwrap();
        zip.write_all(&std::fs::read(package.executable).unwrap())
            .unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        manifest["artifacts"][super::super::process_install::target()]["sha256"] =
            json!(format!("{:x}", Sha256::digest(&bytes)));
        let archive = ctx.paths.temp_dir.join("retention-fixture.zip");
        std::fs::create_dir_all(&ctx.paths.temp_dir).unwrap();
        std::fs::write(&archive, bytes).unwrap();
        import_preferences_package(ctx, &manifest, &archive);
        (manifest, archive)
    }

    fn import_preferences_package(ctx: &CoreContext, manifest: &Value, archive: &Path) {
        super::super::process_install::test_import_target(
            ctx,
            manifest,
            super::super::process_install::target(),
            archive,
        )
        .unwrap();
    }

    async fn core_value(core: &super::super::ExtensionsCore, channel: &str, body: Value) -> Value {
        core.handle(channel, &[body]).await.unwrap().unwrap()
    }

    #[tokio::test]
    async fn canonical_preferences_survive_core_uninstall_and_identical_reimports() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let (manifest, archive) = preference_package(&ctx);
        let id = manifest["id"].as_str().unwrap();
        let extension = installed(&ctx, id).unwrap();
        let own = extension.path.join("preferences.json");
        let retained = retained_preferences_path(&extension);
        assert!(!own.exists() && !retained.exists());
        assert!(!ctx
            .paths
            .data_dir
            .join("extensions/perplexity/preferences.json")
            .exists());
        let core = super::super::ExtensionsCore::new(ctx.clone());
        core_value(
            &core,
            "preferences:set",
            json!({"extensionId":id,"values":{
                "theme":"café 雪", "future":{"array":[null,false,5]},
                "commands":{"other":{"keep":"scope"}}
            }}),
        )
        .await;
        core_value(
            &core,
            "preferences:set",
            json!({"extensionId":id,
                "commandName":"ask-perplexity","values":{"language":"Қазақша","theme":"command"}
            }),
        )
        .await;
        let saved = read_json(&own).unwrap();
        let domain = ctx
            .paths
            .data_dir
            .join("external-extensions")
            .join(id)
            .join("domain.json");
        write_json_atomic(&domain, &json!({"domain":["keep",null,9]})).unwrap();
        let unrelated = ctx.paths.data_dir.join("extensions/unrelated");
        write_json_atomic(
            &unrelated.join("package.json"),
            &json!({"name":"Unrelated"}),
        )
        .unwrap();
        std::fs::write(unrelated.join("marker"), b"keep unrelated package").unwrap();
        let domain_bytes = std::fs::read(&domain).unwrap();
        let unrelated_bytes = std::fs::read(unrelated.join("package.json")).unwrap();
        for name in ["background.json", "startup-probes.json"] {
            write_json_atomic(
                &ctx.paths.data_dir.join("extensions").join(name),
                &json!({id:{"owned":"remove"},"raycast.unrelated":{"owned":"keep"}}),
            )
            .unwrap();
        }
        let index = ctx
            .paths
            .data_dir
            .join("extensions/launcher-index")
            .join(format!("{id}.json"));
        let other_index = index.with_file_name("raycast.unrelated.json");
        write_json_atomic(&index, &json!({"owned":"remove"})).unwrap();
        write_json_atomic(&other_index, &json!({"owned":"keep"})).unwrap();
        for revision in [1, 2] {
            let active = super::super::process_install::resolve(&ctx, id).unwrap();
            assert!(active.executable.is_file());
            core_value(&core, "extension:uninstall", json!(id)).await;
            assert!(!extension.path.exists(), "no preferences-only phantom root");
            assert!(!active.dir.exists() && !active.executable.exists());
            assert!(!extension.path.join("active.json").exists());
            assert!(!index.exists());
            for channel in [
                "extension:list",
                "extensions:listInstalled",
                "core:extensions:commands",
            ] {
                let values = core_value(&core, channel, Value::Null).await;
                assert!(
                    values
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|v| v["id"] != id && v["extensionId"] != id),
                    "{channel}: {values}"
                );
            }
            assert!(super::super::process_install::resolve(&ctx, id).is_err());
            assert!(core
                .handle(
                    "preferences:set",
                    &[json!({"extensionId":id,"values":{"late":true}})]
                )
                .await
                .unwrap()
                .is_err());
            assert!(
                !extension.path.exists(),
                "late save must not recreate the package"
            );
            for name in ["background.json", "startup-probes.json"] {
                let db = read_json(&ctx.paths.data_dir.join("extensions").join(name)).unwrap();
                assert!(db.get(id).is_none());
                assert_eq!(db["raycast.unrelated"], json!({"owned":"keep"}));
            }
            assert_eq!(std::fs::read(&domain).unwrap(), domain_bytes);
            assert_eq!(
                std::fs::read(unrelated.join("package.json")).unwrap(),
                unrelated_bytes
            );
            assert_eq!(
                std::fs::read(unrelated.join("marker")).unwrap(),
                b"keep unrelated package"
            );
            assert_eq!(read_json(&other_index).unwrap(), json!({"owned":"keep"}));
            let expected = if revision == 1 {
                saved.clone()
            } else {
                json!({"theme":"latest","future":{"array":[null,false,5]},
                    "commands":{"other":{"keep":"scope"},"ask-perplexity":{
                        "language":"latest-language","theme":"command"}}})
            };
            assert_eq!(read_json(&retained).unwrap(), expected);
            import_preferences_package(&ctx, &manifest, &archive);
            assert!(!own.exists());
            let values = core_value(&core, "preferences:get", json!({"extensionId":id})).await;
            assert_eq!(values["theme"], expected["theme"]);
            assert_eq!(values["newGlobal"], 17);
            let values = core_value(
                &core,
                "preferences:get",
                json!({
                "extensionId":id,"commandName":"ask-perplexity"}),
            )
            .await;
            assert_eq!(
                values["language"],
                expected["commands"]["ask-perplexity"]["language"]
            );
            assert_eq!(values["theme"], "command");
            assert_eq!(values["newCommand"], "command-default");
            assert_eq!(values["future"], saved["future"]);
            if revision == 1 {
                core_value(
                    &core,
                    "preferences:set",
                    json!({"extensionId":id,
                    "values":{"theme":"latest"}}),
                )
                .await;
                core_value(
                    &core,
                    "preferences:set",
                    json!({"extensionId":id,
                    "commandName":"ask-perplexity","values":{"language":"latest-language"}}),
                )
                .await;
            }
        }
        assert!(!core.worker_running().await);
        core.shutdown().await;
    }

    #[test]
    fn legacy_fallback_and_own_retained_precedence_survive_uninstall() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let (manifest, archive) = preference_package(&ctx);
        let id = manifest["id"].as_str().unwrap();
        let legacy = ctx.paths.data_dir.join("extensions/perplexity");
        write_json_atomic(&legacy.join("package.json"), &json!({"name":"Legacy"})).unwrap();
        let legacy_preferences = legacy.join("preferences.json");
        write_json_atomic(
            &legacy_preferences,
            &json!({"theme":"legacy","commands":{
            "ask-perplexity":{"language":"legacy-language"}}}),
        )
        .unwrap();
        assert_eq!(
            handle(&ctx, "preferences:get", &[json!({"extensionId":id})]).unwrap()["theme"],
            "legacy"
        );
        // Preserve the existing shared legacy-write behavior before retention exists.
        handle(
            &ctx,
            "preferences:set",
            &[json!({"extensionId":id,"values":{"theme":"legacy-latest"}})],
        )
        .unwrap();
        let legacy_bytes = std::fs::read(&legacy_preferences).unwrap();
        let extension = installed(&ctx, id).unwrap();
        let retained = retained_preferences_path(&extension);
        uninstall(&ctx, id).unwrap();
        assert_eq!(
            read_json(&retained).unwrap(),
            read_json(&legacy_preferences).unwrap()
        );
        assert_eq!(std::fs::read(&legacy_preferences).unwrap(), legacy_bytes);
        import_preferences_package(&ctx, &manifest, &archive);
        assert_eq!(
            handle(&ctx, "preferences:get", &[json!({"extensionId":id})]).unwrap()["theme"],
            "legacy-latest"
        );
        write_json_atomic(&retained, &json!({"theme":"retained-current"})).unwrap();
        assert_eq!(
            handle(&ctx, "preferences:get", &[json!({"extensionId":id})]).unwrap()["theme"],
            "retained-current"
        );
        write_json_atomic(
            &extension.path.join("preferences.json"),
            &json!({"theme":"current-own",
            "commands":{"ask-perplexity":{"language":"current-language"}}}),
        )
        .unwrap();
        assert_eq!(
            handle(&ctx, "preferences:get", &[json!({"extensionId":id})]).unwrap()["theme"],
            "current-own"
        );
        uninstall(&ctx, id).unwrap();
        assert_eq!(read_json(&retained).unwrap()["theme"], "current-own");
        assert_eq!(std::fs::read(&legacy_preferences).unwrap(), legacy_bytes);
        import_preferences_package(&ctx, &manifest, &archive);
        assert_eq!(
            handle(
                &ctx,
                "preferences:get",
                &[json!({"extensionId":id,
            "commandName":"ask-perplexity"})]
            )
            .unwrap()["language"],
            "current-language"
        );
        assert_eq!(
            handle(
                &ctx,
                "preferences:get",
                &[json!({"extensionId":"raycast.perplexity"})]
            )
            .unwrap()["theme"],
            "legacy-latest"
        );
        assert_eq!(
            handle(&ctx, "preferences:get", &[json!({"extensionId":id})]).unwrap()["theme"],
            "current-own"
        );
        // Removing the legacy identity cannot erase canonical retained preferences.
        uninstall(&ctx, "raycast.perplexity").unwrap();
        assert!(super::super::process_install::resolve(&ctx, id).is_ok());
        assert_eq!(read_json(&retained).unwrap()["theme"], "current-own");
    }

    #[test]
    fn old_canonical_preferences_are_retained_from_every_managed_root() {
        for root_index in 0..3 {
            let temp = tempfile::tempdir().unwrap();
            let ctx = context(temp.path());
            let id = "com.tezbar.native.perplexity";
            let root = managed_roots(&ctx)[root_index].join(id);
            // A genuine obsolete schema-2 shape: explicitly needs upgrade, no fake health.
            write_json_atomic(
                &root.join("package.json"),
                &json!({"schemaVersion":2,
                "id":id,"name":"Perplexity","commands":[]}),
            )
            .unwrap();
            let saved =
                json!({"unknown":[null,"雪",false],"commands":{"ask-perplexity":{"saved":23}}});
            write_json_atomic(&root.join("preferences.json"), &saved).unwrap();
            assert!(installed(&ctx, id).unwrap().manifest["healthError"].is_string());
            uninstall(&ctx, id).unwrap();
            assert!(!root.exists());
            assert!(list(&ctx).as_array().unwrap().is_empty());
            super::super::process_install::test_install(&ctx, "normal");
            let extension = installed(&ctx, id).unwrap();
            assert_eq!(
                read_json(&preferences_path(&extension).unwrap()).unwrap(),
                saved
            );
            assert_eq!(
                preferences(&extension, Some("ask-perplexity")).unwrap()["saved"],
                23
            );
        }
    }

    #[test]
    fn legacy_fallback_retention_survives_a_changed_managed_install_root() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let id = "com.tezbar.native.perplexity";
        let root = ctx
            .paths
            .data_dir
            .join("extension-registry/packages")
            .join(id);
        write_json_atomic(
            &root.join("package.json"),
            &json!({"schemaVersion":2,"id":id}),
        )
        .unwrap();
        let legacy = ctx
            .paths
            .data_dir
            .join("extension-registry/perplexity/preferences.json");
        let saved = json!({"global":"saved","commands":{"ask-perplexity":{"value":"saved"}}});
        write_json_atomic(&legacy, &saved).unwrap();
        let legacy_bytes = std::fs::read(&legacy).unwrap();
        assert_eq!(
            preferences(&installed(&ctx, id).unwrap(), None).unwrap()["global"],
            "saved"
        );
        uninstall(&ctx, id).unwrap();
        super::super::process_install::test_install(&ctx, "normal");
        let extension = installed(&ctx, id).unwrap();
        assert_eq!(
            read_json(&preferences_path(&extension).unwrap()).unwrap(),
            saved
        );
        assert_eq!(
            preferences(&extension, Some("ask-perplexity")).unwrap()["value"],
            "saved"
        );
        assert_eq!(std::fs::read(&legacy).unwrap(), legacy_bytes);
    }

    #[test]
    fn retention_failures_leave_original_package_and_ownership_untouched() {
        for failure in [
            "corrupt-own",
            "own-directory",
            "blocked-parent",
            "blocked-target",
            "corrupt-retained",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let ctx = context(temp.path());
            super::super::process_install::test_install(&ctx, "normal");
            let id = "com.tezbar.native.perplexity";
            let extension = installed(&ctx, id).unwrap();
            let own = extension.path.join("preferences.json");
            let retained = retained_preferences_path(&extension);
            std::fs::write(&own, b"{\"saved\":\"original\"}").unwrap();
            match failure {
                "corrupt-own" => std::fs::write(&own, b"{corrupt").unwrap(),
                "own-directory" => {
                    std::fs::remove_file(&own).unwrap();
                    std::fs::create_dir(&own).unwrap();
                }
                "blocked-parent" => std::fs::write(retained.parent().unwrap(), b"blocked").unwrap(),
                "blocked-target" => std::fs::create_dir_all(&retained).unwrap(),
                "corrupt-retained" => {
                    std::fs::remove_file(&own).unwrap();
                    std::fs::create_dir_all(retained.parent().unwrap()).unwrap();
                    std::fs::write(&retained, b"{corrupt").unwrap();
                }
                _ => unreachable!(),
            }
            let active = super::super::process_install::resolve(&ctx, id).unwrap();
            let executable_bytes = std::fs::read(&active.executable).unwrap();
            let db = ctx.paths.data_dir.join("extensions/installed.json");
            let active_path = extension.path.join("active.json");
            let mut guarded = vec![db, active_path];
            for name in [
                "background.json",
                "startup-probes.json",
                "launcher-index/com.tezbar.native.perplexity.json",
            ] {
                let path = ctx.paths.data_dir.join("extensions").join(name);
                write_json_atomic(&path, &json!({id:{"keep":true}})).unwrap();
                guarded.push(path);
            }
            if own.is_file() {
                guarded.push(own.clone());
            }
            if retained.is_file() {
                guarded.push(retained.clone());
            }
            let snapshots: Vec<_> = guarded.iter().map(|p| std::fs::read(p).unwrap()).collect();
            assert!(uninstall(&ctx, id).is_err(), "{failure}");
            assert!(extension.path.is_dir() && active.dir.is_dir());
            assert_eq!(std::fs::read(active.executable).unwrap(), executable_bytes);
            for (path, before) in guarded.iter().zip(snapshots) {
                assert_eq!(
                    std::fs::read(path).unwrap(),
                    before,
                    "{failure}: {}",
                    path.display()
                );
            }
            assert_eq!(list(&ctx).as_array().unwrap().len(), 1);
        }
    }

    #[cfg(unix)]
    #[test]
    fn preference_symlinks_cannot_redirect_retention_or_removal() {
        use std::os::unix::fs::symlink;
        for boundary in ["own", "retained-parent", "retained-file", "managed-parent"] {
            let temp = tempfile::tempdir().unwrap();
            let ctx = context(temp.path());
            super::super::process_install::test_install(&ctx, "normal");
            let id = "com.tezbar.native.perplexity";
            let extension = installed(&ctx, id).unwrap();
            let own = extension.path.join("preferences.json");
            let retained = retained_preferences_path(&extension);
            write_json_atomic(&own, &json!({"saved":"original"})).unwrap();
            let outside = temp.path().join("outside");
            std::fs::create_dir(&outside).unwrap();
            let outside_preferences = outside.join("preferences.json");
            std::fs::write(&outside_preferences, b"{\"outside\":true}").unwrap();
            match boundary {
                "own" => {
                    std::fs::remove_file(&own).unwrap();
                    symlink(&outside_preferences, &own).unwrap();
                }
                "retained-parent" => symlink(&outside, retained.parent().unwrap()).unwrap(),
                "retained-file" => {
                    std::fs::create_dir_all(retained.parent().unwrap()).unwrap();
                    symlink(&outside_preferences, &retained).unwrap();
                }
                "managed-parent" => {
                    symlink(&outside, ctx.paths.data_dir.join("extension-registry")).unwrap()
                }
                _ => unreachable!(),
            }
            assert!(uninstall(&ctx, id).is_err(), "{boundary}");
            assert!(extension.path.join("active.json").is_file());
            assert_eq!(
                std::fs::read(&outside_preferences).unwrap(),
                b"{\"outside\":true}"
            );
        }
    }

    #[test]
    fn concurrent_save_and_uninstall_keep_the_latest_acknowledged_preferences() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        super::super::process_install::test_install(&ctx, "normal");
        let id = "com.tezbar.native.perplexity";
        let extension = installed(&ctx, id).unwrap();
        write_json_atomic(
            &extension.path.join("preferences.json"),
            &json!({"saved":"before"}),
        )
        .unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let writer_ctx = ctx.clone();
        let writer_barrier = barrier.clone();
        let writer = std::thread::spawn(move || {
            writer_barrier.wait();
            handle(
                &writer_ctx,
                "preferences:set",
                &[json!({"extensionId":id,"values":{"saved":"latest"}})],
            )
        });
        let remover_ctx = ctx.clone();
        let remover = std::thread::spawn(move || {
            barrier.wait();
            uninstall(&remover_ctx, id)
        });
        let saved = writer.join().unwrap();
        remover.join().unwrap().unwrap();
        let retained = read_json(&retained_preferences_path(&extension)).unwrap();
        match saved {
            Ok(_) => assert_eq!(retained["saved"], "latest"),
            Err(error) => {
                assert!(error.contains("not installed"), "{error}");
                assert_eq!(retained["saved"], "before");
            }
        }
        assert!(!extension.path.exists());
        assert!(list(&ctx).as_array().unwrap().is_empty());
    }
}

/// Verified metadata snapshot used by generic launcher contribution selection.
pub(super) fn launcher_packages(ctx: &CoreContext) -> Vec<super::process_install::Package> {
    use std::io::Read;
    // Inspect small active manifests first. Never hash all ordinary extension
    // executables merely to answer an unrelated keystroke or find contributors.
    let mut packages = Vec::new();
    for entry in std::fs::read_dir(ctx.paths.data_dir.join("extensions/packages"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .take(512)
    {
        let Some(id) = entry
            .file_name()
            .to_str()
            .map(str::to_owned)
            .filter(|s| s.starts_with("com.tezbar.native."))
        else {
            continue;
        };
        let candidate = (|| -> Option<super::process_install::Package> {
            let root = super::process_install::root(ctx, &id).ok()?;
            let active = read_json(&root.join("active.json")).ok()?;
            let version = active["directory"].as_str()?;
            super::process_install::component(version).ok()?;
            let dir = root.join("versions").join(version);
            if std::fs::symlink_metadata(&dir)
                .ok()?
                .file_type()
                .is_symlink()
            {
                return None;
            }
            let path = dir.join("extension.json");
            let meta = std::fs::symlink_metadata(&path).ok()?;
            if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 1024 * 1024 {
                return None;
            }
            let mut bytes = Vec::new();
            std::fs::File::open(path)
                .ok()?
                .take(1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .ok()?;
            if bytes.len() > 1024 * 1024 {
                return None;
            }
            let hint: Value = serde_json::from_slice(&bytes).ok()?;
            if !hint["launcherContributions"]
                .as_array()
                .is_some_and(|a| !a.is_empty())
            {
                return None;
            }
            // The untrusted hint only selects verification; it grants no authority.
            super::process_install::resolve(ctx, &id).ok()
        })();
        if let Some(package) = candidate {
            packages.push(package);
            if packages.len() >= 64 {
                break;
            }
        }
    }
    packages
}
