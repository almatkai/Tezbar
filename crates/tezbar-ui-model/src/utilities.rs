//! Utility presentation state. Effects call the existing core stores, never a parallel database.
pub mod monitor;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ClipboardContent {
    Text {
        text: String,
        preview: String,
        #[serde(rename = "charCount")]
        char_count: usize,
        #[serde(rename = "lineCount")]
        line_count: usize,
    },
    Image {
        #[serde(rename = "imagePath")]
        image_path: String,
        width: u32,
        height: u32,
        #[serde(rename = "byteSize")]
        byte_size: u64,
    },
    File {
        paths: Vec<String>,
        preview: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardEntry {
    pub id: String,
    pub created_at: u64,
    pub pinned: bool,
    pub is_secret: bool,
    #[serde(flatten)]
    pub content: ClipboardContent,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ClipboardFilter {
    #[default]
    All,
    Text,
    Image,
    Url,
    File,
}
impl ClipboardEntry {
    pub fn text(&self) -> Option<&str> {
        match &self.content {
            ClipboardContent::Text { text, .. } => Some(text),
            _ => None,
        }
    }
    pub fn is_url(&self) -> bool {
        self.text().is_some_and(|s| {
            let s = s.trim();
            s.starts_with("http://") || s.starts_with("https://") || s.starts_with("www.")
        })
    }
    fn matches(&self, q: &str, f: ClipboardFilter) -> bool {
        let filter = match f {
            ClipboardFilter::All => true,
            ClipboardFilter::Text => {
                matches!(self.content, ClipboardContent::Text { .. }) && !self.is_url()
            }
            ClipboardFilter::Url => self.is_url(),
            ClipboardFilter::Image => matches!(self.content, ClipboardContent::Image { .. }),
            ClipboardFilter::File => matches!(self.content, ClipboardContent::File { .. }),
        };
        filter
            && (q.is_empty()
                || match &self.content {
                    ClipboardContent::Text { text, .. } => text.to_lowercase().contains(q),
                    ClipboardContent::Image { image_path, .. } => image_path
                        .rsplit(['/', '\\'])
                        .next()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(q),
                    ClipboardContent::File { paths, .. } => {
                        paths.iter().any(|p| p.to_lowercase().contains(q))
                    }
                })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum ClipboardEffect {
    List {
        request: u64,
    },
    Restore {
        request: u64,
        id: String,
    },
    Delete {
        request: u64,
        id: String,
    },
    TogglePin {
        request: u64,
        id: String,
    },
    Reveal(String),
    LoadImage {
        request: u64,
        id: String,
    },
    /// Execute on the blocking worker pool, then return TransformComplete to the owning selection.
    Transform {
        request: u64,
        id: String,
        action: tezbar_core::instant::transform::ClipboardAction,
        text: String,
    },
    WriteText(String),
    SaveQr {
        png: Vec<u8>,
    },
    WritePng(Vec<u8>),
    Clear {
        request: u64,
    },
    VaultDraft {
        source_clipboard_id: String,
        text: String,
    },
    Hide,
}
#[derive(Default, Debug)]
pub struct Clipboard {
    pub rows: Vec<ClipboardEntry>,
    query: String,
    filter: ClipboardFilter,
    pub selected: usize,
    pub loading: bool,
    pub error: Option<String>,
    pub conversion: Option<Result<String, String>>,
    next: u64,
    pending: BTreeMap<u64, ClipboardEffect>,
    image_cache: VecDeque<(String, Option<String>)>,
    selection_owner: Option<String>,
}
impl Clipboard {
    pub fn query(&self) -> &str {
        &self.query
    }
    pub fn filter(&self) -> ClipboardFilter {
        self.filter
    }
    pub fn set_query(&mut self, query: String) -> Option<ClipboardEffect> {
        self.query = query;
        self.select(self.selected)
    }
    pub fn set_filter(&mut self, filter: ClipboardFilter) -> Option<ClipboardEffect> {
        self.filter = filter;
        self.select(self.selected)
    }

    fn request(&mut self, f: impl FnOnce(u64) -> ClipboardEffect) -> ClipboardEffect {
        self.next += 1;
        let e = f(self.next);
        self.pending.insert(self.next, e.clone());
        e
    }
    pub fn visible(&self) -> Vec<&ClipboardEntry> {
        let q = self.query.trim().to_lowercase();
        let mut v: Vec<_> = self
            .rows
            .iter()
            .filter(|r| r.matches(&q, self.filter))
            .collect();
        v.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then(b.created_at.cmp(&a.created_at))
        });
        v
    }
    pub fn selected_entry(&self) -> Option<&ClipboardEntry> {
        self.visible().get(self.selected).copied()
    }
    pub fn select(&mut self, index: usize) -> Option<ClipboardEffect> {
        let v = self.visible();
        let index = index.min(v.len().saturating_sub(1));
        let e = v.get(index).copied().cloned();
        self.selected = index;
        let id = e.as_ref().map(|e| e.id.clone());
        if id != self.selection_owner {
            self.conversion = None;
            self.pending
                .retain(|_, e| !matches!(e, ClipboardEffect::Transform { .. }));
            self.selection_owner = id.clone();
        }
        let e = e?;
        if !e.is_secret
            && matches!(
                e.content,
                ClipboardContent::Image { .. } | ClipboardContent::File { .. }
            )
            && !self.image_cache.iter().any(|(id, _)| *id == e.id)
            && !self
                .pending
                .values()
                .any(|e2| matches!(e2,ClipboardEffect::LoadImage{id,..}if *id==e.id))
        {
            return Some(self.request(|request| ClipboardEffect::LoadImage { request, id: e.id }));
        }
        None
    }
    pub fn load(&mut self) -> ClipboardEffect {
        self.pending
            .retain(|_, e| !matches!(e, ClipboardEffect::List { .. }));
        self.loading = true;
        self.request(|request| ClipboardEffect::List { request })
    }
    pub fn loaded(&mut self, request: u64, result: Result<Vec<ClipboardEntry>, String>) {
        if !matches!(
            self.pending.remove(&request),
            Some(ClipboardEffect::List { .. })
        ) {
            return;
        }
        self.loading = false;
        match result {
            Ok(rows) => {
                self.rows = rows;
                self.error = None;
                let count = self.visible().len();
                self.selected = self.selected.min(count.saturating_sub(1));
            }
            Err(e) => self.error = Some(e),
        }
    }
    pub fn restore(&mut self) -> Option<ClipboardEffect> {
        let id = self.selected_entry()?.id.clone();
        Some(self.request(|request| ClipboardEffect::Restore { request, id }))
    }
    pub fn delete(&mut self) -> Option<ClipboardEffect> {
        let id = self.selected_entry()?.id.clone();
        Some(self.request(|request| ClipboardEffect::Delete { request, id }))
    }
    pub fn pin(&mut self) -> Option<ClipboardEffect> {
        let id = self.selected_entry()?.id.clone();
        Some(self.request(|request| ClipboardEffect::TogglePin { request, id }))
    }
    pub fn completed(&mut self, request: u64, result: Result<(), String>) -> Vec<ClipboardEffect> {
        let Some(e) = self.pending.remove(&request) else {
            return vec![];
        };
        if let Err(e) = result {
            self.error = Some(e);
            return vec![];
        }
        self.error = None;
        match e {
            ClipboardEffect::Restore { .. } => vec![ClipboardEffect::Hide],
            ClipboardEffect::Delete { id, .. } => {
                self.rows.retain(|r| r.id != id);
                self.image_cache.retain(|(i, _)| *i != id);
                self.select(self.selected).into_iter().collect()
            }
            ClipboardEffect::Clear { .. } => {
                self.rows.clear();
                self.image_cache.clear();
                self.conversion = None;
                vec![]
            }
            ClipboardEffect::TogglePin { id, .. } => {
                if let Some(r) = self.rows.iter_mut().find(|r| r.id == id) {
                    r.pinned = !r.pinned;
                }
                vec![]
            }
            _ => vec![],
        }
    }
    pub fn image_loaded(&mut self, request: u64, result: Result<String, String>) {
        self.image_preview_loaded(request, result.map(Some));
    }
    pub fn image_preview_loaded(&mut self, request: u64, result: Result<Option<String>, String>) {
        let Some(ClipboardEffect::LoadImage { id, .. }) = self.pending.remove(&request) else {
            return;
        };
        match result {
            Ok(data) if data.as_ref().is_none_or(|d| d.len() <= 8 * 1024 * 1024) => {
                self.image_cache.retain(|(i, _)| *i != id);
                self.image_cache.push_back((id, data));
                while self.image_cache.len() > 8
                    || self
                        .image_cache
                        .iter()
                        .map(|(_, d)| d.as_ref().map_or(0, String::len))
                        .sum::<usize>()
                        > 16 * 1024 * 1024
                {
                    self.image_cache.pop_front();
                }
            }
            Ok(_) => {
                self.image_cache.push_back((id, None));
                self.error = Some("Clipboard image exceeds preview budget".into());
            }
            Err(e) => {
                self.image_cache.push_back((id, None));
                self.error = Some(e);
            }
        }
        while self.image_cache.len() > 8 {
            self.image_cache.pop_front();
        }
    }
    pub fn image(&self, id: &str) -> Option<&str> {
        self.image_cache
            .iter()
            .find(|(i, _)| i == id)
            .and_then(|(_, d)| d.as_deref())
    }
    pub fn convert(
        &mut self,
        action: tezbar_core::instant::transform::ClipboardAction,
    ) -> Option<ClipboardEffect> {
        let e = self.selected_entry()?;
        if e.is_secret {
            return None;
        }
        let (id, text) = (e.id.clone(), e.text()?.to_owned());
        self.conversion = None;
        self.pending
            .retain(|_, e| !matches!(e, ClipboardEffect::Transform { .. }));
        Some(self.request(|request| ClipboardEffect::Transform {
            request,
            id,
            action,
            text,
        }))
    }
    pub fn conversion_complete(&mut self, request: u64, result: Result<String, String>) -> bool {
        let Some(ClipboardEffect::Transform { id, .. }) = self.pending.remove(&request) else {
            return false;
        };
        if self
            .selected_entry()
            .is_none_or(|e| e.id != id || e.is_secret)
        {
            return false;
        }
        self.conversion = Some(result);
        true
    }
    pub fn copy_conversion(&self) -> Option<ClipboardEffect> {
        Some(ClipboardEffect::WriteText(
            self.conversion.as_ref()?.as_ref().ok()?.clone(),
        ))
    }
    pub fn qr_export(&self) -> Result<ClipboardEffect, String> {
        if self.selected_entry().is_some_and(|e| e.is_secret) {
            return Err("Secure items cannot be converted".into());
        }
        let s = self
            .selected_entry()
            .and_then(ClipboardEntry::text)
            .ok_or("QR needs a text entry")?;
        if s.trim().is_empty() {
            return Err("QR needs non-empty text".into());
        }
        Ok(ClipboardEffect::SaveQr {
            png: tezbar_core::instant::qr::png(s)?,
        })
    }
    pub fn vault_draft(&self) -> Option<ClipboardEffect> {
        let e = self.selected_entry()?;
        Some(ClipboardEffect::VaultDraft {
            source_clipboard_id: e.id.clone(),
            text: e.text()?.into(),
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub trigger: String,
    pub body_template: String,
    pub resolved_preview: String,
    pub readonly: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct SnippetDraft {
    pub label: String,
    pub trigger: String,
    pub body: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Quicklink {
    pub id: String,
    pub name: String,
    pub template: String,
    pub created_at: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub enum CrudKind {
    Snippet,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub body: String,
    pub preview: String,
    pub trigger: String,
    pub readonly: bool,
}
impl From<Snippet> for Item {
    fn from(s: Snippet) -> Self {
        Self {
            id: s.id,
            title: s.title,
            subtitle: s.subtitle,
            body: s.body_template,
            preview: s.resolved_preview,
            trigger: s.trigger,
            readonly: s.readonly,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum CrudEffect {
    List {
        request: u64,
        kind: CrudKind,
    },
    Write {
        request: u64,
        kind: CrudKind,
        id: Option<String>,
        draft: SnippetDraft,
        revision: u64,
        owner: u64,
    },
    Delete {
        request: u64,
        kind: CrudKind,
        id: String,
    },
    ConfirmDelete(String),
    Copy {
        request: u64,
        kind: CrudKind,
        id: String,
    },
    Hide,
}
#[derive(Debug)]
pub struct Crud {
    pub kind: CrudKind,
    pub rows: Vec<Item>,
    pub selected: usize,
    pub query: String,
    pub draft: Option<(Option<String>, SnippetDraft)>,
    pub error: Option<String>,
    pub loading: bool,
    revision: u64,
    owner: u64,
    next: u64,
    pending: BTreeMap<u64, CrudEffect>,
}
impl Crud {
    pub fn new(kind: CrudKind) -> Self {
        Self {
            kind,
            rows: vec![],
            selected: 0,
            query: String::new(),
            draft: None,
            error: None,
            loading: false,
            revision: 0,
            owner: 0,
            next: 0,
            pending: BTreeMap::new(),
        }
    }
    fn request(&mut self, e: impl FnOnce(u64) -> CrudEffect) -> CrudEffect {
        self.next += 1;
        let e = e(self.next);
        self.pending.insert(self.next, e.clone());
        e
    }
    pub fn visible(&self) -> Vec<&Item> {
        let q = self.query.trim().to_lowercase();
        let mut v: Vec<_> = self
            .rows
            .iter()
            .filter(|r| {
                q.is_empty()
                    || format!(
                        "{} {} {} {} {}",
                        r.title, r.subtitle, r.trigger, r.body, r.preview
                    )
                    .to_lowercase()
                    .contains(&q)
            })
            .collect();
        v.sort_by(|a, b| {
            a.readonly
                .cmp(&b.readonly)
                .then(a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        v
    }
    pub fn load(&mut self) -> CrudEffect {
        self.pending
            .retain(|_, e| !matches!(e, CrudEffect::List { .. }));
        self.loading = true;
        let kind = self.kind.clone();
        self.request(|request| CrudEffect::List { request, kind })
    }
    pub fn loaded(&mut self, request: u64, result: Result<Vec<Item>, String>) {
        if !matches!(self.pending.remove(&request), Some(CrudEffect::List { .. })) {
            return;
        }
        self.loading = false;
        match result {
            Ok(rows) => {
                self.rows = rows;
                self.selected = self.selected.min(self.visible().len().saturating_sub(1));
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }
    pub fn create(&mut self) {
        self.owner += 1;
        self.revision += 1;
        self.draft = Some((None, SnippetDraft::default()));
        self.error = None;
    }
    pub fn edit(&mut self, id: &str) -> bool {
        let Some(r) = self.rows.iter().find(|r| r.id == id && !r.readonly) else {
            return false;
        };
        self.owner += 1;
        self.draft = Some((
            Some(r.id.clone()),
            SnippetDraft {
                label: r.title.clone(),
                trigger: r.trigger.clone(),
                body: r.body.clone(),
            },
        ));
        self.revision += 1;
        self.error = None;
        true
    }
    pub fn change(&mut self, d: SnippetDraft) {
        if let Some((_, draft)) = &mut self.draft {
            *draft = d;
            self.revision += 1;
        }
    }
    pub fn cancel(&mut self) {
        self.owner += 1;
        self.revision += 1;
        self.draft = None;
    }
    pub fn save(&mut self) -> Result<CrudEffect, String> {
        let (id, mut draft) = self.draft.clone().ok_or("No editor draft")?;
        draft.label = draft.label.trim().into();
        draft.trigger = draft.trigger.trim().into();
        draft.body = draft.body.replace("\r\n", "\n");
        {
            for (value, max, name) in [
                (draft.label.trim(), 200, "Title"),
                (draft.trigger.trim(), 48, "Trigger"),
                (draft.body.as_str(), 100000, "Body"),
            ] {
                if value.trim().is_empty() {
                    return Err(format!("{name} is required"));
                }
                if value.encode_utf16().count() > max {
                    return Err(format!("{name} must be at most {max} characters"));
                }
            }
            if draft.trigger.contains(['\n', '\r']) {
                return Err("Trigger must be a single line".into());
            }
            if self.rows.iter().any(|r| {
                Some(&r.id) != id.as_ref()
                    && r.trigger.trim().to_lowercase() == draft.trigger.trim().to_lowercase()
            }) {
                return Err("Another snippet already uses this trigger".into());
            }
        }
        if self
            .pending
            .values()
            .any(|e| matches!(e,CrudEffect::Write{owner,..}if *owner==self.owner))
        {
            return Err("Save already in flight".into());
        }
        let revision = self.revision;
        let owner = self.owner;
        let kind = self.kind.clone();
        Ok(self.request(|request| CrudEffect::Write {
            request,
            kind,
            id,
            draft,
            revision,
            owner,
        }))
    }
    pub fn delete(&mut self, id: &str, confirmed: bool) -> Option<CrudEffect> {
        if self.draft.is_some() || !self.rows.iter().any(|r| r.id == id && !r.readonly) {
            return None;
        }
        if !confirmed {
            return Some(CrudEffect::ConfirmDelete(id.into()));
        }
        let kind = self.kind.clone();
        Some(self.request(|request| CrudEffect::Delete {
            request,
            kind,
            id: id.into(),
        }))
    }
    pub fn copy(&mut self, id: &str) -> Option<CrudEffect> {
        if self.draft.is_some() || !self.rows.iter().any(|r| r.id == id) {
            return None;
        }
        let kind = self.kind.clone();
        Some(self.request(|request| CrudEffect::Copy {
            request,
            kind,
            id: id.into(),
        }))
    }
    /// Core create returns an ID. Adopt it even when the draft has newer unsaved edits.
    pub fn write_completed(
        &mut self,
        request: u64,
        result: Result<String, String>,
    ) -> Vec<CrudEffect> {
        let Some(CrudEffect::Write {
            owner,
            revision,
            id,
            ..
        }) = self.pending.get(&request).cloned()
        else {
            return vec![];
        };
        match result {
            Ok(created) if id.is_some() || !created.is_empty() => {
                self.pending.remove(&request);
                if owner == self.owner {
                    if revision == self.revision {
                        self.draft = None;
                    } else if let Some((draft_id, _)) = &mut self.draft {
                        *draft_id = id.or(Some(created));
                    }
                }
                self.error = None;
                vec![self.load()]
            }
            result => {
                self.pending.remove(&request);
                if owner == self.owner {
                    self.error = Some(
                        result
                            .err()
                            .unwrap_or_else(|| "Create returned no snippet ID".into()),
                    );
                }
                vec![]
            }
        }
    }
    pub fn completed(&mut self, request: u64, result: Result<(), String>) -> Vec<CrudEffect> {
        if matches!(
            self.pending.get(&request),
            Some(CrudEffect::Write { id: None, .. })
        ) && result.is_ok()
        {
            self.error =
                Some("Create completion requires write_completed with the created ID".into());
            return vec![]; // retain ownership so an ID-bearing completion can still arrive
        }
        let Some(e) = self.pending.remove(&request) else {
            return vec![];
        };
        if let Err(e) = result {
            self.error = Some(e);
            return vec![];
        }
        self.error = None;
        match e {
            CrudEffect::Write {
                revision,
                owner,
                id,
                ..
            } => {
                if id.is_none() {
                    self.error = Some(
                        "Create completion requires write_completed with the created ID".into(),
                    );
                    return vec![];
                }
                if owner == self.owner && revision == self.revision {
                    self.draft = None;
                }
                vec![self.load()]
            }
            CrudEffect::Delete { id, .. } => {
                self.rows.retain(|r| r.id != id);
                self.selected = self.selected.min(self.visible().len().saturating_sub(1));
                vec![]
            }
            CrudEffect::Copy { .. } => vec![CrudEffect::Hide],
            _ => vec![],
        }
    }
}
/// A visible utility owns one request; hides invalidate replies and stop polling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PollKind {
    Indexing,
    Permissions,
    SystemMonitor,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PollEffect {
    pub generation: u64,
    pub kind: PollKind,
    pub query: String,
}
#[derive(Debug)]
pub struct UtilityPoll<T> {
    pub kind: PollKind,
    pub value: Option<T>,
    pub error: Option<String>,
    pub loading: bool,
    pub visible: bool,
    pub auto_refresh: bool,
    generation: u64,
    flight: Option<u64>,
    due: u64,
    pub query: String,
    interval: u64,
}
impl<T> UtilityPoll<T> {
    pub fn new(kind: PollKind) -> Self {
        Self {
            kind,
            value: None,
            error: None,
            loading: false,
            visible: false,
            auto_refresh: true,
            generation: 0,
            flight: None,
            due: 0,
            query: String::new(),
            interval: match kind {
                PollKind::SystemMonitor => 2500,
                PollKind::Indexing => 0,
                PollKind::Permissions => 0,
            },
        }
    }
    pub fn show(&mut self, now: u64) {
        self.visible = true;
        self.due = now;
    }
    pub fn hide(&mut self) {
        self.visible = false;
        self.generation += 1;
        self.flight = None;
        self.loading = false;
    }
    pub fn search(&mut self, q: String, now: u64) {
        self.query = q;
        self.generation += 1;
        self.flight = None;
        self.due = now + 160;
    }
    pub fn tick(&mut self, now: u64) -> Option<PollEffect> {
        if !self.visible || self.flight.is_some() || now < self.due {
            return None;
        }
        self.generation += 1;
        self.flight = Some(self.generation);
        self.loading = self.value.is_none();
        Some(PollEffect {
            generation: self.generation,
            kind: self.kind,
            query: self.query.clone(),
        })
    }
    pub fn complete(&mut self, g: u64, result: Result<T, String>, now: u64) -> bool {
        if self.flight != Some(g) {
            return false;
        }
        self.flight = None;
        self.loading = false;
        self.due = if self.interval == 0 || !self.auto_refresh {
            u64::MAX
        } else {
            now + self.interval
        };
        match result {
            Ok(v) => {
                self.value = Some(v);
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
        true
    }
    pub fn set_auto_refresh(&mut self, enabled: bool, now: u64) {
        self.auto_refresh = enabled;
        self.due = if enabled { now } else { u64::MAX };
    }
    pub fn refresh(&mut self, now: u64) {
        self.due = now;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn item(id: &str) -> Item {
        Item {
            id: id.into(),
            title: id.into(),
            subtitle: String::new(),
            body: "body".into(),
            preview: "body".into(),
            trigger: id.into(),
            readonly: false,
        }
    }
    #[test]
    fn draft_revision_and_errors_remain_owned() {
        let mut c = Crud::new(CrudKind::Snippet);
        c.rows = vec![item("a"), item("b")];
        assert!(c.edit("a"));
        let CrudEffect::Write { request, .. } = c.save().unwrap() else {
            panic!()
        };
        c.edit("b");
        c.change(SnippetDraft {
            label: "new b".into(),
            body: "changed".into(),
            trigger: "b".into(),
        });
        assert_eq!(c.completed(request, Ok(())).len(), 1);
        assert_eq!(c.draft.as_ref().unwrap().0, Some("b".into()));
        let CrudEffect::Write { request, .. } = c.save().unwrap() else {
            panic!()
        };
        c.completed(request, Err("disk full".into()));
        assert!(c.draft.is_some());
        assert_eq!(c.error.as_deref(), Some("disk full"));
    }
    #[test]
    fn readonly_and_confirmed_delete() {
        let mut c = Crud::new(CrudKind::Snippet);
        let mut r = item("shipped");
        r.readonly = true;
        c.rows = vec![r, item("user")];
        assert!(!c.edit("shipped"));
        assert!(c.delete("shipped", true).is_none());
        assert_eq!(
            c.delete("user", false),
            Some(CrudEffect::ConfirmDelete("user".into()))
        );
        let CrudEffect::Delete { request, .. } = c.delete("user", true).unwrap() else {
            panic!()
        };
        c.completed(request, Err("denied".into()));
        assert_eq!(c.rows.len(), 2);
    }
    #[test]
    fn stale_poll_and_failure_retry() {
        let mut p = UtilityPoll::new(PollKind::SystemMonitor);
        assert!(p.tick(0).is_none());
        p.show(0);
        let e = p.tick(0).unwrap();
        p.hide();
        assert!(!p.complete(e.generation, Ok(1), 1));
        p.show(2);
        let e = p.tick(2).unwrap();
        p.complete(e.generation, Err("offline".into()), 3);
        assert_eq!(p.error.as_deref(), Some("offline"));
        p.refresh(4);
        let e = p.tick(4).unwrap();
        p.complete(e.generation, Ok(2), 5);
        assert_eq!(p.value, Some(2));
        assert!(p.error.is_none());
    }
    #[test]
    fn clipboard_filter_order_and_conversion_ownership() {
        let mut c = Clipboard::default();
        c.rows = vec![
            ClipboardEntry {
                id: "1".into(),
                created_at: 2,
                pinned: false,
                is_secret: false,
                content: ClipboardContent::Text {
                    text: "hello".into(),
                    preview: "hello".into(),
                    char_count: 5,
                    line_count: 1,
                },
            },
            ClipboardEntry {
                id: "2".into(),
                created_at: 1,
                pinned: true,
                is_secret: false,
                content: ClipboardContent::Text {
                    text: "https://a.test".into(),
                    preview: "url".into(),
                    char_count: 14,
                    line_count: 1,
                },
            },
        ];
        assert_eq!(c.visible()[0].id, "2");
        c.set_filter(ClipboardFilter::Text);
        assert_eq!(c.visible()[0].id, "1");
        c.select(0);
        let ClipboardEffect::Transform {
            request,
            action,
            text,
            ..
        } = c
            .convert(tezbar_core::instant::transform::ClipboardAction::Case)
            .unwrap()
        else {
            panic!()
        };
        c.conversion_complete(
            request,
            tezbar_core::instant::transform::apply_clipboard_action(action, &text),
        );
        assert_eq!(c.conversion, Some(Ok("HELLO".into())));
        c.set_filter(ClipboardFilter::All);
        c.select(0);
        assert!(c.conversion.is_none());
        let ClipboardEffect::Restore { request, .. } = c.restore().unwrap() else {
            panic!()
        };
        assert!(c.completed(request, Err("denied".into())).is_empty());
        assert_eq!(c.rows.len(), 2);
    }
}

impl Clipboard {
    pub fn clear(&mut self) -> ClipboardEffect {
        self.request(|request| ClipboardEffect::Clear { request })
    }
    pub fn preview_text(&self) -> Option<String> {
        let e = self.selected_entry()?;
        if e.is_secret {
            return Some("Secure item".into());
        }
        e.text().map(String::from)
    }
    pub fn copy_qr(&self) -> Result<ClipboardEffect, String> {
        match self.qr_export()? {
            ClipboardEffect::SaveQr { png } => Ok(ClipboardEffect::WritePng(png)),
            _ => unreachable!(),
        }
    }
}
/// LRU for lazy icon adapters, including negative lookups; accounting matches JS UTF-16 bytes.
#[derive(Debug)]
pub struct StringCache {
    entries: VecDeque<(String, Option<String>)>,
    max_entries: usize,
    max_bytes: usize,
    bytes: usize,
}
impl StringCache {
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_entries,
            max_bytes,
            bytes: 0,
        }
    }
    fn cost(k: &str, v: Option<&str>) -> usize {
        2 * (k.encode_utf16().count() + v.map_or(0, |v| v.encode_utf16().count()))
    }
    pub fn get(&mut self, key: &str) -> Option<Option<&str>> {
        let i = self.entries.iter().position(|(k, _)| k == key)?;
        let e = self.entries.remove(i)?;
        self.entries.push_back(e);
        Some(self.entries.back()?.1.as_deref())
    }
    pub fn set(&mut self, key: String, value: Option<String>) {
        self.delete(&key);
        let cost = Self::cost(&key, value.as_deref());
        if cost > self.max_bytes || self.max_entries == 0 {
            return;
        }
        self.bytes += cost;
        self.entries.push_back((key, value));
        while self.entries.len() > self.max_entries || self.bytes > self.max_bytes {
            if let Some((k, v)) = self.entries.pop_front() {
                self.bytes -= Self::cost(&k, v.as_deref());
            }
        }
    }
    pub fn delete(&mut self, key: &str) -> bool {
        if let Some(i) = self.entries.iter().position(|(k, _)| k == key) {
            let (k, v) = self.entries.remove(i).unwrap();
            self.bytes -= Self::cost(&k, v.as_deref());
            true
        } else {
            false
        }
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
    pub fn keys(&self) -> Vec<&str> {
        self.entries.iter().map(|(k, _)| k.as_str()).collect()
    }
}
#[cfg(test)]
mod cache_tests {
    use super::*;
    #[test]
    fn evicts_least_recently_used_including_cached_misses() {
        let mut c = StringCache::new(2, 4 * 1024 * 1024);
        c.set("a".into(), None);
        c.set("b".into(), Some("icon".into()));
        assert_eq!(c.get("a"), Some(None));
        c.set("c".into(), Some("icon".into()));
        assert_eq!(c.keys(), ["a", "c"]);
    }
    #[test]
    fn bounds_bytes_replacement_deletion_clear() {
        let mut c = StringCache::new(10, 20);
        for k in ["a", "b"] {
            c.set(k.into(), Some("1234".into()));
        }
        c.set("a".into(), Some("12345678".into()));
        assert_eq!(c.keys(), ["a"]);
        c.set("a".into(), Some("too large to retain".into()));
        assert!(c.keys().is_empty());
        c.set("b".into(), Some("1234".into()));
        c.delete("b");
        for k in ["c", "d"] {
            c.set(k.into(), Some("1234".into()));
        }
        assert_eq!(c.keys().len(), 2);
        c.clear();
        for k in ["e", "f"] {
            c.set(k.into(), Some("1234".into()));
        }
        assert_eq!(c.keys().len(), 2);
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionId {
    Accessibility,
    Automation,
    InputMonitoring,
    Microphone,
    Calendar,
    ScreenRecording,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionState {
    Granted,
    Denied,
    Restricted,
    NotDetermined,
    Unsupported,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PermissionDescriptor {
    pub id: PermissionId,
    pub title: String,
    pub summary: String,
    pub rationale: String,
    pub settings_url: Option<String>,
    pub remediation: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
    pub descriptor: PermissionDescriptor,
    pub state: PermissionState,
    pub checked_at: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PermissionsSnapshot {
    pub platform: String,
    pub statuses: Vec<PermissionStatus>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum PermissionEffect {
    Request { generation: u64, id: PermissionId },
    Refresh,
}
#[derive(Debug, Default)]
pub struct Permissions {
    pub snapshot: Option<PermissionsSnapshot>,
    pub pending: Option<PermissionId>,
    pub banner: Option<(bool, String)>,
    generation: u64,
}
impl Permissions {
    pub fn request(&mut self, id: PermissionId) -> Option<PermissionEffect> {
        if self.pending.is_some() {
            return None;
        }
        let s = self
            .snapshot
            .as_ref()?
            .statuses
            .iter()
            .find(|s| s.descriptor.id == id)?;
        if matches!(
            s.state,
            PermissionState::Granted | PermissionState::Unsupported
        ) {
            return None;
        }
        self.generation += 1;
        self.pending = Some(id.clone());
        self.banner = None;
        Some(PermissionEffect::Request {
            generation: self.generation,
            id,
        })
    }
    pub fn complete(
        &mut self,
        g: u64,
        result: Result<PermissionStatus, String>,
    ) -> Vec<PermissionEffect> {
        if g != self.generation || self.pending.take().is_none() {
            return vec![];
        }
        self.banner = Some(match result {
            Ok(s) => {
                let granted = s.state == PermissionState::Granted;
                (
                    granted,
                    if granted {
                        format!("{} is now granted.", s.descriptor.title)
                    } else {
                        format!("{}: {}", s.descriptor.title, s.descriptor.remediation)
                    },
                )
            }
            Err(e) => (false, e),
        });
        vec![PermissionEffect::Refresh]
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IndexedSource {
    pub id: String,
    pub path: String,
    pub title: String,
    pub byte_size: u64,
    pub modified_at: u64,
    pub indexed_at: Option<u64>,
    pub status: String,
    pub error: Option<String>,
    pub total_page_count: u64,
    pub indexed_page_count: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourcePage {
    pub sources: Vec<IndexedSource>,
    pub total: usize,
    pub offset: usize,
    pub has_more: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum IndexingEffect {
    List {
        generation: u64,
        query: String,
        offset: usize,
        limit: usize,
    },
    Snapshot,
    Pause,
    Resume,
    Start,
    OpenFile(String),
    Reveal(String),
}
#[derive(Debug, Default)]
pub struct Indexing {
    pub query: String,
    pub page: Option<SourcePage>,
    pub selected: usize,
    pub error: Option<String>,
    pub loading: bool,
    generation: u64,
    pending: Option<(u64, bool)>,
    deadline: Option<u64>,
}
impl Indexing {
    pub fn search(&mut self, q: String, now: u64) {
        self.query = q;
        self.generation += 1;
        self.pending = None;
        self.deadline = Some(now + 160);
    }
    pub fn tick(&mut self, now: u64) -> Option<IndexingEffect> {
        if self.deadline.is_some_and(|t| now >= t) {
            self.deadline = None;
            self.load(false)
        } else {
            None
        }
    }
    pub fn load(&mut self, append: bool) -> Option<IndexingEffect> {
        if self.pending.is_some() {
            return None;
        }
        let offset = if append {
            let p = self.page.as_ref()?;
            if !p.has_more {
                return None;
            }
            p.offset + p.sources.len()
        } else {
            0
        };
        self.generation += 1;
        self.pending = Some((self.generation, append));
        self.loading = true;
        Some(IndexingEffect::List {
            generation: self.generation,
            query: self.query.clone(),
            offset,
            limit: 200,
        })
    }
    pub fn complete(&mut self, g: u64, result: Result<SourcePage, String>) -> bool {
        let Some((owned, append)) = self.pending else {
            return false;
        };
        if g != owned {
            return false;
        }
        self.pending = None;
        self.loading = false;
        match result {
            Ok(mut page) => {
                if append {
                    if let Some(current) = self.page.take() {
                        let mut ids: std::collections::HashSet<_> =
                            current.sources.iter().map(|s| s.id.clone()).collect();
                        let mut merged = current.sources;
                        merged.extend(
                            page.sources
                                .into_iter()
                                .filter(|s| ids.insert(s.id.clone())),
                        );
                        page.sources = merged;
                        page.offset = 0;
                    }
                } else {
                    self.selected = 0;
                }
                self.page = Some(page);
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
        true
    }
    pub fn source_action(&self, reveal: bool) -> Option<IndexingEffect> {
        let path = self.page.as_ref()?.sources.get(self.selected)?.path.clone();
        Some(if reveal {
            IndexingEffect::Reveal(path)
        } else {
            IndexingEffect::OpenFile(path)
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum EmojiEffect {
    PersistRecent(Vec<String>),
    HideThenPaste(String),
}
#[derive(Debug, Default)]
pub struct EmojiPicker {
    pub query: String,
    pub category: Option<String>,
    pub selected: usize,
    pub recent: Vec<String>,
}
impl EmojiPicker {
    pub fn visible(&self) -> Vec<&str> {
        use tezbar_core::instant::emoji::{search, DATA};
        let mut v = vec![];
        if self.query.trim().is_empty() && self.category.as_deref().is_none_or(|s| s == "All") {
            v.extend(
                self.recent
                    .iter()
                    .filter(|s| DATA.iter().any(|e| e.char == ***s))
                    .map(String::as_str),
            );
        }
        v.extend(
            search(&self.query, self.category.as_deref(), DATA.len())
                .into_iter()
                .map(|e| e.char.as_str()),
        );
        v
    }
    pub fn move_selection(&mut self, key: &str) {
        let delta = match key {
            "ArrowLeft" => -1,
            "ArrowRight" => 1,
            "ArrowUp" => -8,
            "ArrowDown" => 8,
            _ => 0,
        };
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(self.visible().len().saturating_sub(1));
    }
    pub fn activate(&mut self, index: usize) -> Vec<EmojiEffect> {
        let Some(emoji) = self.visible().get(index).map(|s| s.to_string()) else {
            return vec![];
        };
        crate::launcher::record_recent(&mut self.recent, &emoji, 24);
        vec![
            EmojiEffect::PersistRecent(self.recent.clone()),
            EmojiEffect::HideThenPaste(emoji),
        ]
    }
}
#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[test]
    fn secure_clipboard_never_enters_conversion() {
        let mut c = Clipboard::default();
        c.rows.push(ClipboardEntry {
            id: "s".into(),
            created_at: 1,
            pinned: false,
            is_secret: true,
            content: ClipboardContent::Text {
                text: "fixture-secret".into(),
                preview: "hidden".into(),
                char_count: 14,
                line_count: 1,
            },
        });
        assert_eq!(c.preview_text(), Some("Secure item".into()));
        assert!(c
            .convert(tezbar_core::instant::transform::ClipboardAction::Base64)
            .is_none());
        assert!(c.qr_export().is_err());
    }
    #[test]
    fn emoji_visible_grid_identity_and_hide_paste_order() {
        let mut e = EmojiPicker::default();
        e.recent = vec!["😀".into(), "unknown".into()];
        assert_eq!(e.visible()[0], "😀");
        assert_eq!(e.visible()[1], "😀");
        e.move_selection("ArrowDown");
        assert_eq!(e.selected, 8);
        let effects = e.activate(1);
        assert_eq!(
            effects.last(),
            Some(&EmojiEffect::HideThenPaste("😀".into()))
        );
        assert_eq!(e.recent, ["😀", "unknown"]);
    }
    #[test]
    fn indexing_pages_reject_old_filter_and_retry_error() {
        let mut s = Indexing::default();
        s.search("old".into(), 0);
        let IndexingEffect::List { generation, .. } = s.tick(160).unwrap() else {
            panic!()
        };
        s.search("new".into(), 161);
        assert!(!s.complete(generation, Err("old failure".into())));
        let IndexingEffect::List {
            generation, limit, ..
        } = s.tick(321).unwrap()
        else {
            panic!()
        };
        assert_eq!(limit, 200);
        s.complete(generation, Err("offline".into()));
        assert_eq!(s.error.as_deref(), Some("offline"));
        assert!(s.load(false).is_some());
    }
}
#[cfg(test)]
mod auto_refresh_tests {
    use super::*;
    #[test]
    fn monitor_auto_refresh_toggle_keeps_snapshot_and_manual_refresh() {
        let mut p = UtilityPoll::new(PollKind::SystemMonitor);
        p.show(0);
        let e = p.tick(0).unwrap();
        p.complete(e.generation, Ok(1), 1);
        p.set_auto_refresh(false, 2);
        assert_eq!(p.value, Some(1));
        assert!(p.tick(10000).is_none());
        p.refresh(10001);
        let e = p.tick(10001).unwrap();
        p.complete(e.generation, Ok(2), 10002);
        assert!(p.tick(99999).is_none());
        p.set_auto_refresh(true, 100000);
        assert!(p.tick(100000).is_some());
    }
}

#[cfg(test)]
mod opus_repairs {
    use super::*;
    fn text(id: &str) -> ClipboardEntry {
        ClipboardEntry {
            id: id.into(),
            created_at: 1,
            pinned: false,
            is_secret: false,
            content: ClipboardContent::Text {
                text: id.into(),
                preview: id.into(),
                char_count: 1,
                line_count: 1,
            },
        }
    }
    #[test]
    fn opus_clipboard_file_negative_preview_filter_clamp() {
        let mut c = Clipboard::default();
        c.rows = vec![
            text("one"),
            ClipboardEntry {
                id: "file".into(),
                created_at: 0,
                pinned: false,
                is_secret: false,
                content: ClipboardContent::File {
                    paths: vec!["/fixture".into()],
                    preview: "file".into(),
                },
            },
        ];
        let ClipboardEffect::LoadImage { request, .. } = c.select(1).unwrap() else {
            panic!()
        };
        c.image_preview_loaded(request, Ok(None));
        assert!(c.select(1).is_none());
        c.set_query("one".into());
        assert_eq!(c.selected, 0);
        assert_eq!(c.selected_entry().unwrap().id, "one");
        c.set_query("".into());
        assert!(c.select(1).is_none());
        c.image_cache.clear();
        let ClipboardEffect::LoadImage { request, .. } = c.select(1).unwrap() else {
            panic!()
        };
        c.image_loaded(request, Err("missing".into()));
        assert!(c.select(1).is_none());
    }
    #[test]
    fn opus_clipboard_transform_is_off_thread_and_selection_owned() {
        let mut c = Clipboard::default();
        c.rows = vec![text("A"), text("B")];
        c.select(0);
        let ClipboardEffect::Transform { request, text, .. } = c
            .convert(tezbar_core::instant::transform::ClipboardAction::Json)
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(text, "A");
        assert!(c.conversion.is_none());
        c.select(1);
        assert!(!c.conversion_complete(request, Ok("late".into())));
        assert!(c.conversion.is_none());
    }
    #[test]
    fn opus_snippet_create_adopts_id_for_newer_revision_and_unicode_uniqueness() {
        let mut c = Crud::new(CrudKind::Snippet);
        c.create();
        c.change(SnippetDraft {
            label: "Title".into(),
            trigger: "ТЕСТ".into(),
            body: "Body".into(),
        });
        let CrudEffect::Write { request, .. } = c.save().unwrap() else {
            panic!()
        };
        c.change(SnippetDraft {
            label: "Title".into(),
            trigger: "ТЕСТ".into(),
            body: "New".into(),
        });
        assert!(c.save().is_err());
        c.write_completed(request, Ok("created-id".into()));
        assert_eq!(c.draft.as_ref().unwrap().0.as_deref(), Some("created-id"));
        assert!(matches!(c.save().unwrap(),CrudEffect::Write{id:Some(id),..}if id=="created-id"));
        c.create();
        c.rows.push(Item {
            id: "other".into(),
            title: "Other".into(),
            subtitle: String::new(),
            body: "B".into(),
            preview: "B".into(),
            trigger: "тест".into(),
            readonly: false,
        });
        c.change(SnippetDraft {
            label: "Title".into(),
            trigger: "ТЕСТ".into(),
            body: "Body".into(),
        });
        assert!(c.save().is_err());
    }
    #[test]
    fn opus_snippet_late_create_cannot_adopt_into_different_draft() {
        let mut c = Crud::new(CrudKind::Snippet);
        c.create();
        c.change(SnippetDraft {
            label: "One".into(),
            trigger: "one".into(),
            body: "B".into(),
        });
        let CrudEffect::Write { request, .. } = c.save().unwrap() else {
            panic!()
        };
        c.create();
        c.write_completed(request, Ok("old-id".into()));
        assert_eq!(c.draft.as_ref().unwrap().0, None);
    }
}
