//! Small native control constructors share visual and semantic state.
use crate::tokens::{self, Tokens};
use gpui::{prelude::*, *};
actions!(
    window_chrome,
    [
        /// A press on empty header/footer chrome; the window owner may turn it
        /// into a window move once the pointer travels.
        ArmWindowMove
    ]
);
/// Presses that no focusable control claimed (text fields focus and prevent
/// default) ask the window owner to arm a move.
fn arm_window_move(_: &MouseDownEvent, w: &mut Window, cx: &mut App) {
    if !w.default_prevented() {
        w.dispatch_action(Box::new(ArmWindowMove), cx);
    }
}
#[derive(Clone, Copy)]
pub enum ButtonTone {
    Primary,
    Quiet,
    Ghost,
    Danger,
}
pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    tone: ButtonTone,
    disabled: bool,
    t: Tokens,
) -> Stateful<Div> {
    let label = label.into();
    div()
        .id(id)
        .role(Role::Button)
        .a11y_synthetic_children(move |b| {
            if disabled {
                b.parent_node().set_disabled();
            }
        })
        .aria_label(label.clone())
        .tab_index(0)
        .flex()
        .items_center()
        .justify_center()
        .px(px(13.6 * t.zoom))
        .py(px(6.4 * t.zoom))
        .rounded(px(8. * t.zoom))
        .text_size(px(12. * t.zoom))
        .line_height(px(12. * t.zoom))
        .font_weight(FontWeight::MEDIUM)
        .text_color(if disabled {
            t.text4
        } else if matches!(tone, ButtonTone::Primary) {
            rgb(0xffffff).into()
        } else if matches!(tone, ButtonTone::Danger) {
            t.danger
        } else {
            t.text1
        })
        .bg(if matches!(tone, ButtonTone::Primary) {
            t.accent_strong
        } else if matches!(tone, ButtonTone::Quiet) {
            t.pill
        } else {
            transparent_black()
        })
        .child(label)
}
pub fn keycap(label: impl Into<SharedString>, t: Tokens) -> Div {
    div()
        .h(px(16. * t.zoom))
        .min_w(px(16. * t.zoom))
        .px(px(4. * t.zoom))
        .rounded(px(4. * t.zoom))
        .bg(t.pill_strong)
        .line_height(px(16. * t.zoom))
        .text_size(px(9.5 * t.zoom))
        .text_color(t.text3)
        .child(label.into())
}
/// Keep an existing Back action's listeners and accessibility while showing a compact chevron.
pub fn back_button(control: Stateful<Div>, t: Tokens) -> Stateful<Div> {
    control
        .aria_label("Back")
        .text_size(px(0.))
        .size(px(24. * t.zoom))
        .ml(px(-4. * t.zoom))
        .mr(px(-6. * t.zoom))
        .p_0()
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .rounded(px(7. * t.zoom))
        .border_1()
        .border_color(transparent_black())
        .bg(transparent_black())
        .cursor_pointer()
        .child(svg().data(b"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"black\" stroke-width=\"2.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"m15 18-6-6 6-6\"/></svg>" as &[u8])
            .size(px(14. * t.zoom)).text_color(t.text2))
}
pub fn checkbox(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
    t: Tokens,
) -> Stateful<Div> {
    let label = label.into();
    div()
        .id(id)
        .role(Role::CheckBox)
        .aria_label(label.clone())
        .aria_toggled(if checked {
            Toggled::True
        } else {
            Toggled::False
        })
        .tab_index(0)
        .flex()
        .items_center()
        .gap(px(8. * t.zoom))
        .text_size(px(12. * t.zoom))
        .text_color(t.text1)
        .child(
            div()
                .size(px(14. * t.zoom))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(3. * t.zoom))
                .border_1()
                .border_color(t.rim)
                .bg(if checked { t.accent_strong } else { t.field })
                .when(checked, |d| {
                    d.child(
                        svg()
                            .data(br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12" fill="none" stroke="black" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="m2.5 6 2.5 2.5 4.5-5"/></svg>"#.as_slice())
                            .size(px(12. * t.zoom))
                            .text_color(rgb(0xffffff)),
                    )
                }),
        )
        .child(label)
}
pub fn switch(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
    t: Tokens,
) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::Switch)
        .aria_label(label.into())
        .aria_toggled(if checked {
            Toggled::True
        } else {
            Toggled::False
        })
        .tab_index(0)
        .w(px(28. * t.zoom))
        .h(px(16. * t.zoom))
        .rounded(px(8. * t.zoom))
        .bg(if checked {
            t.accent_strong
        } else {
            t.pill_strong
        })
        .flex()
        .items_center()
        .when(checked, |d| d.justify_end())
        .px(px(2. * t.zoom))
        .child(
            div()
                .size(px(12. * t.zoom))
                .rounded_full()
                .bg(rgb(0xffffff)),
        )
}
pub fn sheet(t: Tokens) -> Div {
    div()
        .line_height(relative(1.5))
        .size_full()
        .flex()
        .flex_col()
        .rounded(px(tokens::SHEET_RADIUS))
        .border_1()
        .border_color(t.rim)
        .bg(t.sheet)
        .text_color(t.text1)
        .overflow_hidden()
}
pub fn header(t: Tokens) -> Div {
    div()
        .debug_selector(|| "surface-header".into())
        .flex()
        .items_center()
        .h(px(tokens::HEADER * t.zoom))
        .flex_shrink_0()
        .px(px(14. * t.zoom))
        .border_b_1()
        .border_color(t.hairline)
        .on_mouse_down(MouseButton::Left, arm_window_move)
}
pub fn footer(t: Tokens) -> Div {
    div()
        .debug_selector(|| "surface-footer".into())
        .flex()
        .items_center()
        .gap(px(8. * t.zoom))
        .h(px(tokens::FOOTER * t.zoom))
        .flex_shrink_0()
        .px(px(12. * t.zoom))
        // Content masks are rectangular: without its own radius the footer fill
        // pokes square corners past the sheet's rounded border (visible wherever the
        // OS does not mask the window shape, e.g. Windows).
        .rounded_b(px(tokens::SHEET_RADIUS - 1.))
        .bg(t.bar_float)
        .text_size(px(10.5 * t.zoom))
        .text_color(t.text3)
        // Lists scroll under the translucent footer; presses must not reach the
        // rows beneath it (scrolling still does).
        .block_mouse_except_scroll()
        .on_mouse_down(MouseButton::Left, arm_window_move)
}
pub fn popover(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    t: Tokens,
) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::Dialog)
        .aria_label(label.into())
        .rounded(px(10. * t.zoom))
        .border_1()
        .border_color(t.rim)
        .bg(t.popover)
        .p(px(4. * t.zoom))
}
pub fn segment(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    t: Tokens,
) -> Stateful<Div> {
    let label = label.into();
    div()
        .id(id)
        .role(Role::Tab)
        .aria_label(label.clone())
        .aria_selected(selected)
        .tab_index(0)
        .px(px(10. * t.zoom))
        .py(px(5. * t.zoom))
        .rounded(px(5. * t.zoom))
        .bg(if selected {
            t.pill_strong
        } else {
            transparent_black()
        })
        .child(label)
}
pub fn dropdown(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    expanded: bool,
    t: Tokens,
) -> Stateful<Div> {
    button(id, label, ButtonTone::Quiet, false, t)
        .role(Role::ComboBox)
        .aria_expanded(expanded)
}
/// Own this alongside the popover. Closing restores the originating focus; no focus leaks to a hidden view.
pub struct FocusScope {
    restore: FocusHandle,
    pub focus: FocusHandle,
}
impl FocusScope {
    pub fn new(restore: FocusHandle, cx: &mut App) -> Self {
        Self {
            restore,
            focus: cx.focus_handle(),
        }
    }
    pub fn open(&self, w: &mut Window, cx: &mut App) {
        w.focus(&self.focus, cx);
    }
    pub fn close(self, w: &mut Window, cx: &mut App) {
        w.focus(&self.restore, cx);
    }
}
/// Activation follows the same callback for pointer, keyboard and AccessKit. Held keys do not repeat.
pub fn action_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    tone: ButtonTone,
    disabled: bool,
    t: Tokens,
    activate: impl Fn(&mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    button(id, label, tone, disabled, t)
        .when(disabled, |d| d.tab_index(-1).aria_description("Disabled"))
        .border_1()
        .border_color(transparent_black())
        .focus(move |d| d.border_color(t.accent_strong))
        .on_click(move |_, w, cx| {
            if !disabled {
                activate(w, cx);
            }
        })
}
impl FocusScope {
    pub fn cycle(handles: &[FocusHandle], backwards: bool, w: &mut Window, cx: &mut App) {
        if handles.is_empty() {
            return;
        }
        let current = handles.iter().position(|f| f.is_focused(w));
        let next = match current {
            Some(i) if backwards => (i + handles.len() - 1) % handles.len(),
            Some(i) => (i + 1) % handles.len(),
            None => 0,
        };
        w.focus(&handles[next], cx);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;
    /// A focusable, clickable row with the surface footer floated over it, as
    /// in launcher lists that scroll under the translucent footer.
    struct ChromeTest {
        row_clicks: std::rc::Rc<std::cell::Cell<usize>>,
        arms: std::rc::Rc<std::cell::Cell<usize>>,
        row_focus: FocusHandle,
        input_focus: FocusHandle,
    }
    impl Render for ChromeTest {
        fn render(&mut self, w: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let t = crate::tokens::tokens(w);
            let (clicks, arms) = (self.row_clicks.clone(), self.arms.clone());
            sheet(t)
                .relative()
                .on_action(move |_: &ArmWindowMove, _, _| arms.set(arms.get() + 1))
                .child(
                    header(t).child(
                        div()
                            .debug_selector(|| "chrome-input".into())
                            .flex_1()
                            .h_full()
                            .track_focus(&self.input_focus),
                    ),
                )
                .child(
                    div()
                        .id("row")
                        .debug_selector(|| "chrome-row".into())
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .h(px(80.))
                        .track_focus(&self.row_focus)
                        .on_click(move |_, _, _| clicks.set(clicks.get() + 1)),
                )
                .child(
                    footer(t)
                        .debug_selector(|| "chrome-footer".into())
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0(),
                )
                .size(px(400.))
        }
    }
    #[gpui::test]
    fn footer_press_arms_a_move_and_never_reaches_rows_beneath(cx: &mut gpui::TestAppContext) {
        let (clicks, arms) = Default::default();
        let (_view, cx) = cx.add_window_view(|_, cx| ChromeTest {
            row_clicks: std::rc::Rc::clone(&clicks),
            arms: std::rc::Rc::clone(&arms),
            row_focus: cx.focus_handle(),
            input_focus: cx.focus_handle(),
        });
        cx.run_until_parked();
        let row: std::rc::Rc<std::cell::Cell<usize>> = clicks;
        let arms: std::rc::Rc<std::cell::Cell<usize>> = arms;
        // The row is reachable above the footer.
        let row_bounds = cx.debug_bounds("chrome-row").unwrap();
        cx.simulate_click(row_bounds.origin + point(px(20.), px(10.)), Modifiers::default());
        cx.run_until_parked();
        assert_eq!(row.get(), 1);
        assert_eq!(arms.get(), 0);
        // A press on the footer arms a window move and does not click the row.
        let footer = cx.debug_bounds("chrome-footer").unwrap();
        cx.simulate_click(footer.center(), Modifiers::default());
        cx.run_until_parked();
        assert_eq!(row.get(), 1);
        assert_eq!(arms.get(), 1);
        // Header chrome arms too, but a focusable control inside it (the text field) does not.
        let input = cx.debug_bounds("chrome-input").unwrap();
        cx.simulate_click(input.center(), Modifiers::default());
        cx.run_until_parked();
        assert_eq!(arms.get(), 1);
        let header = cx.debug_bounds("surface-header").unwrap();
        cx.simulate_click(header.origin + point(px(5.), px(10.)), Modifiers::default());
        cx.run_until_parked();
        assert_eq!(arms.get(), 2);
    }
    struct ButtonTest {
        hits: std::rc::Rc<std::cell::Cell<usize>>,
        focus: FocusHandle,
        disabled: bool,
    }
    impl Render for ButtonTest {
        fn render(&mut self, w: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let hits = self.hits.clone();
            action_button(
                "action",
                "Run",
                ButtonTone::Quiet,
                self.disabled,
                crate::tokens::tokens(w),
                move |_, _| hits.set(hits.get() + 1),
            )
            .track_focus(&self.focus)
        }
    }
    #[gpui::test]
    fn keyboard_activation_uses_one_guarded_callback(cx: &mut gpui::TestAppContext) {
        let hits = std::rc::Rc::new(std::cell::Cell::new(0));
        let (view, cx) = cx.add_window_view(|_, cx| ButtonTest {
            hits: hits.clone(),
            focus: cx.focus_handle(),
            disabled: false,
        });
        cx.update(|w, cx| {
            let focus = view.read(cx).focus.clone();
            w.focus(&focus, cx);
        });
        cx.run_until_parked();
        for (index, key) in ["enter", "space"].into_iter().enumerate() {
            let keystroke = Keystroke::parse(key).unwrap();
            cx.simulate_event(KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            });
            assert_eq!(hits.get(), index);
            for _ in 0..3 {
                cx.simulate_event(KeyDownEvent {
                    keystroke: keystroke.clone(),
                    is_held: true,
                    prefer_character_input: false,
                });
            }
            assert_eq!(hits.get(), index);
            cx.simulate_event(KeyUpEvent { keystroke });
            assert_eq!(hits.get(), index + 1);
        }
        view.update(cx, |v, cx| {
            v.disabled = true;
            cx.notify();
        });
        cx.run_until_parked();
        let keystroke = Keystroke::parse("enter").unwrap();
        cx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        cx.simulate_event(KeyUpEvent { keystroke });
        assert_eq!(hits.get(), 2);
    }
    #[gpui::test]
    fn focus_scope_cycles_and_restores(cx: &mut gpui::TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, cx| ButtonTest {
            hits: Default::default(),
            focus: cx.focus_handle(),
            disabled: false,
        });
        cx.update(|w, cx| {
            let restore = cx.focus_handle();
            let a = cx.focus_handle();
            let b = cx.focus_handle();
            let scope = FocusScope::new(restore.clone(), cx);
            w.focus(&a, cx);
            FocusScope::cycle(&[a.clone(), b.clone()], false, w, cx);
            assert!(b.is_focused(w));
            FocusScope::cycle(&[a.clone(), b.clone()], true, w, cx);
            assert!(a.is_focused(w));
            scope.close(w, cx);
            assert!(restore.is_focused(w));
        });
    }
}

/// The product pin marker from ClipboardView, shared by native rows.
pub fn pin(t: Tokens) -> impl IntoElement {
    div().id("pin-marker").role(Role::Image).aria_label("Pinned")
        .child(gpui::svg().data(br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><path fill="white" d="M5 .5 6 3l2.6.4-1.9 1.8.5 2.6L5 6.6 2.8 7.8l.5-2.6L1.4 3.4 4 3 5 .5Z"/></svg>"#.as_slice()).size(px(10. * t.zoom)).text_color(t.text3))
}
/// Status text whose letters shimmer through blue, white and violet, like the React
/// "Generating…" label. Callers show plain text instead when reduce motion is on.
pub fn shimmer_text(
    id: impl Into<ElementId>,
    label: &'static str,
    light: bool,
) -> impl IntoElement {
    let palette: &[u32] = if light {
        &[0x1d4ed8, 0x0369a1, 0x6d28d9, 0x9d174d]
    } else {
        &[0x93c5fd, 0xa5f3fc, 0xffffff, 0xd8b4fe, 0xf9a8d4]
    };
    let count = label.chars().count() as f32;
    StyledText::new(label).with_animation(
        id,
        Animation::new(std::time::Duration::from_millis(2200))
            .repeat()
            .with_max_fps(30.0),
        move |text, progress| {
            text.with_highlights(label.char_indices().enumerate().map(|(i, (start, ch))| {
                let stop =
                    (progress - 0.5 * i as f32 / count).rem_euclid(1.0) * palette.len() as f32;
                let index = stop as usize % palette.len();
                let next = rgb(palette[(index + 1) % palette.len()]).alpha(stop.fract());
                let color = rgb(palette[index]).blend(next);
                (
                    start..start + ch.len_utf8(),
                    HighlightStyle {
                        color: Some(color.into()),
                        ..Default::default()
                    },
                )
            }))
        },
    )
}
