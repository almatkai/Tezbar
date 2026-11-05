//! Image policy and leases. Adapters fetch/decode resources only on explicit effects.
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Variant {
    Icon,
    Avatar,
    Screenshot,
    Artwork,
}
impl Variant {
    pub fn width(self) -> u32 {
        match self {
            Self::Icon | Self::Avatar => 128,
            Self::Screenshot => 980,
            Self::Artwork => 640,
        }
    }
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    Builtin(String),
    File(PathBuf),
    Remote(String),
    Data(String),
    Blob(String),
    Glyph(String),
}
pub enum Effect {
    Fetch {
        token: u64,
        url: String,
        max_bytes: usize,
        max_width: u32,
    },
    ReadFile {
        token: u64,
        path: PathBuf,
        max_bytes: usize,
    },
    DecodeData {
        token: u64,
        data: String,
        max_bytes: usize,
    },
    Release(String),
}
/// Absolute legacy icons require explicit adapter-provided authorization. Relative assets stay in the extension root.
pub fn resolve(
    raw: &str,
    extension_root: &Path,
    allowed_legacy: &BTreeSet<PathBuf>,
) -> Result<Source, &'static str> {
    if raw.starts_with("raycast-") || raw.starts_with("tezbar-") {
        return Ok(Source::Builtin(raw.into()));
    }
    if raw.starts_with("blob:") {
        return Ok(Source::Blob(raw.into()));
    }
    if raw.starts_with("data:image/") {
        if raw.len() > 2 * 1024 * 1024 * 4 / 3 + 128 {
            return Err("Image exceeds limit");
        }
        return Ok(Source::Data(raw.into()));
    }
    if let Ok(u) = url::Url::parse(raw) {
        match u.scheme() {
            "https" if u.username().is_empty() && u.password().is_none() => {
                return Ok(Source::Remote(u.into()))
            }
            "file" => {
                let p = u.to_file_path().map_err(|_| "Invalid file URI")?;
                return local(p, extension_root, allowed_legacy);
            }
            _ => return Err("Unsupported image scheme"),
        }
    }
    if raw.chars().count() <= 4 && !raw.is_ascii() {
        return Ok(Source::Glyph(raw.into()));
    }
    let path = Path::new(raw);
    if path.is_absolute() {
        return local(path.into(), extension_root, allowed_legacy);
    }
    let relative = raw.strip_prefix("./").unwrap_or(raw);
    let path = Path::new(relative);
    if path.components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err("Image path escapes extension");
    }
    local(extension_root.join(path), extension_root, allowed_legacy)
}
fn local(path: PathBuf, root: &Path, allowed: &BTreeSet<PathBuf>) -> Result<Source, &'static str> {
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("Image path escapes extension");
    }
    if !path.starts_with(root) && !allowed.contains(&path) {
        return Err("Local image is outside extension");
    }
    Ok(Source::File(path))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Lease(u64);
struct Entry {
    token: u64,
    users: BTreeSet<Lease>,
    handle: Option<String>,
    bytes: usize,
    touched: u64,
}
#[derive(Default)]
pub struct ImageCache {
    entries: BTreeMap<(Source, Variant), Entry>,
    leases: BTreeMap<Lease, (Source, Variant)>,
    next: u64,
    bytes: usize,
}
impl ImageCache {
    pub fn acquire(
        &mut self,
        source: Source,
        variant: Variant,
    ) -> Result<(Lease, Option<Effect>), &'static str> {
        self.next += 1;
        let lease = Lease(self.next);
        let key = (source.clone(), variant);
        if let Some(e) = self.entries.get_mut(&key) {
            e.users.insert(lease);
            e.touched = self.next;
            self.leases.insert(lease, key);
            return Ok((lease, None));
        }
        if self.entries.len() >= 128 {
            return Err("Image cache is full");
        }
        let token = self.next;
        let effect = match source {
            Source::Remote(url) => Some(Effect::Fetch {
                token,
                url,
                max_bytes: 2 * 1024 * 1024,
                max_width: variant.width(),
            }),
            Source::File(path) => Some(Effect::ReadFile {
                token,
                path,
                max_bytes: 2 * 1024 * 1024,
            }),
            Source::Data(data) => Some(Effect::DecodeData {
                token,
                data,
                max_bytes: 2 * 1024 * 1024,
            }),
            _ => None,
        };
        self.entries.insert(
            key.clone(),
            Entry {
                token,
                users: [lease].into(),
                handle: None,
                bytes: 0,
                touched: self.next,
            },
        );
        self.leases.insert(lease, key);
        Ok((lease, effect))
    }
    pub fn peek(&self, lease: Lease) -> Option<&str> {
        self.entries
            .get(self.leases.get(&lease)?)
            .and_then(|e| e.handle.as_deref())
    }
    pub fn ready(&mut self, token: u64, handle: String, bytes: usize) -> Vec<Effect> {
        let key = self
            .entries
            .iter()
            .find(|(_, e)| e.token == token)
            .map(|(k, _)| k.clone());
        let Some(key) = key else {
            return vec![Effect::Release(handle)];
        };
        if bytes > 2 * 1024 * 1024 || self.bytes + bytes > 16 * 1024 * 1024 {
            return vec![Effect::Release(handle)];
        }
        let e = self.entries.get_mut(&key).unwrap();
        if e.users.is_empty() {
            self.entries.remove(&key);
            return vec![Effect::Release(handle)];
        }
        let mut effects = vec![];
        if let Some(old) = e.handle.replace(handle) {
            self.bytes -= e.bytes;
            effects.push(Effect::Release(old));
        }
        e.bytes = bytes;
        self.bytes += bytes;
        effects
    }
    pub fn release(&mut self, lease: Lease) -> Vec<Effect> {
        let Some(key) = self.leases.remove(&lease) else {
            return vec![];
        };
        let e = self.entries.get_mut(&key).unwrap();
        e.users.remove(&lease);
        if !e.users.is_empty() {
            return vec![];
        }
        let e = self.entries.remove(&key).unwrap();
        self.bytes -= e.bytes;
        e.handle.map(Effect::Release).into_iter().collect()
    }
    pub fn idle(&mut self) -> Vec<Effect> {
        let leases = self.leases.keys().copied().collect::<Vec<_>>();
        leases.into_iter().flat_map(|l| self.release(l)).collect()
    }
    pub fn retained_bytes(&self) -> usize {
        self.bytes
    }
}
#[cfg(test)]
mod tests;
