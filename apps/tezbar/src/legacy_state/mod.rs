//! M11 unexported draft. No discovery through HOME; no native or legacy writes here.
//! The Launcher/UI-state owner must serialize prepare + atomic commit before first hydration.
pub mod leveldb;
pub mod webkit;
use std::collections::BTreeMap;
use tezbar_ui_model::ui_state::{ImportReport, KEYS, UiState};

pub const ITEM_LIMIT: usize = 2 * 1024 * 1024;
pub const TOTAL_LIMIT: usize = 64 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Diagnostic {
    Missing,
    Corrupt,
    Changed,
    TooLarge,
    UnsupportedEncoding,
    OriginUnverified,
    WalPresent,
    AmbiguousOrigin,
    InvalidValue,
    CacheRebuilt,
}
/// Diagnostics contain only allowlisted key names and reason codes, never raw values/paths.
#[derive(Default)]
pub struct ReadReport {
    pub values: BTreeMap<String, String>,
    pub diagnostics: Vec<(Option<&'static str>, Diagnostic)>,
    pub complete: bool,
}
impl ReadReport {
    pub fn unavailable(reason: Diagnostic) -> Self {
        Self {
            diagnostics: vec![(None, reason)],
            ..Self::default()
        }
    }
}
pub fn known_key(key: &str) -> Option<&'static str> {
    KEYS.into_iter().find(|k| *k == key)
}
pub fn cache(key: &str) -> bool {
    matches!(
        key,
        "tezbar:home-search-results:v2" | "tezbar:search-candidates:v2"
    )
}
pub fn utf16le(bytes: &[u8]) -> Result<String, Diagnostic> {
    if bytes.len() > ITEM_LIMIT || bytes.len() % 2 != 0 {
        return Err(Diagnostic::UnsupportedEncoding);
    }
    let words: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .collect();
    String::from_utf16(&words).map_err(|_| Diagnostic::UnsupportedEncoding)
}
/// Chromium CachedStorageArea::StorageFormat: 0 = UTF16, 1 = Latin1 (NOT UTF8).
pub fn chromium_string(bytes: &[u8]) -> Result<String, Diagnostic> {
    if bytes.len() > ITEM_LIMIT {
        return Err(Diagnostic::TooLarge);
    }
    match bytes.split_first() {
        Some((0, rest)) => utf16le(rest),
        Some((1, rest)) => Ok(rest.iter().map(|b| char::from(*b)).collect()),
        _ => Err(Diagnostic::UnsupportedEncoding),
    }
}
pub enum Prepared {
    Existing(UiState),
    /// May be shown in memory but must NOT be durably written: transport needs a retry.
    Retry(ReadReport),
    /// Passed to the ONE existing owner; only its successful write persists importedLegacy.
    Commit {
        report: ImportReport,
        expected_revision: u64,
        diagnostics: Vec<(Option<&'static str>, Diagnostic)>,
    },
}
pub fn prepare(
    existing: Option<&str>,
    revision: u64,
    source: ReadReport,
) -> Result<Prepared, &'static str> {
    if let Some(raw) = existing {
        // Even importedLegacy=false existing native state wins. Never overwrite a corrupt file.
        return UiState::decode(raw)
            .map(Prepared::Existing)
            .map_err(|_| "Native UI state requires recovery; legacy import was not written");
    }
    if !source.complete {
        return Ok(Prepared::Retry(source));
    }
    let report =
        UiState::import_legacy(None, &source.values).map_err(|_| "Legacy import rejected")?;
    Ok(Prepared::Commit {
        report,
        expected_revision: revision,
        diagnostics: source.diagnostics,
    })
}
/// Pure handoff check. Owner checks immediately before its existing write_atomic, while
/// holding its exclusive mutation ownership. No second writer, mutex, executor or marker file.
pub fn commit_candidate(
    plan: &Prepared,
    current_revision: u64,
    native_exists: bool,
) -> Option<&UiState> {
    match plan {
        Prepared::Commit {
            report,
            expected_revision,
            ..
        } if *expected_revision == current_revision && !native_exists => Some(&report.state),
        _ => None,
    }
}
#[cfg(test)]
mod tests;

/// Explicit discovery inputs. Missing/ambiguous stores never mean an empty profile.
#[derive(Clone)]
pub enum Source {
    WebKit(std::path::PathBuf),
    /// The locked direct Default path is tried only if no alternate candidate exists.
    WebView2(std::path::PathBuf),
}
impl Source {
    pub fn read(&self) -> ReadReport {
        match self {
            Self::WebKit(root) => match webkit::discover_modern(root) {
                Ok(path) => webkit::read_consistent(&path, &webkit::ModernOrigin),
                Err(e) => ReadReport::unavailable(e),
            },
            Self::WebView2(root) => {
                let direct = root.join("Default/Local Storage/leveldb");
                let alternate = root.join("EBWebView/Default/Local Storage/leveldb");
                if alternate.exists() {
                    return ReadReport::unavailable(Diagnostic::AmbiguousOrigin);
                }
                if !direct.exists() {
                    return ReadReport::unavailable(Diagnostic::Missing);
                }
                leveldb::read(&direct)
            }
        }
    }
}
/// Called exclusively by Launcher before hydration or by its explicit recovery action.
/// No runtime tasks or other state writers exist until this returns.
pub fn bootstrap(
    path: &std::path::Path,
    revision: u64,
    source: &Source,
) -> Result<UiState, String> {
    match std::fs::symlink_metadata(path) {
        Ok(m) => {
            if !m.is_file() || m.file_type().is_symlink() || m.len() > 4 * 1024 * 1024 {
                return Err("Native preferences need recovery; original preserved".into());
            }
            use std::io::Read;
            let mut raw = String::new();
            std::fs::File::open(path)
                .map_err(|_| "Native preferences unavailable; original preserved")?
                .take(4 * 1024 * 1024 + 1)
                .read_to_string(&mut raw)
                .map_err(|_| "Native preferences unavailable; original preserved")?;
            if raw.len() > 4 * 1024 * 1024 {
                return Err(
                    "Native preferences exceed the supported size; original preserved".into(),
                );
            }
            return UiState::decode(&raw)
                .map_err(|_| "Native preferences need recovery; original preserved".into());
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("Native preferences unavailable; original preserved".into()),
    }
    let plan = prepare(None, revision, source.read()).map_err(str::to_owned)?;
    let state = commit_candidate(&plan, revision, path.try_exists().map_err(|_| "Cannot inspect native preferences")?)
        .ok_or("Legacy preferences unavailable. Close the previous app, then Retry saved changes. Originals preserved.")?;
    state.write_atomic_if_absent(path)?;
    Ok(state.clone())
}
