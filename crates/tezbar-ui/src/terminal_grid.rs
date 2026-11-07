//! First-party GPUI terminal grid over crates.io alacritty_terminal (Apache-2.0).
//! No PTY, event loop, clipboard, shell or process is owned by this module.
use alacritty_terminal::{
    event::{Event, EventListener, WindowSize},
    grid::{Dimensions, Scroll},
    index::{Column, Line, Point as CellPoint, Side},
    selection::{Selection, SelectionType},
    term::{Config, Osc52, Term, TermMode, cell::Flags},
    vte::ansi::{Color, CursorShape, CursorStyle as VtCursorStyle, Processor, Rgb},
};
use gpui::{prelude::*, *};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

pub const SCROLLBACK: usize = 100_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridSize {
    pub cols: u16,
    pub rows: u16,
}
impl Default for GridSize {
    fn default() -> Self {
        Self { cols: 80, rows: 24 }
    }
}
impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows as usize
    }
    fn screen_lines(&self) -> usize {
        self.rows as usize
    }
    fn columns(&self) -> usize {
        self.cols as usize
    }
}
#[derive(Clone, Copy, Debug)]
pub struct CellMetrics {
    pub width: f32,
    pub height: f32,
}
impl CellMetrics {
    pub fn grid(self, width: f32, height: f32) -> Option<GridSize> {
        if ![width, height, self.width, self.height]
            .iter()
            .all(|n| n.is_finite() && *n > 0.)
        {
            return None;
        }
        Some(GridSize {
            cols: (width / self.width).floor().clamp(2., 500.) as u16,
            rows: (height / self.height).floor().clamp(2., 300.) as u16,
        })
    }
    pub fn cell(self, x: f32, y: f32, size: GridSize) -> (usize, usize) {
        (
            (x / self.width).floor().max(0.).min((size.cols - 1) as f32) as usize,
            (y / self.height)
                .floor()
                .max(0.)
                .min((size.rows - 1) as f32) as usize,
        )
    }
}
#[derive(Clone, Copy)]
pub struct Palette {
    pub ansi: [u32; 16],
    pub foreground: u32,
    pub cursor: u32,
    pub selection: u32,
    pub light: bool,
}
impl Palette {
    pub fn new(light: bool) -> Self {
        if light {
            Self {
                ansi: [
                    0x26334b, 0xb42342, 0x08764c, 0x9a5b00, 0x2856ad, 0x7c3eaa, 0x08788a, 0x526079,
                    0x61708a, 0xcf3f5e, 0x16885b, 0xae6a0c, 0x386cc7, 0x9256c2, 0x168da0, 0x26334b,
                ],
                foreground: 0x26334b,
                cursor: 0x4c44c1,
                selection: 0x4c44c133,
                light,
            }
        } else {
            Self {
                ansi: [
                    0x20212f, 0xfb7185, 0x86efac, 0xfde68a, 0x93c5fd, 0xc4b5fd, 0x67e8f9, 0xe5e7eb,
                    0x73758a, 0xfda4af, 0xbbf7d0, 0xfef3c7, 0xbfdbfe, 0xddd6fe, 0xa5f3fc, 0xffffff,
                ],
                foreground: 0xe8e8f0,
                cursor: 0xa7f3d0,
                selection: 0x6366f155,
                light,
            }
        }
    }
    fn indexed(self, i: usize) -> Option<u32> {
        match i {
            0..=15 => Some(self.ansi[i]),
            16..=231 => {
                let i = i - 16;
                let level = |n| if n == 0 { 0 } else { 55 + 40 * n };
                Some(((level(i / 36) << 16) | (level(i / 6 % 6) << 8) | level(i % 6)) as u32)
            }
            232..=255 => Some((8 + 10 * (i as u32 - 232)) * 0x010101),
            256 | 267 => Some(self.foreground),
            257 => None,
            258 => Some(self.cursor),
            259..=266 => self.indexed(i - 259).map(dim),
            268 => Some(dim(self.foreground)),
            _ => None,
        }
    }
    fn resolve(self, c: Color, colors: &alacritty_terminal::term::color::Colors) -> Option<u32> {
        let i = match c {
            Color::Spec(c) => return Some(rgb_u32(c)),
            Color::Named(n) => n as usize,
            Color::Indexed(i) => i as usize,
        };
        colors[i].map(rgb_u32).or_else(|| self.indexed(i))
    }
}
fn rgb_u32(c: Rgb) -> u32 {
    ((c.r as u32) << 16) | ((c.g as u32) << 8) | c.b as u32
}
fn dim(c: u32) -> u32 {
    ((((c >> 16) & 255) * 2 / 3) << 16) | ((((c >> 8) & 255) * 2 / 3) << 8) | ((c & 255) * 2 / 3)
}
#[derive(Default)]
struct Replies {
    queue: VecDeque<Event>,
    overflow: bool,
}
#[derive(Clone, Default)]
struct Listener(Arc<Mutex<Replies>>);
impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        // OS clipboard requests are deliberately not handled. OSC52 is disabled in Config.
        if !matches!(
            &event,
            Event::PtyWrite(_) | Event::ColorRequest(..) | Event::TextAreaSizeRequest(..)
        ) {
            return;
        }
        let mut q = self.0.lock().unwrap();
        if q.queue.len() >= 128 || matches!(&event, Event::PtyWrite(s) if s.len()>16384) {
            q.overflow = true;
            return;
        }
        q.queue.push_back(event);
    }
}
pub struct TerminalGrid {
    term: Term<Listener>,
    processor: Processor,
    listener: Listener,
    pub size: GridSize,
    pub palette: Palette,
}
impl TerminalGrid {
    pub fn new(size: GridSize, light: bool) -> Self {
        let size = GridSize {
            cols: size.cols.clamp(2, 500),
            rows: size.rows.clamp(2, 300),
        };
        let listener = Listener::default();
        let config = Config {
            scrolling_history: SCROLLBACK,
            default_cursor_style: VtCursorStyle {
                shape: CursorShape::Beam,
                blinking: true,
            },
            osc52: Osc52::Disabled,
            ..Default::default()
        };
        let term = Term::new(config, &size, listener.clone());
        let mut this = Self {
            term,
            processor: Processor::new(),
            listener,
            size,
            palette: Palette::new(light),
        };
        this.feed(b"\x1b[20h"); // Match xterm convertEol:true (LF is normalized during feed).
        this
    }
    pub fn feed(&mut self, data: &[u8]) {
        self.processor
            .advance(&mut xterm_newline::Adapter(&mut self.term), data);
    }
    pub fn resize(&mut self, size: GridSize) {
        let size = GridSize {
            cols: size.cols.clamp(2, 500),
            rows: size.rows.clamp(2, 300),
        };
        self.size = size;
        self.term.resize(size);
    }
    pub fn mode(&self) -> TermMode {
        *self.term.mode()
    }
    pub fn scroll(&mut self, lines: i32) {
        self.term.scroll_display(Scroll::Delta(lines));
    }
    pub fn bottom(&mut self) {
        self.term.scroll_display(Scroll::Bottom);
    }
    pub fn history_size(&self) -> usize {
        self.term.history_size()
    }
    pub fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }
    fn point(&self, col: usize, row: usize) -> CellPoint {
        CellPoint::new(
            Line(row.min(self.size.rows as usize - 1) as i32 - self.display_offset() as i32),
            Column(col.min(self.size.cols as usize - 1)),
        )
    }
    pub fn start_selection(&mut self, col: usize, row: usize, ty: SelectionType) {
        self.term.selection = Some(Selection::new(ty, self.point(col, row), Side::Left));
    }
    pub fn extend_selection(&mut self, col: usize, row: usize) {
        let p = self.point(col, row);
        if let Some(s) = &mut self.term.selection {
            s.update(p, Side::Right);
        }
    }
    pub fn selected_text(&self) -> Option<String> {
        self.term.selection_to_string()
    }
    pub fn clear_selection(&mut self) {
        self.term.selection = None;
    }
    pub fn cursor_cell(&self) -> Option<(usize, usize)> {
        let c = self.term.renderable_content();
        let y = c.cursor.point.line.0 + c.display_offset as i32;
        (c.cursor.shape != CursorShape::Hidden && y >= 0 && y < self.size.rows as i32)
            .then_some((c.cursor.point.column.0, y.max(0) as usize))
    }
    pub fn blink_needed(&self) -> bool {
        self.cursor_cell().is_some() && self.term.cursor_style().blinking
    }
    pub fn discard_protocol_replies(&mut self) {
        let mut q = self.listener.0.lock().unwrap();
        q.queue.clear();
        q.overflow = false;
    }
    pub fn protocol_replies(&mut self, metrics: CellMetrics) -> Result<Vec<String>, &'static str> {
        let mut q = self.listener.0.lock().unwrap();
        if std::mem::take(&mut q.overflow) {
            q.queue.clear();
            return Err("Terminal protocol reply buffer overflow");
        }
        let mut out = vec![];
        for e in q.queue.drain(..) {
            match e {
                Event::PtyWrite(s) => out.push(s),
                Event::ColorRequest(i, f) => {
                    let c = self.term.colors()[i.min(alacritty_terminal::term::color::COUNT - 1)]
                        .map(rgb_u32)
                        .or_else(|| self.palette.indexed(i))
                        .unwrap_or(if self.palette.light {
                            0xfafafb
                        } else {
                            0x10131d
                        });
                    out.push(f(Rgb {
                        r: (c >> 16) as u8,
                        g: (c >> 8) as u8,
                        b: c as u8,
                    }));
                }
                Event::TextAreaSizeRequest(f) => out.push(f(WindowSize {
                    num_cols: self.size.cols,
                    num_lines: self.size.rows,
                    cell_width: metrics.width.round().clamp(1., 65535.) as u16,
                    cell_height: metrics.height.round().clamp(1., 65535.) as u16,
                })),
                _ => {}
            }
        }
        Ok(out)
    }
    pub fn frame(&self) -> Frame {
        let content = self.term.renderable_content();
        let mut rows: Vec<Vec<PaintCell>> = (0..self.size.rows).map(|_| vec![]).collect();
        for indexed in content.display_iter {
            let y = indexed.point.line.0 + content.display_offset as i32;
            if y < 0 || y >= self.size.rows as i32 {
                continue;
            }
            let cell = indexed.cell;
            let flags = cell.flags;
            let mut fg = self
                .palette
                .resolve(cell.fg, content.colors)
                .unwrap_or(self.palette.foreground);
            let mut bg = self.palette.resolve(cell.bg, content.colors);
            if flags.contains(Flags::BOLD) {
                if let Color::Named(n) = cell.fg {
                    fg = self
                        .palette
                        .resolve(Color::Named(n.to_bright()), content.colors)
                        .unwrap_or(fg);
                }
            }
            if flags.contains(Flags::DIM) {
                fg = dim(fg);
            }
            if flags.contains(Flags::INVERSE) {
                let old = fg;
                fg = bg.unwrap_or(if self.palette.light {
                    0xfafafb
                } else {
                    0x10131d
                });
                bg = Some(old);
            }
            let selected = content
                .selection
                .as_ref()
                .is_some_and(|s| s.contains(indexed.point));
            let mut text = if flags.intersects(
                Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER | Flags::HIDDEN,
            ) {
                String::new()
            } else {
                cell.c.to_string()
            };
            if !text.is_empty() {
                if let Some(chars) = cell.zerowidth() {
                    text.extend(chars);
                }
            }
            rows[y as usize].push(PaintCell {
                col: indexed.point.column.0,
                text,
                fg,
                bg,
                selected,
                bold: flags.contains(Flags::BOLD),
                italic: flags.contains(Flags::ITALIC),
                underline: flags.intersects(Flags::ALL_UNDERLINES),
                strike: flags.contains(Flags::STRIKEOUT),
                wide: flags.contains(Flags::WIDE_CHAR),
            });
        }
        let text = rows
            .iter()
            .map(|r| {
                r.iter()
                    .map(|c| c.text.as_str())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n");
        let cursor = self.cursor_cell();
        let cursor_line = cursor
            .and_then(|(_, y)| rows.get(y))
            .map(|r| r.iter().map(|c| c.text.as_str()).collect::<String>())
            .unwrap_or_default();
        Frame {
            rows,
            text,
            cursor_line,
            cursor,
            palette: self.palette,
        }
    }
    pub fn url_at(&self, col: usize, row: usize) -> Option<String> {
        let p = self.point(col, row);
        let cell = &self.term.grid()[p];
        if let Some(h) = cell.hyperlink() {
            return allowed_url(h.uri());
        }
        // Scan the bounded logical line, including wrapped continuations.
        let mut start = p.line;
        let mut end = p.line;
        while start.0 > -(self.term.history_size() as i32)
            && self.term.grid()
                [CellPoint::new(start - Line(1), Column(self.size.cols as usize - 1))]
            .flags
            .contains(Flags::WRAPLINE)
        {
            start -= 1;
        }
        while end.0 < self.size.rows as i32 - 1
            && self.term.grid()[CellPoint::new(end, Column(self.size.cols as usize - 1))]
                .flags
                .contains(Flags::WRAPLINE)
        {
            end += 1;
        }
        // Do not allocate an unbounded wrapped line from hostile output.
        if end.0 - start.0 > 64 {
            return None;
        }
        let mut text = String::new();
        let mut hit = 0;
        for y in start.0..=end.0 {
            for x in 0..self.size.cols as usize {
                let c = &self.term.grid()[CellPoint::new(Line(y), Column(x))];
                if y == p.line.0 && x == p.column.0 {
                    hit = text.len();
                }
                if !c.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    text.push(c.c);
                    if let Some(z) = c.zerowidth() {
                        text.extend(z);
                    }
                }
            }
        }
        let left = text[..hit]
            .rfind(char::is_whitespace)
            .map_or(0, |i| i + text[i..].chars().next().unwrap().len_utf8());
        let right = text[hit..]
            .find(char::is_whitespace)
            .map_or(text.len(), |i| hit + i);
        allowed_url(
            text[left..right].trim_matches(|c| {
                matches!(c, '"' | '\'' | '<' | '>' | '(' | ')' | '[' | ']' | ',')
            }),
        )
    }
}
pub fn allowed_url(s: &str) -> Option<String> {
    if s.len() > 8192 || s.chars().any(char::is_control) {
        return None;
    }
    let u = url::Url::parse(s).ok()?;
    matches!(u.scheme(), "http" | "https" | "mailto").then(|| u.to_string())
}
#[derive(Clone)]
pub struct PaintCell {
    pub col: usize,
    pub text: String,
    pub fg: u32,
    pub bg: Option<u32>,
    pub selected: bool,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub wide: bool,
}
#[derive(Clone)]
pub struct Frame {
    pub rows: Vec<Vec<PaintCell>>,
    pub text: String,
    pub cursor_line: String,
    pub cursor: Option<(usize, usize)>,
    pub palette: Palette,
}

/// Coalesce compatible mono ASCII spans per row; wide/combining/fallback glyphs
/// stay anchored to their own cell so font fallback cannot move the next cell.
fn row_runs(cells: &[PaintCell]) -> Vec<PaintCell> {
    let mut runs: Vec<PaintCell> = Vec::new();
    for cell in cells {
        if cell.text.is_empty() {
            continue;
        }
        if let Some(last) = runs.last_mut() {
            if !last.wide
                && !cell.wide
                && last.text.is_ascii()
                && cell.text.is_ascii()
                && cell.col == last.col + last.text.len()
                && last.fg == cell.fg
                && last.bold == cell.bold
                && last.italic == cell.italic
                && last.underline == cell.underline
                && last.strike == cell.strike
            {
                last.text.push_str(&cell.text);
                continue;
            }
        }
        runs.push(cell.clone());
    }
    runs
}

/// A single native element; no DOM node or GPUI text field per cell. Shape only visible
/// cells, and keep glyph coordinates at integer cell multiples (including fallback fonts).
pub fn element(
    frame: Frame,
    font_size: u8,
    cursor_on: bool,
    preedit: String,
    geometry: impl FnOnce(Bounds<Pixels>, CellMetrics, &mut App) + 'static,
    input: impl FnOnce(Bounds<Pixels>, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let accessible = frame.text.clone();
    let cursor_line = frame.cursor_line.clone();
    div()
        .id("terminal-grid")
        .role(Role::Document)
        .aria_label("Terminal")
        .a11y_synthetic_children(move |b| {
            let node = b.parent_node();
            node.set_read_only();
            node.set_value(accessible.clone());
            node.set_description(format!("Cursor line: {cursor_line}"));
        })
        .size_full()
        .overflow_hidden()
        .child(
            canvas(
                move |bounds, w, cx| {
                    let font = font("SFMono-Regular");
                    let probe = w.text_system().shape_line(
                        "M".into(),
                        px(font_size as f32),
                        &[TextRun {
                            len: 1,
                            font: font.clone(),
                            color: rgb(frame.palette.foreground).into(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }],
                        None,
                    );
                    let metrics = CellMetrics {
                        width: f32::from(probe.width).max(1.),
                        height: font_size as f32 * 1.22,
                    };
                    geometry(bounds, metrics, cx);
                    let mut glyphs = vec![];
                    let mut quads = vec![];
                    for (row, cells) in frame.rows.iter().enumerate() {
                        for cell in cells {
                            let at = point(
                                bounds.left() + px(cell.col as f32 * metrics.width),
                                bounds.top() + px(row as f32 * metrics.height),
                            );
                            let b = Bounds::new(
                                at,
                                size(
                                    px(metrics.width * if cell.wide { 2. } else { 1. }),
                                    px(metrics.height),
                                ),
                            );
                            if let Some(bg) = cell.bg {
                                quads.push(fill(b, rgb(bg)));
                            }
                            if cell.selected {
                                quads.push(fill(b, rgba(frame.palette.selection)));
                            }
                        }
                        for cell in row_runs(cells) {
                            if cell.text.trim().is_empty() && !cell.underline && !cell.strike {
                                continue;
                            }
                            let at = point(
                                bounds.left() + px(cell.col as f32 * metrics.width),
                                bounds.top() + px(row as f32 * metrics.height),
                            );
                            let mut f = font.clone();
                            if cell.bold {
                                f.weight = FontWeight::BOLD;
                            }
                            if cell.italic {
                                f.style = FontStyle::Italic;
                            }
                            let line = w.text_system().shape_line(
                                cell.text.clone().into(),
                                px(font_size as f32),
                                &[TextRun {
                                    len: cell.text.len(),
                                    font: f,
                                    color: rgb(cell.fg).into(),
                                    background_color: None,
                                    underline: cell.underline.then_some(UnderlineStyle {
                                        thickness: px(1.),
                                        color: Some(rgb(cell.fg).into()),
                                        wavy: false,
                                    }),
                                    strikethrough: cell.strike.then_some(StrikethroughStyle {
                                        thickness: px(1.),
                                        color: Some(rgb(cell.fg).into()),
                                    }),
                                }],
                                None,
                            );
                            glyphs.push((at, line));
                        }
                    }
                    if let Some((col, row)) = frame.cursor {
                        let at = point(
                            bounds.left() + px(col as f32 * metrics.width),
                            bounds.top() + px(row as f32 * metrics.height),
                        );
                        if cursor_on {
                            quads.push(fill(
                                Bounds::new(at, size(px(1.5), px(metrics.height))),
                                rgb(frame.palette.cursor),
                            ));
                        }
                        if !preedit.is_empty() {
                            let line = w.text_system().shape_line(
                                preedit.clone().into(),
                                px(font_size as f32),
                                &[TextRun {
                                    len: preedit.len(),
                                    font,
                                    color: rgb(frame.palette.foreground).into(),
                                    background_color: None,
                                    underline: Some(UnderlineStyle {
                                        thickness: px(1.),
                                        color: None,
                                        wavy: false,
                                    }),
                                    strikethrough: None,
                                }],
                                None,
                            );
                            glyphs.push((at, line));
                        }
                    }
                    (glyphs, quads, metrics)
                },
                move |bounds, (glyphs, quads, metrics), w, cx| {
                    input(bounds, w, cx);
                    w.with_content_mask(Some(ContentMask { bounds }), |w| {
                        for q in quads {
                            w.paint_quad(q);
                        }
                        for (at, line) in glyphs {
                            let _ =
                                line.paint(at, px(metrics.height), TextAlign::Left, None, w, cx);
                        }
                    });
                },
            )
            .size_full(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;
    #[test]
    fn vt_split_utf8_csi_erase_alternate_screen() {
        let mut t = TerminalGrid::new(GridSize { cols: 10, rows: 3 }, false);
        t.feed(b"a\x1b[31");
        t.feed(b"mR\x1b[0m");
        let f = t.frame();
        assert_eq!(&f.rows[0][1].text, "R");
        assert_eq!(f.rows[0][1].fg, 0xfb7185);
        t.feed(&[0xe7, 0x95]);
        t.feed(&[0x8c]);
        assert_eq!(t.frame().rows[0][2].text, "界");
        t.feed(b"\x1b[?1049hALT");
        // DECSET 1049 copies the primary cursor (column 4 after wide 界).
        // Full-screen programs explicitly CUP home; the adapter must not invent it.
        assert_eq!(t.frame().rows[0][4].text, "A");
        assert_eq!(t.frame().rows[0][5].text, "L");
        assert_eq!(t.frame().rows[0][6].text, "T");
        t.feed(b"\x1b[2J\x1b[HALT");
        assert!(t.frame().text.starts_with("ALT"));
        t.feed(b"\x1b[?1049l");
        assert!(t.frame().text.starts_with("aR界"));
        t.feed(b"\x1b[2J\x1b[Hok");
        assert!(t.frame().text.starts_with("ok"));
    }
    #[test]
    fn vt_combining_truecolor_selection_and_url() {
        let mut t = TerminalGrid::new(GridSize { cols: 40, rows: 3 }, true);
        t.feed("e\u{301}\x1b[38;2;1;2;3mX\x1b[0m https://example.org".as_bytes());
        let f = t.frame();
        assert_eq!(f.rows[0][0].text, "e\u{301}");
        assert_eq!(f.rows[0][1].fg, 0x010203);
        t.start_selection(0, 0, SelectionType::Simple);
        t.extend_selection(1, 0);
        assert_eq!(t.selected_text().as_deref(), Some("e\u{301}X"));
        assert_eq!(t.url_at(8, 0).as_deref(), Some("https://example.org/"));
        assert!(allowed_url("file:///etc/passwd").is_none());
    }
    #[test]
    fn history_cap_retains_viewport_and_palette_edges() {
        let mut t = TerminalGrid::new(GridSize { cols: 2, rows: 2 }, false);
        for _ in 0..SCROLLBACK + 20 {
            t.feed(b"x\r\n");
        }
        assert!(t.history_size() <= SCROLLBACK);
        t.scroll(5);
        let at = t.display_offset();
        t.feed(b"z\r\n");
        assert!(t.display_offset() >= at);
        assert_eq!(Palette::new(true).indexed(9), Some(0xcf3f5e));
        assert_eq!(Palette::new(false).indexed(196), Some(0xff0000));
        assert_eq!(Palette::new(false).indexed(255), Some(0xeeeeee));
    }
    #[test]
    fn metrics_are_physical_cell_derived_and_bounded() {
        let m = CellMetrics {
            width: 7.25,
            height: 14.64,
        };
        assert_eq!(
            m.grid(725., 352.),
            Some(GridSize {
                cols: 100,
                rows: 24
            })
        );
        assert_eq!(
            m.grid(100000., 100000.),
            Some(GridSize {
                cols: 500,
                rows: 300
            })
        );
        assert_eq!(m.grid(f32::NAN, 100.), None);
    }
    #[test]
    fn vt_device_reply_no_osc52_host_access() {
        let mut t = TerminalGrid::new(GridSize::default(), false);
        t.feed(b"\x1b[6n\x1b]52;c;SGVsbG8=\x07");
        assert_eq!(
            t.protocol_replies(CellMetrics {
                width: 7.,
                height: 15.
            })
            .unwrap(),
            vec!["\x1b[1;1R"]
        );
    }
}

// Preserve the reference xterm convertEol option at the decoded VT action boundary.
// All parsing and every other action remain the published Alacritty/VTE implementation.
mod xterm_newline {
    use alacritty_terminal::vte::ansi::cursor_icon::CursorIcon;
    use alacritty_terminal::{event::EventListener, term::Term, vte::ansi::*};
    pub struct Adapter<'a, L: EventListener>(pub &'a mut Term<L>);
    impl<L: EventListener> Handler for Adapter<'_, L> {
        fn set_title(&mut self, arg0: Option<String>) {
            self.0.set_title(arg0)
        }
        fn set_cursor_style(&mut self, arg0: Option<CursorStyle>) {
            self.0.set_cursor_style(arg0)
        }
        fn set_cursor_shape(&mut self, arg0: CursorShape) {
            self.0.set_cursor_shape(arg0)
        }
        fn input(&mut self, arg0: char) {
            self.0.input(arg0)
        }
        fn goto(&mut self, arg0: i32, arg1: usize) {
            self.0.goto(arg0, arg1)
        }
        fn goto_line(&mut self, arg0: i32) {
            self.0.goto_line(arg0)
        }
        fn goto_col(&mut self, arg0: usize) {
            self.0.goto_col(arg0)
        }
        fn insert_blank(&mut self, arg0: usize) {
            self.0.insert_blank(arg0)
        }
        fn move_up(&mut self, arg0: usize) {
            self.0.move_up(arg0)
        }
        fn move_down(&mut self, arg0: usize) {
            self.0.move_down(arg0)
        }
        fn identify_terminal(&mut self, arg0: Option<char>) {
            self.0.identify_terminal(arg0)
        }
        fn device_status(&mut self, arg0: usize) {
            self.0.device_status(arg0)
        }
        fn move_forward(&mut self, arg0: usize) {
            self.0.move_forward(arg0)
        }
        fn move_backward(&mut self, arg0: usize) {
            self.0.move_backward(arg0)
        }
        fn move_down_and_cr(&mut self, arg0: usize) {
            self.0.move_down_and_cr(arg0)
        }
        fn move_up_and_cr(&mut self, arg0: usize) {
            self.0.move_up_and_cr(arg0)
        }
        fn put_tab(&mut self, arg0: u16) {
            self.0.put_tab(arg0)
        }
        fn backspace(&mut self) {
            self.0.backspace()
        }
        fn carriage_return(&mut self) {
            self.0.carriage_return()
        }
        fn linefeed(&mut self) {
            self.0.carriage_return();
            self.0.linefeed()
        }
        fn bell(&mut self) {
            self.0.bell()
        }
        fn substitute(&mut self) {
            self.0.substitute()
        }
        fn newline(&mut self) {
            self.0.newline()
        }
        fn set_horizontal_tabstop(&mut self) {
            self.0.set_horizontal_tabstop()
        }
        fn scroll_up(&mut self, arg0: usize) {
            self.0.scroll_up(arg0)
        }
        fn scroll_down(&mut self, arg0: usize) {
            self.0.scroll_down(arg0)
        }
        fn insert_blank_lines(&mut self, arg0: usize) {
            self.0.insert_blank_lines(arg0)
        }
        fn delete_lines(&mut self, arg0: usize) {
            self.0.delete_lines(arg0)
        }
        fn erase_chars(&mut self, arg0: usize) {
            self.0.erase_chars(arg0)
        }
        fn delete_chars(&mut self, arg0: usize) {
            self.0.delete_chars(arg0)
        }
        fn move_backward_tabs(&mut self, arg0: u16) {
            self.0.move_backward_tabs(arg0)
        }
        fn move_forward_tabs(&mut self, arg0: u16) {
            self.0.move_forward_tabs(arg0)
        }
        fn save_cursor_position(&mut self) {
            self.0.save_cursor_position()
        }
        fn restore_cursor_position(&mut self) {
            self.0.restore_cursor_position()
        }
        fn clear_line(&mut self, arg0: LineClearMode) {
            self.0.clear_line(arg0)
        }
        fn clear_screen(&mut self, arg0: ClearMode) {
            self.0.clear_screen(arg0)
        }
        fn clear_tabs(&mut self, arg0: TabulationClearMode) {
            self.0.clear_tabs(arg0)
        }
        fn set_tabs(&mut self, arg0: u16) {
            self.0.set_tabs(arg0)
        }
        fn reset_state(&mut self) {
            self.0.reset_state()
        }
        fn reverse_index(&mut self) {
            self.0.reverse_index()
        }
        fn terminal_attribute(&mut self, arg0: Attr) {
            self.0.terminal_attribute(arg0)
        }
        fn set_mode(&mut self, arg0: Mode) {
            self.0.set_mode(arg0)
        }
        fn unset_mode(&mut self, arg0: Mode) {
            self.0.unset_mode(arg0)
        }
        fn report_mode(&mut self, arg0: Mode) {
            self.0.report_mode(arg0)
        }
        fn set_private_mode(&mut self, arg0: PrivateMode) {
            self.0.set_private_mode(arg0)
        }
        fn unset_private_mode(&mut self, arg0: PrivateMode) {
            self.0.unset_private_mode(arg0)
        }
        fn report_private_mode(&mut self, arg0: PrivateMode) {
            self.0.report_private_mode(arg0)
        }
        fn set_scrolling_region(&mut self, arg0: usize, arg1: Option<usize>) {
            self.0.set_scrolling_region(arg0, arg1)
        }
        fn set_keypad_application_mode(&mut self) {
            self.0.set_keypad_application_mode()
        }
        fn unset_keypad_application_mode(&mut self) {
            self.0.unset_keypad_application_mode()
        }
        fn set_active_charset(&mut self, arg0: CharsetIndex) {
            self.0.set_active_charset(arg0)
        }
        fn configure_charset(&mut self, arg0: CharsetIndex, arg1: StandardCharset) {
            self.0.configure_charset(arg0, arg1)
        }
        fn set_color(&mut self, arg0: usize, arg1: Rgb) {
            self.0.set_color(arg0, arg1)
        }
        fn dynamic_color_sequence(&mut self, arg0: String, arg1: usize, arg2: &str) {
            self.0.dynamic_color_sequence(arg0, arg1, arg2)
        }
        fn reset_color(&mut self, arg0: usize) {
            self.0.reset_color(arg0)
        }
        fn clipboard_store(&mut self, arg0: u8, arg1: &[u8]) {
            self.0.clipboard_store(arg0, arg1)
        }
        fn clipboard_load(&mut self, arg0: u8, arg1: &str) {
            self.0.clipboard_load(arg0, arg1)
        }
        fn decaln(&mut self) {
            self.0.decaln()
        }
        fn push_title(&mut self) {
            self.0.push_title()
        }
        fn pop_title(&mut self) {
            self.0.pop_title()
        }
        fn text_area_size_pixels(&mut self) {
            self.0.text_area_size_pixels()
        }
        fn text_area_size_chars(&mut self) {
            self.0.text_area_size_chars()
        }
        fn set_hyperlink(&mut self, arg0: Option<Hyperlink>) {
            self.0.set_hyperlink(arg0)
        }
        fn set_mouse_cursor_icon(&mut self, arg0: CursorIcon) {
            self.0.set_mouse_cursor_icon(arg0)
        }
        fn report_keyboard_mode(&mut self) {
            self.0.report_keyboard_mode()
        }
        fn push_keyboard_mode(&mut self, arg0: KeyboardModes) {
            self.0.push_keyboard_mode(arg0)
        }
        fn pop_keyboard_modes(&mut self, arg0: u16) {
            self.0.pop_keyboard_modes(arg0)
        }
        fn set_keyboard_mode(&mut self, arg0: KeyboardModes, arg1: KeyboardModesApplyBehavior) {
            self.0.set_keyboard_mode(arg0, arg1)
        }
        fn set_modify_other_keys(&mut self, arg0: ModifyOtherKeys) {
            self.0.set_modify_other_keys(arg0)
        }
        fn report_modify_other_keys(&mut self) {
            self.0.report_modify_other_keys()
        }
        fn set_scp(&mut self, arg0: ScpCharPath, arg1: ScpUpdateMode) {
            self.0.set_scp(arg0, arg1)
        }
    }
}
