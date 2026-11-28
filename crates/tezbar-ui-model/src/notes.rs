//! Rich HTML document and note save ownership; the core remains the sole note store.
use regex::Regex;
macro_rules! cached_regex {
    ($pattern:literal) => {{
        static RE: std::sync::LazyLock<Regex> =
            std::sync::LazyLock::new(|| Regex::new($pattern).unwrap());
        &*RE
    }};
}
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
const EMPTY: &str = "<div><br></div>";
pub fn format_date(
    ms: u64,
    formatter: &dyn tezbar_core::instant::transform::PresentationFormatter,
) -> String {
    i64::try_from(ms)
        .ok()
        .filter(|ms| chrono::DateTime::from_timestamp_millis(*ms).is_some())
        .map(|ms| formatter.local_timestamp(ms))
        .unwrap_or_else(|| "—".into())
}
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
    static RULES: std::sync::LazyLock<Vec<(Regex, &str)>> = std::sync::LazyLock::new(|| {
        [
            (r"\*\*([^*\n]+)\*\*", "$1"),
            (r"__([^_\n]+)__", "$1"),
            (r"\*([^*\n]+)\*", "$1"),
            (r"_([^_\n]+)_", "$1"),
            (r"`([^`\n]+)`", "$1"),
            (r"\[([^\]]+)\]\([^)]+\)", "$1"),
            (r"(?m)^#{1,6}\s+", ""),
            (r"(?m)^\s*[-*+]\s+", ""),
            (r"(?m)^\s*\d+\.\s+", ""),
        ]
        .into_iter()
        .map(|(p, r)| (Regex::new(p).unwrap(), r))
        .collect()
    });
    for (re, replacement) in RULES.iter() {
        s = re.replace_all(&s, *replacement).into_owned();
    }
    s
}
pub fn sanitize_html(s: &str) -> Result<String, String> {
    if s.len() > 1_048_576 {
        return Err("Note exceeds 1 MiB".into());
    }
    use html5ever::tokenizer::{
        states::RawKind, BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer,
        TokenizerOpts,
    };
    use std::cell::RefCell;
    #[derive(Default)]
    struct Clean {
        html: String,
        suppressed: Option<String>,
    }
    struct Sink(RefCell<Clean>);
    impl TokenSink for Sink {
        type Handle = ();
        fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
            let mut state = self.0.borrow_mut();
            match token {
                Token::TagToken(tag) => {
                    let name = tag.name.as_ref();
                    if state.suppressed.is_some() {
                        if tag.kind == TagKind::EndTag && state.suppressed.as_deref() == Some(name)
                        {
                            state.suppressed = None;
                        }
                    } else if tag.kind == TagKind::StartTag && matches!(name, "script" | "style") {
                        state.suppressed = Some(name.into());
                        return TokenSinkResult::RawData(if name == "script" {
                            RawKind::ScriptData
                        } else {
                            RawKind::Rawtext
                        });
                    } else if TAGS.contains(&name) {
                        if tag.kind == TagKind::StartTag {
                            state.html.push_str(&format!("<{name}>"));
                        } else if name != "br" {
                            state.html.push_str(&format!("</{name}>"));
                        }
                    }
                }
                Token::CharacterTokens(text) if state.suppressed.is_none() => {
                    state.html.push_str(&escape(&text))
                }
                Token::NullCharacterToken if state.suppressed.is_none() => {
                    state.html.push('\u{fffd}')
                }
                _ => {} // comments, doctype, processing instructions and unknown tags carry no visible text
            }
            TokenSinkResult::Continue
        }
    }
    let tokenizer = Tokenizer::new(
        Sink(RefCell::new(Clean::default())),
        TokenizerOpts {
            discard_bom: false,
            ..Default::default()
        },
    );
    let input = BufferQueue::default();
    input.push_back(s.into());
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    let html = tokenizer.sink.0.into_inner().html;
    Ok(if html.trim().is_empty() {
        EMPTY.into()
    } else {
        html.trim().into()
    })
}

pub fn normalize_stored(s: &str) -> Result<String, String> {
    if cached_regex!(r"(?is)<(?:/?[a-z]|!|\?).*>").is_match(s) {
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
    // Normalize raw legacy HTML structurally before the canonical subset's text projection.
    let clean = sanitize_html(s).unwrap_or_default();
    let s = clean.as_str();
    let s = cached_regex!(r"(?i)<br\s*/?\s*>|</(?:div|p|li|h[1-6])>").replace_all(s, "\n");
    let s = s.replace("<li>", "- ");
    let s = cached_regex!(r"<[^>]+>").replace_all(&s, "");
    let s = strip_markdown(&decode(&s))
        .replace('\r', "")
        .replace('\u{a0}', " ");
    let s = cached_regex!(r"[ \t]+\n").replace_all(&s, "\n");
    cached_regex!(r"\n{3,}")
        .replace_all(&s, "\n\n")
        .trim()
        .into()
}
/// Plain text (Markdown included) kept verbatim as one paragraph per line.
pub fn html_from_text(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let text = text.trim_matches('\n');
    if text.trim().is_empty() {
        return EMPTY.into();
    }
    text.split('\n')
        .map(|line| {
            if line.trim().is_empty() {
                EMPTY.into()
            } else {
                format!("<div>{}</div>", escape(line))
            }
        })
        .collect()
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
        let re = cached_regex!(r"</?([a-z]+)>");
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
    /// Markdown source for the rendered note: Markdown typed into the note stays
    /// verbatim, and the editor's bold, italic, underline and lists become theirs.
    pub fn markdown(&self) -> String {
        fn line_start(out: &mut String) {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
        }
        fn wrap(children: &[Node], open: &str, close: &str, out: &mut String) {
            let mut s = String::new();
            inline(children, &mut s);
            let core = s.trim();
            if core.is_empty() {
                out.push_str(&s);
                return;
            }
            // Emphasis markers must touch the text they wrap.
            let start = s.len() - s.trim_start().len();
            let end = s.trim_end().len();
            out.push_str(&s[..start]);
            out.push_str(open);
            out.push_str(core);
            out.push_str(close);
            out.push_str(&s[end..]);
        }
        fn list(ordered: bool, items: &[Node], out: &mut String) {
            for (index, item) in items.iter().enumerate() {
                line_start(out);
                let marker = if ordered {
                    format!("{}. ", index + 1)
                } else {
                    "- ".into()
                };
                let mut body = String::new();
                match item {
                    Node::Element { tag, children } if tag == "li" => inline(children, &mut body),
                    n => inline(std::slice::from_ref(n), &mut body),
                }
                let indent = format!("\n{}", " ".repeat(marker.len()));
                out.push_str(&marker);
                out.push_str(&body.trim_end_matches('\n').replace('\n', &indent));
                out.push('\n');
            }
        }
        fn inline(nodes: &[Node], out: &mut String) {
            for n in nodes {
                match n {
                    Node::Text(t) => out.push_str(t),
                    Node::Element { tag, children } => match tag.as_str() {
                        "br" => out.push('\n'),
                        "strong" | "b" => wrap(children, "**", "**", out),
                        "em" | "i" => wrap(children, "*", "*", out),
                        "u" => wrap(children, "<u>", "</u>", out),
                        "ul" | "ol" => {
                            line_start(out);
                            list(tag == "ol", children, out);
                        }
                        "div" | "p" | "li" => {
                            line_start(out);
                            inline(children, out);
                            line_start(out);
                        }
                        _ => inline(children, out),
                    },
                }
            }
        }
        let mut out = String::new();
        inline(&self.nodes, &mut out);
        out.trim_end().to_string()
    }
    fn html_len(&self) -> usize {
        fn len(n: &Node) -> usize {
            match n {
                Node::Text(t) => t
                    .chars()
                    .map(|c| match c {
                        '&' => 5,
                        '<' | '>' => 4,
                        '"' => 6,
                        '\'' => 6,
                        _ => c.len_utf8(),
                    })
                    .sum(),
                Node::Element { tag, children } => {
                    tag.len()
                        + 2
                        + if tag == "br" {
                            0
                        } else {
                            tag.len() + 3 + children.iter().map(len).sum::<usize>()
                        }
                }
            }
        }
        self.nodes.iter().map(len).sum()
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
        if self.html_len()
            + text
                .chars()
                .map(|c| match c {
                    '&' => 5,
                    '<' | '>' => 4,
                    '"' | '\'' => 6,
                    _ => c.len_utf8(),
                })
                .sum::<usize>()
            > 1_048_576
        {
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
    Message(String),
}
#[derive(Debug)]
struct Draft {
    doc: Document,
    revision: u64,
    persisted: String,
    html: String,
    deadline: u64,
    undo: Vec<Patch>,
    redo: Vec<Patch>,
}
/// Ephemeral native editor offsets in its visual UTF-8 projection. The model keeps
/// them opaque; the input adapter validates them against the restored projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorSelection {
    pub start: usize,
    pub end: usize,
    pub reversed: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UndoResult {
    pub changed: bool,
    pub selection: Option<EditorSelection>,
}
#[derive(Clone, Copy, Debug)]
struct SelectionChange {
    before: EditorSelection,
    after: EditorSelection,
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
    plain_cache: std::cell::RefCell<BTreeMap<u64, (u64, String)>>,
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
                self.plain_cache.get_mut().clear();
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }
    pub fn visible(&self) -> Vec<&Note> {
        let q = self.query.trim().to_lowercase();
        let mut cache = self.plain_cache.borrow_mut();
        let ids: std::collections::BTreeSet<_> = self.rows.iter().map(|n| n.created_at).collect();
        cache.retain(|id, _| ids.contains(id));
        let mut rows: Vec<_> = self
            .rows
            .iter()
            .filter(|n| {
                if q.is_empty() {
                    return true;
                }
                let cached = cache
                    .entry(n.created_at)
                    .or_insert_with(|| (n.updated_at, plain_text(&n.text).to_lowercase()));
                if cached.0 != n.updated_at {
                    *cached = (n.updated_at, plain_text(&n.text).to_lowercase());
                }
                cached.1.contains(&q)
            })
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
                            && d.html == d.persisted
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
                    html: doc.html(),
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
        self.edit_transaction(now, None, |document| change(document).map(|()| None))
    }
    /// Content and selection metadata enter the same bounded undo transaction.
    pub fn edit_with_selection(
        &mut self,
        now: u64,
        before: EditorSelection,
        change: impl FnOnce(&mut Document) -> Result<EditorSelection, String>,
    ) -> Result<(), String> {
        if before.start > before.end {
            return Err("Invalid editor selection".into());
        }
        self.edit_transaction(now, Some(before), |document| {
            let after = change(document)?;
            if after.start > after.end {
                return Err("Invalid editor selection".into());
            }
            Ok(Some(after))
        })
    }
    fn edit_transaction(
        &mut self,
        now: u64,
        selection_before: Option<EditorSelection>,
        change: impl FnOnce(&mut Document) -> Result<Option<EditorSelection>, String>,
    ) -> Result<(), String> {
        let id = self.selected.ok_or("No note selected")?;
        let d = self.drafts.get_mut(&id).ok_or("No draft")?;
        let before = d.html.clone();
        let selection_after = match change(&mut d.doc) {
            Ok(selection) => selection,
            Err(error) => {
                d.doc = Document::load(&before)?;
                return Err(error);
            }
        };
        let after = d.doc.html();
        if after != before {
            d.html = after.clone();
            let mut patch = Patch::between(&before, &after);
            patch.selection = selection_before
                .zip(selection_after)
                .map(|(before, after)| SelectionChange { before, after });
            d.undo.push(patch);
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
        self.undo_with_selection(redo, now).changed
    }
    pub fn undo_with_selection(&mut self, redo: bool, now: u64) -> UndoResult {
        let Some(id) = self.selected else {
            return UndoResult::default();
        };
        let Some(d) = self.drafts.get_mut(&id) else {
            return UndoResult::default();
        };
        let from = if redo { &mut d.redo } else { &mut d.undo };
        let Some(patch) = from.last() else {
            return UndoResult::default();
        };
        let mut html = d.html.clone();
        let (remove, insert) = if redo {
            (&patch.removed, &patch.inserted)
        } else {
            (&patch.inserted, &patch.removed)
        };
        if html.get(patch.start..patch.start + remove.len()) != Some(remove) {
            return UndoResult::default();
        }
        html.replace_range(patch.start..patch.start + remove.len(), insert);
        let Ok(doc) = Document::load(&html) else {
            return UndoResult::default();
        };
        let selection = patch.selection.map(|selection| {
            if redo {
                selection.after
            } else {
                selection.before
            }
        });
        let patch = from.pop().expect("undo patch was checked above");
        d.doc = doc;
        d.html = html;
        if redo {
            d.undo.push(patch);
        } else {
            d.redo.push(patch);
        }
        d.revision += 1;
        d.deadline = now + 260;
        self.save_states.insert(id, SaveState::Dirty);
        UndoResult {
            changed: true,
            selection,
        }
    }
    pub fn tick(&mut self, now: u64) -> Vec<Effect> {
        let ready: Vec<_> = self
            .drafts
            .iter()
            .filter(|(id, d)| {
                d.html != d.persisted
                    && d.deadline <= now
                    && self.save_states.get(id) == Some(&SaveState::Dirty)
                    && !self
                        .pending
                        .values()
                        .any(|e| matches!(e,Effect::Save{note_id,..}if note_id==*id))
            })
            .map(|(id, d)| (*id, d.revision, d.html.clone()))
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
                self.plain_cache.get_mut().remove(&note_id);
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
    /// A new note holding `text` verbatim, so Markdown keeps its syntax.
    pub fn create_from_text(&mut self, text: &str) -> Effect {
        let html = html_from_text(text);
        if html.len() > 1_048_576 {
            return Effect::Message("Text is too large for a note".into());
        }
        self.request(|request| Effect::Create { request, html })
    }
    pub fn created(&mut self, request: u64, result: Result<Note, String>) -> bool {
        if !matches!(self.pending.remove(&request), Some(Effect::Create { .. })) {
            return false;
        }
        match result {
            Ok(n) => {
                let id = n.created_at;
                self.plain_cache.get_mut().remove(&id);
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
                self.plain_cache.get_mut().remove(&note_id);
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
        let d = self.drafts.get(&self.selected?)?;
        let text = plain_text(&d.html);
        Some(if text.is_empty() {
            Effect::Message("Note is empty".into())
        } else {
            Effect::Copy(text)
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notes_render_typed_markdown_verbatim_and_editor_formatting_as_markdown() {
        let source = "**Password: `x`**\r\n\r\n| A | B |\n| --- | --- |\n| 1 | <2> |\n";
        let html = html_from_text(source);
        assert_eq!(
            html,
            "<div>**Password: `x`**</div><div><br></div><div>| A | B |</div>\
             <div>| --- | --- |</div><div>| 1 | &lt;2&gt; |</div>"
        );
        let doc = Document::load(&html).unwrap();
        assert_eq!(
            doc.markdown(),
            "**Password: `x`**\n\n| A | B |\n| --- | --- |\n| 1 | <2> |"
        );
        let doc = Document::load(
            "<div>a <strong>bold </strong>and <em>it</em> <u>u</u></div>\
             <ul><li>one</li><li>two</li></ul><ol><li>x<br>y</li></ol><div><br></div><div>end</div>",
        )
        .unwrap();
        assert_eq!(
            doc.markdown(),
            "a **bold** and *it* <u>u</u>\n- one\n- two\n1. x\n   y\n\nend"
        );
        let mut notes = Notes::default();
        assert!(matches!(
            notes.create_from_text("# Title\n- item"),
            Effect::Create { html, .. } if html == "<div># Title</div><div>- item</div>"
        ));
        assert!(matches!(
            notes.create_from_text(&"x".repeat(1_048_577)),
            Effect::Message(_)
        ));
    }
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
    selection: Option<SelectionChange>,
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
            selection: None,
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

#[cfg(test)]
mod opus_repairs {
    use super::*;
    #[test]
    fn opus_html_structural_tokens() {
        for (s, want) in [
            ("<div><!--StartFragment-->Hello<o:p></o:p></div>", "Hello"),
            ("<div title=\"a>b\">x", "x"),
            (
                "<div><?xml version='1.0'?><span>Office</span></div>",
                "Office",
            ),
        ] {
            let d = Document::load(s).unwrap();
            assert_eq!(plain_text(&d.html()), want);
        }
    }
    #[test]
    fn opus_empty_note_copy() {
        let mut n = Notes::default();
        n.rows.push(Note {
            created_at: 1,
            updated_at: 1,
            text: EMPTY.into(),
        });
        n.select(1).unwrap();
        assert_eq!(n.copy(), Some(Effect::Message("Note is empty".into())));
    }
}

#[cfg(test)]
mod opus_html_fixtures {
    use super::*;
    #[test]
    fn opus_jsdom_office_webkit_structural_reference() {
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("notes/opus_html_fixtures.json")).unwrap();
        for fixture in fixtures["fixtures"].as_array().unwrap() {
            let input = fixture["input"].as_str().unwrap();
            let doc = Document::load(input).unwrap();
            assert_eq!(
                plain_text(&doc.html()),
                fixture["plain"].as_str().unwrap(),
                "{input}"
            );
            assert_eq!(Document::load(&doc.html()).unwrap().html(), doc.html());
        }
    }
    #[test]
    fn opus_notes_revision_cache_and_idle_tick_scaling() {
        let mut notes = Notes::default();
        for id in 0..500 {
            notes.rows.push(Note {
                created_at: id,
                updated_at: 1,
                text: format!("<div>Note {id}</div><div>body &amp; more text</div>"),
            });
        }
        notes.query = "body".into();
        let start = std::time::Instant::now();
        assert_eq!(notes.visible().len(), 500);
        let cold = start.elapsed();
        let start = std::time::Instant::now();
        for _ in 0..20 {
            assert_eq!(notes.visible().len(), 500);
        }
        let warm = start.elapsed();
        assert_eq!(notes.plain_cache.borrow().len(), 500);
        notes.rows[0].text = "<div>different</div>".into();
        notes.rows[0].updated_at += 1;
        assert_eq!(notes.visible().len(), 499);
        for id in 0..32 {
            notes.select(id).unwrap();
        }
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            assert!(notes.tick(1000).is_empty());
        }
        let idle = start.elapsed();
        eprintln!(
            "notes_scaling notes=500 cold_ms={:.3} warm20_ms={:.3} drafts=32 idle1000_ms={:.3}",
            cold.as_secs_f64() * 1000.,
            warm.as_secs_f64() * 1000.,
            idle.as_secs_f64() * 1000.
        );
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    fn selected_notes() -> Notes {
        let mut notes = Notes::default();
        notes.rows.push(Note {
            created_at: 1,
            updated_at: 0,
            text: "<div>A<strong>雪</strong>😀</div>".into(),
        });
        notes.rows.push(Note {
            created_at: 2,
            updated_at: 0,
            text: "<div>Second</div>".into(),
        });
        notes.select(1).unwrap();
        notes
    }
    #[test]
    fn rich_undo_and_redo_restore_the_selection_owned_by_the_same_patch() {
        let mut notes = selected_notes();
        let original = notes.document().unwrap().html();
        let before = EditorSelection {
            start: 1,
            end: 4,
            reversed: true,
        };
        let after = EditorSelection {
            start: 4,
            end: 4,
            reversed: false,
        };
        notes
            .edit_with_selection(0, before, |doc| {
                doc.replace(1, 2, "猫")?;
                Ok(after)
            })
            .unwrap();
        let edited = notes.document().unwrap().html();
        assert!(edited.contains("猫"));
        assert_eq!(
            notes.undo_with_selection(false, 1),
            UndoResult {
                changed: true,
                selection: Some(before)
            }
        );
        assert_eq!(notes.document().unwrap().html(), original);
        assert_eq!(
            notes.undo_with_selection(true, 2),
            UndoResult {
                changed: true,
                selection: Some(after)
            }
        );
        assert_eq!(notes.document().unwrap().html(), edited);
    }
    #[test]
    fn failed_and_selection_only_changes_never_create_content_history() {
        let mut notes = selected_notes();
        let before = EditorSelection {
            start: 1,
            end: 4,
            reversed: true,
        };
        let after = EditorSelection {
            start: 4,
            end: 4,
            reversed: false,
        };
        notes
            .edit_with_selection(0, before, |doc| {
                doc.replace(1, 2, "猫")?;
                Ok(after)
            })
            .unwrap();
        let edited = notes.document().unwrap().html();
        assert!(notes
            .edit_with_selection(1, after, |doc| {
                doc.replace(0, 1, "Z")?;
                Err("Synthetic failure".into())
            })
            .is_err());
        assert_eq!(notes.document().unwrap().html(), edited);
        notes.edit_with_selection(2, after, |_| Ok(before)).unwrap();
        assert_eq!(notes.undo_with_selection(false, 3).selection, Some(before));
        assert!(!notes.undo_with_selection(false, 4).changed);
        notes.edit(5, |doc| doc.replace(0, 1, "B")).unwrap();
        assert_eq!(
            notes.undo_with_selection(false, 6),
            UndoResult {
                changed: true,
                selection: None
            }
        );
    }
    #[test]
    fn selection_history_stays_with_its_note_and_invalid_metadata_rolls_back() {
        let mut notes = selected_notes();
        let a = EditorSelection {
            start: 0,
            end: 1,
            reversed: false,
        };
        let b = EditorSelection {
            start: 1,
            end: 1,
            reversed: false,
        };
        notes
            .edit_with_selection(0, a, |doc| {
                doc.replace(0, 1, "B")?;
                Ok(b)
            })
            .unwrap();
        notes.select(2).unwrap();
        let c = EditorSelection {
            start: 0,
            end: 6,
            reversed: true,
        };
        notes
            .edit_with_selection(1, c, |doc| {
                doc.replace(0, 6, "C")?;
                Ok(b)
            })
            .unwrap();
        assert_eq!(notes.undo_with_selection(false, 2).selection, Some(c));
        notes.select(1).unwrap();
        let before = notes.document().unwrap().html();
        assert!(notes
            .edit_with_selection(3, b, |doc| {
                doc.replace(0, 1, "X")?;
                Ok(EditorSelection {
                    start: 2,
                    end: 1,
                    reversed: false,
                })
            })
            .is_err());
        assert_eq!(notes.document().unwrap().html(), before);
        assert_eq!(notes.undo_with_selection(false, 4).selection, Some(a));
    }
}
