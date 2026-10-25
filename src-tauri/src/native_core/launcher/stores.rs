use crate::native_core::common::{now_ms, read_json, write_json_atomic, CoreResult};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
#[derive(Clone)]
pub struct Stores {
    root: PathBuf,
    gate: Arc<tokio::sync::Mutex<()>>,
}
impl Stores {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            gate: Arc::new(tokio::sync::Mutex::new(())),
        }
    }
    pub async fn run<F>(&self, f: F) -> CoreResult
    where
        F: FnOnce(&Path) -> CoreResult + Send + 'static,
    {
        let guard = self.gate.clone().lock_owned().await;
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let _guard = guard;
            f(&root)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    pub async fn handle(&self, channel: &str, args: &[Value]) -> CoreResult {
        let c = channel.to_owned();
        let args = args.to_vec();
        self.run(move |root| handle(root, &c, &args)).await
    }
    pub async fn documents(&self) -> CoreResult {
        self.run(documents).await
    }
}
pub fn read_or(path: &Path, default: Value) -> CoreResult {
    if path.exists() {
        read_json(path)
    } else {
        Ok(default)
    }
}
pub fn array<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    v[key]
        .as_array()
        .ok_or_else(|| format!("Invalid {key} store"))
}
pub fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v[key]
        .as_str()
        .ok_or_else(|| format!("{key} must be a string"))
}
pub fn text_arg(args: &[Value], n: usize) -> Result<&str, String> {
    args.get(n)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Argument {} must be a string", n + 1))
}
pub fn load_snippets(root: &Path) -> CoreResult {
    let path = root.join("search/snippets.json");
    let mut v = read_or(&path, json!({"snippets":[]}))?;
    let builtins: Vec<Value> =
        serde_json::from_str(include_str!("default_snippets.json")).map_err(|e| e.to_string())?;
    let items = v["snippets"]
        .as_array_mut()
        .ok_or("Invalid snippets store")?;
    for mut b in builtins {
        if !items.iter().any(|s| s["id"] == b["id"]) {
            b["createdAt"] = json!(now_ms());
            items.push(b)
        }
    }
    if !path.exists() {
        write_json_atomic(&path, &v)?;
    }
    Ok(v)
}
pub fn builtin(id: &str) -> bool {
    serde_json::from_str::<Vec<Value>>(include_str!("default_snippets.json"))
        .unwrap_or_default()
        .iter()
        .any(|s| s["id"] == id)
}
pub fn interpolate(s: &str, clipboard: &str) -> String {
    let now = chrono::Local::now();
    let utc = chrono::Utc::now();
    let host = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| {
            #[cfg(unix)]
            {
                let mut b = [0u8; 256];
                unsafe {
                    libc::gethostname(b.as_mut_ptr().cast(), b.len());
                }
                String::from_utf8_lossy(&b).trim_end_matches('\0').into()
            }
            #[cfg(not(unix))]
            {
                String::new()
            }
        });
    let mut out = s.to_owned();
    for (k, v) in [
        ("date", utc.format("%Y-%m-%d").to_string()),
        ("time", now.format("%H:%M:%S").to_string()),
        (
            "datetime",
            format!("{} {}", utc.format("%Y-%m-%d"), now.format("%H:%M:%S")),
        ),
        (
            "iso",
            utc.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        ),
        ("year", now.format("%Y").to_string()),
        ("timestamp", utc.timestamp().to_string()),
        ("hostname", host),
        ("clipboard", clipboard.into()),
    ] {
        out = out.replace(&format!("${{{k}}}"), &v);
    }
    while let Some(i) = out.find("${uuid}") {
        out.replace_range(i..i + 7, &uuid::Uuid::new_v4().to_string());
    }
    out
}
pub fn snippet_title(s: &Value) -> String {
    s["label"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| match s["id"].as_str() {
            Some("snippet:today") => Some("Get today's date"),
            Some("snippet:time") => Some("Get current time"),
            Some("snippet:issue") => Some("Bug report (GitHub / Jira issue template)"),
            _ => None,
        })
        .unwrap_or_else(|| s["trigger"].as_str().unwrap_or(""))
        .trim()
        .into()
}
pub fn snippet_subtitle(s: &Value) -> String {
    let body = s["body"]
        .as_str()
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let preview = if body.chars().count() > 72 {
        format!("{}…", body.chars().take(69).collect::<String>())
    } else {
        body
    };
    let trigger = s["trigger"].as_str().unwrap_or("").trim_start_matches(';');
    let mut chars = trigger.chars();
    let hint = chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default();
    let mut line = if snippet_title(s) == s["trigger"].as_str().unwrap_or("") {
        preview
    } else if preview.is_empty() {
        hint
    } else {
        format!("{hint} · {preview}")
    };
    if let Some(scope) = s["scope"].as_str().filter(|x| *x != "global") {
        line += &format!(" · {scope}")
    }
    line
}
fn failure(message: &str) -> Value {
    json!({"ok":false,"message":message})
}
fn handle(root: &Path, c: &str, a: &[Value]) -> CoreResult {
    let v = a.first().cloned().unwrap_or(Value::Null);
    if c.starts_with("notes:") {
        let path = root.join("search/notes.json");
        let mut db = read_or(&path, json!({"notes":[]}))?;
        let notes = db["notes"].as_array_mut().ok_or("Invalid notes store")?;
        for n in notes.iter_mut() {
            if n["updatedAt"].is_null() {
                n["updatedAt"] = n["createdAt"].clone();
            }
        }
        let result = match c {
            "notes:list" => return Ok(json!(notes)),
            "notes:append" => {
                let t = v.as_str().ok_or("Note text must be a string")?.trim();
                if t.is_empty() {
                    return Ok(Value::Null);
                }
                let stamp = now_ms().max(
                    notes
                        .iter()
                        .filter_map(|n| n["createdAt"].as_u64())
                        .max()
                        .unwrap_or(0)
                        + 1,
                );
                let n = json!({"text":t,"createdAt":stamp,"updatedAt":stamp});
                notes.insert(0, n.clone());
                notes.truncate(250);
                n
            }
            "notes:update" => {
                let stamp = v["createdAt"].as_u64().ok_or("createdAt is required")?;
                let text = string(&v, "text")?;
                if let Some(n) = notes.iter_mut().find(|n| n["createdAt"] == stamp) {
                    n["text"] = json!(text);
                    n["updatedAt"] = json!(now_ms());
                    json!(true)
                } else {
                    json!(false)
                }
            }
            "notes:delete" => {
                let stamp = v.as_u64().ok_or("createdAt must be a number")?;
                let before = notes.len();
                notes.retain(|n| n["createdAt"] != stamp);
                json!(before != notes.len())
            }
            _ => return Err(format!("Unknown notes channel {c}")),
        };
        write_json_atomic(&path, &db)?;
        return Ok(result);
    }
    if c.starts_with("snippets:") {
        let path = root.join("search/snippets.json");
        let mut db = load_snippets(root)?;
        let items = db["snippets"]
            .as_array_mut()
            .ok_or("Invalid snippets store")?;
        if c == "snippets:list" {
            return Ok(json!(items.iter().map(|s|json!({"id":s["id"],"title":snippet_title(s),"subtitle":snippet_subtitle(s),"trigger":s["trigger"],"bodyTemplate":s["body"],"resolvedPreview":interpolate(s["body"].as_str().unwrap_or(""),""),"readonly":builtin(s["id"].as_str().unwrap_or(""))})).collect::<Vec<_>>()));
        }
        let id = if c == "snippets:add" {
            format!("snippet:user:{}", uuid::Uuid::new_v4())
        } else {
            text_arg(a, 0)?.to_owned()
        };
        if c != "snippets:add" && builtin(&id) {
            return Ok(failure(if c == "snippets:delete" {
                "Built-in snippets cannot be deleted"
            } else {
                "Built-in snippets cannot be edited"
            }));
        }
        if c == "snippets:delete" {
            let before = items.len();
            items.retain(|s| s["id"] != id);
            if before == items.len() {
                return Ok(failure("Snippet not found"));
            }
            write_json_atomic(&path, &db)?;
            return Ok(json!({"ok":true,"message":"Snippet removed"}));
        }
        let payload = if c == "snippets:add" {
            v
        } else {
            a.get(1).cloned().ok_or("Snippet payload required")?
        };
        let label = string(&payload, "label")?.trim();
        let trigger = string(&payload, "trigger")?.trim();
        let body = string(&payload, "body")?.replace("\r\n", "\n");
        for (value, max, msg) in [
            (label, 200, "Title"),
            (trigger, 48, "Trigger"),
            (body.as_str(), 100000, "Body"),
        ] {
            if value.trim().is_empty() {
                return Ok(failure(&format!("{msg} is required")));
            }
            if value.encode_utf16().count() > max {
                return Ok(failure(&format!("{msg} must be at most {max} characters")));
            }
        }
        if trigger.contains(['\n', '\r']) {
            return Ok(failure("Trigger must be a single line"));
        }
        if items.iter().any(|s| {
            s["id"] != id
                && s["trigger"].as_str().unwrap_or("").trim().to_lowercase()
                    == trigger.to_lowercase()
        }) {
            return Ok(failure("Another snippet already uses this trigger"));
        }
        if c == "snippets:add" {
            items.push(
                json!({"id":id,"label":label,"trigger":trigger,"body":body,"createdAt":now_ms()}),
            )
        } else if let Some(s) = items.iter_mut().find(|s| s["id"] == id) {
            s["label"] = json!(label);
            s["trigger"] = json!(trigger);
            s["body"] = json!(body)
        } else {
            return Ok(failure("Snippet not found"));
        }
        write_json_atomic(&path, &db)?;
        return Ok(if c == "snippets:add" {
            json!({"ok":true,"message":"Snippet saved","id":id})
        } else {
            json!({"ok":true,"message":"Snippet updated"})
        });
    }
    if c == "directory-visit:record" {
        let path = PathBuf::from(text_arg(a, 0)?);
        if !path.is_absolute() || !path.is_dir() {
            return Ok(Value::Null);
        }
        let path = path.canonicalize().map_err(|e| e.to_string())?;
        let path = path.to_string_lossy();
        let file = root.join("directory-visits.json");
        let mut db = read_or(&file, json!({"version":1,"visits":{}}))?;
        if !db.is_object() || !db["visits"].is_object() {
            return Err("Invalid directory visits store".into());
        }
        let n = db["visits"][path.as_ref()]["count"].as_u64().unwrap_or(0) + 1;
        db["visits"][path.as_ref()] = json!({"count":n,"lastVisitedAt":now_ms()});
        write_json_atomic(&file, &db)?;
        return Ok(Value::Null);
    }
    Err(format!("Unknown store channel: {c}"))
}
pub fn plain_note(s: &str) -> String {
    let mut out = s.to_owned();
    for (pattern, replacement) in [
        (r"(?i)<br\s*/?\s*>|</(?:div|p|li|h[1-6])>", "\n"),
        (r"(?i)<li>", "- "),
        (r"<[^>]+>", ""),
    ] {
        out = regex::Regex::new(pattern)
            .unwrap()
            .replace_all(&out, replacement)
            .into_owned();
    }
    for (from, to) in [
        ("&nbsp;", " "),
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#039;", "'"),
    ] {
        out = out.replace(from, to);
    }
    for (pattern, replacement) in [
        (r"\*\*([^*\n]+)\*\*", "$1"),
        (r"__([^_\n]+)__", "$1"),
        (r"\*([^*\n]+)\*", "$1"),
        (r"_([^_\n]+)_", "$1"),
        (r"`([^`\n]+)`", "$1"),
        (r"\[([^\]]+)\]\([^)]+\)", "$1"),
        (r"(?m)^#{1,6}\s+", ""),
        (r"(?m)^\s*[-*+]\s+", ""),
        (r"(?m)^\s*\d+\.\s+", ""),
    ] {
        out = regex::Regex::new(pattern)
            .unwrap()
            .replace_all(&out, replacement)
            .into_owned();
    }
    out = out.replace('\r', "").replace('\u{00a0}', " ");
    out = regex::Regex::new(r"[ \t]+\n")
        .unwrap()
        .replace_all(&out, "\n")
        .into_owned();
    regex::Regex::new(r"\n{3,}")
        .unwrap()
        .replace_all(&out, "\n\n")
        .trim()
        .into()
}
pub fn document(
    id: &str,
    cat: &str,
    title: &str,
    subtitle: &str,
    tokens: &str,
    action: Value,
    updated: u64,
) -> Value {
    json!({"id":id,"category":cat,"title":title,"subtitle":subtitle,"tokens":tokens,"action":action,"updatedAt":updated})
}
fn documents(root: &Path) -> CoreResult {
    let mut docs = vec![];
    let notes = read_or(&root.join("search/notes.json"), json!({"notes":[]}))?;
    for n in array(&notes, "notes")? {
        if let (Some(t), Some(stamp)) = (n["text"].as_str(), n["createdAt"].as_u64()) {
            let plain = plain_note(t);
            let title = plain
                .lines()
                .next()
                .unwrap_or("(note)")
                .chars()
                .take(100)
                .collect::<String>();
            docs.push(document(
                &format!("note:{stamp}"),
                "quick-notes",
                &title,
                "Quick note",
                &plain,
                json!({"type":"copy-text","text":plain}),
                n["updatedAt"].as_u64().unwrap_or(stamp),
            ));
        }
    }
    let snippets = load_snippets(root)?;
    for s in array(&snippets, "snippets")? {
        if let (Some(id), Some(body)) = (s["id"].as_str(), s["body"].as_str()) {
            docs.push(document(
                id,
                "snippets",
                &snippet_title(s),
                &snippet_subtitle(s),
                &format!(
                    "{} {} {}",
                    s["trigger"].as_str().unwrap_or(""),
                    body,
                    s["label"].as_str().unwrap_or("")
                ),
                json!({"type":"copy-text","text":interpolate(body,"")}),
                s["createdAt"].as_u64().unwrap_or(0),
            ));
        }
    }
    let path = root.join("search/quick-links.json");
    let links = read_or(
        &path,
        json!({"links":[{"id":"ql:google","name":"Google Search","template":"https://www.google.com/search?q={query}","createdAt":now_ms()}]}),
    )?;
    if !path.exists() {
        write_json_atomic(&path, &links)?;
    }
    for l in array(&links, "links")? {
        let template = string(l, "template")?;
        docs.push(document(
            string(l, "id")?,
            "quick-links",
            string(l, "name")?,
            &l["profile"]
                .as_str()
                .map(|p| format!("Quick link ({p})"))
                .unwrap_or("Quick link".into()),
            &format!("{} {template}", string(l, "name")?),
            json!({"type":"open-url","url":template.replace("{query}","")}),
            l["createdAt"].as_u64().unwrap_or(0),
        ));
    }
    Ok(json!(docs))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn concurrent_notes_preserve_envelope() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("search")).unwrap();
        write_json_atomic(
            &dir.path().join("search/notes.json"),
            &json!({"notes":[{"text":"старое","createdAt":1}],"future":true}),
        )
        .unwrap();
        let store = Stores::new(dir.path().into());
        let mut tasks = vec![];
        for i in 0..15 {
            let s = store.clone();
            tasks.push(tokio::spawn(async move {
                s.handle("notes:append", &[json!(format!("қазақ {i}"))])
                    .await
                    .unwrap()
            }));
        }
        for t in tasks {
            t.await.unwrap();
        }
        let rows = store.handle("notes:list", &[]).await.unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 16);
        let stamps: std::collections::HashSet<_> = rows
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["createdAt"].as_u64().unwrap())
            .collect();
        assert_eq!(stamps.len(), 16);
        assert_eq!(
            read_json(&dir.path().join("search/notes.json")).unwrap()["future"],
            true
        );
        assert_eq!(rows[15]["updatedAt"], 1);
    }
}
