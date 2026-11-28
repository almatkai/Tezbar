//! A terminal has read-only output and a separate transient native composition span.
//! Marked text never enters the PTY; only replace_text_in_range commits it.
use super::*;
use std::ops::Range;
#[derive(Default)]
pub(super) struct Preedit {
    pub text: String,
    pub selected: Range<usize>,
}
impl Preedit {
    pub fn clear(&mut self) {
        self.text.clear();
        self.selected = 0..0;
    }
    fn byte(&self, n: usize) -> usize {
        let mut units = 0;
        for (i, c) in self.text.char_indices() {
            if units >= n {
                return i;
            }
            units += c.len_utf16();
        }
        self.text.len()
    }
    pub fn replace(&mut self, r: Option<Range<usize>>, s: &str, selected: Option<Range<usize>>) {
        let r = r
            .map(|r| self.byte(r.start)..self.byte(r.end.max(r.start)))
            .unwrap_or(0..self.text.len());
        if self.text.len() - (r.end - r.start) + s.len() > 16 * 1024 {
            return;
        }
        self.text.replace_range(r, s);
        let count = self.text.encode_utf16().count();
        self.selected = selected
            .map(|r| r.start.min(count)..r.end.max(r.start).min(count))
            .unwrap_or(count..count);
    }
}
impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        r: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let start = self.preedit.byte(r.start);
        let end = self.preedit.byte(r.end.max(r.start));
        *actual = Some(
            self.preedit.text[..start].encode_utf16().count()
                ..self.preedit.text[..end].encode_utf16().count(),
        );
        Some(self.preedit.text[start..end].into())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.preedit.selected.clone(),
            reversed: false,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        (!self.preedit.text.is_empty()).then(|| 0..self.preedit.text.encode_utf16().count())
    }
    fn unmark_text(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.preedit.clear();
        w.notify_text_input_changed(&self.focus);
        self.sync_blink(cx);
        self.changed(cx);
    }
    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preedit.clear();
        if self.state.visible {
            self.state.write(text.into(), true);
            self.track_cwd(cx);
            self.pump(cx);
        }
        self.cursor_on = true;
        w.notify_text_input_changed(&self.focus);
        self.sync_blink(cx);
        self.changed(cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.state.visible || text.len() > 16 * 1024 {
            return;
        }
        self.preedit.replace(r, text, selected);
        w.notify_text_input_changed(&self.focus);
        self.sync_blink(cx);
        self.changed(cx);
    }
    fn bounds_for_range(
        &mut self,
        r: Range<usize>,
        _: Bounds<Pixels>,
        w: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let (b, m) = self.geometry?;
        let (x, y) = self.state.grid.cursor_cell()?;
        let start = self.preedit.byte(r.start);
        let end = self.preedit.byte(r.end.max(r.start));
        let text = self.preedit.text.clone();
        let line = w.text_system().shape_line(
            text.clone().into(),
            px(self.state.font_size as f32),
            &[TextRun {
                len: text.len(),
                font: font("SFMono-Regular"),
                color: rgb(self.state.grid.palette.foreground).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );
        let offset = f32::from(line.x_for_index(start));
        let width = f32::from(line.x_for_index(end) - line.x_for_index(start)).max(1.);
        Some(Bounds::new(
            point(
                b.left() + px(x as f32 * m.width + offset),
                b.top() + px(y as f32 * m.height),
            ),
            size(px(width), px(m.height)),
        ))
    }
    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let b = self.bounds_for_range(0..0, Bounds::default(), w, cx)?;
        let (_, m) = self.geometry?;
        let chars = (f32::from(p.x - b.left()) / m.width).round().max(0.) as usize;
        Some(
            self.preedit
                .text
                .chars()
                .take(chars)
                .map(char::len_utf16)
                .sum(),
        )
    }
    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.preedit.text.encode_utf16().count())
    }
    fn set_selected_text_range(&mut self, r: Range<usize>, w: &mut Window, cx: &mut Context<Self>) {
        let n = self.preedit.text.encode_utf16().count();
        self.preedit.selected = r.start.min(n)..r.end.min(n);
        w.notify_text_selection_changed(&self.focus);
        self.changed(cx);
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
    fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
        self.state.visible && !self.state.disposed && self.state.configure.is_none()
    }
    fn paste(&mut self, item: ClipboardItem, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = item.text() {
            self.paste(self.state.generation, &text, cx);
        }
    }
}
