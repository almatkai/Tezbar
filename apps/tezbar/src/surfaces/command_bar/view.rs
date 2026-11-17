use super::*;
use presentation::RowAction;
use tezbar_ui_model::chat::RunActivity;

#[derive(Clone)]
struct PinTooltip {
    title: String,
    subtitle: String,
    slot: usize,
}
impl Render for PinTooltip {
    fn render(&mut self, w: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::tokens(w);
        controls::popover("pin-tooltip", self.title.clone(), t)
            .role(Role::Tooltip)
            .p(px(8. * t.zoom))
            .max_w(px(340. * t.zoom))
            .text_size(px(12. * t.zoom))
            .text_color(t.text1)
            .child(div().child(self.title.clone()))
            .when(!self.subtitle.is_empty(), |d| {
                d.child(div().text_color(t.text3).child(self.subtitle.clone()))
            })
            .child(controls::keycap(
                format!("⌥{}", ui_state::slot_key(self.slot)),
                t,
            ))
    }
}
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
    /// Pointer moved during a pin-rail drag (React `movePinPointerDrag` /
    /// `moveSearchResultPointerDrag`).
    fn drag_moved(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let rail = self.pin_bounds.get();
        let items = self.pin_item_bounds.borrow().clone();
        let trash = self.trash_bounds.get();
        let Some(drag) = &mut self.drag else {
            return;
        };
        if !drag.active {
            // 6px for both, like React's SEARCH_RESULT_PIN_DRAG_THRESHOLD: a slightly
            // shaky click on a pin must still run it.
            let threshold = 6.;
            let dx = f32::from(pos.x - drag.start.x);
            let dy = f32::from(pos.y - drag.start.y);
            if dx * dx + dy * dy < threshold * threshold {
                return;
            }
            drag.active = true;
        }
        let in_rail = rail.is_some_and(|b| b.contains(&pos));
        let insert = || {
            items
                .iter()
                .position(|b| b.is_some_and(|b| pos.y < b.top() + b.size.height / 2.))
                .unwrap_or(items.len())
        };
        let (target, unpin) = match drag.source {
            DragSource::Pin => {
                let past_last = items
                    .iter()
                    .rev()
                    .find_map(|b| *b)
                    .is_some_and(|b| pos.y > b.bottom() + px(8.));
                let unpin = in_rail && (trash.is_some_and(|b| b.contains(&pos)) || past_last);
                (in_rail.then(insert).filter(|_| !unpin), unpin)
            }
            DragSource::Result(_) => (in_rail.then(insert), false),
        };
        drag.target = target;
        drag.unpin = unpin;
        cx.notify();
    }
    /// Pointer released: commit the drop (React `commitPinDrop` / `pinSearchResultFromDrag`).
    fn drag_released(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        cx.notify();
        if !drag.active {
            return;
        }
        match drag.source {
            DragSource::Pin => {
                self.suppress_pin_click = true;
                let title = self
                    .state
                    .pins
                    .iter()
                    .find(|p| p.id == drag.id)
                    .map(|p| p.title.clone());
                if drag.unpin {
                    self.state.pins.retain(|p| p.id != drag.id);
                    ui_state::normalize_pins(&mut self.state.pins);
                    self.persist(cx);
                    if let Some(title) = title {
                        self.message(format!("Unpinned: {title}"), 2500, cx);
                    }
                } else if let Some(target) = drag.target {
                    let before: Vec<_> = self.state.pins.iter().map(|p| p.id.clone()).collect();
                    ui_state::reorder_pin(&mut self.state.pins, &drag.id, target);
                    if self.state.pins.iter().map(|p| &p.id).ne(before.iter()) {
                        self.persist(cx);
                    }
                }
            }
            DragSource::Result(row) => {
                self.suppress_row_click = true;
                let Some(target) = drag.target else {
                    return;
                };
                if self.state.pins.iter().any(|p| p.id == row.id) {
                    let before: Vec<_> = self.state.pins.iter().map(|p| p.id.clone()).collect();
                    ui_state::reorder_pin(&mut self.state.pins, &row.id, target);
                    if self.state.pins.iter().map(|p| &p.id).ne(before.iter()) {
                        self.persist(cx);
                        self.message(format!("Moved pin: {}", row.title), 2500, cx);
                    }
                } else if self.state.pins.len() >= ui_state::MAX_PINS {
                    self.message(format!("Pin limit reached ({})", ui_state::MAX_PINS), 2500, cx);
                } else {
                    ui_state::insert_pin(&mut self.state.pins, &row, Some(target));
                    self.persist(cx);
                    self.message(format!("Pinned: {}", row.title), 2500, cx);
                }
            }
        }
    }
    /// Footer hints per mode, mirroring the React launcher's `CommandSurfaceFooter`.
    fn footer_hints(&self, mode: &Mode) -> Vec<(&'static str, String)> {
        let key = |k: &str| k.to_owned();
        if self.form.is_some() {
            return vec![("Run", key("↵"))];
        }
        match mode {
            Mode::Terminal => vec![
                ("Stop", chord("X")),
                ("Delete", chord("D")),
                ("Pin / Unpin", chord("P")),
                ("Settings", chord("K")),
            ],
            Mode::Ai => vec![
                ("Providers", chord(",")),
                ("Attach screen", chord("⇧S")),
                ("Open chat", key("↵")),
                ("New chat", chord("N")),
                ("Close window", format!("Esc {}", chord(""))),
            ],
            Mode::Applications => vec![("Open", key("↵"))],
            Mode::Path => vec![("Complete", key("↵")), ("Open", chord("↵"))],
            Mode::Basic | Mode::Deep => {
                let mut hints = vec![("Pin / Unpin", chord("P")), ("Save note", chord("N"))];
                if *mode == Mode::Deep && !self.rows.is_empty() {
                    hints.push((
                        if self.deep_navigation { "Search input" } else { "Results" },
                        key("Tab"),
                    ));
                }
                if self
                    .selected()
                    .as_ref()
                    .and_then(launcher::quick_look_path)
                    .is_some()
                {
                    hints.push(("Quick Look", key("Space")));
                }
                hints
            }
        }
    }
    /// ⌘P parity with the React launcher: pin straight into the next ⌥ slot, or unpin.
    fn toggle_pin(&mut self, row: Row, cx: &mut Context<Self>) {
        if self.state.pins.iter().any(|p| p.id == row.id) {
            self.state.pins.retain(|p| p.id != row.id);
            ui_state::normalize_pins(&mut self.state.pins);
            self.persist(cx);
            self.message(format!("Unpinned: {}", row.title), 2500, cx);
        } else if !row.pinnable() || row.ephemeral {
            self.message("Temporary results can’t be pinned".into(), 2500, cx);
        } else if self.state.pins.len() >= ui_state::MAX_PINS {
            self.message(format!("Pin limit reached ({})", ui_state::MAX_PINS), 2500, cx);
        } else {
            ui_state::insert_pin(&mut self.state.pins, &row, None);
            self.persist(cx);
            self.message(format!("Pinned: {}", row.title), 2500, cx);
        }
    }
    fn row_action(&mut self, row: Row, action: RowAction, cx: &mut Context<Self>) {
        self.menu = None;
        match action {
            RowAction::Open => self.click(self.model.session.generation, row.id, cx),
            RowAction::Pin => self.toggle_pin(row, cx),
            RowAction::QuickLook => {
                let selected = self
                    .model
                    .session
                    .rows
                    .iter()
                    .position(|r| r.id == row.id)
                    .unwrap_or(self.model.session.selected);
                let paths = launcher::quick_look_paths(&self.model.session.rows, selected)
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
        self.clear_argument_results(cx);
        self.execute_activation(a, cx);
    }
    fn complete_path_for_id(&mut self, id: &str, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.paths.get(id).cloned() {
            if path["kind"] == "file" || path["kind"] == "application" {
                self.click(self.model.session.generation, id.to_owned(), cx);
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
    fn complete_path(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.selected() else { return };
        self.complete_path_for_id(&row.id, w, cx);
    }
    pub(super) fn return_home(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let from_chat = self
            .navigation
            .as_ref()
            .is_some_and(|n| n.surface == Surface::AiChat);
        self.acknowledge_navigation(cx);
        self.hide_chat(cx);
        self.hide_terminal(w, cx);
        self.hide_extensions(w, cx);
        self.hide_utilities(w, cx);
        self.prefix = None;
        self.model_menu = false;
        self.terminal_settings = false;
        self.navigation = None;
        self.sync_task_indicator(cx);
        self.form = None;
        self.form_activation = None;
        self.clear_argument_results(cx);
        self.menu = None;
        self.history = false;
        self.retire_answer(cx);
        self.model.terminal_mode = false;
        self.terminal_cwd = None;
        self.input.update(cx, |i, cx| i.clear_silently(w, cx));
        self.dispatch(Input::Hide, cx);
        if from_chat {
            // A leading space opens the AI conversation list with an empty query.
            self.model.input = " ".into();
        }
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
        if let Some((row, selected)) = self.menu.take() {
            self.input.focus_handle(cx).focus(w, cx);
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
            self.remember_query(cx);
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
            if mode.mode == Mode::Path {
                if let Some(row) = self.selected() {
                    if let Some(path) = self.paths.get(&row.id) {
                        if path["kind"] == "directory" {
                            self.complete_path(w, cx);
                            return;
                        }
                    }
                }
            }
            self.dispatch(Input::Enter { repeat }, cx);
            return;
        }
        if mode.mode == Mode::Path {
            self.remember_query(cx);
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
        self.remember_query(cx);
        self.answer_active = true;
        self.answer_busy = true;
        self.answer_flush_pending = false;
        self.update_answer(false, cx);
        let generation = self.model.session.generation;
        self.answer_serial += 1;
        let owner = format!(
            "native:{}:{}:{}",
            std::process::id(),
            self.view_owner,
            self.answer_serial
        );
        self.answer_owner = Some(owner.clone());
        let request = Request::QueryOwned {
            query: self.model.input.clone(),
            owner: owner.clone(),
        };
        let adapter = self.adapter.clone();
        // CoreEvents subscription was installed at construction, before this stream request.
        self.tasks.push(cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move { adapter.send(request).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| {
                if generation == this.model.session.generation
                    && this.visible
                    && this.answer_owner.as_deref() == Some(owner.as_str())
                {
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
    fn application_arrow(&mut self, delta: isize, w: &mut Window, cx: &mut Context<Self>) -> bool {
        if launcher::input_mode(&self.model.input, self.model.terminal_mode).mode
            != Mode::Applications
            || self.navigation.is_some()
            || self.form.is_some()
            || self.menu.is_some()
            || self.model_menu
            || self.input.read(cx).marked_range.is_some()
            || !self.input.focus_handle(cx).is_focused(w)
        {
            return false;
        }
        self.deep_navigation = true;
        self.dispatch(Input::Move(delta), cx);
        tezbar_ui::virtual_list::reveal(&self.scroll, self.model.session.selected, 7);
        self.schedule_icon_viewport(w, cx);
        true
    }
    pub(super) fn key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        let key = e.keystroke.key.as_str();
        if key == "escape" && self.task_indicator.update(cx, |view, cx| view.dismiss(cx)) {
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        let primary = if cfg!(target_os = "macos") {
            e.keystroke.modifiers.platform
        } else {
            e.keystroke.modifiers.control
        };
        let alt = e.keystroke.modifiers.alt;
        let shift = e.keystroke.modifiers.shift;
        if e.is_held && primary && matches!(key, "q" | "n") {
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if self.model_menu && self.model_search.read(cx).marked_range.is_some() {
            if key == "escape" {
                self.model_search
                    .update(cx, |field, cx| field.cancel_composition(w, cx));
                w.prevent_default();
                cx.stop_propagation();
            }
            return;
        }
        let composing_field = self
            .form
            .as_ref()
            .and_then(|form| {
                form.inputs
                    .iter()
                    .find(|field| field.read(cx).marked_range.is_some())
                    .cloned()
            })
            .or_else(|| {
                self.input
                    .read(cx)
                    .marked_range
                    .is_some()
                    .then(|| self.input.clone())
            });
        if let Some(field) = composing_field {
            if key == "escape" {
                field.update(cx, |i, cx| i.cancel_composition(w, cx));
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
                tezbar_ui_model::router::NewItem::Note => Surface::Notes,
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
        if self.argument_dropdown_key(e, w, cx) {
            return;
        }
        if key == "escape" {
            if let Some((_, _)) = self.menu.take() {
                self.input.focus_handle(cx).focus(w, cx);
            } else if let Some(form) = &mut self.form {
                let index = form.args.selected;
                let field = &form.inputs[index];
                if !form.args.values[index].is_empty() {
                    form.args.values[index].clear();
                    field.update(cx, |i, cx| i.clear_silently(w, cx));
                    self.argument_changed(index, "", cx);
                } else {
                    if let Some(a) = self.form_activation.take() {
                        self.model.session.activation_finished(a.generation);
                    }
                    self.form = None;
                    self.clear_argument_results(cx);
                    self.input.focus_handle(cx).focus(w, cx);
                }
            } else if self.model_menu {
                self.close_models(w, cx);
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
                        let text = if let Some(directory) = mode.ai_working_directory {
                            if mode.ai_task.is_empty() {
                                directory
                            } else {
                                format!("{directory}  ")
                            }
                        } else if self.model.input.trim().is_empty() {
                            String::new()
                        } else {
                            " ".into()
                        };
                        self.edit(&text, w, cx);
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
            if self.model_search.focus_handle(cx).is_focused(w)
                && matches!(key, "up" | "down" | "enter")
            {
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
                self.model_scroll
                    .scroll_to_item(self.model_choice, ScrollStrategy::Nearest);
                cx.notify();
                w.prevent_default();
                cx.stop_propagation();
            }
            return;
        }
        if self.terminal_settings {
            if key == "enter" && self.input.focus_handle(cx).is_focused(w) {
                if !e.is_held {
                    self.save_terminal_settings(cx);
                }
                w.prevent_default();
                cx.stop_propagation();
            }
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
                "tab" => {
                    *selected = if shift {
                        selected.saturating_sub(1)
                    } else {
                        (*selected + 1).min(presentation::menu(row).len().saturating_sub(1))
                    }
                }
                "enter" => self.enter(e.is_held, false, w, cx),
                _ => {}
            }
            cx.notify();
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if alt
            && !primary
            && !e.is_held
            && launcher::input_mode(&self.model.input, self.model.terminal_mode).mode != Mode::Ai
        {
            let code = match key {
                "-" => "Minus",
                "=" => "Equal",
                _ => "",
            };
            if let Some(slot) = ui_state::pin_slot(code, key) {
                if let Some(pin) = self.state.pins.get(slot - 1) {
                    let row = presentation::pinned_row(pin);
                    self.dispatch(
                        Input::ActivatePin {
                            row,
                            rank: slot - 1,
                        },
                        cx,
                    );
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
            if launcher::input_mode(&self.model.input, false).mode == Mode::Ai {
                self.message("Pinned commands are hidden in AI mode".into(), 2500, cx);
            } else if let Some(row) = self.selected() {
                self.toggle_pin(row, cx);
            } else {
                self.message("No command selected to pin or unpin".into(), 2500, cx);
            }
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if primary && key == "k" {
            if let Some(row) = self.selected() {
                self.menu = Some((row, 0));
                self.popover_focus.focus(w, cx);
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
            && !e.is_held
            && !shift
            && self.input.focus_handle(cx).is_focused(w)
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
            && (self.model.session.rows.is_empty() || self.model.session.selected == 0)
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
            if mode != Mode::Applications {
                self.path_list
                    .scroll_to_reveal_item(self.model.session.selected);
            } else {
                tezbar_ui::virtual_list::reveal(
                    &self.scroll,
                    self.model.session.selected,
                    if mode == Mode::Applications { 7 } else { 1 },
                );
            }
            self.schedule_icon_viewport(w, cx);
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
        let activity = self.chat_row_status(&row);
        let activity_id = format!("chat-activity-{id}");
        let image = self.icons.get(&id).cloned();
        let selected = index == self.model.session.selected;
        let pin_slot = self
            .state
            .pins
            .iter()
            .position(|p| p.id == id)
            .map(|i| i + 1);
        let instant = !grid && matches!(row.category.as_str(), "calculator" | "color-converter");
        let currency = row.id.starts_with("currency:");
        let menu_row = row.clone();
        // Path completion rows (React `completePath` list): two lines, mono title,
        // text badge, and the neutral glass selection instead of the accent fill.
        let path_meta = (!grid).then(|| self.paths.get(&id)).flatten();
        let path = path_meta.is_some();
        let path_badge: SharedString = path_meta
            .and_then(|v| v["badge"].as_str())
            .map(|badge| SharedString::from(badge.to_owned()))
            .unwrap_or_else(|| match row.category.as_str() {
                "directory" => "Folder".into(),
                "application" => "Open With".into(),
                _ => "File".into(),
            });
        tezbar_ui::virtual_list::row(
            id.clone().into(),
            title.clone().into(),
            index,
            self.rows.len(),
            selected,
            t,
        )
        .when(grid, |d| {
            d.h(px(96. * t.zoom))
                .flex_col()
                .justify_center()
                .px(px(4. * t.zoom))
                .py(px(8. * t.zoom))
                .overflow_hidden()
        })
        .when(!grid, |d| d.px(px(8. * t.zoom)))
        .when(instant, |d| d.h(px(52. * t.zoom)).px(px(8. * t.zoom)).py(px(6. * t.zoom)))
        .gap(px(if grid { 8. } else { 10. } * t.zoom))
        .when(path, |d| {
            d.h_auto()
                .min_h(px(36. * t.zoom))
                .px(px(10. * t.zoom))
                .py(px(6. * t.zoom))
                .gap(px(12. * t.zoom))
                .text_color(t.text2)
                .bg(if selected { t.selected } else { transparent_black() })
        })
        // Pinned rows: a short amber tick on the leading edge (legacy launcher parity).
        .when(pin_slot.is_some() && !grid, |d| {
            d.relative().child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .child(div().w(px(2.)).h(px(14. * t.zoom)).rounded_full().bg(rgba(0xfcd34dcc))),
            )
        })
        .debug_selector(move || format!("result-{index}"))
        .when(!grid && !instant && row.pinnable() && !row.ephemeral, |d| {
            let drag_row = row.clone();
            d.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, _| {
                    this.suppress_row_click = false;
                    let mode = launcher::input_mode(&this.model.input, this.model.terminal_mode).mode;
                    if matches!(mode, Mode::Basic | Mode::Deep) && this.navigation.is_none() {
                        this.drag = Some(PinDrag {
                            id: drag_row.id.clone(),
                            start: e.position,
                            active: false,
                            target: None,
                            unpin: false,
                            source: DragSource::Result((*drag_row).clone()),
                        });
                    }
                }),
            )
        })
        .on_click(cx.listener(move |this, _, w, cx| {
            // A drag that ended on this row is a drop, not an activation.
            if std::mem::take(&mut this.suppress_row_click) {
                return;
            }
            if let Some(path) = this.paths.get(&id) {
                if path["kind"] == "directory" {
                    this.complete_path_for_id(&id, w, cx);
                    return;
                }
            }
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
            cx.listener(move |this, _, w, cx| {
                if !this.model.session.pending() {
                    this.menu = Some((menu_row.as_ref().clone(), 0));
                    this.popover_focus.focus(w, cx);
                    cx.notify();
                }
            }),
        )
        .child(if instant {
            let glyph = if row.category == "color-converter" {
                let raw = match &row.action { Action::CopyText { text } | Action::CopyAndPasteText { text } => text.as_str(), _ => self.model.input.as_str() };
                let color = tezbar_core::instant::color::parse_color(raw);
                div().size(px(14. * t.zoom)).rounded_full().border_1().border_color(rgba(0xffffff4d))
                    .when_some(color, |d, color| d.bg(gpui::Rgba { r: color.r as f32 / 255., g: color.g as f32 / 255., b: color.b as f32 / 255., a: color.a as f32 })).into_any_element()
            } else {
                let data: &'static [u8] = if currency {
                    br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 14 14" fill="none" stroke="black" stroke-width="1.1" stroke-linecap="round"><circle cx="7" cy="7" r="5.25"/><path d="M9 5.25c-.4-.55-1.1-.95-2-.95-1.1 0-2 .55-2 1.4 0 2 4 1 4 3 0 .85-.9 1.4-2 1.4-.9 0-1.6-.4-2-.95M7 3.25v7.5"/></svg>"#
                } else {
                    br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 14 14" fill="none" stroke="black" stroke-width="1.1"><rect x="2.5" y="1.5" width="9" height="11" rx="1.5"/><path d="M4.25 3.25h5.5v2h-5.5zM4.5 7.5h1M6.5 7.5h1M8.5 7.5h1M4.5 9.75h1M6.5 9.75h1M8.5 9.75h1"/></svg>"#
                };
                svg().data(data).size(px(14. * t.zoom)).text_color(rgb(0xffffff)).into_any_element()
            };
            div().size(px(22. * t.zoom)).flex_shrink_0().rounded(px(6. * t.zoom)).bg(rgb(0x71717a)).flex().items_center().justify_center().child(glyph).into_any_element()
        } else if let Some(image) = image {
            img(image)
                .size(px(if grid { 40. } else { 22. } * t.zoom))
                .flex_shrink_0()
                .into_any_element()
        } else {
            icons::tile(&self.rows[index], if grid { 40. } else { 22. } * t.zoom).into_any_element()
        })
        .when(path, |d| {
            d.child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2. * t.zoom))
                    .debug_selector(move || format!("result-text-{index}"))
                    .child(
                        div()
                            .font_family(tezbar_ui::tokens::mono_family())
                            .text_size(px(12.5 * t.zoom))
                            .text_color(t.text1)
                            .text_ellipsis()
                            .child(title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10.5 * t.zoom))
                            .text_color(t.text4)
                            .text_ellipsis()
                            .child(row.subtitle.clone()),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .ml(px(12. * t.zoom))
                    .text_size(px(11. * t.zoom))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.text4)
                    .debug_selector(move || format!("result-kind-{index}"))
                    .child(path_badge.clone()),
            )
        })
        .when(!path, |d| d.child(
            div()
                .flex()
                .items_baseline()
                .when(instant, |d| d.flex_col().items_start().gap(px(2. * t.zoom)))
                .when(grid, |d| d.justify_center())
                .when(!grid, |d| d.flex_1())
                .min_w_0()
                .w_full()
                .gap(px(if instant { 2. } else { 8. } * t.zoom))
                .debug_selector(move || format!("result-text-{index}"))
                .child(
                    div()
                        .min_w_0()
                        .max_w(relative(if grid || instant { 1. } else { 0.7 }))
                        .when(!grid, |d| d.flex_shrink_0())
                        .when(grid, |d| {
                            d.w_full()
                                .h(px(32. * t.zoom))
                                .line_height(px(16. * t.zoom))
                                .whitespace_normal()
                                .line_clamp(2)
                                .text_center()
                        })
                        .text_size(px(13. * t.zoom))
                        .font_weight(FontWeight::MEDIUM)
                        .when(instant, |d| d.font_family(tezbar_ui::tokens::mono_family()).text_size(px(14. * t.zoom)).font_weight(FontWeight::SEMIBOLD))
                        .text_ellipsis()
                        .child(title),
                )
                .when(!grid, |d| {
                    let subtitle = self.rows[index].subtitle.clone();
                    d.when(!subtitle.is_empty(), |d| {
                        d.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .text_size(px(12. * t.zoom))
                                .text_color(if selected && !instant { rgba(0xffffffb8).into() } else if instant { t.text3 } else { t.text4 })
                                .when(instant, |d| d.font_family(tezbar_ui::tokens::mono_family()).text_size(px(11. * t.zoom)))
                                .child(if instant { format!("{} · {subtitle}", if self.rows[index].category == "color-converter" { "Color" } else if currency { "Currency" } else { "Calculator" }) } else { subtitle }),
                        )
                    })
                }),
        ))
        .when(instant, |d| d.child(div().flex().items_center().gap(px(6.)).text_size(px(10.)).font_family(tezbar_ui::tokens::mono_family()).text_color(t.text3).child(controls::keycap("↵", t)).child("copy")))
        .when_some(pin_slot.filter(|_| !grid && !instant && !path), |d, slot| {
            d.child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(2. * t.zoom))
                    .debug_selector(move || format!("result-pin-slot-{index}"))
                    .child(controls::keycap("⌥", t))
                    .child(controls::keycap(ui_state::slot_key(slot), t)),
            )
        })
        .when(!grid && !instant && !path, |d| {
            let badge = if let Some(activity) = activity {
                if activity == RunActivity::WaitingForApproval || cx.reduce_motion() {
                    div().child(activity.label()).into_any_element()
                } else {
                    controls::shimmer_text(activity_id, activity.label(), t.light && !selected)
                        .into_any_element()
                }
            } else {
                div()
                    .child(presentation::kind_label(&row.category).to_owned())
                    .into_any_element()
            };
            d.child(
                div()
                    .flex_shrink_0()
                    .px(px(6. * t.zoom))
                    .py(px(2. * t.zoom))
                    .rounded(px(5. * t.zoom))
                    .when(activity.is_none(), |d| {
                        d.bg(if selected { rgba(0xffffff29).into() } else { t.pill })
                    })
                    .when(activity.is_some(), |d| d.font_weight(FontWeight::MEDIUM))
                    .text_size(px(10.5 * t.zoom))
                    .line_height(px(14. * t.zoom))
                    .text_color(if activity == Some(RunActivity::WaitingForApproval) {
                        t.warning
                    } else if selected {
                        rgb(0xffffff).into()
                    } else {
                        t.text3
                    })
                    .debug_selector(move || format!("result-kind-{index}"))
                    .child(badge),
            )
        })
    }
}
impl Render for Launcher {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.migration_pending() {
            let t = tokens::tokens(w);
            return controls::sheet(t)
                .debug_selector(|| "preference-recovery-frame".into())
                .font_family(tokens::font_family(cx, false))
                .p_6()
                .gap_4()
                .child("Recover saved preferences")
                .child(self.error.clone().unwrap_or_else(|| {
                    "Saved preferences are unavailable. Originals have been preserved.".into()
                }))
                .child(
                    div()
                        .id("retry-legacy-preferences")
                        .debug_selector(|| "retry-legacy-preferences".into())
                        .cursor_pointer()
                        .child("Retry saved changes")
                        .on_click(cx.listener(|this, _, _, _| {
                            let _ = this.relay.send(Event::RetryLegacy);
                        })),
                )
                .child(
                    div()
                        .id("fresh-legacy-recovery")
                        .cursor_pointer()
                        .child(if self.recovery_confirm {
                            "Confirm: use defaults and keep the old preferences untouched"
                        } else {
                            "Start with defaults…"
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            if this.recovery_confirm {
                                let _ = this.relay.send(Event::ConfirmFreshState);
                            } else {
                                this.recovery_confirm = true;
                                cx.notify();
                            }
                        })),
                )
                .child(
                    div()
                        .id("quit-legacy-recovery")
                        .cursor_pointer()
                        .child("Quit Tezbar")
                        .on_click(cx.listener(|this, _, _, _| {
                            let _ = this.relay.send(Event::Quit);
                        })),
                )
                .into_any_element();
        }
        if self.onboarding_active {
            if let Some(onboarding) = &self.onboarding {
                return div()
                    .size_full()
                    .child(onboarding.clone())
                    .into_any_element();
            }
        }
        if self.extension_mounted() {
            if self
                .navigation
                .as_ref()
                .is_some_and(|n| n.surface == Surface::Extensions)
            {
                if let Some(v) = &self.extensions.store {
                    return div().size_full().child(v.clone()).into_any_element();
                }
            } else if let Some(v) = &self.extensions.runtime {
                return div().size_full().child(v.clone()).into_any_element();
            }
        }
        if self.terminal_mounted() {
            return div()
                .size_full()
                .child(self.terminal.view.clone().unwrap())
                .into_any_element();
        }
        if self.chat_mounted() {
            return div()
                .size_full()
                .child(self.chat.view.clone().unwrap())
                .into_any_element();
        }
        w.set_rem_size(px(16. * self.zoom));
        let t = tokens::tokens(w);
        let utility = self
            .utilities
            .as_ref()
            .filter(|view| {
                let view = view.read(cx);
                self.navigation
                    .as_ref()
                    .is_some_and(|navigation| navigation.surface == view.surface)
                    && view.visible
            })
            .cloned();
        let utility_active = utility.is_some();
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
            .on_scroll(cx.listener(|this, _, w, cx| this.schedule_icon_viewport(w, cx)))
            .size_full()
            .into_any_element()
        } else {
            list(
                self.path_list.clone(),
                cx.processor(move |this, index: usize, _, cx| {
                    let section = this
                        .paths
                        .get(&this.rows[index].id)
                        .and_then(|v| v["section"].as_str());
                    let previous = index
                        .checked_sub(1)
                        .and_then(|i| this.paths.get(&this.rows[i].id))
                        .and_then(|v| v["section"].as_str());
                    let heading = if index == 0 || section != previous {
                        section.and_then(presentation::path_section)
                    } else {
                        None
                    };
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .when_some(heading, |d, heading| {
                            d.child(
                                div()
                                    .id(SharedString::from(format!("path-section-{index}")))
                                    .debug_selector(move || format!("path-section-{index}"))
                                    .role(Role::Heading)
                                    .aria_label(heading)
                                    .px(px(12. * t.zoom))
                                    .pt(px(8. * t.zoom))
                                    .pb(px(4. * t.zoom))
                                    .text_size(px(11. * t.zoom))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.text4)
                                    .child(heading),
                            )
                        })
                        .child(this.row_element(index, false, t, cx))
                        .when(index == 0, |d| d.pt(px(6. * t.zoom)))
                        .when(index + 1 < this.rows.len(), |d| d.pb(px(2. * t.zoom)))
                        .when(index + 1 == this.rows.len(), |d| {
                            d.pb(px((tokens::FOOTER + 8.) * t.zoom))
                        })
                        .into_any_element()
                }),
            )
            .size_full()
            .into_any_element()
        };
        let content = if let Some(utility) = utility {
            div()
                .flex_1()
                .min_w_0()
                .w_full()
                .max_w(relative(1.))
                .min_h_0()
                .child(utility)
                .into_any_element()
        } else if let Some(n) = &self.navigation {
            div()
                .flex_1()
                .p(px(16.))
                .child(navigation::surface_title(&n.surface))
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
                    this.save_terminal_settings(cx);
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
            for index in 0..form.inputs.len() {
                if !form.inline() {
                    body = body.child(self.argument_field(index, t, cx));
                }
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
                                this.clear_argument_results(cx);
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
                .id("launcher-listbox").flex().flex_col()
                .debug_selector(|| "launcher-listbox".into())
                .min_w_0()
                .role(if grid { Role::Grid } else { Role::ListBox })
                .aria_label("Search results")
                .flex_1()
                .min_h_0()
                .px(px(6. * t.zoom))
                // Lists scroll edge to edge under the header and footer; their first and
                // last rows carry the breathing room instead of the clipping container.
                .when(grid || self.rows.is_empty(), |d| d.pt(px(8.)))
                .when(grid, |d| d.pb(px(tokens::FOOTER * t.zoom)))
                .when(self.rows.is_empty() && (mode == Mode::Ai || mode == Mode::Terminal || (mode == Mode::Deep && self.model.session.pending())), |d| {
                    let text = if mode == Mode::Ai {
                        if self.model.session.pending() { "Loading conversations…" }
                        else if !launcher::input_mode(&self.model.input, self.model.terminal_mode).ai_task.trim().is_empty() { "No matching conversations. Press Enter to ask AI as a new chat." }
                        else { "No saved conversations yet. Type a question and press Enter to start one." }
                    } else if mode == Mode::Terminal { "No saved terminal sessions yet. Press Enter to start a new terminal." } else { "Searching indexed files" };
                    let mut state = div().id("launcher-empty-state").role(Role::Status).flex_1().flex().flex_col().items_center().justify_center().gap(px(8.)).p(px(24.)).text_size(px(12.)).text_color(t.text3).child(text);
                    if mode == Mode::Deep {
                        let skeleton = div().w_full().max_w(px(520. * t.zoom)).mt(px(20.)).flex().flex_col().gap(px(8.))
                            .children([0.82, 0.68, 0.91].into_iter().map(|width| {
                                div().flex().items_center().gap(px(12.)).rounded(px(tokens::ROW_RADIUS)).border_1().border_color(t.hairline).bg(t.bar).px(px(12.)).py(px(10.))
                                    .child(div().size(px(32. * t.zoom)).rounded(px(tokens::ROW_RADIUS)).bg(t.pill))
                                    .child(div().flex_1().flex().flex_col().gap(px(8.))
                                        .child(div().w(relative(width)).h(px(8.)).rounded_full().bg(t.pill_strong))
                                        .child(div().w(relative(width * 0.6)).h(px(6.)).rounded_full().bg(t.pill)))
                            }));
                        state = state.child(div().text_size(px(11.)).text_color(t.text4).child(format!("Matching text, OCR, and vector embeddings for “{}”", self.model.input.trim_start_matches('!').trim()))).child(skeleton);
                    }
                    d.child(state)
                })
                .when(!self.rows.is_empty(), |d| d.child(list))
                .into_any_element()
        };
        let pin_bounds = self.pin_bounds.clone();
        let pin_geometry = canvas(
            |_, _, _| (),
            move |bounds, _, _, _| pin_bounds.set(Some(bounds)),
        )
        .absolute()
        .size_full();
        // Geometry is repainted every frame; stale bounds must not survive a hidden rail.
        self.pin_bounds.set(None);
        self.trash_bounds.set(None);
        *self.pin_item_bounds.borrow_mut() = vec![None; self.state.pins.len()];
        let accent: Hsla = rgb(0x5b5ef0).into();
        let dragging_pin = self
            .drag
            .as_ref()
            .filter(|d| d.active && matches!(d.source, DragSource::Pin));
        let dragging_result = self
            .drag
            .as_ref()
            .is_some_and(|d| d.active && matches!(d.source, DragSource::Result(_)));
        let drop_index = self.drag.as_ref().filter(|d| d.active).and_then(|d| d.target);
        let unpin_active = dragging_pin.is_some_and(|d| d.unpin);
        let dragging_pin_id = dragging_pin.map(|d| d.id.clone());
        let drop_line = move || {
            div()
                .absolute()
                .left(px(4.))
                .right(px(4.))
                .h(px(2.))
                .rounded_full()
                .bg(accent)
        };
        let mut pin_list = div()
            .id("pin-list")
            .flex()
            .flex_col()
            .items_center()
            .flex_1()
            .min_h_0()
            .gap(px(4. * t.zoom))
            .overflow_y_scroll()
            .when(self.state.pins.is_empty() && dragging_result, |d| {
                d.child(
                    div()
                        .size(px(32. * t.zoom))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(8. * t.zoom))
                        .border_1()
                        .border_dashed()
                        .border_color(Hsla { a: 0.6, ..accent })
                        .bg(Hsla { a: 0.08, ..accent })
                        .font_family(tezbar_ui::tokens::mono_family())
                        .text_size(px(15. * t.zoom))
                        .text_color(accent)
                        .child("+"),
                )
            });
        if self.navigation.is_none() {
            let count = self.state.pins.len();
            for (index, pin) in self.state.pins.iter().enumerate() {
                let id = pin.id.clone();
                let drag_id = id.clone();
                let item_bounds = self.pin_item_bounds.clone();
                let dimmed = dragging_pin_id.as_deref() == Some(id.as_str());
                let tooltip = PinTooltip {
                    title: pin.title.clone(),
                    subtitle: pin.subtitle.clone(),
                    slot: index + 1,
                };
                pin_list = pin_list.child(
                    div()
                        .id(SharedString::from(format!("pin-{id}")))
                        .role(Role::Button)
                        .aria_label(format!(
                            "{} · pinned slot {}",
                            pin.title,
                            ui_state::slot_key(index + 1)
                        ))
                        .tab_index(0)
                        .relative()
                        .flex_shrink_0()
                        .size(px(36. * t.zoom))
                        .rounded(px(8. * t.zoom))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor(CursorStyle::OpenHand)
                        .when(dimmed, |d| d.opacity(0.45))
                        .when(!dimmed, |d| d.hover(|d| d.bg(t.selected)))
                        .child(
                            canvas(|_, _, _| (), move |bounds, _, _, _| {
                                if let Some(slot) = item_bounds.borrow_mut().get_mut(index) {
                                    *slot = Some(bounds);
                                }
                            })
                            .absolute()
                            .size_full(),
                        )
                        .when(drop_index == Some(index), |d| d.child(drop_line().top(px(-3.))))
                        .when(index + 1 == count && drop_index == Some(count), |d| {
                            d.child(drop_line().bottom(px(-3.)))
                        })
                        .when(self.drag.is_none(), |d| {
                            d.tooltip(move |_, cx| cx.new(|_| tooltip.clone()).into())
                        })
                        .child(if let Some(image) = self.icons.get(&pin.id) {
                            img(image.clone()).size(px(22. * t.zoom)).into_any_element()
                        } else {
                            icons::tile(&presentation::pinned_row(pin), 22. * t.zoom)
                                .into_any_element()
                        })
                        .child(
                            div()
                                .absolute()
                                .right_0()
                                .bottom_0()
                                .rounded(px(4.))
                                .px(px(2.))
                                .bg(t.pill_strong)
                                .text_size(px(8.5 * t.zoom))
                                .child(ui_state::slot_key(index + 1)),
                        )
                        .debug_selector(move || format!("pin-{index}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, e: &MouseDownEvent, _, _| {
                                this.suppress_pin_click = false;
                                this.drag = Some(PinDrag {
                                    id: drag_id.clone(),
                                    start: e.position,
                                    active: false,
                                    target: Some(index),
                                    unpin: false,
                                    source: DragSource::Pin,
                                })
                            }),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.suppress_pin_click
                                && this.drag.as_ref().is_none_or(|d| !d.active)
                            {
                                if let Some(pin) = this.state.pins.get(index) {
                                    let row = presentation::pinned_row(pin);
                                    this.dispatch(Input::ActivatePin { row, rank: index }, cx);
                                }
                            }
                            this.suppress_pin_click = false;
                        })),
                );
            }
        }
        let trash_bounds = self.trash_bounds.clone();
        let rose: Hsla = rgb(0xfb7185).into();
        let pins = div()
            .relative()
            .id("pins")
            .role(Role::Toolbar)
            .aria_label("Pinned commands")
            .flex()
            .flex_col()
            .items_center()
            .flex_shrink_0()
            .w(px(44. * t.zoom))
            .px(px(4. * t.zoom))
            .pt(px(6. * t.zoom))
            // The footer floats over the bottom edge; keep the trash target above it.
            .pb(px((tokens::FOOTER + 4.) * t.zoom))
            .border_r_1()
            .border_color(t.hairline)
            .when(unpin_active, |d| {
                d.border_color(Hsla { a: 0.35, ..rgb(0xfda4af).into() })
                    .bg(Hsla { a: 0.04, ..rose })
            })
            .when(dragging_result, |d| {
                d.border_color(Hsla { a: 0.4, ..accent }).bg(Hsla { a: 0.035, ..accent })
            })
            .child(pin_geometry)
            .child(pin_list)
            .when(dragging_pin.is_some(), |d| {
                d.child(
                    div()
                        .debug_selector(|| "pin-unpin-target".into())
                        .mt(px(4. * t.zoom))
                        .h(px(32. * t.zoom))
                        .w_full()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(8. * t.zoom))
                        .border_1()
                        .border_dashed()
                        .when(unpin_active, |d| {
                            d.border_color(Hsla { a: 0.55, ..rgb(0xfda4af).into() })
                                .bg(Hsla { a: 0.15, ..rose })
                                .text_color(rgb(0xfecdd3))
                        })
                        .when(!unpin_active, |d| {
                            d.border_color(rgba(0xffffff1a))
                                .bg(rgba(0xffffff06))
                                .text_color(Hsla { a: 0.7, ..t.text4 })
                        })
                        .child(
                            canvas(|_, _, _| (), move |bounds, _, _, _| trash_bounds.set(Some(bounds)))
                                .absolute()
                                .size_full(),
                        )
                        .child(
                            svg()
                                .data(br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 14 14" fill="none" stroke="white" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"><path d="M3.25 4.25h7.5M5.25 4.25V3h3.5v1.25M4.25 4.25l.5 7h4.5l.5-7M6 6.25v3M8 6.25v3"/></svg>"#.as_slice())
                                .size(px(14. * t.zoom))
                                .text_color(if unpin_active { rgb(0xfecdd3).into() } else { Hsla { a: 0.7, ..t.text4 } }),
                        ),
                )
            });
        let footer = if let Some(e) = &self.error {
            e.clone()
        } else if let Some(n) = self.footer_notice() {
            n.to_owned()
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
                        if let Some(origin) = this.move_arm {
                            if e.pressed_button != Some(MouseButton::Left) {
                                this.move_arm = None;
                            } else if move_started(origin, e.position) {
                                this.move_arm = None;
                                let _ = this.relay.send(Event::SnapBegin(Some((
                                    f32::from(origin.x),
                                    f32::from(origin.y),
                                ))));
                            }
                        }
                        if this.resize.is_some() && e.pressed_button != Some(MouseButton::Left) {
                            this.end_pointer_gesture(cx);
                        } else {
                            let _ = this.update_pointer_gesture(w, cx);
                        }
                        if this.drag.is_some() {
                            if e.pressed_button == Some(MouseButton::Left) {
                                this.drag_moved(e.position, cx);
                            } else {
                                // Released outside the window: cancel, never unpin.
                                this.drag = None;
                                cx.notify();
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
                        this.move_arm = None;
                        if this.resize.is_some() {
                            this.end_pointer_gesture(cx);
                        }
                        if e.button != MouseButton::Left {
                            return;
                        }
                        this.drag_released(cx);
                    });
                });
            },
        )
        .absolute()
        .size_full();
        // Utilities own the same sheet chrome as every other mounted screen.
        // Wrapping them in another sheet doubles the translucent background,
        // border and rounded corners compared with the main launcher.
        let root = if utility_active {
            div().size_full().flex().flex_col()
        } else {
            controls::sheet(t)
        };
        root
            .relative()
            .font_family(tokens::font_family(cx, false))
            // Empty header/footer chrome (any mounted surface) moves the window.
            .on_action(cx.listener(|this, _: &controls::ArmWindowMove, w, _| {
                this.move_arm = Some(w.mouse_position());
            }))
            .capture_key_down(cx.listener(|this, e, w, cx| this.key(e, w, cx)))
            // TextInput consumes horizontal-arrow actions before key_down runs.
            .capture_action(cx.listener(|this, _: &tezbar_ui::text_input::Left, w, cx| {
                if this.application_arrow(-1, w, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(|this, _: &tezbar_ui::text_input::Right, w, cx| {
                if this.application_arrow(1, w, cx) {
                    cx.stop_propagation();
                }
            }))
            // The text field consumes Backspace as an action, so key_down never sees it.
            // An empty mode query (`>`, `!`, `` ` ``) leaves the mode instead of doing nothing.
            .capture_action(cx.listener(|this, _: &Backspace, w, cx| {
                if this.navigation.is_none()
                    && this.form.is_none()
                    && this.menu.is_none()
                    && !this.model_menu
                    && !this.terminal_settings
                    && this.input.focus_handle(cx).is_focused(w)
                    && matches!(this.model.input.as_str(), "!" | "`" | ">")
                {
                    this.return_home(w, cx);
                    cx.stop_propagation();
                }
            }))
            .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, w, cx| {
                if let Some(gesture) = this.resize {
                    let _ = gesture.update(w, this.zoom, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.resize = None;
                    let _ = this.relay.send(Event::SnapEnd);
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.resize = None;
                    if this.drag.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .when(!utility_active, |d| {
                d.child(
                    controls::header(t)
                        .gap(px(10. * t.zoom))
                        .child(
                            icons::search_badge(&mode, t.zoom)
                                .id("drag-handle")
                                .on_mouse_down(MouseButton::Left, {
                                    let relay = self.relay.clone();
                                    move |e: &MouseDownEvent, _, cx| {
                                        // The badge starts the move at once; the chrome arm must not re-start it.
                                        cx.stop_propagation();
                                        let _ = relay.send(Event::SnapBegin(Some((
                                            f32::from(e.position.x),
                                            f32::from(e.position.y),
                                        ))));
                                    }
                                }),
                        )
                        .when(mode == Mode::Deep, |d| {
                            d.child(
                                div()
                                    .mr(px(8.))
                                    .text_size(px(12.))
                                    .text_color(t.text3)
                                    .child("Deep"),
                            )
                        })
                        .when_some(
                            self.form.as_ref().filter(|form| form.inline()),
                            |d, form| {
                                d.child(
                                    div()
                                        .max_w(relative(0.4))
                                        .text_ellipsis()
                                        .text_size(px(15. * t.zoom))
                                        .child(form.row.title.clone()),
                                )
                                .child(controls::keycap(form.labels[0].title.clone(), t))
                            },
                        )
                        .child(
                            div()
                                .relative()
                                .flex()
                                .items_center()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    self.form
                                        .as_ref()
                                        .filter(|form| form.inline())
                                        .map(|form| form.inputs[0].clone())
                                        .unwrap_or_else(|| self.input.clone()),
                                )
                                // React `.path-command-hint`: an invisible copy of the query
                                // pushes "› Open terminal" to sit right after the typed path.
                                .when(mode == Mode::Path && self.form.is_none(), |d| {
                                    let value = self.input.read(cx).content.clone();
                                    d.child(
                                        div()
                                            .debug_selector(|| "path-command-hint".into())
                                            .absolute()
                                            .top_0()
                                            .bottom_0()
                                            .left_0()
                                            .right_0()
                                            .flex()
                                            .items_center()
                                            .gap(px(10. * t.zoom))
                                            .overflow_hidden()
                                            .text_size(px(15. * t.zoom))
                                            .line_height(px(22. * t.zoom))
                                            .child(div().flex_shrink_0().invisible().child(value))
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_shrink_0()
                                                    .items_center()
                                                    .h(px(20. * t.zoom))
                                                    .px(px(7. * t.zoom))
                                                    .rounded(px(6. * t.zoom))
                                                    .border_1()
                                                    .border_color(rgba(0xb4b1ff33))
                                                    .bg(rgba(0x7f76e61a))
                                                    .font_family(tezbar_ui::tokens::mono_family())
                                                    .text_size(px(11. * t.zoom))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(t.accent_strong)
                                                    .child(">"),
                                            )
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .text_ellipsis()
                                                    .text_color(t.text4)
                                                    .child("Open terminal"),
                                            ),
                                    )
                                }),
                        )
                        // React `.launcher-mode-chips` (`! Deep`, `> Terminal`, `Space AI`).
                        .when(mode == Mode::Basic && self.form.is_none(), |d| {
                            let chip = |mark: Option<&'static str>, label: &'static str| {
                                div()
                                    .flex()
                                    .flex_shrink_0()
                                    .items_center()
                                    .gap(px(4. * t.zoom))
                                    .h(px(20. * t.zoom))
                                    .px(px(7. * t.zoom))
                                    .rounded(px(6. * t.zoom))
                                    .bg(t.pill)
                                    .text_size(px(11. * t.zoom))
                                    .text_color(t.text3)
                                    .when_some(mark, |d, mark| {
                                        d.child(div().font_family(tezbar_ui::tokens::mono_family()).child(mark))
                                    })
                                    .child(label)
                            };
                            d.child(
                                div()
                                    .debug_selector(|| "launcher-mode-chips".into())
                                    .flex()
                                    .flex_row_reverse()
                                    .flex_shrink_0()
                                    .h(px(20. * t.zoom))
                                    .gap(px(6. * t.zoom))
                                    .overflow_hidden()
                                    .child(chip(None, "Space AI"))
                                    .child(chip(Some(">"), "Terminal"))
                                    .child(chip(Some("!"), "Deep")),
                            )
                        })
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
                                .role(Role::ComboBox)
                                .aria_expanded(self.model_menu)
                                .aria_label("Choose AI model")
                                .on_click(cx.listener(
                                    |this, _, w, cx| {
                                        this.open_models(cx);
                                        this.model_search.focus_handle(cx).focus(w, cx);
                                    },
                                )),
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
            })
            .when(self.history && !utility_active, |d| {
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
            .when(self.message_kind.is_some() && !utility_active, |d| {
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
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .when(
                        (!self.state.pins.is_empty() || dragging_result)
                            && matches!(mode, Mode::Basic | Mode::Deep)
                            && self.form.is_none()
                            && self.navigation.is_none()
                            && !self.answer_active,
                        |d| d.child(pins),
                    )
                    .child(content),
            )
            .when(self.model_menu, |d| d.child(self.model_popover(t, cx)))
            .when_some(self.menu.clone(), |d, (row, selected)| {
                d.child(
                    controls::popover("result-actions", "Result actions", t)
                        .role(Role::Menu)
                        .track_focus(&self.popover_focus)
                        .on_mouse_down_out(cx.listener(|this, _, w, cx| {
                            this.menu = None;
                            this.input.focus_handle(cx).focus(w, cx);
                            cx.notify();
                        }))
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
                                .role(Role::MenuItem)
                                .when(selected == index, |d| d.aria_active_descendant())
                                .w_full()
                                .on_click(cx.listener(
                                    move |this, _, w, cx| {
                                        this.input.focus_handle(cx).focus(w, cx);
                                        this.row_action(row.clone(), action.clone(), cx)
                                    },
                                ))
                            },
                        )),
                )
            })
            .when(!utility_active, |d| {
                // Full-width bar floating over the results (legacy `.launcher-footer`).
                d.child(
                    controls::footer(t)
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .bg(gpui::Hsla { a: 0.92, ..t.bar_float })
                        // React `CommandSurfaceFooter`: pl-2 pr-3, gap-x-3, 11px hints.
                        .pl(px(8. * t.zoom))
                        .pr(px(12. * t.zoom))
                        .gap(px(12. * t.zoom))
                        .text_size(px(11. * t.zoom))
                        .child(
                            div()
                                .id("launcher-status")
                                .role(Role::Status)
                                .when(self.error.is_some(), |d| d.text_color(t.danger))
                                .child(footer),
                        )
                        .when(self.error.is_some() && !self.ready, |d| {
                            let relay = self.relay.clone();
                            d.child(
                                controls::action_button(
                                    "retry-core",
                                    "Retry",
                                    controls::ButtonTone::Quiet,
                                    false,
                                    t,
                                    move |_, _| {
                                        let _ = relay.send(Event::RetryCore);
                                    },
                                )
                                .debug_selector(|| "retry-core".into()),
                            )
                        })
                        .child(div().flex_1())
                        .child(self.task_indicator.clone())
                        .children(self.footer_hints(&mode).into_iter().map(|(label, keys)| {
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6. * t.zoom))
                                .child(label)
                                .child(kbd_group(keys, t))
                        }))
                        .when(
                            self.form.is_none() && matches!(mode, Mode::Basic | Mode::Deep),
                            |d| {
                                d.child(
                                    div()
                                        .id("deep-search-hint")
                                        .role(Role::Button)
                                        .aria_label("Use Deep Search")
                                        .flex()
                                        .items_center()
                                        .gap(px(6. * t.zoom))
                                        .cursor_pointer()
                                        .when(mode == Mode::Deep, |d| d.text_color(rgb(0x67e8f9)))
                                        .when(mode != Mode::Deep, |d| d.hover(|d| d.text_color(t.text1)))
                                        .child("Deep Search")
                                        .child(kbd_group(chord("↵"), t))
                                        .on_click(cx.listener(|this, _, w, cx| {
                                            if launcher::input_mode(&this.model.input, this.model.terminal_mode).mode == Mode::Basic {
                                                this.enter(false, true, w, cx);
                                            }
                                        })),
                                )
                            },
                        ),
                )
            })
            .child(tracker)
            .children(resize::handles(cx, |view, edge, w, _| {
                view.resize = resize::Gesture::begin(edge, w)
            }))
            .into_any_element()
    }
}

/// Primary-modifier chord label: `⌘K` on macOS, `Ctrl+K` elsewhere.
fn chord(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("⌘{key}")
    } else if key.is_empty() {
        "Ctrl".into()
    } else {
        format!("Ctrl+{key}")
    }
}

/// React `.kbd-group`: one pill per key combination, 10px keys in text-2.
fn kbd_group(keys: impl Into<SharedString>, t: tokens::Tokens) -> Div {
    div()
        .flex()
        .items_center()
        .h(px(16. * t.zoom))
        .px(px(5. * t.zoom))
        .rounded(px(5. * t.zoom))
        .bg(t.pill_strong)
        .text_size(px(10. * t.zoom))
        .line_height(px(16. * t.zoom))
        .text_color(t.text2)
        .child(keys.into())
}

/// A press only becomes a move after real travel, so plain clicks on chips stay clicks.
pub(super) fn move_started(origin: Point<Pixels>, now: Point<Pixels>) -> bool {
    let (dx, dy) = (f32::from(now.x - origin.x), f32::from(now.y - origin.y));
    dx * dx + dy * dy >= 16.
}
#[cfg(test)]
mod move_zone_tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn travel_threshold_keeps_plain_clicks_as_clicks() {
        assert!(!move_started(point(px(10.), px(10.)), point(px(12.), px(12.))));
        assert!(move_started(point(px(10.), px(10.)), point(px(14.), px(10.))));
    }
}
