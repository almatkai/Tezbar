//! Rich HTML document and note save ownership; the core remains the sole note store.
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
const EMPTY: &str = "<div><br></div>";
const TAGS: [&str; 11] = [
    "div", "br", "p", "strong", "b", "em", "i", "u", "ul", "ol", "li",
];
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#039;")
}
fn decode(s: &str) -> String {
    if !s.bytes().any(|b| matches!(b, b'&' | b'\r' | 0)) {
        return s.to_owned();
    }
    use html5ever::tokenizer::{
        states, BufferQueue, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
    };
    use std::cell::RefCell;
    struct Text(RefCell<String>);
    impl TokenSink for Text {
        type Handle = ();
        fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
            match token {
                Token::CharacterTokens(text) => self.0.borrow_mut().push_str(&text),
                Token::NullCharacterToken => self.0.borrow_mut().push('\u{fffd}'),
                _ => {}
            }
            TokenSinkResult::Continue
        }
    }
    // The old editor used a textarea's HTML fragment decoder. RCDATA with no
    // matching end tag gives the same one-pass character references without
    // constructing a DOM, treating markup as UI, or interpreting any script.
    let tokenizer = Tokenizer::new(
        Text(RefCell::new(String::with_capacity(s.len()))),
        TokenizerOpts {
            initial_state: Some(states::RawData(states::Rcdata)),
            discard_bom: false,
            ..Default::default()
        },
    );
    let input = BufferQueue::default();
    input.push_back(s.into());
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    tokenizer.sink.0.into_inner()
}
fn strip_markdown(s: &str) -> String {
    let mut s = s.to_string();
    for (p, r) in [
        (r"\*\*([^*\n]+)\*\*", "$1"),
        (r"__([^_\n]+)__", "$1"),
        (r"\*([^*\n]+)\*", "$1"),
        (r"_([^_\n]+)_", "$1"),
        (r"`([^`\n]+)`", "$1"),
        (r"\[([^\]]+)\]\([^)]+\)", "$1"),
        (r"(?m)^#{1,6}\s+", ""),
        (r"(?m)^\s*[-*+]\s+", ""),
        (r"(?m)^\s*\d+\.\s+", ""),
    ] {
        s = Regex::new(p).unwrap().replace_all(&s, r).into_owned();
    }
    s
}
pub fn sanitize_html(s: &str) -> Result<String, String> {
    if s.len() > 1_048_576 {
        return Err("Note exceeds 1 MiB".into());
    }
    let s = Regex::new(r"(?is)<script.*?</script>|<style.*?</style>")
        .unwrap()
        .replace_all(s, "");
    let s = Regex::new(r"(?i)<(/?)([a-z0-9-]+)(?:\s[^>]*)?\s*/?>")
        .unwrap()
        .replace_all(&s, |c: &regex::Captures| {
            let n = c[2].to_lowercase();
            if TAGS.contains(&n.as_str()) {
                format!("<{}{n}>", &c[1])
            } else {
                String::new()
            }
        });
    Ok(if s.trim().is_empty() {
        EMPTY.into()
    } else {
        s.trim().into()
    })
}
pub fn normalize_stored(s: &str) -> Result<String, String> {
    if Regex::new(r"(?is)</?[a-z].*>").unwrap().is_match(s) {
        sanitize_html(s)
    } else {
        if s.len() > 1_048_576 {
            return Err("Note exceeds 1 MiB".into());
        }
        let s = strip_markdown(s).replace('\r', "");
        let s = s.trim();
        Ok(if s.is_empty() {
            EMPTY.into()
        } else {
            s.split('\n')
                .map(|l| {
                    if l.trim().is_empty() {
                        EMPTY.into()
                    } else {
                        format!("<div>{}</div>", escape(l))
                    }
                })
                .collect()
        })
    }
}
pub fn plain_text(s: &str) -> String {
    let s = Regex::new(r"(?i)<br\s*/?\s*>|</(?:div|p|li|h[1-6])>")
        .unwrap()
        .replace_all(s, "\n");
    let s = s.replace("<li>", "- ");
    let s = Regex::new(r"<[^>]+>").unwrap().replace_all(&s, "");
    let s = strip_markdown(&decode(&s))
        .replace('\r', "")
        .replace('\u{a0}', " ");
    let s = Regex::new(r"[ \t]+\n").unwrap().replace_all(&s, "\n");
    Regex::new(r"\n{3,}")
        .unwrap()
        .replace_all(&s, "\n\n")
        .trim()
        .into()
}
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Text(String),
    Element { tag: String, children: Vec<Node> },
}
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub nodes: Vec<Node>,
}
impl Document {
    pub fn load(s: &str) -> Result<Self, String> {
        let html = normalize_stored(s)?;
        let re = Regex::new(r"</?([a-z]+)>").unwrap();
        let mut stack: Vec<(String, Vec<Node>)> = vec![(String::new(), vec![])];
        let mut pos = 0;
        for c in re.captures_iter(&html) {
            let m = c.get(0).unwrap();
            if m.start() > pos {
                stack
                    .last_mut()
                    .unwrap()
                    .1
                    .push(Node::Text(decode(&html[pos..m.start()])));
            }
            let tag = &c[1];
            if m.as_str().starts_with("</") {
                if let Some(i) = stack.iter().rposition(|(t, _)| t == tag) {
                    while stack.len() > i {
                        let (tag, children) = stack.pop().unwrap();
                        stack
                            .last_mut()
                            .unwrap()
                            .1
                            .push(Node::Element { tag, children });
                    }
                }
            } else if tag == "br" {
                stack.last_mut().unwrap().1.push(Node::Element {
                    tag: "br".into(),
                    children: vec![],
                });
            } else {
                if stack.len() > 128 {
                    return Err("Note nesting exceeds 128 levels".into());
                }
                stack.push((tag.into(), vec![]));
            }
            pos = m.end();
        }
        if pos < html.len() {
            stack
                .last_mut()
                .unwrap()
                .1
                .push(Node::Text(decode(&html[pos..])));
        }
        while stack.len() > 1 {
            let (tag, children) = stack.pop().unwrap();
            stack
                .last_mut()
                .unwrap()
                .1
                .push(Node::Element { tag, children });
        }
        Ok(Self {
            nodes: stack.pop().unwrap().1,
        })
    }
    pub fn html(&self) -> String {
        fn write(nodes: &[Node], out: &mut String) {
            for n in nodes {
                match n {
                    Node::Text(t) => out.push_str(&escape(t)),
                    Node::Element { tag, children } => {
                        out.push_str(&format!("<{tag}>"));
                        write(children, out);
                        if tag != "br" {
                            out.push_str(&format!("</{tag}>"));
                        }
                    }
                }
            }
        }
        let mut out = String::new();
        write(&self.nodes, &mut out);
        if out.is_empty() {
            EMPTY.into()
        } else {
            out
        }
    }
    pub fn text_len(&self) -> usize {
        fn len(n: &Node) -> usize {
            match n {
                Node::Text(t) => t.chars().count(),
                Node::Element { tag, children } => {
                    if tag == "br" {
                        1
                    } else {
                        children.iter().map(len).sum()
                    }
                }
            }
        }
        self.nodes.iter().map(len).sum()
    }
    /// Code point offsets, shared across runs; native input adapter handles marked text before committing.
    pub fn replace(&mut self, start: usize, end: usize, text: &str) -> Result<(), String> {
        if start > end || end > self.text_len() {
            return Err("Selection outside document".into());
        }
        if self.html().len() + text.len() > 1_048_576 {
            return Err("Note exceeds 1 MiB".into());
        }
        fn walk(
            nodes: &mut Vec<Node>,
            offset: &mut usize,
            start: usize,
            end: usize,
            text: &str,
            inserted: &mut bool,
        ) {
            for node in nodes.iter_mut() {
                match node {
                    Node::Text(t) => {
                        let count = t.chars().count();
                        let lo = start.saturating_sub(*offset).min(count);
                        let hi = end.saturating_sub(*offset).min(count);
                        if end >= *offset && start <= *offset + count {
                            let mut next = t.chars().take(lo).collect::<String>();
                            if !*inserted {
                                next.push_str(text);
                                *inserted = true;
                            }
                            next.extend(t.chars().skip(hi));
                            *t = next;
                        }
                        *offset += count;
                    }
                    Node::Element { tag, children } => {
                        if tag == "br" {
                            if start <= *offset && end > *offset {
                                *node = Node::Text(if !*inserted {
                                    *inserted = true;
                                    text.into()
                                } else {
                                    String::new()
                                });
                            }
                            *offset += 1;
                        } else {
                            walk(children, offset, start, end, text, inserted);
                        }
                    }
                }
            }
        }
        let mut inserted = false;
        walk(&mut self.nodes, &mut 0, start, end, text, &mut inserted);
        if !inserted {
            self.nodes.push(Node::Text(text.into()));
        }
        Ok(())
    }
    pub fn format(&mut self, start: usize, end: usize, style: Style) -> Result<(), String> {
        if start >= end || end > self.text_len() {
            return Err("Select text to format".into());
        }
        let tag = style.tag();
        fn walk(nodes: &mut Vec<Node>, offset: &mut usize, start: usize, end: usize, tag: &str) {
            let mut out = vec![];
            for n in std::mem::take(nodes) {
                match n {
                    Node::Text(t) => {
                        let count = t.chars().count();
                        let lo = start.saturating_sub(*offset).min(count);
                        let hi = end.saturating_sub(*offset).min(count);
                        if lo < hi {
                            let before = t.chars().take(lo).collect::<String>();
                            let inside = t.chars().skip(lo).take(hi - lo).collect();
                            let after = t.chars().skip(hi).collect::<String>();
                            if !before.is_empty() {
                                out.push(Node::Text(before));
                            }
                            out.push(Node::Element {
                                tag: tag.into(),
                                children: vec![Node::Text(inside)],
                            });
                            if !after.is_empty() {
                                out.push(Node::Text(after));
                            }
                        } else {
                            out.push(Node::Text(t));
                        }
                        *offset += count;
                    }
                    Node::Element {
                        tag: t,
                        mut children,
                    } => {
                        if t == "br" {
                            *offset += 1;
                        } else {
                            walk(&mut children, offset, start, end, tag);
                        }
                        out.push(Node::Element { tag: t, children });
                    }
                }
            }
            *nodes = out;
        }
        walk(&mut self.nodes, &mut 0, start, end, tag);
        Ok(())
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Style {
    Bold,
    Italic,
    Underline,
}
impl Style {
    fn tag(self) -> &'static str {
        match self {
            Self::Bold => "strong",
            Self::Italic => "em",
            Self::Underline => "u",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub created_at: u64,
    pub updated_at: u64,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq)]
pub enum SaveState {
    Idle,
    Dirty,
    Saving,
    Saved,
    Error(String),
}
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    List {
        request: u64,
    },
    Create {
        request: u64,
        html: String,
    },
    Save {
        request: u64,
        note_id: u64,
        revision: u64,
        html: String,
    },
    Delete {
        request: u64,
        note_id: u64,
    },
    Copy(String),
}
#[derive(Debug)]
struct Draft {
    doc: Document,
    revision: u64,
    persisted: String,
    deadline: u64,
    undo: Vec<Patch>,
    redo: Vec<Patch>,
}
#[derive(Debug, Default)]
pub struct Notes {
    pub rows: Vec<Note>,
    pub selected: Option<u64>,
    pub query: String,
    pub error: Option<String>,
    drafts: BTreeMap<u64, Draft>,
    pending: BTreeMap<u64, Effect>,
    next_request: u64,
    pub loading: bool,
    pub save_states: BTreeMap<u64, SaveState>,
    saved_until: BTreeMap<u64, u64>,
}
impl Notes {
    fn request(&mut self, e: impl FnOnce(u64) -> Effect) -> Effect {
        self.next_request += 1;
        let e = e(self.next_request);
        self.pending.insert(self.next_request, e.clone());
        e
    }
    pub fn load(&mut self) -> Effect {
        self.pending
            .retain(|_, e| !matches!(e, Effect::List { .. }));
        self.loading = true;
        self.request(|request| Effect::List { request })
    }
    pub fn loaded(&mut self, request: u64, result: Result<Vec<Note>, String>) {
        if !matches!(self.pending.remove(&request), Some(Effect::List { .. })) {
            return;
        }
        self.loading = false;
        match result {
            Ok(rows) => {
                self.rows = rows;
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }
    pub fn visible(&self) -> Vec<&Note> {
        let q = self.query.trim().to_lowercase();
        let mut rows: Vec<_> = self
            .rows
            .iter()
            .filter(|n| q.is_empty() || plain_text(&n.text).to_lowercase().contains(&q))
            .collect();
        rows.sort_by_key(|n| std::cmp::Reverse(n.updated_at));
        rows
    }
    pub fn select(&mut self, id: u64) -> Result<(), String> {
        let n = self
            .rows
            .iter()
            .find(|n| n.created_at == id)
            .ok_or("Unknown note")?;
        if !self.drafts.contains_key(&id) {
            while self.drafts.len() >= 32
                || self
                    .drafts
                    .values()
                    .map(|d| d.persisted.len())
                    .sum::<usize>()
                    > 8 * 1024 * 1024
            {
                let candidate = self
                    .drafts
                    .iter()
                    .find(|(key, d)| {
                        Some(**key) != self.selected
                            && d.doc.html() == d.persisted
                            && !self
                                .pending
                                .values()
                                .any(|e| matches!(e,Effect::Save{note_id,..}if note_id==*key))
                    })
                    .map(|(key, _)| *key);
                if let Some(key) = candidate {
                    self.drafts.remove(&key);
                } else {
                    return Err("Finish saving open drafts before opening another note".into());
                }
            }
            let doc = Document::load(&n.text)?;
            self.drafts.insert(
                id,
                Draft {
                    persisted: doc.html(),
                    doc,
                    revision: 0,
                    deadline: 0,
                    undo: vec![],
                    redo: vec![],
                },
            );
        }
        self.selected = Some(id);
        Ok(())
    }
    pub fn document(&self) -> Option<&Document> {
        self.drafts.get(&self.selected?).map(|d| &d.doc)
    }
    pub fn edit(
        &mut self,
        now: u64,
        change: impl FnOnce(&mut Document) -> Result<(), String>,
    ) -> Result<(), String> {
        let id = self.selected.ok_or("No note selected")?;
        let d = self.drafts.get_mut(&id).ok_or("No draft")?;
        let before = d.doc.html();
        if let Err(error) = change(&mut d.doc) {
            d.doc = Document::load(&before)?;
            return Err(error);
        }
        let after = d.doc.html();
        if after != before {
            d.undo.push(Patch::between(&before, &after));
            while d.undo.len() > 100
                || d.undo
                    .iter()
                    .map(|p| p.removed.len() + p.inserted.len())
                    .sum::<usize>()
                    > 4 * 1024 * 1024
            {
                d.undo.remove(0);
            }
            d.redo.clear();
            d.revision += 1;
            d.deadline = now + 260;
            self.save_states.insert(id, SaveState::Dirty);
        }
        Ok(())
    }
    pub fn undo(&mut self, redo: bool, now: u64) -> bool {
        let Some(id) = self.selected else {
            return false;
        };
        let Some(d) = self.drafts.get_mut(&id) else {
            return false;
        };
        let from = if redo { &mut d.redo } else { &mut d.undo };
        let Some(patch) = from.pop() else {
            return false;
        };
        let mut html = d.doc.html();
        let (remove, insert) = if redo {
            (&patch.removed, &patch.inserted)
        } else {
            (&patch.inserted, &patch.removed)
        };
        if html.get(patch.start..patch.start + remove.len()) != Some(remove) {
            return false;
        }
        html.replace_range(patch.start..patch.start + remove.len(), insert);
        let Ok(doc) = Document::load(&html) else {
            return false;
        };
        d.doc = doc;
        if redo {
            d.undo.push(patch);
        } else {
            d.redo.push(patch);
        }
        d.revision += 1;
        d.deadline = now + 260;
        self.save_states.insert(id, SaveState::Dirty);
        true
    }
    pub fn tick(&mut self, now: u64) -> Vec<Effect> {
        let ready: Vec<_> = self
            .drafts
            .iter()
            .filter(|(id, d)| {
                d.doc.html() != d.persisted
                    && d.deadline <= now
                    && self.save_states.get(id) == Some(&SaveState::Dirty)
                    && !self
                        .pending
                        .values()
                        .any(|e| matches!(e,Effect::Save{note_id,..}if note_id==*id))
            })
            .map(|(id, d)| (*id, d.revision, d.doc.html()))
            .collect();
        let mut out = vec![];
        for (id, revision, html) in ready {
            out.push(self.request(|request| Effect::Save {
                request,
                note_id: id,
                revision,
                html,
            }));
            self.save_states.insert(id, SaveState::Saving);
        }
        for (id, expiry) in &self.saved_until {
            if now >= *expiry && self.save_states.get(id) == Some(&SaveState::Saved) {
                self.save_states.insert(*id, SaveState::Idle);
            }
        }
        out
    }
    pub fn saved(&mut self, request: u64, result: Result<(), String>, now: u64) -> bool {
        let Some(Effect::Save {
            note_id,
            revision,
            html,
            ..
        }) = self.pending.remove(&request)
        else {
            return false;
        };
        let Some(d) = self.drafts.get_mut(&note_id) else {
            return false;
        };
        match result {
            Ok(()) => {
                d.persisted = html.clone();
                if let Some(n) = self.rows.iter_mut().find(|n| n.created_at == note_id) {
                    n.text = html;
                    n.updated_at = now;
                }
                let current = d.revision == revision;
                self.save_states.insert(
                    note_id,
                    if current {
                        SaveState::Saved
                    } else {
                        SaveState::Dirty
                    },
                );
                if current {
                    self.saved_until.insert(note_id, now + 1200);
                }
            }
            Err(e) => {
                self.save_states.insert(
                    note_id,
                    if d.revision == revision {
                        SaveState::Error(e)
                    } else {
                        SaveState::Dirty
                    },
                );
            }
        }
        true
    }
    pub fn retry(&mut self, now: u64) {
        if let Some(id) = self.selected {
            if let Some(d) = self.drafts.get_mut(&id) {
                d.deadline = now;
                self.save_states.insert(id, SaveState::Dirty);
            }
        }
    }
    pub fn create(&mut self) -> Effect {
        self.request(|request| Effect::Create {
            request,
            html: "<div>Untitled note</div>".into(),
        })
    }
    pub fn created(&mut self, request: u64, result: Result<Note, String>) -> bool {
        if !matches!(self.pending.remove(&request), Some(Effect::Create { .. })) {
            return false;
        }
        match result {
            Ok(n) => {
                let id = n.created_at;
                self.rows.insert(0, n);
                self.query.clear();
                let _ = self.select(id);
            }
            Err(e) => self.error = Some(e),
        }
        true
    }
    pub fn delete(&mut self, id: u64) -> Effect {
        self.request(|request| Effect::Delete {
            request,
            note_id: id,
        })
    }
    pub fn deleted(&mut self, request: u64, result: Result<(), String>) -> bool {
        let Some(Effect::Delete { note_id, .. }) = self.pending.remove(&request) else {
            return false;
        };
        match result {
            Ok(()) => {
                self.rows.retain(|n| n.created_at != note_id);
                self.drafts.remove(&note_id);
                self.save_states.remove(&note_id);
                self.pending
                    .retain(|_, e| !matches!(e,Effect::Save{note_id:id,..}if *id==note_id));
                if self.selected == Some(note_id) {
                    self.selected = None;
                }
            }
            Err(e) => self.error = Some(e),
        }
        true
    }
    pub fn copy(&self) -> Option<Effect> {
        self.document().map(|d| Effect::Copy(plain_text(&d.html())))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn html_subset_preserves_formatting_and_strips_active_content() {
        let d = Document::load(
            "<div class='x'><strong>A&amp;B</strong><u>雪</u><script>no</script><img src=x></div>",
        )
        .unwrap();
        assert_eq!(d.html(), "<div><strong>A&amp;B</strong><u>雪</u></div>");
        assert_eq!(plain_text(&d.html()), "A&B雪");
        assert_eq!(
            normalize_stored("**title**\nbody").unwrap(),
            "<div>title</div><div>body</div>"
        );
    }
    #[test]
    fn cross_run_edit_format_and_undo() {
        let mut n = Notes::default();
        n.rows.push(Note {
            created_at: 1,
            updated_at: 1,
            text: "<div>A<strong>BC</strong>D</div>".into(),
        });
        n.select(1).unwrap();
        n.edit(0, |d| d.replace(1, 3, "雪")).unwrap();
        assert_eq!(plain_text(&n.document().unwrap().html()), "A雪D");
        assert!(n.undo(false, 1));
        assert_eq!(plain_text(&n.document().unwrap().html()), "ABCD");
        n.edit(2, |d| d.format(0, 4, Style::Underline)).unwrap();
        assert!(n.document().unwrap().html().contains("<u>"));
    }
    #[test]
    fn save_completion_stays_with_owner_and_revision() {
        let mut n = Notes::default();
        for id in [1, 2] {
            n.rows.push(Note {
                created_at: id,
                updated_at: 0,
                text: "<div>old</div>".into(),
            });
        }
        n.select(1).unwrap();
        n.edit(0, |d| d.replace(0, 3, "first")).unwrap();
        assert!(n.tick(259).is_empty());
        let Effect::Save { request, .. } = n.tick(260)[0] else {
            panic!()
        };
        n.edit(261, |d| d.replace(0, 5, "second")).unwrap();
        n.select(2).unwrap();
        n.saved(request, Ok(()), 300);
        assert_eq!(n.selected, Some(2));
        assert_eq!(plain_text(&n.document().unwrap().html()), "old");
        assert_eq!(n.save_states[&1], SaveState::Dirty);
        let Effect::Save { request, .. } = n.tick(521)[0] else {
            panic!()
        };
        n.saved(request, Err("disk full".into()), 522);
        assert!(matches!(n.save_states[&1], SaveState::Error(_)));
        n.select(1).unwrap();
        n.retry(600);
        assert_eq!(n.tick(600).len(), 1);
    }
}

/// Undo retains only the changed HTML span, rather than cloning a document on every keystroke.
#[derive(Debug)]
struct Patch {
    start: usize,
    removed: String,
    inserted: String,
}
impl Patch {
    fn between(before: &str, after: &str) -> Self {
        let mut start = before
            .bytes()
            .zip(after.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        while !before.is_char_boundary(start) || !after.is_char_boundary(start) {
            start -= 1;
        }
        let max = before.len().min(after.len()) - start;
        let mut suffix = before
            .bytes()
            .rev()
            .zip(after.bytes().rev())
            .take(max)
            .take_while(|(a, b)| a == b)
            .count();
        while !before.is_char_boundary(before.len() - suffix)
            || !after.is_char_boundary(after.len() - suffix)
        {
            suffix -= 1;
        }
        Self {
            start,
            removed: before[start..before.len() - suffix].into(),
            inserted: after[start..after.len() - suffix].into(),
        }
    }
}
impl Document {
    /// Enter splits the owning paragraph/list item and retains inline formatting on both sides.
    pub fn enter(&mut self, position: usize) -> Result<(), String> {
        if position > self.text_len() {
            return Err("Caret outside document".into());
        }
        fn len(n: &Node) -> usize {
            match n {
                Node::Text(s) => s.chars().count(),
                Node::Element { tag, children } => {
                    if tag == "br" {
                        1
                    } else {
                        children.iter().map(len).sum()
                    }
                }
            }
        }
        fn split(nodes: Vec<Node>, at: usize) -> (Vec<Node>, Vec<Node>) {
            let (mut left, mut right, mut offset) = (vec![], vec![], 0);
            for n in nodes {
                let size = len(&n);
                if offset + size <= at {
                    left.push(n);
                } else if offset >= at {
                    right.push(n);
                } else {
                    match n {
                        Node::Text(s) => {
                            left.push(Node::Text(s.chars().take(at - offset).collect()));
                            right.push(Node::Text(s.chars().skip(at - offset).collect()));
                        }
                        Node::Element { tag, children } => {
                            let (a, b) = split(children, at - offset);
                            left.push(Node::Element {
                                tag: tag.clone(),
                                children: a,
                            });
                            right.push(Node::Element { tag, children: b });
                        }
                    }
                }
                offset += size;
            }
            (left, right)
        }
        fn br_if_empty(nodes: &mut Vec<Node>) {
            if nodes.is_empty() {
                nodes.push(Node::Element {
                    tag: "br".into(),
                    children: vec![],
                });
            }
        }
        fn visit(nodes: &mut Vec<Node>, at: usize) -> bool {
            let mut offset = 0;
            for i in 0..nodes.len() {
                let size = len(&nodes[i]);
                if at <= offset + size {
                    if let Node::Element { tag, children } = &mut nodes[i] {
                        if ["ul", "ol"].contains(&tag.as_str()) {
                            return visit(children, at - offset);
                        }
                        if ["div", "p", "li"].contains(&tag.as_str()) {
                            let tag = tag.clone();
                            let (a, b) = split(std::mem::take(children), at - offset);
                            let mut a = a;
                            let mut b = b;
                            br_if_empty(&mut a);
                            br_if_empty(&mut b);
                            *children = a;
                            nodes.insert(i + 1, Node::Element { tag, children: b });
                            return true;
                        }
                    }
                    break;
                }
                offset += size;
            }
            false
        }
        if !visit(&mut self.nodes, position) {
            let (a, b) = split(std::mem::take(&mut self.nodes), position);
            self.nodes = vec![
                Node::Element {
                    tag: "div".into(),
                    children: a,
                },
                Node::Element {
                    tag: "div".into(),
                    children: b,
                },
            ];
        }
        Ok(())
    }
    /// Toggling an active style unwraps matching tags only in the selected span.
    pub fn toggle_style(&mut self, start: usize, end: usize, style: Style) -> Result<(), String> {
        if start >= end || end > self.text_len() {
            return Err("Select text to format".into());
        }
        fn count(n: &Node) -> usize {
            match n {
                Node::Text(s) => s.chars().count(),
                Node::Element { tag, children } => {
                    if tag == "br" {
                        1
                    } else {
                        children.iter().map(count).sum()
                    }
                }
            }
        }
        fn active(
            nodes: &[Node],
            offset: &mut usize,
            start: usize,
            end: usize,
            tag: &str,
            inside: bool,
        ) -> bool {
            let mut all = true;
            for n in nodes {
                match n {
                    Node::Text(s) => {
                        let size = s.chars().count();
                        if *offset < end && *offset + size > start {
                            all &= inside;
                        }
                        *offset += size;
                    }
                    Node::Element { tag: t, children } => {
                        if t == "br" {
                            *offset += 1;
                        } else {
                            all &= active(
                                children,
                                offset,
                                start,
                                end,
                                tag,
                                inside
                                    || t == tag
                                    || tag == "strong" && t == "b"
                                    || tag == "em" && t == "i",
                            );
                        }
                    }
                }
            }
            all
        }
        fn split(nodes: Vec<Node>, at: usize) -> (Vec<Node>, Vec<Node>) {
            let (mut a, mut b, mut off) = (vec![], vec![], 0);
            for n in nodes {
                let size = count(&n);
                if off + size <= at {
                    a.push(n);
                } else if off >= at {
                    b.push(n);
                } else {
                    match n {
                        Node::Text(t) => {
                            a.push(Node::Text(t.chars().take(at - off).collect()));
                            b.push(Node::Text(t.chars().skip(at - off).collect()));
                        }
                        Node::Element { tag, children } => {
                            let (left, right) = split(children, at - off);
                            a.push(Node::Element {
                                tag: tag.clone(),
                                children: left,
                            });
                            b.push(Node::Element {
                                tag,
                                children: right,
                            });
                        }
                    }
                }
                off += size;
            }
            (a, b)
        }
        fn unwrap(nodes: &mut Vec<Node>, offset: &mut usize, start: usize, end: usize, tag: &str) {
            let mut out = vec![];
            for mut n in std::mem::take(nodes) {
                let size = count(&n);
                let off = *offset;
                *offset += size;
                if off < end && off + size > start {
                    if let Node::Element { tag: t, children } = &mut n {
                        if t == tag || tag == "strong" && t == "b" || tag == "em" && t == "i" {
                            let lo = start.saturating_sub(off).min(size);
                            let hi = end.saturating_sub(off).min(size);
                            let (before, tail) = split(std::mem::take(children), lo);
                            let (mut inside, after) = split(tail, hi - lo);
                            let t = t.clone();
                            if !before.is_empty() {
                                out.push(Node::Element {
                                    tag: t.clone(),
                                    children: before,
                                });
                            }
                            unwrap(&mut inside, &mut (off + lo), start, end, tag);
                            out.extend(inside);
                            if !after.is_empty() {
                                out.push(Node::Element {
                                    tag: t,
                                    children: after,
                                });
                            }
                            continue;
                        }
                        unwrap(children, &mut off.clone(), start, end, tag);
                    }
                }
                out.push(n);
            }
            *nodes = out;
        }
        let tag = style.tag();
        if active(&self.nodes, &mut 0, start, end, tag, false) {
            unwrap(&mut self.nodes, &mut 0, start, end, tag);
            Ok(())
        } else {
            self.format(start, end, style)
        }
    }
}
pub fn title(html: &str) -> String {
    let text = plain_text(html);
    let first = text.lines().next().unwrap_or("").trim();
    if first.is_empty() {
        "(untitled note)".into()
    } else {
        first.chars().take(100).collect()
    }
}
pub fn summary(html: &str) -> String {
    let text = plain_text(html);
    let s = text.lines().skip(1).collect::<Vec<_>>().join(" ");
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.is_empty() {
        "No body yet".into()
    } else {
        s.chars().take(180).collect()
    }
}
#[cfg(test)]
mod editor_tests {
    use super::*;
    #[test]
    fn lists_continue_and_styles_toggle() {
        let mut d = Document::load("<ul><li><strong>abcd</strong></li></ul>").unwrap();
        d.enter(2).unwrap();
        assert_eq!(
            d.html(),
            "<ul><li><strong>ab</strong></li><li><strong>cd</strong></li></ul>"
        );
        d.toggle_style(0, 4, Style::Bold).unwrap();
        assert_eq!(d.html(), "<ul><li>ab</li><li>cd</li></ul>");
        d.toggle_style(0, 2, Style::Italic).unwrap();
        assert!(d.html().contains("<em>ab</em>"));
    }
    #[test]
    fn title_summary_and_nested_input_bounds() {
        assert_eq!(title("<div><b>Title</b></div><div>Body</div>"), "Title");
        assert_eq!(summary("<div>Title</div><div>Body</div>"), "Body");
        assert!(Document::load(&"<div>".repeat(130)).is_err());
    }
}
#[cfg(test)]
mod partial_style_test {
    use super::*;
    #[test]
    fn unformats_only_selected_text_preserving_outer_block() {
        let mut d = Document::load("<div><strong>abcd</strong></div>").unwrap();
        d.toggle_style(1, 3, Style::Bold).unwrap();
        assert_eq!(
            d.html(),
            "<div><strong>a</strong>bc<strong>d</strong></div>"
        );
    }
}

#[cfg(test)]
mod legacy_html_compatibility_tests {
    use super::*;

    #[test]
    fn self_closing_legacy_breaks_remain_line_breaks() {
        let document = Document::load("<div>A<br/>B<br />C</div>").unwrap();
        assert_eq!(document.html(), "<div>A<br>B<br>C</div>");
    }

    #[test]
    fn named_legacy_entities_keep_their_visible_text() {
        let stored = "<div>&copy; &eacute; &mdash; &unknownEntity;</div>";
        let document = Document::load(stored).unwrap();
        assert_eq!(document.html(), "<div>© é — &amp;unknownEntity;</div>");
        assert_eq!(plain_text(stored), "© é — &unknownEntity;");
    }

    #[test]
    fn legacy_entities_are_decoded_once_with_html_numeric_rules() {
        let document =
            Document::load("<div>&amp;copy; &#x80; &#0; &copy &NotEqualTilde;</div>").unwrap();
        assert_eq!(document.html(), "<div>&amp;copy; € � © ≂̸</div>");
    }
}
