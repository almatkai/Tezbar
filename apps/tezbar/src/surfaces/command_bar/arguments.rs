//! Declared argument controls. Choices use M3's (display title, wire value) contract.
use super::*;
impl Launcher {
    fn toggle_argument_dropdown(&mut self, index: usize, w: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = &mut self.form else { return };
        if form.args.fields[index].choices.is_empty() {
            return;
        }
        if form.dropdown.is_some_and(|(field, _)| field == index) {
            form.dropdown = None;
            form.inputs[index].focus_handle(cx).focus(w, cx);
        } else {
            let selected = form.args.fields[index]
                .choices
                .iter()
                .position(|(_, value)| value == &form.args.values[index])
                .unwrap_or(0);
            form.dropdown = Some((index, selected));
            form.args.selected = index;
            self.popover_focus.focus(w, cx);
        }
        cx.notify();
    }
    fn choose_argument_dropdown(
        &mut self,
        index: usize,
        choice: usize,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = &mut self.form else { return };
        form.args.selected = index;
        if form.args.complete(choice) {
            let value = form.args.values[index].clone();
            form.inputs[index].update(cx, |field, cx| field.set_text(&value, w, cx));
        }
        form.dropdown = None;
        form.inputs[index].focus_handle(cx).focus(w, cx);
        cx.notify();
    }
    pub(super) fn argument_dropdown_key(
        &mut self,
        e: &KeyDownEvent,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(form) = &mut self.form else {
            return false;
        };
        let key = e.keystroke.key.as_str();
        if let Some((index, choice)) = form.dropdown {
            match key {
                "up" => form.dropdown = Some((index, choice.saturating_sub(1))),
                "down" => {
                    form.dropdown = Some((
                        index,
                        (choice + 1).min(form.args.fields[index].choices.len().saturating_sub(1)),
                    ))
                }
                "enter" | "space" => {
                    if !e.is_held {
                        self.choose_argument_dropdown(index, choice, w, cx);
                    }
                }
                "escape" => {
                    form.dropdown = None;
                    form.inputs[index].focus_handle(cx).focus(w, cx);
                }
                "tab" => {
                    form.dropdown = None;
                    form.args.tab(e.keystroke.modifiers.shift);
                    form.inputs[form.args.selected]
                        .focus_handle(cx)
                        .focus(w, cx);
                }
                _ => return false,
            }
        } else {
            let selected = form.inputs.iter().enumerate().find_map(|(index, field)| {
                (form.labels[index].dropdown && field.focus_handle(cx).is_focused(w))
                    .then_some(index)
            });
            let Some(index) = selected else { return false };
            if !matches!(key, "enter" | "space" | "down" | "up") {
                return false;
            }
            if !e.is_held {
                self.toggle_argument_dropdown(index, w, cx);
            }
        }
        w.prevent_default();
        cx.stop_propagation();
        cx.notify();
        true
    }
    pub(super) fn argument_field(
        &self,
        index: usize,
        t: tokens::Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let form = self.form.as_ref().unwrap();
        let label = &form.labels[index];
        let mut line = div()
            .relative()
            .flex()
            .flex_col()
            .gap(px(4. * t.zoom))
            .child(div().text_size(px(12. * t.zoom)).child(label.title.clone()));
        if !label.dropdown {
            return line.child(form.inputs[index].clone()).into_any_element();
        }
        let selected = form.args.fields[index]
            .choices
            .iter()
            .find(|(_, value)| value == &form.args.values[index]);
        let expanded = form.dropdown.is_some_and(|(field, _)| field == index);
        line = line.child(
            controls::dropdown(
                SharedString::from(format!("argument-dropdown-{index}")),
                selected
                    .map(|(title, _)| title.as_str())
                    .unwrap_or("Choose…")
                    .to_owned(),
                expanded,
                t,
            )
            .track_focus(&form.inputs[index].focus_handle(cx))
            .aria_label(label.title.clone())
            .debug_selector(move || format!("argument-dropdown-{index}"))
            .on_click(
                cx.listener(move |this, _, w, cx| this.toggle_argument_dropdown(index, w, cx)),
            ),
        );
        if let Some((_, choice)) = form.dropdown.filter(|(field, _)| *field == index) {
            line = line.child(
                controls::popover(
                    SharedString::from(format!("argument-options-{index}")),
                    label.title.clone(),
                    t,
                )
                .role(Role::ListBox)
                .track_focus(&self.popover_focus)
                .absolute()
                .top(px(58. * t.zoom))
                .left_0()
                .right_0()
                .max_h(px(200. * t.zoom))
                .overflow_y_scroll()
                .on_mouse_down_out(cx.listener(move |this, _, w, cx| {
                    if let Some(form) = &mut this.form {
                        form.dropdown = None;
                        form.inputs[index].focus_handle(cx).focus(w, cx);
                    }
                    cx.notify();
                }))
                .children(form.args.fields[index].choices.iter().enumerate().map(
                    |(option, (title, _))| {
                        controls::button(
                            SharedString::from(format!("argument-option-{index}-{option}")),
                            title.clone(),
                            controls::ButtonTone::Ghost,
                            false,
                            t,
                        )
                        .role(Role::ListBoxOption)
                        .aria_selected(option == choice)
                        .aria_position_in_set(option + 1)
                        .aria_size_of_set(form.args.fields[index].choices.len())
                        .when(option == choice, |d| {
                            d.aria_active_descendant().bg(t.selected)
                        })
                        .w_full()
                        .on_click(cx.listener(move |this, _, w, cx| {
                            this.choose_argument_dropdown(index, option, w, cx)
                        }))
                    },
                )),
            );
        }
        line.into_any_element()
    }
}
