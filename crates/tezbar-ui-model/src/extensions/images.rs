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
    AppPath { extension_id: String, path: PathBuf },
    File(PathBuf),
    Remote(String),
    Data(String),
    Blob(String),
    Glyph(String),
}
pub enum Effect {
    ResolveAppPath {
        token: u64,
        extension_id: String,
        path: PathBuf,
        max_width: u32,
    },
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
/// Absolute legacy icons require explicit adapter-provided authorization.
/// SDK asset names are relative to assets/; explicit assets/ paths keep that root.
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
    // Host-independent Windows syntax check must precede URL parsing (C: is a URL scheme).
    if windows_absolute(raw) {
        if raw.split(['\\', '/']).any(|part| part == "..") {
            return Err("Image path escapes extension");
        }
        let path = PathBuf::from(raw);
        return if allowed_legacy.contains(&path) {
            Ok(Source::File(path))
        } else {
            Err("Local image is outside extension")
        };
    }
    if let Ok(u) = url::Url::parse(raw) {
        match u.scheme() {
            "https" if u.username().is_empty() && u.password().is_none() => {
                return Ok(Source::Remote(u.into()));
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
    let path = if path.starts_with("assets") {
        extension_root.join(path)
    } else {
        extension_root.join("assets").join(path)
    };
    local(path, extension_root, allowed_legacy)
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
    failed: bool,
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
        if self.leases.len() >= 1024 {
            return Err("Too many image leases");
        }
        if let Some(e) = self.entries.get_mut(&key) {
            e.users.insert(lease);
            e.touched = self.next;
            let effect = if e.failed {
                e.failed = false;
                e.token = self.next;
                request(&source, variant, self.next)
            } else {
                None
            };
            self.leases.insert(lease, key);
            return Ok((lease, effect));
        }
        if self.entries.len() >= 128 {
            return Err("Image cache is full");
        }
        let token = self.next;
        let effect = request(&source, variant, token);
        self.entries.insert(
            key.clone(),
            Entry {
                token,
                users: [lease].into(),
                handle: None,
                bytes: 0,
                touched: self.next,
                failed: false,
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
        if bytes > 2 * 1024 * 1024
            || self.bytes.saturating_sub(self.entries[&key].bytes) + bytes > 16 * 1024 * 1024
        {
            self.failed(token);
            return vec![Effect::Release(handle)];
        }
        let e = self.entries.get_mut(&key).unwrap();
        if e.failed || e.users.is_empty() {
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
    /// A fetch/decode/lookup error leaves a placeholder. Next acquire retries with a
    /// fresh token; stale success/failure cannot mutate that retry.
    pub fn failed(&mut self, token: u64) {
        if let Some(e) = self
            .entries
            .values_mut()
            .find(|e| e.token == token && e.handle.is_none())
        {
            e.failed = true;
        }
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

fn windows_absolute(raw: &str) -> bool {
    let b = raw.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'\\' | b'/'))
        || raw.starts_with("\\\\")
        || raw.starts_with("//")
}
fn request(source: &Source, variant: Variant, token: u64) -> Option<Effect> {
    Some(match source {
        Source::Remote(url) => Effect::Fetch {
            token,
            url: url.clone(),
            max_bytes: 2 * 1024 * 1024,
            max_width: variant.width(),
        },
        Source::File(path) => Effect::ReadFile {
            token,
            path: path.clone(),
            max_bytes: 2 * 1024 * 1024,
        },
        Source::Data(data) => Effect::DecodeData {
            token,
            data: data.clone(),
            max_bytes: 2 * 1024 * 1024,
        },
        Source::AppPath { extension_id, path } => Effect::ResolveAppPath {
            token,
            extension_id: extension_id.clone(),
            path: path.clone(),
            max_width: variant.width(),
        },
        _ => return None,
    })
}
/// A selected catalog icon owns one lease and its current catalog fallback. Changing
/// selection clears the old lease synchronously, before an adapter dispatches effects.
#[derive(Default)]
pub struct CatalogIcon {
    lease: Option<Lease>,
    pub fallback: Option<Source>,
}
impl CatalogIcon {
    pub fn select(
        &mut self,
        cache: &mut ImageCache,
        extension_id: &str,
        app_path: Option<PathBuf>,
        fallback: Option<Source>,
    ) -> Result<Vec<Effect>, &'static str> {
        let mut effects = self.clear(cache);
        self.fallback = fallback;
        if let Some(path) = app_path {
            let (lease, effect) = cache.acquire(
                Source::AppPath {
                    extension_id: extension_id.into(),
                    path,
                },
                Variant::Icon,
            )?;
            self.lease = Some(lease);
            effects.extend(effect);
        }
        Ok(effects)
    }
    pub fn handle<'a>(&self, cache: &'a ImageCache) -> Option<&'a str> {
        cache.peek(self.lease?)
    }
    pub fn clear(&mut self, cache: &mut ImageCache) -> Vec<Effect> {
        self.fallback = None;
        self.lease
            .take()
            .map(|l| cache.release(l))
            .unwrap_or_default()
    }
}
