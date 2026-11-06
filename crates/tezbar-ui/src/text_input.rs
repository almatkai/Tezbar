// Adapted from Zed GPUI examples/input.rs at c84af5b99cbc366d675064e52b23664a015b6e35.
// Copyright 2022–2025 Zed Industries, Inc. Licensed Apache-2.0.
// Changes: single-line launcher, committed-text events, composition, word editing and theme.
use std::ops::Range;

use gpui::{
    App, Bounds, Context, CursorStyle, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    FocusHandle, Focusable, GlobalElementId, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine, SharedString, Style, TextRun,
    UTF16Selection, UnderlineStyle, Window, actions, div, fill, point, prelude::*, px, relative,
    size,
};
use unicode_segmentation::*;

actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
        Quit,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        DeleteWord,
        SelectHome,
        SelectEnd,
        Undo,
        Redo,
        Newline,
        Up,
        Down,
        SelectUp,
        SelectDown,
        DocumentHome,
        DocumentEnd,
        SelectDocumentHome,
        SelectDocumentEnd,
        DeleteLineStart,
        DeleteWordForward,
    ]
);

#[derive(Clone, Debug)]
pub struct FieldOptions {
    pub name: SharedString,
    pub placeholder: SharedString,
    pub multiline: bool,
    pub password: bool,
    pub error: Option<SharedString>,
    pub border: bool,
}
impl Default for FieldOptions {
    fn default() -> Self {
        Self {
            name: "Text".into(),
            placeholder: "".into(),
            multiline: false,
            password: false,
            error: None,
            border: true,
        }
    }
}
#[derive(Clone)]
struct Snapshot {
    text: SharedString,
    selection: Range<usize>,
    reversed: bool,
}
pub enum ClipboardEffect {
    Read,
    Write(String),
}
impl gpui::EventEmitter<ClipboardEffect> for TextInput {}
pub struct TextInput {
    pub options: FieldOptions,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    focus_handle: FocusHandle,
    pub content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    pub marked_range: Option<Range<usize>>,
    before_composition: Option<SharedString>,
    last_layout: Option<ShapedLine>,
    lines: Vec<(usize, ShapedLine, Bounds<Pixels>)>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    scroll_x: Pixels,
}

pub struct Committed(pub String);
impl gpui::EventEmitter<Committed> for TextInput {}
impl TextInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            options: FieldOptions::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: "Search apps, files and commands".into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            before_composition: None,
            last_layout: None,
            lines: Vec::new(),
            last_bounds: None,
            is_selecting: false,
            scroll_x: px(0.),
        }
    }
    pub fn clear_silently(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.content = "".into();
        self.selected_range = 0..0;
        self.selection_reversed = false;
        self.marked_range = None;
        self.before_composition = None;
        self.lines.clear();
        self.last_layout = None;
        self.last_bounds = None;
        self.is_selecting = false;
        self.scroll_x = px(0.);
        window.notify_text_input_changed(&self.focus_handle);
        cx.notify();
    }
    pub fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.selected_range = 0..self.content.len();
        self.replace_text_in_range(None, text, window, cx);
    }
    pub fn cancel_composition(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(before) = self.before_composition.take() {
            self.content = before;
            self.selected_range = self.content.len()..self.content.len();
        }
        self.lines.clear();
        self.marked_range = None;
        window.notify_text_input_changed(&self.focus_handle);
        cx.notify();
    }
    fn word_boundary(&self, right: bool) -> usize {
        let offset = self.cursor_offset();
        if right {
            self.content
                .unicode_word_indices()
                .map(|(i, w)| i + w.len())
                .find(|&i| i > offset)
                .unwrap_or(self.content.len())
        } else {
            self.content
                .unicode_word_indices()
                .map(|(i, _)| i)
                .rev()
                .find(|&i| i < offset)
                .unwrap_or(0)
        }
    }
    fn word_left(&mut self, _: &WordLeft, w: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.word_boundary(false), w, cx)
    }
    fn word_right(&mut self, _: &WordRight, w: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.word_boundary(true), w, cx)
    }
    fn select_word_left(&mut self, _: &SelectWordLeft, w: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.word_boundary(false), w, cx)
    }
    fn select_word_right(&mut self, _: &SelectWordRight, w: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.word_boundary(true), w, cx)
    }
    fn select_home(&mut self, _: &SelectHome, w: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.line_boundary(false), w, cx)
    }
    fn select_end(&mut self, _: &SelectEnd, w: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.line_boundary(true), w, cx)
    }
    fn delete_word(&mut self, _: &DeleteWord, w: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.word_boundary(false), w, cx);
        }
        self.replace_text_in_range(None, "", w, cx)
    }

    fn left(&mut self, _: &Left, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), window, cx);
        } else {
            self.move_to(self.selected_range.start, window, cx)
        }
    }

    fn right(&mut self, _: &Right, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), window, cx);
        } else {
            self.move_to(self.selected_range.end, window, cx)
        }
    }

    fn select_left(&mut self, _: &SelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), window, cx);
    }

    fn select_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), window, cx);
    }

    fn select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, window, cx);
        self.select_to(self.content.len(), window, cx)
    }

    // Fields currently paint unwrapped shaped lines. These are visual line boundaries;
    // document commands remain separate, including for multiline fields.
    fn line_boundary(&self, end: bool) -> usize {
        let cursor = self.cursor_offset();
        if end {
            self.content[cursor..]
                .find('\n')
                .map(|i| cursor + i)
                .unwrap_or(self.content.len())
        } else {
            self.content[..cursor]
                .rfind('\n')
                .map(|i| i + 1)
                .unwrap_or(0)
        }
    }
    fn home(&mut self, _: &Home, w: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.line_boundary(false), w, cx);
    }
    fn end(&mut self, _: &End, w: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.line_boundary(true), w, cx);
    }
    fn document_home(&mut self, _: &DocumentHome, w: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, w, cx);
    }
    fn document_end(&mut self, _: &DocumentEnd, w: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), w, cx);
    }
    fn select_document_home(
        &mut self,
        _: &SelectDocumentHome,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(0, w, cx);
    }
    fn select_document_end(
        &mut self,
        _: &SelectDocumentEnd,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(self.content.len(), w, cx);
    }
    fn delete_line_start(&mut self, _: &DeleteLineStart, w: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.line_boundary(false), w, cx);
        }
        self.replace_text_in_range(None, "", w, cx);
    }
    fn delete_word_forward(
        &mut self,
        _: &DeleteWordForward,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            self.select_to(self.word_boundary(true), w, cx);
        }
        self.replace_text_in_range(None, "", w, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let prev = self.previous_boundary(self.cursor_offset());
            if self.cursor_offset() == prev {
                window.play_system_bell();
                return;
            }
            self.select_to(prev, window, cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            if self.cursor_offset() == next {
                window.play_system_bell();
                return;
            }
            self.select_to(next, window, cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        if event.click_count == 2 {
            let at = self.index_for_mouse_position(event.position);
            if let Some((i, word)) = self
                .content
                .unicode_word_indices()
                .find(|(i, w)| *i <= at && at <= i + w.len())
            {
                self.selected_range = i..i + word.len();
                self.selection_reversed = false;
                window.notify_text_selection_changed(&self.focus_handle);
                cx.notify();
                return;
            }
        }
        window.focus(&self.focus_handle, cx);
        self.is_selecting = true;
        window.request_virtual_keyboard();

        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), window, cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), window, cx)
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting {
            if event.pressed_button != Some(MouseButton::Left) {
                self.is_selecting = false;
                return;
            }
            self.select_to(self.index_for_mouse_position(event.position), window, cx);
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(ClipboardEffect::Read);
    }
    pub fn paste_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_text_in_range(None, text, window, cx);
    }
    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.options.password && !self.selected_range.is_empty() {
            cx.emit(ClipboardEffect::Write(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }
    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.options.password && !self.selected_range.is_empty() {
            cx.emit(ClipboardEffect::Write(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.content.clone(),
            selection: self.selected_range.clone(),
            reversed: self.selection_reversed,
        }
    }
    fn checkpoint(&mut self) {
        let snapshot = self.snapshot();
        self.undo.push(snapshot);
        self.redo.clear();
        while self.undo.len() > 100
            || self.undo.iter().map(|x| x.text.len()).sum::<usize>() > 1024 * 1024
        {
            self.undo.remove(0);
        }
    }
    fn restore(&mut self, s: Snapshot, w: &mut Window, cx: &mut Context<Self>) {
        self.lines.clear();
        self.content = s.text;
        self.selected_range = s.selection;
        self.selection_reversed = s.reversed;
        self.marked_range = None;
        self.before_composition = None;
        w.notify_text_input_changed(&self.focus_handle);
        cx.emit(Committed(self.content.to_string()));
        cx.notify();
    }
    fn undo(&mut self, _: &Undo, w: &mut Window, cx: &mut Context<Self>) {
        if self.marked_range.is_some() {
            self.cancel_composition(w, cx);
            return;
        }
        if let Some(s) = self.undo.pop() {
            self.redo.push(self.snapshot());
            self.restore(s, w, cx);
        }
    }
    fn redo(&mut self, _: &Redo, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(self.snapshot());
            self.restore(s, w, cx);
        }
    }
    fn newline(&mut self, _: &Newline, w: &mut Window, cx: &mut Context<Self>) {
        if self.marked_range.is_some() {
            cx.propagate();
            return;
        }
        if self.options.multiline {
            self.replace_text_in_range(None, "\n", w, cx);
        } else {
            cx.propagate();
        }
    }
    pub fn configure(&mut self, options: FieldOptions) {
        self.placeholder = options.placeholder.clone();
        self.options = options;
    }
    fn vertical(&mut self, down: bool, extend: bool, w: &mut Window, cx: &mut Context<Self>) {
        if !self.options.multiline {
            cx.propagate();
            return;
        }
        let cursor = self.cursor_offset();
        let start = self.content[..cursor]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let column = self.content[start..cursor].graphemes(true).count();
        let target = if down {
            self.content[cursor..].find('\n').map(|i| cursor + i + 1)
        } else if start > 0 {
            Some(
                self.content[..start - 1]
                    .rfind('\n')
                    .map(|i| i + 1)
                    .unwrap_or(0),
            )
        } else {
            None
        };
        if let Some(start) = target {
            let line = self.content[start..].split('\n').next().unwrap_or("");
            let offset = line
                .grapheme_indices(true)
                .nth(column)
                .map(|(i, _)| i)
                .unwrap_or(line.len());
            if extend {
                self.select_to(start + offset, w, cx);
            } else {
                self.move_to(start + offset, w, cx);
            }
        }
    }
    fn up(&mut self, _: &Up, w: &mut Window, cx: &mut Context<Self>) {
        self.vertical(false, false, w, cx);
    }
    fn down(&mut self, _: &Down, w: &mut Window, cx: &mut Context<Self>) {
        self.vertical(true, false, w, cx);
    }
    fn select_up(&mut self, _: &SelectUp, w: &mut Window, cx: &mut Context<Self>) {
        self.vertical(false, true, w, cx);
    }
    fn select_down(&mut self, _: &SelectDown, w: &mut Window, cx: &mut Context<Self>) {
        self.vertical(true, true, w, cx);
    }
    fn move_to(&mut self, offset: usize, window: &Window, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify()
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let Some((start, line, bounds)) = self
            .lines
            .iter()
            .find(|(_, _, b)| position.y < b.bottom())
            .or_else(|| self.lines.last())
        else {
            return 0;
        };
        let at = line.closest_index_for_x(position.x - bounds.left() + self.scroll_x);
        if self.options.password {
            self.content[*start..]
                .grapheme_indices(true)
                .nth(line.text[..at].graphemes(true).count())
                .map(|(i, _)| start + i)
                .unwrap_or(self.content.len())
        } else {
            start + at
        }
    }
    fn select_to(&mut self, offset: usize, window: &Window, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        let start = self.offset_from_utf16(range_utf16.start);
        let end = self.offset_from_utf16(range_utf16.end);
        start.min(end)..start.max(end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(if self.options.password {
            "•".repeat(self.content[range].graphemes(true).count())
        } else {
            self.content[range].to_string()
        })
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_range = self.range_from_utf16(&range_utf16);
        self.selection_reversed = false;
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify();
    }

    fn text_length_utf16(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.content.encode_utf16().count())
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.marked_range = None;
        self.before_composition = None;
        cx.emit(Committed(self.content.to_string()));
        window.notify_text_input_changed(&self.focus_handle);
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let single_line = if self.options.multiline {
            new_text.replace("\r\n", "\n").replace('\r', "\n")
        } else {
            new_text.replace(['\r', '\n'], " ")
        };
        let new_text = single_line.as_str();
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        if self.before_composition.is_none() {
            self.checkpoint();
        }
        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.selection_reversed = false;
        self.lines.clear();
        self.marked_range.take();
        self.before_composition = None;
        cx.emit(Committed(self.content.to_string()));
        window.notify_text_input_changed(&self.focus_handle);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.before_composition.is_none() {
            self.checkpoint();
            self.before_composition = Some(self.content.clone());
        }
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        self.lines.clear();
        let insertion_start_utf16 = self.offset_to_utf16(range.start);
        let inserted_length_utf16 = new_text.encode_utf16().count();
        self.selected_range = new_selected_range_utf16
            .map(|selected| {
                self.range_from_utf16(
                    &(insertion_start_utf16 + selected.start.min(inserted_length_utf16)
                        ..insertion_start_utf16 + selected.end.min(inserted_length_utf16)),
                )
            })
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        self.selection_reversed = false;

        window.notify_text_input_changed(&self.focus_handle);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _bounds: Bounds<Pixels>,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let (start, line, bounds) = self
            .lines
            .iter()
            .rev()
            .find(|(i, _, _)| *i <= range.start)?;
        let local = range.start - start;
        let end = (range.end - start).min(line.text.len());
        let index = if self.options.password {
            self.content[*start..range.start].graphemes(true).count() * "•".len()
        } else {
            local
        };
        Some(Bounds::new(
            point(
                bounds.left() + line.x_for_index(index) - self.scroll_x,
                bounds.top(),
            ),
            size(
                (line.x_for_index(end) - line.x_for_index(index)).max(px(1.)),
                bounds.size.height,
            ),
        ))
    }
    fn selection_bounds_for_range(
        &mut self,
        r: Range<usize>,
        b: Bounds<Pixels>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Bounds<Pixels>> {
        self.bounds_for_range(r, b, w, cx).into_iter().collect()
    }
    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.offset_to_utf16(self.index_for_mouse_position(p)))
    }
}

struct TextElement {
    input: Entity<TextInput>,
    text: String,
    selection: Range<usize>,
    reversed: bool,
    password: bool,
    multiline: bool,
    name: SharedString,
}
struct PrepaintState {
    lines: Vec<(usize, ShapedLine, Bounds<Pixels>)>,
    quads: Vec<PaintQuad>,
}
impl IntoElement for TextElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;
    fn id(&self) -> Option<ElementId> {
        Some("text-content".into())
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn a11y_role(&self) -> Option<gpui::Role> {
        Some(if self.password {
            gpui::Role::PasswordInput
        } else if self.multiline {
            gpui::Role::MultilineTextInput
        } else {
            gpui::Role::TextInput
        })
    }
    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        node.set_label(self.name.to_string());
        node.set_value(if self.password {
            "•".repeat(self.text.graphemes(true).count())
        } else {
            self.text.clone()
        });
    }
    fn a11y_synthetic_children(
        &mut self,
        _: &mut Self::PrepaintState,
        b: &mut gpui::A11ySubtreeBuilder,
    ) {
        // Passwords disclose neither clear text nor text length/selection to accessibility.
        if self.password {
            return;
        }
        let mut run = accesskit::Node::new(gpui::Role::TextRun);
        run.set_value(self.text.clone());
        run.set_character_lengths(
            self.text
                .graphemes(true)
                .map(|c| c.len().min(255) as u8)
                .collect::<Vec<_>>(),
        );
        let id = b.synthetic_node_id(0u8);
        b.push_child(id, run);
        let pos = |at: usize| accesskit::TextPosition {
            node: id,
            character_index: self.text[..at].graphemes(true).count(),
        };
        let (anchor, focus) = if self.reversed {
            (self.selection.end, self.selection.start)
        } else {
            (self.selection.start, self.selection.end)
        };
        b.parent_node()
            .set_text_selection(accesskit::TextSelection {
                anchor: pos(anchor),
                focus: pos(focus),
            });
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        w: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let lines = if self.multiline {
            self.text.split('\n').count().max(3)
        } else {
            1
        };
        style.size.height = (w.line_height() * lines as f32).into();
        (w.request_layout(style, [], cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        w: &mut Window,
        cx: &mut App,
    ) -> PrepaintState {
        let input = self.input.read(cx);
        let style = w.text_style();
        let mut lines = vec![];
        let mut quads = vec![];
        let mut at = 0;
        let height = w.line_height();
        let mut scroll_x = input.scroll_x;
        let text = if input.content.is_empty() {
            input.placeholder.to_string()
        } else {
            input.content.to_string()
        };
        for (n, source) in text.split('\n').enumerate() {
            let display: SharedString = if input.options.password && !input.content.is_empty() {
                "•".repeat(source.graphemes(true).count()).into()
            } else {
                source.to_string().into()
            };
            let color = if input.content.is_empty() {
                crate::tokens::tokens(w).text4
            } else {
                style.color
            };
            let base = TextRun {
                len: display.len(),
                font: style.font(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let mut runs = vec![];
            if let Some(marked) = input
                .marked_range
                .as_ref()
                .filter(|_| !input.options.password)
            {
                let start = marked.start.saturating_sub(at).min(source.len());
                let end = marked.end.saturating_sub(at).min(source.len());
                for (len, underline) in [
                    (start, None),
                    (
                        end - start,
                        Some(UnderlineStyle {
                            color: Some(color),
                            thickness: px(1.),
                            wavy: false,
                        }),
                    ),
                    (source.len() - end, None),
                ] {
                    if len > 0 {
                        runs.push(TextRun {
                            len,
                            underline,
                            ..base.clone()
                        });
                    }
                }
            } else {
                runs.push(base);
            }
            let line = w.text_system().shape_line(
                display,
                style.font_size.to_pixels(w.rem_size()),
                &runs,
                None,
            );
            if !input.options.multiline {
                let caret = if input.options.password {
                    input.content[..input.cursor_offset()]
                        .graphemes(true)
                        .count()
                        * "•".len()
                } else {
                    input.cursor_offset()
                };
                let x = line.x_for_index(caret);
                if x < scroll_x {
                    scroll_x = x;
                }
                if x - scroll_x > bounds.size.width - px(2.) {
                    scroll_x = (x - bounds.size.width + px(2.)).max(px(0.));
                }
            }
            let b = Bounds::new(
                point(bounds.left(), bounds.top() + height * n as f32),
                size(bounds.size.width, height),
            );
            let map = |index: usize| {
                if input.options.password {
                    source[..index.min(source.len())].graphemes(true).count() * "•".len()
                } else {
                    index.min(source.len())
                }
            };
            let sel = &input.selected_range;
            if !sel.is_empty() && sel.start <= at + source.len() && sel.end >= at {
                let left = map(sel.start.saturating_sub(at));
                let right = map(sel.end.saturating_sub(at));
                quads.push(fill(
                    Bounds::from_corners(
                        point(b.left() + line.x_for_index(left) - scroll_x, b.top()),
                        point(b.left() + line.x_for_index(right) - scroll_x, b.bottom()),
                    ),
                    crate::tokens::tokens(w).pill_strong,
                ));
            } else if sel.is_empty()
                && input.cursor_offset() >= at
                && input.cursor_offset() <= at + source.len()
                && input.focus_handle.is_focused(w)
            {
                let index = map(input.cursor_offset() - at);
                quads.push(fill(
                    Bounds::new(
                        point(b.left() + line.x_for_index(index) - scroll_x, b.top()),
                        size(px(1.), height),
                    ),
                    color,
                ));
            }
            lines.push((at, line, b));
            at += source.len() + 1;
        }
        self.input.update(cx, |input, _| input.scroll_x = scroll_x);
        PrepaintState { lines, quads }
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        p: &mut PrepaintState,
        w: &mut Window,
        cx: &mut App,
    ) {
        if self.input.read(cx).is_selecting {
            let input = self.input.clone();
            w.on_mouse_event(move |event: &MouseMoveEvent, phase, w, cx| {
                if phase == gpui::DispatchPhase::Capture {
                    input.update(cx, |input, cx| input.on_mouse_move(event, w, cx));
                }
            });
            let input = self.input.clone();
            w.on_mouse_event(move |event: &MouseUpEvent, phase, w, cx| {
                if phase == gpui::DispatchPhase::Capture {
                    input.update(cx, |input, cx| input.on_mouse_up(event, w, cx));
                }
            });
        }
        let focus = self.input.read(cx).focus_handle.clone();
        w.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        for quad in p.quads.drain(..) {
            w.paint_quad(quad);
        }
        let scroll = self.input.read(cx).scroll_x;
        for (_, line, b) in &p.lines {
            let _ = line.paint(
                point(b.left() - scroll, b.top()),
                w.line_height(),
                gpui::TextAlign::Left,
                None,
                w,
                cx,
            );
        }
        self.input.update(cx, |input, _| {
            input.lines = std::mem::take(&mut p.lines);
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = crate::tokens::tokens(window);
        let field = cx.entity();
        let selection_field = field.clone();

        div()
            .id("field")
            .role(if self.options.password {
                gpui::Role::PasswordInput
            } else if self.options.multiline {
                gpui::Role::MultilineTextInput
            } else {
                gpui::Role::TextInput
            })
            .aria_value(if self.options.password {
                "•".repeat(self.content.graphemes(true).count())
            } else {
                self.content.to_string()
            })
            .aria_label(self.options.name.clone())
            .aria_placeholder(self.placeholder.clone())
            .on_a11y_action(gpui::AccessibleAction::Focus, move |_, w, cx| {
                let focus = field.read(cx).focus_handle.clone();
                w.focus(&focus, cx);
            })
            .on_a11y_action(
                gpui::AccessibleAction::SetTextSelection,
                move |data, w, cx| {
                    if let Some(accesskit::ActionData::SetTextSelection(selection)) = data {
                        selection_field.update(cx, |field, cx| {
                            if field.options.password {
                                return;
                            }
                            let offset = |i: usize| {
                                field
                                    .content
                                    .grapheme_indices(true)
                                    .nth(i)
                                    .map(|(i, _)| i)
                                    .unwrap_or(field.content.len())
                            };
                            let anchor = offset(selection.anchor.character_index);
                            let focus = offset(selection.focus.character_index);
                            field.selected_range = anchor.min(focus)..anchor.max(focus);
                            field.selection_reversed = focus < anchor;
                            w.notify_text_selection_changed(&field.focus_handle);
                            cx.notify();
                        });
                    }
                },
            )
            .border(if self.options.border { px(1.) } else { px(0.) })
            .rounded(px(7.))
            .border_color(if self.options.error.is_some() {
                t.danger
            } else if self.focus_handle.is_focused(window) {
                t.accent_strong
            } else {
                t.hairline
            })
            .text_color(t.text1)
            .flex()
            .key_context("TextInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .overflow_hidden()
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::delete_word))
            .on_action(cx.listener(Self::document_home))
            .on_action(cx.listener(Self::document_end))
            .on_action(cx.listener(Self::select_document_home))
            .on_action(cx.listener(Self::select_document_end))
            .on_action(cx.listener(Self::delete_line_start))
            .on_action(cx.listener(Self::delete_word_forward))
            .line_height(px(22. * t.zoom))
            .text_size(px(15. * t.zoom))
            .child(div().min_h(px(22. * t.zoom)).w_full().child(TextElement {
                input: cx.entity(),
                text: self.content.to_string(),
                selection: self.selected_range.clone(),
                reversed: self.selection_reversed,
                password: self.options.password,
                multiline: self.options.multiline,
                name: self.options.name.clone(),
            }))
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// Offset conversion rounds a partial surrogate forward, matching the native input contract.
pub fn utf16_to_utf8(text: &str, offset: usize) -> usize {
    let mut utf16 = 0;
    for (i, c) in text.char_indices() {
        if utf16 >= offset {
            return i;
        }
        utf16 += c.len_utf16();
    }
    text.len()
}
pub fn utf8_to_utf16(text: &str, offset: usize) -> usize {
    text.char_indices()
        .take_while(|(i, _)| *i < offset)
        .map(|(_, c)| c.len_utf16())
        .sum()
}
pub fn bind_keys(cx: &mut App) {
    use gpui::KeyBinding;
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-left", Home, Some("TextInput")),
        KeyBinding::new("cmd-right", End, Some("TextInput")),
        KeyBinding::new("cmd-shift-left", SelectHome, Some("TextInput")),
        KeyBinding::new("cmd-shift-right", SelectEnd, Some("TextInput")),
        KeyBinding::new("cmd-up", DocumentHome, Some("TextInput")),
        KeyBinding::new("cmd-down", DocumentEnd, Some("TextInput")),
        KeyBinding::new("cmd-shift-up", SelectDocumentHome, Some("TextInput")),
        KeyBinding::new("cmd-shift-down", SelectDocumentEnd, Some("TextInput")),
        KeyBinding::new("cmd-backspace", DeleteLineStart, Some("TextInput")),
    ]);
    cx.bind_keys([KeyBinding::new(
        if cfg!(target_os = "macos") {
            "alt-delete"
        } else {
            "ctrl-delete"
        },
        DeleteWordForward,
        Some("TextInput"),
    )]);
    cx.bind_keys([
        KeyBinding::new("up", Up, Some("TextInput")),
        KeyBinding::new("down", Down, Some("TextInput")),
        KeyBinding::new("shift-up", SelectUp, Some("TextInput")),
        KeyBinding::new("shift-down", SelectDown, Some("TextInput")),
        KeyBinding::new("backspace", Backspace, Some("TextInput")),
        KeyBinding::new("delete", Delete, Some("TextInput")),
        KeyBinding::new("left", Left, Some("TextInput")),
        KeyBinding::new("right", Right, Some("TextInput")),
        KeyBinding::new("shift-left", SelectLeft, Some("TextInput")),
        KeyBinding::new("shift-right", SelectRight, Some("TextInput")),
        KeyBinding::new("home", Home, Some("TextInput")),
        KeyBinding::new("end", End, Some("TextInput")),
        KeyBinding::new("shift-home", SelectHome, Some("TextInput")),
        KeyBinding::new("shift-end", SelectEnd, Some("TextInput")),
        KeyBinding::new("enter", Newline, Some("TextInput")),
    ]);

    cx.bind_keys([
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-a"
            } else {
                "ctrl-a"
            },
            SelectAll,
            Some("TextInput"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-c"
            } else {
                "ctrl-c"
            },
            Copy,
            Some("TextInput"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-x"
            } else {
                "ctrl-x"
            },
            Cut,
            Some("TextInput"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-v"
            } else {
                "ctrl-v"
            },
            Paste,
            Some("TextInput"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-z"
            } else {
                "ctrl-z"
            },
            Undo,
            Some("TextInput"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-shift-z"
            } else {
                "ctrl-y"
            },
            Redo,
            Some("TextInput"),
        ),
    ]);
    cx.bind_keys([
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "alt-left"
            } else {
                "ctrl-left"
            },
            WordLeft,
            Some("TextInput"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "alt-right"
            } else {
                "ctrl-right"
            },
            WordRight,
            Some("TextInput"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "alt-shift-left"
            } else {
                "ctrl-shift-left"
            },
            SelectWordLeft,
            Some("TextInput"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "alt-shift-right"
            } else {
                "ctrl-shift-right"
            },
            SelectWordRight,
            Some("TextInput"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "alt-backspace"
            } else {
                "ctrl-backspace"
            },
            DeleteWord,
            Some("TextInput"),
        ),
    ]);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn offset_tables_cover_cyrillic_combining_surrogate_and_emoji() {
        let s = "aқе́😀👨‍👩‍👧";
        for (i, _) in s.char_indices() {
            let u = utf8_to_utf16(s, i);
            assert_eq!(utf16_to_utf8(s, u), i);
        }
        assert_eq!(utf16_to_utf8("😀x", 1), 4);
        assert_eq!(utf16_to_utf8("a", 999), 1);
    }
    #[gpui::test]
    fn composition_emits_only_when_committed(cx: &mut gpui::TestAppContext) {
        let (input, cx) = cx.add_window_view(|_, cx| TextInput::new(cx));
        let mut events = vec![];
        let sink = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        let copy = sink.clone();
        cx.update(|_, cx| {
            cx.subscribe(&input, move |_, e: &Committed, _| {
                copy.lock().unwrap().push(e.0.clone())
            })
            .detach()
        });
        cx.update(|w, cx| {
            input.update(cx, |field, cx| {
                field.replace_and_mark_text_in_range(None, "に", None, w, cx);
                assert!(sink.lock().unwrap().is_empty());
                field.replace_text_in_range(None, "日本", w, cx);
            })
        });
        events.extend(sink.lock().unwrap().clone());
        assert_eq!(events, ["日本"]);
    }
    #[gpui::test]
    fn grapheme_deletion_and_undo_redo_restore_selection(cx: &mut gpui::TestAppContext) {
        let (field, cx) = cx.add_window_view(|_, cx| TextInput::new(cx));
        cx.update(|w, cx| {
            field.update(cx, |f, cx| {
                f.set_text("a👨‍👩‍👧е́", w, cx);
                f.backspace(&Backspace, w, cx);
                assert_eq!(f.content.as_ref(), "a👨‍👩‍👧");
                f.backspace(&Backspace, w, cx);
                assert_eq!(f.content.as_ref(), "a");
                f.undo(&Undo, w, cx);
                assert_eq!(f.content.as_ref(), "a👨‍👩‍👧");
                f.redo(&Redo, w, cx);
                assert_eq!(f.content.as_ref(), "a");
            })
        });
    }
    #[gpui::test]
    fn password_render_and_accessibility_never_expose_secret(cx: &mut gpui::TestAppContext) {
        let (field, cx) = cx.add_window_view(|_, cx| TextInput::new(cx));
        cx.update(|w, cx| {
            field.update(cx, |f, cx| {
                f.options.password = true;
                f.set_text("secret😀", w, cx);
                let element = TextElement {
                    input: field.clone(),
                    text: f.content.to_string(),
                    selection: f.selected_range.clone(),
                    reversed: false,
                    password: true,
                    multiline: false,
                    name: "Password".into(),
                };
                let mut node = accesskit::Node::new(gpui::Role::PasswordInput);
                element.write_a11y_info(&mut node);
                assert_eq!(node.value(), Some("•••••••"));
                assert_eq!(node.label(), Some("Password"));
            })
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let f = field.read(cx);
            assert!(
                f.lines
                    .iter()
                    .all(|(_, line, _)| !line.text.contains("secret"))
            );
        });
    }
    #[gpui::test]
    fn multiline_navigation_and_singleline_normalization(cx: &mut gpui::TestAppContext) {
        let (field, cx) = cx.add_window_view(|_, cx| TextInput::new(cx));
        cx.update(|w, cx| {
            field.update(cx, |f, cx| {
                f.set_text("a\nb", w, cx);
                assert_eq!(f.content.as_ref(), "a b");
                f.options.multiline = true;
                f.set_text("one\ntwo", w, cx);
                f.home(&Home, w, cx);
                assert_eq!(f.cursor_offset(), 4);
                f.up(&Up, w, cx);
                assert_eq!(f.cursor_offset(), 0);
                f.down(&Down, w, cx);
                assert_eq!(f.cursor_offset(), 4);
                f.end(&End, w, cx);
                assert_eq!(f.cursor_offset(), 7);
            })
        });
    }
    #[gpui::test]
    fn cancellation_restores_original_without_committing_composition(
        cx: &mut gpui::TestAppContext,
    ) {
        let (field, cx) = cx.add_window_view(|_, cx| TextInput::new(cx));
        cx.update(|w, cx| {
            field.update(cx, |f, cx| {
                f.set_text("before", w, cx);
                f.replace_and_mark_text_in_range(None, "に", None, w, cx);
                f.cancel_composition(w, cx);
                assert_eq!(f.content.as_ref(), "before");
                assert!(f.marked_range.is_none());
            })
        });
    }
    #[gpui::test]
    fn history_is_bounded_and_partial_utf16_ranges_are_safe(cx: &mut gpui::TestAppContext) {
        let (field, cx) = cx.add_window_view(|_, cx| TextInput::new(cx));
        cx.update(|w, cx| {
            field.update(cx, |f, cx| {
                for _ in 0..150 {
                    f.replace_text_in_range(None, "x", w, cx);
                }
                assert_eq!(f.undo.len(), 100);
                f.set_text("😀x", w, cx);
                f.replace_text_in_range(Some(1..2), "y", w, cx);
                assert_eq!(f.content.as_ref(), "😀yx");
                assert_eq!(f.range_from_utf16(&(99..0)), 0..f.content.len());
            })
        });
    }
    #[gpui::test]
    fn clicking_placeholder_never_selects_bytes_outside_empty_content(
        cx: &mut gpui::TestAppContext,
    ) {
        let (field, cx) = cx.add_window_view(|_, cx| TextInput::new(cx));
        cx.run_until_parked();
        cx.update(|_, cx| {
            let field = field.read(cx);
            assert_eq!(field.index_for_mouse_position(point(px(200.), px(10.))), 0);
        });
    }
}

#[cfg(test)]
mod repair_tests {
    use super::*;
    #[gpui::test]
    fn selection_drag_tracks_outside_field_and_any_button_release_ends(
        cx: &mut gpui::TestAppContext,
    ) {
        let (field, cx) = cx.add_window_view(|_, cx| TextInput::new(cx));
        cx.update(|w, cx| field.update(cx, |f, cx| f.set_text("one two three", w, cx)));
        cx.run_until_parked();
        let bounds = cx.update(|_, cx| field.read(cx).last_bounds.unwrap());
        cx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position: point(bounds.left() + px(1.), bounds.center().y),
            click_count: 1,
            ..Default::default()
        });
        cx.run_until_parked();
        cx.simulate_event(MouseMoveEvent {
            position: point(bounds.right() + px(200.), bounds.center().y),
            pressed_button: Some(MouseButton::Left),
            ..Default::default()
        });
        cx.update(|_, cx| {
            assert_eq!(
                field.read(cx).selected_range.end,
                field.read(cx).content.len()
            )
        });
        cx.simulate_event(MouseUpEvent {
            button: MouseButton::Right,
            position: point(bounds.right() + px(200.), bounds.center().y),
            ..Default::default()
        });
        cx.update(|_, cx| assert!(!field.read(cx).is_selecting));
    }
    #[gpui::test]
    fn native_line_document_and_word_delete_keypresses(cx: &mut gpui::TestAppContext) {
        let (field, cx) = cx.add_window_view(|_, cx| TextInput::new(cx));
        cx.update(|w, cx| {
            bind_keys(cx);
            field.update(cx, |f, cx| {
                f.options.multiline = true;
                f.set_text("one е́\ntwo 👨‍👩‍👧 end", w, cx);
                w.focus(&f.focus_handle, cx);
            });
        });
        cx.run_until_parked();
        #[cfg(target_os = "macos")]
        {
            cx.simulate_keystrokes("cmd-left");
            cx.update(|_, cx| assert_eq!(field.read(cx).cursor_offset(), "one е́\n".len()));
            cx.simulate_keystrokes("cmd-shift-right");
            cx.update(|_, cx| {
                assert_eq!(
                    &field.read(cx).content[field.read(cx).selected_range.clone()],
                    "two 👨‍👩‍👧 end"
                )
            });
            cx.simulate_keystrokes("cmd-up");
            cx.update(|_, cx| assert_eq!(field.read(cx).cursor_offset(), 0));
            cx.simulate_keystrokes("cmd-shift-down");
            cx.update(|_, cx| {
                assert_eq!(
                    field.read(cx).selected_range,
                    0..field.read(cx).content.len()
                )
            });
            cx.simulate_keystrokes("cmd-down cmd-backspace");
            cx.update(|_, cx| assert_eq!(field.read(cx).content.as_ref(), "one е́\n"));
            cx.simulate_keystrokes("cmd-up alt-delete");
            cx.update(|_, cx| assert_eq!(field.read(cx).content.as_ref(), " е́\n"));
        }
        #[cfg(not(target_os = "macos"))]
        {
            cx.simulate_keystrokes("home shift-end");
            cx.update(|_, cx| {
                assert_eq!(
                    &field.read(cx).content[field.read(cx).selected_range.clone()],
                    "two 👨‍👩‍👧 end"
                )
            });
            cx.simulate_keystrokes("home ctrl-delete");
            cx.update(|_, cx| assert_eq!(field.read(cx).content.as_ref(), "one е́\n 👨‍👩‍👧 end"));
        }
    }
}
