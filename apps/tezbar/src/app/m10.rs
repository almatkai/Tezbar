//! M10 typed host effects. Invoked only for the currently mounted source.
use super::*;
use crate::surfaces::{
    command_bar::TerminalSource,
    terminal::{TerminalEffect, input::Zoom},
};
impl Owner {
    pub(super) fn m10_effect(
        &mut self,
        source: TerminalSource,
        effect: TerminalEffect,
        cx: &mut Context<Self>,
    ) {
        let owned = self
            .launcher
            .update(cx, |v, _, cx| v.accepts_terminal_source(source, cx))
            .unwrap_or(false);
        if !owned || self.lifecycle.quitting {
            return;
        }
        match effect {
            TerminalEffect::Back { .. } => {
                let _ = self
                    .launcher
                    .update(cx, |v, w, cx| v.terminal_back(source, w, cx));
            }
            TerminalEffect::OpenSettings => self.settings_intent(None, cx),
            TerminalEffect::SessionsToggle => {
                if self.panel_visible {
                    let _ = self.relay.send(Event::PanelHide);
                } else {
                    let rows = self
                        .launcher
                        .update(cx, |v, _, cx| v.terminal_panel_rows(cx))
                        .unwrap_or_default();
                    self.panel(rows, cx);
                }
            }
            TerminalEffect::Copy(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
            TerminalEffect::ReadPaste { generation } => {
                let read = cx.read_from_clipboard_async();
                self.tasks.push(cx.spawn(async move |this, cx| {
                    if let Ok(Some(item)) = read.await {
                        if let Some(text) = item.text() {
                            let _ = this.update(cx, |this, cx| {
                                let _ = this.launcher.update(cx, |v, _, cx| {
                                    v.terminal_paste(source, generation, &text, cx)
                                });
                            });
                        }
                    }
                }));
            }
            TerminalEffect::OpenUrl(url) => {
                if allowed_url(&url) {
                    cx.open_url(&url);
                }
            }
            TerminalEffect::Zoom(z) => {
                let _ = self.relay.send(Event::Zoom(match z {
                    Zoom::In => 0.1,
                    Zoom::Out => -0.1,
                    Zoom::Reset => 0.,
                }));
            }
            TerminalEffect::Pins(pins) => {
                let _ = self
                    .launcher
                    .update(cx, |v, _, cx| v.terminal_pins(pins, cx));
            }
        }
    }
}
fn allowed_url(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && !url.chars().any(char::is_control)
}
#[cfg(test)]
mod tests {
    #[test]
    fn host_url_scheme_is_finite() {
        assert!(super::allowed_url("https://example.test/a"));
        for s in ["file:///private", "javascript:alert(1)", "https://a\n"] {
            assert!(!super::allowed_url(s));
        }
    }
}
