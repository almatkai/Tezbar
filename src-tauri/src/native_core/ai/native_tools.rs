//! Native tools use live approval policy. Prompts are not an OS sandbox.
use super::*;
use serde::Deserialize;
use std::{
    io::Read,
    path::{Path, PathBuf},
};
const FILE_LIMIT: u64 = runtime::limits::FILE_BYTES;

pub(super) fn definitions() -> Vec<Value> {
    let string = json!({"type":"string"});
    [
        (if cfg!(windows){"powershell"}else{"bash"},"Execute a shell command in cwd. Complex/unknown/destructive commands require confirmation.",json!({"command":string,"timeoutSeconds":{"type":"integer","minimum":1,"maximum":600}}),vec!["command"]),
        ("read","Read a UTF-8 regular file inside cwd (maximum 2 MiB).",json!({"path":string,"offset":{"type":"integer","minimum":1},"limit":{"type":"integer","minimum":1}}),vec!["path"]),
        ("write","Atomically replace/create a UTF-8 file inside cwd; parent must exist. Always requires approval.",json!({"path":string,"content":string,"createParents":{"type":"boolean"}}),vec!["path","content"]),
        ("edit","Replace exactly one occurrence of oldText in a UTF-8 file inside cwd. Always requires approval.",json!({"path":string,"oldText":string,"newText":string}),vec!["path","oldText","newText"]),
        ("ls","List a directory. Sensitive entries are omitted unless directly approved.",json!({"path":string}),vec!["path"]),
        ("find","Find names in a bounded directory tree.",json!({"path":string,"query":string}),vec!["path"]),
        ("grep","Bounded literal file-content search.",json!({"query":string,"path":string}),vec!["query"]),
        ("screen_capture","Capture the active display. Requires approval except in Turbo.",json!({}),vec![]),
        ("project_search","Narrow literal text search inside cwd; skips dependencies/build/git and symlinks. Bounded, may be incomplete.",json!({"query":string,"path":string}),vec!["query"]),
        ("launcher_search","Search Tezbar local launcher names and metadata.",json!({"query":string}),vec!["query"]),
        ("pc_search","Search user-approved indexed knowledge; cite returned paths/pages.",json!({"query":string}),vec!["query"]),
        ("pc_read","Read nearby indexed knowledge from a pc_search result ID.",json!({"resultId":string}),vec!["resultId"]),
    ].into_iter().map(|(name,description,properties,required)|json!({"type":"function","function":{"name":name,"description":description,"parameters":{"type":"object","properties":properties,"required":required,"additionalProperties":false}}})).collect()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileArgs {
    path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct WriteArgs {
    path: String,
    content: String,
    #[serde(default)]
    create_parents: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct EditArgs {
    path: String,
    old_text: String,
    new_text: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct BashArgs {
    command: String,
    timeout_seconds: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    #[serde(default)]
    query: String,
    path: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryArgs {
    query: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ReadArgs {
    result_id: String,
}
fn parse<T: serde::de::DeserializeOwned>(a: Value) -> Result<T, String> {
    serde_json::from_value(a).map_err(|e| format!("Native arguments: {e}"))
}
fn nonempty(s: &str, limit: usize) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > limit || s.contains('\0') {
        Err("Native arguments: empty, oversized or NUL-containing string".into())
    } else {
        Ok(())
    }
}
fn path(root: &Path, raw: &str, create: bool) -> Result<PathBuf, String> {
    nonempty(raw, 4096)?;
    let candidate = root.join(raw);
    let resolved = if candidate.exists() {
        candidate.canonicalize().map_err(err)?
    } else if create {
        let parent = candidate
            .parent()
            .ok_or("Native path: missing parent")?
            .canonicalize()
            .map_err(err)?;
        parent.join(
            candidate
                .file_name()
                .ok_or("Native path: missing filename")?,
        )
    } else {
        return Err("Native path: does not exist".into());
    };
    // A dangling symlink must not be replaced as if it were an ordinary new file.
    if !candidate.exists() && candidate.symlink_metadata().is_ok() {
        return Err("Native path: dangling symlink".into());
    }
    Ok(resolved)
}
fn read_file(path: &Path) -> Result<String, String> {
    if !path.metadata().map_err(err)?.is_file() {
        return Err("Native file: expected a regular file".into());
    }
    let file = std::fs::File::open(path).map_err(err)?;
    if !file.metadata().map_err(err)?.is_file() {
        return Err("Native file: expected a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(FILE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() as u64 > FILE_LIMIT {
        return Err("Native file: exceeds 2 MiB".into());
    }
    String::from_utf8(bytes).map_err(|_| "Native file: not UTF-8".into())
}
fn replacement(path: &Path, old: &str, new: &str) -> Result<String, String> {
    nonempty(old, FILE_LIMIT as usize)?;
    let text = read_file(path)?;
    // Count overlapping matches too (e.g. 'aa' in 'aaa').
    if text
        .char_indices()
        .filter(|(i, _)| text[*i..].starts_with(old))
        .count()
        != 1
    {
        return Err("Native edit: oldText must occur exactly once (ambiguous or missing)".into());
    }
    let text = text.replacen(old, new, 1);
    if text.len() as u64 > FILE_LIMIT {
        return Err("Native edit: result exceeds 2 MiB".into());
    }
    Ok(text)
}
pub(super) enum Tool {
    Extension(String, Value),
    Bash(String, PathBuf, u64),
    Read(PathBuf, usize, usize),
    Write(PathBuf, String, bool),
    Directory(PathBuf, Option<String>, bool),
    Screen,
    Edit(PathBuf, String, String),
    Search(PathBuf, String),
    Route(&'static str, String),
}
impl Tool {
    pub fn label(&self) -> String {
        match self {
            Self::Extension(name, _) => name.clone(),
            Self::Directory(p, _, _) => format!("list {}", p.display()),
            Self::Screen => "Capture screen".into(),
            Self::Bash(c, ..) => format!("bash: {}", cut(c, 60)),
            Self::Read(p, ..) => format!("read {}", p.display()),
            Self::Write(p, ..) => format!("write {}", p.display()),
            Self::Edit(p, _, _) => format!("edit {} (1 change)", p.display()),
            Self::Search(_, q) => format!("project search: {}", cut(q, 60)),
            Self::Route(r, q) => format!("{r}: {}", cut(q, 60)),
        }
    }
}
pub(super) fn prepare(call: &Value, root: &Path) -> Result<Tool, String> {
    let a: Value = serde_json::from_str(s(&call["function"]["arguments"])).map_err(err)?;
    let tool = match s(&call["function"]["name"]) {
        "bash" | "powershell" => {
            let a: BashArgs = parse(a)?;
            nonempty(&a.command, 16384)?;
            Tool::Bash(
                a.command,
                root.into(),
                a.timeout_seconds.unwrap_or(120).clamp(1, 600),
            )
        }
        "read" => {
            let a: FileArgs = parse(a)?;
            let p = path(root, &a.path, false)?;
            if !p.is_file() {
                return Err("Native file: expected a regular file".into());
            }
            Tool::Read(
                p,
                a.offset.unwrap_or(1).saturating_sub(1),
                a.limit.unwrap_or(usize::MAX),
            )
        }
        "write" => {
            let a: WriteArgs = parse(a)?;
            if a.content.len() as u64 > FILE_LIMIT {
                return Err("Native write: exceeds 2 MiB".into());
            }
            let p = if a.create_parents {
                create_path(root, &a.path)?
            } else {
                path(root, &a.path, true)?
            };
            if p.exists() && !p.is_file() {
                return Err("Native write: expected a regular file".into());
            }
            Tool::Write(p, a.content, a.create_parents)
        }
        "edit" => {
            let a: EditArgs = parse(a)?;
            let p = path(root, &a.path, false)?;
            replacement(&p, &a.old_text, &a.new_text)?;
            Tool::Edit(p, a.old_text, a.new_text)
        }
        "screen_capture" => {
            if a != json!({}) {
                return Err("screen_capture takes no arguments".into());
            }
            Tool::Screen
        }
        "ls" | "find" => {
            let a: SearchArgs = parse(a)?;
            Tool::Directory(
                path(root, a.path.as_deref().unwrap_or("."), false)?,
                if a.query.is_empty() {
                    None
                } else {
                    Some(a.query)
                },
                call["function"]["name"] == "find",
            )
        }
        "grep" | "project_search" => {
            let a: SearchArgs = parse(a)?;
            nonempty(&a.query, 512)?;
            Tool::Search(
                path(root, a.path.as_deref().unwrap_or("."), false)?,
                a.query,
            )
        }
        "launcher_search" | "pc_search" => {
            let a: QueryArgs = parse(a)?;
            nonempty(&a.query, 512)?;
            Tool::Route(
                if call["function"]["name"] == "pc_search" {
                    "core:knowledge:search"
                } else {
                    "search:all"
                },
                a.query,
            )
        }
        "pc_read" => {
            let a: ReadArgs = parse(a)?;
            nonempty(&a.result_id, 512)?;
            Tool::Route("core:knowledge:read", a.result_id)
        }
        _ => return Err("Native arguments: unknown tool; batch rejected before effects".into()),
    };
    Ok(tool)
}
impl AiCore {
    pub(super) async fn native_tool(
        &self,
        tool: Tool,
        root: &Path,
        approval_mode: Option<&str>,
        run: &str,
        cancel: &Cancel,
    ) -> Result<String, String> {
        let raw = self.read_config().await?;
        // Only an explicit per-request override is pinned. UI changes to the
        // configured mode apply before every subsequent tool in a live run.
        let mode = approval_mode
            .or_else(|| raw["agentApprovalMode"].as_str())
            .unwrap_or("default");
        let sensitive = |p: &Path| {
            runtime::policy::sensitive(
                p,
                &self.inner.ctx.paths.home_dir,
                &self.inner.ctx.paths.config_dir,
            )
        };
        if let Tool::Bash(command, ..) = &tool {
            if runtime::policy::broad_search(command, &self.inner.ctx.paths.home_dir)
                && !self
                    .inner
                    .state
                    .lock()
                    .unwrap()
                    .indexed_attempts
                    .contains(run)
            {
                return Err(
                    "Use launcher_search or pc_search before broad personal-file scans".into(),
                );
            }
        }
        let needs_prompt = match &tool {
            Tool::Bash(c, ..) => runtime::policy::shell_asks(
                c,
                mode,
                &raw,
                c.contains(".env")
                    || c.contains(".ssh")
                    || c.contains(".aws")
                    || c.contains(".pi/agent"),
            ),
            Tool::Read(p, ..) | Tool::Search(p, _) | Tool::Directory(p, ..) => {
                runtime::policy::file_asks(false, !p.starts_with(root), sensitive(p), mode)
            }
            Tool::Write(p, ..) | Tool::Edit(p, ..) => {
                runtime::policy::file_asks(true, !p.starts_with(root), sensitive(p), mode)
            }
            Tool::Screen => mode != "turbo",
            _ => mode == "review",
        };
        if needs_prompt
            && !self
                .approval_prompt(
                    run,
                    "Allow native tool?",
                    &tool.label_for_approval(),
                    cancel,
                    matches!(&tool, Tool::Bash(..)),
                )
                .await
        {
            return Err("Native permission: tool was denied or approval expired".into());
        }
        if cancel.is_cancelled()
            || !self
                .inner
                .state
                .lock()
                .unwrap()
                .agents
                .get(run)
                .is_some_and(|r| !r.cancel.is_cancelled())
        {
            return Err("Operation cancelled".into());
        }
        match &tool {
            Tool::Read(p, ..)
            | Tool::Edit(p, ..)
            | Tool::Write(p, ..)
            | Tool::Search(p, _)
            | Tool::Directory(p, ..) => {
                let resolved = if matches!(&tool, Tool::Write(_, _, true)) {
                    create_path(root, p.to_str().ok_or("Native path: not UTF-8")?)?
                } else {
                    path(
                        root,
                        p.to_str().ok_or("Native path: not UTF-8")?,
                        matches!(&tool, Tool::Write(..)),
                    )?
                };
                if &resolved != p {
                    return Err("Native path: changed since approval".into());
                }
            }
            _ => {}
        }
        match tool {
            Tool::Extension(..) => Err("Extension call must use its immutable run table".into()),
            Tool::Bash(c, root, timeout) => {
                let mut command = if cfg!(windows) {
                    let mut shell = self.command("powershell.exe").await;
                    shell.args(["-NoLogo","-NoProfile","-NonInteractive","-Command",&format!("[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); $OutputEncoding = [Console]::OutputEncoding; {c}")]);
                    shell
                } else {
                    let mut shell = self.command("/bin/bash").await;
                    shell
                        .args(["--noprofile", "--norc", "-c", &c])
                        .env_remove("BASH_ENV")
                        .env_remove("ENV");
                    shell
                };
                process::output(
                    command.current_dir(root),
                    Duration::from_secs(timeout),
                    cancel,
                )
                .await
            }
            Tool::Screen => Err("Screen capture must use the image result path".into()),
            Tool::Read(p, offset, limit) => tokio::task::spawn_blocking(move || {
                read_file(&p).map(|text| {
                    text.lines()
                        .skip(offset)
                        .take(limit)
                        .collect::<Vec<_>>()
                        .join("\n")
                })
            })
            .await
            .map_err(err)?,
            Tool::Write(p, text, parents) => {
                let cancel = cancel.clone();
                tokio::task::spawn_blocking(move || {
                    if cancel.is_cancelled() {
                        return Err("Operation cancelled".into());
                    }
                    if parents {
                        std::fs::create_dir_all(p.parent().ok_or("No parent")?).map_err(err)?;
                    }
                    atomic_write(&p, &text, &cancel)
                })
                .await
                .map_err(err)?
            }
            Tool::Edit(p, old, new) => {
                let cancel = cancel.clone();
                tokio::task::spawn_blocking(move || {
                    let text = replacement(&p, &old, &new)?;
                    atomic_write(&p, &text, &cancel)
                })
                .await
                .map_err(err)?
            }
            Tool::Search(p, q) => {
                let cancel = cancel.clone();
                tokio::task::spawn_blocking(move || project_search(&p, &q, &cancel))
                    .await
                    .map_err(err)?
            }
            Tool::Directory(p, q, recursive) => {
                let cancel = cancel.clone();
                tokio::task::spawn_blocking(move || directory(&p, q.as_deref(), recursive, &cancel))
                    .await
                    .map_err(err)?
            }
            Tool::Route(route, q) => {
                if route != "core:knowledge:read" {
                    self.inner
                        .state
                        .lock()
                        .unwrap()
                        .indexed_attempts
                        .insert(run.into());
                }
                let args = match route {
                    "core:knowledge:read" => vec![json!(q), json!(8000)],
                    "core:knowledge:search" => vec![json!(q), json!(8)],
                    _ => vec![json!(q)],
                };
                tokio::select! {biased; _=cancel.cancelled()=>Err("Operation cancelled".into()),r=tokio::time::timeout(Duration::from_secs(20),self.inner.ctx.request(route,args))=>r.map_err(|_|"Native search: timed out".to_string())?.and_then(|v|{if route=="search:all" {search_projection::launcher_results(&v,10).map(|v|v.to_string())}else{Ok(v.to_string())}})}
            }
        }
    }
}
impl Tool {
    fn label_for_approval(&self) -> String {
        match self {
            Self::Extension(name, _) => name.clone(),
            Self::Directory(p, _, _) => format!("list {}", p.display()),
            Self::Screen => "Capture screen".into(),
            Self::Bash(c, ..) => c.clone(),
            Self::Write(p, text, _) => format!("write {}\n{}", p.display(), cut(text, 4000)),
            Self::Edit(p, old, new) => format!(
                "edit {}\noldText: {}\nnewText: {}",
                p.display(),
                cut(old, 2000),
                cut(new, 2000)
            ),
            _ => self.label(),
        }
    }
}
fn atomic_write(path: &Path, text: &str, cancel: &Cancel) -> Result<String, String> {
    use std::io::Write;
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().ok_or("Native path: no parent")?)
        .map_err(err)?;
    if let Ok(meta) = path.metadata() {
        temp.as_file()
            .set_permissions(meta.permissions())
            .map_err(err)?;
    }
    temp.write_all(text.as_bytes()).map_err(err)?;
    if cancel.is_cancelled() {
        return Err("Operation cancelled".into());
    }
    temp.persist(path).map_err(err)?;
    Ok(format!(
        "Wrote {} UTF-8 bytes to {}",
        text.len(),
        path.display()
    ))
}
fn project_search(root: &Path, query: &str, cancel: &Cancel) -> Result<String, String> {
    let mut out = String::new();
    let mut files = 0;
    let mut visited = 0;
    let mut bytes = 0;
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .max_depth(16)
        .into_iter()
        .filter_entry(|e| {
            !e.file_name().to_string_lossy().starts_with(".env")
                && !matches!(
                    e.file_name().to_str(),
                    Some(
                        ".git"
                            | "node_modules"
                            | "target"
                            | "dist"
                            | ".ssh"
                            | ".aws"
                            | ".gnupg"
                            | ".pi"
                    )
                )
        })
    {
        if cancel.is_cancelled() {
            return Err("Operation cancelled".into());
        }
        visited += 1;
        if visited > 2000 {
            out.push_str("\n[Search truncated]");
            break;
        }
        let entry = entry.map_err(err)?;
        if !entry.file_type().is_file() {
            continue;
        }
        files += 1;
        if files > 1000 || bytes > 8 * 1024 * 1024 || out.len() > 32000 {
            out.push_str("\n[Search truncated]");
            break;
        }
        if let Ok(text) = read_file(entry.path()) {
            bytes += text.len();
            for (i, line) in text
                .lines()
                .enumerate()
                .filter(|(_, l)| l.contains(query))
                .take(20)
            {
                out.push_str(&format!(
                    "{}:{}: {}\n",
                    entry.path().display(),
                    i + 1,
                    cut(line, 300)
                ));
            }
        }
    }
    if out.is_empty() {
        out.push_str("No matches in the bounded project search.");
    }
    Ok(out)
}

fn create_path(root: &Path, raw: &str) -> Result<PathBuf, String> {
    nonempty(raw, 4096)?;
    let candidate = root.join(raw);
    let mut current = candidate.as_path();
    let mut tail = vec![];
    while !current.exists() {
        let name = current.file_name().ok_or("Invalid parent directory")?;
        if name == ".." || name == "." {
            return Err("Invalid parent directory".into());
        }
        tail.push(name.to_os_string());
        current = current.parent().ok_or("No existing parent directory")?;
    }
    let mut path = current.canonicalize().map_err(err)?;
    for name in tail.into_iter().rev() {
        path.push(name)
    }
    Ok(path)
}
fn directory(
    root: &Path,
    query: Option<&str>,
    recursive: bool,
    cancel: &Cancel,
) -> Result<String, String> {
    let mut out = String::new();
    let mut count = 0;
    for e in walkdir::WalkDir::new(root)
        .follow_links(false)
        .max_depth(if recursive { 16 } else { 1 })
        .into_iter()
        .filter_entry(|e| {
            !matches!(
                e.file_name().to_str(),
                Some(
                    ".git"
                        | "node_modules"
                        | "target"
                        | "dist"
                        | ".ssh"
                        | ".aws"
                        | ".gnupg"
                        | ".pi"
                )
            ) && !e.file_name().to_string_lossy().starts_with(".env")
        })
    {
        if cancel.is_cancelled() {
            return Err("Operation cancelled".into());
        }
        count += 1;
        if count > 2000 || out.len() > 63000 {
            out.push_str("\n[Listing truncated]");
            break;
        }
        let e = e.map_err(err)?;
        if e.depth() == 0 || query.is_some_and(|q| !e.file_name().to_string_lossy().contains(q)) {
            continue;
        }
        out.push_str(&format!(
            "{}{}\n",
            e.path().display(),
            if e.file_type().is_dir() { "/" } else { "" }
        ));
    }
    Ok(out)
}

impl AiCore {
    pub(super) async fn native_screen(
        &self,
        cfg: &Value,
        approval_mode: Option<&str>,
        run: &str,
        cancel: &Cancel,
    ) -> Result<Vec<runtime::transcript::Block>, String> {
        use runtime::transcript::Block;
        let raw = self.read_config().await?;
        let mode = approval_mode
            .or_else(|| raw["agentApprovalMode"].as_str())
            .unwrap_or("default");
        if mode != "turbo"
            && !self
                .approval_prompt(
                    run,
                    "Allow screen capture?",
                    "Capture the active display",
                    cancel,
                    false,
                )
                .await
        {
            return Err("Native permission: screen capture denied".into());
        }
        if cancel.is_cancelled() {
            return Err("Operation cancelled".into());
        }
        let image = tokio::select! {biased;_=cancel.cancelled()=>return Err("Operation cancelled".into()),r=self.inner.ctx.request("core:platform:screenshot",vec![json!({"maxWidth":1600,"hideLauncher":true})])=>r?};
        let images = legacy::normalize_images(&[image])?;
        let defaults = config::defaults();
        let p = s(&cfg["provider"]);
        let vision = cfg["providerModels"][p]
            .as_array()
            .or_else(|| defaults["DEFAULT_PROVIDER_MODELS"][p].as_array())
            .and_then(|m| m.iter().find(|m| m["id"] == cfg["model"]))
            .and_then(|m| m["capabilities"].as_array())
            .is_some_and(|c| c.iter().any(|c| c == "vision"));
        if vision {
            Ok(images
                .into_iter()
                .map(|v| Block::Image {
                    mime: s(&v["mimeType"]).into(),
                    data: s(&v["data"]).into(),
                })
                .collect())
        } else {
            let text = self.ocr_images(&images, cancel).await?;
            if text.is_empty() {
                return Err("The selected model cannot read images. Choose a vision model or remove the attachment.".into());
            }
            Ok(vec![Block::Text { text }])
        }
    }
}
