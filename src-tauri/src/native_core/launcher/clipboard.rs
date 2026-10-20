use super::{
    clipboard_native as native,
    stores::{read_or, text_arg},
};
use crate::native_core::common::{now_ms, write_json_atomic, CoreContext, CoreResult};
use base64::Engine;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    io::Cursor,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};
#[derive(Clone)]
pub struct ClipboardCore {
    tx: mpsc::Sender<Command>,
    thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
}
enum Command {
    Request(String, Vec<Value>, tokio::sync::oneshot::Sender<CoreResult>),
    Watch(bool),
    Stop(tokio::sync::oneshot::Sender<()>),
}
struct Worker {
    ctx: CoreContext,
    db: Option<Value>,
    clipboard: Option<arboard::Clipboard>,
    last_sequence: Option<u64>,
    last_id: String,
    watch: bool,
    image_fingerprints: HashMap<PathBuf, (u64, u64, String)>,
    config: Value,
    next_config: std::time::Instant,
}
impl ClipboardCore {
    pub fn new(ctx: CoreContext) -> Self {
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("launcher-clipboard".into())
            .spawn(move || {
                let mut w = Worker {
                    ctx,
                    db: None,
                    clipboard: None,
                    last_sequence: None,
                    last_id: String::new(),
                    watch: false,
                    image_fingerprints: HashMap::new(),
                    config: json!({}),
                    next_config: std::time::Instant::now(),
                };
                loop {
                    match rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(Command::Request(c, a, reply)) => {
                            let _ = reply.send(w.handle(&c, &a));
                        }
                        Ok(Command::Watch(enabled)) => w.watch = enabled,
                        Ok(Command::Stop(reply)) => {
                            let _ = reply.send(());
                            break;
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    if w.watch {
                        if let Err(e) = w.poll() {
                            log::debug!("Clipboard capture: {e}");
                        }
                    }
                }
            })
            .expect("clipboard worker");
        Self {
            tx,
            thread: Arc::new(Mutex::new(Some(thread))),
        }
    }
    pub fn start(&self, native: bool) {
        let _ = self.tx.send(Command::Watch(native));
    }
    pub async fn shutdown(&self) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if self.tx.send(Command::Stop(tx)).is_ok() {
            let _ = rx.await;
        }
        let thread = self.thread.lock().ok().and_then(|mut t| t.take());
        if let Some(t) = thread {
            let _ = tokio::task::spawn_blocking(move || t.join()).await;
        }
    }
    pub async fn handle(&self, c: &str, a: &[Value]) -> CoreResult {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(Command::Request(c.into(), a.to_vec(), tx))
            .map_err(|_| "Clipboard worker stopped")?;
        rx.await.map_err(|e| e.to_string())?
    }
}
fn hash(s: &[u8]) -> String {
    format!("{:x}", Sha1::digest(s))
}
pub fn text_entry(text: &str, now: u64) -> Option<Value> {
    if text.trim().is_empty() {
        return None;
    }
    let id = format!("text:{}", &hash(format!("text|{text}").as_bytes())[..12]);
    let t = text.trim();
    let secret=t.len()>=16&&(regex::Regex::new(r"(?i)^(sk-[a-z0-9]{16,}|gh[pousr]_[a-z0-9_]{20,})$|password\s*[=:]|api[_-]?key\s*[=:]|token\s*[=:]").unwrap().is_match(t));
    Some(
        json!({"id":id,"kind":"text","createdAt":now,"pinned":false,"isSecret":secret,"text":text,"preview":text.lines().find(|l|!l.trim().is_empty()).unwrap_or("").chars().take(140).collect::<String>(),"charCount":text.encode_utf16().count(),"lineCount":text.split('\n').count()}),
    )
}
fn file_entry(paths: Vec<String>) -> Option<Value> {
    let mut seen = HashSet::new();
    let paths: Vec<_> = paths
        .into_iter()
        .map(|p| p.trim().to_owned())
        .filter(|p| !p.is_empty() && seen.insert(p.clone()))
        .collect();
    if paths.is_empty() {
        return None;
    }
    let mut sorted = paths.clone();
    sorted.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    let id = format!(
        "file:{}",
        &hash(format!("file|{}", sorted.join("|")).as_bytes())[..12]
    );
    let basename = Path::new(&paths[0])
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let preview = if paths.len() == 1 {
        basename.into_owned()
    } else {
        format!("{basename} + {} more", paths.len() - 1)
    };
    Some(
        json!({"id":id,"kind":"file","createdAt":now_ms(),"pinned":false,"isSecret":false,"paths":paths,"preview":preview}),
    )
}
impl Worker {
    fn dir(&self) -> PathBuf {
        self.ctx.paths.data_dir.join("search/clipboard-images")
    }
    fn path(&self) -> PathBuf {
        self.ctx.paths.data_dir.join("search/clipboard.json")
    }
    fn load(&mut self) -> Result<(), String> {
        if self.db.is_some() {
            return Ok(());
        }
        let mut db = read_or(&self.path(), json!({"items":[]}))?;
        let items = db["items"]
            .as_array_mut()
            .ok_or("Invalid clipboard store")?;
        items.retain(|i| {
            !i["id"].as_str().unwrap_or("").is_empty()
                && i["createdAt"].is_number()
                && match i["kind"].as_str() {
                    Some("text") => i["text"].as_str().is_some_and(|s| !s.trim().is_empty()),
                    Some("image") => i["imagePath"]
                        .as_str()
                        .is_some_and(|s| Path::new(s).is_file()),
                    Some("file") => i["paths"].as_array().is_some_and(|s| !s.is_empty()),
                    _ => false,
                }
        });
        for i in items.iter_mut() {
            if i["pinned"].is_null() {
                i["pinned"] = json!(false)
            }
            if i["isSecret"].is_null() {
                i["isSecret"] = json!(false)
            }
            if i["kind"] == "file" {
                if let Some(paths) = i["paths"].as_array() {
                    let paths: Vec<String> = paths
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect();
                    if let Some(entry) = file_entry(paths) {
                        i["preview"] = entry["preview"].clone();
                    }
                }
            }
            if i["kind"] == "image" {
                for key in ["width", "height", "byteSize"] {
                    if i[key].is_null() {
                        i[key] = json!(0)
                    }
                }
            }
            if i["kind"] == "text" {
                let t = i["text"].as_str().unwrap_or("");
                let fresh = text_entry(t, i["createdAt"].as_u64().unwrap_or(0)).unwrap();
                for k in ["preview", "charCount", "lineCount"] {
                    if i[k].is_null() {
                        i[k] = fresh[k].clone()
                    }
                }
            }
        }
        self.db = Some(db);
        Ok(())
    }
    fn items(&self) -> &Vec<Value> {
        self.db.as_ref().unwrap()["items"].as_array().unwrap()
    }
    fn save_items(&mut self, items: Vec<Value>) -> Result<(), String> {
        let mut next = self.db.as_ref().unwrap().clone();
        next["items"] = json!(items);
        write_json_atomic(&self.path(), &next)?;
        self.db = Some(next);
        Ok(())
    }
    fn native(&mut self) -> Result<&mut arboard::Clipboard, String> {
        if self.clipboard.is_none() {
            self.clipboard = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?)
        }
        Ok(self.clipboard.as_mut().unwrap())
    }
    fn managed(&self, path: &Path) -> bool {
        if !std::fs::symlink_metadata(self.dir())
            .is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
        {
            return false;
        }
        let Ok(root) = self.dir().canonicalize() else {
            return false;
        };
        let Ok(data) = self.ctx.paths.data_dir.canonicalize() else {
            return false;
        };
        if root != data.join("search/clipboard-images") {
            return false;
        }
        let Ok(p) = path.canonicalize() else {
            return false;
        };
        p.parent() == Some(root.as_path())
            && p.extension().is_some_and(|e| e == "png")
            && std::fs::symlink_metadata(path)
                .is_ok_and(|s| s.is_file() && !s.file_type().is_symlink())
    }
    fn remove_image(&self, i: &Value) {
        if let Some(p) = i["imagePath"].as_str() {
            let p = Path::new(p);
            if self.managed(p) {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    fn insert(&mut self, mut entry: Value) -> Result<(), String> {
        self.load()?;
        let id = entry["id"].as_str().unwrap_or("").to_owned();
        if self.last_id == id {
            return Ok(());
        }
        if let Some(old) = self.items().iter().find(|i| i["id"] == id) {
            // Keep fields written by newer versions when refreshing a legacy row.
            let mut merged = old.clone();
            for (key, value) in entry.as_object().ok_or("Invalid clipboard entry")? {
                merged[key] = value.clone();
            }
            entry = merged;
            entry["pinned"] = old["pinned"].clone();
            entry["isSecret"] = json!(
                old["isSecret"].as_bool().unwrap_or(false)
                    || entry["isSecret"].as_bool().unwrap_or(false)
            );
        }
        let mut pinned: Vec<_> = self
            .items()
            .iter()
            .filter(|i| i["id"] != id && i["pinned"] == true)
            .cloned()
            .collect();
        pinned.push(entry);
        pinned.extend(
            self.items()
                .iter()
                .filter(|i| i["id"] != id && i["pinned"] != true)
                .cloned(),
        );
        pinned.truncate(200);
        self.save_items(pinned)?;
        // Failed writes must remain retryable on the next poll.
        self.last_id = id;
        Ok(())
    }
    fn image_entry(&mut self, image: arboard::ImageData<'_>, max: f64) -> Result<Value, String> {
        let rgba = image::RgbaImage::from_raw(
            image.width as u32,
            image.height as u32,
            image.bytes.into_owned(),
        )
        .ok_or("Invalid clipboard pixels")?;
        let mut image = image::DynamicImage::ImageRgba8(rgba);
        let pixels = image.width() as f64 * image.height() as f64;
        if max > 0. && pixels > max * 1_000_000. {
            let scale = (max * 1_000_000. / pixels).sqrt();
            image = image.resize_exact(
                (image.width() as f64 * scale).round().max(1.) as u32,
                (image.height() as f64 * scale).round().max(1.) as u32,
                image::imageops::FilterType::Triangle,
            );
        }
        let mut bytes = Cursor::new(vec![]);
        image
            .write_to(&mut bytes, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        let bytes = bytes.into_inner();
        // Older Electron PNG encoders can produce different bytes for identical pixels.
        // Retain their persisted IDs, pins and original PNG when that image is copied again.
        let pixel_hash = hash(image.as_bytes());
        let entries = self
            .db
            .as_ref()
            .and_then(|v| v["items"].as_array())
            .cloned()
            .unwrap_or_default();
        if self.image_fingerprints.len() > 512 {
            self.image_fingerprints.clear();
        }
        for mut old in entries {
            if old["kind"] != "image"
                || old["width"].as_u64() != Some(image.width() as u64)
                || old["height"].as_u64() != Some(image.height() as u64)
            {
                continue;
            }
            let Some(path) = old["imagePath"].as_str().map(PathBuf::from) else {
                continue;
            };
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|t| t.as_millis() as u64)
                .unwrap_or(0);
            let fingerprint = match self.image_fingerprints.get(&path) {
                Some((m, l, hash)) if *m == mtime && *l == meta.len() => Some(hash.clone()),
                _ => std::fs::read(&path)
                    .ok()
                    .and_then(|bytes| image::load_from_memory(&bytes).ok())
                    .map(|i| hash(i.to_rgba8().as_raw())),
            };
            if let Some(hash) = fingerprint {
                self.image_fingerprints
                    .insert(path, (mtime, meta.len(), hash.clone()));
                if hash == pixel_hash {
                    old["createdAt"] = json!(now_ms());
                    return Ok(old);
                }
            }
        }
        let hash = hash(&bytes);
        let path = self.dir().join(format!("{hash}.png"));
        std::fs::create_dir_all(self.dir()).map_err(|e| e.to_string())?;
        if !path.exists() {
            std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        }
        Ok(
            json!({"id":format!("image:{}",&hash[..12]),"kind":"image","createdAt":now_ms(),"pinned":false,"isSecret":false,"imagePath":path,"width":image.width(),"height":image.height(),"byteSize":bytes.len()}),
        )
    }
    fn poll(&mut self) -> Result<(), String> {
        if std::time::Instant::now() >= self.next_config {
            self.config = self.ctx.config.read()?;
            self.next_config = std::time::Instant::now() + Duration::from_secs(1);
        }
        if self.config["clipboardWatchEnabled"] == false {
            return Ok(());
        }
        let sequence = native::sequence();
        if sequence.is_some() && sequence == self.last_sequence {
            return Ok(());
        }
        self.load()?;
        let paths = native::files();
        if let Some(entry) = file_entry(paths) {
            self.insert(entry)?;
            self.last_sequence = sequence;
            return Ok(());
        }
        if self.config["clipboardCaptureImages"] == true {
            if let Ok(image) = self.native()?.get_image() {
                let entry = self.image_entry(
                    image,
                    self.config["clipboardMaxImageMegapixels"]
                        .as_f64()
                        .filter(|n| *n > 0.)
                        .unwrap_or(2.),
                )?;
                self.insert(entry)?;
                self.last_sequence = sequence;
                return Ok(());
            }
        }
        match self.native()?.get_text() {
            Ok(text) => {
                if let Some(e) = text_entry(&text, now_ms()) {
                    self.insert(e)?;
                }
            }
            Err(arboard::Error::ContentNotAvailable) => {}
            Err(e) => return Err(e.to_string()),
        }
        self.last_sequence = sequence;
        Ok(())
    }
    fn write_text(&mut self, text: String) -> Result<(), String> {
        self.native()?.set_text(text).map_err(|e| e.to_string())
    }
    fn write_image(&mut self, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() > 32 * 1024 * 1024 {
            return Err("PNG exceeds 32 MB".into());
        }
        let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?
            .into_rgba8();
        let (width, height) = image.dimensions();
        self.native()?
            .set_image(arboard::ImageData {
                width: width as usize,
                height: height as usize,
                bytes: Cow::Owned(image.into_raw()),
            })
            .map_err(|e| e.to_string())
    }
    fn cleanup(&mut self, all: bool) -> CoreResult {
        self.load()?;
        let image_count = self.items().iter().filter(|i| i["kind"] == "image").count();
        if all {
            self.save_items(
                self.items()
                    .iter()
                    .filter(|i| i["kind"] != "image")
                    .cloned()
                    .collect(),
            )?;
        }
        let referenced: HashSet<_> = self
            .items()
            .iter()
            .filter_map(|i| i["imagePath"].as_str().map(str::to_owned))
            .collect();
        let (mut removed, mut freed) = (0, 0);
        if let Ok(entries) = std::fs::read_dir(self.dir()) {
            for e in entries.flatten() {
                let p = e.path();
                if self.managed(&p) && !referenced.contains(p.to_string_lossy().as_ref()) {
                    let bytes = e.metadata().map(|m| m.len()).unwrap_or(0);
                    if std::fs::remove_file(p).is_ok() {
                        removed += 1;
                        freed += bytes;
                    }
                }
            }
        }
        Ok(json!({"removed":if all{image_count}else{removed},"freedBytes":freed}))
    }
    fn handle(&mut self, c: &str, a: &[Value]) -> CoreResult {
        match c {
            "clipboard:read" => {
                return match self.native()?.get_text() {
                    Ok(text) => Ok(json!(text)),
                    Err(arboard::Error::ContentNotAvailable) => Ok(json!("")),
                    Err(e) => Err(e.to_string()),
                };
            }
            "clipboard:write" => {
                self.write_text(text_arg(a, 0)?.into())?;
                return Ok(json!({"ok":true}));
            }
            "clipboard:write-png" => {
                let raw = text_arg(a, 0)?;
                let result = (|| {
                    let data = raw
                        .strip_prefix("data:image/png;base64,")
                        .ok_or("Invalid PNG data URL")?;
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(data)
                        .map_err(|e| e.to_string())?;
                    self.write_image(&bytes)
                })();
                return Ok(match result {
                    Ok(()) => json!({"ok":true}),
                    Err(error) => json!({"ok":false,"error":error}),
                });
            }
            "core:clipboard:cleanup" => return self.cleanup(false),
            "storage:clear-clipboard-images" => return self.cleanup(true),
            _ => {}
        }
        self.load()?;
        if c == "core:clipboard:capture-text" {
            let text = text_arg(a, 0)?;
            if let Some(entry) = text_entry(text, now_ms()) {
                self.insert(entry)?;
            }
            return Ok(Value::Null);
        }
        if c == "clipboard:list" {
            return Ok(json!(self.items()));
        }
        if c == "clipboard:clear" {
            let old = self.items().clone();
            self.save_items(vec![])?;
            for i in old {
                self.remove_image(&i)
            }
            return Ok(Value::Null);
        }
        let id = text_arg(a, 0)?;
        let Some(entry) = self.items().iter().find(|i| i["id"] == id).cloned() else {
            return Ok(if c == "clipboard:image" {
                Value::Null
            } else {
                json!(false)
            });
        };
        match c {
            "clipboard:delete" => {
                let next = self
                    .items()
                    .iter()
                    .filter(|i| i["id"] != id)
                    .cloned()
                    .collect();
                self.save_items(next)?;
                if !self
                    .items()
                    .iter()
                    .any(|i| i["imagePath"] == entry["imagePath"])
                {
                    self.remove_image(&entry)
                }
                Ok(json!(true))
            }
            "clipboard:toggle-pin" => {
                let mut items = self.items().clone();
                for i in &mut items {
                    if i["id"] == id {
                        i["pinned"] = json!(i["pinned"] != true)
                    }
                }
                items.sort_by_key(|i| i["pinned"] != true);
                self.save_items(items)?;
                Ok(json!(true))
            }
            "clipboard:restore" => {
                match entry["kind"].as_str() {
                    Some("text") => self.write_text(entry["text"].as_str().unwrap_or("").into())?,
                    Some("image") => {
                        let p = entry["imagePath"].as_str().ok_or("Image path missing")?;
                        let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
                        self.write_image(&bytes)?
                    }
                    Some("file") => {
                        let paths: Vec<_> = entry["paths"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .filter_map(|p| p.as_str().map(str::to_owned))
                            .collect();
                        native::write_files(&paths)?
                    }
                    _ => return Err("Unsupported clipboard format".into()),
                }
                self.last_sequence = native::sequence();
                self.last_id = id.into();
                Ok(json!(true))
            }
            "clipboard:image" => {
                let path = entry["imagePath"]
                    .as_str()
                    .or_else(|| entry["paths"][0].as_str());
                let Some(path) = path else {
                    return Ok(Value::Null);
                };
                let bytes = match std::fs::read(path) {
                    Ok(bytes) => bytes,
                    Err(_) => return Ok(Value::Null),
                };
                if entry["kind"] == "image" {
                    return Ok(
                        json!({"dataUrl":format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(&bytes)),"width":entry["width"],"height":entry["height"],"byteSize":bytes.len()}),
                    );
                }
                let image = match image::load_from_memory(&bytes) {
                    Ok(image) => image,
                    Err(_) => return Ok(Value::Null),
                };
                let mut png = Cursor::new(vec![]);
                image
                    .write_to(&mut png, image::ImageFormat::Png)
                    .map_err(|e| e.to_string())?;
                let png = png.into_inner();
                Ok(
                    json!({"dataUrl":format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(&png)),"width":image.width(),"height":image.height(),"byteSize":png.len()}),
                )
            }
            _ => Err(format!("Unknown clipboard channel {c}")),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_core::common::CorePaths;
    fn context(p: &Path) -> CoreContext {
        CoreContext::headless(CorePaths {
            data_dir: p.into(),
            config_dir: p.join("config"),
            resource_dir: p.join("resource"),
            temp_dir: p.join("tmp"),
            home_dir: p.join("home"),
        })
    }
    #[tokio::test]
    async fn blank_legacy_text_is_ignored_and_failed_capture_can_retry() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = context(dir.path());
        let path = ctx.paths.data_dir.join("search/clipboard.json");
        write_json_atomic(
            &path,
            &json!({"items":[{"id":"blank","kind":"text","text":"  ","createdAt":1}]}),
        )
        .unwrap();
        let c = ClipboardCore::new(ctx);
        assert_eq!(c.handle("clipboard:list", &[]).await.unwrap(), json!([]));
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(c
            .handle("core:clipboard:capture-text", &[json!("retry this")])
            .await
            .is_err());
        std::fs::remove_dir(&path).unwrap();
        c.handle("core:clipboard:capture-text", &[json!("retry this")])
            .await
            .unwrap();
        assert_eq!(
            c.handle("clipboard:list", &[]).await.unwrap()[0]["text"],
            "retry this"
        );
        c.shutdown().await;
    }

    #[tokio::test]
    async fn recapture_preserves_unknown_fields_and_non_image_preview_is_null() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = context(dir.path());
        let mut old = text_entry("repeat", 1).unwrap();
        old["futureMetadata"] = json!({"keep":42});
        old["pinned"] = json!(true);
        let text_file = dir.path().join("document.txt");
        std::fs::write(&text_file, b"ordinary file").unwrap();
        let file = file_entry(vec![text_file.to_string_lossy().into()]).unwrap();
        write_json_atomic(
            &ctx.paths.data_dir.join("search/clipboard.json"),
            &json!({"items":[old.clone(),file.clone()]}),
        )
        .unwrap();
        let c = ClipboardCore::new(ctx);
        c.handle("core:clipboard:capture-text", &[json!("repeat")])
            .await
            .unwrap();
        let list = c.handle("clipboard:list", &[]).await.unwrap();
        let entry = list
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == old["id"])
            .unwrap();
        assert_eq!(entry["futureMetadata"]["keep"], 42);
        assert_eq!(entry["pinned"], true);
        assert!(c
            .handle("clipboard:image", &[file["id"].clone()])
            .await
            .unwrap()
            .is_null());
        std::fs::remove_file(text_file).unwrap();
        assert!(c
            .handle("clipboard:image", &[file["id"].clone()])
            .await
            .unwrap()
            .is_null());
        c.shutdown().await;
    }

    #[tokio::test]
    async fn history_ids_pins_and_safe_cleanup() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        std::fs::create_dir_all(root.join("search/clipboard-images")).unwrap();
        let external = root.join("external.png");
        std::fs::write(&external, "keep").unwrap();
        let entry = text_entry("Қазақ 😀", 1).unwrap();
        assert_eq!(entry["charCount"], 8);
        let mut image = json!({"id":"image:legacy","createdAt":2,"kind":"image","imagePath":external,"pinned":false});
        image["width"] = json!(1);
        write_json_atomic(
            &root.join("search/clipboard.json"),
            &json!({"items":[entry.clone(),image],"future":42}),
        )
        .unwrap();
        let c = ClipboardCore::new(context(root));
        c.handle("clipboard:toggle-pin", &[entry["id"].clone()])
            .await
            .unwrap();
        c.handle("core:clipboard:capture-text", &[json!("other")])
            .await
            .unwrap();
        c.handle("core:clipboard:capture-text", &[json!("Қазақ 😀")])
            .await
            .unwrap();
        let items = c.handle("clipboard:list", &[]).await.unwrap();
        assert_eq!(items[0]["id"], entry["id"]);
        assert_eq!(items[0]["pinned"], true);
        c.handle("storage:clear-clipboard-images", &[])
            .await
            .unwrap();
        assert!(external.exists());
        assert_eq!(
            crate::native_core::common::read_json(&root.join("search/clipboard.json")).unwrap()
                ["future"],
            42
        );
        c.shutdown().await;
    }
    #[test]
    fn sha_and_secret() {
        let e = text_entry("password=1234567890123456", 10).unwrap();
        assert_eq!(e["isSecret"], true);
        assert_eq!(
            text_entry("hello", 1).unwrap()["id"],
            format!("text:{}", &hash(b"text|hello")[..12])
        );
        let a = file_entry(vec!["/b".into(), "/a".into()]).unwrap();
        let b = file_entry(vec!["/a".into(), "/b".into()]).unwrap();
        assert_eq!(a["id"], b["id"]);
    }
    #[test]
    fn images_keep_legacy_sha_ids_and_resize_without_touching_native_clipboard() {
        use image::ImageEncoder;
        let dir = tempfile::tempdir().unwrap();
        let ctx = context(dir.path());
        let image_dir = ctx.paths.data_dir.join("search/clipboard-images");
        std::fs::create_dir_all(&image_dir).unwrap();
        let pixels = vec![128u8; 16 * 16 * 4];
        let mut old_png = vec![];
        image::codecs::png::PngEncoder::new_with_quality(
            &mut old_png,
            image::codecs::png::CompressionType::Best,
            image::codecs::png::FilterType::NoFilter,
        )
        .write_image(&pixels, 16, 16, image::ExtendedColorType::Rgba8)
        .unwrap();
        let digest = hash(&old_png);
        let id = format!("image:{}", &digest[..12]);
        let path = image_dir.join(format!("{digest}.png"));
        std::fs::write(&path, &old_png).unwrap();
        write_json_atomic(&ctx.paths.data_dir.join("search/clipboard.json"),&json!({"items":[{"id":id,"kind":"image","imagePath":path,"width":16,"height":16,"byteSize":old_png.len(),"createdAt":1,"pinned":true,"isSecret":false}]})).unwrap();
        let mut w = Worker {
            ctx,
            db: None,
            clipboard: None,
            last_sequence: None,
            last_id: String::new(),
            watch: false,
            image_fingerprints: HashMap::new(),
            config: json!({}),
            next_config: std::time::Instant::now(),
        };
        w.load().unwrap();
        let entry = w
            .image_entry(
                arboard::ImageData {
                    width: 16,
                    height: 16,
                    bytes: Cow::Owned(pixels),
                },
                2.,
            )
            .unwrap();
        assert_eq!(entry["id"], id);
        assert_eq!(entry["imagePath"], json!(path));
        w.insert(entry).unwrap();
        assert_eq!(w.items().len(), 1);
        assert_eq!(w.items()[0]["pinned"], true);
        let payload = w.handle("clipboard:image", &[json!(id)]).unwrap();
        assert_eq!(payload["byteSize"], old_png.len());
        let data = payload["dataUrl"]
            .as_str()
            .unwrap()
            .strip_prefix("data:image/png;base64,")
            .unwrap();
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(data)
                .unwrap(),
            old_png
        );
        let resized = w
            .image_entry(
                arboard::ImageData {
                    width: 1200,
                    height: 1000,
                    bytes: Cow::Owned(vec![255; 1200 * 1000 * 4]),
                },
                0.2,
            )
            .unwrap();
        let width = resized["width"].as_u64().unwrap();
        let height = resized["height"].as_u64().unwrap();
        assert!(width * height < 201000);
        assert!(width < 1200);
        assert!(w.clipboard.is_none());
        let result = w.cleanup(true).unwrap();
        assert_eq!(result["removed"], 1);
        assert!(!path.exists());
        assert!(w.clipboard.is_none());
    }
}
