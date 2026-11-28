//! Retained modal editors. All mutations still go through the existing settings models.
use super::panels::{card, line, note, stack};
use super::*;
use tezbar_ui::tokens::Tokens;
impl SettingsContent {
    pub(super) fn import_configuration(&mut self, cx: &mut Context<Self>) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import AI extension configuration (.json)".into()),
        });
        self.next_job += 1;
        let job = self.next_job;
        let window = self.window;
        let task = cx.spawn(async move |this, cx| {
            let path = prompt
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
                .and_then(|mut paths| paths.pop());
            let result = if let Some(path) = path {
                Some(
                    cx.background_executor()
                        .spawn(async move {
                            let metadata = std::fs::metadata(&path).map_err(|_| {
                                "Could not read the selected configuration.".to_owned()
                            })?;
                            if metadata.len() > 2 * 1024 * 1024 {
                                return Err("Configuration exceeds the 2 MB import limit.".into());
                            }
                            std::fs::read_to_string(path).map_err(|_| {
                                "Could not read the selected configuration as UTF-8.".to_owned()
                            })
                        })
                        .await,
                )
            } else {
                None
            };
            let _ = cx.update_window(window, |_, w, cx| {
                let _ = this.update(cx, |v, cx| {
                    v.jobs.remove(&job);
                    match result {
                        Some(Ok(value)) => {
                            v.action(Action::Dialog(Some(SettingsDialog::Import)), w, cx);
                            v.action(Action::SetField("import".into(), value), w, cx);
                        }
                        Some(Err(error)) => v.error = Some(error),
                        None => {}
                    }
                    cx.notify();
                });
            });
        });
        self.jobs.insert(job, task);
    }
    pub(super) fn render_dialog(&self, t: Tokens, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.dialog.as_ref()?;
        let (title, body) = match dialog {
            SettingsDialog::CustomProvider => {
                let protocol = self.value("provider.protocol");
                (
                    "Add custom provider".into(),
                    stack()
                        .child(self.labeled("provider.name", "Provider name"))
                        .child(self.labeled("provider.url", "Base URL"))
                        .child(
                            self.select(
                                "Custom provider protocol",
                                if protocol.is_empty() {
                                    "openai-chat"
                                } else {
                                    &protocol
                                },
                                [
                                    "openai-chat",
                                    "openai-responses",
                                    "anthropic",
                                    "gemini",
                                    "ollama",
                                ]
                                .into_iter()
                                .map(|p| {
                                    (
                                        p.into(),
                                        Action::SetField("provider.protocol".into(), p.into()),
                                    )
                                })
                                .collect(),
                                t,
                                cx,
                            ),
                        )
                        .child(self.primary("Create", Action::CreateNamedProvider, t, cx)),
                )
            }
            SettingsDialog::Model(id) => {
                let key = format!("model.{}.{id}", self.provider.provider);
                (format!("Edit {id}"),stack().child(note("Model metadata, capabilities, pricing, context window, and picker visibility.",t)).child(self.labeled(&key,"Model metadata (JSON)"))
                    .child(self.b("Save model",Action::SaveModel(id.clone()),t,cx)))
            }
            SettingsDialog::Connection => {
                let transport = self.value("connection.transport");
                let mut d = stack()
                    .child(self.labeled("connection.name", "Name"))
                    .child(
                        line()
                            .child(self.choice(
                                "Program on this computer",
                                Action::SetField("connection.transport".into(), "stdio".into()),
                                t,
                                cx,
                            ))
                            .child(self.choice(
                                "Web address",
                                Action::SetField("connection.transport".into(), "http".into()),
                                t,
                                cx,
                            )),
                    )
                    .child(note("Connect any program or service that supports MCP.", t));
                if transport == "http" {
                    d = d.child(self.labeled("connection.url", "Web address"));
                } else {
                    d =
                        d.child(self.labeled("connection.command", "Command"))
                            .child(self.labeled(
                                "connection.timeout",
                                "Time limit, seconds (default: 120)",
                            ))
                            .child(self.labeled("connection.args", "Arguments, one per line"))
                            .child(self.labeled("connection.cwd", "Working directory (optional)"))
                            .child(self.labeled(
                                "connection.env",
                                "Environment variables (JSON, optional)",
                            ));
                }
                d = d.child(self.primary("Review connection", Action::PreviewConnection, t, cx));
                if let Some(preview) = &self.contributions.preview {
                    d = d
                        .child(
                            card(t)
                                .child("Review connection")
                                .child(view::safe_display(preview)),
                        )
                        .child(self.primary(
                            "Add reviewed connection",
                            Action::Contribution(ContributionIntent::AddPreview),
                            t,
                            cx,
                        ));
                }
                ("Add connection".into(), d)
            }
            SettingsDialog::Import => {
                let mut d=stack().child(note("Paste an MCP configuration or AI extension manifest. Preview the command, arguments, environment and requested tools before importing.",t))
                    .child(self.labeled("import","Paste mcpServers or manifest"))
                    .child(self.primary("Preview import",Action::Contribution(ContributionIntent::Preview),t,cx));
                if let Some(preview) = &self.contributions.preview {
                    d = d
                        .child(
                            card(t)
                                .child("Review import")
                                .child(view::safe_display(preview)),
                        )
                        .child(self.primary(
                            "Add reviewed import",
                            Action::Contribution(ContributionIntent::AddPreview),
                            t,
                            cx,
                        ));
                }
                ("Add AI extension".into(), d)
            }
            SettingsDialog::Permissions => {
                ("System Access".into(), self.render_permission_cards(t, cx))
            }
            SettingsDialog::Preferences(id, command) => {
                let mut d = stack();
                if let Some((_, v)) = self.preferences.get(&(id.clone(), command.clone())) {
                    if v.is_null() {
                        d = d.child(note("Loading preferences…", t));
                    }
                    for f in v["preferences"].as_array().into_iter().flatten() {
                        if command.is_some() && f["commandName"].as_str() != command.as_deref() {
                            continue;
                        }
                        let name = text(&f["name"]);
                        let label = f["title"].as_str().unwrap_or(&name);
                        let key = format!(
                            "pref.{id}.{}.{name}",
                            command.as_deref().unwrap_or("$extension")
                        );
                        if f["type"] == "checkbox" {
                            let checked = self.value(&key) == "true";
                            d = d.child(
                                self.button(
                                    format!("preference-{key}"),
                                    label.to_owned(),
                                    Action::PreferenceValue(key, json!(!checked)),
                                    t,
                                    cx,
                                )
                                .role(Role::CheckBox)
                                .aria_toggled(if checked {
                                    Toggled::True
                                } else {
                                    Toggled::False
                                }),
                            );
                        } else if f["type"] == "dropdown" {
                            let current = self.value(&key);
                            let options = f["data"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .map(|o| {
                                    (
                                        o["title"].as_str().unwrap_or("").to_owned(),
                                        Action::PreferenceValue(key.clone(), o["value"].clone()),
                                    )
                                })
                                .collect();
                            d = d
                                .child(note(label.to_owned(), t))
                                .child(self.select(&key, &current, options, t, cx));
                        } else {
                            d = d.child(self.labeled(&key, label));
                        }
                        if let Some(desc) = f["description"].as_str() {
                            d = d.child(note(desc.to_owned(), t));
                        }
                    }
                    d = d.child(self.primary(
                        "Save preferences",
                        Action::SavePreferences(id.clone(), command.clone()),
                        t,
                        cx,
                    ));
                } else {
                    d = d.child(note("Loading preferences…", t));
                }
                ("Preferences".into(), d)
            }
            SettingsDialog::Extension { id, section } => {
                let e = self.contributions.display["extensions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|e| e["id"] == *id)
                    .cloned()
                    .unwrap_or(Value::Null);
                let name = e["name"].as_str().unwrap_or("AI extension");
                let title = format!("{name} — {}", view::display_name(section));
                let mut d = stack();
                let mut tabs = line();
                for (s, label) in [
                    ("config", "Configuration"),
                    ("credentials", "Credentials"),
                    ("permissions", "Permissions"),
                    ("tools", "Tools"),
                    ("oauth", "Sign in"),
                ] {
                    tabs = tabs.child(
                        self.button(
                            format!("extension-dialog-{s}"),
                            label,
                            Action::Dialog(Some(SettingsDialog::Extension {
                                id: id.clone(),
                                section: s.into(),
                            })),
                            t,
                            cx,
                        )
                        .role(Role::Tab)
                        .aria_selected(section == s)
                        .when(section == s, |b| b.bg(t.pill_strong)),
                    );
                }
                d = d.child(tabs);
                use ContributionIntent as I;
                match section.as_str() {
                    "credentials" => {
                        d=d.child(note("Saved values are never shown. Leave a field blank to keep its current credential.",t));
                        let slots = self
                            .secret_slots
                            .get(id)
                            .map(Vec::as_slice)
                            .unwrap_or_default();
                        if slots.is_empty() {
                            d = d.child(note(
                                "No credential fields requested by this extension.",
                                t,
                            ));
                        }
                        for (i, descriptor) in slots.iter().enumerate() {
                            d = d
                                .child(self.labeled(
                                    &format!("secret.slot.{id}.{i}"),
                                    &format!("{} · {}", descriptor.label, descriptor.scope),
                                ))
                                .child(self.button(
                                    format!("save-secret-{i}"),
                                    format!("Save {}", descriptor.label),
                                    Action::Contribution(I::SaveSavedSecret(id.clone(), i)),
                                    t,
                                    cx,
                                ));
                        }
                        d = d.child(self.b(
                            "Refresh credential fields",
                            Action::Contribution(I::ReloadSecretFields(id.clone())),
                            t,
                            cx,
                        ));
                    }
                    "config" => {
                        for (i, c) in e["connections"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .enumerate()
                        {
                            let mut connection = card(t).child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(c["name"].as_str().unwrap_or("Connection").to_owned()),
                            );
                            for field in ["command", "args", "url"] {
                                if field == "url" && c["transport"] == "stdio" {
                                    continue;
                                }
                                connection = connection
                                    .child(self.labeled(
                                        &format!("connection.{id}.{i}.{field}"),
                                        &view::display_name(field),
                                    ))
                                    .child(self.button(
                                        format!("connection-save-{i}-{field}"),
                                        format!("Save {}", view::display_name(field)),
                                        Action::Contribution(I::SaveRow(
                                            id.clone(),
                                            "connections".into(),
                                            i,
                                            field.into(),
                                        )),
                                        t,
                                        cx,
                                    ));
                            }
                            d = d.child(connection);
                        }
                        d = d.child(note("Advanced manifest configuration", t));
                        for collection in
                            ["connections", "providers", "prompts", "commands", "agents"]
                        {
                            d = d
                                .child(self.labeled(
                                    &format!("manifest.{id}.{collection}"),
                                    &format!("{} (JSON)", view::display_name(collection)),
                                ))
                                .child(self.button(
                                    format!("manifest-save-{collection}"),
                                    format!("Save {collection}"),
                                    Action::Contribution(I::SaveCollection(
                                        id.clone(),
                                        collection.into(),
                                    )),
                                    t,
                                    cx,
                                ));
                        }
                        d = d.child(self.labeled(
                            "allow.confirm",
                            "Type the extension name to allow all tools",
                        ));
                    }
                    "permissions" => {
                        d=d.child(note("Review commands, environment variables, requested tools, schemas and trust pins before enabling. Changed tools remain disabled until accepted.",t));
                        for c in e["connections"].as_array().into_iter().flatten() {
                            d = d.child(card(t).child(view::safe_display(c)));
                        }
                        d = d
                            .child(self.b(
                                "Review changes",
                                Action::Contribution(I::Review(id.clone())),
                                t,
                                cx,
                            ))
                            .child(self.b(
                                "Keep disabled",
                                Action::Contribution(I::Enable(id.clone(), false)),
                                t,
                                cx,
                            ));
                        if let Some(review) = self.review.as_ref().filter(|r| r["id"] == *id) {
                            d = d
                                .child(card(t).child(view::safe_display(review)))
                                .child(self.b(
                                    "Accept exact reviewed changes",
                                    Action::Contribution(I::Accept(id.clone())),
                                    t,
                                    cx,
                                ));
                        }
                    }
                    "tools" => {
                        let mut count = 0;
                        for c in e["connections"].as_array().into_iter().flatten() {
                            let cid = text(&c["id"]);
                            for tool in c["catalog"]["tools"].as_array().into_iter().flatten() {
                                count += 1;
                                let tool_name = text(&tool["name"]);
                                let enabled = c["disabledTools"][&tool_name] != true;
                                d = d.child(
                                    card(t)
                                        .child(
                                            line()
                                                .justify_between()
                                                .child(tool_name.clone())
                                                .child(
                                                    self.button(
                                                        format!("tool-toggle-{cid}-{tool_name}"),
                                                        if enabled {
                                                            "Enabled"
                                                        } else {
                                                            "Disabled"
                                                        },
                                                        Action::Contribution(I::SetTool(
                                                            id.clone(),
                                                            tool_name.clone(),
                                                            !enabled,
                                                            Some(cid.clone()),
                                                        )),
                                                        t,
                                                        cx,
                                                    )
                                                    .role(Role::Switch)
                                                    .aria_toggled(if enabled {
                                                        Toggled::True
                                                    } else {
                                                        Toggled::False
                                                    }),
                                                ),
                                        )
                                        .child(note(text(&tool["description"]), t))
                                        .child(note(view::safe_display(&tool["inputSchema"]), t))
                                        .child(self.button(
                                            format!("tool-reset-{cid}-{tool_name}"),
                                            "Ask again before using",
                                            Action::Contribution(I::Operation(
                                                id.clone(),
                                                ContributionAction::ClearAllowance,
                                                vec![json!(id), json!(cid), json!(tool_name)],
                                            )),
                                            t,
                                            cx,
                                        )),
                                );
                            }
                            for r in c["catalog"]["resources"].as_array().into_iter().flatten() {
                                let uri = text(&r["uri"]);
                                d = d.child(self.button(
                                    format!("resource-{cid}-{uri}"),
                                    format!("Read {}", r["name"].as_str().unwrap_or(&uri)),
                                    Action::Contribution(I::Operation(
                                        id.clone(),
                                        ContributionAction::ReadResource,
                                        vec![
                                            json!({"extensionId":id,"connectionId":cid,"uri":uri}),
                                        ],
                                    )),
                                    t,
                                    cx,
                                ));
                            }
                            for p in c["catalog"]["prompts"].as_array().into_iter().flatten() {
                                let name = text(&p["name"]);
                                d=d.child(self.button(format!("prompt-{cid}-{name}"),format!("Preview {name}"),Action::Contribution(I::Operation(id.clone(),ContributionAction::PromptResolve,vec![json!({"extensionId":id,"connectionId":cid,"name":name}),json!({})])),t,cx));
                            }
                        }
                        if count == 0 {
                            d=d.child(note("No tools discovered. Test this connection to load its catalog.",t)).child(self.b("Test connection",Action::Contribution(I::Test(id.clone())),t,cx));
                        }
                    }
                    "oauth" => {
                        for c in e["connections"].as_array().into_iter().flatten() {
                            let cid = text(&c["id"]);
                            let mut row = card(t)
                                .child(c["name"].as_str().unwrap_or("Connection").to_owned())
                                .child(note(view::safe_display(&c["oauth"]), t));
                            for (label, action) in [
                                ("Sign in", ContributionAction::OAuthStart),
                                ("Check sign-in", ContributionAction::OAuthStatus),
                                ("Cancel sign-in", ContributionAction::OAuthCancel),
                            ] {
                                row = row.child(self.button(
                                    format!("oauth-{cid}-{label}"),
                                    label,
                                    Action::Contribution(I::Operation(
                                        id.clone(),
                                        action,
                                        vec![json!(id), json!(cid)],
                                    )),
                                    t,
                                    cx,
                                ));
                            }
                            d = d.child(row);
                        }
                    }
                    _ => {}
                }
                d = d.child(self.b(
                    "Show logs",
                    Action::Contribution(I::Operation(
                        id.clone(),
                        ContributionAction::Logs,
                        vec![json!(id)],
                    )),
                    t,
                    cx,
                ));
                if let Some((_, result)) = &self.contributions.result {
                    d = d.child(note(view::safe_display(result), t));
                }
                (title, d)
            }
        };
        let weak = cx.entity().downgrade();
        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .p(px(24.))
                .bg(rgba(0x00000066))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .id("settings-modal")
                        .track_focus(&self.dialog_focus)
                        .tab_group()
                        .tab_stop(false)
                        .role(Role::Dialog)
                        .aria_label(title.clone())
                        .w_full()
                        .max_w(px(680.))
                        .max_h(relative(0.92))
                        .flex()
                        .flex_col()
                        .rounded(px(12.))
                        .border_1()
                        .border_color(t.rim)
                        .bg(t.popover_solid)
                        .overflow_hidden()
                        .child(
                            line()
                                .flex_shrink_0()
                                .justify_between()
                                .px(px(20.))
                                .py(px(14.))
                                .border_b_1()
                                .border_color(t.hairline)
                                .child(
                                    div()
                                        .text_size(px(14.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(title),
                                )
                                .child(
                                    tezbar_ui::controls::action_button(
                                        "close-settings-modal",
                                        "×",
                                        tezbar_ui::controls::ButtonTone::Ghost,
                                        false,
                                        t,
                                        move |w, cx| {
                                            let _ = weak.update(cx, |v, cx| {
                                                v.action(Action::Dialog(None), w, cx)
                                            });
                                        },
                                    )
                                    .aria_label("Close dialog"),
                                ),
                        )
                        .child(
                            div()
                                .id("settings-modal-content")
                                .flex_1()
                                .min_h(px(0.))
                                .overflow_y_scroll()
                                .p(px(20.))
                                .child(body),
                        )
                        .when_some(self.error.clone(), |d, e| {
                            d.child(
                                div()
                                    .id("settings-modal-error")
                                    .role(Role::Alert)
                                    .px(px(20.))
                                    .pb(px(12.))
                                    .text_size(px(12.))
                                    .text_color(t.danger)
                                    .child(e),
                            )
                        }),
                )
                .into_any_element(),
        )
    }
}
