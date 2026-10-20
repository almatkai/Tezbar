use super::{
    db::Database,
    stores::{document, read_or},
    text,
};
use crate::native_core::common::{now_ms, CoreContext, CoreResult};
use notify::Watcher;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
pub async fn output(program: &str, args: &[&str], timeout: u64) -> Result<String, String> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args).kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let out = tokio::time::timeout(std::time::Duration::from_secs(timeout), cmd.output())
        .await
        .map_err(|_| format!("{program} timed out"))?
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
pub async fn bundle_id(path: &str) -> Value {
    #[cfg(target_os = "macos")]
    {
        let path = Path::new(path).join("Contents/Info.plist");
        if let Ok(s) = output(
            "/usr/bin/plutil",
            &[
                "-extract",
                "CFBundleIdentifier",
                "raw",
                "-o",
                "-",
                &path.to_string_lossy(),
            ],
            2,
        )
        .await
        {
            let s = s.trim();
            if !s.is_empty() {
                return json!(s);
            }
        }
    }
    let _ = path;
    Value::Null
}
pub async fn applications(ctx: &CoreContext) -> CoreResult {
    let home = ctx.paths.home_dir.clone();
    let mut apps = tokio::task::spawn_blocking(move || {
        let mut out = vec![];
        let mut seen = HashSet::new();
        #[cfg(target_os = "macos")]
        let roots = vec![
            PathBuf::from("/Applications"),
            PathBuf::from("/Applications/Utilities"),
            PathBuf::from("/System/Applications"),
            PathBuf::from("/System/Applications/Utilities"),
            PathBuf::from("/System/Library/CoreServices/Applications"),
            PathBuf::from("/System/Library/CoreServices"),
            home.join("Applications"),
        ];
        #[cfg(target_os = "windows")]
        let roots = vec![
            PathBuf::from(std::env::var("ProgramData").unwrap_or("C:\\ProgramData".into()))
                .join("Microsoft/Windows/Start Menu/Programs"),
            PathBuf::from(
                std::env::var("APPDATA")
                    .unwrap_or_else(|_| home.join("AppData/Roaming").to_string_lossy().into()),
            )
            .join("Microsoft/Windows/Start Menu/Programs"),
        ];
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let roots = vec![
            home.join(".local/share/applications"),
            PathBuf::from("/usr/share/applications"),
        ];
        for root in roots {
            for entry in walkdir::WalkDir::new(root)
                .max_depth(if cfg!(windows) { 12 } else { 1 })
                .into_iter()
                .filter_map(Result::ok)
            {
                let path = entry.path();
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                if !(if cfg!(target_os = "macos") {
                    ext == "app"
                } else if cfg!(windows) {
                    ext.eq_ignore_ascii_case("lnk")
                } else {
                    ext == "desktop"
                }) {
                    continue;
                }
                let name = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                if seen.insert(name.to_lowercase()) {
                    out.push(json!({"name":name,"path":path}));
                }
            }
        }
        out
    })
    .await
    .map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    if let Ok(s) = output(
        "powershell.exe",
        &[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-StartApps | Select-Object Name,AppID | ConvertTo-Json -Compress",
        ],
        8,
    )
    .await
    {
        let _ = merge_windows_start_apps(&mut apps, &s);
    }
    #[cfg(target_os = "macos")]
    for app in &mut apps {
        let id = bundle_id(app["path"].as_str().unwrap_or("")).await;
        if !id.is_null() {
            app["bundleId"] = id;
        }
    }
    apps.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(json!(apps))
}
pub fn app_documents(apps: &[Value]) -> Vec<Value> {
    apps.iter()
        .filter_map(|a| {
            let name = a["name"].as_str()?;
            let path = a["path"].as_str()?;
            let mut d = document(
                &format!("app:{path}"),
                "applications",
                name,
                path,
                &format!("{name} {path} {}", text::aliases(name).join(" ")),
                json!({"type":"open-app","appName":name,"appPath":path}),
                now_ms(),
            );
            d["sourcePath"] = json!(path);
            Some(d)
        })
        .collect()
}
pub fn command_documents(native: &[Value]) -> Vec<Value> {
    let mut out = vec![];
    for d in native {
        let Some(id) = d["id"].as_str() else { continue };
        if [
            "list-listening-ports",
            "open-emoji-picker",
            "open-quick-notes",
        ]
        .contains(&id)
        {
            continue;
        }
        out.push(document(
            &format!("native:{id}"),
            "native-command",
            d["title"].as_str().unwrap_or(id),
            d["subtitle"].as_str().unwrap_or(""),
            &format!(
                "{} {} {} {}",
                d["title"].as_str().unwrap_or(""),
                d["subtitle"].as_str().unwrap_or(""),
                d["category"].as_str().unwrap_or(""),
                d["keywords"]
                    .as_array()
                    .map(|a| a
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" "))
                    .unwrap_or_default()
            ),
            json!({"type":"run-native-command","commandId":id}),
            now_ms(),
        ));
    }
    for (id, title, subtitle, keywords) in [
        (
            "open-settings",
            "Open Settings",
            "Tezbar settings",
            "settings preferences /settings",
        ),
        (
            "open-indexing",
            "Indexing Status",
            "Knowledge index · progress, storage, and indexed files",
            "indexing index knowledge status progress files storage cache /indexing",
        ),
        (
            "open-extensions-settings",
            "Extensions",
            "Settings · Extensions tab",
            "extensions raycast /extensions",
        ),
        (
            "open-extensions",
            if cfg!(windows) {
                "Loved Extensions"
            } else {
                "Extensions Store"
            },
            if cfg!(windows) {
                "Browse Tezbar-recommended GitHub extensions"
            } else {
                "Browse and install extensions"
            },
            "store extension store extensions store raycast store /store",
        ),
        (
            "open-snippets",
            "Open Snippets",
            "Tezbar snippets",
            "snippets text snippets /snippets",
        ),
        (
            "open-notes",
            "Open Notes",
            "Tezbar quick notes",
            "notes quick notes /notes",
        ),
        (
            "start-onboarding",
            "Start Onboarding",
            "Walk through Tezbar setup again",
            "onboarding setup welcome tour get started intro /onboarding",
        ),
        (
            "open-emoji-picker",
            "Open Emoji Picker",
            "Tezbar emoji picker",
            "emoji symbols /emoji",
        ),
        (
            "check-for-updates",
            "Check for Updates",
            "Check for new Tezbar updates",
            "update updates check for updates upgrade version обновление обновы /update",
        ),
    ] {
        out.push(document(
            &format!("command:{id}"),
            "commands",
            title,
            subtitle,
            &format!("{title} {subtitle} {keywords}"),
            json!({"type":"invoke-command","commandId":id}),
            now_ms(),
        ));
    }
    out
}
pub fn extension_documents(commands: &[Value], config: &Value) -> Vec<Value> {
    commands.iter().filter_map(|c|{
 let ext=c["extensionId"].as_str().map(str::to_owned).or_else(||c["extName"].as_str().map(|s|if s.starts_with("raycast."){s.to_owned()}else{format!("raycast.{s}")}))?;
 let name=c["commandName"].as_str().or_else(||c["cmdName"].as_str()).or_else(||c["name"].as_str())?;
 let title=c["title"].as_str().unwrap_or(name);let id=format!("extcmd:{ext}:{name}");if config["disabledCommands"][&id]==true{return None}
 let subtitle=c["extensionName"].as_str().or_else(||c["extensionTitle"].as_str()).unwrap_or(&ext);
 let keywords=c["keywords"].as_array().map(|a|a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" ")).unwrap_or_default();
 let mut d=document(&id,"extensions",title,subtitle,&format!("{title} {name} {ext} {subtitle} {} {keywords} {}",c["description"].as_str().unwrap_or(""),config["commandAliases"][&id].as_str().unwrap_or("")),json!({"type":"run-extension-command","extensionId":ext,"commandName":name,"title":title,"iconPath":c["iconPath"],"commandArgumentDefinitions":c.get("commandArgumentDefinitions").unwrap_or(&c["argumentDefinitions"])}),c["installedAt"].as_u64().unwrap_or(now_ms()));d["popularity"]=c["downloadCount"].clone();Some(d)
}).collect()
}
const SKIP: &[&str] = &[
    "node_modules",
    ".git",
    ".next",
    ".cache",
    "Library",
    "build",
    "coverage",
    "dist",
    "out",
    "target",
];
fn allowed(path: &Path) -> bool {
    !path
        .components()
        .any(|c| SKIP.contains(&c.as_os_str().to_string_lossy().as_ref()))
        && [
            "", "md", "txt", "json", "yaml", "yml", "ts", "tsx", "js", "jsx", "go", "py", "rs",
            "swift", "pdf", "png", "jpg",
        ]
        .contains(
            &path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
                .as_str(),
        )
}
pub fn file_document(path: &Path) -> Option<Value> {
    if !allowed(path) {
        return None;
    }
    let m = path.metadata().ok()?;
    if !m.is_file() {
        return None;
    }
    let p = path.to_string_lossy();
    let title = path.file_name()?.to_string_lossy();
    let mtime = m
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    let mut d = document(
        &format!("file:{p}"),
        "files",
        &title,
        &p,
        &format!("{title} {p}"),
        json!({"type":"open-file","path":p}),
        mtime,
    );
    d["sourcePath"] = json!(p);
    d["sourceMtime"] = json!(mtime);
    Some(d)
}
pub fn roots(ctx: &CoreContext) -> Vec<PathBuf> {
    ["Desktop", "Documents", "Downloads", "Pictures"]
        .iter()
        .map(|p| ctx.paths.home_dir.join(p))
        .collect()
}
pub async fn scan(ctx: CoreContext, db: Database, stop: Arc<AtomicBool>) -> Result<(), String> {
    let scan_roots = roots(&ctx);
    let (tx, mut rx) = tokio::sync::mpsc::channel(2);
    let scanning = tokio::task::spawn_blocking(move || {
        let mut batch = vec![];
        let mut count = 0;
        let mut visited = 0;
        'scan: for root in scan_roots {
            for entry in walkdir::WalkDir::new(root)
                .follow_links(false)
                .into_iter()
                .filter_entry(|e| !SKIP.contains(&e.file_name().to_string_lossy().as_ref()))
                .filter_map(Result::ok)
            {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                visited += 1;
                if visited > 300_000 {
                    break 'scan;
                }
                if !entry.file_type().is_file() {
                    continue;
                }
                if let Some(d) = file_document(entry.path()) {
                    batch.push(d);
                    count += 1;
                }
                if batch.len() >= 400 {
                    if tx.blocking_send(std::mem::take(&mut batch)).is_err() {
                        return;
                    }
                }
                if count >= 75000 {
                    break;
                }
            }
            if count >= 75000 {
                break;
            }
        }
        if !batch.is_empty() {
            let _ = tx.blocking_send(batch);
        }
    });
    while let Some(batch) = rx.recv().await {
        while ctx.foreground_active() {
            tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        }
        db.upsert(batch, vec![]).await?;
    }
    scanning.await.map_err(|e| e.to_string())?;
    let candidates=db.call(|c|{let mut stmt=c.prepare("SELECT id,source_path FROM documents WHERE category='files' AND source_path IS NOT NULL LIMIT 75000").map_err(|e|e.to_string())?;let rows=stmt.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;Ok(json!(rows))}).await?;
    let managed = roots(&ctx);
    let deleted = tokio::task::spawn_blocking(move || {
        candidates
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| {
                let id = v[0].as_str()?;
                let p = Path::new(v[1].as_str()?);
                if managed.iter().any(|root| p.starts_with(root))
                    && matches!(p.try_exists(), Ok(false))
                {
                    Some(id.to_owned())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| e.to_string())?;
    db.call(move |c| {
        let tx = c.transaction().map_err(|e| e.to_string())?;
        for id in deleted {
            tx.execute("DELETE FROM documents WHERE id=?", [id])
                .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(Value::Null)
    })
    .await?;
    Ok(())
}
pub async fn watch(
    ctx: CoreContext,
    db: Database,
    stop: Arc<AtomicBool>,
    scanning: Arc<tokio::sync::Mutex<()>>,
) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<notify::Result<notify::Event>>(256);
    let overflow = Arc::new(AtomicBool::new(false));
    let dirty = overflow.clone();
    let mut watcher = match notify::recommended_watcher(move |e: notify::Result<notify::Event>| {
        if e.as_ref()
            .is_ok_and(|e| matches!(e.kind, notify::EventKind::Access(_)))
        {
            return;
        }
        if tx.try_send(e).is_err() {
            dirty.store(true, Ordering::Release);
        }
    }) {
        Ok(w) => w,
        Err(e) => {
            log::warn!("File watcher: {e}");
            return;
        }
    };
    for root in roots(&ctx) {
        if let Err(e) = watcher.watch(&root, notify::RecursiveMode::Recursive) {
            log::debug!("Watch {}: {e}", root.display());
        }
    }
    while !stop.load(Ordering::Relaxed) {
        if overflow.swap(false, Ordering::AcqRel) {
            let _guard = scanning.lock().await;
            if let Err(e) = scan(ctx.clone(), db.clone(), stop.clone()).await {
                log::warn!("File watcher rescan: {e}");
            }
        }
        let event = tokio::select! {v=rx.recv()=>v,_=tokio::time::sleep(std::time::Duration::from_millis(500))=>continue};
        let event = match event {
            Some(Ok(e)) => e,
            Some(Err(e)) => {
                log::debug!("File watch event: {e}");
                overflow.store(true, Ordering::Release);
                continue;
            }
            None => return,
        };
        let mut scan_dirs = matches!(
            event.kind,
            notify::EventKind::Create(_)
                | notify::EventKind::Modify(notify::event::ModifyKind::Name(_))
        );
        let mut paths = event.paths;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        while let Ok(Ok(e)) = rx.try_recv() {
            scan_dirs |= matches!(
                e.kind,
                notify::EventKind::Create(_)
                    | notify::EventKind::Modify(notify::event::ModifyKind::Name(_))
            );
            paths.extend(e.paths);
        }
        paths.sort();
        paths.dedup();
        let _scan_guard = scanning.lock().await;
        let (upserts, deleted) = match tokio::task::spawn_blocking(move || {
            let mut docs = vec![];
            let mut deleted = vec![];
            for p in paths {
                if scan_dirs && p.is_dir() {
                    for e in walkdir::WalkDir::new(&p)
                        .follow_links(false)
                        .into_iter()
                        .filter_entry(|e| !SKIP.contains(&e.file_name().to_string_lossy().as_ref()))
                        .filter_map(Result::ok)
                        .take(75000)
                    {
                        if docs.len() >= 75000 {
                            break;
                        }
                        if e.file_type().is_file() {
                            if let Some(d) = file_document(e.path()) {
                                docs.push(d)
                            }
                        }
                    }
                    continue;
                }
                if let Some(d) = file_document(&p) {
                    docs.push(d)
                } else if !p.exists() {
                    deleted.push(format!("file:{}", p.display()))
                }
            }
            (docs, deleted)
        })
        .await
        {
            Ok(v) => v,
            Err(_) => continue,
        };
        let _ = db.upsert(upserts, vec![]).await;
        let _ = db
            .call(move |c| {
                let tx = c.transaction().map_err(|e| e.to_string())?;
                for id in deleted {
                    tx.execute("DELETE FROM documents WHERE id=?1 OR (category='files' AND source_path>=?2 AND source_path<?3)",rusqlite::params![id,format!("{}{}",id.trim_start_matches("file:"),std::path::MAIN_SEPARATOR),format!("{}{}",id.trim_start_matches("file:"),char::from_u32(std::path::MAIN_SEPARATOR as u32+1).unwrap())]).map_err(|e|e.to_string())?;
                }
                tx.commit().map_err(|e| e.to_string())?;
                Ok(Value::Null)
            })
            .await;
    }
}
pub fn resolve(home: &Path, input: &str) -> PathBuf {
    let input = input
        .trim()
        .strip_suffix("/...")
        .or_else(|| input.trim().strip_suffix("\\..."))
        .map(|s| format!("{s}{}", std::path::MAIN_SEPARATOR))
        .unwrap_or_else(|| input.trim().into());
    if input == "~" || input == "/" || input.is_empty() {
        return home.into();
    }
    if let Some(s) = input.strip_prefix("~/") {
        return home.join(s);
    }
    if input.starts_with('/')
        && ![
            "/Users/",
            "/Volumes/",
            "/private/",
            "/tmp/",
            "/var/",
            "/System/",
            "/Library/",
        ]
        .iter()
        .any(|p| input.starts_with(p))
        && input != "/Users"
        && input != "/Volumes"
    {
        return home.join(input.trim_start_matches('/'));
    }
    let p = PathBuf::from(input);
    if p.is_absolute() {
        p
    } else {
        home.join(p)
    }
}
fn display(home: &Path, path: &Path) -> String {
    path.strip_prefix(home)
        .map(|p| {
            if p.as_os_str().is_empty() {
                "~".into()
            } else {
                format!("~/{}", p.display())
            }
        })
        .unwrap_or_else(|_| path.to_string_lossy().into())
}
fn slash(home: &Path, path: &Path, dir: bool) -> String {
    let mut s = path
        .strip_prefix(home)
        .map(|p| format!("/{}", p.display()))
        .unwrap_or_else(|_| path.to_string_lossy().into());
    if dir && !s.ends_with('/') {
        s.push('/')
    }
    s.replace('\\', "/")
}
pub fn usage_keys(path: &Path) -> Vec<String> {
    let p = path.to_string_lossy();
    let parent = path.parent().unwrap_or(path).to_string_lossy();
    if path.is_dir() {
        vec![format!("folder:{p}"), format!("sibling-folder:{parent}")]
    } else {
        let mut keys = vec![format!("parent:{parent}")];
        if let Some(ext) = path.extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            keys.push(format!("parent-ext:{parent}:.{ext}"));
            keys.push(format!("ext:.{ext}"));
        }
        keys
    }
}
pub fn complete(ctx: &CoreContext, query: &str, apps: &[Value]) -> CoreResult {
    let home = &ctx.paths.home_dir;
    let launchpad = query.trim_start().starts_with('`');
    let mut target = resolve(home, query);
    let mut term = if launchpad {
        query.trim_start().trim_start_matches('`').trim().to_owned()
    } else {
        String::new()
    };
    let mut app_mode = false;
    if query.ends_with(' ') && target.exists() {
        app_mode = true
    } else if !target.exists() {
        for (i, c) in query.char_indices().rev() {
            if c == ' ' {
                let before = resolve(home, &query[..i]);
                if before.exists() {
                    target = before;
                    term = query[i + 1..].trim().into();
                    app_mode = true;
                    break;
                }
            }
        }
    }
    let applications_directory = target == Path::new("/Applications")
        || target == Path::new("/System/Applications")
        || target == Path::new("/System/Applications/Utilities")
        || target == home.join("Applications");
    if launchpad || app_mode || applications_directory {
        let usage = read_or(
            &ctx.paths.data_dir.join("open-with-usage.json"),
            json!({"version":1,"keys":{},"aliases":{}}),
        )?;
        let alias = app_term(&term);
        let mut matching: Vec<_> = apps
            .iter()
            .filter(|app| {
                let name = app["name"].as_str().unwrap_or("");
                app_matches(name, &term) || usage["aliases"][&alias][name].is_object()
            })
            .collect();
        matching.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        if !app_mode {
            if !launchpad {
                matching.truncate(50)
            }
            return Ok(json!(matching.into_iter().enumerate().map(|(i,app)|{let path=app["path"].as_str().unwrap_or("");json!({"id":format!("path-installed-app:{path}"),"title":app["name"],"subtitle":display(home,Path::new(path)),"kind":"application","badge":"Application","value":path,"path":path,"appPath":path,"appName":app["name"],"applicationAction":"open","score":2000-i as i64})}).collect::<Vec<_>>()));
        }
        let mut recommendations: Vec<(String, f64, u64)> = vec![];
        for app in &matching {
            let name = app["name"].as_str().unwrap_or("");
            let mut score = 0.;
            for (i, key) in usage_keys(&target).iter().enumerate() {
                let entry = &usage["keys"][key][name];
                let count = entry["count"].as_f64().unwrap_or(0.);
                if count > 0. {
                    let age = now_ms().saturating_sub(entry["lastUsedAt"].as_u64().unwrap_or(0))
                        as f64
                        / 86_400_000.;
                    score += count
                        * if i == 0 {
                            5.
                        } else if i == 1 {
                            3.
                        } else {
                            1.
                        }
                        + (14. - age).max(0.);
                }
            }
            let learned = &usage["aliases"][&alias][name];
            let alias_count = learned["count"].as_u64().unwrap_or(0);
            if score > 0. || alias_count > 0 {
                recommendations.push((
                    name.to_owned(),
                    if alias_count > 0 {
                        1_000_000. + alias_count as f64
                    } else {
                        score
                    },
                    learned["lastUsedAt"].as_u64().unwrap_or(0),
                ));
            }
        }
        recommendations.sort_by(|a, b| {
            b.1.total_cmp(&a.1)
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| a.0.cmp(&b.0))
        });
        recommendations.truncate(5);
        let make_item = |app: &Value, index: usize, recommended: bool| {
            let path = app["path"].as_str().unwrap_or("");
            let name = app["name"].as_str().unwrap_or("");
            let section = if recommended {
                "recommended"
            } else {
                "applications"
            };
            json!({"id":format!("path-app:{section}:{path}"),"title":name,"subtitle":format!("Open {} with {name}",display(home,&target)),"kind":"application","section":section,"badge":if recommended{"Recommended"}else{"Open With"},"value":format!("{} {name}",slash(home,&target,false)),"path":target,"appPath":path,"appName":name,"applicationAction":"open-with","score":if recommended{4000-index as i64}else{1000-index as i64}})
        };
        let mut out = vec![];
        for (name, _, _) in &recommendations {
            if let Some(app) = matching.iter().find(|a| a["name"] == name.as_str()) {
                out.push(make_item(app, out.len(), true));
            }
        }
        let default = json!({"id":format!("path-default:{}",target.display()),"title":format!("Open in {}",default_app(&target)),"subtitle":format!("Open {}",display(home,&target)),"kind":"application","section":"default","badge":"Default","value":format!("{} ",slash(home,&target,false)),"path":target,"applicationAction":"open-with","score":2000});
        let recommended_len = out.len();
        if term.trim().is_empty() {
            out.push(default.clone());
        }
        for app in matching {
            if out.len() >= 49 {
                break;
            }
            let name = app["name"].as_str().unwrap_or("");
            if !recommendations.iter().any(|(n, _, _)| n == name) {
                out.push(make_item(
                    app,
                    out.len() - recommended_len - usize::from(term.trim().is_empty()),
                    false,
                ));
            }
        }
        if !term.trim().is_empty() {
            out.push(default);
        }
        return Ok(json!(out));
    }
    let (directory, prefix) = if target.is_dir() {
        (target, String::new())
    } else {
        (
            target.parent().unwrap_or(home).into(),
            target
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase(),
        )
    };
    let mut out = vec![];
    if query.trim() == "/" {
        let visits = read_or(
            &ctx.paths.data_dir.join("directory-visits.json"),
            json!({"version":1,"visits":{}}),
        )?;
        for (i, (p, _score)) in recommendations(&visits["visits"], home)
            .into_iter()
            .take(5)
            .enumerate()
        {
            let path = Path::new(&p);
            out.push(json!({"id":format!("path-recommended:{p}"),"title":path.file_name().unwrap_or_default().to_string_lossy(),"subtitle":display(home,path),"kind":"directory","section":"recommended","badge":"Recommended","value":slash(home,path,true),"path":p,"score":5000-i as i64}));
        }
    }
    if let Ok(entries) = std::fs::read_dir(&directory) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || !name.to_lowercase().contains(&prefix) {
                continue;
            }
            let path = e.path();
            let Ok(kind) = e.file_type() else { continue };
            if !kind.is_dir() && !kind.is_file() {
                continue;
            }
            if out.iter().any(|i| i["path"].as_str() == path.to_str()) {
                continue;
            }
            out.push(json!({"id":format!("path:{}",path.display()),"title":name,"subtitle":display(home,&path),"kind":if kind.is_dir(){"directory"}else{"file"},"value":slash(home,&path,kind.is_dir()),"path":path,"score":(if kind.is_dir(){1000}else{500})+if name.to_lowercase()==prefix{1000}else if name.to_lowercase().starts_with(&prefix){100}else{0}}));
        }
    }
    out.sort_by(|a, b| {
        b["score"]
            .as_i64()
            .cmp(&a["score"].as_i64())
            .then_with(|| a["title"].as_str().cmp(&b["title"].as_str()))
    });
    out.truncate(50);
    Ok(json!(out))
}
pub fn recommendations(visits: &Value, home: &Path) -> Vec<(String, f64)> {
    let Some(visits) = visits.as_object() else {
        return vec![];
    };
    let valid: Vec<_> = visits
        .iter()
        .filter(|(p, v)| {
            Path::new(p).is_absolute()
                && v["count"].as_u64().unwrap_or(0) > 0
                && v["lastVisitedAt"].is_number()
        })
        .collect();
    let mut siblings = std::collections::HashMap::<String, usize>::new();
    for (p, _) in &valid {
        if let Some(parent) = Path::new(p).parent() {
            *siblings.entry(parent.to_string_lossy().into()).or_default() += 1
        }
    }
    let mut merged = std::collections::HashMap::<String, f64>::new();
    for (p, v) in valid {
        let parent = Path::new(p)
            .parent()
            .unwrap_or(Path::new(p))
            .to_string_lossy();
        let path = if siblings.get(parent.as_ref()).copied().unwrap_or(0) >= 3
            && Path::new(parent.as_ref()) != home
        {
            parent.into_owned()
        } else {
            p.clone()
        };
        if Path::new(&path) == home || !Path::new(&path).is_dir() {
            continue;
        }
        let age =
            now_ms().saturating_sub(v["lastVisitedAt"].as_u64().unwrap_or(0)) as f64 / 86_400_000.;
        *merged.entry(path).or_default() +=
            v["count"].as_f64().unwrap_or(0.) * 10. + (14. - age).max(0.);
    }
    let mut ranked: Vec<_> = merged.into_iter().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut out: Vec<(String, f64)> = vec![];
    for (p, s) in ranked {
        if !out.iter().any(|(strong, _)| {
            p.starts_with(&format!("{strong}/")) || strong.starts_with(&format!("{p}/"))
        }) {
            out.push((p, s))
        }
    }
    out
}

pub fn app_term(s: &str) -> String {
    s.to_lowercase()
        .replace('&', "and")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}
fn app_matches(name: &str, term: &str) -> bool {
    let term = app_term(term);
    if term.is_empty() {
        return true;
    }
    let acronym: String = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter_map(|s| s.chars().next())
        .collect::<String>()
        .to_lowercase();
    app_term(name).contains(&term)
        || acronym.contains(&term)
        || text::aliases(name)
            .iter()
            .any(|a| app_term(a).contains(&term))
}
fn default_app(p: &Path) -> &'static str {
    if p.is_dir() {
        return if cfg!(windows) {
            "File Explorer"
        } else {
            "Finder"
        };
    }
    let ext = p
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if [
        "png", "jpg", "jpeg", "gif", "heic", "webp", "tiff", "bmp", "pdf",
    ]
    .contains(&ext.as_str())
    {
        return "Preview";
    }
    if ["mov", "mp4", "m4v", "avi", "mkv", "m2ts", "mts"].contains(&ext.as_str())
        || (ext == "ts"
            && regex::Regex::new(r"(?i)\b(movie|movies|video|videos)\b")
                .unwrap()
                .is_match(&p.parent().unwrap_or(p).to_string_lossy()))
    {
        return "QuickTime Player";
    }
    "Default App"
}

#[cfg(any(windows, test))]
pub fn merge_windows_start_apps(apps: &mut Vec<Value>, raw: &str) -> Result<(), String> {
    let v: Value = serde_json::from_str(raw).map_err(|e| e.to_string())?;
    let items = v.as_array().cloned().unwrap_or_else(|| vec![v]);
    for item in items {
        if let (Some(name), Some(id)) = (item["Name"].as_str(), item["AppID"].as_str()) {
            let name = name.trim();
            let id = id.trim();
            if !name.is_empty()
                && !id.is_empty()
                && !apps.iter().any(|app| {
                    app["name"]
                        .as_str()
                        .unwrap_or("")
                        .eq_ignore_ascii_case(name)
                })
            {
                apps.push(
                    json!({"name":name,"path":format!("shell:AppsFolder\\{id}"),"windowsAppId":id}),
                );
            }
        }
    }
    Ok(())
}
