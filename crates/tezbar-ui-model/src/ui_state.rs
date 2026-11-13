//! Versioned UI state. Legacy discovery is deliberately an adapter responsibility (M11).
use crate::launcher::{Action, Row};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};
use tezbar_core::instant::currency::Preferences;
pub const KEYS: [&str; 14] = [
    "tezbar:pinned-commands",
    "tezbar:recent-extension-commands",
    "tezbar:launcher-query-history:v1",
    "tezbar:home-search-results:v2",
    "tezbar:search-candidates:v2",
    "tezbar:terminal-defaults",
    "tezbar:terminal-pinned-sessions",
    "tezbar:recent-emojis",
    "tezbar:currency:prefs:v1",
    "tezbar:theme",
    "tezbar:release-notes-version",
    "tezbar:last-update-check",
    "tezbar:auto-update-enabled",
    "tezbar:settings-tab",
];
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pin {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub category: String,
    pub action: Action,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_data_url: Option<String>,
    pub slot: usize,
}
impl From<&Row> for Pin {
    fn from(r: &Row) -> Self {
        Self {
            id: r.id.clone(),
            title: r.title.clone(),
            subtitle: r.subtitle.clone(),
            category: r.category.clone(),
            action: r.action.clone(),
            icon_data_url: r.icon_data_url.clone(),
            slot: 0,
        }
    }
}
/// ⌥1…9, ⌥0, ⌥-, ⌥= (React `MAX_PINNED_COMMANDS`).
pub const MAX_PINS: usize = 12;
pub fn normalize_pins(pins: &mut Vec<Pin>) {
    pins.truncate(MAX_PINS);
    for (i, p) in pins.iter_mut().enumerate() {
        p.slot = i + 1;
    }
}
pub fn insert_pin(pins: &mut Vec<Pin>, r: &Row, index: Option<usize>) {
    pins.insert(index.unwrap_or(pins.len()).min(pins.len()), r.into());
    normalize_pins(pins);
}
pub fn reorder_pin(pins: &mut Vec<Pin>, id: &str, index: usize) {
    let Some(from) = pins.iter().position(|p| p.id == id) else {
        return;
    };
    let mut to = index.min(pins.len());
    let p = pins.remove(from);
    if from < to {
        to -= 1;
    }
    pins.insert(to, p);
    normalize_pins(pins);
}
pub fn pin_slot(code: &str, key: &str) -> Option<usize> {
    match code {
        "Minus" => return Some(11),
        "Equal" => return Some(12),
        _ => {}
    }
    let digit = code.strip_prefix("Digit").unwrap_or(key);
    if digit.len() != 1 {
        return None;
    }
    let n = digit.parse::<usize>().ok()?;
    (n <= 9).then_some(if n == 0 { 10 } else { n })
}
pub fn slot_key(slot: usize) -> String {
    match slot {
        10 => "0".into(),
        11 => "-".into(),
        12 => "=".into(),
        n => n.to_string(),
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalDefaults {
    pub save_for: String,
    pub keep_alive_for: String,
}
impl Default for TerminalDefaults {
    fn default() -> Self {
        Self {
            save_for: "week".into(),
            keep_alive_for: "3h".into(),
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    White,
    Dark,
    System,
}
impl Theme {
    pub fn resolved(self, system_dark: bool) -> Self {
        if self == Self::System {
            if system_dark {
                Self::Dark
            } else {
                Self::White
            }
        } else {
            self
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct UiState {
    pub version: u32,
    pub imported_legacy: bool,
    pub pins: Vec<Pin>,
    pub recent_extension_commands: Vec<String>,
    pub launcher_query_history: Vec<String>,
    pub terminal_defaults: TerminalDefaults,
    pub terminal_pinned_sessions: Vec<String>,
    pub recent_emojis: Vec<String>,
    pub currency: Preferences,
    pub theme: Theme,
    pub release_notes_version: Option<String>,
    pub last_update_check: Option<f64>,
    pub auto_update_enabled: bool,
    pub settings_tab: String,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}
impl Default for UiState {
    fn default() -> Self {
        Self {
            version: 1,
            imported_legacy: false,
            pins: vec![],
            recent_extension_commands: vec![],
            launcher_query_history: vec![],
            terminal_defaults: TerminalDefaults::default(),
            terminal_pinned_sessions: vec![],
            recent_emojis: vec![],
            currency: Preferences::default(),
            theme: Theme::System,
            release_notes_version: None,
            last_update_check: None,
            auto_update_enabled: false,
            settings_tab: "general".into(),
            unknown: BTreeMap::new(),
        }
    }
}
fn strings(value: &Value, cap: usize, predicate: impl Fn(&str) -> bool) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|s| s.len() <= 8192 && predicate(s))
        .take(cap)
        .map(String::from)
        .collect()
}
pub fn parse_pins(v: &Value) -> Vec<Pin> {
    let mut pins = vec![];
    for p in v.as_array().into_iter().flatten().take(1000) {
        let mut p = p.clone();
        if let Some(m) = p.as_object_mut() {
            m.insert("slot".into(), json!(1));
            for key in ["id", "title", "subtitle"] {
                let v = m
                    .get(key)
                    .filter(|v| !v.is_null())
                    .map(|v| {
                        if let Some(s) = v.as_str() {
                            s.to_owned()
                        } else {
                            v.to_string()
                        }
                    })
                    .unwrap_or_default();
                m.insert(key.into(), json!(v));
            }
        }
        if let Ok(mut p) = serde_json::from_value::<Pin>(p) {
            p.id = p.id.trim().into();
            p.title = p.title.trim().into();
            p.subtitle = p.subtitle.trim().into();
            if !p.id.is_empty() && !p.title.is_empty() {
                pins.push(p);
            }
        }
        if pins.len() == 12 {
            break;
        }
    }
    normalize_pins(&mut pins);
    pins
}
#[derive(Clone, Debug, PartialEq)]
pub struct ImportReport {
    pub state: UiState,
    pub rejected: Vec<String>,
    pub ignored_caches: Vec<String>,
    pub used_existing: bool,
}
impl UiState {
    pub fn record_launcher_query(&mut self, entry: &str) {
        self.launcher_query_history = vec![entry.into()];
    }

    /// Existing valid native state always wins. Corrupt existing state is an error to avoid overwriting it.
    pub fn import_legacy(
        existing: Option<&str>,
        legacy: &BTreeMap<String, String>,
    ) -> Result<ImportReport, String> {
        if let Some(s) = existing {
            let state = Self::decode(s)?;
            return Ok(ImportReport {
                state,
                rejected: vec![],
                ignored_caches: vec![],
                used_existing: true,
            });
        }
        let mut s = Self::default();
        let mut rejected = vec![];
        let mut ignored = vec![];
        for (k, v) in legacy {
            if !KEYS.contains(&k.as_str()) {
                continue;
            }
            if matches!(
                k.as_str(),
                "tezbar:home-search-results:v2" | "tezbar:search-candidates:v2"
            ) {
                ignored.push(k.clone());
                continue;
            }
            if s.import_key(k, v).is_err() {
                rejected.push(k.clone());
            }
        }
        s.imported_legacy = true;
        Ok(ImportReport {
            state: s,
            rejected,
            ignored_caches: ignored,
            used_existing: false,
        })
    }
    pub fn import_key(&mut self, k: &str, raw: &str) -> Result<(), String> {
        if raw.len() > 2 * 1024 * 1024 {
            return Err("State item exceeds 2 MiB".into());
        }
        let parsed = || serde_json::from_str::<Value>(raw).map_err(|e| e.to_string());
        match k {
            "tezbar:pinned-commands" => {
                let v = parsed()?;
                if !v.is_array() {
                    return Err("Pins must be an array".into());
                }
                self.pins = parse_pins(&v);
            }
            "tezbar:recent-extension-commands" => {
                self.recent_extension_commands = strings(&parsed()?, usize::MAX, |_| true)
                    .into_iter()
                    .map(|s| s.trim().to_owned())
                    .filter(|s| s.starts_with("extcmd:"))
                    .take(20)
                    .collect()
            }
            "tezbar:launcher-query-history:v1" => {
                self.launcher_query_history = strings(&parsed()?, 1, |s| !s.trim().is_empty())
            }
            "tezbar:terminal-pinned-sessions" => {
                self.terminal_pinned_sessions = strings(&parsed()?, 512, |_| true)
            }
            "tezbar:recent-emojis" => {
                self.recent_emojis = strings(&parsed()?, usize::MAX, |_| true)
                    .into_iter()
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
                    .take(24)
                    .collect()
            }
            "tezbar:terminal-defaults" => {
                let v = parsed()?;
                let save = v["saveFor"].as_str().unwrap_or("");
                let keep = v["keepAliveFor"].as_str().unwrap_or("");
                self.terminal_defaults = TerminalDefaults {
                    save_for: if ["day", "week", "month", "forever"].contains(&save) {
                        save
                    } else {
                        "week"
                    }
                    .into(),
                    keep_alive_for: if ["3h", "8h", "day", "until-stop"].contains(&keep) {
                        keep
                    } else {
                        "3h"
                    }
                    .into(),
                };
            }
            "tezbar:currency:prefs:v1" => {
                let v = parsed()?;
                let mut p = Preferences::default();
                p.pinned = v["pinned"].as_str().map(String::from);
                if let Some(usage) = v["usage"].as_object() {
                    for (k, v) in usage {
                        if let (Some(count), Some(last)) =
                            (v["count"].as_f64(), v["lastUsedAt"].as_f64())
                        {
                            if count.is_finite() && last.is_finite() {
                                p.usage.insert(
                                    k.clone(),
                                    tezbar_core::instant::currency::Usage {
                                        count: count.floor().max(0.) as u64,
                                        last_used_at: last.floor().max(0.) as u64,
                                    },
                                );
                            }
                        }
                    }
                }
                p.validate();
                self.currency = p;
            }
            "tezbar:theme" => {
                self.theme = match raw {
                    "white" => Theme::White,
                    "dark" => Theme::Dark,
                    "system" => Theme::System,
                    _ => return Err("Unknown theme".into()),
                }
            }
            "tezbar:release-notes-version" => self.release_notes_version = Some(raw.into()),
            "tezbar:last-update-check" => {
                self.last_update_check = crate::update_policy::read_last(Some(raw));
            }
            "tezbar:auto-update-enabled" => self.auto_update_enabled = raw == "true",
            "tezbar:settings-tab" => {
                self.settings_tab = if [
                    "general",
                    "ai",
                    "voice",
                    "knowledge",
                    "extensions",
                    "permissions",
                    "storage",
                    "advanced",
                ]
                .contains(&raw)
                {
                    raw
                } else {
                    "general"
                }
                .into()
            }
            "tezbar:home-search-results:v2" | "tezbar:search-candidates:v2" => {}
            _ => return Err("Not an importable browser key".into()),
        }
        Ok(())
    }
    pub fn export_legacy(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        macro_rules! j {
            ($k:expr,$v:expr) => {
                out.insert($k.into(), serde_json::to_string($v).unwrap());
            };
        }
        j!(KEYS[0], &self.pins);
        j!(KEYS[1], &self.recent_extension_commands);
        j!(KEYS[2], &self.launcher_query_history);
        j!(KEYS[5], &self.terminal_defaults);
        j!(KEYS[6], &self.terminal_pinned_sessions);
        j!(KEYS[7], &self.recent_emojis);
        j!(KEYS[8], &self.currency);
        out.insert(
            KEYS[9].into(),
            match self.theme {
                Theme::White => "white",
                Theme::Dark => "dark",
                Theme::System => "system",
            }
            .into(),
        );
        if let Some(v) = &self.release_notes_version {
            out.insert(KEYS[10].into(), v.clone());
        }
        if let Some(v) = self.last_update_check {
            out.insert(KEYS[11].into(), tezbar_core::instant::number(v));
        }
        out.insert(KEYS[12].into(), self.auto_update_enabled.to_string());
        out.insert(KEYS[13].into(), self.settings_tab.clone());
        out
    }
    pub fn decode(raw: &str) -> Result<Self, String> {
        if raw.len() > 4 * 1024 * 1024 {
            return Err("UI state exceeds 4 MiB".into());
        }
        let mut s: Self = serde_json::from_str(raw).map_err(|e| e.to_string())?;
        if s.version != 1 {
            return Err("Unsupported UI state version".into());
        }
        normalize_pins(&mut s.pins);
        s.recent_extension_commands.truncate(20);
        s.launcher_query_history.truncate(1);
        s.terminal_pinned_sessions.truncate(512);
        s.recent_emojis.truncate(24);
        s.currency.validate();
        if s.last_update_check.is_some_and(|v| !v.is_finite()) {
            s.last_update_check = None;
        }
        Ok(s)
    }
    pub fn encode(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }
    /// Explicit adapter seam: caller chooses the path. No profile discovery or global path lookup.
    pub fn write_atomic(&self, path: &Path) -> Result<(), String> {
        self.write_atomic_mode(path, false)
    }
    /// Bootstrap-only no-replace commit; the marker and state are one durable object.
    pub fn write_atomic_if_absent(&self, path: &Path) -> Result<(), String> {
        self.write_atomic_mode(path, true)
    }
    fn write_atomic_mode(&self, path: &Path, absent: bool) -> Result<(), String> {
        use std::io::Write;
        let parent = path.parent().ok_or("Missing state parent directory")?;
        let encoded = self.encode()?;
        if encoded.len() > 4 * 1024 * 1024 {
            return Err("UI state exceeds 4 MiB".into());
        }
        let (temporary, mut file) = temp_file(parent)?;
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let cleanup = Cleanup(temporary);
        file.write_all(encoded.as_bytes())
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        if absent {
            // link is atomic and fails if another writer created the destination. Never replace it.
            std::fs::hard_link(&cleanup.0, path).map_err(|e| e.to_string())?;
        } else {
            rename_with_retry(|| std::fs::rename(&cleanup.0, path), cfg!(windows))?;
        }
        #[cfg(unix)]
        std::fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
fn temp_file(parent: &Path) -> Result<(std::path::PathBuf, std::fs::File), String> {
    use std::fs::OpenOptions;
    for n in 0..100 {
        let path = parent.join(format!(".ui-state-{}-{n}.tmp", std::process::id()));
        let mut o = OpenOptions::new();
        o.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600);
        }
        match o.open(&path) {
            Ok(f) => return Ok((path, f)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Unable to acquire UI state temporary file".into())
}
#[cfg(test)]
mod tests;

// Windows may temporarily deny replacement while an antivirus/reader holds a handle.
// Retry only sharing/lock violations, bounded to 150 ms; never delete the destination.
fn rename_with_retry(
    mut rename: impl FnMut() -> std::io::Result<()>,
    windows: bool,
) -> Result<(), String> {
    for attempt in 0..=5 {
        match rename() {
            Ok(()) => return Ok(()),
            Err(e) if windows && matches!(e.raw_os_error(), Some(32 | 33)) && attempt < 5 => {
                std::thread::sleep(std::time::Duration::from_millis(10 * (attempt + 1)))
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    unreachable!()
}
