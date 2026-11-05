//! Narrow, feature-gated seam for isolated native launcher experiments.
//! No raw dispatcher, Tauri application, window, shortcut or production profile.
use crate::native_core::{
    common::{CoreContext, CorePaths},
    NativeCore,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

const MARKER: &str = ".tezbar-native-launcher-prototype";
#[derive(Clone)]
pub struct PrototypeProfile {
    root: PathBuf,
    resources: PathBuf,
    identity: std::fs::Metadata,
}
impl PrototypeProfile {
    /// Both roots must carry matching 40-hex source manifests. Writable paths are
    /// derived exclusively from a marked, canonical child of the OS temp root.
    pub fn open(root: &Path, resource_dir: &Path) -> Result<Self, String> {
        if !root.is_absolute()
            || std::fs::symlink_metadata(root)
                .map_err(err)?
                .file_type()
                .is_symlink()
        {
            return Err("Profile must be an absolute non-symlink directory".into());
        }
        let root = root.canonicalize().map_err(err)?;
        let temp = std::env::temp_dir().canonicalize().map_err(err)?;
        if !root.starts_with(&temp) || root == temp {
            return Err("Prototype profile must be a child of OS temp".into());
        }
        let resources = resource_dir.canonicalize().map_err(err)?;
        let source = std::fs::read_to_string(root.join(MARKER)).map_err(err)?;
        if source.len() != 40
            || !source.bytes().all(|b| b.is_ascii_hexdigit())
            || std::fs::read_to_string(resources.join(".tezbar-prototype-source")).map_err(err)?
                != source
        {
            return Err(
                "Profile and read-only build resources must have matching source SHA manifests"
                    .into(),
            );
        }
        for name in ["data", "config", "home", "temp"] {
            let path = root.join(name);
            if path.exists()
                && (std::fs::symlink_metadata(&path)
                    .map_err(err)?
                    .file_type()
                    .is_symlink()
                    || path.canonicalize().map_err(err)? != path)
            {
                return Err("Symlink in prototype paths".into());
            }
            std::fs::create_dir_all(&path).map_err(err)?;
        }
        let profile = Self {
            identity: std::fs::metadata(&root).map_err(err)?,
            root,
            resources,
        };
        profile.validate_tree()?;
        Ok(profile)
    }
    fn validate(&self) -> Result<(), String> {
        if self.root.canonicalize().map_err(err)? != self.root {
            return Err("Profile root changed".into());
        }
        let current = std::fs::metadata(&self.root).map_err(err)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if (current.dev(), current.ino()) != (self.identity.dev(), self.identity.ino()) {
                return Err("Profile root replaced".into());
            }
        }
        #[cfg(not(unix))]
        if current.created().ok() != self.identity.created().ok() {
            return Err("Profile root replaced".into());
        }
        for name in [MARKER, "data", "config", "home", "temp"] {
            let path = self.root.join(name);
            if std::fs::symlink_metadata(&path)
                .map_err(err)?
                .file_type()
                .is_symlink()
                || path.canonicalize().map_err(err)? != path
            {
                return Err("Symlink in prototype paths".into());
            }
        }
        Ok(())
    }
    fn validate_tree(&self) -> Result<(), String> {
        self.validate()?;
        for name in ["data", "config", "home", "temp"] {
            for entry in walkdir::WalkDir::new(self.root.join(name)).follow_links(false) {
                if entry.map_err(err)?.file_type().is_symlink() {
                    return Err("Symlinks are forbidden in synthetic profiles".into());
                }
            }
        }
        Ok(())
    }
    fn paths(&self) -> CorePaths {
        CorePaths {
            data_dir: self.root.join("data"),
            config_dir: self.root.join("config"),
            home_dir: self.root.join("home"),
            temp_dir: self.root.join("temp"),
            resource_dir: self.resources.clone(),
        }
    }
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    Native,
    Unsupported,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IconRef {
    kind: String,
    path: String,
}
impl IconRef {
    pub fn key(&self) -> String {
        format!("{}:{}", self.kind, self.path)
    }
}
#[derive(Clone, Debug)]
pub struct Row {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub category: String,
    pub support: Support,
    pub icon: Option<IconRef>,
    action: Value,
    owner: Arc<()>,
}
impl Row {
    pub fn ticket(&self, query: &str, rank: usize) -> Ticket {
        Ticket {
            action: self.action.clone(),
            context: json!({"resultId":self.id,"query":query,"rank":rank}),
            support: self.support,
            owner: self.owner.clone(),
        }
    }
}
/// Owned and non-Clone: consumed once. Only rows obtained from this launcher can mint it.
pub struct Ticket {
    action: Value,
    context: Value,
    support: Support,
    owner: Arc<()>,
}
#[derive(Debug, PartialEq)]
pub enum Activation {
    Done,
    Failed(String),
    Unsupported,
}
#[derive(Clone)]
pub struct Launcher {
    core: Arc<NativeCore>,
    profile: PrototypeProfile,
    owner: Arc<()>,
    startup: tokio::sync::watch::Receiver<Option<Result<(), String>>>,
}
impl Launcher {
    pub fn start(profile: &PrototypeProfile) -> Result<Self, String> {
        Self::start_inner(profile, true)
    }
    fn start_inner(profile: &PrototypeProfile, desktop: bool) -> Result<Self, String> {
        profile.validate_tree()?;
        let ctx = CoreContext::new(profile.paths(), None);
        let ctx = if desktop {
            ctx.with_headless_desktop_services()
        } else {
            ctx
        };
        let core = NativeCore::new(ctx)?;
        let (tx, startup) = tokio::sync::watch::channel(None);
        let starting = core.clone();
        tokio::spawn(async move {
            let result = starting.start().await;
            let _ = tx.send(Some(result));
        });
        Ok(Self {
            core,
            profile: profile.clone(),
            owner: Arc::new(()),
            startup,
        })
    }
    pub async fn ready(&self) -> Result<(), String> {
        let mut rx = self.startup.clone();
        loop {
            if let Some(result) = rx.borrow().clone() {
                return result;
            }
            rx.changed().await.map_err(err)?;
        }
    }
    pub async fn search(&self, query: &str) -> Result<Vec<Row>, String> {
        self.profile.validate()?;
        self.ready().await?;
        let value = self.core.request("search:all", &[json!(query)]).await?;
        value
            .as_array()
            .ok_or_else(|| "Invalid search response".to_owned())?
            .iter()
            .map(|v| self.row(v))
            .collect()
    }
    fn row(&self, v: &Value) -> Result<Row, String> {
        let text = |key: &str| v[key].as_str().unwrap_or("").to_owned();
        let action = v.get("action").cloned().ok_or("Missing action")?;
        let support = classify(&action);
        let icon = match action["type"].as_str() {
            Some("open-app") => action["appPath"].as_str().map(|p| IconRef {
                kind: "application".into(),
                path: p.into(),
            }),
            Some("open-file") => action["path"].as_str().map(|p| IconRef {
                kind: "file".into(),
                path: p.into(),
            }),
            _ => None,
        };
        Ok(Row {
            id: text("id"),
            title: text("title"),
            subtitle: text("subtitle"),
            category: text("category"),
            support,
            icon,
            action,
            owner: self.owner.clone(),
        })
    }
    pub async fn activate(&self, ticket: Ticket) -> Activation {
        if !Arc::ptr_eq(&ticket.owner, &self.owner) {
            return Activation::Failed("Ticket belongs to another launcher".into());
        }
        // `Row::support` is a public display projection. Never trust a caller's
        // edits to it as authority to execute the private action envelope.
        if ticket.support != Support::Native || classify(&ticket.action) != Support::Native {
            return Activation::Unsupported;
        }
        if let Err(e) = self.profile.validate_tree() {
            return Activation::Failed(e);
        }
        match self
            .core
            .request(
                "search:execute",
                &[json!({"action":ticket.action,"context":ticket.context})],
            )
            .await
        {
            Ok(v) if v["ok"] == true => Activation::Done,
            Ok(v) => Activation::Failed(v["message"].as_str().unwrap_or("Action failed").into()),
            Err(e) => Activation::Failed(e),
        }
    }
    pub async fn icon(&self, icon: &IconRef) -> Option<Vec<u8>> {
        self.profile.validate().ok()?;
        let data = self
            .core
            .request(
                "asset-icon:data-url",
                &[json!({"kind":icon.kind,"path":icon.path})],
            )
            .await
            .ok()?;
        STANDARD
            .decode(data.as_str()?.strip_prefix("data:image/png;base64,")?)
            .ok()
    }
    /// Fixed status hook for benchmark readiness; no arbitrary channel access.
    pub async fn status(&self) -> Result<Value, String> {
        self.ready().await?;
        self.core.ctx.request("core:launcher:status", vec![]).await
    }
    pub async fn trim_idle_memory(&self) {
        self.core.trim_idle_memory().await;
    }
    pub async fn shutdown(&self) {
        self.core.shutdown().await;
    }
}
fn classify(action: &Value) -> Support {
    if matches!(
        action["type"].as_str(),
        Some("open-app" | "open-file" | "open-url" | "open-with-app" | "add-note" | "copy-text")
    ) {
        Support::Native
    } else {
        Support::Unsupported
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> (tempfile::TempDir, PrototypeProfile) {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        let resources = root.join("resources");
        std::fs::create_dir(&resources).unwrap();
        let source = "a".repeat(40);
        std::fs::write(root.join(MARKER), &source).unwrap();
        std::fs::write(resources.join(".tezbar-prototype-source"), source).unwrap();
        let p = PrototypeProfile::open(&root, &resources).unwrap();
        (d, p)
    }
    #[test]
    fn isolation_rejects_unmarked_and_swapped_paths() {
        assert!(PrototypeProfile::open(Path::new("/"), Path::new("/")).is_err());
        let (_d, p) = profile();
        #[cfg(unix)]
        {
            let moved = p.root.with_extension("moved");
            std::fs::rename(&p.root, &moved).unwrap();
            std::os::unix::fs::symlink(&moved, &p.root).unwrap();
            assert!(p.validate().is_err());
            std::fs::remove_file(&p.root).unwrap();
            std::fs::rename(moved, &p.root).unwrap();
            std::os::unix::fs::symlink("/", p.root.join("data/escape")).unwrap();
            assert!(p.validate_tree().is_err());
        }
    }
    #[tokio::test]
    async fn core_order_opaque_actions_and_single_usage_record() {
        let (_d, p) = profile();
        // Exercise the facade without reading the live clipboard in unit tests.
        let launcher = Launcher::start_inner(&p, false).unwrap();
        launcher.ready().await.unwrap();
        let query = "prototype unique note";
        let direct = launcher
            .core
            .request("search:all", &[json!(query)])
            .await
            .unwrap();
        let rows = launcher.search(query).await.unwrap();
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            direct
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["id"].as_str().unwrap())
                .collect::<Vec<_>>()
        );
        let unsupported = rows
            .iter()
            .find(|r| r.support == Support::Unsupported)
            .unwrap();
        assert_eq!(
            launcher.activate(unsupported.ticket(query, 1)).await,
            Activation::Unsupported
        );
        let mut edited = unsupported.clone();
        edited.support = Support::Native;
        assert_eq!(
            launcher.activate(edited.ticket(query, 1)).await,
            Activation::Unsupported
        );

        let (rank, row) = rows
            .iter()
            .enumerate()
            .find(|(_, r)| r.id.starts_with("note-add:"))
            .unwrap();
        assert_eq!(
            launcher.activate(row.ticket(query, rank + 1)).await,
            Activation::Done
        );
        let db = rusqlite::Connection::open(p.root.join("data/search/index.sqlite3")).unwrap();
        let count: i64 = db
            .query_row("SELECT count(*) FROM click_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        let notes: Value =
            serde_json::from_slice(&std::fs::read(p.root.join("data/search/notes.json")).unwrap())
                .unwrap();
        assert_eq!(notes["notes"].as_array().unwrap().len(), 1);
        launcher.shutdown().await;
    }
}
