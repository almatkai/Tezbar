use super::*;
impl Launcher {
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
        let adapter = self.adapter.clone();
        let generation = self.model.session.generation;
        self.tasks.push(cx.spawn(async move |this, cx| {
            let result =
                gpui_tokio::Tokio::spawn(cx, async move { adapter.send(Request::LlmConfig).await })
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| {
                if !this.visible || generation != this.model.session.generation {
                    return;
                }
                match result {
                    Ok(config) => {
                        let defaults = tezbar_ui_model::settings::provider_catalog();
                        let mut choices = Vec::new();
                        let mut providers = defaults["RECOMMENDED_AI_MODEL"]
                            .as_object()
                            .into_iter()
                            .flatten()
                            .map(|(id, _)| id.clone())
                            .collect::<Vec<_>>();
                        for p in config["customProviders"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|p| p["id"].as_str())
                        {
                            if !providers.iter().any(|id| id == p) {
                                providers.push(p.into());
                            }
                        }
                        for provider in providers {
                            let models =
                                config["providerModels"][&provider].as_array().or_else(|| {
                                    defaults["DEFAULT_PROVIDER_MODELS"][&provider].as_array()
                                });
                            for model in models.into_iter().flatten().take(100) {
                                if let Some(id) = model["id"].as_str() {
                                    choices.push((provider.clone(), id.to_owned()));
                                }
                            }
                        }
                        this.model_choices = choices;
                        this.model_choice = 0;
                        this.model_config = config;
                    }
                    Err(e) => this.error = Some(e),
                }
                cx.notify();
            });
        }));
    }
    pub(super) fn select_model(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((provider, model)) = self.model_choices.get(index).cloned() else {
            return;
        };
        if self.model_saving {
            return;
        }
        self.model_saving = true;
        self.model_menu = false;
        // Use the M3 write owner to merge only selected keys into a freshly read config.
        // A model picker never writes a stale cached provider/credential map.
        let adapter = self.adapter.clone();
        let lock = self.state_writer.clone();
        self.writes.push(cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move {
                let _guard = lock.lock().await;
                use tezbar_ui_model::settings::{Edit, Effect, WriteQueue};
                let mut queue = WriteQueue::default();
                queue.register(1);
                queue.enqueue(
                    1,
                    vec![
                        Edit::Select {
                            provider: provider.clone(),
                            model: model.clone(),
                        },
                        Edit::Activate { provider, model },
                    ],
                );
                let Some(Effect::Read { token }) = queue.next_effect() else {
                    return Err("Model selection did not produce a config read".into());
                };
                let current = adapter.send(Request::LlmConfig).await?;
                let Some(Effect::Save { token, patch }) = queue.read_result(token, Ok(current))
                else {
                    return Err("Model selection could not merge the latest config".into());
                };
                let result = adapter.send(Request::LlmSet(patch)).await?;
                let success = result["ok"] != false;
                queue.finish(token, success);
                if !success {
                    return Err(result["message"]
                        .as_str()
                        .unwrap_or("Could not save selected model")
                        .into());
                }
                adapter.send(Request::LlmConfig).await
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| {
                this.model_saving = false;
                *this.write_error.lock().unwrap() = result.as_ref().err().cloned();
                match result {
                    Ok(config) => this.model_config = config,
                    Err(e) => this.error = Some(e),
                }
                cx.notify();
            });
        }));
    }
    pub(super) fn vault_copy(&mut self, id: String, cx: &mut Context<Self>) {
        self.message("Copying…".into(), 2500, cx);
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
                            this.message("Waiting for Touch ID…".into(), 9000, cx);
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
                    let _ = this.update(cx, |this, cx| {
                        this.message(
                            result.err().unwrap_or_else(|| "Unlock cancelled".into()),
                            2500,
                            cx,
                        );
                    });
                    return;
                }
            }
            let current = this
                .update(cx, |this, _| {
                    this.visible && generation == this.model.session.generation
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
                    this.message(
                        match result {
                            Ok(v) if v["ok"] != false => "Copied · clears in 30 s".into(),
                            Ok(_) => "Could not copy the secret".into(),
                            Err(e) => e,
                        },
                        2500,
                        cx,
                    );
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
