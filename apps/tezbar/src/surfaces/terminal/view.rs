use super::*;
use alacritty_terminal::selection::SelectionType;
use input::{KeyResult, MouseKind, Zoom};
use tezbar_ui::{
    controls::{self, ButtonTone},
    terminal_grid,
};
use tezbar_ui_model::terminal::{KeepAliveFor, SaveFor};
impl TerminalView {
    pub(super) fn act(&mut self, key: KeyResult, w: &mut Window, cx: &mut Context<Self>) {
        let configure = matches!(key, KeyResult::Configure);
        let text_zoom = matches!(key, KeyResult::TextZoom(_));
        let configuring = self.state.configure.is_some();
        self.state.handle_key(key);
        if configuring && self.state.configure.is_none() {
            self.restore_terminal_focus(cx);
        }
        if configure {
            if let Some(c) = &self.state.configure {
                self.name.update(cx, |n, cx| n.set_text(&c.name, w, cx));
            }
        }
        if text_zoom
            && self.state.configure.is_none()
            && self.state.visible
            && !self.state.disposed
        {
            self.focus.focus(w, cx);
        }
        self.pump(cx);
        self.sync_blink(cx);
        self.changed(cx);
    }
    fn pointer_down(&mut self, e: &MouseDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        if !self.state.visible || self.state.configure.is_some() {
            return;
        }
        self.motion.reset();
        self.focus.focus(w, cx);
        let Some((col, row)) = self.mouse_cell(e.position) else {
            return;
        };
        if e.button == MouseButton::Left && (e.modifiers.platform || e.modifiers.control) {
            if let Some(url) = self.state.grid.url_at(col, row) {
                self.state.emit(TerminalEffect::OpenUrl(url));
                self.changed(cx);
                return;
            }
        }
        if let Some(bytes) = input::mouse(
            MouseKind::Press(button(e.button)),
            col,
            row,
            modifiers(e.modifiers),
            self.state.grid.mode(),
        ) {
            self.state.write_pointer_bytes(bytes);
            self.pump(cx);
        } else if !e.modifiers.shift
            && self
                .state
                .grid
                .mode()
                .intersects(alacritty_terminal::term::TermMode::MOUSE_MODE)
        {
            self.state.error = Some("Mouse coordinates exceed the active protocol range.".into());
        } else if e.button == MouseButton::Left {
            self.state.grid.start_selection(
                col,
                row,
                match e.click_count {
                    2 => SelectionType::Semantic,
                    3.. => SelectionType::Lines,
                    _ => SelectionType::Simple,
                },
            );
            self.selection_drag = true;
            self.state.dirty = true;
        }
        self.changed(cx);
    }
    fn pointer_up(&mut self, e: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.selection_drag = false;
        self.motion.reset();
        if let Some((col, row)) = self.mouse_cell(e.position) {
            if let Some(b) = input::mouse(
                MouseKind::Release(button(e.button)),
                col,
                row,
                modifiers(e.modifiers),
                self.state.grid.mode(),
            ) {
                self.state.write_pointer_bytes(b);
                self.pump(cx);
            }
        }
        self.changed(cx);
    }
    fn pointer_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some((col, row)) = self.mouse_cell(e.position) else {
            return;
        };
        if self.selection_drag && e.dragging() {
            self.state.grid.extend_selection(col, row);
            self.state.dirty = true;
            self.changed(cx);
        } else if let Some(b) = self.motion.report(
            self.state.generation,
            col,
            row,
            e.pressed_button.map(button),
            modifiers(e.modifiers),
            self.state.grid.mode(),
        ) {
            if !self.state.write_pointer_bytes(b) {
                self.motion.reset();
            }
            self.pump(cx);
        }
    }
    fn wheel(&mut self, e: &ScrollEvent, w: &mut Window, cx: &mut Context<Self>) {
        let delta = f32::from(e.delta.pixel_delta(px(self.state.metrics.height)).y);
        if e.modifiers.platform || (cfg!(not(target_os = "macos")) && e.modifiers.control) {
            self.scroll_wheel.reset();
            self.wheel += delta;
            if self.wheel.abs() >= 40. {
                let z = if self.wheel > 0. { Zoom::In } else { Zoom::Out };
                self.wheel = 0.;
                self.act(
                    if e.modifiers.alt {
                        KeyResult::UiZoom(z)
                    } else {
                        KeyResult::TextZoom(z)
                    },
                    w,
                    cx,
                );
            }
        } else {
            self.wheel = 0.;
            let (col, row) = self.mouse_cell(e.position).unwrap_or((0, 0));
            let lines = self.scroll_wheel.lines(delta, self.state.metrics.height);
            match input::scroll(
                lines,
                col,
                row,
                modifiers(e.modifiers),
                self.state.grid.mode(),
            ) {
                input::ScrollResult::Bytes(b) => {
                    if self
                        .state
                        .grid
                        .mode()
                        .intersects(alacritty_terminal::term::TermMode::MOUSE_MODE)
                        && !e.modifiers.shift
                    {
                        if !self.state.write_pointer_bytes(b) {
                            self.scroll_wheel.reset();
                        }
                    } else {
                        self.state.write_scroll_bytes(b);
                    }
                    self.pump(cx);
                }
                input::ScrollResult::Rows(lines) => {
                    self.state.grid.scroll(lines);
                    self.state.dirty = true;
                    self.sync_blink(cx);
                    self.changed(cx);
                }
                input::ScrollResult::None => {}
            }
        }
        cx.stop_propagation();
        w.prevent_default();
    }
}
fn button(b: MouseButton) -> u8 {
    match b {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        _ => 2,
    }
}
impl Render for TerminalView {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.state.visible || self.state.disposed {
            return div().into_any_element();
        }
        let t = tezbar_ui::tokens::tokens(w);
        let session = self.state.active.as_ref().and_then(|id| {
            self.state
                .sessions
                .rows
                .iter()
                .find(|s| &s.session_id == id)
        });
        let directory = session
            .map(|s| tezbar_ui_model::terminal::directory_label(&s.cwd))
            .unwrap_or_else(|| "Shell".into());
        let command = session
            .and_then(|s| s.last_command.as_deref().or(s.initial_command.as_deref()))
            .unwrap_or("")
            .to_owned();
        if self.state.dirty || self.frame.is_none() {
            self.frame = Some(self.state.grid.frame());
            self.state.dirty = false;
        }
        let frame = self.frame.as_ref().unwrap().clone();
        let layout_owner = cx.entity();
        let input_owner = cx.entity();
        let grid = terminal_grid::element(
            frame,
            self.state.font_size,
            self.cursor_on,
            self.preedit.text.clone(),
            move |bounds, metrics, cx| {
                layout_owner.update(cx, |this, cx| {
                    let changed = this.geometry.is_none_or(|(b, m)| {
                        b != bounds || m.width != metrics.width || m.height != metrics.height
                    });
                    this.geometry = Some((bounds, metrics));
                    if changed {
                        let owner = cx.entity().downgrade();
                        cx.defer(move |cx| {
                            let _ = owner.update(cx, |v, cx| v.layout_changed(cx));
                        });
                    }
                });
            },
            move |bounds, w, cx| {
                let focus = input_owner.read(cx).focus.clone();
                w.handle_input(
                    &focus,
                    ElementInputHandler::new(bounds, input_owner.clone()),
                    cx,
                );
            },
        );
        let mut sheet = controls::sheet(t)
            .id("terminal-view")
            .font_family(tezbar_ui::tokens::font_family(cx, false))
            .relative()
            .track_focus(&self.focus)
            .key_context("Terminal")
            .on_key_down(cx.listener(Self::key))
            .child(
                controls::header(t)
                    .h(px(44. * t.zoom))
                    .px(px(16. * t.zoom))
                    .gap(px(2. * t.zoom))
                    .justify_between()
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .flex()
                            .items_center()
                            .gap(px(10. * t.zoom))
                            .text_size(px(14. * t.zoom))
                            .text_color(t.text2)
                            .child(div().truncate().child(directory))
                            .when(!command.is_empty(), |d| {
                                d.child(div().w(px(1.)).h(px(14.)).bg(t.hairline)).child(
                                    div()
                                        .flex_1()
                                        .truncate()
                                        .font_family(tezbar_ui::tokens::mono_family())
                                        .text_size(px(13. * t.zoom))
                                        .text_color(t.text1)
                                        .child(command),
                                )
                            }),
                    )
                    .child(
                        controls::button(
                            "term-minus",
                            "−",
                            ButtonTone::Ghost,
                            self.state.font_size <= 8,
                            t,
                        )
                        .px(px(8. * t.zoom))
                        .debug_selector(|| "terminal-zoom-out".into())
                        .on_click(cx.listener(|this, _, w, cx| {
                            this.act(KeyResult::TextZoom(Zoom::Out), w, cx)
                        })),
                    )
                    .child(
                        controls::button(
                            "term-reset",
                            format!("{}%", self.state.font_size as u16 * 100 / 12),
                            ButtonTone::Ghost,
                            false,
                            t,
                        )
                        .px(px(4. * t.zoom))
                        .min_w(px(40. * t.zoom))
                        .on_click(cx.listener(|this, _, w, cx| {
                            this.act(KeyResult::TextZoom(Zoom::Reset), w, cx)
                        })),
                    )
                    .child(
                        controls::button(
                            "term-plus",
                            "+",
                            ButtonTone::Ghost,
                            self.state.font_size >= 24,
                            t,
                        )
                        .px(px(8. * t.zoom))
                        .debug_selector(|| "terminal-zoom-in".into())
                        .on_click(cx.listener(|this, _, w, cx| {
                            this.act(KeyResult::TextZoom(Zoom::In), w, cx)
                        })),
                    )
                    .child(
                        controls::button(
                            "term-sessions",
                            format!(
                                "Sessions ({})",
                                self.state
                                    .sessions
                                    .rows
                                    .iter()
                                    .filter(|s| s.running)
                                    .count()
                            ),
                            ButtonTone::Quiet,
                            false,
                            t,
                        )
                        .ml(px(8. * t.zoom))
                        .px(px(10. * t.zoom))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.state.emit(TerminalEffect::SessionsToggle);
                            this.changed(cx);
                        })),
                    )
                    .child(
                        controls::button("term-new", "+", ButtonTone::Ghost, false, t)
                            .px(px(8. * t.zoom))
                            .aria_label("New session")
                            .debug_selector(|| "terminal-new-session".into())
                            .on_click(cx.listener(|this, _, w, cx| {
                                this.new_session(cx);
                                if this.state.visible
                                    && !this.state.disposed
                                    && this.state.configure.is_none()
                                {
                                    this.focus.focus(w, cx);
                                }
                            })),
                    ),
            )
            .child(
                div()
                    .id("terminal-cells")
                    .flex_1()
                    .min_h_0()
                    .mb(px(8. * t.zoom))
                    .p(px(12. * t.zoom))
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::pointer_down))
                    .on_mouse_down(MouseButton::Middle, cx.listener(Self::pointer_down))
                    .on_mouse_down(MouseButton::Right, cx.listener(Self::pointer_down))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::pointer_up))
                    .on_mouse_up(MouseButton::Middle, cx.listener(Self::pointer_up))
                    .on_mouse_up(MouseButton::Right, cx.listener(Self::pointer_up))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::pointer_up))
                    .on_mouse_move(cx.listener(Self::pointer_move))
                    .on_scroll(cx.listener(Self::wheel))
                    .child(grid),
            )
            .child(
                controls::footer(t)
                    .child(
                        controls::button(
                            "term-configure",
                            "Configure",
                            ButtonTone::Ghost,
                            false,
                            t,
                        )
                        .debug_selector(|| "terminal-configure-action".into())
                        .on_click(
                            cx.listener(|this, _, w, cx| this.act(KeyResult::Configure, w, cx)),
                        ),
                    )
                    .child(if let Some(error) = &self.state.error {
                        div()
                            .id("terminal-error")
                            .debug_selector(|| "terminal-error".into())
                            .role(Role::Status)
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12. * t.zoom))
                            .text_color(t.danger)
                            .child(error.clone())
                            .into_any_element()
                    } else {
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(if cfg!(target_os = "macos") {
                                "Configure ⌘K   Switch sessions ⌘1–9   Back Esc / ⌘["
                            } else {
                                "Configure Ctrl+Shift+K   Switch sessions Ctrl+1–9   Back Esc"
                            })
                            .into_any_element()
                    })
                    .when_some(self.task_indicator.clone(), |footer, indicator| {
                        footer.child(indicator)
                    }),
            );
        if let Some(c) = self.state.configure.clone() {
            let mut drawer = div()
                .id("terminal-configure")
                .debug_selector(|| "terminal-configure".into())
                .role(Role::Dialog)
                .aria_label("Terminal Session")
                .flex()
                .flex_col()
                .gap(px(12. * t.zoom))
                .w_full().max_w(px(520. * t.zoom))
                .p(px(16. * t.zoom))
                .rounded(px(18. * t.zoom))
                .bg(t.popover_solid)
                .border_1().border_color(t.hairline)
                .child("Terminal Session")
                .child("Save the session label and control how long the process stays alive after you leave.")
                .child(self.name.clone())
                .child("Save for");
            let save = [
                ("Day", SaveFor::Day),
                ("Week", SaveFor::Week),
                ("Month", SaveFor::Month),
                ("Forever", SaveFor::Forever),
            ];
            let mut options = div().flex().gap(px(4.));
            for (index, (label, value)) in save.into_iter().enumerate() {
                options = options.child(
                    controls::button(
                        ("term-save-for", index),
                        label,
                        if c.defaults.save_for == value {
                            ButtonTone::Quiet
                        } else {
                            ButtonTone::Ghost
                        },
                        false,
                        t,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(c) = &mut this.state.configure {
                            c.defaults.save_for = value.clone();
                        }
                        this.changed(cx);
                    })),
                );
            }
            drawer = drawer.child(options).child("Keep alive");
            let mut options = div().flex().gap(px(4.));
            for (index, (label, value)) in [
                ("3 hours", KeepAliveFor::ThreeHours),
                ("8 hours", KeepAliveFor::EightHours),
                ("Day", KeepAliveFor::Day),
                ("Until stop", KeepAliveFor::UntilStop),
            ]
            .into_iter()
            .enumerate()
            {
                options = options.child(
                    controls::button(
                        ("term-keep-alive", index),
                        label,
                        if c.defaults.keep_alive_for == value {
                            ButtonTone::Quiet
                        } else {
                            ButtonTone::Ghost
                        },
                        false,
                        t,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(c) = &mut this.state.configure {
                            c.defaults.keep_alive_for = value.clone();
                        }
                        this.changed(cx);
                    })),
                );
            }
            drawer = drawer.child(options).child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        controls::button(
                            "term-config-cancel",
                            "Cancel",
                            ButtonTone::Ghost,
                            false,
                            t,
                        )
                        .debug_selector(|| "terminal-config-cancel".into())
                        .on_click(cx.listener(|this, _, w, cx| this.act(KeyResult::Back, w, cx))),
                    )
                    .child(
                        controls::button(
                            "term-config-stop",
                            "Stop",
                            ButtonTone::Danger,
                            !self.state.running,
                            t,
                        )
                        .on_click(cx.listener(|this, _, w, cx| this.act(KeyResult::Stop, w, cx))),
                    )
                    .child(
                        controls::button("term-config-save", "Save", ButtonTone::Primary, false, t)
                            .debug_selector(|| "terminal-config-save".into())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.save_configure();
                                this.pump(cx);
                                this.changed(cx);
                            })),
                    ),
            );
            sheet = sheet.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_start()
                    .justify_center()
                    .px(px(32. * t.zoom))
                    .py(px(48. * t.zoom))
                    .bg(rgba(0x00000059))
                    .child(drawer),
            );
        }
        sheet.into_any_element()
    }
}
