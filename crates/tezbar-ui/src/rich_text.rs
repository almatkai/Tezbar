//! Notes' native editing projection. TextInput owns OS text/IME/selection, Document owns rich data.
use crate::{
    controls::{self, ButtonTone},
    text_input::{ByteSelection, ExternalEdit, FieldOptions, StyledRange, TextInput},
    tokens,
};
use gpui::{prelude::*, *};
use tezbar_ui_model::notes::{Document, EditorSelection, Node, Style};
#[derive(Clone, Debug)]
pub struct SelectionChange {
    pub before: EditorSelection,
    pub after: EditorSelection,
}
fn model_selection(s: ByteSelection) -> EditorSelection {
    EditorSelection {
        start: s.range.start,
        end: s.range.end,
        reversed: s.reversed,
    }
}
#[derive(Clone, Debug)]
pub enum Edit {
    Replace {
        owner: Option<u64>,
        start: usize,
        end: usize,
        text: String,
        styles: Vec<(Style, bool)>,
        selection: SelectionChange,
    },
    Format {
        owner: Option<u64>,
        start: usize,
        end: usize,
        style: Style,
        selection: SelectionChange,
    },
    Enter {
        owner: Option<u64>,
        position: usize,
        end: usize,
        selection: SelectionChange,
    },
    List {
        owner: Option<u64>,
        ordered: bool,
        selection: SelectionChange,
    },
    Undo {
        owner: Option<u64>,
        redo: bool,
    },
}
impl Edit {
    pub fn owner(&self) -> Option<u64> {
        match self {
            Self::Replace { owner, .. }
            | Self::Format { owner, .. }
            | Self::Enter { owner, .. }
            | Self::List { owner, .. }
            | Self::Undo { owner, .. } => *owner,
        }
    }
}
pub enum Clipboard {
    Read { owner: Option<u64> },
    Write(String),
}
impl EventEmitter<Edit> for RichText {}
impl EventEmitter<Clipboard> for RichText {}
#[derive(Default)]
struct Projection {
    text: String,
    styles: Vec<StyledRange>,
    points: Vec<(usize, usize)>,
    boundaries: Vec<(usize, Vec<usize>, Vec<usize>)>,
    blocks: Vec<(usize, usize, Vec<usize>)>,
}
fn block(tag: &str) -> bool {
    matches!(tag, "div" | "p" | "li")
}
fn has_block(nodes: &[Node]) -> bool {
    nodes
        .iter()
        .any(|n| matches!(n,Node::Element{tag,children} if block(tag)||has_block(children)))
}
impl Projection {
    fn from_document(doc: &Document) -> Self {
        fn begin(
            owner: &[usize],
            p: &mut Projection,
            last: &mut Option<Vec<usize>>,
            model: usize,
            visual: &mut usize,
        ) {
            if last.as_deref() != Some(owner) {
                if let Some(left) = last.take() {
                    p.boundaries.push((*visual, left, owner.to_vec()));
                    p.points.push((p.text.len(), model));
                    p.text.push('\n');
                    *visual += 1;
                }
                *last = Some(owner.to_vec());
            }
        }
        fn visit(
            nodes: &[Node],
            path: &mut Vec<usize>,
            p: &mut Projection,
            style: (bool, bool, bool),
            owner: Option<&[usize]>,
            model: &mut usize,
            last: &mut Option<Vec<usize>>,
            visual: &mut usize,
        ) {
            for (index, n) in nodes.iter().enumerate() {
                path.push(index);
                match n {
                    Node::Text(text) => {
                        if !text.is_empty() {
                            begin(owner.unwrap_or(&[]), p, last, *model, visual);
                        }
                        let start = p.text.len();
                        for c in text.chars() {
                            p.points.push((p.text.len(), *model));
                            p.text.push(c);
                            *model += 1;
                            *visual += 1;
                        }
                        if style.0 || style.1 || style.2 {
                            p.styles.push(StyledRange {
                                range: start..p.text.len(),
                                bold: style.0,
                                italic: style.1,
                                underline: style.2,
                            });
                        }
                    }
                    Node::Element { tag, children } => {
                        let is_block = block(tag);
                        let path_copy = path.clone();
                        let owner = if is_block {
                            Some(path_copy.as_slice())
                        } else {
                            owner
                        };
                        let leaf = is_block && !has_block(children);
                        if leaf {
                            begin(path, p, last, *model, visual);
                        }
                        let start = *visual;
                        if tag == "br" {
                            begin(owner.unwrap_or(&[]), p, last, *model, visual);
                            p.points.push((p.text.len(), *model));
                            p.text.push('\n');
                            *model += 1;
                            *visual += 1;
                        } else {
                            visit(
                                children,
                                path,
                                p,
                                (
                                    style.0 || matches!(tag.as_str(), "strong" | "b"),
                                    style.1 || matches!(tag.as_str(), "em" | "i"),
                                    style.2 || tag == "u",
                                ),
                                owner,
                                model,
                                last,
                                visual,
                            );
                        }
                        if leaf {
                            p.blocks.push((start, *visual, path.clone()));
                        }
                    }
                }
                path.pop();
            }
        }
        let mut p = Self::default();
        visit(
            &doc.nodes,
            &mut vec![],
            &mut p,
            (false, false, false),
            None,
            &mut 0,
            &mut None,
            &mut 0,
        );
        p.points.push((p.text.len(), doc.text_len()));
        p
    }
    fn position(&self, byte: usize) -> usize {
        self.points
            .iter()
            .rev()
            .find(|(offset, _)| *offset <= byte)
            .map(|(_, p)| *p)
            .unwrap_or(0)
    }
    fn model_at(&self, visual: usize) -> usize {
        let byte = self
            .text
            .char_indices()
            .nth(visual)
            .map(|(b, _)| b)
            .unwrap_or(self.text.len());
        self.position(byte)
    }
}
fn node<'a>(nodes: &'a [Node], path: &[usize]) -> Option<&'a Node> {
    let (&index, rest) = path.split_first()?;
    let n = nodes.get(index)?;
    if rest.is_empty() {
        Some(n)
    } else if let Node::Element { children, .. } = n {
        node(children, rest)
    } else {
        None
    }
}
fn node_mut<'a>(nodes: &'a mut [Node], path: &[usize]) -> Option<&'a mut Node> {
    let (&index, rest) = path.split_first()?;
    let n = nodes.get_mut(index)?;
    if rest.is_empty() {
        Some(n)
    } else if let Node::Element { children, .. } = n {
        node_mut(children, rest)
    } else {
        None
    }
}
fn remove(nodes: &mut Vec<Node>, path: &[usize]) {
    let Some((&index, rest)) = path.split_first() else {
        return;
    };
    if index >= nodes.len() {
        return;
    }
    if rest.is_empty() {
        nodes.remove(index);
        return;
    }
    if let Node::Element { tag, children } = &mut nodes[index] {
        remove(children, rest);
        if children.is_empty() && matches!(tag.as_str(), "ul" | "ol") {
            nodes.remove(index);
        }
    }
}
fn split_newlines(nodes: &mut Vec<Node>) {
    let mut out = Vec::with_capacity(nodes.len());
    for n in std::mem::take(nodes) {
        match n {
            Node::Text(text) if text.contains('\n') => {
                for (i, line) in text.split('\n').enumerate() {
                    if i > 0 {
                        out.push(Node::Element {
                            tag: "br".into(),
                            children: vec![],
                        });
                    }
                    if !line.is_empty() {
                        out.push(Node::Text(line.into()));
                    }
                }
            }
            Node::Element { tag, mut children } => {
                split_newlines(&mut children);
                out.push(Node::Element { tag, children });
            }
            n => out.push(n),
        }
    }
    *nodes = out;
}
fn children_at<'a>(nodes: &'a mut Vec<Node>, path: &[usize]) -> Option<&'a mut Vec<Node>> {
    if path.is_empty() {
        Some(nodes)
    } else {
        match node_mut(nodes, path) {
            Some(Node::Element { children, .. }) => Some(children),
            _ => None,
        }
    }
}
fn join_ancestor(
    nodes: &mut Vec<Node>,
    ancestor: &[usize],
    descendant: &[usize],
    ancestor_is_left: bool,
) -> Result<(), String> {
    let branch = *descendant
        .get(ancestor.len())
        .ok_or("Missing paragraph boundary")?;
    if ancestor_is_left {
        let moved = match node(nodes, descendant) {
            Some(Node::Element { children, .. }) => children.clone(),
            _ => return Err("Missing paragraph".into()),
        };
        remove(nodes, descendant);
        let parent = children_at(nodes, ancestor).ok_or("Missing paragraph parent")?;
        let at = branch.min(parent.len());
        parent.splice(at..at, moved);
    } else {
        let parent = children_at(nodes, ancestor).ok_or("Missing paragraph parent")?;
        let start = (branch + 1).min(parent.len());
        let end=parent[start..].iter().position(|n|matches!(n,Node::Element{tag,children}if block(tag)||matches!(tag.as_str(),"ul"|"ol")||has_block(children))).map(|n|start+n).unwrap_or(parent.len());
        let moved = parent.drain(start..end).collect::<Vec<_>>();
        match node_mut(nodes, descendant) {
            Some(Node::Element { children, .. }) => children.extend(moved),
            _ => return Err("Missing paragraph".into()),
        }
    }
    Ok(())
}
/// Structural edits run inside Notes::edit. This helper owns no draft, serializer or undo stack.
pub fn replace_visual(
    doc: &mut Document,
    start: usize,
    end: usize,
    text: &str,
) -> Result<(), String> {
    let projection = Projection::from_document(doc);
    if start > end || end > projection.text.chars().count() {
        return Err("Selection outside document".into());
    }
    doc.replace(projection.model_at(start), projection.model_at(end), text)?;
    for (_, left, right) in projection
        .boundaries
        .iter()
        .rev()
        .filter(|(at, _, _)| *at >= start && *at < end)
    {
        if right.starts_with(left) {
            join_ancestor(&mut doc.nodes, left, right, true)?;
            continue;
        }
        if left.starts_with(right) {
            join_ancestor(&mut doc.nodes, right, left, false)?;
            continue;
        }
        let children = match node(&doc.nodes, right) {
            Some(Node::Element { children, .. }) => children.clone(),
            _ => continue,
        };
        if let Some(Node::Element {
            children: left_children,
            ..
        }) = node_mut(&mut doc.nodes, left)
        {
            left_children.extend(children);
            remove(&mut doc.nodes, right);
        }
    }
    split_newlines(&mut doc.nodes);
    if doc.html().len() > 1_048_576 {
        return Err("Note exceeds 1 MiB".into());
    }
    Ok(())
}
pub fn enter_visual(doc: &mut Document, start: usize, end: usize) -> Result<(), String> {
    replace_visual(doc, start, end, "")?;
    let p = Projection::from_document(doc);
    if let Some((_, _, path)) = p.blocks.iter().find(|(a, b, _)| start >= *a && start <= *b) {
        if let Some(Node::Element { tag, children }) = node(&doc.nodes, path) {
            if tag == "li"
                && children.iter().all(|n| {
                    matches!(n,Node::Element{tag,..} if tag=="br")
                        || matches!(n,Node::Text(s) if s.is_empty())
                })
            {
                let (&item_index, list_path) = path.split_last().ok_or("Missing list item")?;
                let (&list_index, parent_path) = list_path.split_last().ok_or("Missing list")?;
                let siblings = if parent_path.is_empty() {
                    &mut doc.nodes
                } else {
                    match node_mut(&mut doc.nodes, parent_path) {
                        Some(Node::Element { children, .. }) => children,
                        _ => return Err("Missing list parent".into()),
                    }
                };
                let (tag, tail, empty) = match siblings.get_mut(list_index) {
                    Some(Node::Element { tag, children })
                        if matches!(tag.as_str(), "ul" | "ol") =>
                    {
                        let tail = children.split_off(item_index + 1);
                        children.pop();
                        (tag.clone(), tail, children.is_empty())
                    }
                    _ => return Err("Missing list".into()),
                };
                let at = if empty {
                    siblings.remove(list_index);
                    list_index
                } else {
                    list_index + 1
                };
                siblings.insert(
                    at,
                    Node::Element {
                        tag: "div".into(),
                        children: vec![Node::Element {
                            tag: "br".into(),
                            children: vec![],
                        }],
                    },
                );
                if !tail.is_empty() {
                    siblings.insert(
                        at + 1,
                        Node::Element {
                            tag,
                            children: tail,
                        },
                    );
                }
                return Ok(());
            }
        }
    }
    doc.enter(p.model_at(start))
}
pub struct RichText {
    pub input: Entity<TextInput>,
    owner: Option<u64>,
    document: Option<Document>,
    projection: Projection,
    window: AnyWindowHandle,
    _subscriptions: Vec<Subscription>,
    revision: u64,
    next_selection: Option<ByteSelection>,
    typing_styles: [Option<bool>; 3],
    typing_anchor: Option<usize>,
    input_generation: u64,
}
impl RichText {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            let mut input = TextInput::new(cx);
            input.configure(FieldOptions {
                name: "Note editor".into(),
                placeholder: "Write a note…".into(),
                multiline: true,
                border: false,
                ..Default::default()
            });
            input.delegate_edits(true);
            input
        });
        let subscriptions = Self::input_subscriptions(&input, 0, cx);
        Self {
            input,
            owner: None,
            document: None,
            projection: Default::default(),
            window: window.window_handle(),
            _subscriptions: subscriptions,
            revision: 0,
            next_selection: None,
            typing_styles: [None; 3],
            typing_anchor: None,
            input_generation: 0,
        }
    }
    fn input_subscriptions(
        input: &Entity<TextInput>,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> Vec<Subscription> {
        let sub = cx.subscribe(input, move |this, _, e: &ExternalEdit, cx| {
            if this.input_generation != generation {
                return;
            }
            let owner = this.owner;
            match e {
                ExternalEdit::Replace {
                    before,
                    after,
                    before_selection,
                    after_selection,
                } => {
                    let prefix = before
                        .chars()
                        .zip(after.chars())
                        .take_while(|(a, b)| a == b)
                        .map(|(c, _)| c.len_utf8())
                        .sum::<usize>();
                    let suffix = before[prefix..]
                        .chars()
                        .rev()
                        .zip(after[prefix..].chars().rev())
                        .take_while(|(a, b)| a == b)
                        .map(|(c, _)| c.len_utf8())
                        .sum::<usize>();
                    let start = before[..prefix].chars().count();
                    let end = before[..before.len() - suffix].chars().count();
                    if this.typing_anchor != Some(before_selection.range.start) {
                        this.typing_styles = [None; 3];
                    }
                    let styles = this
                        .typing_styles
                        .iter()
                        .enumerate()
                        .filter_map(|(i, value)| {
                            value.map(|value| {
                                ([Style::Bold, Style::Italic, Style::Underline][i], value)
                            })
                        })
                        .collect();
                    this.typing_anchor = Some(after_selection.range.end);
                    cx.emit(Edit::Replace {
                        owner,
                        styles,
                        start,
                        end,
                        text: after[prefix..after.len() - suffix].into(),
                        selection: SelectionChange {
                            before: model_selection(before_selection.clone()),
                            after: model_selection(after_selection.clone()),
                        },
                    });
                }
                ExternalEdit::Undo { redo } => cx.emit(Edit::Undo { owner, redo: *redo }),
                ExternalEdit::Newline => {
                    let selection = this.input.read(cx).selection_bytes();
                    let position = this.projection.text[..selection.range.start]
                        .chars()
                        .count();
                    let end = this.projection.text[..selection.range.end].chars().count();
                    this.next_selection = Some(ByteSelection {
                        range: selection.range.start + 1..selection.range.start + 1,
                        reversed: false,
                    });
                    cx.emit(Edit::Enter {
                        owner,
                        position,
                        end,
                        selection: SelectionChange {
                            before: model_selection(selection.clone()),
                            after: model_selection(this.next_selection.clone().unwrap()),
                        },
                    });
                }
            }
        });
        let clip = cx.subscribe(
            input,
            move |this, _, e: &crate::text_input::ClipboardEffect, cx| {
                if this.input_generation != generation {
                    return;
                }
                match e {
                    crate::text_input::ClipboardEffect::Read => {
                        cx.emit(Clipboard::Read { owner: this.owner })
                    }
                    crate::text_input::ClipboardEffect::Write(s) => {
                        cx.emit(Clipboard::Write(s.clone()))
                    }
                }
            },
        );
        vec![sub, clip]
    }
    fn fresh_input(cx: &mut Context<Self>) -> Entity<TextInput> {
        cx.new(|cx| {
            let mut input = TextInput::new(cx);
            input.configure(FieldOptions {
                name: "Note editor".into(),
                placeholder: "Write a note…".into(),
                multiline: true,
                border: false,
                ..Default::default()
            });
            input.delegate_edits(true);
            input
        })
    }
    pub fn restore_selection(&mut self, selection: EditorSelection) {
        self.next_selection = Some(ByteSelection {
            range: selection.start..selection.end,
            reversed: selection.reversed,
        });
    }
    pub fn force_refresh(&mut self) {
        self.document = None;
    }
    pub fn composing(&self, cx: &App) -> bool {
        self.input.read(cx).marked_range.is_some()
    }
    pub fn cancel_composition(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |i, cx| i.cancel_composition(w, cx));
    }
    pub fn set_document(&mut self, owner: Option<u64>, doc: Document, cx: &mut Context<Self>) {
        if self.owner == owner
            && self.document.as_ref() == Some(&doc)
            && self.next_selection.is_none()
        {
            return;
        }
        let changed_owner = self.owner != owner;
        if changed_owner {
            self.input_generation += 1;
            self.typing_styles = [None; 3];
            self.typing_anchor = None;
        }
        self.owner = owner;
        self.document = Some(doc.clone());
        self.projection = Projection::from_document(&doc);
        self.revision += 1;
        let revision = self.revision;
        let entity = cx.entity().downgrade();
        let window = self.window;
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, w, cx| {
                let _ = entity.update(cx, |this, cx| {
                    if this.revision != revision {
                        return;
                    }
                    if changed_owner {
                        let focused = this.input.focus_handle(cx).is_focused(w);
                        this._subscriptions.clear();
                        this.input = Self::fresh_input(cx);
                        this._subscriptions =
                            Self::input_subscriptions(&this.input, this.input_generation, cx);
                        if focused {
                            this.input.focus_handle(cx).focus(w, cx);
                        }
                    }
                    let selection = if let Some(selection) = this.next_selection.take() {
                        selection
                    } else if changed_owner {
                        ByteSelection {
                            range: 0..0,
                            reversed: false,
                        }
                    } else {
                        this.input.read(cx).selection_bytes()
                    };
                    this.input.update(cx, |input, cx| {
                        input.replace_silently(&this.projection.text, selection, w, cx);
                        input.set_styled_ranges(this.projection.styles.clone(), cx);
                    });
                    cx.notify();
                });
            });
        });
    }
    pub fn paste_text(
        &mut self,
        owner: Option<u64>,
        text: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if owner != self.owner {
            return;
        } // Plain native paste cannot introduce active markup; Document escapes it.
        self.input
            .update(cx, |input, cx| input.paste_text(text, w, cx));
    }
    pub fn format(&mut self, style: Style, w: &mut Window, cx: &mut Context<Self>) {
        if self.composing(cx) {
            return;
        }
        let selection = model_selection(self.input.read(cx).selection_bytes());
        let range = self.input.read(cx).selection_bytes().range;
        let start = self.projection.position(range.start);
        let end = self.projection.position(range.end);
        if start == end {
            let byte = range.start.saturating_sub(1);
            let inherited = self
                .projection
                .styles
                .iter()
                .find(|s| s.range.contains(&byte))
                .map(|s| (s.bold, s.italic, s.underline))
                .unwrap_or_default();
            let (index, active) = match style {
                Style::Bold => (0, inherited.0),
                Style::Italic => (1, inherited.1),
                Style::Underline => (2, inherited.2),
            };
            if self.typing_anchor != Some(range.start) {
                self.typing_styles = [None; 3];
            }
            self.typing_styles[index] = Some(!self.typing_styles[index].unwrap_or(active));
            self.typing_anchor = Some(range.start);
            cx.notify();
        }
        if start < end {
            cx.emit(Edit::Format {
                owner: self.owner,
                start,
                end,
                style,
                selection: SelectionChange {
                    before: selection,
                    after: selection,
                },
            });
        }
        w.focus(&self.input.focus_handle(cx), cx);
    }
    fn key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        if self.composing(cx) {
            cx.propagate();
            return;
        }
        if e.keystroke.modifiers.platform {
            let style = match e.keystroke.key.as_str() {
                "b" => Some(Style::Bold),
                "i" => Some(Style::Italic),
                "u" => Some(Style::Underline),
                _ => None,
            };
            if let Some(style) = style {
                self.format(style, w, cx);
                cx.stop_propagation();
                return;
            }
        }
        cx.propagate();
    }
}
impl Focusable for RichText {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}
impl Render for RichText {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::tokens(w);
        let mut bar = div().flex().gap(px(4.));
        for (label, style) in [
            ("Bold", Style::Bold),
            ("Italic", Style::Italic),
            ("Underline", Style::Underline),
        ] {
            bar = bar.child(
                controls::button(label, label, ButtonTone::Ghost, false, t)
                    .on_click(cx.listener(move |this, _, w, cx| this.format(style, w, cx))),
            );
        }
        for (label, ordered) in [("Bulleted list", false), ("Numbered list", true)] {
            bar = bar.child(
                controls::button(label, label, ButtonTone::Ghost, false, t).on_click(cx.listener(
                    move |this, _, _, cx| {
                        if !this.composing(cx) {
                            let selection = model_selection(this.input.read(cx).selection_bytes());
                            cx.emit(Edit::List {
                                owner: this.owner,
                                ordered,
                                selection: SelectionChange {
                                    before: selection,
                                    after: selection,
                                },
                            });
                        }
                    },
                )),
            );
        }
        div()
            .flex()
            .flex_col()
            .size_full()
            .on_key_down(cx.listener(Self::key))
            .child(bar)
            .child(
                div()
                    .id("rich-text-scroll")
                    .overflow_y_scroll()
                    .flex_1()
                    .child(self.input.clone()),
            )
    }
}
/// Inline typing attributes are applied through Document's production formatting operations.
pub fn replace_visual_styled(
    doc: &mut Document,
    start: usize,
    end: usize,
    text: &str,
    styles: &[(Style, bool)],
) -> Result<(), String> {
    fn at(
        nodes: &[Node],
        position: usize,
        offset: &mut usize,
        active: (bool, bool, bool),
    ) -> Option<(bool, bool, bool)> {
        for n in nodes {
            match n {
                Node::Text(s) => {
                    let size = s.chars().count();
                    if position >= *offset && position < *offset + size {
                        return Some(active);
                    }
                    *offset += size;
                }
                Node::Element { tag, children } => {
                    if tag == "br" {
                        *offset += 1;
                    } else if let Some(style) = at(
                        children,
                        position,
                        offset,
                        (
                            active.0 || matches!(tag.as_str(), "strong" | "b"),
                            active.1 || matches!(tag.as_str(), "em" | "i"),
                            active.2 || tag == "u",
                        ),
                    ) {
                        return Some(style);
                    }
                }
            }
        }
        None
    }
    let model_start = Projection::from_document(doc).model_at(start);
    replace_visual(doc, start, end, text)?;
    if !text.is_empty() {
        for (style, wanted) in styles {
            let active =
                at(&doc.nodes, model_start, &mut 0, (false, false, false)).unwrap_or_default();
            let active = match style {
                Style::Bold => active.0,
                Style::Italic => active.1,
                Style::Underline => active.2,
            };
            if active != *wanted {
                doc.toggle_style(model_start, model_start + text.chars().count(), *style)?;
            }
        }
    }
    if doc.html().len() > 1_048_576 {
        return Err("Note exceeds 1 MiB".into());
    }
    Ok(())
}
/// Lists change selected paragraph containers while retaining their inline nodes.
pub fn format_list_visual(
    doc: &mut Document,
    start_byte: usize,
    end_byte: usize,
    ordered: bool,
) -> Result<(), String> {
    let p = Projection::from_document(doc);
    let floor = |mut n: usize| {
        n = n.min(p.text.len());
        while !p.text.is_char_boundary(n) {
            n -= 1;
        }
        n
    };
    let start = p.text[..floor(start_byte)].chars().count();
    let end = p.text[..floor(end_byte)].chars().count();
    let selected = p
        .blocks
        .iter()
        .filter(|(a, b, _)| {
            if start == end {
                start >= *a && start <= *b
            } else {
                *b > start && *a < end
            }
        })
        .map(|(_, _, path)| path.clone())
        .collect::<Vec<_>>();
    let target = if ordered { "ol" } else { "ul" };
    let mut out = vec![];
    fn list(out: &mut Vec<Node>, tag: &str, n: Node) {
        if let Some(Node::Element {
            tag: last,
            children,
        }) = out.last_mut()
        {
            if last == tag {
                children.push(n);
                return;
            }
        }
        out.push(Node::Element {
            tag: tag.into(),
            children: vec![n],
        });
    }
    for (index, n) in std::mem::take(&mut doc.nodes).into_iter().enumerate() {
        let paths = selected
            .iter()
            .filter(|path| path.first() == Some(&index))
            .collect::<Vec<_>>();
        if paths.is_empty() && !p.blocks.is_empty() {
            out.push(n);
            continue;
        }
        match n {
            Node::Element { tag, children } if matches!(tag.as_str(), "ul" | "ol") => {
                for (item_index, item) in children.into_iter().enumerate() {
                    if paths.iter().any(|path| path.get(1) == Some(&item_index)) {
                        if tag == target {
                            match item {
                                Node::Element { tag, children } if tag == "li" => {
                                    out.push(Node::Element {
                                        tag: "div".into(),
                                        children,
                                    })
                                }
                                n => out.push(n),
                            }
                        } else {
                            list(&mut out, target, item);
                        }
                    } else {
                        list(&mut out, &tag, item);
                    }
                }
            }
            Node::Element { tag, children } if matches!(tag.as_str(), "div" | "p") => list(
                &mut out,
                target,
                Node::Element {
                    tag: "li".into(),
                    children,
                },
            ),
            n => list(
                &mut out,
                target,
                Node::Element {
                    tag: "li".into(),
                    children: vec![n],
                },
            ),
        }
    }
    doc.nodes = out;
    if doc.html().len() > 1_048_576 {
        return Err("Note exceeds 1 MiB".into());
    }
    Ok(())
}
