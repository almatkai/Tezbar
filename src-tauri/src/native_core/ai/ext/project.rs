//! Project manifests are read-only review items until explicitly imported. Raw
//! bytes are pinned as well as location, so whitespace changes revoke trust.
use super::*;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
fn read_project(cwd: &Path) -> CoreResult {
    let cwd = cwd.canonicalize().map_err(err)?;
    let mut folder = cwd.clone();
    let local = cwd.join(".tezbar/ai-extension.json");
    let path = if local.is_file() {
        local
    } else {
        let mut found = None;
        for parent in cwd.ancestors().take(64) {
            if parent.join(".git").exists() {
                let candidate = parent.join(".tezbar/ai-extension.json");
                if candidate.is_file() {
                    found = Some(candidate);
                    folder = parent.to_path_buf();
                }
                break;
            }
        }
        let Some(path) = found else {
            return Ok(Value::Null);
        };
        path
    };
    let real = path.canonicalize().map_err(err)?;
    if !runtime::policy::within(&real, &folder) {
        return Ok(
            json!({"folder":folder,"path":path,"error":"Project configuration points outside this folder. Review a local file instead."}),
        );
    }
    use std::io::Read;
    let file = std::fs::File::open(&real).map_err(err)?;
    if !file.metadata().map_err(err)?.is_file() {
        return Ok(Value::Null);
    }
    let mut bytes = vec![];
    file.take(256 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() > 256 * 1024 {
        return Ok(
            json!({"folder":folder,"path":path,"error":"Project configuration exceeds 256 KiB"}),
        );
    }
    let mut digest = Sha256::new();
    digest.update(folder.to_string_lossy().as_bytes());
    digest.update([0]);
    digest.update(&bytes);
    let hash = format!("{:x}", digest.finalize());
    let text = String::from_utf8(bytes).map_err(|_| "Project configuration isn't UTF-8")?;
    let parsed = manifest::parse(&text);
    let manifest = match parsed {
        Ok(v) => v,
        Err(e) => return Ok(json!({"folder":folder,"path":path,"entryHash":hash,"error":e})),
    };
    let mut preview = manifest.clone();
    for c in preview["connections"].as_array_mut().into_iter().flatten() {
        for section in ["env", "headers"] {
            for (k, v) in c[section].as_object_mut().into_iter().flatten() {
                if let Some(literal) = v.as_str() {
                    if manifest::secret_key(k, literal) {
                        *v = json!({"secret":k,"label":k});
                    }
                }
            }
        }
    }
    for p in preview["providers"].as_array_mut().into_iter().flatten() {
        for (k, v) in p["headers"].as_object_mut().into_iter().flatten() {
            if let Some(literal) = v.as_str() {
                if manifest::secret_key(k, literal) {
                    *v = json!({"secret":k,"label":k});
                }
            }
        }
    }
    Ok(json!({"folder":folder,"path":path,"entryHash":hash,"manifest":manifest,"preview":preview}))
}
impl AiCore {
    pub(in crate::native_core::ai) async fn project_status(&self, cwd: &Path) -> CoreResult {
        let cwd = cwd.to_path_buf();
        let mut result = tokio::task::spawn_blocking(move || read_project(&cwd))
            .await
            .map_err(err)??;
        if let Some(o) = result.as_object_mut() {
            o.remove("manifest");
        }
        if !result.is_null() {
            let data = self.ext_read().await?;
            if data["extensions"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|e| {
                    e["enabled"] == true
                        && e["projectPin"] == result["entryHash"]
                        && e["projectFile"] == result["path"]
                })
            {
                return Ok(Value::Null);
            }
        }
        Ok(result)
    }
    pub(in crate::native_core::ai) async fn revoke_changed_projects(
        &self,
        cwd: &Path,
    ) -> Result<(), String> {
        let data = self.ext_read().await?;
        let mut changed = vec![];
        for e in data["extensions"].as_array().unwrap() {
            let Some(folder) = e["projectPath"].as_str() else {
                continue;
            };
            if !runtime::policy::within(cwd, Path::new(folder)) {
                continue;
            }
            let file = e["projectFile"].as_str().map(PathBuf::from);
            let pin = e["projectPin"].clone();
            let folder = PathBuf::from(folder);
            let current = tokio::task::spawn_blocking(move || read_project(&folder))
                .await
                .map_err(err)?
                .unwrap_or(Value::Null);
            if current["entryHash"] != pin
                || file.as_ref().is_some_and(|p| current["path"] != json!(p))
            {
                changed.push(s(&e["id"]).to_string());
            }
        }
        if changed.is_empty() {
            return Ok(());
        }
        let core = self.clone();
        self.ext_edit(move |data| {
            for id in changed {
                let r = record_mut(data, &id)?;
                r["enabled"] = json!(false);
                r["needsProjectReview"] = json!(true);
                for c in r["connections"].as_array_mut().into_iter().flatten() {
                    c["tested"] = json!(false);
                    c["allowances"] = json!({});
                    if let Some(client) = core
                        .inner
                        .extensions
                        .clients
                        .lock()
                        .unwrap()
                        .remove(&key(&id, s(&c["id"])))
                    {
                        client.shutdown();
                    }
                }
            }
            Ok(json!({"ok":true}))
        })
        .await
        .map(|_| ())
    }
    pub(in crate::native_core::ai) async fn trust_project(
        &self,
        cwd: &str,
        pin: &str,
    ) -> CoreResult {
        let cwd = PathBuf::from(cwd);
        let project = tokio::task::spawn_blocking(move || read_project(&cwd))
            .await
            .map_err(err)??;
        if project["entryHash"] != pin || !project["manifest"].is_object() {
            return Err("The project configuration changed. Review it again.".into());
        }
        let mut manifest = project["manifest"].clone();
        let id = format!("project.{}", &pin[..32]);
        manifest["id"] = json!(id);
        if let Some(existing) = self.ext_read().await?["extensions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| e["id"] == id)
            .cloned()
        {
            return Ok(existing);
        }
        let record = Box::pin(self.ext_handle("ai-extensions:add", &[manifest])).await?;
        let id = s(&record["id"]).to_string();
        self.ext_edit(move |data| {
            let r = record_mut(data, &id)?;
            r["projectPath"] = project["folder"].clone();
            r["projectFile"] = project["path"].clone();
            r["projectPin"] = project["entryHash"].clone();
            Ok(r.clone())
        })
        .await
    }
}
