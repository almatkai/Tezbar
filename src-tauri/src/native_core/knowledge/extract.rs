use super::text;
use crate::native_core::common::CoreContext;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::{io::AsyncReadExt, process::Command};

pub const TEXT_EXTENSIONS: &str = "txt text md mdx rst org csv tsv log json jsonl yaml yml toml xml html htm css scss less js jsx mjs cjs ts tsx py rb php go rs swift java kt kts c h cc cpp hpp cs sh zsh bash fish sql graphql gql ini cfg conf properties tex bib srt vtt ics vcf diff patch dockerfile gitignore";
const IMAGES: &str = "png jpg jpeg heic heif tif tiff bmp gif webp";
const RICH: &str = "rtf rtfd doc docx odt pages ppt pptx odp key xls xlsx ods numbers epub";
pub fn extension(path: &Path) -> String {
    path.extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase()
}
fn contains(set: &str, ext: &str) -> bool {
    set.split(' ').any(|s| s == ext)
}
pub fn indexable(path: &Path) -> bool {
    let ext = extension(path);
    contains(TEXT_EXTENSIONS, &ext)
        || contains(IMAGES, &ext)
        || contains(RICH, &ext)
        || ext == "pdf"
        || ["dockerfile", "makefile", "license", "readme", "changelog"].contains(
            &path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
                .as_str(),
        )
}
pub fn max_bytes(path: &Path) -> u64 {
    let ext = extension(path);
    if ext == "pdf" || contains(RICH, &ext) {
        150 * 1024 * 1024
    } else if contains(IMAGES, &ext) {
        75 * 1024 * 1024
    } else {
        64 * 1024 * 1024
    }
}
pub fn skip(name: &str, directory: bool) -> bool {
    if name.starts_with('.')
        || [
            "Library",
            "Applications",
            "System",
            "AppData",
            "Windows",
            "Program Files",
            "Program Files (x86)",
            "ProgramData",
            "$RECYCLE.BIN",
            "node_modules",
            "bower_components",
            "Pods",
            "DerivedData",
            "build",
            "dist",
            "coverage",
            "out",
            "target",
            "__pycache__",
            "venv",
        ]
        .contains(&name)
    {
        return true;
    }
    let lower = name.to_lowercase();
    if directory {
        return [
            ".app",
            ".bundle",
            ".framework",
            ".photoslibrary",
            ".photolibrary",
            ".plugin",
            ".xcarchive",
        ]
        .iter()
        .any(|s| lower.ends_with(s));
    }
    if [
        "Thumbs.db",
        "desktop.ini",
        "package-lock.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "bun.lock",
        "bun.lockb",
        "Cargo.lock",
        "composer.lock",
        "Gemfile.lock",
        "Pipfile.lock",
        "poetry.lock",
    ]
    .contains(&name)
        || lower.ends_with(".min.js")
        || lower.ends_with(".min.css")
    {
        return true;
    }
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(||regex::Regex::new(r"(?i)^(?:id_(?:rsa|dsa|ecdsa|ed25519)|authorized_keys|known_hosts)$|(?:^|[._-])(?:credentials?|secrets?|tokens?)(?:[._-]|$)|\.(?:pem|key|p12|pfx|keystore)$").unwrap()).is_match(name)
}
#[derive(Clone)]
pub struct Cancellation {
    pub generation: Arc<AtomicU64>,
    pub expected: u64,
}
impl Cancellation {
    pub fn check(&self) -> Result<(), String> {
        if self.generation.load(Ordering::SeqCst) != self.expected {
            Err("Indexing cancelled".into())
        } else {
            Ok(())
        }
    }
}

// The helper may launch a converter. Kill-on-drop alone only terminates the
// immediate child; the process group/tree must also be released on cancellation
// and when an outer request drops this future.
struct ProcessTree(Option<u32>);
impl Drop for ProcessTree {
    fn drop(&mut self) {
        if let Some(pid) = self.0.take() {
            #[cfg(unix)]
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
            #[cfg(windows)]
            {
                let mut killer = Command::new("taskkill");
                killer
                    .args(["/PID", &pid.to_string(), "/T", "/F"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .creation_flags(0x08000000);
                if let Ok(mut child) = killer.spawn() {
                    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                        runtime.spawn(async move {
                            let _ = child.wait().await;
                        });
                    }
                }
            }
        }
    }
}

/// Bounded pipes and kill-on-drop prevent helper hangs, oversized output, and orphan processes.
pub async fn command(mut cmd: Command, cancel: &Cancellation) -> Result<Vec<u8>, String> {
    cancel.check()?;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    #[cfg(unix)]
    cmd.process_group(0);
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let _tree = ProcessTree(child.id());
    let stdout = child.stdout.take().ok_or("Missing process stdout")?;
    let stderr = child.stderr.take().ok_or("Missing process stderr")?;
    let read = async {
        let mut data = Vec::new();
        stdout
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut data)
            .await
            .map_err(|e| e.to_string())?;
        if data.len() > 64 * 1024 * 1024 {
            return Err("Native extractor output exceeded 64 MB".into());
        }
        Ok(data)
    };
    let err = async {
        let mut data = Vec::new();
        // Keep draining after the diagnostic cap: closing the pipe can break
        // helpers, and leaving it unread can block them before stdout completes.
        let mut stderr = stderr;
        let mut block = [0; 8192];
        loop {
            let n = stderr.read(&mut block).await.map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            let keep = n.min(65536usize.saturating_sub(data.len()));
            data.extend_from_slice(&block[..keep]);
        }
        Ok::<_, String>(data)
    };
    let run = async {
        let (status, data, error) = tokio::try_join!(
            async { child.wait().await.map_err(|e| e.to_string()) },
            read,
            err
        )?;
        if !status.success() {
            return Err(format!(
                "Native extractor exited {status}: {}",
                String::from_utf8_lossy(&error)
            ));
        }
        Ok(data)
    };
    let cancelled = async {
        loop {
            cancel.check()?;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    };
    tokio::select! { result=run=>result, result=cancelled=> result, _=tokio::time::sleep(std::time::Duration::from_secs(600))=>Err("Native extractor timed out".into()) }
}
fn strip_markup(s: &str) -> String {
    let s = regex::Regex::new(r"(?is)<style\b[^>]*>.*?</style>|<script\b[^>]*>.*?</script>")
        .unwrap()
        .replace_all(s, " ")
        .into_owned();
    let s = regex::Regex::new(r"<[^>]+>")
        .unwrap()
        .replace_all(&s, " ")
        .into_owned();
    let s = s
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    regex::Regex::new(r"[ \t]{2,}")
        .unwrap()
        .replace_all(&s, " ")
        .trim()
        .to_string()
}
fn helper(ctx: &CoreContext) -> Option<PathBuf> {
    let mut paths = vec![];
    if let Some(p) = std::env::var_os("SCREENOCR_HELPER_PATH") {
        paths.push(PathBuf::from(p));
    }
    for base in [
        &ctx.paths.resource_dir,
        &ctx.paths.resource_dir.join("_up_"),
        &ctx.paths.resource_dir.join(".."),
    ] {
        paths.push(base.join("native/screenocr/screenocr-helper"));
        paths.push(base.join("screenocr-helper"));
    }
    paths.into_iter().find(|p| p.is_file())
}
async fn screen(
    ctx: &CoreContext,
    name: &str,
    input: Value,
    cancel: &Cancellation,
) -> Result<Value, String> {
    let executable = helper(ctx).ok_or("The macOS text extraction helper is not available")?;
    let mut cmd = Command::new(executable);
    cmd.arg(name).arg(input.to_string());
    let bytes = command(cmd, cancel).await?;
    let response: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if response["ok"] != true {
        return Err(response["error"]
            .as_str()
            .unwrap_or("Native text extraction failed")
            .into());
    }
    Ok(response["value"].clone())
}
pub async fn extract(
    ctx: &CoreContext,
    path: &Path,
    depth: &str,
    cancel: &Cancellation,
) -> Result<Value, String> {
    cancel.check()?;
    let ext = extension(path);
    let mut warnings = vec![];
    let (mut pages, mut images, extractor, total);
    if ext == "pdf" && cfg!(target_os = "macos") {
        let pdf=screen(ctx,"extract-pdf",json!({"documentPath":path,"maxPages":if depth=="basic" {20}else{2000},"ocrScannedPages":depth!="basic","maxOcrPages":if depth=="basic" {0}else if depth=="smart" {20}else{2000},"ocrEveryPage":depth=="deep","languageCorrection":true,"languages":["en-US","ru-RU"]}),cancel).await?;
        pages = pdf["pages"]
            .as_array()
            .ok_or("Invalid PDF extraction response")?
            .clone();
        images = vec![];
        total = pdf["totalPages"].as_u64().unwrap_or(pages.len() as u64);
        extractor = "macos-pdfkit-vision";
    } else if contains(IMAGES, &ext) {
        pages = vec![];
        images = vec![];
        total = 1;
        if depth == "basic" {
            warnings.push("Image OCR is disabled at the selected Knowledge Depth.");
            extractor = "metadata-only-image";
        } else if !cfg!(target_os = "macos") {
            warnings.push("Image OCR is not available on this platform yet.");
            extractor = "unsupported-image";
        } else {
            let ocr = screen(
                ctx,
                "recognize-file",
                json!({"imagePath":path,"languageCorrection":true,"languages":["en-US","ru-RU"]}),
                cancel,
            )
            .await?;
            pages.push(json!({"pageNumber":1,"ocr":{"text":ocr["text"],"blocks":ocr["blocks"].as_array().cloned().unwrap_or_default(),"averageConfidence":ocr["averageConfidence"]}}));
            images.push(json!({"id":format!("{}:image:1",path.display()),"pageNumber":1,"ocrText":ocr["text"],"width":ocr["width"],"height":ocr["height"]}));
            extractor = "macos-vision-image";
        }
    } else {
        let mut value = String::new();
        if contains(TEXT_EXTENSIONS, &ext) || ext.is_empty() {
            let p = path.to_owned();
            let (bytes, size) = tokio::task::spawn_blocking(move || {
                use std::io::Read;
                let file = std::fs::File::open(p).map_err(|e| e.to_string())?;
                let size = file.metadata().map_err(|e| e.to_string())?.len();
                let mut b = vec![];
                file.take(8 * 1024 * 1024)
                    .read_to_end(&mut b)
                    .map_err(|e| e.to_string())?;
                Ok::<_, String>((b, size))
            })
            .await
            .map_err(|e| e.to_string())??;
            let sample = &bytes[..bytes.len().min(8192)];
            if !sample.is_empty()
                && sample.iter().filter(|b| **b == 0).count() as f64 / sample.len() as f64 > 0.02
            {
                return Err("File appears to contain binary data".into());
            }
            value = String::from_utf8_lossy(&bytes)
                .trim_start_matches('\u{feff}')
                .trim()
                .to_string();
            if ["html", "htm", "xml"].contains(&ext.as_str()) {
                value = strip_markup(&value);
            }
            if size > bytes.len() as u64 {
                warnings.push("Only the first 8 MB of this large text file was indexed.");
            }
        } else if contains(RICH, &ext) {
            if cfg!(target_os = "macos") {
                let mut cmd = Command::new("/usr/bin/textutil");
                cmd.args(["-convert", "txt", "-stdout"]).arg(path);
                if let Ok(bytes) = command(cmd, cancel).await {
                    value = String::from_utf8_lossy(&bytes).trim().into();
                }
            }
            if value.is_empty() {
                let members = if ext == "epub" {
                    vec!["*.xhtml", "*.html"]
                } else if ["xlsx", "ods", "numbers"].contains(&ext.as_str()) {
                    vec!["*.xml"]
                } else {
                    vec!["*.xml", "*.xhtml"]
                };
                for member in members {
                    let mut cmd = Command::new("unzip");
                    cmd.arg("-p").arg(path).arg(member);
                    if let Ok(bytes) = command(cmd, cancel).await {
                        let s = strip_markup(&String::from_utf8_lossy(&bytes));
                        if !s.is_empty() {
                            value.push_str(&s);
                            value.push_str("\n\n");
                        }
                    }
                }
            }
            if value.trim().is_empty() {
                return Err("No local extractor is available for this document format".into());
            }
        }
        if value.is_empty() && cfg!(target_os = "macos") {
            let mut cmd = Command::new("/usr/bin/mdls");
            cmd.args(["-raw", "-name", "kMDItemTextContent"]).arg(path);
            if let Ok(bytes) = command(cmd, cancel).await {
                let s = String::from_utf8_lossy(&bytes).trim().to_string();
                if s != "(null)" {
                    value = s;
                }
            }
        }
        if value.is_empty() {
            warnings.push("No textual content could be extracted from this file.");
        }
        pages = vec![json!({"pageNumber":1,"extractedText":value})];
        images = vec![];
        total = 1;
        extractor = if ext == "pdf" {
            "metadata-pdf"
        } else if contains(RICH, &ext) {
            "system-document-text"
        } else {
            "plain-text"
        };
    }
    for page in &mut pages {
        if let Some(s) = page["extractedText"].as_str() {
            page["extractedText"] = json!(text::redact(s));
        }
        if let Some(s) = page["ocr"]["text"].as_str() {
            page["ocr"]["text"] = json!(text::redact(s));
        }
        if let Some(blocks) = page["ocr"]["blocks"].as_array_mut() {
            for block in blocks {
                if let Some(s) = block["text"].as_str() {
                    block["text"] = json!(text::redact(s));
                }
            }
        }
    }
    for image in &mut images {
        if let Some(s) = image["ocrText"].as_str() {
            image["ocrText"] = json!(text::redact(s));
        }
    }
    Ok(
        json!({"pages":pages,"images":images,"extractor":{"id":extractor,"version":"1.0.0"},"totalPageCount":total,"warnings":warnings}),
    )
}
