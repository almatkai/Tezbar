//! Launcher presentation policy. Core owns ranking and search; this module owns event generations.
pub mod session;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
pub use session::SearchSession;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};
use tezbar_core::launcher::text;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Action {
    OpenApp {
        #[serde(rename = "appName")]
        app_name: String,
        #[serde(rename = "appPath", default)]
        app_path: Option<String>,
    },
    OpenFile {
        path: String,
    },
    OpenUrl {
        url: String,
    },
    OpenWithApp {
        path: String,
        #[serde(rename = "appName", default)]
        app_name: Option<String>,
    },
    CopyText {
        text: String,
    },
    CopyAndPasteText {
        text: String,
    },
    AddNote {
        text: String,
    },
    RunExtensionCommand {
        #[serde(rename = "extensionId")]
        extension_id: String,
        #[serde(rename = "commandName")]
        command_name: String,
        #[serde(flatten)]
        extra: HashMap<String, Value>,
    },
    InvokeCommand {
        #[serde(rename = "commandId")]
        command_id: String,
        #[serde(default)]
        payload: Value,
    },
    RunNativeCommand {
        #[serde(rename = "commandId")]
        command_id: String,
    },
    RunShell {
        command: String,
    },
    InstallExtension {
        #[serde(flatten)]
        extra: HashMap<String, Value>,
    },
    OpenVaultItem {
        #[serde(rename = "id")]
        item_id: String,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    pub category: String,
    #[serde(default)]
    pub score: f64,
    pub action: Action,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_data_url: Option<String>,
    #[serde(default)]
    pub ephemeral: bool,
}
impl Row {
    pub fn pinnable(&self) -> bool {
        !matches!(self.category.as_str(), "calculator" | "color-converter")
            && !self.id.starts_with("deep-search:")
    }
    pub fn command(id: &str, title: &str) -> Self {
        Self {
            id: format!("command:{id}"),
            title: title.into(),
            subtitle: String::new(),
            category: "commands".into(),
            score: 0.,
            action: Action::InvokeCommand {
                command_id: id.into(),
                payload: Value::Null,
            },
            icon_data_url: None,
            ephemeral: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Basic,
    Deep,
    Terminal,
    Ai,
    Path,
    Applications,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputMode {
    pub parsed_search_query: ParsedSearchQuery,
    pub is_deep_search_mode: bool,
    pub slash_query: String,
    pub is_slash_input: bool,
    pub is_application_input: bool,
    pub is_completion_input: bool,
    pub mode: Mode,
    pub query: String,
    pub ai_task: String,
    pub ai_working_directory: Option<String>,
}
pub fn input_mode(value: &str, terminal: bool) -> InputMode {
    let deep = value.starts_with('!');
    let sep = value.find("  ");
    let dir = sep.is_some_and(|i| value[..i].trim_start().starts_with('/'));
    let ai = !terminal && !deep && (value.starts_with(' ') || value.ends_with("  ") || dir);
    let mut task = String::new();
    let mut cwd = None;
    if ai {
        if value.starts_with(' ') {
            task = value.trim().into();
        } else {
            let i = sep.unwrap();
            let prefix = value[..i].trim();
            let suffix = value[i + 2..].trim();
            if dir {
                cwd = Some(prefix.into());
                task = suffix.into();
            } else {
                task = format!("{prefix} {suffix}").trim().into();
            }
        }
    }
    let mode = if terminal {
        Mode::Terminal
    } else if deep {
        Mode::Deep
    } else if ai {
        Mode::Ai
    } else if value.trim_start().starts_with('/') {
        Mode::Path
    } else if value.trim_start().starts_with('`') {
        Mode::Applications
    } else {
        Mode::Basic
    };
    let slash = !terminal && value.trim_start().starts_with('/');
    let app = !terminal && value.trim_start().starts_with('`');
    InputMode {
        parsed_search_query: parse_search_query(value),
        is_deep_search_mode: !terminal && deep,
        slash_query: value.trim_start().into(),
        is_slash_input: slash,
        is_application_input: app,
        is_completion_input: !terminal && !ai && (slash || app),
        query: if deep {
            value[1..].trim().into()
        } else {
            value.trim().into()
        },
        mode,
        ai_task: task,
        ai_working_directory: cwd,
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AiBoot {
    NewChat {
        #[serde(rename = "workingDirectory")]
        working_directory: String,
    },
    Submit {
        prompt: String,
        #[serde(rename = "workingDirectory", skip_serializing_if = "Option::is_none")]
        working_directory: Option<String>,
    },
}
pub fn ai_boot(m: &InputMode, resolved: Option<String>) -> Option<AiBoot> {
    if m.ai_working_directory.is_some() && resolved.is_some() && m.ai_task.is_empty() {
        Some(AiBoot::NewChat {
            working_directory: resolved.unwrap(),
        })
    } else if !m.ai_task.is_empty() {
        Some(AiBoot::Submit {
            prompt: m.ai_task.clone(),
            working_directory: resolved,
        })
    } else {
        None
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ParsedSearchQuery {
    pub mode: &'static str,
    pub query: String,
}
pub fn parse_search_query(s: &str) -> ParsedSearchQuery {
    let deep = s.starts_with('!');
    ParsedSearchQuery {
        mode: if deep { "deep" } else { "basic" },
        query: if deep { s[1..].trim() } else { s.trim() }.into(),
    }
}
pub fn deep_search_input(s: &str) -> String {
    format!("!{}", s.trim())
}
pub fn deep_search_draft_input(s: &str) -> String {
    format!("!{s}")
}
pub fn request_input(s: &str) -> String {
    let m = input_mode(s, false);
    if m.mode == Mode::Deep {
        format!("!{}", m.query)
    } else {
        s.trim().into()
    }
}
pub fn history_entry(s: &str, terminal: bool) -> Option<String> {
    if terminal || s.trim().is_empty() {
        return None;
    }
    let m = input_mode(s, false);
    Some(if m.mode == Mode::Ai {
        if let Some(cwd) = m.ai_working_directory {
            format!("{cwd}  {}", m.ai_task).trim_end().into()
        } else {
            format!(" {}", s.trim())
        }
    } else {
        s.trim().into()
    })
}
pub fn should_recall(key: &str, value: &str, deep: bool, selected: isize, count: usize) -> bool {
    key == "ArrowUp" && value.is_empty() && !deep && (count == 0 || selected <= 0)
}
#[derive(Clone, Debug, PartialEq)]
pub struct IntegerValidation {
    pub min: u64,
    pub max: u64,
    pub max_length: usize,
    pub message: String,
}
pub fn sanitize_argument(s: &str, v: Option<&IntegerValidation>) -> String {
    match v {
        Some(v) => s
            .chars()
            .filter(char::is_ascii_digit)
            .take(v.max_length)
            .collect(),
        None => s.into(),
    }
}
pub fn validate_argument(s: &str, v: Option<&IntegerValidation>) -> Result<(), String> {
    if let Some(v) = v {
        if !s.is_empty()
            && s.len() <= v.max_length
            && s.bytes().all(|b| b.is_ascii_digit())
            && s.parse::<u64>()
                .is_ok_and(|n| n <= 9_007_199_254_740_991 && n >= v.min && n <= v.max)
        {
            Ok(())
        } else {
            Err(v.message.clone())
        }
    } else if s.is_empty() {
        Err("Enter an argument".into())
    } else {
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct ArgumentField {
    pub name: String,
    pub required: bool,
    pub validation: Option<IntegerValidation>,
    pub choices: Vec<(String, String)>,
}
#[derive(Clone, Debug)]
pub struct Arguments {
    pub row_id: String,
    pub fields: Vec<ArgumentField>,
    pub values: Vec<String>,
    pub selected: usize,
}
impl Arguments {
    pub fn new(row_id: String, fields: Vec<ArgumentField>) -> Self {
        Self {
            row_id,
            values: vec![String::new(); fields.len()],
            fields,
            selected: 0,
        }
    }
    pub fn edit(&mut self, s: &str) {
        if let Some(f) = self.fields.get(self.selected) {
            self.values[self.selected] = sanitize_argument(s, f.validation.as_ref());
        }
    }
    pub fn tab(&mut self, back: bool) {
        if !self.fields.is_empty() {
            self.selected = if back {
                self.selected.saturating_sub(1)
            } else {
                (self.selected + 1).min(self.fields.len() - 1)
            };
        }
    }
    pub fn complete(&mut self, index: usize) -> bool {
        let Some((_, v)) = self
            .fields
            .get(self.selected)
            .and_then(|f| f.choices.get(index))
        else {
            return false;
        };
        self.values[self.selected] = v.clone();
        true
    }
    pub fn submit(&self) -> Result<HashMap<String, String>, String> {
        let mut out = HashMap::new();
        for (f, v) in self.fields.iter().zip(&self.values) {
            if f.required || !v.is_empty() {
                validate_argument(v, f.validation.as_ref())?;
            }
            out.insert(f.name.clone(), v.clone());
        }
        Ok(out)
    }
}
/// Stable merge; incoming replacements keep their first insertion position. No icon copies during filtering.
pub fn merge_candidates(current: &[Arc<Row>], incoming: &[Arc<Row>], cap: usize) -> Vec<Arc<Row>> {
    let mut out = vec![];
    let mut index = HashMap::new();
    for r in incoming.iter().filter(|r| !r.ephemeral) {
        if let Some(&i) = index.get(&r.id) {
            out[i] = Arc::clone(r);
        } else if out.len() < cap {
            index.insert(r.id.clone(), out.len());
            out.push(Arc::clone(r));
        }
    }
    for r in current.iter().filter(|r| !r.ephemeral) {
        if out.len() >= cap {
            break;
        }
        if !index.contains_key(&r.id) {
            index.insert(r.id.clone(), out.len());
            out.push(Arc::clone(r));
        }
    }
    out
}
pub fn optimistic<'a>(rows: &'a [Arc<Row>], query: &str) -> Vec<&'a Arc<Row>> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return rows.iter().collect();
    }
    let variants = text::variants(&q);
    let mut hits: Vec<_> = rows
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let haystack = format!("{} {}", r.title, r.subtitle).to_lowercase();
            let mut score: f64 = -1.;
            for (j, v) in variants.iter().enumerate() {
                let terms = text::tokens(v);
                let app = if r.category == "applications" {
                    text::app_match(&r.title, v)
                } else {
                    0.
                };
                if terms.is_empty() || app <= 0. && !terms.iter().all(|t| haystack.contains(t)) {
                    continue;
                }
                let title = r.title.trim().to_lowercase();
                let sub = r.subtitle.to_lowercase();
                let tokens = text::tokens(&title);
                let lexical = if title == *v {
                    1000.
                } else if title.starts_with(v) {
                    900.
                } else if tokens.contains(v) {
                    850.
                } else if title.contains(v) {
                    750.
                } else if sub.contains(v) {
                    700.
                } else {
                    600.
                };
                let query = text::Query::new(v);
                let relevance = (lexical
                    + text::surface_boost(&r.category, &r.title, &r.subtitle, &query))
                .max(app * 1600.);
                score = score.max(relevance * if j == 0 { 1. } else { 0.9 });
            }
            (score >= 0.).then_some((score, i, r))
        })
        .collect();
    hits.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then(b.2.score.total_cmp(&a.2.score))
            .then(a.1.cmp(&b.1))
    });
    hits.into_iter().map(|(_, _, r)| r).collect()
}
pub fn good_metadata(query: &str, rows: &[Arc<Row>]) -> bool {
    let q = query.trim().to_lowercase();
    if q.encode_utf16().count() < 2 || text::tokens(&q).is_empty() {
        return true;
    }
    let variants = text::variants(&q);
    rows.iter().take(10).any(|r| {
        if r.category == "knowledge"
            || r.id.starts_with("deep-search:")
            || r.id.starts_with("note-add:")
        {
            return false;
        }
        let title = r.title.trim().to_lowercase();
        let meta = format!("{title} {}", r.subtitle.trim().to_lowercase());
        let mt = text::tokens(&meta);
        variants.iter().any(|q| {
            if r.category == "applications" && text::app_match(&title, q) > 0.
                || title == *q
                || title.starts_with(q)
                || meta.contains(q)
            {
                return true;
            }
            let terms = text::tokens(q);
            !terms.is_empty()
                && terms.iter().all(|q| {
                    mt.iter().any(|m| {
                        m == q
                            || m.starts_with(q)
                            || q.starts_with(m)
                                && m.encode_utf16().count()
                                    >= 3.max((q.encode_utf16().count() * 7).div_ceil(10))
                    })
                })
                && (terms.len() > 1 || r.score >= 650.)
        })
    })
}
pub fn deep_recommendation(query: &str, rows: &[Arc<Row>]) -> Option<Row> {
    let q = query.trim();
    if q.encode_utf16().count() < 3 || good_metadata(q, rows) {
        return None;
    }
    let mut r = Row::command(
        "activate-deep-search",
        &format!("Deep Search “{}”", q.chars().take(72).collect::<String>()),
    );
    r.id = format!(
        "deep-search:{}",
        tezbar_core::instant::transform::url_encode(&q.to_lowercase()).ok()?
    );
    r.category = "knowledge".into();
    r.subtitle = "No strong metadata match · Search inside indexed file contents".into();
    r.score = rows.iter().fold(1200f64, |v, r| v.max(r.score + 1.));
    r.action = Action::InvokeCommand {
        command_id: "activate-deep-search".into(),
        payload: json!({"query":q}),
    };
    Some(r)
}
#[derive(Debug, Default)]
pub struct ResultCache {
    home: Vec<Arc<Row>>,
    candidates: Vec<Arc<Row>>,
    queries: VecDeque<(String, Vec<Arc<Row>>)>,
}
impl ResultCache {
    pub fn accept(&mut self, q: &str, rows: Vec<Arc<Row>>) {
        self.candidates = merge_candidates(&self.candidates, &rows, 1000);
        if q.is_empty() {
            self.home = rows
                .iter()
                .filter(|r| !r.ephemeral && r.category != "knowledge")
                .take(40)
                .cloned()
                .collect();
        }
        self.queries.retain(|(k, _)| k != q);
        self.queries
            .push_back((q.into(), rows.into_iter().take(100).collect()));
        while self.queries.len() > 24 {
            self.queries.pop_front();
        }
    }
    pub fn preview(&self, q: &str) -> Vec<Arc<Row>> {
        if q.is_empty() {
            return self.home.clone();
        }
        if let Some((_, r)) = self.queries.iter().find(|(k, _)| k == q) {
            return r.clone();
        }
        optimistic(&self.candidates, q)
            .into_iter()
            .take(100)
            .cloned()
            .collect()
    }
    pub fn invalidate(&mut self) {
        self.home.clear();
        self.candidates.clear();
        self.queries.clear();
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KeyDecision {
    Select(isize),
    Activate,
    Secondary,
    Complete { back: bool },
    QuickLook,
    Pin,
    History,
    Recall,
    EnterTerminal,
    LeaveMode,
    Clear,
    Hide,
    Ignore,
}
pub fn key_decision(
    key: &str,
    primary: bool,
    shift: bool,
    value: &str,
    mode: &Mode,
    selected: isize,
    count: usize,
    composing: bool,
) -> KeyDecision {
    if composing {
        return KeyDecision::Ignore;
    }
    if primary {
        return match key {
            "Enter" => KeyDecision::Secondary,
            "p" => KeyDecision::Pin,
            "h" => KeyDecision::History,
            "Escape" => KeyDecision::Hide,
            _ => KeyDecision::Ignore,
        };
    }
    match key {
        "Escape" => {
            if !value.is_empty() && !matches!(value, "!" | "`" | ">" | " ") {
                KeyDecision::Clear
            } else if *mode != Mode::Basic {
                KeyDecision::LeaveMode
            } else {
                KeyDecision::Hide
            }
        }
        "Backspace" if value.is_empty() && *mode != Mode::Basic => KeyDecision::LeaveMode,
        "ArrowUp" if should_recall(key, value, *mode == Mode::Deep, selected, count) => {
            KeyDecision::Recall
        }
        "ArrowUp" => KeyDecision::Select(if *mode == Mode::Applications { -7 } else { -1 }),
        "ArrowDown" => KeyDecision::Select(if *mode == Mode::Applications { 7 } else { 1 }),
        "ArrowLeft" if *mode == Mode::Applications => KeyDecision::Select(-1),
        "ArrowRight" if *mode == Mode::Applications => KeyDecision::Select(1),
        "Enter" => KeyDecision::Activate,
        "Tab" => KeyDecision::Complete { back: shift },
        " " => KeyDecision::QuickLook,
        ">" if value.is_empty() => KeyDecision::EnterTerminal,
        _ => KeyDecision::Ignore,
    }
}
/// Mounted owner token prevents an old surface from unregistering its successor.
#[derive(Debug, Default)]
pub struct EscapeGate {
    owner: Option<u64>,
}
impl EscapeGate {
    pub fn mount(&mut self, id: u64) {
        self.owner = Some(id);
    }
    pub fn unmount(&mut self, id: u64) {
        if self.owner == Some(id) {
            self.owner = None;
        }
    }
    pub fn consumes(&self, id: u64, decision: KeyDecision) -> bool {
        self.owner == Some(id) && !matches!(decision, KeyDecision::Hide | KeyDecision::Ignore)
    }
}
pub fn release_notes_row(rows: &mut Vec<Arc<Row>>, query: &str, unread: bool, version: &str) {
    let q = query.trim().to_lowercase();
    if !(if q.is_empty() {
        unread
    } else {
        [
            "show updated version",
            "release notes",
            "what's new",
            "tezbar version",
        ]
        .iter()
        .any(|s| s.contains(&q))
    }) {
        return;
    }
    rows.retain(|r| r.id != "command:show-updated-version");
    let mut r = Row::command("show-updated-version", "Release Notes");
    r.subtitle = format!("Tezbar v{version} · Press Enter to read what’s new");
    r.score = 1_000_001.;
    rows.insert(0, Arc::new(r));
}
/// De-duplicate per-session recent identities without changing persisted identifiers.
pub fn record_recent(history: &mut Vec<String>, id: &str, cap: usize) {
    history.retain(|s| s != id);
    history.insert(0, id.into());
    history.truncate(cap);
}
pub fn stable_unique(rows: &[Arc<Row>]) -> bool {
    let mut ids = HashSet::new();
    rows.iter().all(|r| ids.insert(&r.id))
}
pub mod controller;
#[cfg(test)]
mod ts_cases;
pub fn quick_look_path(row: &Row) -> Option<&str> {
    let Action::OpenFile { path } = &row.action else {
        return None;
    };
    let path = path.trim();
    let name = path.rsplit(['/', '\\']).next()?;
    let ext = name.rsplit_once('.')?.1.to_lowercase();
    [
        "avif", "aiff", "avi", "bmp", "csv", "doc", "docx", "flac", "gif", "heic", "heif", "htm",
        "html", "ico", "jpeg", "jpg", "jfif", "key", "m4a", "m4v", "md", "mov", "mp3", "mp4",
        "numbers", "odp", "ods", "odt", "pages", "pdf", "png", "ppt", "pptx", "rtf", "rtfd", "svg",
        "tsv", "tif", "tiff", "txt", "wav", "webm", "webp", "xls", "xlsm", "xlsx",
    ]
    .contains(&ext.as_str())
    .then_some(path)
}
pub fn quick_look_paths(rows: &[Arc<Row>], selected: usize) -> Vec<&str> {
    if rows
        .get(selected)
        .and_then(|r| quick_look_path(r))
        .is_none()
    {
        return vec![];
    }
    let mut seen = HashSet::new();
    rows[selected..]
        .iter()
        .chain(&rows[..selected])
        .filter_map(|r| quick_look_path(r))
        .filter(|p| seen.insert(*p))
        .collect()
}
pub fn can_quick_look(count: usize, navigation: bool, path: Option<&str>) -> bool {
    path.is_some() && (navigation || count == 1)
}
pub fn toggle_deep_navigation(active: bool, count: usize) -> (bool, isize) {
    if active || count == 0 {
        (false, -1)
    } else {
        (true, 0)
    }
}
#[cfg(test)]
mod preview_tests {
    use super::*;
    fn file(path: &str) -> Arc<Row> {
        let mut r = Row::command("file", "file");
        r.action = Action::OpenFile { path: path.into() };
        Arc::new(r)
    }
    #[test]
    fn returns_only_previewable_file_paths() {
        for p in ["/tmp/report.pdf", "/tmp/review.docx", "/tmp/scan.PDF"] {
            assert_eq!(quick_look_path(&file(p)), Some(p));
        }
        assert!(quick_look_path(&file("/tmp/archive.zip")).is_none());
        assert!(quick_look_path(&Row::command("x", "x")).is_none());
    }
    #[test]
    fn sole_result_needs_no_navigation() {
        assert!(can_quick_look(1, false, Some("x")));
        assert!(!can_quick_look(2, false, Some("x")));
        assert!(can_quick_look(2, true, Some("x")));
        assert!(!can_quick_look(1, false, None));
    }
    #[test]
    fn toggle_deep_navigation_state() {
        assert_eq!(toggle_deep_navigation(false, 2), (true, 0));
        assert_eq!(toggle_deep_navigation(true, 2), (false, -1));
        assert_eq!(toggle_deep_navigation(false, 0), (false, -1));
    }
    #[test]
    fn preview_navigation_starts_at_selection() {
        let r = vec![
            file("/tmp/first.jpg"),
            file("/tmp/archive.zip"),
            file("/tmp/second.pdf"),
            file("/tmp/third.png"),
        ];
        assert_eq!(
            quick_look_paths(&r, 2),
            ["/tmp/second.pdf", "/tmp/third.png", "/tmp/first.jpg"]
        );
        assert!(quick_look_paths(&r, 1).is_empty());
    }
}
pub fn is_plain_space(key: &str, code: &str, primary: bool, alt: bool, shift: bool) -> bool {
    (key == " " || code == "Space") && !primary && !alt && !shift
}
#[cfg(test)]
mod notes_row_tests {
    use super::*;
    #[test]
    fn unread_notes_first_ahead_of_updater() {
        let mut r = vec![Arc::new(Row::command(
            "check-for-updates",
            "Check for Updates",
        ))];
        release_notes_row(&mut r, "", true, "0.2.2");
        assert_eq!(r[0].title, "Release Notes");
        assert_eq!(r[1].title, "Check for Updates");
        assert!(r[0].subtitle.contains("v0.2.2"));
    }
    #[test]
    fn home_unchanged_after_read() {
        let mut r = vec![Arc::new(Row::command("x", "x"))];
        let old = r.clone();
        release_notes_row(&mut r, "", false, "1");
        assert_eq!(r, old);
    }
    #[test]
    fn searchable_after_read() {
        for q in [
            "show updated version",
            "release notes",
            "what's new",
            "version",
        ] {
            let mut r = vec![];
            release_notes_row(&mut r, q, false, "1");
            assert_eq!(r[0].title, "Release Notes");
        }
    }
    #[test]
    fn unrelated_file_deep_app_queries_unchanged() {
        for q in ["/notes", "!release notes", "`tezbar", "spreadsheet"] {
            let mut r = vec![];
            release_notes_row(&mut r, q, true, "1");
            assert!(r.is_empty());
        }
    }
    #[test]
    fn no_duplicate_release_notes() {
        let mut r = vec![];
        release_notes_row(&mut r, "", true, "1");
        release_notes_row(&mut r, "", true, "1");
        assert_eq!(r.len(), 1);
    }
    #[test]
    fn recognizes_only_unmodified_space() {
        assert!(is_plain_space(" ", "Space", false, false, false));
        assert!(!is_plain_space(" ", "Space", true, false, false));
        assert!(!is_plain_space(" ", "Space", false, true, false));
        assert!(!is_plain_space(" ", "Space", false, false, true));
    }
}
#[cfg(test)]
mod cache_tests {
    use super::*;
    #[test]
    fn candidates_merge_stable_identity_and_ephemeral_bound() {
        let a = Arc::new(Row::command("a", "old"));
        let mut e = Row::command("e", "ephemeral");
        e.ephemeral = true;
        let newest = Arc::new(Row::command("a", "new"));
        let b = Arc::new(Row::command("b", "b"));
        let rows = merge_candidates(&[a], &[newest.clone(), b, Arc::new(e)], 2);
        assert_eq!(rows.len(), 2);
        assert!(Arc::ptr_eq(&rows[0], &newest));
        assert!(stable_unique(&rows));
    }
    #[test]
    fn cached_queries_are_bounded_and_current_exact_wins() {
        let mut c = ResultCache::default();
        for i in 0..100 {
            c.accept(
                &i.to_string(),
                vec![Arc::new(Row::command(&i.to_string(), "fixture"))],
            );
        }
        assert_eq!(c.queries.len(), 24);
        assert_eq!(c.preview("99")[0].id, "command:99");
        assert!(c.candidates.len() <= 1000);
        c.invalidate();
        assert!(c.preview("99").is_empty());
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum EscapeResult {
    DismissModelMenu,
    CancelPendingArguments,
    ClearInlineArgument,
    ExitInlineArgument,
    SetInput(String),
    CloseTerminalSettings,
    LeaveTerminal,
    Unhandled,
}
pub fn escape_result(
    value: &str,
    terminal: bool,
    model_menu: bool,
    pending_arguments: bool,
    inline_argument: Option<&str>,
    terminal_settings: bool,
) -> EscapeResult {
    if model_menu {
        return EscapeResult::DismissModelMenu;
    }
    if pending_arguments {
        return EscapeResult::CancelPendingArguments;
    }
    if let Some(arg) = inline_argument {
        return if arg.is_empty() {
            EscapeResult::ExitInlineArgument
        } else {
            EscapeResult::ClearInlineArgument
        };
    }
    let m = input_mode(value, terminal);
    if m.mode == Mode::Ai {
        return EscapeResult::SetInput(if let Some(cwd) = m.ai_working_directory {
            if m.ai_task.is_empty() {
                cwd
            } else {
                format!("{cwd}  ")
            }
        } else if value.trim().is_empty() {
            String::new()
        } else {
            " ".into()
        });
    }
    if value.starts_with('!') {
        return EscapeResult::SetInput(if m.query.is_empty() {
            String::new()
        } else {
            "!".into()
        });
    }
    if let Some(rest) = value.trim_start().strip_prefix('`') {
        return EscapeResult::SetInput(if rest.trim().is_empty() {
            String::new()
        } else {
            "`".into()
        });
    }
    if terminal_settings {
        return EscapeResult::CloseTerminalSettings;
    }
    if terminal {
        return EscapeResult::LeaveTerminal;
    }
    EscapeResult::Unhandled
}
#[cfg(test)]
mod escape_tests {
    use super::*;
    #[test]
    fn two_step_escape_keeps_hidden_mode_sentinels() {
        for (s, next) in [
            (" hello", " "),
            (" ", ""),
            ("!hello", "!"),
            ("!", ""),
            ("`chrome", "`"),
            ("`", ""),
            ("/path  task", "/path  "),
            ("/path  ", "/path"),
        ] {
            assert_eq!(
                escape_result(s, false, false, false, None, false),
                EscapeResult::SetInput(next.into())
            );
        }
        assert_eq!(
            escape_result("x", true, false, false, None, true),
            EscapeResult::CloseTerminalSettings
        );
        assert_eq!(
            escape_result("x", true, false, false, None, false),
            EscapeResult::LeaveTerminal
        );
        assert_eq!(
            escape_result("x", false, false, false, Some("argument"), false),
            EscapeResult::ClearInlineArgument
        );
        assert_eq!(
            escape_result("x", false, false, false, Some(""), false),
            EscapeResult::ExitInlineArgument
        );
    }
}
