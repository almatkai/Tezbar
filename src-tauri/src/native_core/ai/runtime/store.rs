//! Atomic, private canonical turn files and content-addressed image storage.
use super::super::{common, err};
use super::transcript::*;
use base64::Engine;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
static REVISIONS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String,u64>>> = std::sync::OnceLock::new();
fn revisions() -> &'static std::sync::Mutex<std::collections::HashMap<String,u64>> { REVISIONS.get_or_init(Default::default) }
fn revision_keys(root: &Path, session: &str, turn: &str) -> [String;3] {
    [root.join("llm/transcripts").to_string_lossy().into_owned(), folder(root,session).to_string_lossy().into_owned(), folder(root,session).join(key(turn)).to_string_lossy().into_owned()]
}
pub fn generation(root: &Path, session: &str, turn: &str) -> Vec<u64> {
    let _guard = LOCK.lock().unwrap();
    let revisions = revisions().lock().unwrap();
    revision_keys(root,session,turn).iter().map(|k| *revisions.get(k).unwrap_or(&0)).collect()
}
pub fn save_at(root: &Path, session: &str, turn: &str, messages: &[Message], expected: &[u64]) -> Result<(),String> {
    let _guard = LOCK.lock().map_err(|_| "Native transcript store lock failed")?;
    let revisions = revisions().lock().map_err(err)?;
    let current: Vec<_> = revision_keys(root,session,turn).iter().map(|k| *revisions.get(k).unwrap_or(&0)).collect();
    if current != expected { return Err("Conversation was deleted. This run cannot save history.".into()); }
    drop(revisions);
    save_unlocked(root,session,turn,messages)
}
fn key(id: &str) -> String {
    format!("{:x}", Sha256::digest(id.as_bytes()))
}
fn folder(root: &Path, session: &str) -> PathBuf {
    root.join("llm/transcripts").join(key(session))
}
fn private_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("Native store: invalid path")?;
    std::fs::create_dir_all(parent).map_err(err)?;
    let mut f = tempfile::NamedTempFile::new_in(parent).map_err(err)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        f.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(err)?;
    }
    f.write_all(bytes).map_err(err)?;
    f.as_file().sync_all().map_err(err)?;
    f.persist(path).map_err(err)?;
    Ok(())
}
pub fn save(root: &Path, session: &str, turn: &str, messages: &[Message]) -> Result<(), String> {
    let _guard = LOCK
        .lock()
        .map_err(|_| "Native transcript store lock failed")?;
    save_unlocked(root,session,turn,messages)
}
fn save_unlocked(root: &Path, session: &str, turn: &str, messages: &[Message]) -> Result<(), String> {
    let dir = folder(root, session);
    let blobs = dir.join("blobs");
    let mut messages = messages.to_vec();
    fn images(blocks: &mut [Block], dir: &Path) -> Result<(), String> {
        for b in blocks {
            match b {
                Block::Image { data, .. } => {
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(&*data)
                        .map_err(err)?;
                    let hash = format!("{:x}", Sha256::digest(&bytes));
                    private_write(&dir.join(&hash), &bytes)?;
                    *data = format!("blob:{hash}");
                }
                Block::ToolResult { blocks, .. } => images(blocks, dir)?,
                _ => {}
            }
        }
        Ok(())
    }
    for m in &mut messages {
        images(&mut m.blocks, &blobs)?;
    }
    private_write(
        &dir.join(format!("{}.json", key(turn))),
        &serde_json::to_vec(&messages).map_err(err)?,
    )?;
    gc(&dir)?;
    Ok(())
}
pub fn load(root: &Path, session: &str, turn: &str) -> Result<Option<Vec<Message>>, String> {
    let dir = folder(root, session);
    let path = dir.join(format!("{}.json", key(turn)));
    if !path.exists() {
        return Ok(None);
    }
    if path.metadata().map_err(err)?.len() > 32 * 1024 * 1024 {
        return Err("Native store: transcript exceeds limit".into());
    }
    let mut messages: Vec<Message> =
        serde_json::from_value(common::read_json(&path)?).map_err(err)?;
    fn images(blocks: &mut Vec<Block>, dir: &Path) {
        for b in blocks {
            match b {
                Block::Image { data, .. } => {
                    if let Some(hash) = data.strip_prefix("blob:") {
                        if hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                            match std::fs::File::open(dir.join(hash)).and_then(|f| {
                                use std::io::Read;
                                let mut bytes = vec![];
                                f.take(super::limits::IMAGE_BYTES as u64 + 1)
                                    .read_to_end(&mut bytes)?;
                                Ok(bytes)
                            }) {
                                Ok(bytes) if bytes.len() <= super::limits::IMAGE_BYTES => {
                                    *data = base64::engine::general_purpose::STANDARD.encode(bytes)
                                }
                                _ => {
                                    *b = Block::Text {
                                        text: "[Earlier image is no longer available]".into(),
                                    }
                                }
                            }
                        } else {
                            *b = Block::Text {
                                text: "[Earlier image is no longer available]".into(),
                            }
                        }
                    }
                }
                Block::ToolResult { blocks, .. } => images(blocks, dir),
                _ => {}
            }
        }
    }
    for m in &mut messages {
        images(&mut m.blocks, &dir.join("blobs"));
    }
    Ok(Some(messages))
}
fn gc(dir: &Path) -> Result<(), String> {
    let blobs = dir.join("blobs");
    if !blobs.exists() {
        return Ok(());
    }
    let mut referenced = std::collections::HashSet::new();
    fn scan(v: &Value, out: &mut std::collections::HashSet<String>) {
        match v {
            Value::String(s) => {
                if let Some(h) = s.strip_prefix("blob:") {
                    out.insert(h.into());
                }
            }
            Value::Array(a) => {
                for v in a {
                    scan(v, out)
                }
            }
            Value::Object(o) => {
                for v in o.values() {
                    scan(v, out)
                }
            }
            _ => {}
        }
    }
    for e in std::fs::read_dir(dir).map_err(err)? {
        let e = e.map_err(err)?;
        if e.path().extension().is_some_and(|s| s == "json") {
            scan(&common::read_json(&e.path())?, &mut referenced)
        }
    }
    let mut entries = vec![];
    for e in std::fs::read_dir(&blobs).map_err(err)? {
        let e = e.map_err(err)?;
        if !referenced.contains(&e.file_name().to_string_lossy().into_owned()) {
            std::fs::remove_file(e.path()).map_err(err)?;
        } else {
            let m = e.metadata().map_err(err)?;
            entries.push((m.modified().ok(), m.len(), e.path()));
        }
    }
    entries.sort_by_key(|e| e.0);
    let mut total: u64 = entries.iter().map(|e| e.1).sum();
    for (_, len, path) in entries {
        if total <= super::limits::CONVERSATION_IMAGES as u64 {
            break;
        }
        std::fs::remove_file(path).map_err(err)?;
        total -= len;
    }
    Ok(())
}
pub fn delete(root: &Path, session: Option<&str>, turn: Option<&str>) -> Result<(), String> {
    let _guard = LOCK
        .lock()
        .map_err(|_| "Native transcript store lock failed")?;
    let dir = session
        .map(|s| folder(root, s))
        .unwrap_or_else(|| root.join("llm/transcripts"));
    let revision_key = if let (Some(session),Some(turn)) = (session,turn) { folder(root,session).join(key(turn)) } else { dir.clone() };
    let mut revisions = revisions().lock().map_err(err)?;
    *revisions.entry(revision_key.to_string_lossy().into_owned()).or_default() += 1;
    drop(revisions);
    if !dir.exists() {
        return Ok(());
    }
    if let Some(turn) = turn {
        let path = dir.join(format!("{}.json", key(turn)));
        if path.exists() {
            std::fs::remove_file(path).map_err(err)?;
        }
        gc(&dir)
    } else {
        std::fs::remove_dir_all(dir).map_err(err)
    }
}
