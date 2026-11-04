//! Permission prompts are not an OS sandbox. Paths are limited to canonical cwd
//! for stage 1. All mutations and all unrecognized shell syntax require approval.
use super::*;
use serde::Deserialize;
use std::{
    io::Read,
    path::{Path, PathBuf},
};
const FILE_LIMIT: u64 = 256 * 1024;

pub(super) fn definitions() -> Vec<Value> {
    let string = json!({"type":"string"});
    [
        ("bash","Execute a shell command in cwd. Complex/unknown/destructive commands require confirmation.",json!({"command":string}),vec!["command"]),
        ("read","Read a UTF-8 regular file inside cwd (maximum 256 KiB).",json!({"path":string}),vec!["path"]),
        ("write","Atomically replace/create a UTF-8 file inside cwd; parent must exist. Always requires approval.",json!({"path":string,"content":string}),vec!["path","content"]),
        ("edit","Replace exactly one occurrence of oldText in a UTF-8 file inside cwd. Always requires approval.",json!({"path":string,"oldText":string,"newText":string}),vec!["path","oldText","newText"]),
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
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteArgs {
    path: String,
    content: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct EditArgs {
    path: String,
    old_text: String,
    new_text: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BashArgs {
    command: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
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
    if !resolved.starts_with(root) {
        return Err(
            "Native path: outside cwd (including symlinks) is unsupported in stage 1".into(),
        );
    }
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
        return Err("Native file: exceeds 256 KiB".into());
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
        return Err("Native edit: result exceeds 256 KiB".into());
    }
    Ok(text)
}
pub(super) enum Tool {
    Bash(String, PathBuf),
    Read(PathBuf),
    Write(PathBuf, String),
    Edit(PathBuf, String, String),
    Search(PathBuf, String),
    Route(&'static str, String),
}
impl Tool {
    pub fn label(&self) -> String {
        match self {
            Self::Bash(c, _) => format!("bash: {}", cut(c, 60)),
            Self::Read(p) => format!("read {}", p.display()),
            Self::Write(p, _) => format!("write {}", p.display()),
            Self::Edit(p, _, _) => format!("edit {} (1 change)", p.display()),
            Self::Search(_, q) => format!("project search: {}", cut(q, 60)),
            Self::Route(r, q) => format!("{r}: {}", cut(q, 60)),
        }
    }
}
pub(super) fn prepare(call: &Value, root: &Path) -> Result<Tool, String> {
    let a: Value = serde_json::from_str(s(&call["function"]["arguments"])).map_err(err)?;
    let tool = match s(&call["function"]["name"]) {
        "bash" => {
            let a: BashArgs = parse(a)?;
            nonempty(&a.command, 16384)?;
            Tool::Bash(a.command, root.into())
        }
        "read" => {
            let a: FileArgs = parse(a)?;
            let p = path(root, &a.path, false)?;
            if !p.is_file() {
                return Err("Native file: expected a regular file".into());
            }
            Tool::Read(p)
        }
        "write" => {
            let a: WriteArgs = parse(a)?;
            if a.content.len() as u64 > FILE_LIMIT {
                return Err("Native write: exceeds 256 KiB".into());
            }
            let p = path(root, &a.path, true)?;
            if p.exists() && !p.is_file() {
                return Err("Native write: expected a regular file".into());
            }
            Tool::Write(p, a.content)
        }
        "edit" => {
            let a: EditArgs = parse(a)?;
            let p = path(root, &a.path, false)?;
            replacement(&p, &a.old_text, &a.new_text)?;
            Tool::Edit(p, a.old_text, a.new_text)
        }
        "project_search" => {
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
// Deliberately no shell parser: an exact, small read-only allowlist. Every
// chain, redirection, expansion, unknown flag/program needs a prompt, including
// remembered commands in turbo. No broad remembered prefix auto-allow.
fn safe_command(command: &str) -> bool {
    matches!(command.trim(), "pwd" | "whoami" | "ls")
}
impl AiCore {
    pub(super) async fn native_tool(
        &self,
        tool: Tool,
        root: &Path,
        cfg: &Value,
        run: &str,
        cancel: &Cancel,
    ) -> Result<String, String> {
        let raw = self.read_config().await?;
        let review = cfg["agentApprovalMode"]
            .as_str()
            .or_else(|| raw["agentApprovalMode"].as_str())
            == Some("review");
        let needs_prompt = review
            || match &tool {
                Tool::Bash(c, _) => !safe_command(c),
                Tool::Write(..) | Tool::Edit(..) => true,
                _ => false,
            };
        if needs_prompt
            && !self
                .approval_prompt(
                    run,
                    "Allow native tool?",
                    &tool.label_for_approval(),
                    cancel,
                    false,
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
            Tool::Read(p) | Tool::Edit(p, ..) | Tool::Write(p, ..) | Tool::Search(p, _) => {
                let resolved = path(
                    root,
                    p.to_str().ok_or("Native path: not UTF-8")?,
                    matches!(&tool, Tool::Write(..)),
                )?;
                if &resolved != p {
                    return Err("Native path: changed since approval".into());
                }
            }
            _ => {}
        }
        match tool {
            Tool::Bash(c, root) => {
                if cfg!(windows) {
                    return Err("Native tool: bash requires a Unix host in stage 1".into());
                }
                // No Pi or login-shell initialization on native commands.
                let mut command = match c.trim() {
                    "pwd" => process::command("/bin/pwd"),
                    "whoami" => process::command("/usr/bin/whoami"),
                    "ls" => process::command("/bin/ls"),
                    _ => {
                        let mut shell = process::command("/bin/bash");
                        shell
                            .args(["--noprofile", "--norc", "-c", &c])
                            .env_remove("BASH_ENV")
                            .env_remove("ENV");
                        shell
                    }
                };
                process::output(command.current_dir(root), Duration::from_secs(60), cancel).await
            }
            Tool::Read(p) => read_file(&p),
            Tool::Write(p, text) => atomic_write(&p, &text, cancel),
            Tool::Edit(p, old, new) => {
                let text = replacement(&p, &old, &new)?;
                atomic_write(&p, &text, cancel)
            }
            Tool::Search(p, q) => project_search(&p, &q, cancel),
            Tool::Route(route, q) => {
                let args = match route {
                    "core:knowledge:read" => vec![json!(q), json!(8000)],
                    "core:knowledge:search" => vec![json!(q), json!(8)],
                    _ => vec![json!(q)],
                };
                tokio::select! {biased; _=cancel.cancelled()=>Err("Operation cancelled".into()),r=tokio::time::timeout(Duration::from_secs(20),self.inner.ctx.request(route,args))=>r.map_err(|_|"Native search: timed out".to_string())?.and_then(|v|{if route=="search:all" {gateway::launcher_results(&v,10).map(|v|v.to_string())}else{Ok(v.to_string())}})}
            }
        }
    }
}
impl Tool {
    fn label_for_approval(&self) -> String {
        match self {
            Self::Bash(c, _) => c.clone(),
            Self::Write(p, text) => format!("write {}\n{}", p.display(), cut(text, 4000)),
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
            !matches!(
                e.file_name().to_str(),
                Some(".git" | "node_modules" | "target" | "dist")
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
