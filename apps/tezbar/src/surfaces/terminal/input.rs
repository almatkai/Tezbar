//! Xterm input encoding and app interception; no VT output parsing lives here.
use alacritty_terminal::term::TermMode;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub command: bool,
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zoom {
    In,
    Out,
    Reset,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyResult {
    PlatformText,
    Ignore,
    Bytes(String),
    Back,
    Configure,
    Stop,
    Pin,
    Delete,
    Copy,
    Paste,
    TextZoom(Zoom),
    UiZoom(Zoom),
    Session(usize),
}
pub fn key(key: &str, code: &str, m: Modifiers, mode: TermMode, composing: bool) -> KeyResult {
    if composing {
        return KeyResult::PlatformText;
    }
    let lower = key.to_lowercase();
    // Shell control characters are never app actions on macOS. Windows app
    // actions use Ctrl+Shift so Ctrl+D/X/K/P remain available to the shell.
    let primary = m.command || (cfg!(not(target_os = "macos")) && m.control);
    let app_action = m.command || (cfg!(not(target_os = "macos")) && m.control && m.shift);
    if primary {
        let z = match (key, code) {
            ("+" | "=", _) | (_, "NumpadAdd") => Some(Zoom::In),
            ("-", _) | (_, "NumpadSubtract") => Some(Zoom::Out),
            ("0", _) | (_, "Numpad0") => Some(Zoom::Reset),
            _ => None,
        };
        if let Some(z) = z {
            return if m.alt {
                KeyResult::UiZoom(z)
            } else {
                KeyResult::TextZoom(z)
            };
        }
        if !m.alt && app_action {
            match lower.as_str() {
                "k" => return KeyResult::Configure,
                "x" => return KeyResult::Stop,
                "p" => return KeyResult::Pin,
                "d" => return KeyResult::Delete,
                "[" if m.command => return KeyResult::Back,
                "c" if m.command || m.shift => return KeyResult::Copy,
                "v" if m.command || m.shift => return KeyResult::Paste,
                _ => {}
            }
        }
        if let Some(i) = tezbar_ui_model::terminal::shortcut_index(
            code, key, m.command, m.control, m.alt, m.shift,
        ) {
            return KeyResult::Session(i);
        }
    }
    if key == "escape" || key == "Escape" {
        return KeyResult::Back;
    }
    // macOptionIsMeta was false in the old terminal. Let AppKit insert the
    // produced character (or start a dead-key composition), not ESC+base key.
    if cfg!(target_os = "macos") && m.alt && !m.command && !m.control && key.chars().count() == 1 {
        return KeyResult::PlatformText;
    }
    let modifier = 1 + u8::from(m.shift) + 2 * u8::from(m.alt) + 4 * u8::from(m.control);
    let final_char = match lower.as_str() {
        "up" | "arrowup" => Some('A'),
        "down" | "arrowdown" => Some('B'),
        "right" | "arrowright" => Some('C'),
        "left" | "arrowleft" => Some('D'),
        "home" => Some('H'),
        "end" => Some('F'),
        _ => None,
    };
    if let Some(c) = final_char {
        return KeyResult::Bytes(if modifier != 1 {
            format!("\x1b[1;{modifier}{c}")
        } else if mode.contains(TermMode::APP_CURSOR) {
            format!("\x1bO{c}")
        } else {
            format!("\x1b[{c}")
        });
    }
    let tilde = match lower.as_str() {
        "insert" => Some(2),
        "delete" => Some(3),
        "pageup" => Some(5),
        "pagedown" => Some(6),
        "f5" => Some(15),
        "f6" => Some(17),
        "f7" => Some(18),
        "f8" => Some(19),
        "f9" => Some(20),
        "f10" => Some(21),
        "f11" => Some(23),
        "f12" => Some(24),
        _ => None,
    };
    if let Some(n) = tilde {
        return KeyResult::Bytes(if modifier == 1 {
            format!("\x1b[{n}~")
        } else {
            format!("\x1b[{n};{modifier}~")
        });
    }
    let bytes = match lower.as_str() {
        "enter" => Some("\r".into()),
        "backspace" => Some("\x7f".into()),
        "tab" if m.shift => Some("\x1b[Z".into()),
        "tab" => Some("\t".into()),
        "f1" | "f2" | "f3" | "f4" => {
            let c = (b'P' + lower.as_bytes()[1] - b'1') as char;
            Some(if modifier == 1 {
                format!("\x1bO{c}")
            } else {
                format!("\x1b[1;{modifier}{c}")
            })
        }
        _ if m.control && key.len() == 1 && !m.command => match key.as_bytes()[0] {
            b'a'..=b'z' | b'A'..=b'Z' => {
                Some(((key.as_bytes()[0].to_ascii_uppercase() & 0x1f) as char).to_string())
            }
            b' ' | b'@' => Some("\0".into()),
            b'\\' => Some("\x1c".into()),
            b']' => Some("\x1d".into()),
            b'^' => Some("\x1e".into()),
            b'_' => Some("\x1f".into()),
            _ => None,
        },
        _ if m.alt && !m.command && key.chars().count() == 1 => Some(key.into()),
        _ => None,
    };
    if let Some(b) = bytes {
        KeyResult::Bytes(if m.alt { format!("\x1b{b}") } else { b })
    } else if m.command {
        KeyResult::Ignore
    } else {
        KeyResult::PlatformText
    }
}
pub fn paste(text: &str, mode: TermMode) -> String {
    // Never permit pasted ESC to close bracketed mode early.
    let text = text
        .replace('\x1b', "")
        .replace("\r\n", "\n")
        .replace('\n', "\r");
    if mode.contains(TermMode::BRACKETED_PASTE) {
        format!("\x1b[200~{text}\x1b[201~")
    } else {
        text
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseKind {
    Press(u8),
    Release(u8),
    Motion(Option<u8>),
    Wheel(bool),
}
pub fn mouse(
    kind: MouseKind,
    col: usize,
    row: usize,
    m: Modifiers,
    mode: TermMode,
) -> Option<Vec<u8>> {
    if m.shift || !mode.intersects(TermMode::MOUSE_MODE) {
        return None;
    }
    let (button, release) = match kind {
        MouseKind::Press(b) => (b, false),
        MouseKind::Release(b) => (b, true),
        MouseKind::Wheel(up) => (if up { 64 } else { 65 }, false),
        MouseKind::Motion(button) => {
            if !mode.contains(TermMode::MOUSE_MOTION)
                && !(button.is_some() && mode.contains(TermMode::MOUSE_DRAG))
            {
                return None;
            }
            (32 + button.unwrap_or(3), false)
        }
    };
    let button = button + 8 * u8::from(m.alt) + 16 * u8::from(m.control);
    if mode.contains(TermMode::SGR_MOUSE) {
        Some(
            format!(
                "\x1b[<{button};{};{}{}",
                col + 1,
                row + 1,
                if release { 'm' } else { 'M' }
            )
            .into_bytes(),
        )
    } else {
        let b = if release {
            3 + 8 * u8::from(m.alt) + 16 * u8::from(m.control)
        } else {
            button
        };
        if mode.contains(TermMode::UTF8_MOUSE) && col < 2015 && row < 2015 {
            Some(
                format!(
                    "\x1b[M{}{}{}",
                    (b + 32) as char,
                    char::from_u32(col as u32 + 33)?,
                    char::from_u32(row as u32 + 33)?
                )
                .into_bytes(),
            )
        } else if col < 223 && row < 223 {
            // Native core's current write seam is UTF-8 String. Do not misencode raw
            // legacy mouse bytes >=128 as Unicode. Parent must release WriteBytes.
            Some(vec![27, b'[', b'M', b + 32, col as u8 + 33, row as u8 + 33])
        } else {
            None
        }
    }
}
pub fn font_size(current: u8, zoom: Zoom) -> u8 {
    match zoom {
        Zoom::In => current.saturating_add(1).min(24),
        Zoom::Out => current.saturating_sub(1).max(8),
        Zoom::Reset => 12,
    }
}

/// Fractional trackpad pixels survive event boundaries. Pathological single
/// deltas are bounded to 128 rows/reports rather than creating unbounded input.
#[derive(Default)]
pub struct WheelAccumulator {
    pixels: f32,
}
impl WheelAccumulator {
    pub fn reset(&mut self) {
        self.pixels = 0.;
    }
    pub fn lines(&mut self, delta: f32, height: f32) -> i32 {
        if !delta.is_finite() || !height.is_finite() || height <= 0. {
            return 0;
        }
        self.pixels += delta.clamp(-128. * height, 128. * height);
        let rows = (self.pixels / height).trunc() as i32;
        self.pixels -= rows as f32 * height;
        rows
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum ScrollResult {
    None,
    Rows(i32),
    Bytes(Vec<u8>),
}
pub fn scroll(lines: i32, col: usize, row: usize, m: Modifiers, mode: TermMode) -> ScrollResult {
    if lines == 0 {
        return ScrollResult::None;
    }
    let lines = lines.clamp(-128, 128);
    if !m.shift && mode.intersects(TermMode::MOUSE_MODE) {
        return mouse(MouseKind::Wheel(lines > 0), col, row, m, mode)
            .map(|b| ScrollResult::Bytes(b.repeat(lines.unsigned_abs() as usize)))
            .unwrap_or(ScrollResult::None);
    }
    if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL)
        && !mode.intersects(TermMode::MOUSE_MODE)
    {
        let seq = match (lines > 0, mode.contains(TermMode::APP_CURSOR)) {
            (true, false) => b"\x1b[A",
            (false, false) => b"\x1b[B",
            (true, true) => b"\x1bOA",
            (false, true) => b"\x1bOB",
        };
        ScrollResult::Bytes(seq.repeat(lines.unsigned_abs() as usize))
    } else {
        ScrollResult::Rows(lines)
    }
}
#[derive(Default)]
pub struct MotionReporter {
    last: Option<(u64, usize, usize, Option<u8>, Modifiers, TermMode)>,
}
impl MotionReporter {
    pub fn reset(&mut self) {
        self.last = None;
    }
    pub fn report(
        &mut self,
        owner: u64,
        col: usize,
        row: usize,
        button: Option<u8>,
        modifiers: Modifiers,
        mode: TermMode,
    ) -> Option<Vec<u8>> {
        let key = (owner, col, row, button, modifiers, mode);
        let result = mouse(MouseKind::Motion(button), col, row, modifiers, mode);
        if result.is_none() {
            self.reset();
            return None;
        }
        if self.last == Some(key) {
            return None;
        }
        self.last = Some(key);
        result
    }
}
