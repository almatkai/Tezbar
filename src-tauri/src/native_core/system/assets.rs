use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
fn data(path: &Path) -> Value {
    let mime = match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "svg" => "image/svg+xml",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "image/png",
    };
    std::fs::read(path)
        .ok()
        .map(|b| json!(format!("data:{mime};base64,{}", STANDARD.encode(b))))
        .unwrap_or(Value::Null)
}
fn valid_application(path: &str) -> bool {
    if cfg!(target_os = "macos") {
        path.ends_with(".app")
    } else if cfg!(windows) {
        path.starts_with("shell:AppsFolder\\")
            || ["exe", "lnk", "url"]
                .iter()
                .any(|e| path.to_ascii_lowercase().ends_with(&format!(".{e}")))
    } else {
        false
    }
}
impl SystemCore {
    pub(super) async fn icon(&self, kind: &str, raw: &str) -> CoreResult {
        let path = raw.trim();
        if path.is_empty() {
            return Ok(Value::Null);
        }
        if kind == "extension" {
            if path.starts_with("https://") || path.starts_with("http://") {
                return Ok(json!(path));
            }
            let path = PathBuf::from(path);
            return blocking(move || Ok(data(&path))).await;
        }
        if !matches!(kind, "application" | "file")
            || (kind == "application" && !valid_application(path))
        {
            return Ok(Value::Null);
        }
        // UWP icon extraction is not available through ExtractAssociatedIcon; match legacy null behavior.
        if path.starts_with("shell:AppsFolder\\") {
            return Ok(Value::Null);
        }
        let path = PathBuf::from(path);
        let p = path.clone();
        let metadata = blocking(move || {
            Ok(std::fs::metadata(&p)
                .ok()
                .map(|m| {
                    json!([
                        m.len(),
                        m.modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_millis().to_string())
                    ])
                })
                .unwrap_or(Value::Null))
        })
        .await?;
        if metadata.is_null() {
            return Ok(Value::Null);
        }
        let key = format!("{kind}:{}:{metadata}", path.display());
        if let Some(v) = self.state.icons.lock().await.get(&key).cloned() {
            return Ok(v);
        }
        let _slot = self
            .state
            .icon_slots
            .acquire()
            .await
            .map_err(|e| e.to_string())?;
        if let Some(v) = self.state.icons.lock().await.get(&key).cloned() {
            return Ok(v);
        }
        let hash = format!("{:x}", Sha1::digest(key.as_bytes()));
        let dir = self
            .ctx
            .paths
            .data_dir
            .join("icon-cache")
            .join(if kind == "application" {
                "applications"
            } else {
                "files"
            })
            .join(hash);
        let dest = dir.join(if kind == "application" {
            "icon.png"
        } else {
            "preview.png"
        });
        let d = dir.clone();
        blocking(move || {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
            Ok(Value::Null)
        })
        .await?;
        let exists = tokio::fs::try_exists(&dest).await.unwrap_or(false);
        let generated = if exists {
            Ok(())
        } else if cfg!(windows) {
            let mut c = Command::new("powershell.exe");
            c.args(["-NoLogo","-NoProfile","-NonInteractive","-Command",r#"$ErrorActionPreference='Stop';Add-Type -AssemblyName System.Drawing;$icon=[System.Drawing.Icon]::ExtractAssociatedIcon($env:TEZBAR_ICON_SOURCE);if($null -eq $icon){exit 2};try{$bitmap=$icon.ToBitmap();try{$bitmap.Save($env:TEZBAR_ICON_DEST,[System.Drawing.Imaging.ImageFormat]::Png)}finally{$bitmap.Dispose()}}finally{$icon.Dispose()}"#]).env("TEZBAR_ICON_SOURCE",&path).env("TEZBAR_ICON_DEST",&dest);
            output(c, Duration::from_secs(5)).await.map(|_| ())
        } else if kind == "application" {
            self.mac_app_icon(&path, &dest).await
        } else {
            let mut c = Command::new("/usr/bin/qlmanage");
            c.args(["-t", "-i", "-s", "64", "-o"]).arg(&dir).arg(&path);
            match output(c, Duration::from_secs(3)).await {
                Ok(_) => {
                    let generated = dir.join(format!(
                        "{}.png",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                    tokio::fs::rename(generated, &dest)
                        .await
                        .map_err(|e| e.to_string())
                }
                Err(e) => Err(e),
            }
        };
        let v = if generated.is_ok() {
            let p = dest.clone();
            blocking(move || Ok(data(&p))).await?
        } else if kind == "application" && path.to_string_lossy().to_lowercase().contains("brew") {
            let resource = self.ctx.paths.resource_dir.clone();
            blocking(move || {
                for p in [
                    resource.join("icons/homebrew.png"),
                    resource.join("resources/icons/homebrew.png"),
                ] {
                    let v = data(&p);
                    if !v.is_null() {
                        return Ok(v);
                    }
                }
                Ok(Value::Null)
            })
            .await?
        } else {
            Value::Null
        };
        let mut cache = self.state.icons.lock().await;
        if cache.len() >= 1024 {
            cache.clear();
        }
        cache.insert(key, v.clone());
        Ok(v)
    }
    async fn mac_app_icon(&self, path: &Path, dest: &Path) -> Result<(), String> {
        let resources = path.join("Contents/Resources");
        let plist = path.join("Contents/Info.plist");
        let configured = run(
            "/usr/bin/plutil",
            &[
                "-extract",
                "CFBundleIconFile",
                "raw",
                "-o",
                "-",
                &plist.to_string_lossy(),
            ],
        )
        .await
        .ok();
        let source = blocking(move || {
            let configured = configured.map(|s| {
                if Path::new(&s).extension().is_none() {
                    format!("{s}.icns")
                } else {
                    s
                }
            });
            if let Some(s) = configured {
                let p = resources.join(s);
                if p.is_file() {
                    return Ok(json!(p));
                }
            }
            let entries = std::fs::read_dir(&resources).map_err(|e| e.to_string())?;
            for e in entries.flatten() {
                if e.path()
                    .extension()
                    .is_some_and(|s| s.eq_ignore_ascii_case("icns"))
                {
                    return Ok(json!(e.path()));
                }
            }
            Err("Application has no icon file".into())
        })
        .await?;
        run(
            "/usr/bin/sips",
            &[
                "-Z",
                "64",
                "-s",
                "format",
                "png",
                source.as_str().ok_or("Invalid icon path")?,
                "--out",
                &dest.to_string_lossy(),
            ],
        )
        .await?;
        Ok(())
    }
    pub(super) async fn quick_look(&self, v: &Value) -> CoreResult {
        let mut paths: Vec<String> = v
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_else(|| v.as_str().map(|s| vec![s.to_string()]).unwrap_or_default());
        paths = paths
            .into_iter()
            .map(|p| p.trim().to_owned())
            .filter(|p| previewable(p))
            .collect();
        let mut seen = std::collections::HashSet::new();
        paths.retain(|p| seen.insert(p.clone()));
        if paths.is_empty() {
            return Ok(result(
                false,
                "The selected item cannot be previewed with Quick Look",
            ));
        }
        if !cfg!(target_os = "macos") {
            return Ok(result(false, "Quick Look is only available on macOS"));
        }
        let paths = blocking(move || {
            Ok(json!(paths
                .into_iter()
                .filter(|p| Path::new(p).exists())
                .collect::<Vec<_>>()))
        })
        .await?;
        let paths: Vec<String> = serde_json::from_value(paths).map_err(|e| e.to_string())?;
        if paths.is_empty() {
            return Ok(result(false, "The selected file no longer exists"));
        }
        let mut command = Command::new("/usr/bin/qlmanage");
        command
            .arg("-p")
            .args(&paths)
            .kill_on_drop(true)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        // Await panel lifetime, with no short CLI timeout: caller keeps blur suppression until completion.
        let status = command.status().await;
        Ok(match status {
            Ok(s) if s.success() => result(
                true,
                format!(
                    "Previewed {}",
                    Path::new(&paths[0])
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                ),
            ),
            Ok(s) => result(false, format!("Could not preview file: {s}")),
            Err(e) => result(false, format!("Could not preview file: {e}")),
        })
    }
}
pub(super) fn previewable(p: &str) -> bool {
    let ext = Path::new(p)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    [
        "avif", "aiff", "avi", "bmp", "csv", "doc", "docx", "flac", "gif", "heic", "heif", "htm",
        "html", "ico", "jpeg", "jpg", "jfif", "key", "m4a", "m4v", "md", "mov", "mp3", "mp4",
        "numbers", "odp", "ods", "odt", "pages", "pdf", "png", "ppt", "pptx", "rtf", "rtfd", "svg",
        "tsv", "tif", "tiff", "txt", "wav", "webm", "webp", "xls", "xlsm", "xlsx",
    ]
    .contains(&ext.as_str())
}
