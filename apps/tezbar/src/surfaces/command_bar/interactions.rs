use super::*;
impl Launcher {
    pub(super) fn defer_icon_viewport(&mut self, cx: &mut Context<Self>) {
        let entity = cx.entity().downgrade();
        let window = self.window;
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, w, cx| {
                let _ = entity.update(cx, |this, cx| this.schedule_icon_viewport(w, cx));
            });
        });
    }
    pub(super) fn schedule_icon_viewport(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.icon_frame_pending
            || !self.visible
            || self.navigation.is_some()
            || self.form.is_some()
        {
            return;
        }
        self.icon_frame_pending = true;
        let entity = cx.entity().downgrade();
        w.on_next_frame(move |_, cx| {
            let _ = entity.update(cx, |this, cx| {
                this.icon_frame_pending = false;
                // A coalesced frame always reads the latest rows and viewport.
                this.refresh_icon_viewport(cx);
            });
        });
    }
    fn refresh_icon_viewport(&mut self, cx: &mut Context<Self>) {
        if !self.visible || self.navigation.is_some() || self.form.is_some() {
            return;
        }
        let mode = launcher::input_mode(&self.model.input, self.model.terminal_mode).mode;
        let columns = if mode == Mode::Applications { 7 } else { 1 };
        let row_height = if columns == 7 { 96. } else { 36. } * self.zoom;
        let (top, height) = if mode != Mode::Applications {
            (
                self.path_list.logical_scroll_top().item_ix,
                f32::from(self.path_list.viewport_bounds().size.height),
            )
        } else {
            let scroll = self.scroll.0.borrow();
            (
                ((-f32::from(scroll.base_handle.offset().y) / row_height)
                    .max(0.)
                    .floor() as usize)
                    * columns,
                f32::from(scroll.base_handle.bounds().size.height),
            )
        };
        let count = ((height / row_height).ceil().max(1.) as usize + 2) * columns;
        let start = top.saturating_sub(columns).min(self.rows.len());
        let range = start..start.saturating_add(count.min(108)).min(self.rows.len());
        if self.icon_viewport != range || self.icon_generation != self.model.session.generation {
            self.icon_viewport = range;
            self.icon_generation = self.model.session.generation;
            self.prepare_icons(cx);
            cx.notify();
        }
    }
    pub(super) fn retire_answer(&mut self, cx: &mut Context<Self>) {
        self.answer_active = false;
        self.answer_busy = false;
        self.answer_owner = None;
        self.answer_flush_pending = false;
        self.answer_text.clear();
        self.answer_revision += 1;
        let revision = self.answer_revision;
        self.answer
            .update(cx, |answer, cx| answer.set_content(revision, "", true, cx));
    }
    pub(super) fn clear_argument_results(&mut self, cx: &mut Context<Self>) {
        self.argument_revision += 1;
        self.argument_task = None;
        self.argument_rows.clear();
        self.model.update(Input::ArgumentValue(None), self.now());
        cx.notify();
    }
    pub(super) fn remember_query(&mut self, cx: &mut Context<Self>) {
        if let Some(query) = launcher::history_entry(&self.model.input, self.model.terminal_mode) {
            self.state.record_launcher_query(&query);
            self.persist(cx);
        }
    }
    pub(super) fn save_terminal_settings(&mut self, cx: &mut Context<Self>) {
        self.state.terminal_defaults = self.terminal_draft.clone();
        self.terminal_settings = false;
        self.persist(cx);
        self.message("Terminal defaults saved".into(), 2500, cx);
    }
    pub(super) fn terminal_id(&self) -> Option<String> {
        self.model
            .session
            .rows
            .get(self.model.session.selected)
            .and_then(|row| {
                if let Action::InvokeCommand { payload, .. } = &row.action {
                    payload["sessionId"].as_str().map(str::to_owned)
                } else {
                    None
                }
            })
    }
    pub(super) fn terminal_pin(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.terminal_id() {
            if self.state.terminal_pinned_sessions.contains(&id) {
                self.state.terminal_pinned_sessions.retain(|i| i != &id);
            } else {
                self.state.terminal_pinned_sessions.insert(0, id);
            }
            self.persist(cx);
            self.load_terminal(cx);
        }
    }
    pub(super) fn terminal_action(&mut self, delete: bool, cx: &mut Context<Self>) {
        if let Some(id) = self.terminal_id() {
            let request = if delete {
                Request::TerminalDelete(id)
            } else {
                Request::TerminalStop(id)
            };
            self.request_notice(request, cx);
        }
    }
    pub(super) fn open_models(&mut self, cx: &mut Context<Self>) {
        if self.model_saving {
            return;
        }
        self.model_menu = true;
        self.model_query.clear();
        let field = self.model_search.clone();
        let window = self.window;
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, w, cx| {
                field.update(cx, |field, cx| field.clear_silently(w, cx))
            });
        });
        self.model_provider = self.model_config["provider"]
            .as_str()
            .unwrap_or("ollama")
            .into();
        self.rebuild_models(cx);
        self.model_config_epoch += 1;
        self.model_preview_epoch += 1;
        let preview_epoch = self.model_preview_epoch;
        let epoch = self.model_config_epoch;
        let adapter = self.adapter.clone();
        self.model_config_task = Some(cx.spawn(async move |this, cx| {
            let result =
                gpui_tokio::Tokio::spawn(cx, async move { adapter.send(Request::LlmConfig).await })
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| {
                if !this.visible || epoch != this.model_config_epoch {
                    return;
                }
                match result {
                    Ok(config) => {
                        if this.model_preview_epoch == preview_epoch {
                            this.model_provider =
                                config["provider"].as_str().unwrap_or("ollama").into();
                        }
                        this.model_config = config;
                        this.rebuild_models(cx);
                    }
                    Err(e) => {
                        this.error = Some(e);
                        cx.notify();
                    }
                }
            });
        }));
    }
    pub(super) fn select_model(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((provider, model)) = self.model_choices.get(index).cloned() else {
            return;
        };
        if self.model_saving
            || !tezbar_ui_model::settings::providers::configured(&self.model_config, &provider)
        {
            return;
        }
        self.model_saving = true;
        self.model_menu = false;
        let field = self.input.clone();
        let window = self.window;
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, w, cx| field.focus_handle(cx).focus(w, cx));
        });
        let Some(store) = self.settings_store.clone() else {
            self.model_saving = false;
            self.error = Some("Settings owner is unavailable".into());
            cx.notify();
            return;
        };
        use tezbar_ui_model::settings::Edit;
        store.enqueue(
            self.settings_owner,
            vec![
                Edit::Select {
                    provider: provider.clone(),
                    model: model.clone(),
                },
                Edit::Activate { provider, model },
            ],
        );
        let adapter = self.adapter.clone();
        let owner = self.settings_owner;
        self.writes.push(cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move {
                store.drain_owner(owner).await?;
                adapter.send(Request::LlmConfig).await
            })
            .await
            .unwrap_or_else(|_| Err("Could not save selected model".into()));
            let _ = this.update(cx, |this, cx| {
                this.model_saving = false;
                match result {
                    Ok(config) => this.model_config = config,
                    Err(_) => {
                        this.error =
                            Some("Could not save selected model. Retry in Settings.".into())
                    }
                }
                cx.notify();
            });
        }));
    }
    /// Cmd+C on a Vault search result. Progress and the outcome are shown on that row.
    pub(super) fn vault_copy(&mut self, row_id: String, id: String, cx: &mut Context<Self>) {
        self.set_row_status(row_id.clone(), "Copying…", RowStatusTone::Busy, None, cx);
        let adapter = self.adapter.clone();
        let generation = self.model.session.generation;
        self.tasks.push(cx.spawn(async move |this, cx| {
            let status = gpui_tokio::Tokio::spawn(cx, {
                let adapter = adapter.clone();
                async move { adapter.send(Request::VaultStatus).await }
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
            let unlocked = match status {
                Ok(s) => s["unlocked"] == true,
                Err(e) => {
                    let _ = this.update(cx, |this, cx| {
                        this.row_status = None;
                        this.error = Some(e);
                        cx.notify();
                    });
                    return;
                }
            };
            if !unlocked {
                let current = this
                    .update(cx, |this, cx| {
                        let current = this.visible && generation == this.model.session.generation;
                        if current {
                            this.set_row_status(
                                row_id.clone(),
                                "Unlocking…",
                                RowStatusTone::Busy,
                                None,
                                cx,
                            );
                        } else {
                            this.row_status = None;
                        }
                        current
                    })
                    .unwrap_or(false);
                if !current {
                    return;
                }
                let result = gpui_tokio::Tokio::spawn(cx, {
                    let adapter = adapter.clone();
                    async move { adapter.send(Request::VaultUnlock).await }
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
                if !result.as_ref().is_ok_and(|v| v["ok"] != false) {
                    let row_id = row_id.clone();
                    let _ = this.update(cx, |this, cx| {
                        this.set_row_status(
                            row_id,
                            result.err().unwrap_or_else(|| "Unlock cancelled".into()),
                            RowStatusTone::Failed,
                            Some(3500),
                            cx,
                        );
                    });
                    return;
                }
            }
            let current = this
                .update(cx, |this, cx| {
                    let current = this.visible && generation == this.model.session.generation;
                    if !current {
                        this.row_status = None;
                        cx.notify();
                    }
                    current
                })
                .unwrap_or(false);
            if !current {
                return;
            }
            let result =
                gpui_tokio::Tokio::spawn(
                    cx,
                    async move { adapter.send(Request::VaultCopy(id)).await },
                )
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| {
                if this.visible && generation == this.model.session.generation {
                    let (text, tone) = match result {
                        Ok(v) if v["ok"] != false => {
                            ("Copied · clears in 30 s".into(), RowStatusTone::Done)
                        }
                        Ok(_) => ("Could not copy".into(), RowStatusTone::Failed),
                        Err(e) => (e, RowStatusTone::Failed),
                    };
                    this.set_row_status(row_id, text, tone, Some(4000), cx);
                } else {
                    this.row_status = None;
                    cx.notify();
                }
            });
        }));
    }
    pub(super) fn argument_changed(&mut self, index: usize, value: &str, cx: &mut Context<Self>) {
        let Some(form) = &mut self.form else {
            return;
        };
        form.args.selected = index;
        form.args.edit(value);
        self.model
            .update(Input::ArgumentValue(Some(value.into())), self.now());
        // The declared argument contribution has separate generation ownership from the base query.
        self.argument_revision += 1;
        let revision = self.argument_revision;
        let Some(form) = &self.form else {
            return;
        };
        let Action::RunExtensionCommand {
            extension_id,
            command_name,
            ..
        } = &form.row.action
        else {
            return;
        };
        let request = Request::ExtensionSearch(
            json!({"text":value,"surface":"argument","extensionId":extension_id,"commandName":command_name,"argumentName":form.args.fields[index].name}),
        );
        let adapter = self.adapter.clone();
        self.argument_task = Some(cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move {
                let value = adapter.send(request).await?;
                serde_json::from_value::<Vec<Row>>(value).map_err(|e| e.to_string())
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| {
                if this.form.is_some() && this.argument_revision == revision {
                    this.argument_rows = result.unwrap_or_default();
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }
}
