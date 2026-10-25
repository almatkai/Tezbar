//! Optional protocol-1 launcher contributions. No extension domain knowledge.
use super::{
    process_install::{self, Package},
    registry,
};
use crate::native_core::common::{read_json, write_json_atomic, CoreContext, CoreResult};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::PathBuf};
pub(super) const FEATURE: &str = "launcher.v1";
const MAX_INDEX: usize = 256 * 1024;

pub(super) fn descriptors(m: &Value) -> impl Iterator<Item = &Value> {
    m["launcherContributions"].as_array().into_iter().flatten()
}
fn bounded_text<'a>(v: &'a Value, name: &str, max: usize) -> Result<&'a str, String> {
    v[name]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control))
        .ok_or_else(|| format!("Invalid launcher {name}"))
}
pub(super) fn validate(m: &Value) -> Result<(), String> {
    let ds = match m.get("launcherContributions") {
        Some(ds) => ds
            .as_array()
            .filter(|a| a.len() <= 8)
            .ok_or("Too many launcher contributions")?
            .as_slice(),
        None => &[],
    };
    let mut ids = HashSet::new();
    for d in ds {
        let id = bounded_text(d, "id", 128)?;
        process_install::component(id)?;
        if !ids.insert(id) {
            return Err("Duplicate launcher contribution".into());
        }
        let command = bounded_text(d, "commandName", 128)?;
        let cmd = m["commands"]
            .as_array()
            .and_then(|a| a.iter().find(|c| c["name"] == command))
            .ok_or("Launcher must target its own command")?;
        let surfaces = d["surfaces"]
            .as_array()
            .filter(|a| !a.is_empty() && a.len() <= 2)
            .ok_or("Invalid launcher surfaces")?;
        if surfaces.iter().any(|s| s != "query" && s != "argument") {
            return Err("Invalid launcher surface".into());
        }
        if surfaces.iter().any(|s| s == "argument") {
            let name = bounded_text(d, "argumentName", 128)?;
            if !cmd["arguments"]
                .as_array()
                .is_some_and(|a| a.iter().any(|a| a["name"] == name))
            {
                return Err("Launcher argument must belong to its command".into());
            }
        }
        if d["limit"].as_u64().is_none_or(|n| !(1..=32).contains(&n))
            || d["timeoutMs"]
                .as_u64()
                .is_none_or(|n| !(50..=3000).contains(&n))
        {
            return Err("Invalid launcher bounds".into());
        }
        let selectors = d["selectors"]
            .as_object()
            .ok_or("Missing launcher selectors")?;
        if selectors
            .keys()
            .any(|k| !["terms", "digitRun"].contains(&k.as_str()))
        {
            return Err("Unknown launcher selector".into());
        }
        if let Some(terms) = selectors.get("terms") {
            let terms = terms
                .as_array()
                .filter(|a| a.len() <= 32)
                .ok_or("Invalid launcher terms")?;
            if terms.iter().any(|t| {
                t.as_str().is_none_or(|s| {
                    s.is_empty() || s.len() > 128 || s.chars().any(char::is_control)
                })
            }) {
                return Err("Invalid launcher term".into());
            }
        }
        if let Some(n) = selectors.get("digitRun") {
            let min = n["min"].as_u64().ok_or("Invalid digit selector")?;
            let max = n["max"].as_u64().ok_or("Invalid digit selector")?;
            if !(1..=16).contains(&min) || !(min..=16).contains(&max) {
                return Err("Invalid digit selector".into());
            }
        }
        if d["index"] == true
            && !m["capabilities"].as_array().is_some_and(|a| {
                a.iter().any(|v| {
                    v == "launcher" || v == "launcher.publish" || v == "host.launcher.publish"
                })
            })
        {
            return Err("Launcher index requires publish capability".into());
        }
        if d.get("index").is_some_and(|v| !v.is_boolean()) {
            return Err("Invalid launcher index policy".into());
        }
        if let Some(paths) = d.get("bootstrapMigrationPaths") {
            if d["index"] != true
                || paths.as_array().is_none_or(|a| {
                    a.is_empty()
                        || a.len() > 8
                        || a.iter().any(|p| {
                            p.as_str()
                                .is_none_or(|k| m["migrationPaths"].get(k).is_none())
                        })
                })
            {
                return Err("Invalid launcher bootstrap grants".into());
            }
        }
    }
    for cmd in m["commands"].as_array().into_iter().flatten() {
        if let Some(l) = cmd.get("launcher") {
            if let Some(id) = l.get("id") {
                let command = cmd["name"].as_str().unwrap_or("");
                let valid = std::iter::once(&m["id"])
                    .chain(m["replaces"].as_array().into_iter().flatten())
                    .filter_map(Value::as_str)
                    .any(|ext| id == &format!("extcmd:{ext}:{command}"));
                if !valid {
                    return Err("Invalid launcher command ID".into());
                }
            }
            if l["recommendation"].as_u64().is_none_or(|n| n > 2000) {
                return Err("Invalid launcher recommendation".into());
            }
            if let Some(a) = l.get("inlineArgument") {
                let name = bounded_text(a, "name", 128)?;
                if !cmd["arguments"]
                    .as_array()
                    .is_some_and(|v| v.iter().any(|v| v["name"] == name))
                {
                    return Err("Invalid inline argument association".into());
                }
                bounded_text(a, "title", 128)?;
                bounded_text(a, "prompt", 256)?;
                if let Some(v) = a.get("validation") {
                    if v["kind"] != "integer"
                        || v["maxLength"]
                            .as_u64()
                            .is_none_or(|n| !(1..=16).contains(&n))
                        || v["min"]
                            .as_u64()
                            .zip(v["max"].as_u64())
                            .is_none_or(|(min, max)| min > max)
                    {
                        return Err("Invalid inline validation".into());
                    }
                    bounded_text(v, "message", 256)?;
                }
            }
        }
    }
    Ok(())
}
pub(super) fn selected(d: &Value, text: &str) -> bool {
    let q = text.trim().to_lowercase();
    if q.is_empty() {
        return false;
    }
    if d["selectors"]["terms"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .any(|s| q.contains(&s.to_lowercase()))
    {
        return true;
    }
    let n = &d["selectors"]["digitRun"];
    let min = n["min"].as_u64().unwrap_or(17) as usize;
    let max = n["max"].as_u64().unwrap_or(0) as usize;
    q.split(|c: char| !c.is_ascii_digit())
        .any(|s| s.len() >= min && s.len() <= max)
}
pub(super) fn fingerprint(m: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(m).unwrap_or_default())
    )
}
fn path(ctx: &CoreContext, id: &str) -> Result<PathBuf, String> {
    process_install::component(id)?;
    let mut parent = ctx.paths.data_dir.clone();
    for part in ["extensions", "launcher-index"] {
        parent.push(part);
        if let Ok(metadata) = std::fs::symlink_metadata(&parent) {
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err("Unsafe launcher index directory".into());
            }
        }
    }
    Ok(parent.join(format!("{id}.json")))
}
fn read_index(ctx: &CoreContext, m: &Value) -> CoreResult {
    let p = path(ctx, m["id"].as_str().ok_or("Missing identity")?)?;
    if !p.exists() {
        return Ok(json!({}));
    }
    let metadata = std::fs::symlink_metadata(&p).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_INDEX as u64
    {
        return Err("Invalid launcher index file".into());
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(&p)
        .map_err(|e| e.to_string())?
        .take(MAX_INDEX as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_INDEX {
        return Err("Launcher index too large".into());
    }
    let db: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if db["fingerprint"] != fingerprint(m) {
        return Ok(json!({}));
    }
    Ok(db)
}
pub(super) fn purge(ctx: &CoreContext, id: &str) -> Result<(), String> {
    // Caller holds PACKAGE_LOCK through uninstall.
    let p = path(ctx, id)?;
    if p.exists() {
        std::fs::remove_file(p).map_err(|e| e.to_string())?;
    }
    Ok(())
}
/// Read only verified installed contributors; this runs on metadata refresh, never per keystroke.
pub(super) fn packages(ctx: &CoreContext) -> Vec<Package> {
    registry::launcher_packages(ctx)
}
pub(super) fn current(ctx: &CoreContext, p: &Package) -> bool {
    let id = p.manifest["id"].as_str().unwrap_or("");
    process_install::root(ctx, id)
        .ok()
        .and_then(|r| read_json(&r.join("active.json")).ok())
        .is_some_and(|a| {
            a["directory"].as_str() == p.dir.file_name().and_then(|s| s.to_str())
                && std::fs::read(p.dir.join("extension.json"))
                    .ok()
                    .is_some_and(|bytes| {
                        a["manifestSha256"] == format!("{:x}", Sha256::digest(&bytes))
                            && serde_json::from_slice::<Value>(&bytes).ok().as_ref()
                                == Some(&p.manifest)
                    })
        })
}
pub(super) fn enabled(ctx: &CoreContext, m: &Value, command: &str) -> bool {
    let Ok(config) = ctx.config.read() else {
        return false;
    };
    let mut ids = vec![m["id"].as_str().unwrap_or("")];
    ids.extend(
        m["replaces"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str),
    );
    !ids.iter()
        .any(|id| config["disabledCommands"][format!("extcmd:{id}:{command}")] == true)
}
pub(super) fn rows(
    m: &Value,
    d: &Value,
    values: &Value,
    indexed: bool,
) -> Result<Vec<Value>, String> {
    let values = values
        .as_array()
        .filter(|a| a.len() <= if indexed { 256 } else { 32 })
        .ok_or("Launcher rows exceed limit")?;
    let mut ids = HashSet::new();
    let mut result = Vec::new();
    for r in values {
        let id = bounded_text(r, "id", 256)?;
        if !ids.insert(id) {
            return Err("Duplicate launcher row".into());
        }
        let title = bounded_text(r, "title", 512)?;
        let subtitle = r["subtitle"]
            .as_str()
            .filter(|s| s.len() <= 1024 && !s.chars().any(char::is_control))
            .ok_or("Invalid launcher subtitle")?;
        let score = r["score"]
            .as_i64()
            .filter(|n| (0..=20000).contains(n))
            .ok_or("Invalid launcher score")?;
        let action = &r["action"];
        if action["type"] != "run-extension-command" || action["commandName"] != d["commandName"] {
            return Err("Launcher row must target its declared own command".into());
        }
        let command = m["commands"]
            .as_array()
            .and_then(|a| a.iter().find(|c| c["name"] == d["commandName"]))
            .ok_or("Missing launcher command")?;
        let arguments = action["argumentValues"]
            .as_object()
            .ok_or("Invalid launcher arguments")?;
        if arguments.len() > 16
            || arguments.iter().any(|(name, v)| {
                v.as_str()
                    .is_none_or(|v| v.len() > 1024 || v.chars().any(char::is_control))
                    || !command["arguments"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|a| a["name"] == *name))
            })
        {
            return Err("Invalid launcher argument value".into());
        }
        let mut row = json!({"id":id,"title":title,"subtitle":subtitle,"category":"extensions","ephemeral":true,"score":score,
            "action":{"type":"run-extension-command","extensionId":m["id"],"commandName":d["commandName"],"title":command["title"],"argumentValues":arguments,"commandArgumentDefinitions":command["arguments"],"launcher":command["launcher"]}});
        if indexed {
            // Closed shape: labels, search tokens and declared string arguments only.
            if r.as_object().is_none_or(|o| {
                o.keys().any(|k| {
                    !["id", "title", "subtitle", "score", "action", "tokens"].contains(&k.as_str())
                })
            }) {
                return Err("Unknown persisted launcher row field".into());
            }
            let tokens = r["tokens"]
                .as_array()
                .filter(|a| !a.is_empty() && a.len() <= 16)
                .ok_or("Invalid index tokens")?;
            if tokens.iter().any(|t| {
                t.as_str().is_none_or(|s| {
                    s.is_empty() || s.len() > 256 || s.chars().any(char::is_control)
                })
            }) {
                return Err("Invalid index token".into());
            }
            row["tokens"] = json!(tokens);
        }
        result.push(row);
    }
    Ok(result)
}
pub(super) fn publish(p: &super::process::Process, params: &Value) -> CoreResult {
    if !p.launcher_enabled() {
        return Err("Launcher feature not negotiated".into());
    }
    let m = &p.package.manifest;
    let id = m["id"].as_str().ok_or("Missing identity")?;
    let d = descriptors(m)
        .find(|d| d["id"] == params["contributionId"] && d["index"] == true)
        .ok_or("Undeclared launcher index")?;
    let revision = params["revision"]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or("Invalid launcher index revision")?;
    let rows = rows(m, d, &params["rows"], true)?;
    let compact:Vec<_>=rows.into_iter().map(|r|json!({"id":r["id"],"title":r["title"],"subtitle":r["subtitle"],"score":r["score"],"tokens":r["tokens"],"action":{"type":"run-extension-command","commandName":d["commandName"],"argumentValues":r["action"]["argumentValues"]}})).collect();
    let _guard = process_install::PACKAGE_LOCK
        .lock()
        .map_err(|e| e.to_string())?;
    if !p.alive() || !current(&p.host.ctx, &p.package) {
        return Err("Stale launcher publisher".into());
    }
    let mut db = read_index(&p.host.ctx, m)?;
    if !db.is_object() {
        return Err("Invalid launcher index".into());
    }
    let key = d["id"].as_str().unwrap();
    if db["indexes"][key]["owner"] == p.generation
        && db["indexes"][key]["revision"]
            .as_u64()
            .is_some_and(|old| old >= revision)
    {
        return Err("Stale launcher index revision".into());
    }
    if db["fingerprint"].is_null() {
        db = json!({"fingerprint":fingerprint(m),"indexes":{}});
    }
    db["indexes"][key] = json!({"owner":p.generation,"revision":revision,"rows":compact});
    if serde_json::to_vec(&db).map_err(|e| e.to_string())?.len() > MAX_INDEX {
        return Err("Launcher index too large".into());
    }
    let index_path = path(&p.host.ctx, id)?;
    let parent = index_path.parent().ok_or("Missing index directory")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    write_json_atomic(&index_path, &db)?;
    p.launcher_revision
        .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    p.host.ctx.emit("launcher:invalidated", json!({}));
    Ok(json!({"published":true,"revision":revision}))
}
pub(super) fn index_rows(
    ctx: &CoreContext,
    m: &Value,
    d: &Value,
    text: &str,
) -> Result<Vec<Value>, String> {
    let db = read_index(ctx, m)?;
    let q = text.trim().to_lowercase();
    if q.is_empty() {
        return Ok(vec![]);
    }
    let selected: Vec<_> = db["indexes"][d["id"].as_str().unwrap_or("")]["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| {
            r["tokens"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .any(|t| t.to_lowercase().contains(&q))
        })
        .take(d["limit"].as_u64().unwrap_or(12) as usize)
        .cloned()
        .collect();
    rows(m, d, &json!(selected), true)
}
pub(super) fn needs_bootstrap(ctx: &CoreContext, p: &Package, explicit: bool) -> bool {
    if !current(ctx, p) {
        return false;
    }
    let db = read_index(ctx, &p.manifest).unwrap_or(json!({}));
    descriptors(&p.manifest).any(|d| {
        d["index"] == true
            && db["indexes"].get(d["id"].as_str().unwrap_or("")).is_none()
            && (explicit
                || d["bootstrapMigrationPaths"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .any(|key| {
                        p.manifest["migrationPaths"][key]
                            .as_str()
                            .and_then(|s| process_install::relative(s).ok())
                            .is_some_and(|r| ctx.paths.data_dir.join(r).is_file())
                    }))
    })
}

#[cfg(test)]
#[path = "launcher_tests.rs"]
mod tests;
