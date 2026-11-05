use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub version: String,
    #[serde(flatten)]
    pub metadata: serde_json::Map<String, Value>,
}
impl Entry {
    pub fn normalize_installed(mut self) -> Self {
        if self.author.is_empty() {
            self.author = "Raycast Community".into();
        }
        if self
            .metadata
            .get("iconUrl")
            .is_none_or(|v| v.as_str().is_none_or(str::is_empty))
        {
            if let Some(icon) = self.metadata.get("iconPath").cloned() {
                self.metadata.insert("iconUrl".into(), icon);
            }
        }
        self
    }
    pub fn effective_runtime(&self, platform: &str) -> Option<&'static str> {
        if self.metadata.get("runtime") == Some(&json!("javascript")) {
            return Some("javascript");
        }
        let n = self.metadata.get("nativeV2")?;
        let commands = n["commands"].as_array()?;
        let total = n["totalCommands"].as_u64()? as usize;
        if total == 0 || total != commands.len() {
            return None;
        }
        let rust = commands
            .iter()
            .filter(|c| c["runtime"] == "rust" && on_platform(c, platform))
            .count();
        Some(if rust == total {
            "rust"
        } else if rust > 0 {
            "mixed"
        } else {
            "javascript"
        })
    }
    pub fn runtime_description(&self, platform: &str) -> Option<(String, String)> {
        let kind = self.effective_runtime(platform)?;
        let n = self.metadata.get("nativeV2");
        let commands = n.and_then(|n| n["commands"].as_array());
        let total = commands.map_or(0, Vec::len);
        let rust = commands
            .into_iter()
            .flatten()
            .filter(|c| c["runtime"] == "rust" && on_platform(c, platform))
            .count();
        let js = commands
            .into_iter()
            .flatten()
            .filter(|c| c["runtime"] == "javascript" && on_platform(c, platform))
            .count();
        Some(match kind {
            "rust" => (
                "Rust".into(),
                format!("Rust — all {total} commands run natively"),
            ),
            "mixed" => {
                let missing = total - rust - js;
                let mut parts = vec![];
                if js > 0 {
                    parts.push(format!("{js} in JavaScript"));
                }
                if missing > 0 {
                    parts.push(format!("{missing} not available on {platform}"));
                }
                (
                    format!("Rust {rust} of {total}"),
                    format!(
                        "Mixed — {rust} of {total} commands run in Rust, {}",
                        parts.join(", ")
                    ),
                )
            }
            _ => (
                if n.is_some() && self.metadata.get("runtime") != Some(&json!("javascript")) {
                    format!("Rust {rust} of {total}")
                } else {
                    "JavaScript".into()
                },
                if n.is_some() && js == 0 {
                    format!("No commands available on {platform}")
                } else {
                    "JavaScript — runs in the compatibility worker".into()
                },
            ),
        })
    }
    pub fn can_install(&self, platform: &str) -> bool {
        if self
            .metadata
            .get("acquisition")
            .is_some_and(|a| a["type"] != "remote")
        {
            return false;
        }
        let Some(n) = self.metadata.get("nativeV2") else {
            return true;
        };
        if n["id"] != self.id {
            return true;
        }
        n["status"] != "unavailable"
            && n["commands"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|c| c["runtime"] != "unavailable" && on_platform(c, platform))
    }
    pub fn acquisition_guidance(&self) -> Option<&'static str> {
        let a = self.metadata.get("acquisition")?;
        if a["type"] == "remote" {
            None
        } else if a["type"] == "local-import-only" {
            Some("Import the trusted release manifest and ZIP with --extensions-v3-import, then close and reopen Tezbar. Use local import again to update or repair.")
        } else {
            Some("Remote installation is unavailable: unsupported acquisition rule.")
        }
    }
    pub fn checked_lines(
        &self,
        current_platform: &str,
        date: impl Fn(u64) -> String,
    ) -> Vec<String> {
        let n = self.metadata.get("nativeV2");
        let command_platforms = n
            .and_then(|n| n["commands"].as_array())
            .into_iter()
            .flatten()
            .flat_map(|c| {
                c["platforms"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
            })
            .collect::<BTreeSet<_>>();
        let declared = self
            .metadata
            .get("platforms")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<BTreeSet<_>>();
        let p = if command_platforms.is_empty() {
            declared
        } else {
            command_platforms
        };
        let mut platforms = ["macOS", "Windows"]
            .into_iter()
            .filter(|v| p.contains(v))
            .collect::<Vec<_>>();
        platforms.sort_by_key(|p| *p != current_platform);
        platforms
            .into_iter()
            .map(|p| {
                let latest = n
                    .and_then(|n| n["verification"].as_array())
                    .into_iter()
                    .flatten()
                    .filter(|c| {
                        c["platform"] == p
                            && c["state"] != "not-run"
                            && c["checkedAt"].as_u64().is_some_and(|t| t > 0)
                    })
                    .max_by_key(|c| c["checkedAt"].as_u64());
                match latest {
                    None => format!("{p} — not checked yet"),
                    Some(c) => {
                        let day = date(c["checkedAt"].as_u64().unwrap());
                        if c["state"] == "failed" {
                            format!(
                                "{p} — failed {day}{}",
                                c["note"]
                                    .as_str()
                                    .map(|s| format!(": {s}"))
                                    .unwrap_or_default()
                            )
                        } else {
                            format!(
                                "{p} — passed {day}{}",
                                c["method"]
                                    .as_str()
                                    .map(|s| format!(" ({s})"))
                                    .unwrap_or_default()
                            )
                        }
                    }
                }
            })
            .collect()
    }
    pub fn commands(&self) -> Vec<Value> {
        let mut commands = self
            .metadata
            .get("commands")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|c| c["name"].as_str().is_some() || c["title"].as_str().is_some())
            .collect::<Vec<_>>();
        if let Some(native) = self
            .metadata
            .get("nativeV2")
            .and_then(|n| n["commands"].as_array())
        {
            for c in native {
                if let Some(old) = commands.iter_mut().find(|old| old["name"] == c["name"]) {
                    old["native"] = c.clone();
                } else {
                    let mut item = c.clone();
                    item["native"] = c.clone();
                    commands.push(item);
                }
            }
        }
        commands
    }
}
fn on_platform(c: &Value, p: &str) -> bool {
    c["platforms"]
        .as_array()
        .is_some_and(|a| a.iter().any(|v| v == p))
}
pub fn command_status(command: &Value, platform: &str) -> String {
    if command["runtime"] == "unavailable" || !on_platform(command, platform) {
        format!("Not on {platform}")
    } else if command["runtime"] == "rust" {
        "Rust".into()
    } else {
        "JavaScript".into()
    }
}
pub fn merge_catalog(store: &[Entry], installed: &[Entry], query: &str) -> Vec<Entry> {
    let mut rows = store.to_vec();
    let needle = query.trim().to_lowercase();
    for local in installed {
        if let Some(row) = rows.iter_mut().find(|r| r.id == local.id) {
            for key in [
                "runtime",
                "acquisition",
                "nativeV2",
                "platforms",
                "commands",
            ] {
                if let Some(v) = local.metadata.get(key) {
                    if key != "commands" || v.as_array().is_some_and(|a| !a.is_empty()) {
                        row.metadata.insert(key.into(), v.clone());
                    }
                }
            }
            if local.metadata.get("runtime") == Some(&json!("javascript"))
                && !local.metadata.contains_key("nativeV2")
            {
                row.metadata.remove("nativeV2");
            }
        } else if [&local.id, &local.name, &local.description, &local.author]
            .iter()
            .any(|s| s.to_lowercase().contains(&needle))
        {
            rows.push(local.clone());
        }
    }
    rows
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Search {
        token: u64,
        query: String,
        offset: usize,
        limit: usize,
    },
    Install(String),
    Remove(String),
    ReadInstalled,
}
#[derive(Default)]
pub struct Catalog {
    pub query: String,
    pub loading: bool,
    pub loading_more: bool,
    pub store: Vec<Entry>,
    pub installed: Vec<Entry>,
    pub selected: Option<String>,
    pub follow_selection: bool,
    pub installing: BTreeMap<String, u8>,
    pub message: Option<String>,
    pub total: usize,
    pub has_more: bool,
    next: u64,
    request: Option<(u64, bool)>,
}
impl Catalog {
    pub fn search(&mut self, query: String, more: bool) -> Option<Effect> {
        if more && (self.loading || self.loading_more || !self.has_more) {
            return None;
        }
        self.query = query.clone();
        self.next += 1;
        self.request = Some((self.next, more));
        self.loading = !more;
        self.loading_more = more;
        if url::Url::parse(&query).is_ok_and(|u| u.host_str() == Some("github.com")) {
            self.store.clear();
            self.loading = false;
            self.loading_more = false;
            self.request = None;
            self.has_more = false;
            self.total = 0;
            return None;
        }
        Some(Effect::Search {
            token: self.next,
            query,
            offset: if more { self.store.len() } else { 0 },
            limit: 25,
        })
    }
    pub fn loaded(
        &mut self,
        token: u64,
        result: Result<(Vec<Entry>, Vec<Entry>, usize, bool), String>,
        select_next: bool,
    ) {
        let Some((t, more)) = self.request else {
            return;
        };
        if t != token {
            return;
        }
        self.request = None;
        self.loading = false;
        self.loading_more = false;
        match result {
            Ok((items, installed, total, has_more)) => {
                if more {
                    if select_next {
                        self.selected =
                            items.first().map(|r| r.id.clone()).or(self.selected.take());
                        self.follow_selection = true;
                    }
                    self.store.extend(items);
                } else {
                    self.store = items;
                    self.installed = installed;
                }
                self.total = total;
                self.has_more = has_more;
            }
            Err(e) => self.message = Some(e),
        }
    }
    pub fn install(&mut self, id: &str, platform: &str) -> Result<Effect, &'static str> {
        let e = self
            .store
            .iter()
            .chain(self.installed.iter())
            .find(|r| r.id == id)
            .ok_or("No extension selected")?;
        if !e.can_install(platform) {
            return Err("Remote installation is unavailable");
        }
        self.installing.insert(id.into(), 5);
        Ok(Effect::Install(id.into()))
    }
    pub fn progress(&mut self, id: &str, progress: u8) {
        if progress >= 100 {
            self.installing.remove(id);
        } else {
            self.installing.insert(id.into(), progress);
        }
    }
    pub fn installed(&mut self, id: &str, message: String) {
        self.installing.remove(id);
        self.message = Some(message);
    }
}
#[cfg(test)]
mod tests;
/// Visible catalog refresh scheduling, including coalesced store-change notifications.
#[derive(Default)]
pub struct VisibleRefresh {
    visible: bool,
    stopped: bool,
    event_at: Option<u64>,
    poll_at: u64,
}
impl VisibleRefresh {
    pub fn start(&mut self, now: u64) -> bool {
        self.visible = true;
        self.stopped = false;
        self.poll_at = now + 20_000;
        true
    }
    pub fn changed(&mut self, now: u64) {
        if !self.stopped && self.visible && self.event_at.is_none() {
            self.event_at = Some(now + 150);
        }
    }
    pub fn visibility(&mut self, visible: bool, now: u64) -> bool {
        self.visible = visible;
        self.event_at = None;
        if visible && !self.stopped {
            self.poll_at = now + 20_000;
            true
        } else {
            false
        }
    }
    pub fn tick(&mut self, now: u64) -> bool {
        if self.stopped || !self.visible {
            return false;
        }
        if self.event_at.is_some_and(|t| now >= t) || now >= self.poll_at {
            self.event_at = None;
            self.poll_at = now + 20_000;
            true
        } else {
            false
        }
    }
    pub fn stop(&mut self) {
        self.stopped = true;
        self.event_at = None;
    }
}
