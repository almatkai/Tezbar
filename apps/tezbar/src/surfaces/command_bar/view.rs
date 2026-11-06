use super::*;
use presentation::RowAction;
impl Launcher {
    fn selected(&self) -> Option<Row> {
        self.model
            .session
            .rows
            .get(self.model.session.selected)
            .map(|r| r.as_ref().clone())
    }
    pub(super) fn click(&mut self, generation: u64, id: String, cx: &mut Context<Self>) {
        self.dispatch(Input::Click { generation, id }, cx);
    }
    fn toggle_pin(&mut self, row: Row, cx: &mut Context<Self>) {
        if self.state.pins.iter().any(|p| p.id == row.id) {
            self.state.pins.retain(|p| p.id != row.id);
            ui_state::normalize_pins(&mut self.state.pins);
            self.persist(cx);
            self.message("Unpinned".into(), 2500, cx);
        } else if !row.pinnable() || row.ephemeral {
            self.message("Temporary results can’t be pinned".into(), 2500, cx);
        } else {
            self.pin_picker = Some((row, self.state.pins.len().min(11)));
            cx.notify();
        }
    }
    fn pin_at(&mut self, row: Row, index: usize, cx: &mut Context<Self>) {
        ui_state::insert_pin(&mut self.state.pins, &row, Some(index));
        self.pin_picker = None;
        self.persist(cx);
        self.message("Pinned".into(), 2500, cx);
    }
    fn row_action(&mut self, row: Row, action: RowAction, cx: &mut Context<Self>) {
        self.menu = None;
        match action {
            RowAction::Open => self.click(self.model.session.generation, row.id, cx),
            RowAction::Pin => self.toggle_pin(row, cx),
            RowAction::QuickLook => {
                let paths = launcher::quick_look_paths(
                    &self.model.session.rows,
                    self.model.session.selected,
                )
                .into_iter()
                .map(str::to_owned)
                .collect();
                self.request_notice(Request::QuickLook(paths), cx);
            }
            RowAction::Reveal => {
                if let Action::OpenFile { path } | Action::OpenWithApp { path, .. } = &row.action {
                    let _ = self.relay.send(Event::Reveal(path.clone()));
                }
            }
            RowAction::Copy | RowAction::Paste => {
                let text = match row.action {
                    Action::OpenFile { path } | Action::OpenWithApp { path, .. } => path,
                    Action::CopyText { text } | Action::CopyAndPasteText { text } => text,
                    _ => return,
                };
                let action = if matches!(action, RowAction::Paste) {
                    Action::CopyAndPasteText { text }
                } else {
                    Action::CopyText { text }
                };
                self.request_notice(
                    Request::Activate {
                        action: json!(action),
                        query: self.model.input.clone(),
                        rank: self.model.session.selected + 1,
                        result_id: row.id,
                    },
                    cx,
                );
            }
        }
    }
    fn submit_form(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.form.as_ref() else {
            return;
        };
        let values = match form.args.submit() {
            Ok(v) => v,
            Err(e) => {
                self.error = Some(e);
                cx.notify();
                return;
            }
        };
        let Some(mut a) = self.form_activation.take() else {
            return;
        };
        if form.generation != self.model.session.generation || form.ticket != a.ticket {
            return;
        }
        if let Action::RunExtensionCommand { extra, .. } = &mut a.row.action {
            extra.insert("argumentValues".into(), json!(values));
        }
        self.form = None;
        self.execute_activation(a, cx);
    }
    fn complete_path(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.selected() else { return };
        if let Some(path) = self.paths.get(&row.id).cloned() {
            if path["kind"] == "file" || path["kind"] == "application" {
                self.click(self.model.session.generation, row.id, cx);
                return;
            }
            if let Some(p) = path["path"].as_str() {
                let adapter = self.adapter.clone();
                let req = Request::DirectoryVisit(p.into());
                self.tasks.push(cx.spawn(async move |_, cx| {
                    let _ =
                        gpui_tokio::Tokio::spawn(cx, async move { adapter.send(req).await }).await;
                }));
            }
            if let Some(value) = path["value"].as_str() {
                self.edit(value, w, cx);
            }
        }
    }
    fn return_home(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.prefix = None;
        self.model_menu = false;
        self.terminal_settings = false;
        self.navigation = None;
        self.form = None;
        self.form_activation = None;
        self.menu = None;
        self.pin_picker = None;
        self.history = false;
        self.answer_active = false;
        self.model.terminal_mode = false;
        self.model.session.reset_transport();
        self.model.input.clear();
        self.input.update(cx, |i, cx| i.clear_silently(w, cx));
        self.dispatch(Input::Show, cx);
        self.input.focus_handle(cx).focus(w, cx);
    }
    fn enter(&mut self, repeat: bool, secondary: bool, w: &mut Window, cx: &mut Context<Self>) {
        if repeat || self.model_saving {
            return;
        }
        if self.form.is_some() {
            self.submit_form(cx);
            return;
        }
        if let Some((row, slot)) = self.pin_picker.take() {
            self.pin_at(row, slot, cx);
            return;
        }
        if let Some((row, selected)) = self.menu.take() {
            if let Some((_, action)) = presentation::menu(&row).get(selected) {
                self.row_action(row, action.clone(), cx);
            }
            return;
        }
        let mode = launcher::input_mode(&self.model.input, self.model.terminal_mode);
        if matches!(mode.mode, Mode::Ai | Mode::Terminal) {
            if mode.mode == Mode::Terminal && self.model.list_selection_active {
                let id = self.selected().and_then(|row| {
                    if let Action::InvokeCommand { payload, .. } = row.action {
                        payload["sessionId"].as_str().map(str::to_owned)
                    } else {
                        None
                    }
                });
                self.dispatch(Input::SelectTerminalSession(id), cx);
            }
            self.dispatch(Input::Enter { repeat }, cx);
            return;
        }
        if mode.mode == Mode::Basic && secondary {
            let deep = format!("!{}", self.model.input.trim());
            self.edit(&deep, w, cx);
            return;
        }
        if mode.mode == Mode::Path && secondary {
            let path = self
                .paths
                .get(&self.selected().map(|r| r.id).unwrap_or_default())
                .and_then(|v| v["path"].as_str())
                .unwrap_or(&self.model.input)
                .to_owned();
            self.request_notice(
                Request::Activate {
                    action: json!(Action::OpenFile { path }),
                    query: self.model.input.clone(),
                    rank: 1,
                    result_id: format!("path-direct:{}", self.model.input),
                },
                cx,
            );
            return;
        }
        if !self.model.session.rows.is_empty() || self.model.session.pending() {
            if mode.mode == Mode::Deep && !self.deep_navigation {
                self.deep_navigation = true;
                cx.notify();
                return;
            }
            self.dispatch(Input::Enter { repeat }, cx);
            return;
        }
        if mode.mode == Mode::Path {
            self.request_notice(
                Request::Activate {
                    action: json!(Action::OpenFile {
                        path: self.model.input.clone()
                    }),
                    query: self.model.input.clone(),
                    rank: 1,
                    result_id: format!("path-direct:{}", self.model.input),
                },
                cx,
            );
            return;
        }
        if mode.mode == Mode::Applications
            || mode.mode == Mode::Deep
            || self.model.input.trim().is_empty()
        {
            return;
        }
        self.answer_text.clear();
        self.answer_active = true;
        self.answer_busy = true;
        self.update_answer(false, cx);
        let generation = self.model.session.generation;
        let request = Request::Query(self.model.input.clone());
        let adapter = self.adapter.clone();
        // CoreEvents subscription was installed at construction, before this stream request.
        self.tasks.push(cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move { adapter.send(request).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| {
                if generation == this.model.session.generation && this.visible {
                    match result {
                        Ok(v) if v["type"] == "extension" => {
                            this.answer_active = false;
                            let s = if v["name"] == "providers" {
                                Surface::Settings("ai".into())
                            } else {
                                Surface::Extensions
                            };
                            this.navigate(s, Payload::None, cx);
                        }
                        Ok(v) if v["type"] != "answer" && v["type"] != "ai" => {
                            this.answer_busy = false;
                            this.answer_active = false;
                            cx.notify();
                        }
                        Ok(_) => {}
                        Err(e) => {
                            this.answer_busy = false;
                            this.error = Some(e);
                            cx.notify();
                        }
                    }
                }
            });
        }));
        let _ = w;
    }
    pub(super) fn key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        let key = e.keystroke.key.as_str();
        let primary = if cfg!(target_os = "macos") {
            e.keystroke.modifiers.platform
        } else {
            e.keystroke.modifiers.control
        };
        let alt = e.keystroke.modifiers.alt;
        let shift = e.keystroke.modifiers.shift;
        if self.input.read(cx).marked_range.is_some()
            || self
                .form
                .as_ref()
                .is_some_and(|f| f.inputs.iter().any(|i| i.read(cx).marked_range.is_some()))
        {
            if key == "escape" {
                self.input.update(cx, |i, cx| i.cancel_composition(w, cx));
                w.prevent_default();
                cx.stop_propagation();
            }
            return;
        }
        if key == "escape" && primary {
            let _ = self.relay.send(Event::Hide);
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if matches!(key, "enter" | "space")
            && !self.input.focus_handle(cx).is_focused(w)
            && self.form.is_none()
            && self.menu.is_none()
            && self.pin_picker.is_none()
            && !self.model_menu
        {
            return;
        }
        if primary && key == "," {
            self.navigate(
                Surface::Settings("ai".into()),
                Payload::Settings { tab: "ai".into() },
                cx,
            );
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if primary && key == "n" {
            let surface = self
                .navigation
                .as_ref()
                .map(|n| n.surface.clone())
                .unwrap_or(Surface::Command);
            let item = tezbar_ui_model::router::new_item(&surface);
            let destination = match item {
                tezbar_ui_model::router::NewItem::Chat => Surface::AiChat,
                tezbar_ui_model::router::NewItem::Snippet => Surface::Snippets,
                tezbar_ui_model::router::NewItem::Vault => Surface::Vault,
                _ => Surface::Notes,
            };
            self.navigate(destination, Payload::New(item), cx);
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if primary
            && key == "q"
            && self.navigation.as_ref().is_some_and(|n| {
                matches!(
                    n.surface,
                    Surface::AiChat | Surface::Terminal | Surface::ExtensionRuntime
                )
            })
        {
            self.return_home(w, cx);
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if self.navigation.is_some() {
            if key == "escape" {
                self.return_home(w, cx);
                w.prevent_default();
                cx.stop_propagation();
            }
            return;
        }
        if primary
            && shift
            && key == "s"
            && launcher::input_mode(&self.model.input, false).mode == Mode::Ai
        {
            self.navigate(
                Surface::AiChat,
                Payload::Ai {
                    prompt: None,
                    cwd: None,
                    resume: None,
                    capture: true,
                },
                cx,
            );
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if key == "escape" {
            if let Some((_, _)) = self.menu.take() {
            } else if self.pin_picker.take().is_some() {
            } else if let Some(form) = &mut self.form {
                let index = form.args.selected;
                let field = &form.inputs[index];
                if !form.args.values[index].is_empty() {
                    form.args.values[index].clear();
                    field.update(cx, |i, cx| i.clear_silently(w, cx));
                } else {
                    if let Some(a) = self.form_activation.take() {
                        self.model.session.activation_finished(a.generation);
                    }
                    self.form = None;
                    self.input.focus_handle(cx).focus(w, cx);
                }
            } else if self.model_menu {
                self.model_menu = false;
            } else if self.terminal_settings {
                self.terminal_settings = false;
            } else if self.answer_active {
                self.answer_active = false;
            } else if self.deep_navigation {
                self.deep_navigation = false;
            } else {
                let mode = launcher::input_mode(&self.model.input, self.model.terminal_mode);
                match mode.mode {
                    Mode::Basic if self.model.input.is_empty() => {
                        let _ = self.relay.send(Event::Hide);
                    }
                    Mode::Deep => {
                        let text = if mode.query.is_empty() { "" } else { "!" };
                        self.edit(text, w, cx);
                    }
                    Mode::Applications => {
                        let text = if self.model.input.trim_start_matches('`').is_empty() {
                            ""
                        } else {
                            "`"
                        };
                        self.edit(text, w, cx);
                    }
                    Mode::Ai => {
                        let text = if mode.ai_task.is_empty() { "" } else { " " };
                        self.edit(text, w, cx);
                    }
                    Mode::Terminal => self.return_home(w, cx),
                    _ => self.edit("", w, cx),
                }
            }
            cx.notify();
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if self.model_menu {
            match key {
                "up" => self.model_choice = self.model_choice.saturating_sub(1),
                "down" => {
                    self.model_choice =
                        (self.model_choice + 1).min(self.model_choices.len().saturating_sub(1))
                }
                "enter" => {
                    if !e.is_held {
                        self.select_model(self.model_choice, cx);
                    }
                }
                _ => {}
            }
            cx.notify();
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if self.form.is_some() {
            if key == "tab" {
                let form = self.form.as_mut().unwrap();
                form.args.tab(shift);
                form.inputs[form.args.selected]
                    .focus_handle(cx)
                    .focus(w, cx);
                w.prevent_default();
                cx.stop_propagation();
            } else if key == "enter" {
                self.enter(e.is_held, primary, w, cx);
                w.prevent_default();
                cx.stop_propagation();
            }
            return;
        }
        if let Some((row, selected)) = &mut self.menu {
            match key {
                "up" => *selected = selected.saturating_sub(1),
                "down" => {
                    *selected = (*selected + 1).min(presentation::menu(row).len().saturating_sub(1))
                }
                "enter" => self.enter(e.is_held, false, w, cx),
                _ => {}
            }
            cx.notify();
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if let Some((_, slot)) = &mut self.pin_picker {
            match key {
                "left" | "up" => *slot = slot.saturating_sub(1),
                "right" | "down" => *slot = (*slot + 1).min(11),
                "enter" => self.enter(e.is_held, false, w, cx),
                _ => {}
            }
            cx.notify();
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if alt && !primary && !e.is_held {
            let code = match key {
                "-" => "Minus",
                "=" => "Equal",
                _ => "",
            };
            if let Some(slot) = ui_state::pin_slot(code, key) {
                if let Some(pin) = self.state.pins.get(slot - 1) {
                    self.click(self.model.session.generation, pin.id.clone(), cx);
                    w.prevent_default();
                    cx.stop_propagation();
                    return;
                }
            }
        }
        if self.model.terminal_mode && primary && !e.is_held {
            match key {
                "k" => {
                    self.terminal_draft = self.state.terminal_defaults.clone();
                    self.terminal_settings = true;
                }
                "p" => self.terminal_pin(cx),
                "x" => self.terminal_action(false, cx),
                "d" => self.terminal_action(true, cx),
                _ => {}
            }
            if matches!(key, "k" | "p" | "x" | "d") {
                cx.notify();
                w.prevent_default();
                cx.stop_propagation();
                return;
            }
        }
        if primary && key == "p" && !e.is_held {
            if let Some(row) = self.selected() {
                self.toggle_pin(row, cx);
            }
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if primary && key == "k" {
            if let Some(row) = self.selected() {
                self.menu = Some((row, 0));
            }
            cx.notify();
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if primary && key == "h" {
            self.navigate(Surface::Clipboard, Payload::None, cx);
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if primary
            && key == "c"
            && self.input.update(cx, |i, cx| {
                i.selected_text_range(false, w, cx)
                    .is_none_or(|s| s.range.is_empty())
            })
        {
            if let Some(Row {
                action: Action::OpenVaultItem { item_id },
                ..
            }) = self.selected()
            {
                self.vault_copy(item_id, cx);
                w.prevent_default();
                cx.stop_propagation();
                return;
            }
        }
        let mode = launcher::input_mode(&self.model.input, self.model.terminal_mode).mode;
        if key == "backspace" && matches!(self.model.input.as_str(), "!" | "`" | ">") {
            self.return_home(w, cx);
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if key == "tab" && mode == Mode::Path {
            self.complete_path(w, cx);
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if key == "space"
            && !primary
            && !alt
            && !shift
            && self
                .selected()
                .as_ref()
                .and_then(launcher::quick_look_path)
                .is_some()
            && (self.deep_navigation || self.rows.len() == 1)
        {
            if let Some(row) = self.selected() {
                self.row_action(row, RowAction::QuickLook, cx);
            }
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if key == "up"
            && self.model.input.is_empty()
            && !self.state.launcher_query_history.is_empty()
        {
            let q = self.state.launcher_query_history[self
                .history_index
                .min(self.state.launcher_query_history.len() - 1)]
            .clone();
            self.history_index += 1;
            self.edit(&q, w, cx);
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if matches!(key, "up" | "down")
            || mode == Mode::Applications && matches!(key, "left" | "right")
        {
            let delta = match key {
                "up" if mode == Mode::Applications => -7,
                "down" if mode == Mode::Applications => 7,
                "up" | "left" => -1,
                _ => 1,
            };
            self.deep_navigation = true;
            self.dispatch(Input::Move(delta), cx);
            tezbar_ui::virtual_list::reveal(
                &self.scroll,
                self.model.session.selected,
                if mode == Mode::Applications { 7 } else { 1 },
            );
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if key == "enter" {
            self.enter(e.is_held, primary, w, cx);
            w.prevent_default();
            cx.stop_propagation();
        }
    }
    pub(super) fn row_element(
        &self,
        index: usize,
        grid: bool,
        t: tokens::Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let row = self.rows[index].clone();
        let id = row.id.clone();
        let generation = self.model.session.displayed.unwrap_or(0);
        let title = row.title.clone();
        let image = self.icons.get(&id).cloned();
        let selected = index == self.model.session.selected;
        let pin = self.state.pins.iter().any(|p| p.id == id);
        tezbar_ui::virtual_list::row(
            id.clone().into(),
            title.clone().into(),
            index,
            self.rows.len(),
            selected,
            t,
        )
        .when(grid, |d| {
            d.h(px(96. * t.zoom)).flex_col().justify_center().py(px(8.))
        })
        .gap(px(10.))
        .debug_selector(move || format!("result-{index}"))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.click(generation, id.clone(), cx);
        }))
        .on_mouse_move(cx.listener(move |this, e: &MouseMoveEvent, _, cx| {
            if this.pointer == Some(e.position) {
                return;
            }
            this.pointer = Some(e.position);
            if generation == this.model.session.generation && !this.model.session.pending() {
                let delta = index as isize - this.model.session.selected as isize;
                this.dispatch(Input::Move(delta), cx);
                this.deep_navigation = true;
            }
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, _, _, cx| {
                if !this.model.session.pending() {
                    this.menu = Some((row.as_ref().clone(), 0));
                    cx.notify();
                }
            }),
        )
        .child(if let Some(image) = image {
            img(image)
                .size(px(if grid { 40. } else { 20. } * t.zoom))
                .into_any_element()
        } else {
            div()
                .size(px(if grid { 40. } else { 20. } * t.zoom))
                .text_color(if selected {
                    rgb(0xffffff).into()
                } else {
                    t.text3
                })
                .child(match self.rows[index].category.as_str() {
                    "applications" => "▦",
                    "files" | "file" => "▤",
                    "knowledge" => "⌕",
                    "extensions" => "◇",
                    "vault" => "⌑",
                    "calculator" => "=",
                    "color-converter" => "◐",
                    _ => "⌘",
                })
                .into_any_element()
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .max_w(relative(0.7))
                .text_size(px(13. * t.zoom))
                .overflow_hidden()
                .child(title),
        )
        .when(!grid, |d| {
            d.child(
                div()
                    .text_size(px(11. * t.zoom))
                    .text_color(if selected {
                        rgb(0xffffff).into()
                    } else {
                        t.text3
                    })
                    .child(self.rows[index].subtitle.clone()),
            )
        })
        .when(pin, |d| d.child(div().text_size(px(11.)).child("⌖")))
    }
}
impl Render for Launcher {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        w.set_rem_size(px(16. * self.zoom));
        let t = tokens::tokens(w);
        let mode = launcher::input_mode(&self.model.input, self.model.terminal_mode).mode;
        let grid = mode == Mode::Applications;
        let list = if grid {
            tezbar_ui::virtual_list::grid(
                "applications",
                self.rows.iter().map(|r| r.id.clone().into()).collect(),
                7,
                cx,
                move |this, index, _, cx| this.row_element(index, true, t, cx).into_any_element(),
            )
            .track_scroll(&self.scroll)
            .size_full()
            .into_any_element()
        } else {
            uniform_list(
                "launcher-results",
                self.rows.len(),
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|index| this.row_element(index, false, t, cx))
                        .collect::<Vec<_>>()
                }),
            )
            .track_scroll(&self.scroll)
            .size_full()
            .into_any_element()
        };
        let content = if let Some(n) = &self.navigation {
            div()
                .flex_1()
                .p(px(16.))
                .child(format!("{:?}", n.surface))
                .child(
                    controls::button(
                        "back-command",
                        "Back to commands",
                        controls::ButtonTone::Quiet,
                        false,
                        t,
                    )
                    .on_click(cx.listener(|this, _, w, cx| this.return_home(w, cx))),
                )
                .into_any_element()
        } else if self.terminal_settings {
            let mut body = div()
                .flex_1()
                .p(px(16.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .child("Terminal settings")
                .child("Save sessions for");
            for value in ["day", "week", "month", "forever"] {
                body = body.child(
                    controls::button(
                        SharedString::from(format!("save-for-{value}")),
                        value,
                        if self.terminal_draft.save_for == value {
                            controls::ButtonTone::Primary
                        } else {
                            controls::ButtonTone::Quiet
                        },
                        false,
                        t,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.terminal_draft.save_for = value.into();
                        cx.notify();
                    })),
                );
            }
            body = body.child("Keep running for");
            for value in ["3h", "8h", "day", "until-stop"] {
                body = body.child(
                    controls::button(
                        SharedString::from(format!("keep-alive-{value}")),
                        value,
                        if self.terminal_draft.keep_alive_for == value {
                            controls::ButtonTone::Primary
                        } else {
                            controls::ButtonTone::Quiet
                        },
                        false,
                        t,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.terminal_draft.keep_alive_for = value.into();
                        cx.notify();
                    })),
                );
            }
            body.child(
                controls::button(
                    "save-terminal-settings",
                    "Save",
                    controls::ButtonTone::Primary,
                    false,
                    t,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.state.terminal_defaults = this.terminal_draft.clone();
                    this.terminal_settings = false;
                    this.persist(cx);
                    this.message("Terminal defaults saved".into(), 2500, cx);
                })),
            )
            .into_any_element()
        } else if let Some(form) = &self.form {
            let mut body = div()
                .flex_1()
                .p(px(16.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(form.row.title.clone());
            for (index, (field, label)) in form.inputs.iter().zip(&form.labels).enumerate() {
                let mut line = div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(div().text_size(px(12.)).child(label.title.clone()))
                    .child(field.clone());
                for (choice_index, (_, title)) in form.args.fields[index].choices.iter().enumerate()
                {
                    line = line.child(
                        controls::button(
                            SharedString::from(format!("choice-{index}-{choice_index}")),
                            title.clone(),
                            controls::ButtonTone::Quiet,
                            false,
                            t,
                        )
                        .on_click(cx.listener(move |this, _, w, cx| {
                            if let Some(form) = &mut this.form {
                                form.args.selected = index;
                                if form.args.complete(choice_index) {
                                    let value = form.args.values[index].clone();
                                    form.inputs[index]
                                        .update(cx, |i, cx| i.set_text(&value, w, cx));
                                }
                            }
                            cx.notify();
                        })),
                    );
                }
                body = body.child(line);
            }
            for (index, row) in self.argument_rows.iter().enumerate() {
                let row = row.clone();
                let revision = self.argument_revision;
                body = body.child(
                    controls::button(
                        SharedString::from(format!("argument-result-{index}")),
                        row.title.clone(),
                        controls::ButtonTone::Ghost,
                        false,
                        t,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.argument_revision == revision {
                            if let Some(mut a) = this.form_activation.take() {
                                a.row = row.clone();
                                this.form = None;
                                this.execute_activation(a, cx);
                            }
                        }
                    })),
                );
            }
            body.child(
                controls::button(
                    "run-arguments",
                    "Run",
                    controls::ButtonTone::Primary,
                    false,
                    t,
                )
                .on_click(cx.listener(|this, _, _, cx| this.submit_form(cx))),
            )
            .into_any_element()
        } else if self.answer_active {
            div()
                .flex_1()
                .min_h_0()
                .child(self.answer.clone())
                .child(if self.answer_busy {
                    "Answering…"
                } else if self.answer_text.is_empty() {
                    "No answer returned"
                } else {
                    ""
                })
                .child(
                    controls::button(
                        "speak-answer",
                        "Speak",
                        controls::ButtonTone::Quiet,
                        self.answer_busy,
                        t,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.request_notice(Request::Speak(this.answer_text.clone()), cx)
                    })),
                )
                .into_any_element()
        } else {
            div()
                .id("launcher-listbox")
                .role(if grid { Role::Grid } else { Role::ListBox })
                .aria_label("Search results")
                .flex_1()
                .min_h_0()
                .p(px(8.))
                .child(list)
                .into_any_element()
        };
        let pin_bounds = self.pin_bounds.clone();
        let pin_geometry = canvas(
            |_, _, _| (),
            move |bounds, _, _, _| pin_bounds.set(Some(bounds)),
        )
        .absolute()
        .size_full();
        let mut pins = div()
            .relative()
            .id("pins")
            .role(Role::Toolbar)
            .aria_label("Pinned commands")
            .flex()
            .gap(px(4.))
            .px(px(12.))
            .py(px(4.))
            .child(pin_geometry);
        if self.model.input.is_empty() && self.navigation.is_none() {
            for (index, pin) in self.state.pins.iter().enumerate() {
                let id = pin.id.clone();
                let drag_id = id.clone();
                let generation = self.model.session.displayed.unwrap_or(0);
                pins = pins.child(
                    controls::button(
                        SharedString::from(format!("pin-{id}")),
                        format!("{}  ⌥{}", pin.title, ui_state::slot_key(index + 1)),
                        controls::ButtonTone::Ghost,
                        false,
                        t,
                    )
                    .debug_selector(move || format!("pin-{index}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &MouseDownEvent, _, _| {
                            this.drag = Some(PinDrag {
                                id: drag_id.clone(),
                                start: e.position,
                                active: false,
                                target: Some(index),
                            })
                        }),
                    )
                    .on_mouse_move(cx.listener(move |this, _, _, _| {
                        if let Some(drag) = &mut this.drag {
                            drag.target = Some(index);
                        }
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.suppress_pin_click && this.drag.as_ref().is_none_or(|d| !d.active)
                        {
                            this.click(generation, id.clone(), cx);
                        }
                        this.suppress_pin_click = false;
                    })),
                );
            }
        }
        let footer = if let Some(e) = &self.error {
            e.clone()
        } else if let Some(n) = &self.notice {
            n.clone()
        } else if !self.ready {
            "Starting search…".into()
        } else if self.model.session.pending() {
            if mode == Mode::Deep {
                "Searching file contents…".into()
            } else {
                "Searching…".into()
            }
        } else {
            String::new()
        };
        let entity = cx.entity();
        let tracker = canvas(
            |_, _, _| (),
            move |_, _, w, _| {
                let view = entity.clone();
                w.on_mouse_event(move |e: &MouseMoveEvent, phase, w, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    view.update(cx, |this, cx| {
                        if this.resize.is_some() && e.pressed_button != Some(MouseButton::Left) {
                            this.end_pointer_gesture(cx);
                        } else {
                            let _ = this.update_pointer_gesture(w, cx);
                        }
                        if let Some(drag) = &mut this.drag {
                            let dx = f32::from(e.position.x - drag.start.x);
                            let dy = f32::from(e.position.y - drag.start.y);
                            if dx * dx + dy * dy >= 36. {
                                drag.active = true;
                            }
                            if this
                                .pin_bounds
                                .get()
                                .is_none_or(|b| !b.contains(&e.position))
                            {
                                drag.target = None;
                            }
                        }
                    });
                });
                let view = entity.clone();
                w.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    view.update(cx, |this, cx| {
                        if this.resize.is_some() {
                            this.end_pointer_gesture(cx);
                        }
                        if e.button != MouseButton::Left {
                            return;
                        }
                        if let Some(drag) = this.drag.take() {
                            if drag.active {
                                this.suppress_pin_click = true;
                                if let Some(target) = drag.target {
                                    let from = this
                                        .state
                                        .pins
                                        .iter()
                                        .position(|p| p.id == drag.id)
                                        .unwrap_or(0);
                                    ui_state::reorder_pin(
                                        &mut this.state.pins,
                                        &drag.id,
                                        if target > from { target + 1 } else { target },
                                    );
                                } else {
                                    this.state.pins.retain(|p| p.id != drag.id);
                                    ui_state::normalize_pins(&mut this.state.pins);
                                }
                                this.persist(cx);
                                cx.notify();
                            }
                        }
                    });
                });
            },
        )
        .absolute()
        .size_full();
        controls::sheet(t)
            .relative()
            .font_family(tokens::font_family(cx, false))
            .capture_key_down(cx.listener(|this, e, w, cx| this.key(e, w, cx)))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, w, cx| {
                if let Some(gesture) = this.resize {
                    let _ = gesture.update(w, this.zoom, cx);
                }
                if let Some(drag) = &mut this.drag {
                    let dx = f32::from(e.position.x - drag.start.x);
                    let dy = f32::from(e.position.y - drag.start.y);
                    if dx * dx + dy * dy >= 36. {
                        drag.active = true;
                    }
                    if f32::from(e.position.y) > 100. {
                        drag.target = None;
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.resize = None;
                    if let Some(drag) = this.drag.take() {
                        if drag.active {
                            if let Some(target) = drag.target {
                                let from = this
                                    .state
                                    .pins
                                    .iter()
                                    .position(|p| p.id == drag.id)
                                    .unwrap_or(0);
                                ui_state::reorder_pin(
                                    &mut this.state.pins,
                                    &drag.id,
                                    if target > from { target + 1 } else { target },
                                );
                            } else {
                                this.state.pins.retain(|p| p.id != drag.id);
                                ui_state::normalize_pins(&mut this.state.pins);
                            }
                            this.persist(cx);
                            cx.notify();
                        }
                    }
                    let _ = this.relay.send(Event::SnapEnd);
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.resize = None;
                    if let Some(drag) = this.drag.take() {
                        if drag.active {
                            this.state.pins.retain(|p| p.id != drag.id);
                            ui_state::normalize_pins(&mut this.state.pins);
                            this.persist(cx);
                        }
                    }
                }),
            )
            .child(
                controls::header(t)
                    .child(div().id("drag-handle").w(px(8.)).h_full().on_mouse_down(
                        MouseButton::Left,
                        {
                            let relay = self.relay.clone();
                            move |_, _, _| {
                                let _ = relay.send(Event::SnapBegin);
                            }
                        },
                    ))
                    .when(mode != Mode::Basic, |d| {
                        d.child(
                            div()
                                .mr(px(8.))
                                .text_size(px(12.))
                                .text_color(t.text3)
                                .child(match mode {
                                    Mode::Deep => "Deep Search",
                                    Mode::Ai => "AI",
                                    Mode::Path => "Directory",
                                    Mode::Applications => "Applications",
                                    Mode::Terminal => "Terminal",
                                    _ => "",
                                }),
                        )
                    })
                    .child(self.input.clone())
                    .when(mode == Mode::Ai, |d| {
                        d.child(
                            controls::button(
                                "model-picker",
                                self.model_config["model"]
                                    .as_str()
                                    .unwrap_or("Choose model")
                                    .to_owned(),
                                controls::ButtonTone::Quiet,
                                self.model_saving,
                                t,
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.open_models(cx))),
                        )
                    })
                    .when(mode == Mode::Terminal, |d| {
                        d.child(
                            div()
                                .text_size(px(11.))
                                .text_color(t.text3)
                                .child(self.terminal_prompt.clone()),
                        )
                    }),
            )
            .when(
                !self.state.pins.is_empty() && self.model.input.is_empty(),
                |d| d.child(pins),
            )
            .when(self.history, |d| {
                d.child(
                    div()
                        .id("query-history")
                        .role(Role::ListBox)
                        .max_h(px(144.))
                        .overflow_y_scroll()
                        .children(self.state.launcher_query_history.iter().enumerate().map(
                            |(index, q)| {
                                let q = q.clone();
                                controls::button(
                                    SharedString::from(format!("history-{index}")),
                                    q.clone(),
                                    controls::ButtonTone::Ghost,
                                    false,
                                    t,
                                )
                                .on_click(cx.listener(
                                    move |this, _, w, cx| {
                                        this.history = false;
                                        this.edit(&q, w, cx);
                                    },
                                ))
                            },
                        )),
                )
            })
            .when(self.message_kind.is_some(), |d| {
                d.child(
                    div()
                        .px(px(12.))
                        .py(px(8.))
                        .border_b_1()
                        .border_color(t.hairline)
                        .bg(t.panel)
                        .max_h(px(160.))
                        .child(self.message_document.clone()),
                )
            })
            .when(mode==Mode::Basic&&self.model.input.is_empty()&&self.navigation.is_none(),|d|d.child(div().px(px(16.)).py(px(4.)).text_size(px(10.5)).text_color(t.text4).child("/directory    ` applications    ! Deep Search    SPACE AI    > terminal")))
            .child(content)
            .when(self.model_menu, |d| {
                d.child(
                    controls::popover("model-menu", "Choose AI model", t)
                        .absolute()
                        .right(px(12.))
                        .top(px(44.))
                        .w(px(360.))
                        .h(px(300.))
                        .child(
                            uniform_list(
                                "model-choice-list",
                                self.model_choices.len(),
                                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                                    range
                                        .map(|index| {
                                            let (provider, model) = &this.model_choices[index];
                                            controls::button(
                                                SharedString::from(format!("model-{index}")),
                                                format!("{provider} · {model}"),
                                                if index == this.model_choice {
                                                    controls::ButtonTone::Primary
                                                } else {
                                                    controls::ButtonTone::Ghost
                                                },
                                                false,
                                                t,
                                            )
                                            .h(px(36.))
                                            .on_click(
                                                cx.listener(move |this, _, _, cx| {
                                                    this.select_model(index, cx)
                                                }),
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                }),
                            )
                            .size_full(),
                        )
                        .child(
                            controls::button(
                                "configure-models",
                                "Configure AI providers",
                                controls::ButtonTone::Quiet,
                                false,
                                t,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.navigate(
                                    Surface::Settings("ai".into()),
                                    Payload::Settings { tab: "ai".into() },
                                    cx,
                                )
                            })),
                        ),
                )
            })
            .when_some(self.menu.clone(), |d, (row, selected)| {
                d.child(
                    controls::popover("result-actions", "Result actions", t)
                        .absolute()
                        .right(px(12.))
                        .bottom(px(36.))
                        .w(px(260.))
                        .children(presentation::menu(&row).into_iter().enumerate().map(
                            |(index, (label, action))| {
                                let row = row.clone();
                                controls::button(
                                    SharedString::from(format!("row-action-{index}")),
                                    label,
                                    if selected == index {
                                        controls::ButtonTone::Primary
                                    } else {
                                        controls::ButtonTone::Ghost
                                    },
                                    false,
                                    t,
                                )
                                .w_full()
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.row_action(row.clone(), action.clone(), cx)
                                    },
                                ))
                            },
                        )),
                )
            })
            .when_some(self.pin_picker.clone(), |d, (row, selected)| {
                d.child(
                    controls::popover("pin-picker", "Choose pin slot", t)
                        .absolute()
                        .right(px(12.))
                        .bottom(px(36.))
                        .w(px(280.))
                        .child("Choose pin slot")
                        .children((0..12).map(|index| {
                            let row = row.clone();
                            let title = self
                                .state
                                .pins
                                .get(index)
                                .map(|p| p.title.as_str())
                                .unwrap_or("Empty");
                            controls::button(
                                SharedString::from(format!("pin-slot-{index}")),
                                format!("⌥{}  {title}", ui_state::slot_key(index + 1)),
                                if index == selected {
                                    controls::ButtonTone::Primary
                                } else {
                                    controls::ButtonTone::Ghost
                                },
                                false,
                                t,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| this.pin_at(row.clone(), index, cx),
                            ))
                        })),
                )
            })
            .child(
                controls::footer(t)
                    .child(
                        div()
                            .id("launcher-status")
                            .role(Role::Status)
                            .when(self.error.is_some(), |d| d.text_color(t.danger))
                            .child(footer),
                    )
                    .when(self.error.is_some() && !self.ready, |d| {
                        let relay = self.relay.clone();
                        d.child(controls::action_button(
                            "retry-core",
                            "Retry",
                            controls::ButtonTone::Quiet,
                            false,
                            t,
                            move |_, _| {
                                let _ = relay.send(Event::RetryCore);
                            },
                        ))
                    })
                    .child(div().flex_1())
                    .child(if self.form.is_some() {
                        "Run"
                    } else if mode == Mode::Deep && !self.deep_navigation {
                        "Navigate"
                    } else if mode==Mode::Ai {"Start chat"} else if mode==Mode::Terminal {"Open terminal"} else {"Open"})
                    .child(controls::keycap("↵", t))
                    .when(mode==Mode::Path,|d|d.child("Complete").child(controls::keycap("Tab",t)))
                    .when(self.selected().as_ref().and_then(launcher::quick_look_path).is_some(),|d|d.child("Quick Look").child(controls::keycap("Space",t)))
                    .child("Actions")
                    .child(controls::keycap("⌘K", t))
                    .child("Pin")
                    .child(controls::keycap("⌘P", t)),
            )
            .child(tracker)
            .children(resize::handles(cx, |view, edge, w, _| {
                view.resize = resize::Gesture::begin(edge, w)
            }))
    }
}
