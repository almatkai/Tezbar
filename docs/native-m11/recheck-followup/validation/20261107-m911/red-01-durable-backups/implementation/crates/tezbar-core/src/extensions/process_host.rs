//! Typed host API, manifest grants, scoped resources and durable activations.
//! Deliberately NOT an IPC/channel bridge. Executables remain trusted native code,
//! not an OS sandbox; capability grants constrain only services provided by Tezbar.
use super::{process::Process, process_install};
use crate::common::{read_json, write_json_atomic, CoreContext, CoreResult};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};
static BACKGROUND_LOCK: Mutex<()> = Mutex::new(());
static BLUR: LazyLock<tokio::sync::Mutex<HashSet<String>>> =
    LazyLock::new(|| tokio::sync::Mutex::new(HashSet::new()));
#[derive(Default)]
struct Resources {
    leases: HashSet<String>,
    trays: HashMap<String, String>,
    notifications: HashSet<String>,
    blur: bool,
}
pub(super) struct Host {
    pub ctx: CoreContext,
    id: String,
    manifest: Value,
    owner: String,
    resources: Mutex<Resources>,
}
impl Host {
    pub fn new(ctx: CoreContext, manifest: &Value) -> Result<Self, String> {
        process_install::validate(manifest)?;
        Ok(Self {
            ctx,
            id: manifest["id"]
                .as_str()
                .ok_or("Missing extension id")?
                .into(),
            manifest: manifest.clone(),
            owner: uuid::Uuid::new_v4().to_string(),
            resources: Mutex::new(Resources::default()),
        })
    }
    pub fn paths(&self) -> Result<(PathBuf, PathBuf), String> {
        let data = self
            .ctx
            .paths
            .data_dir
            .join("external-extensions")
            .join(&self.id);
        let temp = self
            .ctx
            .paths
            .temp_dir
            .join("tezbar-extensions")
            .join(&self.id)
            .join(&self.owner);
        for (base, path) in [
            (&self.ctx.paths.data_dir, &data),
            (&self.ctx.paths.temp_dir, &temp),
        ] {
            std::fs::create_dir_all(base).map_err(|e| e.to_string())?;
            let mut current = base.clone();
            for part in path
                .strip_prefix(base)
                .map_err(|e| e.to_string())?
                .components()
            {
                current.push(part);
                if !current.exists() {
                    match std::fs::create_dir(&current) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(e) => return Err(e.to_string()),
                    }
                }
                let metadata = std::fs::symlink_metadata(&current).map_err(|e| e.to_string())?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err("Extension private directory symlink/non-directory rejected".into());
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&current, std::fs::Permissions::from_mode(0o700))
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        Ok((data, temp))
    }
    pub fn migration_paths(&self) -> Value {
        let mut result = serde_json::Map::new();
        if let Some(paths) = self.manifest["migrationPaths"].as_object() {
            for (key, value) in paths {
                if let Some(path) = value.as_str() {
                    if let Ok(relative) = process_install::relative(path) {
                        let path = self.ctx.paths.data_dir.join(relative);
                        // Explicit files, or narrow directories beneath this
                        // extension's own legacy namespace. Never a profile root.
                        if path
                            .strip_prefix(&self.ctx.paths.data_dir)
                            .ok()
                            .is_some_and(|rel| {
                                let mut cur = self.ctx.paths.data_dir.clone();
                                rel.components().all(|part| {
                                    cur.push(part);
                                    std::fs::symlink_metadata(&cur)
                                        .is_ok_and(|m| !m.file_type().is_symlink())
                                })
                            })
                            && (path.is_file()
                                || (path.is_dir()
                                    && scoped_migration_directory(
                                        self.manifest["slug"].as_str().unwrap_or(""),
                                        value.as_str().unwrap_or(""),
                                    )
                                    && safe_migration_tree(&path)))
                        {
                            result.insert(key.clone(), json!(path));
                        }
                    }
                }
            }
        }
        Value::Object(result)
    }
    pub fn retained(&self) -> bool {
        let r = self.resources.lock().unwrap_or_else(|e| e.into_inner());
        !r.leases.is_empty() || !r.trays.is_empty() || !r.notifications.is_empty() || r.blur
    }
    fn granted(&self, method: &str, cap: &str) -> bool {
        self.manifest["capabilities"].as_array().is_some_and(|a| {
            a.iter()
                .any(|v| v == cap || v == method || v.as_str() == method.strip_prefix("host."))
        })
    }
    fn require(&self, method: &str, cap: &str) -> Result<(), String> {
        if !self.granted(method, cap) {
            Err(format!("Capability not granted: {cap}"))
        } else {
            Ok(())
        }
    }
    fn scoped(&self, id: &str) -> Result<String, String> {
        process_install::component(id)?;
        Ok(format!("tezbar-ext-{}-{}-{id}", self.id, self.owner))
    }
    async fn blur(&self, value: bool) -> CoreResult {
        let mut owners = BLUR.lock().await;
        if value {
            owners.insert(self.owner.clone());
        } else {
            owners.remove(&self.owner);
        }
        self.resources.lock().map_err(|e| e.to_string())?.blur = value;
        let result = self
            .ctx
            .request(
                "core:platform:suppress-blur",
                vec![json!(!owners.is_empty())],
            )
            .await;
        self.resources.lock().map_err(|e| e.to_string())?.blur = value;
        result
    }
    pub async fn request(&self, p: &Arc<Process>, method: &str, params: Value) -> CoreResult {
        let capability = match method {
            "host.clipboard.read"
            | "host.clipboard.write"
            | "host.clipboard.writeImage"
            | "host.clipboard.writeFiles"
            | "host.clipboard.paste"
            | "host.clipboard.pasteText" => "clipboard",
            "host.shell.open" | "host.shell.openWithApp" | "host.shell.reveal" => "shell",
            "host.selection.read" => "selection",
            "host.window.visibility" | "host.window.suppressBlur" => "window",
            "host.dialog.confirm" => "dialog",
            "host.screen.capture" => "screen",
            "host.apps.list" => "apps",
            "host.settings.open" => "settings",
            "host.ai.ask" => "ai",
            "host.background.retain" | "host.background.release" => "background",
            "host.tray.set" | "host.tray.remove" => "tray",
            "host.notifications.show" | "host.notifications.remove" => "notifications",
            "host.safety.authorize" | "host.safety.record" => "safety",
            "host.launcher.publish" => "launcher",
            _ => return Err(format!("Unknown typed host capability: {method}")),
        };
        self.require(method, capability)?;
        if !params.is_object() {
            return Err("Capability parameters must be an object".into());
        }
        if !p.ready() {
            return Err(
                "Host UI/service calls are unavailable before initialization completes".into(),
            );
        }
        if !p.alive() {
            return Err("Extension generation is no longer active".into());
        }
        match method {
            "host.launcher.publish" => super::launcher::publish(p, &params),
            "host.background.retain"|"host.background.release"=>{
                let key=string(&params,"key")?;process_install::component(key)?;
                let retain=method.ends_with("retain");
                // Reserve capacity under the same lock as persistence. A failed
                // retain must never leave a durable activation without an owner.
                let mut r=self.resources.lock().map_err(|e|e.to_string())?;
                if retain&&r.leases.len()>=128&&!r.leases.contains(key){return Err("Too many background leases".into());}
                // Commit durable state BEFORE returning acknowledgment. Do not
                // erase durable records on crash/app shutdown; only release or
                // explicit uninstall removes them.
                background_update(&self.ctx,&self.id,key,if retain&&params["durable"]==true{Some(params["data"].clone())}else{None})?;
                if retain{r.leases.insert(key.into());}else{r.leases.remove(key);}
                p.changed.notify_one();Ok(json!({"retained":retain,"key":key}))
            },
            "host.window.suppressBlur"=>{let value=params["suppress"].as_bool().ok_or("Missing suppress boolean")?;let result=self.blur(value).await;p.changed.notify_one();result},
            "host.tray.set"=>{self.set_tray(p,params).await?;p.changed.notify_one();Ok(json!(true))},
            "host.tray.remove"=>{let id=string(&params,"resourceId")?;self.ctx.host.status_items().remove(&self.scoped(id)?)?;self.resources.lock().map_err(|e|e.to_string())?.trays.remove(id);p.changed.notify_one();Ok(json!(true))},
            "host.notifications.show"=>{
                let id=string(&params,"id")?;let scoped=self.scoped(id)?;
                {let r=self.resources.lock().map_err(|e|e.to_string())?;if r.notifications.len()>=128&&!r.notifications.contains(id){return Err("Too many notification resources".into());}}
                if params.get("action").is_some_and(|v|!v.is_null()){return Err("Notification actions require platform activation integration; not supported by this host revision".into());}
                let title=string(&params,"title")?;let body=string(&params,"body")?;
                // Own the resource before an asynchronous OS call can deliver it.
                // Cancellation/process death must still remove a late delivery.
                self.resources.lock().map_err(|e|e.to_string())?.notifications.insert(id.into());p.changed.notify_one();
                if let Err(error)=self.ctx.host.notify(crate::host::NotificationSpec::Show { id: scoped.clone(), title: title.into(), body: body.into() }).await{
                    let _=self.ctx.host.notify(crate::host::NotificationSpec::Remove { id: scoped.clone() }).await;
                    self.resources.lock().map_err(|e|e.to_string())?.notifications.remove(id);p.changed.notify_one();return Err(error);
                }Ok(json!(true))
            },
            "host.notifications.remove"=>{let id=string(&params,"id")?;self.ctx.host.notify(crate::host::NotificationSpec::Remove { id: self.scoped(id)? }).await?;self.resources.lock().map_err(|e|e.to_string())?.notifications.remove(id);p.changed.notify_one();Ok(json!(true))},
            "host.clipboard.read"=>match params["format"].as_str(){None|Some("text")=>self.ctx.request("clipboard:read",vec![]).await,Some("image")=>{if let Some(r)=self.ctx.host.desktop_effect(crate::host::DesktopEffect::ReadImage){r}else{read_clipboard_image().await}},_=>Err("Unsupported clipboard format".into())},
            "host.clipboard.write"=>{
                let text=string(&params,"text")?;
                if let Some(html)=params.get("html"){if !html.is_string(){return Err("Clipboard HTML must be text".into());}self.ctx.request("core:platform:clipboard-write",vec![json!({"text":text,"html":html})]).await}
                else{self.ctx.request("clipboard:write",vec![json!(text)]).await}
            },
            "host.clipboard.writeImage"=>self.ctx.request("core:platform:clipboard-write",vec![json!({"file":params["file"].as_str().or_else(||params["path"].as_str()).ok_or("Missing image file")?})]).await,
            "host.clipboard.writeFiles"=>{
                let paths=clipboard_file_paths(&params)?;
                if let Some(r)=self.ctx.host.desktop_effect(crate::host::DesktopEffect::WriteFiles(paths.into_iter().map(PathBuf::from).collect())){r}else{write_files(&params).await}
            },
            "host.clipboard.paste"=>self.ctx.request("core:platform:paste",vec![]).await,
            "host.clipboard.pasteText"=>self.ctx.request("core:platform:paste-text",vec![json!(string(&params,"text")?)]).await,
            "host.shell.open"=>self.ctx.request("shell:open",vec![json!(string(&params,"target")?)]).await,
            "host.shell.openWithApp"=>self.ctx.request("core:platform:open-with-app",vec![json!({"path":string(&params,"path")?,"appPath":string(&params,"appPath")?})]).await,
            "host.shell.reveal"=>self.ctx.request("core:platform:reveal-file",vec![json!(string(&params,"path")?)]).await,
            "host.selection.read"=>match params["kind"].as_str(){None|Some("text")=>self.ctx.request("core:platform:selected-text",vec![]).await,Some("files")=>{if let Some(r)=self.ctx.host.desktop_effect(crate::host::DesktopEffect::SelectedFiles){r}else{selected_files().await}},_=>Err("Unsupported selection kind".into())},
            "host.window.visibility"=>self.ctx.request("core:platform:visibility",vec![json!(params["visible"].as_bool().ok_or("Missing visible boolean")?)]).await,
            "host.dialog.confirm"=>self.ctx.request("core:platform:confirm",vec![params]).await,
            "host.screen.capture"=>{
                let value=self.ctx.request("core:platform:screenshot",vec![json!({"maxWidth":params["maxWidth"]})]).await?;
                if value["mimeType"]!="image/png"||!value["data"].as_str().is_some_and(|s|s.len()<=15*1024*1024){return Err("Capture image exceeds protocol limit or has invalid shape".into());}Ok(value)
            },
            "host.apps.list"=>self.ctx.request("core:apps:list",vec![]).await,
            "host.settings.open"=>self.ctx.request("core:platform:open-settings",vec![json!(params["section"].as_str().or_else(||params["surface"].as_str()).unwrap_or("settings-extensions"))]).await,
            "host.ai.ask"=>{
                let prompt=string(&params,"prompt")?;
                let options=params.get("options").cloned().unwrap_or(json!({}));
                if !options.is_object(){return Err("AI options must be an object".into());}
                self.ctx.request("core:ai:extension-ask",vec![json!(prompt),options]).await
            },
            "host.safety.authorize"|"host.safety.record"=>{
                let operation=string(&params,"operation")?;
                if operation.len()>128{return Err("Safety operation too long".into());}
                let mut context=params["context"].as_object().cloned().ok_or("Safety context must be an object")?;
                context.insert("extensionId".into(),json!(self.id));
                let mut args=vec![json!(operation),Value::Object(context)];
                if method.ends_with("record"){args.push(params["result"].clone());}
                self.ctx.request(if method.ends_with("authorize"){"core:safety:authorize"}else{"core:safety:record"},args).await
            },
            _=>unreachable!()
        }
    }
    async fn set_tray(&self, p: &Arc<Process>, params: Value) -> Result<(), String> {
        let weak = Arc::downgrade(p);
        let resource_id = string(&params, "resourceId")?.to_owned();
        let runtime = tokio::runtime::Handle::current();
        let callback = Arc::new(move |action: String| {
            if let Some(p) = weak.upgrade() {
                let params = json!({"resourceId":resource_id,"actionId":action});
                runtime.spawn(async move {
                    let _ = p.background_event(params).await;
                });
            }
        });
        self.set_status_item(&p.package.dir, params, callback)
    }
    fn set_status_item(
        &self,
        package: &std::path::Path,
        params: Value,
        callback: crate::host::StatusCallback,
    ) -> Result<(), String> {
        let resource = string(&params, "resourceId")?;
        {
            let r = self.resources.lock().map_err(|e| e.to_string())?;
            if r.trays.len() >= 128 && !r.trays.contains_key(resource) {
                return Err("Too many tray resources".into());
            }
        }
        let scoped = self.scoped(resource)?;
        set_status_item(&self.ctx, &scoped, package, &params, callback)?;
        self.resources
            .lock()
            .map_err(|e| e.to_string())?
            .trays
            .insert(resource.into(), scoped);
        Ok(())
    }
    pub async fn release_view_resources(&self) -> CoreResult {
        let blur = self.resources.lock().map_err(|e| e.to_string())?.blur;
        if blur {
            self.blur(false).await
        } else {
            Ok(Value::Null)
        }
    }
    pub async fn cleanup(&self) {
        let (blur, notifications, trays) = {
            let mut r = self.resources.lock().unwrap_or_else(|e| e.into_inner());
            let result = (
                r.blur,
                r.notifications.drain().collect::<Vec<_>>(),
                r.trays
                    .drain()
                    .map(|(_, scoped)| scoped)
                    .collect::<Vec<_>>(),
            );
            r.leases.clear();
            result
        };
        for scoped in trays {
            let _ = self.ctx.host.status_items().remove(&scoped);
        }
        if blur {
            let _ = self.blur(false).await;
        }
        for id in notifications {
            if let Ok(id) = self.scoped(&id) {
                let _ = self
                    .ctx
                    .host
                    .notify(crate::host::NotificationSpec::Remove { id })
                    .await;
            }
        }
        if let Ok((_, temp)) = self.paths() {
            let _ = tokio::fs::remove_dir_all(temp).await;
        }
    }
}
fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v[key]
        .as_str()
        .filter(|s| s.len() <= 1024 * 1024)
        .ok_or_else(|| format!("Missing/invalid {key}"))
}
fn background_path(ctx: &CoreContext) -> PathBuf {
    ctx.paths.data_dir.join("extensions/background.json")
}
fn background_update(
    ctx: &CoreContext,
    id: &str,
    key: &str,
    data: Option<Value>,
) -> Result<(), String> {
    let _guard = BACKGROUND_LOCK.lock().map_err(|e| e.to_string())?;
    let path = background_path(ctx);
    let mut db = if path.exists() {
        read_json(&path)?
    } else {
        json!({})
    };
    let entries = db.as_object_mut().ok_or("Invalid background database")?;
    if data.is_some() {
        if entries.len() >= 128 && !entries.contains_key(id) {
            return Err("Too many background extensions".into());
        }
        let extension = entries.entry(id.to_string()).or_insert(json!({}));
        let keys = extension.as_object_mut().ok_or("Invalid background keys")?;
        if keys.len() >= 128 && !keys.contains_key(key) {
            return Err("Too many background leases".into());
        }
        keys.insert(key.to_string(), data.unwrap_or(Value::Null));
    } else if let Some(extension) = entries.get_mut(id) {
        if let Some(keys) = extension.as_object_mut() {
            keys.remove(key);
            if keys.is_empty() {
                entries.remove(id);
            }
        }
    }
    write_json_atomic(&path, &db)
}
pub(super) fn background_records(ctx: &CoreContext) -> CoreResult {
    let _guard = BACKGROUND_LOCK.lock().map_err(|e| e.to_string())?;
    let path = background_path(ctx);
    if path.exists() {
        read_json(&path)
    } else {
        Ok(json!({}))
    }
}
pub(super) fn forget_background(ctx: &CoreContext, id: &str) -> Result<(), String> {
    let _guard = BACKGROUND_LOCK.lock().map_err(|e| e.to_string())?;
    for path in [
        background_path(ctx),
        ctx.paths.data_dir.join("extensions/startup-probes.json"),
    ] {
        if path.exists() {
            let mut db = read_json(&path)?;
            db.as_object_mut()
                .ok_or("Invalid background/probe database")?
                .remove(id);
            write_json_atomic(&path, &db)?;
        }
    }
    Ok(())
}

fn clipboard_file_paths(params: &Value) -> Result<Vec<String>, String> {
    let paths = params["paths"].as_array().ok_or("Missing file paths")?;
    if paths.is_empty() || paths.len() > 128 {
        return Err("Invalid clipboard file count".into());
    }
    let paths = paths
        .iter()
        .map(|p| p.as_str().map(String::from).ok_or("Invalid clipboard path"))
        .collect::<Result<Vec<_>, _>>()?;
    for path in &paths {
        if !std::path::Path::new(path).is_absolute() || !std::path::Path::new(path).is_file() {
            return Err("Clipboard files require existing absolute regular files".into());
        }
    }
    Ok(paths)
}
fn clipboard_files_command(paths: &[String]) -> Result<tokio::process::Command, String> {
    #[cfg(target_os = "macos")]
    {
        let mut cmd = tokio::process::Command::new("/usr/bin/osascript");
        cmd.args(["-e","on run argv\nset fs to {}\nrepeat with p in argv\nset end of fs to POSIX file p\nend repeat\nset the clipboard to fs\nend run","--"]).args(paths).kill_on_drop(true);
        Ok(cmd)
    }
    #[cfg(target_os = "windows")]
    {
        let json = serde_json::to_string(paths)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let script=format!("$ErrorActionPreference='Stop';Add-Type -AssemblyName System.Windows.Forms;$paths=ConvertFrom-Json '{}';$files=New-Object System.Collections.Specialized.StringCollection;$files.AddRange([string[]]$paths);[System.Windows.Forms.Clipboard]::SetFileDropList($files)",json);
        let mut cmd = tokio::process::Command::new("powershell.exe");
        cmd.args(["-NoProfile", "-NonInteractive", "-STA", "-Command", &script])
            .creation_flags(0x08000000)
            .kill_on_drop(true);
        Ok(cmd)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = paths;
        Err("Clipboard file write unsupported".into())
    }
}

async fn run_clipboard_files_command(mut command: tokio::process::Command) -> CoreResult {
    let out = tokio::time::timeout(Duration::from_secs(10), command.output())
        .await
        .map_err(|_| "Clipboard file write timed out")?
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("OS clipboard file write failed".into());
    }
    Ok(json!(true))
}
async fn write_files(params: &Value) -> CoreResult {
    let paths = clipboard_file_paths(params)?;
    run_clipboard_files_command(clipboard_files_command(&paths)?).await
}

/// One-time migration probing is policy of a verified package, never its slug.
pub(super) fn startup_probes(ctx: &CoreContext) -> Result<Vec<(String, Value, String)>, String> {
    let records = {
        let _guard = BACKGROUND_LOCK.lock().map_err(|e| e.to_string())?;
        let path = ctx.paths.data_dir.join("extensions/startup-probes.json");
        if path.exists() {
            read_json(&path)?
        } else {
            json!({})
        }
    };
    let mut probes = Vec::new();
    let roots = ctx.paths.data_dir.join("extensions/packages");
    for entry in std::fs::read_dir(roots)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
    {
        let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Ok(package) = process_install::resolve(ctx, &id) else {
            continue;
        };
        let Some(probe) = package.manifest.get("startupProbe") else {
            continue;
        };
        use sha2::{Digest, Sha256};
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&package.manifest).map_err(|e| e.to_string())?)
        );
        if records[&id] == fingerprint {
            continue;
        }
        let host = Host::new(ctx.clone(), &package.manifest)?;
        let paths = host.migration_paths();
        if !probe["migrationPaths"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|key| key.as_str().is_some_and(|k| paths.get(k).is_some()))
        {
            continue;
        }
        probes.push((id, probe.clone(), fingerprint));
    }
    Ok(probes)
}
pub(super) fn acknowledge_probe(
    ctx: &CoreContext,
    id: &str,
    fingerprint: &str,
) -> Result<(), String> {
    let _guard = BACKGROUND_LOCK.lock().map_err(|e| e.to_string())?;
    let path = ctx.paths.data_dir.join("extensions/startup-probes.json");
    let mut db = if path.exists() {
        read_json(&path)?
    } else {
        json!({})
    };
    db.as_object_mut()
        .ok_or("Invalid startup probe database")?
        .insert(id.to_string(), json!(fingerprint));
    write_json_atomic(&path, &db)
}

/// Generic selected files API; no extension identity/domain decisions.
async fn selected_files() -> CoreResult {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut cmd = tokio::process::Command::new("/usr/bin/osascript");
        cmd.args(["-l","JavaScript","-e",r#"JSON.stringify(Application('Finder').selection().slice(0,128).map(function(item){return item.url();}))"#]);
        cmd
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut cmd = tokio::process::Command::new("powershell.exe");
        cmd.args(["-NoProfile","-NonInteractive","-Command",r#"$ErrorActionPreference='Stop';Add-Type -TypeDefinition 'using System;using System.Runtime.InteropServices;public static class TezbarSelectionWindow{[DllImport("user32.dll")]public static extern IntPtr GetForegroundWindow();}';$foreground=[TezbarSelectionWindow]::GetForegroundWindow().ToInt64();$paths=@();$shell=New-Object -ComObject Shell.Application;foreach($window in $shell.Windows()){if([long]$window.HWND -eq $foreground){$paths=@($window.Document.SelectedItems() | Select-Object -First 128 | ForEach-Object {$_.Path});break}};ConvertTo-Json -InputObject @($paths) -Compress"#]).creation_flags(0x08000000);
        cmd
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return Err("File selection is unsupported on this platform".into());
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        command
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(10), command.output())
            .await
            .map_err(|_| "File selection timed out")?
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err("Cannot read selected files from Finder/Explorer".into());
        }
        if output.stdout.len() > 1024 * 1024 {
            return Err("File selection response too large".into());
        }
        let values: Vec<String> = serde_json::from_slice(&output.stdout)
            .map_err(|_| "Invalid file selection response")?;
        if values.len() > 128 {
            return Err("Too many selected files".into());
        }
        let mut files = Vec::new();
        for value in values {
            #[cfg(target_os = "macos")]
            let path = reqwest::Url::parse(&value)
                .map_err(|e| e.to_string())?
                .to_file_path()
                .map_err(|_| "Selection URL is not a local file")?;
            #[cfg(target_os = "windows")]
            let path = PathBuf::from(value);
            if path.is_absolute() && path.is_file() {
                files.push(path);
            }
        }
        Ok(json!(files))
    }
}

fn scoped_migration_directory(slug: &str, path: &str) -> bool {
    if process_install::relative(path).is_err() {
        return false;
    }
    let normal = format!("native-extensions-v2/{slug}/");
    let old = format!("native-v2-{slug}/");
    path.strip_prefix(&normal)
        .or_else(|| path.strip_prefix(&old))
        .is_some_and(|suffix| !suffix.is_empty())
}
fn safe_migration_tree(path: &std::path::Path) -> bool {
    for (count, entry) in walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .enumerate()
    {
        if count >= 4096 {
            return false;
        }
        let Ok(entry) = entry else { return false };
        if entry.file_type().is_symlink()
            || (!entry.file_type().is_dir() && !entry.file_type().is_file())
        {
            return false;
        }
    }
    true
}
async fn read_clipboard_image() -> CoreResult {
    tokio::task::spawn_blocking(|| {
        let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        let image = clipboard
            .get_image()
            .map_err(|e| format!("Clipboard contains no readable image: {e}"))?;
        encode_clipboard_image(image.width, image.height, image.bytes.into_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}
fn encode_clipboard_image(width: usize, height: usize, pixels: Vec<u8>) -> CoreResult {
    use base64::Engine;
    if width == 0
        || height == 0
        || width > 16384
        || height > 16384
        || width
            .checked_mul(height)
            .and_then(|n| n.checked_mul(4))
            .is_none_or(|n| n > 64 * 1024 * 1024 || n != pixels.len())
    {
        return Err("Clipboard image exceeds dimensions/size limit".into());
    }
    let image = image::RgbaImage::from_raw(width as u32, height as u32, pixels)
        .ok_or("Invalid clipboard image dimensions")?;
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    // Leave room for JSON/base64 inside the 16 MiB protocol frame.
    if bytes.get_ref().len() > 11 * 1024 * 1024 {
        return Err("Encoded clipboard image exceeds protocol limit".into());
    }
    Ok(
        json!({"data":base64::engine::general_purpose::STANDARD.encode(bytes.into_inner()),"mimeType":"image/png","width":width,"height":height}),
    )
}
#[cfg(test)]
mod shape_tests {
    use super::*;
    fn context(path: &std::path::Path) -> CoreContext {
        use crate::common::CorePaths;
        CoreContext::headless(CorePaths {
            data_dir: path.into(),
            config_dir: path.join("config"),
            resource_dir: path.join("resources"),
            temp_dir: path.join("tmp"),
            home_dir: path.into(),
        })
    }
    #[tokio::test]
    async fn clipboard_media_stays_file_references_and_os_failures_reject() {
        let temp = tempfile::tempdir().unwrap();
        let paths = [
            temp.path().join("animated image.gif"),
            temp.path().join("clip's video.mp4"),
        ];
        for path in &paths {
            std::fs::write(path, b"fixture media; never decoded").unwrap();
        }
        let selected = clipboard_file_paths(&json!({"paths":paths})).unwrap();
        assert_eq!(
            selected,
            paths
                .iter()
                .map(|p| p.to_str().unwrap().to_string())
                .collect::<Vec<_>>()
        );
        let command = clipboard_files_command(&selected).unwrap();
        let args = command
            .as_std()
            .get_args()
            .map(|s| s.to_str().unwrap())
            .collect::<Vec<_>>();
        #[cfg(target_os = "macos")]
        {
            assert!(args[1].contains("set end of fs to POSIX file p"));
            assert_eq!(
                &args[3..],
                selected.iter().map(String::as_str).collect::<Vec<_>>()
            );
        }
        #[cfg(target_os = "windows")]
        {
            let script = args.last().unwrap();
            assert!(script.contains("$ErrorActionPreference='Stop'"));
            assert!(script.contains("Clipboard]::SetFileDropList($files)"));
            assert!(script.contains("clip''s video.mp4"));
        }
        // Exercise the production exit/error handling with a harmless test
        // executable, never the real clipboard command constructed above.
        let mut success = tokio::process::Command::new(std::env::current_exe().unwrap());
        success.arg("--list").kill_on_drop(true);
        assert_eq!(run_clipboard_files_command(success).await.unwrap(), true);
        let mut failure = tokio::process::Command::new(std::env::current_exe().unwrap());
        failure
            .arg("--host-review-invalid-argument")
            .kill_on_drop(true);
        assert!(run_clipboard_files_command(failure)
            .await
            .unwrap_err()
            .contains("OS clipboard file write failed"));
        assert!(run_clipboard_files_command(tokio::process::Command::new(
            temp.path().join("missing-executable")
        ))
        .await
        .is_err());
        for invalid in [
            json!({"paths":[]}),
            json!({"paths":["relative.gif"]}),
            json!({"paths":[temp.path()]}),
            json!({"paths":[temp.path().join("missing.mp4")]}),
            json!({"paths":[42]}),
        ] {
            assert!(clipboard_file_paths(&invalid).is_err());
        }
    }
    #[test]
    fn private_directory_creation_is_safe_under_concurrent_first_use() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let m = super::super::v2_catalog::source()
            .into_iter()
            .find(|m| m["slug"] == "perplexity")
            .unwrap();
        let barrier = Arc::new(std::sync::Barrier::new(16));
        let mut tasks = Vec::new();
        for _ in 0..16 {
            let ctx = ctx.clone();
            let m = m.clone();
            let barrier = barrier.clone();
            tasks.push(std::thread::spawn(move || {
                let host = Host::new(ctx, &m).unwrap();
                barrier.wait();
                host.paths().unwrap()
            }));
        }
        for task in tasks {
            let paths = task.join().unwrap();
            assert!(paths.0.is_dir());
            assert!(paths.1.is_dir());
        }
    }
    #[test]
    fn only_own_narrow_migration_directories_are_granted() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let mut m = super::super::v2_catalog::source()
            .into_iter()
            .find(|m| m["slug"] == "speedtest")
            .unwrap();
        m["migrationPaths"] = json!({"cli":"native-extensions-v2/speedtest/cli","old":"native-extensions-v2/speedtest/storage.json","all":"native-extensions-v2","other":"native-extensions-v2/audio-device/cli"});
        for name in [
            "native-extensions-v2/speedtest/cli",
            "native-extensions-v2/audio-device/cli",
        ] {
            std::fs::create_dir_all(temp.path().join(name)).unwrap();
        }
        std::fs::write(
            temp.path()
                .join("native-extensions-v2/speedtest/storage.json"),
            b"{}",
        )
        .unwrap();
        let host = Host::new(ctx, &m).unwrap();
        let paths = host.migration_paths();
        assert!(paths["cli"].as_str().unwrap().ends_with("speedtest/cli"));
        assert!(paths["old"].is_string());
        assert!(paths.get("all").is_none());
        assert!(paths.get("other").is_none());
        assert_eq!(paths.as_object().unwrap().len(), 2);
    }
    #[test]
    fn image_payload_is_real_png_and_bounded() {
        use base64::Engine;
        let result = encode_clipboard_image(1, 1, vec![0, 128, 255, 255]).unwrap();
        assert_eq!(result["mimeType"], "image/png");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(result["data"].as_str().unwrap())
            .unwrap();
        let image = image::load_from_memory(&bytes).unwrap().into_rgba8();
        assert_eq!(image.get_pixel(0, 0).0, [0, 128, 255, 255]);
        assert!(encode_clipboard_image(16385, 1, vec![]).is_err());
    }
    #[test]
    fn migration_directories_are_narrow_and_symlink_free() {
        assert!(scoped_migration_directory(
            "cleanshotx",
            "native-v2-cleanshotx/captures"
        ));
        assert!(scoped_migration_directory(
            "speedtest",
            "native-extensions-v2/speedtest/cli"
        ));
        for path in [
            "native-extensions-v2",
            "native-extensions-v2/speedtest",
            "native-extensions-v2/other/cli",
            "extensions",
            "config",
        ] {
            assert!(!scoped_migration_directory("speedtest", path));
        }
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("file"), b"x").unwrap();
        assert!(safe_migration_tree(temp.path()));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/tmp", temp.path().join("link")).unwrap();
            assert!(!safe_migration_tree(temp.path()));
        }
    }
}
#[cfg(test)]
mod status_port_tests {
    use super::*;
    #[test]
    fn sdk_status_fallback_policy_is_portable_visible_and_distinguishes_refusal() {
        for kind in [
            SdkStatusIconFallback::Absent,
            SdkStatusIconFallback::Unavailable,
        ] {
            assert!(
                sdk_status_icon_fallback(false, kind).is_none(),
                "macOS uses title/tooltip, not duplicate glyphs"
            );
            let (pixels, w, h) = sdk_status_icon_fallback(true, kind).unwrap();
            assert_eq!((w, h), (16, 16));
            assert_eq!(pixels.len(), 16 * 16 * 4);
            assert!(pixels.chunks_exact(4).any(|p| p[3] == 255));
            assert!(pixels.chunks_exact(4).any(|p| p[3] == 0));
        }
        let absent = sdk_status_icon_fallback(true, SdkStatusIconFallback::Absent).unwrap();
        let refused = sdk_status_icon_fallback(true, SdkStatusIconFallback::Unavailable).unwrap();
        assert_ne!(absent, refused);
        assert_eq!(absent.0[(6 * 16 + 7) * 4 + 3], 0);
        assert_eq!(refused.0[(6 * 16 + 7) * 4 + 3], 255);
    }
    #[test]
    fn sdk_status_bitmap_never_decodes_wire_paths_and_native_v2_retains_bounded_relative_icons() {
        let dir = tempfile::tempdir().unwrap();
        let desktop = Arc::new(crate::host::TestHost {
            allow_interaction: true,
            ..Default::default()
        });
        let ctx = CoreContext::new(
            crate::common::CorePaths {
                data_dir: dir.path().into(),
                config_dir: dir.path().into(),
                resource_dir: dir.path().into(),
                temp_dir: dir.path().into(),
                home_dir: dir.path().into(),
            },
            Some(desktop.clone()),
        );
        let params = json!({"items":[],"icon":"/unowned/does-not-exist.png"});
        let pixels = (vec![12, 34, 56, 255], 1, 1);
        set_sdk_status_item(&ctx, "sdk", &params, Arc::new(|_| {}), Some(pixels.clone())).unwrap();
        assert_eq!(
            desktop.status_specs.lock().unwrap()["sdk"].icon,
            Some(pixels)
        );
        assert!(set_sdk_status_item(
            &ctx,
            "sdk",
            &params,
            Arc::new(|_| {}),
            Some((vec![], 129, 1))
        )
        .is_err());
        image::RgbaImage::from_pixel(1, 1, image::Rgba([12, 34, 56, 255]))
            .save(dir.path().join("icon.png"))
            .unwrap();
        set_status_item(
            &ctx,
            "v2",
            dir.path(),
            &json!({"items":[],"icon":"icon.png"}),
            Arc::new(|_| {}),
        )
        .unwrap();
        assert_eq!(
            desktop.status_specs.lock().unwrap()["v2"].icon,
            Some((vec![12, 34, 56, 255], 1, 1))
        );
        image::RgbaImage::from_pixel(513, 1, image::Rgba([0, 0, 0, 255]))
            .save(dir.path().join("large.png"))
            .unwrap();
        assert!(set_status_item(
            &ctx,
            "v2",
            dir.path(),
            &json!({"items":[],"icon":"large.png"}),
            Arc::new(|_| {})
        )
        .is_err());
        assert!(set_status_item(
            &ctx,
            "v2",
            dir.path(),
            &json!({"items":[],"icon":"../outside.png"}),
            Arc::new(|_| {})
        )
        .is_err());
    }
    #[tokio::test]
    async fn scoped_status_item_limits_update_callbacks_and_cleanup_cross_host_port() {
        let dir = tempfile::tempdir().unwrap();
        let desktop = Arc::new(crate::host::TestHost {
            allow_interaction: true,
            ..Default::default()
        });
        let ctx = CoreContext::new(
            crate::common::CorePaths {
                data_dir: dir.path().into(),
                config_dir: dir.path().into(),
                resource_dir: dir.path().into(),
                temp_dir: dir.path().into(),
                home_dir: dir.path().into(),
            },
            Some(desktop.clone()),
        );
        // Use the actual validated manifest schema rather than guessed capabilities.
        let manifest = super::super::v2_catalog::source()
            .into_iter()
            .find(|m| m["slug"] == "perplexity")
            .unwrap();
        let host = Host::new(ctx, &manifest).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let received = calls.clone();
        let cb: crate::host::StatusCallback = Arc::new(move |s| received.lock().unwrap().push(s));
        for i in 0..128 {
            host.set_status_item(
                dir.path(),
                json!({"resourceId":format!("r{i}"),"items":[{"id":"go","title":"Go"}]}),
                cb.clone(),
            )
            .unwrap();
        }
        assert!(host
            .set_status_item(
                dir.path(),
                json!({"resourceId":"overflow","items":[]}),
                cb.clone()
            )
            .unwrap_err()
            .contains("Too many tray resources"));
        host.set_status_item(dir.path(),json!({"resourceId":"r0","title":"Updated","items":[{"id":"go","title":"Go","checked":true}]}),cb.clone()).unwrap();
        let id = host.scoped("r0").unwrap();
        desktop.status.lock().unwrap()[&id]("go".into());
        assert_eq!(*calls.lock().unwrap(), ["go"]);
        assert!(host
            .set_status_item(
                dir.path(),
                json!({"resourceId":"r0","items":vec![json!({"id":"x","title":"X"});129]}),
                cb.clone()
            )
            .unwrap_err()
            .contains("Too many tray items"));
        assert!(host
            .set_status_item(
                dir.path(),
                json!({"resourceId":"r0","items":[{"id":"x","title":"X"},{"id":"x","title":"X"}]}),
                cb.clone()
            )
            .unwrap_err()
            .contains("Duplicate tray action"));
        assert!(host
            .set_status_item(dir.path(), json!({"resourceId":"../escape","items":[]}), cb)
            .is_err());
        host.cleanup().await;
        assert!(desktop.status.lock().unwrap().is_empty());
    }
}

/// Shared native-V2 and SDK status execution boundary. The caller supplies its
/// already authenticated scope; wire payloads never choose foreign identity.
pub(crate) fn set_status_item(
    ctx: &CoreContext,
    scoped: &str,
    package: &std::path::Path,
    params: &Value,
    callback: crate::host::StatusCallback,
) -> Result<(), String> {
    set_status_item_impl(ctx, scoped, Some(package), params, callback, None)
}
#[derive(Clone, Copy)]
pub(crate) enum SdkStatusIconFallback {
    Absent,
    Unavailable,
}
/// Windows does not display tray titles. A neutral circle marks an extension
/// with no declared icon; the same circle plus an exclamation marks a refused
/// declaration. These reuse the existing Circle/exclamation symbol semantics,
/// not a fabricated replacement for the declared icon. Fixed 1 KiB RGBA only:
/// no file, font, network, decoder or independent admission service.
/// The explicit platform argument lets portable tests exercise both policies.
pub(crate) fn sdk_status_icon_fallback(
    windows: bool,
    kind: SdkStatusIconFallback,
) -> Option<crate::host::StatusIcon> {
    if !windows {
        return None;
    }
    let mut pixels = vec![0; 16 * 16 * 4];
    for y in 0..16 {
        for x in 0..16 {
            let dx = x as f32 - 7.5;
            let dy = y as f32 - 7.5;
            let r2 = dx * dx + dy * dy;
            let ring = (5.5 * 5.5..=7.5 * 7.5).contains(&r2);
            let warning = matches!(kind, SdkStatusIconFallback::Unavailable)
                && (7..=8).contains(&x)
                && ((4..=8).contains(&y) || y == 11);
            if ring || warning {
                let i = (y * 16 + x) * 4;
                pixels[i..i + 4].copy_from_slice(&[128, 128, 128, 255]);
            }
        }
    }
    Some((pixels, 16, 16))
}
/// SDK bitmaps arrive from the native host's bounded, canonical asset capability.
/// SDK wire paths never enter the legacy native-V2 relative-path decoder.
pub(crate) fn set_sdk_status_item(
    ctx: &CoreContext,
    scoped: &str,
    params: &Value,
    callback: crate::host::StatusCallback,
    icon: Option<crate::host::StatusIcon>,
) -> Result<(), String> {
    if let Some((pixels, w, h)) = &icon {
        if *w == 0
            || *h == 0
            || *w > 128
            || *h > 128
            || pixels.len() != *w as usize * *h as usize * 4
        {
            return Err("Invalid SDK status bitmap".into());
        }
    }
    set_status_item_impl(ctx, scoped, None, params, callback, icon)
}
fn set_status_item_impl(
    ctx: &CoreContext,
    scoped: &str,
    package: Option<&std::path::Path>,
    params: &Value,
    callback: crate::host::StatusCallback,
    mut icon: Option<crate::host::StatusIcon>,
) -> Result<(), String> {
    let items = params["items"].as_array().ok_or("Tray items required")?;
    if items.len() > 128 {
        return Err("Too many tray items".into());
    }
    fn validate(
        items: &[Value],
        depth: usize,
        count: &mut usize,
        ids: &mut HashSet<String>,
    ) -> Result<(), String> {
        if depth > 8 {
            return Err("Tray nesting limit".into());
        }
        for item in items {
            *count += 1;
            if *count > 128 {
                return Err("Too many tray items".into());
            }
            let id = string(item, "id")?;
            process_install::component(id)?;
            if !ids.insert(id.into()) {
                return Err("Duplicate tray action".into());
            }
            let _ = string(item, "title")?;
            match item["kind"].as_str() {
                None | Some("section" | "separator") => {}
                Some("submenu") => validate(
                    item["items"].as_array().ok_or("Submenu items required")?,
                    depth + 1,
                    count,
                    ids,
                )?,
                _ => return Err("Unknown tray item kind".into()),
            }
        }
        Ok(())
    }
    validate(items, 0, &mut 0, &mut HashSet::new())?;
    if let (Some(package), Some(path)) = (package, params["icon"].as_str()) {
        use std::io::Read;
        let root = std::fs::canonicalize(package).map_err(|e| e.to_string())?;
        let path = std::fs::canonicalize(root.join(process_install::relative(path)?))
            .map_err(|e| e.to_string())?;
        if !path.starts_with(&root) {
            return Err("Tray image is outside package".into());
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(crate::assets::ASSET_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > crate::assets::ASSET_BYTES {
            return Err("Tray image too large".into());
        }
        let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(512);
        limits.max_image_height = Some(512);
        limits.max_alloc = Some(2 * 1024 * 1024);
        reader.limits(limits);
        let image = reader.decode().map_err(|e| e.to_string())?.into_rgba8();
        let (w, h) = image.dimensions();
        icon = Some((image.into_raw(), w, h));
    }
    ctx.host.status_items().set(
        scoped,
        crate::host::StatusItem {
            title: params["title"].as_str().unwrap_or("").into(),
            tooltip: params["tooltip"].as_str().unwrap_or("").into(),
            items: items.clone(),
            icon,
        },
        callback,
    )?;
    Ok(())
}
