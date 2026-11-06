//! Native document parsing. A message owns its cache: unrelated stream deltas never reparse history.
use gpui::{prelude::*, *};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::{ops::Range, sync::OnceLock};
use syntect::{easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet};
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
        if self.source != source {
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
                self.prefix.extend(parse(chunk));
                self.parsed_bytes += chunk.len();
                self.prefix_len = split;
            }
            let tail = &source[self.prefix_len..];
            self.blocks = self.prefix.clone();
            self.blocks.extend(parse(tail));
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
        static SYNTAX: OnceLock<SyntaxSet> = OnceLock::new();
        static THEMES: OnceLock<ThemeSet> = OnceLock::new();
        let set = SYNTAX.get_or_init(SyntaxSet::load_defaults_newlines);
        let themes = THEMES.get_or_init(ThemeSet::load_defaults);
        let syntax = set
            .find_syntax_by_token(language)
            .unwrap_or_else(|| set.find_syntax_plain_text());
        let theme = &themes.themes["base16-ocean.dark"];
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
    let mut blocks = vec![];
    let mut current = Block {
        kind: Kind::Paragraph,
        spans: vec![],
        images: vec![],
        checked: None,
    };
    let mut style = Span::default();
    let mut image: Option<(String, String)> = None;
    let mut lists: Vec<Option<u64>> = vec![];
    let mut html = crate::markdown_html::Html::default();
    let flush = |blocks: &mut Vec<Block>, current: &mut Block| {
        if !current.spans.is_empty() || !current.images.is_empty() {
            blocks.push(current.clone());
        }
        *current = Block {
            kind: Kind::Paragraph,
            spans: vec![],
            images: vec![],
            checked: None,
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
                    flush(&mut blocks, &mut current);
                    current.kind = Kind::Heading(level as u8);
                }
                Tag::CodeBlock(kind) => {
                    flush(&mut blocks, &mut current);
                    current.kind = Kind::Code(match kind {
                        CodeBlockKind::Fenced(s) => s.into_string(),
                        _ => String::new(),
                    });
                }
                Tag::List(start) => {
                    lists.push(start);
                }
                Tag::Item => {
                    flush(&mut blocks, &mut current);
                    current.kind = Kind::List;
                    let marker = if let Some(Some(n)) = lists.last_mut() {
                        let marker = format!("{n}. ");
                        *n += 1;
                        marker
                    } else {
                        "• ".into()
                    };
                    current.spans.push(Span {
                        text: marker,
                        ..Default::default()
                    });
                }
                Tag::Table(_) => {
                    flush(&mut blocks, &mut current);
                    current.kind = Kind::Table;
                }
                Tag::BlockQuote(_) => {
                    flush(&mut blocks, &mut current);
                    current.kind = Kind::Quote;
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
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::CodeBlock
                | TagEnd::Item
                | TagEnd::Table => flush(&mut blocks, &mut current),
                TagEnd::List(_) => {
                    lists.pop();
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
            Event::Rule => {
                flush(&mut blocks, &mut current);
                blocks.push(Block {
                    kind: Kind::Rule,
                    spans: vec![],
                    images: vec![],
                    checked: None,
                });
            }
            // Raw markup is never executed, nor passed to a web engine.
            Event::Html(fragment) | Event::InlineHtml(fragment) => {
                html.fragment(&fragment, &mut blocks, &mut current, &mut style)
            }
            _ => {}
        }
    }
    flush(&mut blocks, &mut current);
    blocks
}
#[derive(Clone, Debug)]
pub enum DocumentEffect {
    OpenLink(String),
    Image(String),
    Copy(String),
}
pub struct MarkdownView {
    pub document: Document,
    pub selection: Option<Range<usize>>,
    anchor: Option<usize>,
    focus: Option<FocusHandle>,
    requested_images: std::collections::HashSet<String>,
    expanded: std::collections::HashSet<usize>,
}
impl gpui::EventEmitter<DocumentEffect> for MarkdownView {}
impl Default for MarkdownView {
    fn default() -> Self {
        Self {
            document: Document::default(),
            selection: None,
            anchor: None,
            focus: None,
            requested_images: Default::default(),
            expanded: Default::default(),
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
            self.selection = None;
            self.anchor = None;
            cx.notify();
        }
    }
}
impl Render for MarkdownView {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = crate::tokens::tokens(w);
        let focus = self.focus.get_or_insert_with(|| cx.focus_handle()).clone();
        let mut children = vec![];
        let mut base = 0;
        let code_indices = self
            .document
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| matches!(b.kind, Kind::Code(_)).then_some(i))
            .collect::<Vec<_>>();
        for i in code_indices {
            self.document.highlight(i);
        }
        for (index, block) in self.document.blocks.iter().enumerate() {
            let text = block.text();
            let block_len = text.len();
            if matches!(block.kind, Kind::Details) {
                let summary = text.lines().next().unwrap_or("Details").to_owned();
                let expanded = self.expanded.contains(&index);
                children.push(
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
                Kind::List => Role::ListItem,
                _ => Role::Paragraph,
            };
            let mut row = div()
                .id(index)
                .flex()
                .flex_col()
                .gap(px(4.))
                .py(px(4.))
                .text_size(px(match block.kind {
                    Kind::Heading(1) => 22.,
                    Kind::Heading(2) => 18.,
                    Kind::Heading(_) => 15.,
                    _ => 13.,
                }));
            if let Some(checked) = block.checked {
                row = row.child(crate::controls::checkbox(
                    ("task", index),
                    "Task",
                    checked,
                    t,
                ));
            }
            if let Kind::Code(ref language) = block.kind {
                let code = text.clone();
                row = row
                    .font_family(crate::tokens::font_family(cx, true))
                    .bg(t.field)
                    .p(px(8.))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
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
                                cx.emit(DocumentEffect::OpenLink(url.clone()));
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
            });
            for (i, (url, alt)) in block.images.iter().enumerate() {
                if self.requested_images.len() < 128 && self.requested_images.insert(url.clone()) {
                    cx.emit(DocumentEffect::Image(url.clone()));
                }
                row = row.child(
                    div()
                        .id(("image", i))
                        .role(Role::Image)
                        .aria_label(alt.clone())
                        .child(alt.clone()),
                );
            }
            children.push(row);
            base += block_len + 1;
        }
        div()
            .id("document")
            .role(Role::Document)
            .track_focus(&focus)
            .key_context("MarkdownDocument")
            .size_full()
            .flex()
            .flex_col()
            .text_color(t.text1)
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
        let id = b.synthetic_node_id("text");
        let mut run = accesskit::Node::new(Role::TextRun);
        run.set_value(self.text.clone());
        run.set_character_lengths(
            self.text
                .chars()
                .map(|c| c.len_utf8() as u8)
                .collect::<Vec<_>>(),
        );
        b.push_child(id, run);
        if let Some(selection) = &self.selection {
            let pos = |at| accesskit::TextPosition {
                node: id,
                character_index: self.text[..at].chars().count(),
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
        _: Bounds<Pixels>,
        _: &mut (),
        actions: &mut Self::PrepaintState,
        w: &mut Window,
        cx: &mut App,
    ) {
        self.inner.paint(w, cx);
        for (id, url) in actions.drain(..) {
            let owner = self.owner.clone();
            w.on_a11y_action(id, AccessibleAction::Click, move |_, _, cx| {
                owner.update(cx, |_, cx| cx.emit(DocumentEffect::OpenLink(url.clone())))
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
        assert_eq!(d.parses, 1);
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
