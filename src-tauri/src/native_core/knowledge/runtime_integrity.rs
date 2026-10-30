//! Installation verification, not a model registry. A cache is populated only by
//! hashing actual bytes in this process; filesystem metadata is only an invalidator.
use super::{server, EmbeddingVariantId, Manifest, ModelDescriptor};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct ModelIdentity {
    pub descriptor: &'static ModelDescriptor,
    pub bytes: u64,
    pub sha: &'static str,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Fingerprint {
    path: PathBuf,
    target: PathBuf,
    bytes: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}
#[derive(Clone)]
pub(super) struct VerifiedInstall {
    fingerprint: Vec<Fingerprint>,
    model: ModelIdentity,
    runtime: String,
    pub executable: PathBuf,
    pub runtime_files: Value,
    pub legacy: bool,
    pub bundle: Option<super::runtime_bundle::VerifiedBundle>,
}
pub(super) fn fingerprint(path: &Path) -> Result<Fingerprint, String> {
    let m = fs::metadata(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if !m.is_file() {
        return Err(format!("Not a file: {}", path.display()));
    }
    Ok(Fingerprint {
        path: path.to_owned(),
        target: fs::canonicalize(path).map_err(|e| e.to_string())?,
        bytes: m.len(),
        modified: m.modified().ok(),
        created: m.created().ok(),
        #[cfg(unix)]
        identity: {
            use std::os::unix::fs::MetadataExt;
            (m.dev(), m.ino(), m.ctime(), m.ctime_nsec())
        },
    })
}
pub(super) fn inventory(directory: &Path) -> Result<Vec<Fingerprint>, String> {
    let root = fs::canonicalize(directory).map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(directory).max_depth(8) {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().is_dir() {
            continue;
        }
        let item = fingerprint(entry.path())?;
        if !item.target.starts_with(&root) {
            return Err("Runtime symlink escapes its installation".into());
        }
        files.push(item);
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}
fn hash(path: &Path, check: &impl Fn() -> Result<(), String>) -> Result<String, String> {
    check()?;
    let before = fingerprint(path)?;
    let mut input = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        check()?;
        let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    if before != fingerprint(path)? {
        return Err("Artifact changed during verification".into());
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub(super) fn runtime_files(
    directory: &Path,
    check: &impl Fn() -> Result<(), String>,
) -> Result<Value, String> {
    let mut result = serde_json::Map::new();
    for item in inventory(directory)? {
        let key = item
            .path
            .strip_prefix(directory)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        result.insert(
            key,
            json!({"bytes":item.bytes,"sha256":hash(&item.path, check)?}),
        );
    }
    Ok(Value::Object(result))
}
pub(super) fn manifest(runtime: &Manifest, model: ModelIdentity, files: Value) -> Value {
    let descriptor = model.descriptor;
    json!({"schema":2,"revision":descriptor.revision,"runtime":runtime.file,
        "variantId":descriptor.id,"sourceRepository":descriptor.repository,
        "modelFile":descriptor.file,"modelBytes":model.bytes,"modelSha256":model.sha,
        "modelKey":descriptor.model_key,"runtimeFiles":files})
}
pub(super) fn verify(
    directory: &Path,
    runtime: &Manifest,
    model: ModelIdentity,
    cached: Option<&VerifiedInstall>,
    check: &impl Fn() -> Result<(), String>,
) -> Result<Option<VerifiedInstall>, String> {
    check()?;
    let descriptor = model.descriptor;
    let path = directory.join("installed.json");
    if !path.exists()
        && !directory.join(descriptor.file).exists()
        && !directory.join("runtime").exists()
    {
        return Ok(None);
    }
    let manifest_fingerprint = fingerprint(&path)?;
    let installed: Value = serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let legacy = installed.get("schema").is_none();
    if (legacy && descriptor.id != EmbeddingVariantId::Q8_0)
        || installed["revision"] != descriptor.revision
        || installed["runtime"] != runtime.file
        || (!legacy && installed["schema"] != 2)
    {
        return Err("Installed model/runtime identity does not match this version".into());
    }
    // Only Q8 predates explicit variant/source fields. Q4 must carry its full
    // provenance even if a malformed manifest resembles an old Q8 installation.
    for (key, expected) in [
        ("variantId", json!(descriptor.id)),
        ("sourceRepository", json!(descriptor.repository)),
    ] {
        if (descriptor.id != EmbeddingVariantId::Q8_0 || installed.get(key).is_some())
            && installed[key] != expected
        {
            return Err(format!("Installed model identity mismatch: {key}"));
        }
    }
    for (key, expected) in [
        ("modelFile", json!(descriptor.file)),
        ("modelBytes", json!(model.bytes)),
        ("modelSha256", json!(model.sha)),
        ("modelKey", json!(descriptor.model_key)),
    ] {
        if (!legacy || installed.get(key).is_some()) && installed[key] != expected {
            return Err(format!("Installed model identity mismatch: {key}"));
        }
    }
    let model_path = directory.join(descriptor.file);
    let model_fingerprint = fingerprint(&model_path)?;
    if model_fingerprint.bytes != model.bytes {
        return Err("Installed model size is incorrect".into());
    }
    let runtime_dir = directory.join("runtime");
    let files = inventory(&runtime_dir)?;
    let executable =
        server(&runtime_dir).ok_or("Installed runtime does not contain llama-server")?;
    let mut current = vec![manifest_fingerprint, model_fingerprint];
    current.extend(files);
    if let Some(cached) = cached {
        if cached.model == model && cached.runtime == runtime.file && cached.fingerprint == current
        {
            return Ok(Some(cached.clone()));
        }
    }
    if hash(&model_path, check)? != model.sha {
        return Err("Installed model failed SHA-256 verification".into());
    }
    let digests = runtime_files(&runtime_dir, check)?;
    // Old installs discarded the verified archive and have no executable digest.
    // Preserve compatibility, but label this as a local baseline, never archive verification.
    // Once observed in this process, that baseline must not silently change.
    let expected = if legacy {
        cached.map(|v| &v.runtime_files)
    } else {
        Some(&installed["runtimeFiles"])
    };
    if let Some(expected) = expected {
        if expected != &digests {
            return Err("Installed runtime failed file integrity verification".into());
        }
    }
    let mut after = vec![fingerprint(&path)?, fingerprint(&model_path)?];
    after.extend(inventory(&runtime_dir)?);
    if current != after {
        return Err("Installation changed during verification".into());
    }
    check()?;
    Ok(Some(VerifiedInstall {
        fingerprint: current,
        model,
        runtime: runtime.file.to_owned(),
        executable,
        runtime_files: digests,
        legacy,
        bundle: None,
    }))
}

/// A foreground request may inspect a verified fingerprint, but never start a
/// fresh full-file hash. A changed/missing artifact requires background verify.
pub(super) fn cached_install(
    directory: &Path,
    runtime: &Manifest,
    model: ModelIdentity,
    cached: Option<&VerifiedInstall>,
) -> Option<VerifiedInstall> {
    let cached = cached?;
    if cached.model != model || cached.runtime != runtime.file {
        return None;
    }
    let mut current = vec![
        fingerprint(&directory.join("installed.json")).ok()?,
        fingerprint(&directory.join(model.descriptor.file)).ok()?,
    ];
    current.extend(inventory(&directory.join("runtime")).ok()?);
    (current == cached.fingerprint
        && cached
            .bundle
            .as_ref()
            .map(|b| b.unchanged())
            .unwrap_or(true))
    .then(|| cached.clone())
}
