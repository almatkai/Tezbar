use crate::common::{
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

#[path = "command_inputs.rs"]
mod inputs;
#[path = "asset_scope.rs"]
mod asset_scope;
pub(crate) use asset_scope::{scope as asset_scope, scope_for_root as asset_scope_for_root};
pub(super) use inputs::command_inputs;

pub(super) struct Installed {
    id: String,
    slug: String,
    path: PathBuf,
    package_dir: Option<PathBuf>,
    manifest: Value,
    data_dir: PathBuf,
}

fn slug(id: &str) -> Result<String, String> {
    let name = id
        .trim()
        .strip_prefix("com.tezbar.native.")
        .or_else(|| id.trim().strip_prefix("raycast."))
        .or_else(|| id.trim().strip_prefix("com.tezbar."))
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

fn profile_roots(data_dir: &Path) -> Vec<PathBuf> {
    vec![
        data_dir.join("extensions"),
        data_dir.join("extensions/packages"),
        data_dir.join("extension-registry/packages"),
    ]
}

fn managed_roots(ctx: &CoreContext) -> Vec<PathBuf> {
    profile_roots(&ctx.paths.data_dir)
}

fn roots(ctx: &CoreContext) -> Vec<PathBuf> {
    let mut result = managed_roots(ctx);
    if let Some(paths) = std::env::var_os("SUPERCMD_EXTENSION_PATHS") {
        result.extend(std::env::split_paths(&paths));
    }
    result
}

// One discovery implementation for the launcher and migration ownership. The
// launcher tolerates unreadable roots; migration cannot treat them as absence.
fn root_discovery_paths(root: &Path, strict: bool) -> Result<Vec<PathBuf>, String> {
    let mut paths = if root.join("package.json").is_file() {
        vec![root.to_path_buf()]
    } else {
        match std::fs::read_dir(root) {
            Ok(entries) => {
                let mut paths = Vec::new();
                for entry in entries {
                    match entry {
                        Ok(entry) => paths.push(entry.path()),
                        Err(e) if strict => return Err(e.to_string()),
                        Err(_) => {}
                    }
                }
                paths
            }
            Err(e) if strict && e.kind() != std::io::ErrorKind::NotFound => {
                return Err(e.to_string())
            }
            Err(_) => Vec::new(),
        }
    };
    // Discovery is flat → managed → registry, then bare slug → raycast.slug
    // within each root. Preference migration must select this same owner.
    paths.sort_by_key(|path| {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        (
            slug(name).unwrap_or_else(|_| name.into()),
            name.starts_with("raycast."),
            name.to_owned(),
        )
    });
    Ok(paths)
}

fn discovery_paths(ctx: &CoreContext) -> Vec<PathBuf> {
    roots(ctx)
        .iter()
        .flat_map(|root| root_discovery_paths(root, false).unwrap_or_default())
        .collect()
}

fn package_identity(path: &Path) -> Option<(String, String, bool)> {
    let name = path.file_name()?.to_str()?;
    let slug = slug(name).ok()?;
    let native = name.starts_with("com.tezbar.native.");
    let id = if native {
        name.to_owned()
    } else {
        format!("raycast.{slug}")
    };
    Some((id, slug, native))
}

/// Parse a declaration's directory-identity slot, not an arbitrary ancestor.
/// The overlapping `extensions/packages` namespace has two structural roles:
/// direct files (or dot-prefixed support paths) can belong to flat `packages`,
/// but a directory-identity component below it names a managed slot. Root
/// package masking must never turn that managed slot into flat-package support.
/// This grammar is independent of source-file presence, validity and config.
/// Only the caller's original declaration is granted; this path is identity only.
pub(super) fn legacy_migration_slot(relative: &Path) -> Option<PathBuf> {
    let parts: Vec<_> = relative.components().map(|c| c.as_os_str()).collect();
    let is = |index: usize, name: &str| {
        parts
            .get(index)
            .and_then(|p| p.to_str())
            .is_some_and(|p| p.eq_ignore_ascii_case(name))
    };
    let slot_len = if is(0, "extensions") {
        if is(1, "packages")
            && parts.len() >= 4
            && package_identity(&parts[..3].iter().collect::<PathBuf>()).is_some()
        {
            3
        } else {
            2
        }
    } else if is(0, "extension-registry") && is(1, "packages") {
        3
    } else {
        return None;
    };
    if parts.len() <= slot_len {
        return None; // Discovery-root manifests and directories are not slots.
    }
    let slot: PathBuf = parts[..slot_len].iter().collect();
    let (_, _, native) = package_identity(&slot)?;
    (!native).then_some(slot)
}

// Both readers skip malformed/non-object manifests. Only the migration reader
// propagates I/O failures: an unobservable candidate may be the selected owner.
fn legacy_manifest(path: &Path) -> Result<Option<Value>, std::io::Error> {
    let bytes = match std::fs::read(path.join("package.json")) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    Ok(serde_json::from_slice::<Value>(&bytes)
        .ok()
        .filter(Value::is_object))
}

/// Observe actual discovery identities before applying any file grant filter.
/// Do not reuse the narrower preference fallback scanner: it has a different
/// compatibility boundary and intentionally does not search external roots.
pub(super) fn legacy_migration_owners(
    ctx: &CoreContext,
    ids: &HashSet<String>,
) -> Result<Vec<PathBuf>, String> {
    let mut remaining = ids.clone();
    let mut owners = Vec::new();
    for root in roots(ctx) {
        if remaining.is_empty() {
            break;
        }
        // A root manifest masks children even when malformed. Check the evidence
        // itself before is_file()/read_dir can silently erase it.
        let base = if root.starts_with(&ctx.paths.data_dir) {
            &ctx.paths.data_dir
        } else {
            &root
        };
        migration_metadata(base, &root)?;
        let root_manifest = root.join("package.json");
        if let Some(metadata) = migration_metadata(base, &root_manifest)? {
            if !metadata.is_file() {
                return Err(migration_error(
                    &root_manifest,
                    "root manifest is not a regular file",
                ));
            }
            std::fs::File::open(&root_manifest)
                .map_err(|e| migration_error(&root_manifest, &e.to_string()))?;
        }
        for path in root_discovery_paths(&root, true).map_err(|e| migration_error(&root, &e))? {
            let Some((id, _, native)) = package_identity(&path) else {
                continue;
            };
            if native || !remaining.contains(&id.to_lowercase()) {
                continue;
            }
            // Selection follows the same manifests as collect(), including
            // symlink-selected directories/metadata. Safety cannot erase an
            // otherwise selected identity and promote a lower duplicate.
            // Refuse special/unobservable files before attempting a read.
            let manifest = path.join("package.json");
            match std::fs::metadata(&manifest) {
                Ok(metadata) if !metadata.is_file() => {
                    return Err(migration_error(
                        &manifest,
                        "candidate manifest is not a regular file",
                    ));
                }
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(migration_error(&manifest, &e.to_string()));
                }
                _ => {}
            }
            if legacy_manifest(&path)
                .map_err(|e| migration_error(&path, &e.to_string()))?
                .is_some()
            {
                migration_metadata(base, &path)?;
                migration_metadata(base, &manifest)?;
                remaining.remove(&id.to_lowercase());
                owners.push(path);
            }
        }
    }
    Ok(owners)
}

pub(super) fn migration_error(path: &Path, reason: &str) -> String {
    format!("Legacy migration cannot establish safe source evidence at {}: {reason}. Repair the source path/permissions and retry; no migration was initialized.", path.display())
}

/// Missing is evidence only after every existing ancestor has been observed.
/// Never follow symlinks (including dangling ones) or accept special files.
pub(super) fn migration_metadata(
    base: &Path,
    path: &Path,
) -> Result<Option<std::fs::Metadata>, String> {
    let relative = path
        .strip_prefix(base)
        .map_err(|e| migration_error(path, &e.to_string()))?;
    let mut current = base.to_path_buf();
    let mut parts = relative.components().peekable();
    loop {
        let metadata = match std::fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(migration_error(&current, &e.to_string())),
        };
        if metadata.file_type().is_symlink()
            || (!metadata.is_file() && !metadata.is_dir())
            || (parts.peek().is_some() && !metadata.is_dir())
        {
            return Err(migration_error(
                &current,
                "symlink or non-regular path rejected",
            ));
        }
        let Some(part) = parts.next() else {
            return Ok(Some(metadata));
        };
        if !matches!(part, Component::Normal(_)) {
            return Err(migration_error(path, "invalid path component"));
        }
        current.push(part);
    }
}

fn collect(ctx: &CoreContext) -> Vec<Installed> {
    collect_paths(ctx, discovery_paths(ctx))
}

fn collect_paths(ctx: &CoreContext, paths: impl IntoIterator<Item = PathBuf>) -> Vec<Installed> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for path in paths {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Ok(slug) = slug(name) else { continue };
        let native = name.starts_with("com.tezbar.native.");
        let mut package_dir = None;
        let mut manifest = if native {
            match super::process_install::resolve(ctx, name) {
                Ok(package) => {
                    package_dir = Some(package.dir);
                    package.manifest
                }
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
            package_dir,
            manifest,
            data_dir: ctx.paths.data_dir.clone(),
        });
    }
    found
}

fn installed(ctx: &CoreContext, id: &str) -> Result<Installed, String> {
    let wanted = slug(id)?;
    let canonical = id.trim().starts_with("com.tezbar.native.");
    // Keep discovery precedence and selected-package validation, but do not hash
    // every installed executable to retrieve one command's preferences.
    let paths = discovery_paths(ctx).into_iter().filter(|path| {
        path.file_name().and_then(|name| name.to_str()).is_some_and(|name| {
            name.starts_with("com.tezbar.native.") == canonical
                && slug(name).is_ok_and(|name| name == wanted)
        })
    });
    collect_paths(ctx, paths)
        .into_iter()
        .next()
        .ok_or_else(|| format!("Extension is not installed: {id}"))
}

/// Native implementations use the same discovery and platform rules as the launcher.
pub(super) fn installed_command(ctx: &CoreContext, id: &str, command: &str) -> Option<Installed> {
    let native = if id.starts_with("raycast.") || id.starts_with("com.tezbar.") && !id.starts_with("com.tezbar.native.") {
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
    resource_root(extension)?;
    let name = fallback(&extension.manifest, "icon", "icon.png");
    if name.starts_with("https://") || name.starts_with("http://") {
        return Some(name.into());
    }
    let relative = icon_relative(name)?;
    icon_candidates(extension, &relative)
        .into_iter()
        .find(|p| contained_icon_file(extension, p))
        .map(|p| p.to_string_lossy().into())
}

fn resource_root(extension: &Installed) -> Option<&Path> {
    if extension.id.starts_with("com.tezbar.native.") {
        extension.package_dir.as_deref()
    } else {
        Some(&extension.path)
    }
}

fn icon_relative(name: &str) -> Option<PathBuf> {
    // Apply Windows path rules on every host, preserving the legacy ./ spelling.
    super::process_install::relative(name.strip_prefix("./").unwrap_or(name)).ok()
}

fn icon_candidates(extension: &Installed, relative: &Path) -> Vec<PathBuf> {
    let Some(root) = resource_root(extension) else {
        return vec![];
    };
    vec![root.join(relative), root.join("assets").join(relative)]
}

fn contained_icon_file(extension: &Installed, path: &Path) -> bool {
    let Some(root) = resource_root(extension) else {
        return false;
    };
    if extension.package_dir.is_some() {
        let Some(member) = path.strip_prefix(root).ok().and_then(Path::to_str) else {
            return false;
        };
        let key = member.replace(std::path::MAIN_SEPARATOR, "/");
        if extension.manifest["files"].get(&key).is_none() {
            return false;
        }
        // Recheck managed descendants when used, including the active directory:
        // replacing it with a symlink after collection must not redirect reads.
        let Ok(relative) = path.strip_prefix(&extension.data_dir) else {
            return false;
        };
        let mut current = extension.data_dir.clone();
        for part in relative.components() {
            if !matches!(part, Component::Normal(_)) {
                return false;
            }
            current.push(part);
            if !std::fs::symlink_metadata(&current).is_ok_and(|m| !m.file_type().is_symlink()) {
                return false;
            }
        }
    }
    // Legacy roots retain their own resources; links cannot escape that root.
    let (Ok(root), Ok(resolved)) = (root.canonicalize(), path.canonicalize()) else {
        return false;
    };
    resolved.starts_with(root) && resolved.is_file()
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
    resource_root(extension)?;
    let name = cmd["icon"].as_str().or_else(|| cmd["iconPath"].as_str())?;
    if name.starts_with("https://") || name.starts_with("http://") {
        return Some(name.into());
    }
    let relative = icon_relative(name)?;
    icon_candidates(extension, &relative)
        .into_iter()
        .find(|p| contained_icon_file(extension, p))
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
    for value in &mut values {
        value["source"] = json!(super::catalog::source_for_id(value["id"].as_str().unwrap_or("")));
    }
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
        values.extend(saved.iter().filter(|(key, _)| key.as_str() != "__tezbar_identity_migration_v1").map(|(key, value)| (key.clone(), value.clone())));
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

fn supported_native_commands(extension: &Installed) -> impl Iterator<Item = &str> {
    let supported = extension.id.starts_with("com.tezbar.native.")
        && extension.manifest["healthError"].is_null() && compatible(&extension.manifest, false);
    extension.manifest["commands"].as_array().into_iter().flatten()
        .filter(move |cmd| supported && cmd["runtime"] == "rust" && compatible(cmd, true))
        .filter_map(|cmd| cmd["name"].as_str())
}

/// The second ID records a fresh edit to a legacy command that may become
/// covered later, even when the profile already contains an old native clear.
pub(crate) fn hotkey_edit_owner(ctx: &CoreContext, requested: &str) -> (String, Option<String>) {
    let Some((id, command)) = requested.strip_prefix("extcmd:").and_then(|s| s.split_once(':')) else {
        return (requested.into(), None);
    };
    if id.starts_with("com.tezbar.native.") { return (requested.into(), None); }
    let slug = super::v2_runtime::slug(id).unwrap_or_else(|| id.strip_prefix("raycast.")
        .or_else(|| id.strip_prefix("com.tezbar.")).unwrap_or(id));
    let canonical = super::v2_runtime::canonical_id(slug);
    let target = format!("extcmd:{canonical}:{command}");
    if !super::command_hotkey_aliases(&target).iter().any(|alias| alias == requested) {
        return (requested.into(), None);
    }
    if installed(ctx, &canonical).is_ok_and(|e| supported_native_commands(&e).any(|name| name == command)) {
        (target, None)
    } else {
        (requested.into(), Some(target))
    }
}

// False means a live shortcut operation failed and the transaction was rolled
// False means a live shortcut operation failed or retained history was unknown/corrupt
// and the transaction was rolled back. Discovery can return its rows, but must retry before caching them.
fn migrate_command_settings(ctx: &CoreContext, extension: &Installed) -> Result<bool, String> {
    let platform = crate::platform::PlatformCore::new(ctx.clone());
    let targets: Vec<_> = supported_native_commands(extension)
        .map(|name| format!("extcmd:{}:{name}", extension.id)).collect();
    let marker = format!("identityMigration:{}", extension.id);
    let granular = format!("identityMigrationHotkeysV2:{}", extension.id);
    let mut live_error = None;
    let mut live = |result: Result<(), String>| result.map_err(|error| {
        live_error = Some(error.clone());
        error
    });
    let mut defer_error = None;
    let result = platform.command_transaction(|| {
        let current = ctx.config.read()?;
        for key in ["disabledCommands", "commandAliases", "commandHotkeys"] {
            if current.get(key).is_some_and(|v| !v.is_object()) {
                return Err(format!("Identity migration: invalid {key}; configuration retained"));
            }
        }
        if current[&granular] == true && targets.iter().all(|id| {
            current[format!("identityMigrationHotkey:{id}")] == true &&
                !super::command_hotkey_aliases(id).iter().any(|alias| current["commandHotkeys"].get(alias).is_some())
        }) { return Ok(()); }
        // Old profiles only recorded a package-wide marker. Honor existing clears
        // once, then track the covered commands so later additions import normally.
        let legacy_clear = current[&marker] == true && current[&granular] != true;
        // V1 copied only these two qualified aliases. Other aliases can be
        // fresh bindings even if the command itself was previously covered.
        let old_alias_present = |config: &Value, id: &str| {
            let name = id.rsplit(':').next().unwrap_or("");
            ["raycast.", "com.tezbar."].iter().any(|prefix|
                config["commandHotkeys"].get(format!("extcmd:{prefix}{}:{name}", extension.slug)).is_some())
        };
        let old_coverage = if legacy_clear && targets.iter().any(|id| old_alias_present(&current, id)) {
            let dir = extension.package_dir.clone().or_else(|| {
                super::process_install::resolve(ctx, &extension.id).ok().map(|p| p.dir)
            });
            let Some(dir) = dir else {
                let err = format!("Identity migration: package directory unavailable for {}", extension.id);
                defer_error = Some(err.clone());
                return Err(err);
            };
            match super::process_install::retained_command_coverage(ctx, &extension.id, &dir, &extension.manifest) {
                Ok(coverage) => coverage,
                Err(err) => {
                    let err = format!("Identity migration: unreadable retained history for {}: {err}", extension.id);
                    defer_error = Some(err.clone());
                    return Err(err);
                }
            }
        } else { HashSet::new() };
        ctx.config.update(|config| {
            if legacy_clear {
                // Older versions copied every manifest command, including commands
                // for other platforms. Undo identifiable duplicate copies only.
                for cmd in extension.manifest["commands"].as_array().into_iter().flatten() {
                    let Some(name) = cmd["name"].as_str() else { continue; };
                    let id = format!("extcmd:{}:{name}", extension.id);
                    if targets.contains(&id) { continue; }
                    let Some(key) = config["commandHotkeys"][&id].as_str().map(str::to_owned) else { continue; };
                    let aliases = super::command_hotkey_aliases(&id);
                    let alias = aliases.iter().find(|alias| config["commandHotkeys"][*alias].as_str() == Some(key.as_str()))
                        .or_else(|| {
                            // An edited/cleared live alias takes precedence over a rollback copy.
                            let done = format!("identityMigrationHotkey:{id}");
                            if !config[&done].is_null() || aliases.iter().any(|a| config["commandHotkeys"].get(a).is_some()) {
                                return None;
                            }
                            let backup = &config[format!("identityMigrationHotkeyBackup:{}", extension.id)];
                            aliases.iter().find(|alias| backup[*alias].as_str() == Some(key.as_str()))
                        });
                    if let Some(alias) = alias {
                        live(platform.restore_legacy_owner(&id, alias, &key))?;
                        config["commandHotkeys"][alias] = json!(key);
                        config["commandHotkeys"].as_object_mut().unwrap().remove(&id);
                    }
                }
            }
            for id in &targets {
                let done = format!("identityMigrationHotkey:{id}");
                let aliases = super::command_hotkey_aliases(id);
                let pending_edit = config[&done].as_str()
                    .and_then(|owner| aliases.iter().find(|alias| alias.as_str() == owner));
                if let Some(owner) = pending_edit {
                    let bindings = config["commandHotkeys"].as_object_mut().ok_or("Invalid commandHotkeys")?;
                    match bindings.get(owner).cloned() {
                        Some(value) => { bindings.insert(id.clone(), value); }
                        None => { bindings.remove(id); }
                    }
                }
                for key in ["disabledCommands", "commandAliases", "commandHotkeys"] {
                    if config.get(key).is_some_and(|v| !v.is_object()) {
                        return Err(format!("Identity migration: invalid {key}; configuration retained"));
                    }
                    let already_imported = if key == "commandHotkeys" {
                        config[&done] == true || pending_edit.is_some() || (legacy_clear
                            && old_alias_present(config, id)
                            && old_coverage.contains(id.rsplit(':').next().unwrap_or("")))
                    } else { config[&marker] == true };
                    if config[key].get(id).is_some() || already_imported { continue; }
                    if let Some(value) = aliases.iter().find_map(|alias| config[key].get(alias)).cloned() {
                        config[key][id] = value;
                    }
                }
                if pending_edit.is_some() || aliases.iter().any(|alias| config["commandHotkeys"].get(alias).is_some()) {
                    super::retire_legacy_command_hotkeys(config, id)?;
                    live(platform.reconcile_command(id, config["commandHotkeys"][id].as_str().unwrap_or("")))?;
                }
                config[done] = json!(true);
            }
            config[&marker] = json!(true);
            config[&granular] = json!(true);
            Ok(())
        })?;
        Ok(())
    });
    match result {
        Ok(()) => Ok(true),
        Err(error) if live_error.as_ref() == Some(&error) || defer_error.as_ref() == Some(&error) => {
            log::warn!("Deferring shortcut migration for {}: {error}", extension.id);
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

fn validate_preference_snapshot(value: &Value) -> Result<(), String> {
    if !value.is_object() || value.get("commands").is_some_and(|v| !v.is_object() || v.as_object().unwrap().values().any(|c| !c.is_object())) {
        return Err("Legacy preference migration: invalid object/scopes; sources retained".into());
    }
    Ok(())
}
fn merge_legacy_preferences(target: &mut Value, source: &Value, canonical: &Value) -> Result<(), String> {
    for (key, value) in source.as_object().unwrap() {
        if key == "commands" {
            for (command, prefs) in value.as_object().unwrap() {
                for (name, value) in prefs.as_object().unwrap() {
                    if canonical["commands"][command].get(name).is_none()
                        && target["commands"][command].get(name).is_some_and(|old| old != value) {
                        return Err("Preference migration: conflicting legacy command settings; sources retained".into());
                    }
                }
            }
        } else if canonical.get(key).is_none() && target.get(key).is_some_and(|old| old != value) {
            return Err("Preference migration: conflicting legacy settings; sources retained".into());
        }
    }
    merge_preferences(target, source);
    Ok(())
}

fn merge_preferences(target: &mut Value, source: &Value) {
    for (key, value) in source.as_object().unwrap() {
        if key == "commands" {
            if target.get(key).is_none() { target[key] = json!({}); }
            for (command, prefs) in value.as_object().unwrap() {
                if target[key].get(command).is_none() { target[key][command] = json!({}); }
                target[key][command].as_object_mut().unwrap().extend(prefs.as_object().unwrap().clone());
            }
        } else { target[key] = value.clone(); }
    }
}

// Select the discoverable package before checking for saved values. A stale
// duplicate cannot supply preferences just because the active owner has none.
fn legacy_preferences_owner(base: &Path, name: &str) -> Result<Option<PathBuf>, String> {
    for root in profile_roots(base) {
        for path in root_discovery_paths(&root, true)? {
            let Some((_, slug, native)) = package_identity(&path) else { continue };
            if native || !slug.eq_ignore_ascii_case(name) { continue; }
            if preference_file(base, &path.join("package.json"))?
                && legacy_manifest(&path).map_err(|e| e.to_string())?.is_some() {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
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
    let batch2 = matches!(extension.slug.as_str(), "json-editor" | "emoji" | "random-data-generator" | "dictionary" | "one-time-password" | "language-tool" | "image-search" | "image-host");
    let retained = retained_preferences_path(extension);
    let retained_snapshot = if batch2 && preference_file(&extension.data_dir, &retained)? {
        let value = read_json(&retained).map_err(|_| "Canonical preference migration: invalid JSON; snapshot retained".to_string())?;
        validate_preference_snapshot(&value)?;
        if value["__tezbar_identity_migration_v1"] == true { return Ok(retained); }
        Some(value)
    } else { None };
    // A retained canonical snapshot is the migration commit. Do not re-import
    // deleted settings on restart; preserve legacy files for JS rollback.
    if !batch2 && preference_file(base, &own)? {
        return Ok(own);
    }
    if !batch2 && preference_file(&extension.data_dir, &retained)? {
        return Ok(retained);
    }
    // Older ports retain their existing retention contract; Batch2 opts into
    // snapshot migration without changing the unrelated installed-port policy.
    if !batch2 {
        if profile_roots(&extension.data_dir).iter().any(|root| extension.path.parent() == Some(root)) {
            if let Some(owner) = legacy_preferences_owner(&extension.data_dir, &extension.slug)? {
                let source = owner.join("preferences.json");
                if preference_file(&extension.data_dir, &source)? { return Ok(source); }
            }
        }
        return Ok(own);
    }
    let mut canonical = retained_snapshot.clone().unwrap_or(json!({}));
    let has_own = preference_file(base, &own)?;
    if has_own {
        let value = read_json(&own).map_err(|_| "Canonical preference migration: invalid JSON".to_string())?;
        validate_preference_snapshot(&value)?;
        merge_preferences(&mut canonical, &value);
    }
    let mut merged = json!({});
    let mut found = false;
    for root in ["extensions", "extensions/packages", "extension-registry/packages"] {
        for identity in [extension.slug.clone(), format!("com.tezbar.{}", extension.slug), format!("raycast.{}", extension.slug)] {
            let source = extension.data_dir.join(root).join(identity).join("preferences.json");
            if preference_file(&extension.data_dir, &source)? {
                let value = read_json(&source).map_err(|_| "Legacy preference migration: invalid JSON".to_string())?;
                validate_preference_snapshot(&value)?;
                merge_legacy_preferences(&mut merged, &value, &canonical)?;
                found = true;
            }
        }
    }
    if has_own || retained_snapshot.is_some() {
        merge_preferences(&mut merged, &canonical);
        found = true;
    }
    if found {
        merged["__tezbar_identity_migration_v1"] = json!(true);
        write_json_atomic(&retained, &merged)?;
        return Ok(retained);
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
        package_dir: None,
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
    let extensions = collect(ctx);
    let mut hotkey_owners = std::collections::HashMap::new();
    for extension in &extensions {
        for command in supported_native_commands(extension) {
            let target = format!("extcmd:{}:{command}", extension.id);
            for alias in super::command_hotkey_aliases(&target) {
                hotkey_owners.insert(alias, target.clone());
            }
        }
    }
    let mut result:Vec<_>=extensions.iter().filter(|e|compatible(&e.manifest,false)).map(|e| {
        let pkg=&e.manifest;
        let commands:Vec<_>=pkg["commands"].as_array().into_iter().flatten().filter(|v|v["name"].is_string()&&compatible(v,true) && (e.id.starts_with("raycast.") || v["runtime"] == "rust")).map(|cmd| {
            let key = format!("extcmd:{}:{}", e.id, cmd["name"].as_str().unwrap());
            json!({"hotkeyId":hotkey_owners.get(&key).unwrap_or(&key),"name":cmd["name"],"title":fallback(cmd,"title",cmd["name"].as_str().unwrap_or("")),"description":fallback(cmd,"description",""),
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

pub(super) fn command_rows(ctx: &CoreContext) -> Result<(Value, bool), String> {
    let mut extensions = collect(ctx);
    let mut complete = true;
    for extension in &extensions {
        if extension.id.starts_with("com.tezbar.native.") && extension.manifest["healthError"].is_null() {
            complete &= migrate_command_settings(ctx, extension)?;
        }
    }
    // Prefer each native command once; keep uncovered legacy commands
    // and both installed packages available for settings/uninstallation.
    extensions.sort_by_key(|entry| !entry.id.starts_with("com.tezbar.native."));
    let mut seen = HashSet::new();
    Ok((json!(extensions
        .iter()
        .flat_map(command_values)
        .filter(|command| { seen.insert(command["id"].as_str().unwrap_or("").to_owned()) })
        .collect::<Vec<_>>()), complete))
}

pub(super) fn handle(ctx: &CoreContext, channel: &str, args: &[Value]) -> CoreResult {
    let body = args.first().cloned().unwrap_or(Value::Null);
    match channel {
        "extension:list" => Ok(list(ctx)),
        "core:extensions:commands" => command_rows(ctx).map(|(rows, _)| rows),
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
    use std::sync::Arc;
    fn context(path: &Path) -> CoreContext {
        CoreContext::headless(crate::common::CorePaths {
            data_dir: path.into(),
            config_dir: path.join("config"),
            resource_dir: path.join("resources"),
            temp_dir: path.join("tmp"),
            home_dir: path.into(),
        })
    }
    #[test]
    fn batch2_disabled_aliases_import_once_and_allow_reenable() {
        for alias in ["raycast.emoji", "com.tezbar.emoji"] {
            let temp = tempfile::tempdir().unwrap();
            let ctx = context(temp.path());
            let extension = Installed { id:"com.tezbar.native.emoji".into(), slug:"emoji".into(), path:temp.path().join("extensions/packages/com.tezbar.native.emoji"), package_dir:None, manifest:json!({"id":"com.tezbar.native.emoji","platforms":[platform()],"commands":[{"name":"search","runtime":"rust"}]}), data_dir:temp.path().into() };
            let old = format!("extcmd:{alias}:search");
            ctx.config.update(|v| { v["disabledCommands"] = json!({old.clone():true}); Ok(()) }).unwrap();
            migrate_command_settings(&ctx, &extension).unwrap();
            let target = "extcmd:com.tezbar.native.emoji:search";
            assert_eq!(ctx.config.read().unwrap()["disabledCommands"][target], true);
            assert!(!super::super::launcher::enabled(&ctx, &extension.manifest, "search"));
            ctx.config.update(|v| { v["disabledCommands"].as_object_mut().unwrap().remove(target); Ok(()) }).unwrap();
            migrate_command_settings(&ctx, &extension).unwrap();
            assert!(super::super::launcher::enabled(&ctx, &extension.manifest, "search"));
            assert_eq!(ctx.config.read().unwrap()["disabledCommands"][&old], true);
        }
    }
    #[test]
    fn migrated_hotkey_tombstone_does_not_resurrect_a_cleared_binding() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        super::super::process_install::test_install(&ctx, "normal");
        let extension = installed(&ctx, "com.tezbar.native.perplexity").unwrap();
        let old = "extcmd:raycast.perplexity:ask-perplexity";
        let target = "extcmd:com.tezbar.native.perplexity:ask-perplexity";
        ctx.config.patch(&json!({"identityMigration:com.tezbar.native.perplexity":true,"commandHotkeys":{old:"Alt+E"},"future":{"keep":true}})).unwrap();
        migrate_command_settings(&ctx, &extension).unwrap();
        let config = ctx.config.read().unwrap();
        assert!(config["commandHotkeys"].get(old).is_none());
        assert!(config["commandHotkeys"].get(target).is_none());
        assert_eq!(config["identityMigrationHotkeyBackup:com.tezbar.native.perplexity"][old], "Alt+E");
        assert_eq!(config["future"]["keep"], true);
        let modified = std::fs::metadata(ctx.config.path()).unwrap().modified().unwrap();
        migrate_command_settings(&ctx, &extension).unwrap();
        assert_eq!(std::fs::metadata(ctx.config.path()).unwrap().modified().unwrap(), modified);
    }

    #[tokio::test]
    async fn migrated_hotkey_clear_and_rebind_update_live_owner_and_survive_restart() {
        for next in ["", "Alt+N"] {
            let temp = tempfile::tempdir().unwrap();
            let paths = context(temp.path()).paths;
            let host = Arc::new(crate::host::TestHost { allow_interaction:true, desktop:true, ..Default::default() });
            let ctx = CoreContext::new(paths.clone(), Some(host.clone()));
            let old = "extcmd:raycast.emoji:search";
            let target = "extcmd:com.tezbar.native.emoji:search";
            ctx.config.patch(&json!({"commandHotkeys":{old:"Alt+E"},"future":42})).unwrap();
            let platform = crate::platform::PlatformCore::new(ctx.clone());
            platform.initialize_shortcuts().unwrap();
            let old_binding = ctx.host.shortcuts().parse("Alt+E").unwrap();
            let extension = Installed { id:"com.tezbar.native.emoji".into(), slug:"emoji".into(), path:temp.path().join("native"), package_dir:None, manifest:json!({"platforms":[super::platform()],"commands":[{"name":"search","runtime":"rust"}]}), data_dir:temp.path().into() };
            migrate_command_settings(&ctx, &extension).unwrap();
            assert_eq!(ctx.config.read().unwrap()["commandHotkeys"][target], "Alt+E");
            let routed = platform.clone();
            ctx.set_router(Arc::new(move |channel, args| {
                let platform = routed.clone();
                Box::pin(async move { platform.handle(&channel, &args).await.unwrap_or_else(|| Err("Unexpected test route".into())) })
            })).unwrap();
            let launcher = crate::launcher::LauncherCore::new(ctx.clone());
            let result = launcher.handle("update-command-hotkey", &[json!(target), json!(next)]).await.unwrap().unwrap();
            assert_eq!(result["ok"], true);
            assert_eq!(result["previousAccelerator"], "Alt+E");
            assert_eq!(platform.command_for_shortcut(old_binding.id()), None);
            let config = ctx.config.read().unwrap();
            assert!(config["commandHotkeys"].get(old).is_none());
            assert_eq!(config["commandHotkeys"].get(target).and_then(Value::as_str), if next.is_empty() { None } else { Some(next) });
            assert_eq!(config["future"], 42);
            let fresh_host = Arc::new(crate::host::TestHost { allow_interaction:true, ..Default::default() });
            let restarted = CoreContext::new(paths, Some(fresh_host.clone()));
            let fresh_platform = crate::platform::PlatformCore::new(restarted.clone());
            fresh_platform.initialize_shortcuts().unwrap();
            assert_eq!(fresh_platform.command_for_shortcut(old_binding.id()), None);
            if !next.is_empty() {
                let binding = restarted.host.shortcuts().parse(next).unwrap();
                assert_eq!(fresh_platform.command_for_shortcut(binding.id()), Some(target.into()));
                let calls = host.calls.lock().unwrap();
                let register = calls.iter().position(|c| c == "register:Alt+N").unwrap();
                let unregister = calls.iter().position(|c| c == "unregister:Alt+E").unwrap();
                assert!(register < unregister);
            }
            assert!(!fresh_host.calls.lock().unwrap().iter().any(|c| c == "register:Alt+E"));
            launcher.shutdown().await.unwrap();
        }
    }

    #[tokio::test]
    async fn discovery_releases_tombstone_and_dispatches_edited_and_newly_covered_hotkeys_after_restart() {
        let temp = tempfile::tempdir().unwrap();
        let paths = context(temp.path()).paths;
        let host = Arc::new(crate::host::TestHost { allow_interaction:true, ..Default::default() });
        let ctx = CoreContext::new(paths.clone(), Some(host.clone()));
        let legacy = "extcmd:raycast.perplexity:ask-perplexity";
        let canonical = "extcmd:com.tezbar.native.perplexity:ask-perplexity";
        let second = "extcmd:raycast.perplexity:second";
        let second_target = "extcmd:com.tezbar.native.perplexity:second";
        let js_only = "extcmd:raycast.perplexity:other-platform";
        let js_package = temp.path().join("extensions/perplexity/package.json");
        write_json_atomic(&js_package, &json!({"name":"perplexity","platforms":[platform()],
            "commands":[{"name":"ask-perplexity"},{"name":"second"},{"name":"other-platform"}]})).unwrap();
        let foreign_command = |m: &mut Value| {
            let mut foreign = m["commands"][0].clone();
            foreign["name"] = json!("other-platform");
            foreign["platforms"] = json!([if platform() == "macOS" { "Windows" } else { "macOS" }]);
            m["commands"].as_array_mut().unwrap().push(foreign);
        };
        super::super::process_install::test_install_with(&ctx, "normal", foreign_command);
        ctx.config.patch(&json!({"identityMigration:com.tezbar.native.perplexity":true,
            "commandHotkeys":{legacy:"Alt+E",second:"Alt+S",js_only:"Alt+P","extcmd:com.tezbar.native.perplexity:other-platform":"Alt+P"}})).unwrap();
        let core = crate::NativeCore::new(ctx.clone()).unwrap();
        core.platform.initialize_shortcuts().unwrap();
        let owner = |key: &str| core.platform.command_for_shortcut(ctx.host.shortcuts().parse(key).unwrap().id());
        assert_eq!(owner("Alt+E").as_deref(), Some(legacy));
        handle(&ctx, "core:extensions:commands", &[]).unwrap();
        assert_eq!(owner("Alt+E"), None);
        assert!(host.calls.lock().unwrap().contains(&"unregister:Alt+E".into()));
        assert_eq!(core.launcher.handle("update-command-hotkey", &[json!("unrelated"),json!("Alt+E")])
            .await.unwrap().unwrap()["ok"], true);
        assert_eq!(owner("Alt+E").as_deref(), Some("unrelated"));
        assert_eq!(core.launcher.handle("update-command-hotkey", &[json!(legacy),json!("Alt+J")])
            .await.unwrap().unwrap()["ok"], true);
        assert_eq!(owner("Alt+J").as_deref(), Some(canonical));
        assert_eq!(owner("Alt+S").as_deref(), Some(second));
        assert_eq!(owner("Alt+P").as_deref(), Some(js_only));
        let schema = settings(&ctx);
        let js_commands = &schema.as_array().unwrap().iter()
            .find(|row| row["extensionId"] == "raycast.perplexity").unwrap()["commands"];
        assert_eq!(js_commands[0]["hotkeyId"], canonical);
        assert_eq!(js_commands[2]["hotkeyId"], js_only);
        assert_eq!(core.launcher.handle("update-command-hotkey", &[json!(js_only),json!("Alt+T")])
            .await.unwrap().unwrap()["ok"], true);
        super::super::process_install::test_install_with(&ctx, "normal", |m| {
            m["version"] = json!("1.1.0");
            let mut next = m["commands"][0].clone();
            next["name"] = json!("second");
            m["commands"].as_array_mut().unwrap().push(next);
            foreign_command(m);
        });
        for _ in 0..2 { handle(&ctx, "core:extensions:commands", &[]).unwrap(); }
        assert_eq!(owner("Alt+S").as_deref(), Some(second_target));
        assert_eq!(owner("Alt+P"), None);
        assert_eq!(owner("Alt+T").as_deref(), Some(js_only));
        let config = ctx.config.read().unwrap();
        assert!(config["commandHotkeys"].get(legacy).is_none());
        assert!(config["commandHotkeys"].get(second).is_none());
        assert!(config["commandHotkeys"].get("extcmd:com.tezbar.native.perplexity:other-platform").is_none());
        let fresh_host = Arc::new(crate::host::TestHost { allow_interaction:true, ..Default::default() });
        let fresh = CoreContext::new(paths, Some(fresh_host.clone()));
        let restarted = crate::NativeCore::new(fresh.clone()).unwrap();
        restarted.platform.initialize_shortcuts().unwrap();
        for (key, expected, extension, name) in [
            ("Alt+J",canonical,"com.tezbar.native.perplexity","ask-perplexity"),
            ("Alt+S",second_target,"com.tezbar.native.perplexity","second"),
            ("Alt+T",js_only,"raycast.perplexity","other-platform"),
        ] {
            let binding = fresh.host.shortcuts().parse(key).unwrap();
            let owner = restarted.platform.command_for_shortcut(binding.id()).unwrap();
            assert_eq!(owner, expected);
            restarted.execute_command_shortcut(&owner).await.unwrap();
            assert_eq!(fresh_host.events.lock().unwrap().last().unwrap(),
                &("run-extension-command-from-hotkey".into(),json!({"extensionId":extension,"commandName":name})));
        }
        restarted.execute_command_shortcut("ext-perplexity-ask-perplexity").await.unwrap();
        assert_eq!(fresh_host.events.lock().unwrap().last().unwrap().1,
            json!({"extensionId":"com.tezbar.native.perplexity","commandName":"ask-perplexity"}));
        assert!(restarted.execute_command_shortcut("extcmd:com.tezbar.native.perplexity:other-platform")
            .await.unwrap_err().contains("unavailable"));
    }

    #[tokio::test]
    async fn first_v2_discovery_after_package_update_preserves_new_and_backup_only_hotkeys() {
        let temp = tempfile::tempdir().unwrap();
        let paths = context(temp.path()).paths;
        let host = Arc::new(crate::host::TestHost { allow_interaction:true, ..Default::default() });
        let ctx = CoreContext::new(paths.clone(), Some(host));
        let cleared = "extcmd:raycast.perplexity:ask-perplexity";
        let second = "extcmd:raycast.perplexity:second";
        let foreign = "extcmd:raycast.perplexity:other-platform";
        let unavailable = "extcmd:com.tezbar.native.perplexity:other-platform";
        write_json_atomic(&temp.path().join("extensions/perplexity/package.json"), &json!({
            "platforms":[platform()], "commands":[{"name":"ask-perplexity"},{"name":"second"},{"name":"other-platform"}]
        })).unwrap();
        let add_foreign = |m: &mut Value| {
            let mut command = m["commands"][0].clone();
            command["name"] = json!("other-platform");
            command["platforms"] = json!([if platform() == "macOS" { "Windows" } else { "macOS" }]);
            m["commands"].as_array_mut().unwrap().push(command);
        };
        super::super::process_install::test_install_with(&ctx, "normal", add_foreign);
        ctx.config.patch(&json!({
            "identityMigration:com.tezbar.native.perplexity":true,
            "commandHotkeys":{cleared:"Alt+E", second:"Alt+S", unavailable:"Alt+P"},
            "identityMigrationHotkeyBackup:com.tezbar.native.perplexity":{foreign:"Alt+P"}
        })).unwrap();
        super::super::process_install::test_install_with(&ctx, "normal", |m| {
            m["version"] = json!("1.1.0");
            add_foreign(m);
            let mut command = m["commands"][0].clone();
            command["name"] = json!("second");
            m["commands"].as_array_mut().unwrap().push(command);
        });
        let core = crate::NativeCore::new(ctx.clone()).unwrap();
        core.platform.initialize_shortcuts().unwrap();
        for _ in 0..2 { handle(&ctx, "core:extensions:commands", &[]).unwrap(); }
        let config = ctx.config.read().unwrap();
        assert!(config["commandHotkeys"].get(unavailable).is_none());
        let backup = &config["identityMigrationHotkeyBackup:com.tezbar.native.perplexity"];
        assert_eq!(backup[cleared], "Alt+E");
        assert_eq!(backup[second], "Alt+S");
        assert_eq!(backup[foreign], "Alt+P");
        let host = Arc::new(crate::host::TestHost { allow_interaction:true, ..Default::default() });
        let fresh = CoreContext::new(paths, Some(host.clone()));
        let restarted = crate::NativeCore::new(fresh.clone()).unwrap();
        restarted.platform.initialize_shortcuts().unwrap();
        let owner = |key: &str| restarted.platform.command_for_shortcut(fresh.host.shortcuts().parse(key).unwrap().id());
        assert_eq!(owner("Alt+E"), None);
        for (key, id, extension, name) in [
            ("Alt+S","extcmd:com.tezbar.native.perplexity:second","com.tezbar.native.perplexity","second"),
            ("Alt+P",foreign,"raycast.perplexity","other-platform"),
        ] {
            let binding = owner(key).unwrap();
            assert_eq!(binding, id);
            restarted.execute_command_shortcut(&binding).await.unwrap();
            assert_eq!(host.events.lock().unwrap().last().unwrap(),
                &("run-extension-command-from-hotkey".into(),json!({"extensionId":extension,"commandName":name})));
        }
    }

    #[tokio::test]
    async fn registration_conflict_keeps_discovery_dispatch_and_retry_available() {
        let temp = tempfile::tempdir().unwrap();
        let host = Arc::new(crate::host::TestHost { allow_interaction:true, ..Default::default() });
        let ctx = CoreContext::new(context(temp.path()).paths, Some(host.clone()));
        super::super::process_install::test_install(&ctx, "normal");
        write_json_atomic(&temp.path().join("extensions/other/package.json"), &json!({
            "platforms":[platform()], "commands":[{"name":"run"}]
        })).unwrap();
        let legacy = "extcmd:raycast.perplexity:ask-perplexity";
        let target = "extcmd:com.tezbar.native.perplexity:ask-perplexity";
        ctx.config.patch(&json!({"commandHotkeys":{legacy:"Alt+S"}})).unwrap();
        let core = crate::NativeCore::new(ctx.clone()).unwrap();
        host.shortcut_results.lock().unwrap().extend([Ok(()), Err("occupied".into())]);
        core.platform.initialize_shortcuts().unwrap();
        let unrelated = "extcmd:raycast.other:run";
        assert_eq!(core.launcher.handle("update-command-hotkey", &[json!(unrelated),json!("Alt+U")])
            .await.unwrap().unwrap()["ok"], true);
        let before = ctx.config.read().unwrap();
        host.shortcut_results.lock().unwrap().extend([Err("occupied".into()), Err("occupied".into())]);
        let rows = ctx.request("core:extensions:commands", vec![]).await.unwrap();
        assert!(rows.as_array().unwrap().iter().any(|row| row["extensionId"] == "raycast.other"));
        let owner = |key: &str| core.platform.command_for_shortcut(ctx.host.shortcuts().parse(key).unwrap().id());
        core.execute_command_shortcut(&owner("Alt+U").unwrap()).await.unwrap();
        assert_eq!(host.events.lock().unwrap().last().unwrap().1,
            json!({"extensionId":"raycast.other","commandName":"run"}));
        assert_eq!(ctx.config.read().unwrap(), before);
        assert_eq!(owner("Alt+S"), None);
        assert!(host.shortcut_results.lock().unwrap().is_empty());
        ctx.request("core:extensions:commands", vec![]).await.unwrap();
        assert_eq!(owner("Alt+S").as_deref(), Some(target));
        assert_eq!(ctx.config.read().unwrap()["commandHotkeys"][target], "Alt+S");
    }

    #[tokio::test]
    async fn canonical_settings_hotkey_rejects_js_row_with_canonical_slug() {
        let temp = tempfile::tempdir().unwrap();
        let host = Arc::new(crate::host::TestHost { allow_interaction:true, ..Default::default() });
        let ctx = CoreContext::new(context(temp.path()).paths, Some(host.clone()));
        write_json_atomic(&temp.path().join("extensions/raycast.com.tezbar.native.perplexity/package.json"),
            &json!({"platforms":[platform()],"commands":[{"name":"ask-perplexity"}]})).unwrap();
        let core = crate::NativeCore::new(ctx.clone()).unwrap();
        let rows = ctx.request("core:extensions:commands", vec![]).await.unwrap();
        assert!(rows.as_array().unwrap().iter().any(|row| row["extName"] == "com.tezbar.native.perplexity"
            && row["extensionId"] == "raycast.com.tezbar.native.perplexity"));
        assert!(core.execute_command_shortcut("extcmd:com.tezbar.native.perplexity:ask-perplexity")
            .await.unwrap_err().contains("unavailable"));
        assert!(!host.events.lock().unwrap().iter().any(|(channel, _)| channel == "run-extension-command-from-hotkey"));
    }

    #[tokio::test]
    async fn corrupt_retained_history_defers_migration_preserves_tombstones_and_allows_unrelated_dispatch() {
        let temp = tempfile::tempdir().unwrap();
        let host = Arc::new(crate::host::TestHost { allow_interaction:true, ..Default::default() });
        let ctx = CoreContext::new(context(temp.path()).paths, Some(host.clone()));
        super::super::process_install::test_install(&ctx, "normal");
        write_json_atomic(&temp.path().join("extensions/other/package.json"), &json!({
            "platforms":[platform()], "commands":[{"name":"run"}]
        })).unwrap();
        let cleared = "extcmd:raycast.perplexity:ask-perplexity";
        let target = "extcmd:com.tezbar.native.perplexity:ask-perplexity";
        let unrelated = "extcmd:raycast.other:run";
        ctx.config.patch(&json!({
            "identityMigration:com.tezbar.native.perplexity": true,
            "commandHotkeys": {cleared: "Alt+E", unrelated: "Alt+U"}
        })).unwrap();
        let core = crate::NativeCore::new(ctx.clone()).unwrap();
        core.platform.initialize_shortcuts().unwrap();
        let versions = ctx.paths.data_dir.join("extensions/packages/com.tezbar.native.perplexity/versions");
        let corrupt_dir = versions.join("0.9.0");
        std::fs::create_dir_all(&corrupt_dir).unwrap();
        std::fs::write(corrupt_dir.join("extension.json"), b"{corrupt-json").unwrap();

        let before = ctx.config.read().unwrap();
        let rows = ctx.request("core:extensions:commands", vec![]).await.unwrap();
        assert!(rows.as_array().unwrap().iter().any(|row| row["extensionId"] == "raycast.other"));
        assert!(rows.as_array().unwrap().iter().any(|row| row["extensionId"] == "com.tezbar.native.perplexity"));

        let owner = |key: &str| core.platform.command_for_shortcut(ctx.host.shortcuts().parse(key).unwrap().id());
        assert_eq!(owner("Alt+U").as_deref(), Some(unrelated));
        core.execute_command_shortcut(&owner("Alt+U").unwrap()).await.unwrap();
        assert_eq!(host.events.lock().unwrap().last().unwrap().1,
            json!({"extensionId":"raycast.other","commandName":"run"}));

        let after = ctx.config.read().unwrap();
        assert_eq!(after, before);
        assert!(after["commandHotkeys"].get(target).is_none());
        assert_eq!(after["commandHotkeys"][cleared], "Alt+E");
        assert_eq!(after.get("identityMigrationHotkeysV2:com.tezbar.native.perplexity"), None);
        assert_eq!(owner("Alt+E"), None);

        std::fs::remove_dir_all(&corrupt_dir).unwrap();
        ctx.request("core:extensions:commands", vec![]).await.unwrap();
        let migrated = ctx.config.read().unwrap();
        assert!(migrated["commandHotkeys"].get(target).is_none());
        assert!(migrated["commandHotkeys"].get(cleared).is_none());
        assert_eq!(migrated["identityMigrationHotkeyBackup:com.tezbar.native.perplexity"][cleared], "Alt+E");
        assert_eq!(migrated["identityMigrationHotkeysV2:com.tezbar.native.perplexity"], true);
    }

    #[test]
    fn batch2_conflicts_abort_unless_canonical_field_exists() {
        let mut target = json!({"secret":"synthetic-a","commands":{"run":{"empty":true}}});
        let source = json!({"secret":"synthetic-b","commands":{"run":{"empty":false}}});
        assert!(merge_legacy_preferences(&mut target, &source, &json!({})).is_err());
        assert!(merge_legacy_preferences(&mut target, &source, &json!({"secret":null})).is_err());
        let canonical = json!({"secret":null,"commands":{"run":{"empty":false}}});
        merge_legacy_preferences(&mut target, &source, &canonical).unwrap();
        merge_preferences(&mut target, &canonical);
        assert_eq!(target["secret"], Value::Null);
        assert_eq!(target["commands"]["run"]["empty"], false);
    }
    #[test]
    fn batch2_identity_migration_aliases_restart_and_corruption() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let extension = Installed { id: "com.tezbar.native.emoji".into(), slug: "emoji".into(), path: temp.path().join("extensions/packages/com.tezbar.native.emoji"), package_dir: None, manifest: json!({"id":"com.tezbar.native.emoji","platforms":[platform()],"commands":[{"name":"search","runtime":"rust"}]}), data_dir: temp.path().into() };
        let a = temp.path().join("extensions/packages/raycast.emoji/preferences.json");
        let b = temp.path().join("extensions/packages/com.tezbar.emoji/preferences.json");
        write_json_atomic(&a, &json!({"theme":"old","commands":{"search":{"a":1}}})).unwrap();
        write_json_atomic(&b, &json!({"locale":"kk","commands":{"search":{"b":2}}})).unwrap();
        let before = std::fs::read(&a).unwrap();
        write_json_atomic(&retained_preferences_path(&extension), &json!({"retainedOnly":false,"theme":"previous-native"})).unwrap();
        write_json_atomic(&extension.path.join("preferences.json"), &json!({"theme":"native","commands":{"search":{"a":9}}})).unwrap();
        let path = preferences_path(&extension).unwrap();
        let saved = read_json(&path).unwrap();
        assert_eq!(saved["locale"], "kk");
        assert_eq!(saved["retainedOnly"], false);
        assert_eq!(saved["__tezbar_identity_migration_v1"], true);
        assert!(preferences(&extension, None).unwrap().get("__tezbar_identity_migration_v1").is_none());
        assert_eq!(preferences(&extension, Some("search")).unwrap()["a"], 9);
        assert_eq!(preferences(&extension, Some("search")).unwrap()["b"], 2);
        assert_eq!(saved["theme"], "native");
        assert_eq!(saved["commands"]["search"], json!({"a":9,"b":2}));
        std::fs::write(&a, "corrupt after commit").unwrap();
        assert_eq!(preferences_path(&extension).unwrap(), path);
        assert_eq!(read_json(&path).unwrap(), saved);
        std::fs::remove_file(&path).unwrap();
        assert!(preferences_path(&extension).unwrap_err().contains("invalid JSON"));
        assert!(!path.exists());
        std::fs::write(&a, before).unwrap();
        ctx.config.update(|v| { *v = json!({"disabledCommands":{"extcmd:raycast.emoji:search":true,"extcmd:com.tezbar.native.emoji:search":false},"commandAliases":{"extcmd:com.tezbar.emoji:search":"find"}}); Ok(()) }).unwrap();
        migrate_command_settings(&ctx, &extension).unwrap();
        let config = ctx.config.read().unwrap();
        assert_eq!(config["disabledCommands"]["extcmd:com.tezbar.native.emoji:search"], false);
        assert_eq!(config["commandAliases"]["extcmd:com.tezbar.native.emoji:search"], "find");
        assert_eq!(config["disabledCommands"]["extcmd:raycast.emoji:search"], true);
        migrate_command_settings(&ctx, &extension).unwrap();
        assert_eq!(ctx.config.read().unwrap(), config);
        assert!(super::super::launcher::enabled(&ctx, &extension.manifest, "search"));
        let restarted = context(temp.path());
        assert_eq!(restarted.config.read().unwrap(), config);
        assert!(super::super::launcher::enabled(&restarted, &extension.manifest, "search"));
        restarted.config.update(|v| { v["disabledCommands"] = json!([]); Ok(()) }).unwrap();
        assert!(migrate_command_settings(&restarted, &extension).unwrap_err().contains("invalid disabledCommands"));
    }
    #[test]
    fn targeted_lookup_bounds_verification_and_rejects_target_tampering() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        super::super::process_install::test_install(&ctx, "normal");
        for i in 0..32 {
            let path = temp.path().join(format!("extensions/packages/com.tezbar.native.unrelated-{i}"));
            std::fs::create_dir_all(path).unwrap();
        }
        let id = "com.tezbar.native.perplexity";
        super::super::process_install::RESOLVED_IDS.with(|ids| ids.borrow_mut().clear());
        assert!(installed_command(&ctx, id, "ask-perplexity").is_some());
        super::super::process_install::RESOLVED_IDS.with(|ids| assert_eq!(&*ids.borrow(), &[id]));
        let package = super::super::process_install::resolve(&ctx, id).unwrap();
        std::fs::write(package.executable, b"tampered").unwrap();
        assert!(installed_command(&ctx, id, "ask-perplexity").is_none());
        assert!(installed_command(&ctx, "raycast.perplexity", "ask-perplexity").is_none());
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
    fn native_icon_prefers_active_version_inventory() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("com.tezbar.native.fixture");
        let package_dir = path.join("versions/1");
        let relative = "resources/icon.png";
        for root in [&path, &package_dir] {
            let icon = root.join(relative);
            std::fs::create_dir_all(icon.parent().unwrap()).unwrap();
            std::fs::write(icon, b"icon fixture").unwrap();
        }
        let mut extension = Installed {
            id: "com.tezbar.native.fixture".into(),
            slug: "fixture".into(),
            path: path.clone(),
            package_dir: Some(package_dir.clone()),
            manifest: json!({"icon":relative,"files":{relative:"verified digest"}}),
            data_dir: temp.path().into(),
        };
        let command = json!({"icon":relative});
        assert_eq!(
            icon_path(&extension).as_deref(),
            package_dir.join(relative).to_str()
        );
        assert_eq!(
            command_icon_path(&extension, &command).as_deref(),
            package_dir.join(relative).to_str()
        );
        extension.manifest["files"] = json!({});
        assert!(icon_path(&extension).is_none());
        assert!(command_icon_path(&extension, &command).is_none());
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
            .join("extension-registry/packages/perplexity/preferences.json");
        write_json_atomic(&legacy.with_file_name("package.json"), &json!({"name":"perplexity"})).unwrap();
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
