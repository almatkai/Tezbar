//! Native document parsing. A message owns its cache: unrelated stream deltas never reparse history.
use gpui::{prelude::*, *};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::{ops::Range, sync::OnceLock};
use syntect::{easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet};
use unicode_segmentation::UnicodeSegmentation;
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Paragraph,
    Heading(u8),
    Code(String),
    List,
    Table,
    Quote,
    Rule,
    Details,
    Footnote(String),
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub strong: bool,
    pub emphasis: bool,
    pub strike: bool,
    pub code: bool,
    pub underline: bool,
    pub link: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Block {
    pub kind: Kind,
    pub spans: Vec<Span>,
    pub images: Vec<(String, String)>,
    pub checked: Option<bool>,
    pub(crate) list_path: Vec<ListItem>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ListItem {
    list: usize,
    item: usize,
}
struct ListFrame {
    id: usize,
    next_number: Option<u64>,
    item: Option<usize>,
    next_item: usize,
    html: bool,
}
pub(crate) struct ListContext {
    next_id: usize,
    stack: Vec<ListFrame>,
}
impl ListContext {
    pub(crate) fn start(&mut self, number: Option<u64>, html: bool) {
        self.stack.push(ListFrame {
            id: self.next_id,
            next_number: number,
            item: None,
            next_item: 0,
            html,
        });
        self.next_id += 1;
    }
    pub(crate) fn end(&mut self, html: bool) {
        if self.stack.last().is_some_and(|frame| frame.html == html) {
            self.stack.pop();
        }
    }
    pub(crate) fn start_item(&mut self, html: bool) -> String {
        let Some(frame) = self.stack.last_mut().filter(|frame| frame.html == html) else {
            return String::new();
        };
        frame.item = Some(frame.next_item);
        frame.next_item += 1;
        if let Some(number) = &mut frame.next_number {
            let marker = format!("{number}. ");
            *number = number.saturating_add(1);
            marker
        } else {
            "• ".into()
        }
    }
    pub(crate) fn end_item(&mut self, html: bool) {
        if let Some(frame) = self.stack.last_mut().filter(|frame| frame.html == html) {
            frame.item = None;
        }
    }
    pub(crate) fn between_html_items(&self) -> bool {
        self.stack
            .last()
            .is_some_and(|frame| frame.html && frame.item.is_none())
    }
    pub(crate) fn path(&self) -> Vec<ListItem> {
        self.stack
            .iter()
            .filter_map(|frame| {
                frame.item.map(|item| ListItem {
                    list: frame.id,
                    item,
                })
            })
            .collect()
    }
}
impl Block {
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}
#[derive(Default)]
pub struct Document {
    revision: Option<u64>,
    source: String,
    pub blocks: Vec<Block>,
    pub parses: usize,
    pub highlighted: Vec<(usize, Vec<(Range<usize>, Hsla)>)>,
    settled: bool,
    prefix_len: usize,
    prefix: Vec<Block>,
    pub parsed_bytes: usize,
}
pub fn safe_link(raw: &str) -> Option<String> {
    let u = url::Url::parse(raw).ok()?;
    matches!(u.scheme(), "https" | "http" | "mailto").then(|| u.into())
}
pub fn safe_image(raw: &str) -> Option<String> {
    if let Ok(u) = url::Url::parse(raw) {
        return (u.scheme() == "https").then(|| u.into());
    }
    let path = std::path::Path::new(raw);
    (!raw.is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
        && !raw.contains('\\'))
    .then(|| format!("asset:{raw}"))
}
impl Document {
    pub fn update(&mut self, revision: u64, source: &str, settled: bool) -> bool {
        if self.revision == Some(revision) && self.settled == settled {
            return false;
        }
        if settled && (!self.settled || self.source != source) {
            self.blocks = parse(source);
            self.prefix = self.blocks.clone();
            self.prefix_len = source.len();
            self.parsed_bytes += source.len();
            self.parses += 1;
            self.source = source.into();
            self.highlighted.clear();
        } else if self.source != source {
            if !source.starts_with(self.source.get(..self.prefix_len).unwrap_or("")) {
                self.prefix_len = 0;
                self.prefix.clear();
            }
            let split = if settled {
                source.len()
            } else {
                closed_prefix(source)
            };
            if split < self.prefix_len {
                self.prefix_len = 0;
                self.prefix.clear();
            }
            if split > self.prefix_len {
                let chunk = &source[self.prefix_len..split];
                self.prefix.extend(parse_at(chunk, self.prefix_len));
                self.parsed_bytes += chunk.len();
                self.prefix_len = split;
            }
            let tail = &source[self.prefix_len..];
            self.blocks = self.prefix.clone();
            self.blocks.extend(parse_at(tail, self.prefix_len));
            self.parsed_bytes += tail.len();
            self.parses += 1;
            self.source = source.into();
            self.highlighted.clear();
        }
        self.revision = Some(revision);
        self.settled = settled;
        true
    }
    /// Explicitly invoked for visible, settled code blocks. Grammars and themes are never loaded at app boot.
    #[cfg(test)]
    pub fn highlight(&mut self, index: usize) {
        if !self.settled || self.highlighted.iter().any(|(i, _)| *i == index) {
            return;
        }
        let Some(block) = self.blocks.get(index) else {
            return;
        };
        let Kind::Code(language) = &block.kind else {
            return;
        };
        let text = block.text();
        if text.len() > 12000 {
            return;
        }
        let spans = syntax_spans(language.clone(), text, false);
        self.highlighted.push((index, spans));
    }
    pub fn selected_text(&self, range: Range<usize>) -> String {
        let text = self
            .blocks
            .iter()
            .map(Block::text)
            .collect::<Vec<_>>()
            .join("\n");
        let start = range.start.min(text.len());
        let end = range.end.min(text.len());
        if start <= end && text.is_char_boundary(start) && text.is_char_boundary(end) {
            text[start..end].into()
        } else {
            String::new()
        }
    }
}
fn syntax_spans(language: String, text: String, light: bool) -> Vec<(Range<usize>, Hsla)> {
    static SYNTAX: OnceLock<SyntaxSet> = OnceLock::new();
    static THEMES: OnceLock<ThemeSet> = OnceLock::new();
    let set = SYNTAX.get_or_init(SyntaxSet::load_defaults_newlines);
    let themes = THEMES.get_or_init(ThemeSet::load_defaults);
    let syntax = set
        .find_syntax_by_token(&language)
        .unwrap_or_else(|| set.find_syntax_plain_text());
    let theme = &themes.themes[if light {
        "InspiredGitHub"
    } else {
        "base16-ocean.dark"
    }];
    let mut h = HighlightLines::new(syntax, theme);
    let mut at = 0;
    let mut spans = vec![];
    for line in text.split_inclusive('\n') {
        if let Ok(parts) = h.highlight_line(line, set) {
            for (style, text) in parts {
                let end = at + text.len();
                spans.push((
                    at..end,
                    rgb((style.foreground.r as u32) << 16
                        | (style.foreground.g as u32) << 8
                        | style.foreground.b as u32)
                    .into(),
                ));
                at = end;
            }
        }
    }
    spans
}
pub(crate) fn append_span(target: &mut Vec<Span>, span: Span) {
    if let Some(last) = target.last_mut() {
        let mut left = last.clone();
        left.text.clear();
        let mut right = span.clone();
        right.text.clear();
        if left == right {
            last.text.push_str(&span.text);
            return;
        }
    }
    target.push(span);
}
fn push_autolinks(span: Span, target: &mut Vec<Span>) {
    if span.link.is_some() || span.code {
        target.push(span);
        return;
    }
    for word in span.text.split_inclusive(char::is_whitespace) {
        let token = word.trim_end();
        let tail = &word[token.len()..];
        let clean = token.trim_end_matches(['.', ',', ';', ')', ']']);
        if let Some(link) = safe_link(clean) {
            target.push(Span {
                text: clean.into(),
                link: Some(link),
                ..span.clone()
            });
            let suffix = format!("{}{}", &token[clean.len()..], tail);
            if !suffix.is_empty() {
                target.push(Span {
                    text: suffix,
                    ..span.clone()
                });
            }
        } else {
            target.push(Span {
                text: word.into(),
                ..span.clone()
            });
        }
    }
}
fn closed_prefix(source: &str) -> usize {
    let mut fence: Option<char> = None;
    let mut at = 0;
    let mut committed = 0;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let marker = trimmed.chars().next().unwrap();
            if fence == Some(marker) {
                fence = None;
            } else if fence.is_none() {
                fence = Some(marker);
            }
        }
        at += line.len();
        if line.trim().is_empty() && fence.is_none() {
            committed = at;
        }
    }
    committed
}
fn parse(source: &str) -> Vec<Block> {
    parse_at(source, 0)
}
fn parse_at(source: &str, offset: usize) -> Vec<Block> {
    let mut blocks = vec![];
    let mut current = Block {
        kind: Kind::Paragraph,
        spans: vec![],
        images: vec![],
        checked: None,
        list_path: vec![],
    };
    let mut style = Span::default();
    let mut image: Option<(String, String)> = None;
    // Each incremental chunk has a disjoint source-byte namespace. List identities
    // cannot collide when a cached prefix is joined to a separately parsed tail.
    let mut lists = ListContext {
        next_id: offset,
        stack: vec![],
    };
    let mut html = crate::markdown_html::Html::default();
    let flush = |blocks: &mut Vec<Block>, current: &mut Block, lists: &ListContext| {
        if matches!(current.kind, Kind::Details) {
            current.spans.push(Span {
                text: "\n".into(),
                ..Default::default()
            });
            return;
        }
        if !current.spans.is_empty() || !current.images.is_empty() {
            current.list_path = lists.path();
            blocks.push(current.clone());
        }
        *current = Block {
            kind: Kind::Paragraph,
            spans: vec![],
            images: vec![],
            checked: None,
            list_path: vec![],
        };
    };
    for event in Parser::new_ext(
        source,
        Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_FOOTNOTES,
    ) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    flush(&mut blocks, &mut current, &lists);
                    if !matches!(current.kind, Kind::Details) {
                        current.kind = Kind::Heading(level as u8);
                    }
                }
                Tag::CodeBlock(kind) => {
                    flush(&mut blocks, &mut current, &lists);
                    if !matches!(current.kind, Kind::Details) {
                        current.kind = Kind::Code(match kind {
                            CodeBlockKind::Fenced(s) => s.into_string(),
                            _ => String::new(),
                        });
                    }
                }
                Tag::List(start) => {
                    flush(&mut blocks, &mut current, &lists);
                    lists.start(start, false);
                }
                Tag::Item => {
                    flush(&mut blocks, &mut current, &lists);
                    if !matches!(current.kind, Kind::Details) {
                        current.kind = Kind::List;
                    }
                    let marker = lists.start_item(false);
                    current.spans.push(Span {
                        text: marker,
                        ..Default::default()
                    });
                }
                Tag::Table(_) => {
                    flush(&mut blocks, &mut current, &lists);
                    if !matches!(current.kind, Kind::Details) {
                        current.kind = Kind::Table;
                    }
                }
                Tag::BlockQuote(_) => {
                    flush(&mut blocks, &mut current, &lists);
                    if !matches!(current.kind, Kind::Details) {
                        current.kind = Kind::Quote;
                    }
                }
                Tag::FootnoteDefinition(label) => {
                    flush(&mut blocks, &mut current, &lists);
                    current.kind = Kind::Footnote(label.to_string());
                    current.spans.push(Span {
                        text: format!("[{}] ", label),
                        ..Default::default()
                    });
                }
                Tag::Strong => style.strong = true,
                Tag::Emphasis => style.emphasis = true,
                Tag::Strikethrough => style.strike = true,
                Tag::Link { dest_url, .. } => style.link = safe_link(&dest_url),
                Tag::Image { dest_url, .. } => {
                    image = Some((dest_url.into_string(), String::new()))
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Strong => style.strong = false,
                TagEnd::Emphasis => style.emphasis = false,
                TagEnd::Strikethrough => style.strike = false,
                TagEnd::Link => style.link = None,
                TagEnd::Image => {
                    if let Some((src, alt)) = image.take() {
                        if let Some(src) = safe_image(&src) {
                            current.images.push((src, alt));
                        }
                    }
                }
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::CodeBlock | TagEnd::Table => {
                    flush(&mut blocks, &mut current, &lists)
                }
                TagEnd::Item => {
                    flush(&mut blocks, &mut current, &lists);
                    lists.end_item(false);
                }
                TagEnd::List(_) => {
                    flush(&mut blocks, &mut current, &lists);
                    lists.end(false);
                }
                TagEnd::TableCell => current.spans.push(Span {
                    text: "\t".into(),
                    ..Default::default()
                }),
                TagEnd::TableRow => current.spans.push(Span {
                    text: "\n".into(),
                    ..Default::default()
                }),
                _ => {}
            },
            Event::Code(text) => {
                let mut span = style.clone();
                span.code = true;
                span.text = text.into_string();
                current.spans.push(span);
            }
            Event::Text(text) => {
                if let Some((_, alt)) = &mut image {
                    alt.push_str(&text);
                } else {
                    let mut span = style.clone();
                    span.code = matches!(current.kind, Kind::Code(_));
                    span.text = text.into_string();
                    push_autolinks(span, &mut current.spans);
                }
            }
            Event::SoftBreak | Event::HardBreak => current.spans.push(Span {
                text: "\n".into(),
                ..Default::default()
            }),
            Event::TaskListMarker(checked) => {
                current.checked = Some(checked);
                if !current.spans.is_empty() {
                    current.spans.remove(0);
                }
            }
            Event::FootnoteReference(label) => current.spans.push(Span {
                text: format!("[{}]", label),
                link: Some(format!("#footnote:{label}")),
                ..Default::default()
            }),
            Event::Rule => {
                flush(&mut blocks, &mut current, &lists);
                blocks.push(Block {
                    kind: Kind::Rule,
                    spans: vec![],
                    images: vec![],
                    checked: None,
                    list_path: lists.path(),
                });
            }
            // Raw markup is never executed, nor passed to a web engine.
            Event::Html(fragment) | Event::InlineHtml(fragment) => {
                html.fragment(&fragment, &mut blocks, &mut current, &mut style, &mut lists)
            }
            _ => {}
        }
    }
    flush(&mut blocks, &mut current, &lists);
    blocks
}
#[derive(Clone, Debug)]
pub enum DocumentEffect {
    OpenLink(String),
    Image(ImageTicket),
    Copy(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageTicket {
    pub source: String,
    generation: u64,
}
enum ImageState {
    Loading,
    Ready(std::sync::Weak<RenderImage>),
    Unavailable,
}
pub struct MarkdownView {
    pub document: Document,
    pub selection: Option<Range<usize>>,
    anchor: Option<usize>,
    focus: Option<FocusHandle>,
    images: std::collections::HashMap<String, (ImageTicket, ImageState)>,
    image_generation: u64,
    expanded: std::collections::HashSet<usize>,
    highlight_task: Option<Task<()>>,
    highlight_theme: Option<bool>,
    scroll: ScrollHandle,
}
impl gpui::EventEmitter<DocumentEffect> for MarkdownView {}
impl Default for MarkdownView {
    fn default() -> Self {
        Self {
            document: Document::default(),
            selection: None,
            anchor: None,
            focus: None,
            images: Default::default(),
            image_generation: 0,
            expanded: Default::default(),
            highlight_task: None,
            highlight_theme: None,
            scroll: ScrollHandle::new(),
        }
    }
}
impl MarkdownView {
    pub fn set_content(
        &mut self,
        revision: u64,
        content: &str,
        settled: bool,
        cx: &mut Context<Self>,
    ) {
        if self.document.update(revision, content, settled) {
            self.highlight_task = None;
            self.selection = None;
            self.anchor = None;
            self.discover_images(cx);
            cx.notify();
        }
    }
    fn discover_images(&mut self, cx: &mut Context<Self>) {
        let mut sources = Vec::new();
        for (source, _) in self.document.blocks.iter().flat_map(|b| &b.images) {
            if !sources.contains(source) && sources.len() < 128 {
                sources.push(source.clone());
            }
        }
        self.images.retain(|source, _| sources.contains(source));
        for source in sources {
            if self.images.contains_key(&source) {
                continue;
            }
            self.image_generation = self.image_generation.wrapping_add(1);
            let ticket = ImageTicket {
                source: source.clone(),
                generation: self.image_generation,
            };
            self.images
                .insert(source, (ticket.clone(), ImageState::Loading));
            cx.emit(DocumentEffect::Image(ticket));
        }
    }
    /// Pending sources are discovered by content updates, never by Render.
    pub fn image_requests(&self) -> Vec<ImageTicket> {
        self.images
            .values()
            .filter_map(|(ticket, state)| {
                matches!(state, ImageState::Loading).then(|| ticket.clone())
            })
            .collect()
    }
    /// The adapter's bounded ImageCache owns decoded bytes and uploaded textures.
    /// A document keeps only a weak reference, so eviction/hide can release them.
    /// Removed/re-added sources get a new ticket and reject an old decode result.
    pub fn set_image(
        &mut self,
        ticket: &ImageTicket,
        image: Option<std::sync::Arc<RenderImage>>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((current, state)) = self.images.get_mut(&ticket.source) else {
            return false;
        };
        if current != ticket {
            return false;
        }
        *state = image.map_or(ImageState::Unavailable, |image| {
            ImageState::Ready(std::sync::Arc::downgrade(&image))
        });
        cx.notify();
        true
    }
    /// Call from an explicit show/retry after the adapter released its image cache.
    pub fn reset_images(&mut self, cx: &mut Context<Self>) {
        self.images.clear();
        self.discover_images(cx);
        cx.notify();
    }
}
impl MarkdownView {
    fn open_link(&mut self, url: String, cx: &mut Context<Self>) {
        if let Some(label) = url.strip_prefix("#footnote:") {
            if let Some(index) = self
                .document
                .blocks
                .iter()
                .position(|b| matches!(&b.kind,Kind::Footnote(id) if id == label))
            {
                if let Some(row) = block_groups(&self.document.blocks)
                    .iter()
                    .position(|group| group.contains(&index))
                {
                    self.scroll.scroll_to_item(row);
                }
                cx.notify();
            }
        } else {
            cx.emit(DocumentEffect::OpenLink(url));
        }
    }
    fn highlight_visible(&mut self, index: usize, light: bool, cx: &mut Context<Self>) {
        if !self.document.settled
            || self.document.highlighted.iter().any(|(i, _)| *i == index)
            || self.highlight_task.as_ref().is_some_and(|t| !t.is_ready())
        {
            return;
        }
        let Some(block) = self.document.blocks.get(index) else {
            return;
        };
        let Kind::Code(language) = &block.kind else {
            return;
        };
        let text = block.text();
        if text.len() > 12000 {
            return;
        }
        static RUNNING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        if RUNNING
            .fetch_update(
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
                |n| (n < 2).then_some(n + 1),
            )
            .is_err()
        {
            return;
        }
        struct Admission;
        impl Drop for Admission {
            fn drop(&mut self) {
                RUNNING.fetch_sub(1, std::sync::atomic::Ordering::Release);
            }
        }
        let admission = Admission;
        let revision = self.document.revision;
        let language = language.clone();
        let work = cx.background_executor().spawn(async move {
            let _admission = admission;
            syntax_spans(language, text, light)
        });
        self.highlight_task = Some(cx.spawn(async move |view, cx| {
            let spans = work.await;
            let _ = view.update(cx, |view, cx| {
                if view.document.revision == revision && view.highlight_theme == Some(light) {
                    if view.document.highlighted.len() >= 32 {
                        view.document.highlighted.remove(0);
                    }
                    view.document.highlighted.push((index, spans));
                    cx.notify();
                }
            });
        }));
    }
}
// Root scroll rows and accessibility containers use list identity, not adjacency or
// block kind. A code block or paragraph can be a continuation of the same list item.
fn block_groups(blocks: &[Block]) -> Vec<Range<usize>> {
    block_groups_at(blocks, 0..blocks.len(), 0)
}
fn block_groups_at(blocks: &[Block], range: Range<usize>, depth: usize) -> Vec<Range<usize>> {
    let mut groups = Vec::new();
    let mut index = range.start;
    while index < range.end {
        let start = index;
        let list = blocks[start].list_path.get(depth).map(|item| item.list);
        index += 1;
        if let Some(list) = list {
            while index < range.end
                && blocks[index]
                    .list_path
                    .get(depth)
                    .is_some_and(|item| item.list == list)
            {
                index += 1;
            }
        }
        groups.push(start..index);
    }
    groups
}
fn render_block_groups(
    blocks: &[Block],
    rows: &mut [Option<Stateful<Div>>],
    range: Range<usize>,
    depth: usize,
) -> Vec<Stateful<Div>> {
    let mut rendered = Vec::new();
    for group in block_groups_at(blocks, range, depth) {
        if let Some(first) = blocks[group.start].list_path.get(depth) {
            let mut items = Vec::new();
            let mut start = group.start;
            while start < group.end {
                let item = blocks[start].list_path[depth];
                let mut end = start + 1;
                while end < group.end && blocks[end].list_path.get(depth) == Some(&item) {
                    end += 1;
                }
                items.push(
                    div()
                        .id(("list-item", start))
                        .role(Role::ListItem)
                        .flex()
                        .flex_col()
                        .children(render_block_groups(blocks, rows, start..end, depth + 1)),
                );
                start = end;
            }
            rendered.push(
                div()
                    .id(("list", first.list))
                    .role(Role::List)
                    .flex()
                    .flex_col()
                    .children(items),
            );
        } else if let Some(row) = rows[group.start].take() {
            rendered.push(row);
        }
    }
    rendered
}
impl Render for MarkdownView {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = crate::tokens::tokens(w);
        let focus = self.focus.get_or_insert_with(|| cx.focus_handle()).clone();
        let mut rows = vec![];
        let mut base = 0;
        let light = t.light;
        if self.highlight_theme != Some(light) {
            self.highlight_theme = Some(light);
            self.highlight_task = None;
            self.document.highlighted.clear();
        }
        for (index, block) in self.document.blocks.iter().enumerate() {
            let text = block.text();
            let block_len = text.len();
            if matches!(block.kind, Kind::Details) {
                let summary = text.lines().next().unwrap_or("Details").to_owned();
                let expanded = self.expanded.contains(&index);
                rows.push(
                    div()
                        .id(index)
                        .child(
                            crate::controls::button(
                                ("summary", index),
                                summary,
                                crate::controls::ButtonTone::Quiet,
                                false,
                                t,
                            )
                            .aria_expanded(expanded)
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if !this.expanded.remove(&index) {
                                        this.expanded.insert(index);
                                    }
                                    cx.notify();
                                },
                            )),
                        )
                        .when(expanded, |d| {
                            d.child(text.lines().skip(1).collect::<Vec<_>>().join("\n"))
                        }),
                );
                base += block_len + 1;
                continue;
            }
            let role = match block.kind {
                Kind::Heading(_) => Role::Heading,
                Kind::Code(_) => Role::Code,
                Kind::Table => Role::Table,
                _ => Role::Paragraph,
            };
            let mut row = div()
                .id(index)
                .flex()
                .flex_col()
                .gap(px(4. * t.zoom))
                .py(px(4. * t.zoom))
                .text_size(px(t.zoom
                    * match block.kind {
                        Kind::Heading(1) => 22.,
                        Kind::Heading(2) => 18.,
                        Kind::Heading(_) => 15.,
                        _ => 13.,
                    }));
            if let Some(checked) = block.checked {
                row = row.child(crate::controls::checkbox(
                    ("task", index),
                    text.clone(),
                    checked,
                    t,
                ));
            }
            if let Kind::Code(ref language) = block.kind {
                let code = text.clone();
                row = row
                    .font_family(crate::tokens::font_family(cx, true))
                    .bg(t.field)
                    .p(px(8. * t.zoom))
                    .child(
                        div()
                            .flex()
                            .gap(px(8. * t.zoom))
                            .child(div().text_color(t.text3).child(language.clone()))
                            .child(
                                crate::controls::button(
                                    ("copy-code", index),
                                    "Copy",
                                    crate::controls::ButtonTone::Quiet,
                                    false,
                                    t,
                                )
                                .on_click(cx.listener(
                                    move |_, _, _, cx| cx.emit(DocumentEffect::Copy(code.clone())),
                                )),
                            ),
                    );
            }
            let selected = self
                .selection
                .as_ref()
                .map(|r| {
                    r.start.saturating_sub(base).min(block_len)
                        ..r.end.saturating_sub(base).min(block_len)
                })
                .filter(|r| !r.is_empty());
            let syntax = self
                .document
                .highlighted
                .iter()
                .find(|(i, _)| *i == index)
                .map(|(_, s)| s.as_slice())
                .unwrap_or(&[]);
            let highlights = highlights(block, selected.clone(), syntax, t);
            let styled = StyledText::new(text.clone()).with_highlights(highlights);
            let layout = styled.layout().clone();
            let down = layout.clone();
            let drag = layout.clone();
            let click = layout.clone();
            let mut at = 0;
            let links = block
                .spans
                .iter()
                .filter_map(|span| {
                    let range = at..at + span.text.len();
                    at = range.end;
                    span.link
                        .clone()
                        .map(|link| (range, span.text.clone(), link))
                })
                .collect::<Vec<_>>();
            let click_links = links.clone();
            let body = div()
                .cursor(CursorStyle::IBeam)
                .child(styled)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, e: &MouseDownEvent, w, cx| {
                        let index = down.index_for_position(e.position).unwrap_or_else(|i| i);
                        let anchor = base + index;
                        this.anchor = Some(anchor);
                        this.selection = Some(anchor..anchor);
                        if let Some(focus) = &this.focus {
                            w.focus(focus, cx);
                        }
                        cx.notify();
                    }),
                )
                .on_mouse_move(cx.listener(move |this, e: &MouseMoveEvent, _, cx| {
                    if let Some(anchor) = this.anchor {
                        let index =
                            base + drag.index_for_position(e.position).unwrap_or_else(|i| i);
                        this.selection = Some(anchor.min(index)..anchor.max(index));
                        cx.notify();
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |this, e: &MouseUpEvent, _, cx| {
                        this.anchor = None;
                        if this.selection.as_ref().is_none_or(|r| r.is_empty()) {
                            let index = click.index_for_position(e.position).unwrap_or_else(|i| i);
                            if let Some((_, _, url)) = click_links
                                .iter()
                                .find(|(range, _, _)| range.contains(&index))
                            {
                                this.open_link(url.clone(), cx);
                            }
                        }
                    }),
                );
            row = row.child(RichBlock {
                inner: body.into_any_element(),
                layout,
                text,
                role,
                level: match block.kind {
                    Kind::Heading(level) => Some(level as usize),
                    _ => None,
                },
                links,
                selection: selected,
                owner: cx.entity(),
                code_index: matches!(block.kind, Kind::Code(_)).then_some(index),
                light,
            });
            for (i, (url, alt)) in block.images.iter().enumerate() {
                let image = self.images.get(url).and_then(|(_, state)| match state {
                    ImageState::Ready(image) => image.upgrade(),
                    _ => None,
                });
                let placeholder = match self.images.get(url).map(|(_, state)| state) {
                    Some(ImageState::Loading) => "Loading image…",
                    _ => "Image unavailable",
                };
                let mut image_row = div()
                    .id(("image", i))
                    .role(Role::Image)
                    .aria_label(alt.clone());
                image_row = if let Some(image) = image {
                    image_row.child(img(image).max_w_full().max_h(px(320. * t.zoom)))
                } else {
                    image_row
                        .text_color(t.text3)
                        .child(format!("{placeholder} · {alt}"))
                };
                row = row.child(image_row);
            }
            rows.push(row);
            base += block_len + 1;
        }
        let mut rows = rows.into_iter().map(Some).collect::<Vec<_>>();
        let children = render_block_groups(
            &self.document.blocks,
            &mut rows,
            0..self.document.blocks.len(),
            0,
        );
        div()
            .id("document")
            .role(Role::Document)
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .track_focus(&focus)
            .key_context("MarkdownDocument")
            .size_full()
            .flex()
            .flex_col()
            .text_color(t.text1)
            .text_size(px(13. * t.zoom))
            .line_height(relative(1.5))
            .on_action(cx.listener(|this, _: &crate::text_input::Copy, _, cx| {
                if let Some(range) = &this.selection {
                    cx.emit(DocumentEffect::Copy(
                        this.document.selected_text(range.clone()),
                    ));
                }
            }))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.anchor = None),
            )
            .children(children)
    }
}
fn highlights(
    block: &Block,
    selection: Option<Range<usize>>,
    syntax: &[(Range<usize>, Hsla)],
    t: crate::tokens::Tokens,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let text = block.text();
    let mut boundaries = std::collections::BTreeSet::from([0, text.len()]);
    let mut at = 0;
    let spans = block
        .spans
        .iter()
        .map(|span| {
            let range = at..at + span.text.len();
            at = range.end;
            boundaries.insert(range.start);
            boundaries.insert(range.end);
            (range, span)
        })
        .collect::<Vec<_>>();
    if let Some(r) = &selection {
        boundaries.insert(r.start);
        boundaries.insert(r.end);
    }
    for (r, _) in syntax {
        boundaries.insert(r.start);
        boundaries.insert(r.end);
    }
    let boundaries = boundaries.into_iter().collect::<Vec<_>>();
    boundaries
        .windows(2)
        .filter_map(|pair| {
            let range = pair[0]..pair[1];
            if range.is_empty() {
                return None;
            }
            let span = spans
                .iter()
                .find(|(r, _)| r.contains(&range.start))
                .map(|(_, s)| *s);
            let mut style = HighlightStyle::default();
            if let Some(span) = span {
                style.font_weight = span.strong.then_some(FontWeight::BOLD);
                style.font_style = span.emphasis.then_some(FontStyle::Italic);
                style.underline = span.underline.then_some(UnderlineStyle {
                    color: None,
                    thickness: px(1.),
                    wavy: false,
                });
                style.strikethrough = span.strike.then_some(StrikethroughStyle {
                    thickness: px(1.),
                    color: None,
                });
                if span.code {
                    style.background_color = Some(t.field);
                }
                if span.link.is_some() {
                    style.color = Some(t.link);
                }
            }
            if let Some((_, color)) = syntax.iter().find(|(r, _)| r.contains(&range.start)) {
                style.color = Some(*color);
            }
            if selection.as_ref().is_some_and(|r| r.contains(&range.start)) {
                style.background_color = Some(t.accent);
                style.color = Some(rgb(0xffffff).into());
            }
            Some((range, style))
        })
        .collect()
}
struct RichBlock {
    inner: AnyElement,
    layout: TextLayout,
    text: String,
    role: Role,
    level: Option<usize>,
    links: Vec<(Range<usize>, String, String)>,
    selection: Option<Range<usize>>,
    owner: Entity<MarkdownView>,
    code_index: Option<usize>,
    light: bool,
}
impl IntoElement for RichBlock {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for RichBlock {
    type RequestLayoutState = ();
    type PrepaintState = Vec<(accesskit::NodeId, String)>;
    fn id(&self) -> Option<ElementId> {
        Some("rich-text".into())
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn a11y_role(&self) -> Option<Role> {
        Some(self.role)
    }
    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        node.set_label(self.text.clone());
        if let Some(level) = self.level {
            node.set_level(level);
        }
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        w: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.inner.request_layout(w, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        w: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.inner.prepaint(w, cx);
        vec![]
    }
    fn a11y_synthetic_children(
        &mut self,
        actions: &mut Self::PrepaintState,
        b: &mut A11ySubtreeBuilder,
    ) {
        if self.role == Role::Table {
            let mut rows = vec![];
            let mut column_count = 0;
            for (r, line) in self.text.lines().enumerate() {
                let mut cells = vec![];
                for (c, text) in line.trim_end_matches('\t').split('\t').enumerate() {
                    let id = b.synthetic_node_id(("cell", r, c));
                    let mut cell = accesskit::Node::new(if r == 0 {
                        Role::ColumnHeader
                    } else {
                        Role::Cell
                    });
                    cell.set_label(text.to_owned());
                    cell.set_row_index(r);
                    cell.set_column_index(c);
                    b.push_child(id, cell);
                    cells.push(id);
                }
                column_count = column_count.max(cells.len());
                let id = b.synthetic_node_id(("row", r));
                let mut row = accesskit::Node::new(Role::Row);
                row.set_row_index(r);
                row.set_children(cells);
                b.push_child(id, row);
                rows.push(id);
            }
            b.parent_node().set_row_count(rows.len());
            b.parent_node().set_column_count(column_count);
            b.parent_node().set_children(rows);
            return;
        }
        let id = b.synthetic_node_id("text");
        let mut run = accesskit::Node::new(Role::TextRun);
        run.set_value(self.text.clone());
        run.set_character_lengths(
            self.text
                .graphemes(true)
                .map(|c| c.len().min(255) as u8)
                .collect::<Vec<_>>(),
        );
        b.push_child(id, run);
        if let Some(selection) = &self.selection {
            let pos = |at| accesskit::TextPosition {
                node: id,
                character_index: self.text[..at].graphemes(true).count(),
            };
            b.parent_node()
                .set_text_selection(accesskit::TextSelection {
                    anchor: pos(selection.start),
                    focus: pos(selection.end),
                });
        }
        for (i, (range, label, url)) in self.links.iter().enumerate() {
            let id = b.synthetic_node_id(("link", i));
            let mut link = accesskit::Node::new(Role::Link);
            link.set_label(label.clone());
            link.set_url(url.clone());
            link.add_action(AccessibleAction::Click);
            if let (Some(start), Some(end)) = (
                self.layout.position_for_index(range.start),
                self.layout.position_for_index(range.end),
            ) {
                link.set_bounds(accesskit::Rect {
                    x0: f64::from(start.x),
                    y0: f64::from(start.y),
                    x1: f64::from(end.x).max(f64::from(start.x) + 1.),
                    y1: f64::from(end.y) + 20.,
                });
            }
            b.push_child(id, link);
            actions.push((id, url.clone()));
        }
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        actions: &mut Self::PrepaintState,
        w: &mut Window,
        cx: &mut App,
    ) {
        if let Some(index) = self.code_index {
            if bounds.intersects(&Bounds::new(point(px(0.), px(0.)), w.viewport_size())) {
                self.owner
                    .update(cx, |view, cx| view.highlight_visible(index, self.light, cx));
            }
        }

        self.inner.paint(w, cx);
        for (id, url) in actions.drain(..) {
            let owner = self.owner.clone();
            w.on_a11y_action(id, AccessibleAction::Click, move |_, _, cx| {
                owner.update(cx, |view, cx| view.open_link(url.clone(), cx))
            });
        }
    }
}
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new(
        if cfg!(target_os = "macos") {
            "cmd-c"
        } else {
            "ctrl-c"
        },
        crate::text_input::Copy,
        Some("MarkdownDocument"),
    )]);
}
#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;
    #[::core::prelude::v1::test]
    fn links_reject_executable_schemes() {
        for s in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,hi",
        ] {
            assert_eq!(safe_link(s), None);
        }
        assert!(safe_link("https://tezbar.app").is_some());
        assert!(safe_link("mailto:test@example.com").is_some());
        assert!(safe_image("http://example.com/a.png").is_none());
    }
    #[::core::prelude::v1::test]
    fn revision_cache_does_not_reparse_history() {
        let mut a = Document::default();
        a.update(1, "# Heading\n\nText", true);
        a.update(1, "# Heading\n\nText", true);
        assert_eq!(a.parses, 1);
        let mut b = Document::default();
        for n in 0..10 {
            b.update(n, &format!("token {n}"), false);
        }
        assert_eq!(a.parses, 1);
        assert_eq!(b.parses, 10);
    }
    #[::core::prelude::v1::test]
    fn native_tasks_tables_links_and_open_fences() {
        let b = parse(
            "# Title\n\n- [x] Done\n\n[a](javascript:bad)\n\n|a|b|\n|-|-|\n|1|2|\n\n```rs\nlet x=1;",
        );
        assert!(b.iter().any(|b| b.checked == Some(true)));
        assert!(b.iter().any(|b| matches!(b.kind, Kind::Heading(1))));
        assert!(b.iter().any(|b| matches!(b.kind, Kind::Code(_))));
        assert!(b.iter().flat_map(|b| &b.spans).all(|s| s.link.is_none()));
    }
    #[::core::prelude::v1::test]
    fn raw_html_never_executes() {
        let b = parse("<script>alert(1)</script>\n\nVisible");
        assert_eq!(
            b.iter().map(Block::text).collect::<String>().trim(),
            "Visible"
        );
    }
    #[::core::prelude::v1::test]
    fn highlight_stays_lazy_while_streaming() {
        let mut d = Document::default();
        d.update(1, "```rust\nlet x=1;\n```", false);
        d.highlight(0);
        assert!(d.highlighted.is_empty());
        d.update(1, "```rust\nlet x=1;\n```", true);
        d.highlight(0);
        assert_eq!(d.highlighted.len(), 1);
        assert_eq!(d.parses, 2);
    }
    #[::core::prelude::v1::test]
    fn streaming_reparses_only_unclosed_tail() {
        let mut d = Document::default();
        let prefix = "A settled paragraph.\n\n".repeat(300);
        d.update(1, &prefix, false);
        let before = d.parsed_bytes;
        for n in 2..20 {
            d.update(n, &format!("{prefix}tail {n}"), false);
        }
        assert!(d.parsed_bytes - before < 1024);
        assert_eq!(d.blocks.len(), 301);
    }

    #[::core::prelude::v1::test]
    fn fixed_html_subset_is_native_and_attribute_safe() {
        let blocks = parse(
            "<div class='ignored'><strong>Bold &amp; safe</strong><br/><a href='javascript:bad'>Never execute</a><img src='https://example.com/image.png' alt='Example'/></div>",
        );
        assert!(
            blocks
                .iter()
                .flat_map(|b| &b.spans)
                .any(|s| s.strong && s.text.contains("Bold & safe"))
        );
        assert!(
            blocks
                .iter()
                .flat_map(|b| &b.spans)
                .all(|s| s.link.is_none())
        );
        assert!(
            blocks
                .iter()
                .any(|b| b.images.iter().any(|(_, alt)| alt == "Example"))
        );
    }
    #[::core::prelude::v1::test]
    fn bare_links_and_sandbox_asset_requests() {
        let blocks = parse("Visit https://tezbar.app. And ![icon](icons/a.png)");
        assert!(
            blocks
                .iter()
                .flat_map(|b| &b.spans)
                .any(|s| s.link.as_deref() == Some("https://tezbar.app/"))
        );
        assert_eq!(safe_image("../secret"), None);
        assert_eq!(safe_image("/etc/passwd"), None);
        assert_eq!(safe_image("icons/a.png"), Some("asset:icons/a.png".into()));
    }
    #[::core::prelude::v1::test]
    fn details_survive_without_executing_html() {
        let blocks = parse("<details><summary>More</summary><p>Body</p></details>");
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b.kind, Kind::Details) && b.text().contains("Body"))
        );
    }

    #[gpui::test]
    fn document_renders_native_roles_and_selection_copy_is_an_effect(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|_, _| MarkdownView::default());
        let events = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        let sink = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &DocumentEffect, _| {
                sink.lock().unwrap().push(event.clone())
            })
            .detach()
        });
        view.update(cx, |view, cx| {
            view.set_content(
                1,
                "# Heading\n\n**Bold** [link](https://tezbar.app)\n\n```rust\nlet a=1;\n```",
                true,
                cx,
            );
        });
        cx.run_until_parked();
        view.update(cx, |view, cx| {
            view.selection = Some(0..7);
            cx.emit(DocumentEffect::Copy(view.document.selected_text(0..7)));
        });
        assert!(
            events
                .lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e,DocumentEffect::Copy(text)if text=="Heading"))
        );
    }
}

#[cfg(test)]
mod repair_tests {
    use super::*;
    use ::core::prelude::v1::test;
    #[::core::prelude::v1::test]
    fn list_identity_keeps_code_paragraphs_and_nested_items_together() {
        let source = "1. Install:\n\n   ```sh\n   echo ready\n   ```\n\n   Continue here.\n\n   - nested one\n   - nested two\n\n2. Next\n3. Last";
        let blocks = parse(source);
        assert_eq!(block_groups(&blocks), [0..blocks.len()]);
        let first = blocks[0].list_path.clone();
        let code = blocks
            .iter()
            .find(|block| matches!(block.kind, Kind::Code(_)))
            .unwrap();
        assert_eq!(code.list_path, first);
        let continuation = blocks
            .iter()
            .find(|block| block.text().contains("Continue here"))
            .unwrap();
        assert_eq!(continuation.list_path, first);
        let nested = blocks
            .iter()
            .filter(|block| block.text().contains("nested"))
            .collect::<Vec<_>>();
        assert_eq!(nested.len(), 2);
        for block in &nested {
            assert_eq!(block.list_path.len(), 2);
            assert_eq!(block.list_path[0], first[0]);
        }
        assert_eq!(nested[0].list_path[1].list, nested[1].list_path[1].list);
        assert_ne!(nested[0].list_path[1].item, nested[1].list_path[1].item);
        let items = blocks
            .iter()
            .map(|block| block.list_path[0].item)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(items.len(), 3);
    }
    #[::core::prelude::v1::test]
    fn adjacent_distinct_lists_and_incremental_chunks_never_merge() {
        let blocks = parse("- one\n- two\n\n* three\n* four");
        assert_eq!(block_groups(&blocks), [0..2, 2..4]);
        assert_ne!(blocks[0].list_path[0].list, blocks[2].list_path[0].list);
        let mut document = Document::default();
        document.update(1, "- one\n\n", false);
        document.update(2, "- one\n\n* two", false);
        assert_eq!(block_groups(&document.blocks), [0..1, 1..2]);
        assert_ne!(
            document.blocks[0].list_path[0].list,
            document.blocks[1].list_path[0].list
        );
    }
    #[::core::prelude::v1::test]
    fn html_list_blocks_keep_their_item_and_ignore_inter_item_whitespace() {
        let blocks = parse(
            "<ol>\n<li><p>First</p><pre>code</pre><p>Continue</p><ul><li>Nested</li></ul></li>\n<li>Second</li>\n</ol>\n\n<ul><li>Separate</li></ul>",
        );
        let groups = block_groups(&blocks)
            .into_iter()
            .filter(|group| !blocks[group.start].list_path.is_empty())
            .collect::<Vec<_>>();
        assert_eq!(groups.len(), 2, "{blocks:?}");
        let first = blocks[groups[0].start].list_path[0];
        for text in ["First", "code", "Continue", "Nested"] {
            let block = blocks
                .iter()
                .find(|block| block.text().contains(text))
                .unwrap();
            assert_eq!(block.list_path[0], first);
        }
        let nested = blocks
            .iter()
            .find(|block| block.text().contains("Nested"))
            .unwrap();
        assert_eq!(nested.list_path.len(), 2);
        let second = blocks
            .iter()
            .find(|block| block.text().contains("Second"))
            .unwrap();
        assert_eq!(second.list_path[0].list, first.list);
        assert_ne!(second.list_path[0].item, first.item);
        assert_ne!(blocks[groups[1].start].list_path[0].list, first.list);
    }
    #[gpui::test]
    fn structured_list_render_consumes_every_continuation_once(cx: &mut gpui::TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| MarkdownView::default());
        cx.update(|_, _| {
            let blocks = parse("1. First\n\n   ```rs\n   let a = 1;\n   ```\n\n   More\n\n   - Nested\n\n2. Last\n\nOutside");
            let mut rows = (0..blocks.len()).map(|index| Some(div().id(index))).collect::<Vec<_>>();
            let rendered = render_block_groups(&blocks, &mut rows, 0..blocks.len(), 0);
            assert_eq!(rendered.len(), 2);
            assert_eq!(rendered[0].a11y_role(), Some(Role::List));
            assert!(rows.iter().all(Option::is_none));
        });
    }
    #[::core::prelude::v1::test]
    fn consecutive_list_items_share_a_container_and_footnote_scroll_row() {
        let blocks = parse("- first\n- second\n- third\n\nParagraph[^one]\n\n[^one]: Note");
        let groups = block_groups(&blocks);
        assert_eq!(groups[0], 0..3);
        assert!(
            blocks[..3]
                .iter()
                .all(|block| matches!(block.kind, Kind::List))
        );
        let footnote = blocks
            .iter()
            .position(|block| matches!(block.kind, Kind::Footnote(_)))
            .unwrap();
        let row = groups
            .iter()
            .position(|group| group.contains(&footnote))
            .unwrap();
        assert_eq!(row, footnote - 2);
        assert!(groups.iter().skip(1).all(|group| group.len() == 1));
    }
    #[::core::prelude::v1::test]
    fn settled_stream_matches_full_parse_for_global_markdown_constructs() {
        for source in [
            "[docs][1]\n\n[1]: https://example.com",
            "word[^one]\n\n[^one]: footnote",
            "- first\n\n  continuation\n\n- second",
            "<details><summary>Summary</summary>\n\nFirst\n\nSecond\n\n</details>",
        ] {
            let mut stream = Document::default();
            for (n, (end, ch)) in source.char_indices().enumerate() {
                stream.update(n as u64, &source[..end + ch.len_utf8()], false);
            }
            let before = stream.parses;
            stream.update(source.len() as u64, source, true);
            let full = parse(source);
            assert_eq!(
                stream
                    .blocks
                    .iter()
                    .map(|b| (&b.kind, &b.spans))
                    .collect::<Vec<_>>(),
                full.iter().map(|b| (&b.kind, &b.spans)).collect::<Vec<_>>(),
                "{source}"
            );
            assert_eq!(stream.parses, before + 1);
            stream.update(source.len() as u64, source, true);
            assert_eq!(stream.parses, before + 1);
        }
    }
    #[::core::prelude::v1::test]
    fn all_details_paragraphs_collapse_and_footnotes_have_internal_anchors() {
        let blocks = parse(
            "<details><summary>Summary</summary>\n\nFirst\n\nSecond\n\n</details>\n\nOutside\n\nword[^one]\n\n[^one]: Footnote",
        );
        let details = blocks
            .iter()
            .find(|b| matches!(b.kind, Kind::Details))
            .unwrap();
        assert!(details.text().contains("First") && details.text().contains("Second"));
        assert!(
            !blocks
                .iter()
                .filter(|b| !matches!(b.kind, Kind::Details))
                .any(|b| b.text().contains("Second"))
        );
        assert!(
            blocks
                .iter()
                .any(|b| matches!(&b.kind,Kind::Footnote(id) if id == "one"))
        );
        assert!(
            blocks
                .iter()
                .flat_map(|b| &b.spans)
                .any(|s| s.link.as_deref() == Some("#footnote:one"))
        );
    }
    #[::core::prelude::v1::test]
    fn syntax_palette_depends_on_appearance() {
        let dark = syntax_spans("rust".into(), "let value = 42;".into(), false);
        let light = syntax_spans("rust".into(), "let value = 42;".into(), true);
        assert!(!dark.is_empty());
        assert_ne!(dark, light);
    }
}

#[cfg(test)]
mod image_tests {
    use super::*;
    use ::core::prelude::v1::test;
    #[gpui::test]
    fn images_are_update_owned_rendered_weakly_and_stale_results_are_rejected(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|_, _| MarkdownView::default());
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&view, move |_, effect: &DocumentEffect, _| {
                if let DocumentEffect::Image(ticket) = effect {
                    sink.lock().unwrap().push(ticket.clone());
                }
            })
            .detach()
        });
        view.update(cx, |v, cx| {
            v.set_content(1, "![Preview](https://example.test/p.png)", true, cx)
        });
        cx.run_until_parked();
        let first = view.read_with(cx, |v, _| v.image_requests().pop().unwrap());
        assert_eq!(events.lock().unwrap().len(), 1);
        for _ in 0..3 {
            view.update(cx, |_, cx| cx.notify());
            cx.run_until_parked();
        }
        assert_eq!(
            events.lock().unwrap().len(),
            1,
            "render starts no image effect"
        );
        let image = std::sync::Arc::new(RenderImage::new(vec![image::Frame::new(
            image::RgbaImage::from_raw(1, 1, vec![255, 0, 0, 255]).unwrap(),
        )]));
        view.update(cx, |v, cx| {
            assert!(v.set_image(&first, Some(image.clone()), cx))
        });
        assert_eq!(
            std::sync::Arc::strong_count(&image),
            1,
            "document cannot retain cache allocations"
        );
        cx.run_until_parked();
        view.update(cx, |v, cx| {
            v.set_content(2, "removed", true, cx);
            v.set_content(3, "![Preview](https://example.test/p.png)", true, cx);
        });
        let second = view.read_with(cx, |v, _| v.image_requests().pop().unwrap());
        assert_ne!(first, second);
        view.update(cx, |v, cx| {
            assert!(!v.set_image(&first, Some(image.clone()), cx));
            assert!(v.set_image(&second, None, cx));
            assert!(v.image_requests().is_empty());
        });
        view.update(cx, |v, cx| v.reset_images(cx));
        let third = view.read_with(cx, |v, _| v.image_requests().pop().unwrap());
        assert_ne!(second, third);
    }
}
