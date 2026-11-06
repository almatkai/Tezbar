use super::*;
use tezbar_ui::{
    controls::{self, ButtonTone},
    tokens::{self, Tokens},
};
impl SettingsContent {
    fn button(
        &self,
        id: String,
        label: impl Into<SharedString>,
        action: Action,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let view = cx.entity().downgrade();
        controls::action_button(
            id.clone(),
            label,
            ButtonTone::Quiet,
            false,
            t,
            move |w, cx| {
                let _ = view.update(cx, |v, cx| v.action(action.clone(), w, cx));
            },
        )
        .debug_selector(move || id.clone())
    }
    fn b(&self, label: &str, action: Action, t: Tokens, cx: &mut Context<Self>) -> Stateful<Div> {
        self.button(
            match &action {
                Action::Knowledge(
                    knowledge::Operation::RemoveRoot(id)
                    | knowledge::Operation::RootEnabled { id, .. },
                ) => format!("settings-{id}-{label}"),
                Action::Knowledge(knowledge::Operation::RootDepth { id, depth }) => {
                    format!("settings-{id}-depth-{depth}")
                }
                Action::CommandAlias(id)
                | Action::CommandHotkey(id)
                | Action::CommandEnabled(id, _)
                | Action::Uninstall(id)
                | Action::Preferences(id, _) => format!("settings-{id}-{label}"),
                Action::Contribution(intent) => {
                    use ContributionIntent::*;
                    let scope = match intent {
                        Select(id)
                        | Test(id)
                        | Enable(id, _)
                        | Remove(id)
                        | Review(id)
                        | Accept(id)
                        | SaveCollection(id, _)
                        | ReloadSecretFields(id)
                        | SaveSecret(id, _, _) => id.clone(),
                        SaveRow(id, collection, index, field) => {
                            format!("{id}-{collection}-{index}-{field}")
                        }
                        SaveSavedSecret(id, index) => format!("{id}-secret-{index}"),
                        SetTool(id, tool, _, connection) => format!("{id}-{connection:?}-{tool}"),
                        Operation(id, action, args) => format!(
                            "{id}-{:?}-{}",
                            std::mem::discriminant(action),
                            args.iter()
                                .find_map(|v| v.get("connectionId").and_then(Value::as_str))
                                .or_else(|| {
                                    if matches!(
                                        action,
                                        ContributionAction::OAuthStart
                                            | ContributionAction::OAuthStatus
                                            | ContributionAction::OAuthCancel
                                            | ContributionAction::ClearAllowance
                                    ) {
                                        args.get(1).and_then(Value::as_str)
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or("")
                        ),
                        _ => String::new(),
                    };
                    format!("settings-{scope}-{label}")
                }
                _ => format!("settings-{label}"),
            },
            label.to_string(),
            action,
            t,
            cx,
        )
    }
    pub(super) fn choice_selected(&self, action: &Action) -> bool {
        match action {
            Action::Theme(theme) => self.state.theme == *theme,
            Action::UpdateChannel(channel) => self.update_channel == *channel,
            Action::Knowledge(knowledge::Operation::Depth(depth)) => self
                .knowledge
                .snapshot
                .as_ref()
                .is_some_and(|s| s.settings.depth == *depth),
            Action::Knowledge(knowledge::Operation::Speed(speed)) => self
                .knowledge
                .snapshot
                .as_ref()
                .is_some_and(|s| s.settings.indexing_speed == *speed),
            Action::Knowledge(knowledge::Operation::RootDepth { id, depth }) => {
                self.knowledge.snapshot.as_ref().is_some_and(|s| {
                    s.roots.iter().any(|r| {
                        r["id"].as_str() == Some(id)
                            && r["depth"].as_str().unwrap_or("inherit") == depth
                    })
                })
            }
            _ => false,
        }
    }
    fn choice(
        &self,
        label: &str,
        action: Action,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = self.choice_selected(&action);
        self.b(label, action, t, cx)
            .role(Role::RadioButton)
            .aria_toggled(if selected {
                Toggled::True
            } else {
                Toggled::False
            })
            .when(selected, |button| button.bg(t.pill_strong))
    }
    fn field_view(&self, id: &str) -> AnyElement {
        self.fields
            .get(id)
            .map(|f| {
                div()
                    .min_h(px(32.))
                    .child(f.input.clone())
                    .into_any_element()
            })
            .unwrap_or_else(|| div().child("Loading…").into_any_element())
    }
    fn labeled(&self, id: &str, label: &str) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(div().text_size(px(12.)).child(label.to_string()))
            .child(self.field_view(id))
    }
    fn row(&self, label: &str, detail: &str, t: Tokens) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .py(px(13.))
            .border_b_1()
            .border_color(t.hairline)
            .child(
                div()
                    .id(format!("heading-{label}"))
                    .role(Role::Heading)
                    .text_size(px(13.))
                    .child(label.to_string()),
            )
            .when(!detail.is_empty(), |d| {
                d.child(
                    div()
                        .text_color(t.text2)
                        .text_size(px(12.))
                        .child(detail.to_string()),
                )
            })
    }
    fn toggle(
        &self,
        label: &str,
        key: &'static str,
        checked: bool,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let view = cx.entity().downgrade();
        controls::checkbox(format!("toggle-{key}"), label.to_string(), checked, t).on_click(
            move |_, w, cx| {
                let _ = view.update(cx, |v, cx| v.action(Action::Toggle(key, !checked), w, cx));
            },
        )
    }
    pub(super) fn tab_control(
        &self,
        index: usize,
        tab: Tab,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.b(tab.title(), Action::Tab(tab), t, cx)
            .role(Role::Tab)
            .aria_selected(self.tab == tab)
            .track_focus(&self.tabs[index])
            .tab_index(if self.tab == tab { 0 } else { -1 })
            .when(self.tab == tab, |d| d.bg(t.pill_strong))
    }
    fn general(&mut self, t: Tokens, cx: &mut Context<Self>) -> Div {
        let mut d = div().flex().flex_col().gap(px(4.));
        let name = self
            .layout
            .as_ref()
            .map(|(n, _)| n.as_str())
            .unwrap_or("Loading display…");
        let mut appearance = self
            .row("Launcher appearance", name, t)
            .child(self.labeled("layout.width", "Width"))
            .child(self.labeled("layout.height", "Height"))
            .child(self.b("Apply size", Action::Layout, t, cx));
        let current_zoom = self.layout.as_ref().map(|(_, p)| p.zoom).unwrap_or(1.0);
        let zoom = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(self.b(
                "Decrease scale",
                Action::Zoom((current_zoom - 0.05).max(0.75)),
                t,
                cx,
            ))
            .child(format!("{:.0}%", current_zoom * 100.))
            .child(self.b(
                "Increase scale",
                Action::Zoom((current_zoom + 0.05).min(2.0)),
                t,
                cx,
            ));
        appearance = appearance.child(zoom).child(self.b(
            "Reset size & scale for this display",
            Action::ResetLayout,
            t,
            cx,
        ));
        d = d.child(appearance);
        d = d.child(
            self.row(
                "Startup",
                "Keeps the launcher surface alive between quick open and close cycles.",
                t,
            )
            .child(
                controls::checkbox("startup", "Launch Tezbar at login", true, t)
                    .tab_index(-1)
                    .aria_description("Read only"),
            ),
        );
        d = d.child(
            self.row(
                "Onboarding",
                "Show the onboarding tour on the next app launch.",
                t,
            )
            .child(
                self.b(
                    "Run onboarding on next launch",
                    Action::Toggle(
                        "hasCompletedOnboarding",
                        self.config["hasCompletedOnboarding"] == false,
                    ),
                    t,
                    cx,
                )
                .role(Role::CheckBox)
                .aria_toggled(if self.config["hasCompletedOnboarding"] == false {
                    Toggled::True
                } else {
                    Toggled::False
                }),
            ),
        );
        d = d.child(
            self.row(
                "Tezbar Hotkey",
                "Press a modifier and another key. Escape cancels; Backspace restores the default.",
                t,
            )
            .child(self.b(
                if self.hotkey_recording {
                    "Press shortcut…"
                } else {
                    self.config["raymesHotkey"]
                        .as_str()
                        .unwrap_or("Record shortcut")
                },
                Action::RecordHotkey,
                t,
                cx,
            )),
        );
        for (id, title) in [
            ("uiStateRetentionMs", "Last Screen"),
            ("extensionRuntimeTimeoutMs", "Extension Return"),
            ("aiModeTimeoutMs", "AI Return"),
            ("terminalModeTimeoutMs", "Terminal Return"),
        ] {
            d = d.child(
                self.row(
                    title,
                    if id == "uiStateRetentionMs" {
                        "Seconds"
                    } else {
                        "Minutes; 0 means Never."
                    },
                    t,
                )
                .child(self.field_view(id)),
            );
        }
        let mut themes = self.row("Appearance", "Applies to all Tezbar windows.", t);
        for (label, theme) in [
            ("System", Theme::System),
            ("White", Theme::White),
            ("Dark", Theme::Dark),
        ] {
            themes = themes.child(self.choice(label, Action::Theme(theme), t, cx));
        }
        d = d.child(themes);
        d = d.child(
            self.row(
                "Currency conversion",
                "Leave blank to use your frequent target currency.",
                t,
            )
            .child(self.field_view("currency"))
            .child(self.b("Save currency", Action::Currency, t, cx)),
        );
        let mut updates = self
            .row("Updates", &update_label(&self.update_status), t)
            .child(self.b(
                "Check for updates",
                Action::Update(UpdateAction::Check),
                t,
                cx,
            ));
        match &self.update_status {
            tezbar_update::AppUpdateStatus::Available { .. } => {
                updates = updates.child(self.b(
                    "Download update",
                    Action::Update(UpdateAction::Download),
                    t,
                    cx,
                ))
            }
            tezbar_update::AppUpdateStatus::Ready { .. } => {
                updates = updates.child(self.b(
                    "Restart to update",
                    Action::Update(UpdateAction::Restart),
                    t,
                    cx,
                ))
            }
            tezbar_update::AppUpdateStatus::ManualAvailable { installer_url, .. } => {
                updates = updates.child(self.b(
                    "Download installer",
                    Action::Update(UpdateAction::Installer(installer_url.clone())),
                    t,
                    cx,
                ))
            }
            _ => {}
        }
        updates = updates
            .child(self.b(
                if self.state.auto_update_enabled {
                    "Disable automatic updates"
                } else {
                    "Enable automatic updates"
                },
                Action::AutoUpdate(!self.state.auto_update_enabled),
                t,
                cx,
            ))
            .child(self.b("Release notes", Action::ReleaseNotes, t, cx));
        for channel in ["stable", "beta", "alpha"] {
            updates = updates.child(self.choice(
                &display_name(channel),
                Action::UpdateChannel(channel.into()),
                t,
                cx,
            ));
        }
        d.child(updates)
    }
    fn ai(&mut self, t: Tokens, cx: &mut Context<Self>) -> Div {
        let mut d = div().flex().flex_col().gap(px(10.));
        let mut providers = div().flex().flex_wrap().gap(px(5.));
        let mut ids = settings::provider_catalog()["RECOMMENDED_AI_MODEL"]
            .as_object()
            .map(|m| m.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        ids.extend(
            self.config["customProviders"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v["id"].as_str().map(str::to_string)),
        );
        for p in ids {
            providers = providers.child(
                self.b(
                    &format!(
                        "{p}{}",
                        if self.config["provider"] == p {
                            " · Active"
                        } else if settings::providers::configured(&self.config, &p) {
                            " · Configured"
                        } else {
                            ""
                        }
                    ),
                    Action::Provider(p.clone()),
                    t,
                    cx,
                )
                .role(Role::Tab)
                .aria_selected(self.provider.provider == p),
            );
        }
        d = d.child(providers);
        let p = self.provider.provider.clone();
        let managed = self.config["customProviders"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|v| v["id"] == p && v.get("_extensionId").is_some());
        d = d.child(self.row(
            &format!("{p} configuration"),
            if managed {
                "Managed by AI Extension. Edit its provider and credentials in AI Extensions."
            } else {
                "Credentials are scoped to this provider."
            },
            t,
        ));
        if !managed {
            for (field, label) in [
                ("protocol", "Protocol"),
                ("baseURL", "Base URL"),
                ("apiKey", "API key"),
                ("headers", "Headers JSON"),
                ("githubOAuthClientId", "GitHub OAuth client ID"),
            ] {
                d = d.child(self.labeled(&format!("provider.{p}.{field}"), label));
            }
        }
        d = d
            .child(self.b("Use this provider", Action::ActivateProvider, t, cx))
            .child(self.b("Discover models", Action::Discover, t, cx));
        for m in self.provider.models.clone() {
            let id = text(&m["id"]);
            d = d.child(
                div()
                    .flex()
                    .gap(px(6.))
                    .child(self.b(
                        &format!(
                            "{}{}",
                            if id == self.provider.selected_model {
                                "✓ "
                            } else {
                                ""
                            },
                            id
                        ),
                        Action::SelectModel(id.clone()),
                        t,
                        cx,
                    ))
                    .child(div().text_color(t.text2).child(text(&m["capabilities"])))
                    .when(!managed, |d| {
                        d.child(self.field_view(&format!("model.{p}.{id}")))
                            .child(self.b(
                                &format!("Save model {id}"),
                                Action::SaveModel(id.clone()),
                                t,
                                cx,
                            ))
                            .child(self.b(&format!("Remove {id}"), Action::RemoveModel(id), t, cx))
                    }),
            );
        }
        if !managed {
            d = d
                .child(self.labeled("model.new", "Add model ID"))
                .child(self.labeled("model.metadata", "Model metadata (JSON)"))
                .child(self.b("Add model", Action::AddModel, t, cx));
        }
        d = d
            .child(self.labeled("provider.new", "Custom provider definition"))
            .child(self.b("Create provider", Action::CreateProvider, t, cx));
        if p.starts_with("custom:") && !managed {
            d = d.child(self.b("Remove provider", Action::RemoveProvider, t, cx));
        }
        if p == "copilot" {
            d = d.child(
                self.row("GitHub Copilot", &format_device(&self.device.phase), t)
                    .child(self.b("Sign in with GitHub", Action::SignIn, t, cx))
                    .child(self.b("Cancel GitHub sign-in", Action::CancelSignIn, t, cx)),
            );
        }
        if p == "antigravity" {
            let mut accounts=self.row("Antigravity accounts","This unofficial integration is not endorsed by Google. Linked accounts stay in their existing account store.",t);
            for a in self.accounts.as_array().into_iter().flatten() {
                let id = text(&a["accountId"]);
                accounts = accounts.child(
                    div()
                        .child(text(&a["email"]))
                        .child(self.b(
                            &format!("Use account {id}"),
                            Action::Account(AccountAction::Switch(id.clone())),
                            t,
                            cx,
                        ))
                        .child(self.b(
                            &format!("Remove account {id}"),
                            Action::Account(AccountAction::Remove(id)),
                            t,
                            cx,
                        )),
                );
            }
            accounts = accounts
                .child(self.b(
                    "Sign in with Google",
                    Action::Account(AccountAction::SignIn),
                    t,
                    cx,
                ))
                .child(self.field_view("account.callback"))
                .child(self.b("Use callback URL", Action::AccountCallback, t, cx))
                .child(self.b(
                    "Cancel Google sign-in",
                    Action::Account(AccountAction::Cancel),
                    t,
                    cx,
                ))
                .child(self.account_status.clone());
            d = d.child(accounts);
        }
        for (key, label, default) in [
            ("thinking", "Enable model thinking", false),
            ("memoryEnabled", "Enable memory retrieval", true),
            ("memoryIncludePrivate", "Include private memory", false),
            (
                "aiActionRequirePermission",
                "Require permission for actions",
                true,
            ),
            (
                "aiActionRedactionEnabled",
                "Redact sensitive action data",
                true,
            ),
        ] {
            d = d.child(self.toggle(
                label,
                key,
                self.config[key].as_bool().unwrap_or(default),
                t,
                cx,
            ));
        }
        d = d
            .child(self.labeled("memoryMaxItems", "Memory items per request"))
            .child(self.labeled(
                "headerTimeoutSeconds",
                "Header wait, seconds (blank: 30 remote, 120 local)",
            ));
        d = d
            .child(
                self.row("Provider status", "", t)
                    .child(safe_display(&self.provider_statuses)),
            )
            .child(
                self.row(
                    "Pi extensions",
                    "Read only; extensions are not loaded by this list.",
                    t,
                )
                .child(safe_display(&self.pi)),
            );
        let mut tasks = self.row(
            "Task overrides",
            "Leave a provider or model blank to use its default.",
            t,
        );
        for task in ["chat", "search", "action", "voice"] {
            tasks = tasks
                .child(self.labeled(
                    &format!("task.{task}.provider"),
                    &format!("{task} provider"),
                ))
                .child(self.labeled(&format!("task.{task}.model"), &format!("{task} model")))
                .child(self.b(
                    &format!("Save {task} override"),
                    Action::SaveTask(task.into()),
                    t,
                    cx,
                ));
        }
        d = d.child(tasks);
        d.child(self.ai_extensions(t, cx))
    }
    fn voice(&self, t: Tokens, cx: &mut Context<Self>) -> Div {
        let modes = if self.voice_modes.is_empty() {
            "none".into()
        } else {
            self.voice_modes.join(", ")
        };
        let mut d = self
            .row(
                "Push to Talk",
                "Tap to open Tezbar. Keep the same shortcut held to start dictation.",
                t,
            )
            .child(text(&self.config["raymesHotkey"]))
            .child(self.b("Change shortcut", Action::Tab(Tab::General), t, cx))
            .child(self.row("Detected Modes", &modes, t));
        for m in self.voice.as_array().into_iter().flatten() {
            let id = text(&m["id"]);
            let name = text(&m["name"]);
            let ready = m["status"] == "downloaded";
            let downloading = m["status"] == "downloading";
            let selected = self.selected_voice == id;
            let mut row = self
                .row(
                    &name,
                    &format!(
                        "{} · {} · {} · {} · {}",
                        text(&m["family"]),
                        text(&m["language"]),
                        text(&m["tier"]),
                        text(&m["sizeLabel"]),
                        display_name(m["status"].as_str().unwrap_or("Not installed"))
                    ),
                    t,
                )
                .child(
                    self.b(
                        &format!("Use {name}"),
                        Action::Voice(VoiceAction::Select(id.clone())),
                        t,
                        cx,
                    )
                    .role(Role::RadioButton)
                    .aria_toggled(if selected {
                        Toggled::True
                    } else {
                        Toggled::False
                    }),
                );
            if m["downloadedBytes"].as_u64().unwrap_or(0) > 0 && !downloading {
                row = row.child(self.b(
                    &format!("Delete {name}"),
                    Action::Voice(VoiceAction::Delete(id.clone())),
                    t,
                    cx,
                ));
            }
            if downloading {
                row = row.child(self.b(
                    &format!("Cancel {name} download"),
                    Action::Voice(VoiceAction::CancelDownload(id.clone())),
                    t,
                    cx,
                ));
                let stage = if m["stage"] == "installing-runtime" {
                    format!("Installing {}…", text(&m["runtime"]["label"]))
                } else {
                    "Downloading model weights…".into()
                };
                row = row.child(stage);
                let progress = m["progress"]
                    .as_f64()
                    .map(|p| format!("{:.0}%", p.clamp(0., 1.) * 100.))
                    .unwrap_or_else(|| "In progress".into());
                row = row.child(
                    div()
                        .id(format!("voice-progress-{id}"))
                        .role(Role::ProgressIndicator)
                        .aria_label(format!("{name}: {progress}"))
                        .child(progress),
                );
            } else if !ready {
                row = row.child(self.b(
                    &format!(
                        "{} {name}",
                        if m["runtime"]["ready"] == true {
                            "Download"
                        } else {
                            "Install & download"
                        }
                    ),
                    Action::Voice(VoiceAction::Download(id)),
                    t,
                    cx,
                ));
            }
            if !downloading {
                row = row.child(format!(
                    "{} {}",
                    text(&m["runtime"]["label"]),
                    if m["runtime"]["ready"] == true {
                        "ready"
                    } else {
                        "not installed"
                    }
                ));
                if m["runtime"]["ready"] != true {
                    row = row.child(text(&m["runtime"]["installCommand"]));
                }
            }
            if m["status"] == "error" {
                row = row.child(
                    div()
                        .text_color(t.danger)
                        .child("Could not download this model. Retry the download."),
                );
            }
            d = d.child(row.child(div().text_color(t.text2).child(download_progress(
                m["downloadedBytes"].as_u64().unwrap_or(0),
                m["totalBytes"].as_u64(),
            ))));
        }
        d.child(
            div().text_color(t.text2).child(
                "Downloading a model also installs its runtime so Hold-to-Speak works locally.",
            ),
        )
    }
    fn knowledge_view(&self, t: Tokens, cx: &mut Context<Self>) -> Div {
        let mut d = self
            .row(
                "DeepSearch",
                "Choose folders, search depth and a local search model.",
                t,
            )
            .child(self.b("Choose folder", Action::ChooseFolder, t, cx))
            .child(self.b(
                "Add major folders",
                Action::Knowledge(knowledge::Operation::AddMajor),
                t,
                cx,
            ));
        if let Some(s) = &self.knowledge.snapshot {
            for depth in ["off", "basic", "smart", "deep"] {
                d = d.child(self.choice(
                    &display_name(depth),
                    Action::Knowledge(knowledge::Operation::Depth(depth.into())),
                    t,
                    cx,
                ));
            }
            for speed in ["slow", "medium", "fast"] {
                d = d.child(self.choice(
                    &display_name(speed),
                    Action::Knowledge(knowledge::Operation::Speed(speed.into())),
                    t,
                    cx,
                ));
            }
            for root in &s.roots {
                let id = text(&root["id"]);
                let mut row = self
                    .row(
                        root["path"].as_str().unwrap_or("Folder"),
                        &display_name(root["status"].as_str().unwrap_or("")),
                        t,
                    )
                    .child(self.b(
                        if root["enabled"] == true {
                            "Disable folder"
                        } else {
                            "Enable folder"
                        },
                        Action::Knowledge(knowledge::Operation::RootEnabled {
                            id: id.clone(),
                            enabled: root["enabled"] != true,
                        }),
                        t,
                        cx,
                    ))
                    .child(self.b(
                        "Remove folder",
                        Action::Knowledge(knowledge::Operation::RemoveRoot(id.clone())),
                        t,
                        cx,
                    ));
                for depth in ["inherit", "off", "basic", "smart", "deep"] {
                    row = row.child(self.choice(
                        &display_name(depth),
                        Action::Knowledge(knowledge::Operation::RootDepth {
                            id: id.clone(),
                            depth: depth.into(),
                        }),
                        t,
                        cx,
                    ));
                }
                d = d.child(row);
            }
            d = d.child(div().text_color(t.text2).child(safe_display(&s.status)));
        } else {
            d = d.child(if self.knowledge.loading {
                "Loading Knowledge…"
            } else {
                "Knowledge status unavailable"
            });
        }
        if let Some((summary, counts)) = self.knowledge.semantic_summary() {
            d = d.child(div().text_color(t.text2).child(summary));
            if let Some((done, total)) = counts {
                d = d.child(format!("{done} of {total} indexed"));
            }
        }
        for progress in self.knowledge.progress() {
            let label = progress
                .percent
                .map(|p| format!("{} download: {p}%", progress.variant.name()))
                .unwrap_or_else(|| format!("Verifying {}", progress.variant.name()));
            d = d.child(
                div()
                    .id(format!("progress-{}", progress.variant.id()))
                    .role(Role::ProgressIndicator)
                    .aria_label(label.clone())
                    .child(label),
            );
        }
        d = d.child(self.b(
            "Use built-in lightweight search",
            Action::Knowledge(knowledge::Operation::Switch(None)),
            t,
            cx,
        ));
        for file in self.knowledge.files() {
            let variant = file.id;
            let mut row = self.row(variant.name(), &self.knowledge.file_label(&file), t);
            if file.state == knowledge::FileState::Ready && file.failure.is_none() {
                let entity = cx.entity().downgrade();
                let selected =
                    self.knowledge.selected() == format!("embeddinggemma-2:{}", variant.id());
                row = row.child(
                    controls::action_button(
                        format!("select-{}", variant.id()),
                        format!("Use {}", variant.name()),
                        ButtonTone::Quiet,
                        self.knowledge.phase != knowledge::Phase::Idle,
                        t,
                        move |w, cx| {
                            let _ = entity.update(cx, |v, cx| {
                                v.action(
                                    Action::KnowledgeControl(
                                        knowledge::Operation::Switch(Some(variant)),
                                        format!("select:{}", variant.id()),
                                    ),
                                    w,
                                    cx,
                                )
                            });
                        },
                    )
                    .role(Role::RadioButton)
                    .aria_toggled(if selected {
                        Toggled::True
                    } else {
                        Toggled::False
                    }),
                );
            }
            for b in self.knowledge.buttons(variant) {
                let operation = if b.trigger.contains("cancel") {
                    knowledge::Operation::Cancel(variant)
                } else if b.trigger.contains("remove") {
                    knowledge::Operation::Remove(variant)
                } else if b.trigger.contains("download") {
                    knowledge::Operation::Download(variant)
                } else {
                    knowledge::Operation::Switch(Some(variant))
                };
                let view = cx.entity().downgrade();
                let action = Action::KnowledgeControl(operation, b.trigger.clone());
                row = row.child(
                    controls::action_button(
                        b.trigger,
                        b.accessible_name,
                        ButtonTone::Quiet,
                        b.inert,
                        t,
                        move |w, cx| {
                            let _ = view.update(cx, |v, cx| v.action(action.clone(), w, cx));
                        },
                    )
                    .track_focus(
                        &self.knowledge_focus[if variant == knowledge::Variant::Standard {
                            0
                        } else {
                            1
                        }],
                    ),
                );
            }
            d = d.child(row);
        }
        for (label, op) in [
            ("Start indexing", knowledge::Operation::Start),
            ("Pause indexing", knowledge::Operation::Pause),
            ("Resume indexing", knowledge::Operation::Resume),
        ] {
            d = d.child(self.b(label, Action::Knowledge(op), t, cx));
        }
        d=d.child(self.row("Processing","On this computer: private, offline, and enabled. Tezbar Cloud: disabled; files are never uploaded.",t));
        for alert in self.knowledge.alerts() {
            d = d.child(
                div()
                    .id(("semantic", line!() as usize))
                    .role(Role::Alert)
                    .text_color(t.danger)
                    .child(alert),
            );
        }
        d
    }
    fn permissions_view(&self, t: Tokens, cx: &mut Context<Self>) -> Div {
        let mut d = self.row(
            "Permissions",
            "Tezbar requests access only when you choose an action.",
            t,
        );
        if let Some(snapshot) = &self.permissions.snapshot {
            for status in &snapshot.statuses {
                let desc = &status.descriptor;
                let mut row = self
                    .row(&desc.title, &desc.rationale, t)
                    .child(format!("{:?}", status.state));
                if !matches!(
                    status.state,
                    tezbar_ui_model::utilities::PermissionState::Granted
                        | tezbar_ui_model::utilities::PermissionState::Unsupported
                ) {
                    row = row.child(self.b(
                        &format!("Request {}", desc.title),
                        Action::Permission(desc.id.clone()),
                        t,
                        cx,
                    ));
                }
                if let Some(url) = &desc.settings_url {
                    row = row.child(self.b(
                        &format!("Open {} settings", desc.title),
                        Action::OpenPermissionSettings(url.clone()),
                        t,
                        cx,
                    ));
                }
                d = d.child(row);
            }
        } else {
            d = d.child("Loading permissions…")
        }
        if let Some((ok, message)) = &self.permissions.banner {
            d = d.child(
                div()
                    .id("permission-feedback")
                    .role(if *ok { Role::Status } else { Role::Alert })
                    .child(message.clone()),
            );
        }
        d
    }
    fn storage_view(&self, t: Tokens, cx: &mut Context<Self>) -> Div {
        let mut d = self.row(
            "Clipboard",
            "Clipboard history is text-only by default. Image capture is opt-in and size-capped.",
            t,
        );
        for (key, label) in [
            ("watchEnabled", "Watch clipboard history"),
            ("captureImages", "Capture copied images"),
        ] {
            let enabled = self.clipboard[key] == true;
            d = d.child(
                self.b(label, Action::ClipboardToggle(key, !enabled), t, cx)
                    .role(Role::CheckBox)
                    .aria_toggled(if enabled {
                        Toggled::True
                    } else {
                        Toggled::False
                    }),
            );
        }
        d = d
            .child(self.labeled("clipboard.size", "Maximum image megapixels"))
            .child(self.b("Save image size", Action::ClipboardSize, t, cx));
        d = d
            .child(
                self.row("Storage breakdown", "", t)
                    .child(safe_display(&self.storage)),
            )
            .child(self.b("Clear clipboard images", Action::ClearImages, t, cx))
            .child(self.b("Vacuum search database", Action::Vacuum, t, cx))
            .child(self.b("Clear legacy Chromium cache", Action::LegacyCleanup, t, cx));
        d
    }
    fn extensions(&mut self, t: Tokens, _w: &mut Window, cx: &mut Context<Self>) -> Div {
        let mut d = self.row("Installed extensions", "", t).child(self.b(
            "Browse Store",
            Action::BrowseStore,
            t,
            cx,
        ));
        let schema = merge_installed(&self.schema, &self.installed);
        let filter = self.value("extensions.filter").to_lowercase();
        d = d.child(self.labeled("extensions.filter", "Filter installed extensions"));
        let rows = schema
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| filter.is_empty() || safe_display(e).to_lowercase().contains(&filter))
            .cloned()
            .collect::<Vec<_>>();
        let selected = self
            .selected_installed
            .clone()
            .or_else(|| rows.first().map(|e| text(&e["extName"])));
        let choices = rows
            .iter()
            .map(|e| (text(&e["extName"]), text(&e["title"])))
            .collect::<Vec<_>>();
        let count = choices.len();
        let selected_row = selected.clone();
        d =
            d.child(
                uniform_list(
                    "settings-installed-list",
                    count,
                    cx.processor(move |_this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|i| {
                                let (id, label) = &choices[i];
                                let action = Action::InstalledSelect(id.clone());
                                tezbar_ui::virtual_list::row(
                                    id.clone().into(),
                                    label.clone().into(),
                                    i,
                                    count,
                                    selected_row.as_ref() == Some(id),
                                    t,
                                )
                                .tab_index(0)
                                .child(label.clone())
                                .on_click(cx.listener(
                                    move |this, _, w, cx| this.action(action.clone(), w, cx),
                                ))
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .h(px(180.)),
            );
        for e in &rows {
            if selected.as_deref() != e["extName"].as_str() {
                continue;
            }
            let id = text(&e["extName"]);
            let extension_id = text(&e["extensionId"]);
            let name = e["title"].as_str().or(e["name"].as_str()).unwrap_or(&id);
            let mut row = self
                .row(name, "", t)
                .child(self.b(
                    &format!("Preferences for {name}"),
                    Action::Preferences(extension_id.clone(), None),
                    t,
                    cx,
                ))
                .child(self.b(
                    &format!("Uninstall {name}"),
                    Action::Uninstall(id.clone()),
                    t,
                    cx,
                ));
            for c in e["commands"].as_array().into_iter().flatten() {
                let command = format!("extcmd:{id}:{}", text(&c["name"]));
                for (kind, label, _source) in [
                    ("alias", "Alias", "commandAliases"),
                    ("hotkey", "Hotkey", "commandHotkeys"),
                ] {
                    let key = format!("command.{kind}.{command}");
                    row = row.child(self.labeled(&key, label));
                }
                row = row
                    .child(self.b(
                        &format!(
                                "Save alias for {}",
                                c["title"]
                                    .as_str()
                                    .or(c["name"].as_str())
                                    .unwrap_or("command")
                            ),
                        Action::CommandAlias(command.clone()),
                        t,
                        cx,
                    ))
                    .child(self.b(
                        &format!(
                                "Save shortcut for {}",
                                c["title"]
                                    .as_str()
                                    .or(c["name"].as_str())
                                    .unwrap_or("command")
                            ),
                        Action::CommandHotkey(command.clone()),
                        t,
                        cx,
                    ))
                    .child(self.b(
                        &format!(
                                "Toggle {}",
                                c["title"]
                                    .as_str()
                                    .or(c["name"].as_str())
                                    .unwrap_or("command")
                            ),
                        Action::CommandEnabled(
                            command.clone(),
                            self.extension_settings["disabledCommands"][&command] == true,
                        ),
                        t,
                        cx,
                    ))
                    .child(self.b(
                        &format!("Preferences {command}"),
                        Action::Preferences(extension_id.clone(), Some(text(&c["name"]))),
                        t,
                        cx,
                    ));
            }
            d = d.child(row);
        }
        for ((id, command), (_, v)) in &self.preferences {
            let mut row = self.row(&format!("{id} preferences"), "", t);
            for f in v["preferences"].as_array().into_iter().flatten() {
                if command.is_some() && f["commandName"].as_str() != command.as_deref() {
                    continue;
                }
                let name = text(&f["name"]);
                let key = format!(
                    "pref.{id}.{}.{name}",
                    command.as_deref().unwrap_or("$extension")
                );
                if f["type"] == "checkbox" {
                    let checked = self.value(&key) == "true";
                    row = row.child(
                        self.b(
                            f["title"].as_str().unwrap_or(&name),
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
                    let mut choices = div()
                        .id(format!("choices-{key}"))
                        .role(Role::Group)
                        .aria_label(f["title"].as_str().unwrap_or(&name).to_string())
                        .flex()
                        .gap(px(6.));
                    for option in f["data"].as_array().into_iter().flatten() {
                        let value = text(&option["value"]);
                        choices = choices.child(self.b(
                            option["title"].as_str().unwrap_or(&value),
                            Action::PreferenceValue(key.clone(), json!(value)),
                            t,
                            cx,
                        ));
                    }
                    row = row.child(choices);
                } else {
                    row = row.child(self.labeled(&key, f["title"].as_str().unwrap_or(&name)));
                }
            }
            d = d.child(row.child(self.button(
                format!("save-preferences-{id}-{command:?}"),
                "Save preferences",
                Action::SavePreferences(id.clone(), command.clone()),
                t,
                cx,
            )));
        }
        d
    }
    pub(super) fn ai_extensions(&mut self, t: Tokens, cx: &mut Context<Self>) -> Div {
        use ContributionIntent as I;
        let mut d=self.row("AI Extensions","Review command, arguments, environment and requested tools before testing or enabling.",t).child(self.labeled("import","Paste mcpServers or manifest")).child(self.b("Preview import",Action::Contribution(I::Preview),t,cx));
        if let Some(preview) = &self.contributions.preview {
            d = d.child(div().child(safe_display(preview))).child(self.b(
                "Add reviewed import",
                Action::Contribution(I::AddPreview),
                t,
                cx,
            ));
        }
        let state = self.contributions.display.clone();
        if state["project"].is_object() {
            d = d.child(
                self.row(
                    "Project AI extensions",
                    "Not running. Review the folder configuration before adding it.",
                    t,
                )
                .child(safe_display(&state["project"]))
                .child(self.b(
                    "Add for this folder",
                    Action::Contribution(I::TrustProject),
                    t,
                    cx,
                ))
                .child(self.b(
                    "Keep project disabled",
                    Action::Contribution(I::DismissProject),
                    t,
                    cx,
                )),
            );
        }
        for e in state["extensions"].as_array().into_iter().flatten() {
            let id = text(&e["id"]);
            let name = e["name"].as_str().unwrap_or("AI extension");
            let mut row = self
                .row(
                    e["name"].as_str().unwrap_or(&id),
                    if e["enabled"] == true {
                        "Enabled"
                    } else {
                        "Disabled"
                    },
                    t,
                )
                .child(self.b(
                    &format!("Details for {name}"),
                    Action::Contribution(I::Select(id.clone())),
                    t,
                    cx,
                ))
                .child(self.b(
                    &format!("Test {name}"),
                    Action::Contribution(I::Test(id.clone())),
                    t,
                    cx,
                ))
                .child(self.b(
                    &format!(
                        "{} {name}",
                        if e["enabled"] == true {
                            "Disable"
                        } else {
                            "Enable"
                        }
                    ),
                    Action::Contribution(I::Enable(id.clone(), e["enabled"] != true)),
                    t,
                    cx,
                ))
                .child(self.b(
                    &format!("Review changes for {name}"),
                    Action::Contribution(I::Review(id.clone())),
                    t,
                    cx,
                ))
                .child(self.b(
                    &format!("Keep {name} disabled"),
                    Action::Contribution(I::Enable(id.clone(), false)),
                    t,
                    cx,
                ))
                .child(self.b(
                    &format!("Remove {name}"),
                    Action::Contribution(I::Remove(id.clone())),
                    t,
                    cx,
                ));
            if self.selected_extension.as_deref() == Some(&id) {
                row = row.child(div().text_color(t.text2).child(safe_display(e)));
                for (index, c) in e["connections"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    let cid = text(&c["id"]);
                    let connection_label = c["name"]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("Connection {}", index + 1));
                    for field in ["command", "args", "url"] {
                        row = row
                            .child(self.labeled(&format!("connection.{id}.{index}.{field}"), field))
                            .child(self.b(
                                &format!("Save {} for {}", display_name(field), connection_label),
                                Action::Contribution(I::SaveRow(
                                    id.clone(),
                                    "connections".into(),
                                    index,
                                    field.into(),
                                )),
                                t,
                                cx,
                            ));
                    }
                    for (label, action, args) in [
                        (
                            "Sign in",
                            ContributionAction::OAuthStart,
                            vec![json!(id), json!(cid)],
                        ),
                        (
                            "Check sign-in",
                            ContributionAction::OAuthStatus,
                            vec![json!(id), json!(cid)],
                        ),
                        (
                            "Cancel sign-in",
                            ContributionAction::OAuthCancel,
                            vec![json!(id), json!(cid)],
                        ),
                        (
                            "List contributions",
                            ContributionAction::ComposerItems,
                            vec![],
                        ),
                    ] {
                        row = row.child(self.b(
                            &format!("{label} · {connection_label}"),
                            Action::Contribution(I::Operation(id.clone(), action, args)),
                            t,
                            cx,
                        ));
                    }
                }
                for c in e["connections"].as_array().into_iter().flatten() {
                    let cid = text(&c["id"]);
                    for tool in c["catalog"]["tools"].as_array().into_iter().flatten() {
                        let name = text(&tool["name"]);
                        row = row
                            .child(self.b(
                                &format!(
                                    "{} {name}",
                                    if c["disabledTools"][&name] == true {
                                        "Enable"
                                    } else {
                                        "Disable"
                                    }
                                ),
                                Action::Contribution(I::SetTool(
                                    id.clone(),
                                    name.clone(),
                                    c["disabledTools"][&name] == true,
                                    Some(cid.clone()),
                                )),
                                t,
                                cx,
                            ))
                            .child(self.b(
                                &format!("Ask again before {name}"),
                                Action::Contribution(I::Operation(
                                    id.clone(),
                                    ContributionAction::ClearAllowance,
                                    vec![json!(id), json!(cid), json!(name)],
                                )),
                                t,
                                cx,
                            ));
                    }
                    for resource in c["catalog"]["resources"].as_array().into_iter().flatten() {
                        let uri = text(&resource["uri"]);
                        row = row.child(self.b(
                            &format!("Read {}", resource["name"].as_str().unwrap_or(&uri)),
                            Action::Contribution(I::Operation(
                                id.clone(),
                                ContributionAction::ReadResource,
                                vec![json!({"extensionId":id,"connectionId":cid,"uri":uri})],
                            )),
                            t,
                            cx,
                        ));
                    }
                    for prompt in c["catalog"]["prompts"].as_array().into_iter().flatten() {
                        let name = text(&prompt["name"]);
                        row = row.child(self.b(
                            &format!("Preview {name}"),
                            Action::Contribution(I::Operation(
                                id.clone(),
                                ContributionAction::PromptResolve,
                                vec![
                                    json!({"extensionId":id,"connectionId":cid,"name":name}),
                                    json!({}),
                                ],
                            )),
                            t,
                            cx,
                        ));
                    }
                }
                if let Some(slots) = self.secret_slots.get(&id) {
                    for (index, descriptor) in slots.iter().enumerate() {
                        row = row
                            .child(self.labeled(
                                &format!("secret.slot.{id}.{index}"),
                                &format!("{} · {}", descriptor.label, descriptor.scope),
                            ))
                            .child(self.b(
                                &format!("Save {}", descriptor.label),
                                Action::Contribution(I::SaveSavedSecret(id.clone(), index)),
                                t,
                                cx,
                            ));
                    }
                }
                row = row.child(self.b(
                    &format!("Refresh credentials for {name}"),
                    Action::Contribution(I::ReloadSecretFields(id.clone())),
                    t,
                    cx,
                ));
                for collection in ["connections", "providers", "prompts", "commands", "agents"] {
                    row = row
                        .child(self.labeled(&format!("manifest.{id}.{collection}"), collection))
                        .child(self.b(
                            &format!("Save {} for {name}", display_name(collection)),
                            Action::Contribution(I::SaveCollection(id.clone(), collection.into())),
                            t,
                            cx,
                        ));
                }
                row = row
                    .child(self.labeled(
                        "allow.confirm",
                        "Type the extension name to allow all tools",
                    ))
                    .child(self.b(
                        &format!("Show logs for {name}"),
                        Action::Contribution(I::Operation(
                            id.clone(),
                            ContributionAction::Logs,
                            vec![json!(id)],
                        )),
                        t,
                        cx,
                    ));
            }
            d = d.child(row);
        }
        if let Some(review) = &self.review {
            let id = text(&review["id"]);
            d = d.child(
                self.row(
                    "Review tool changes",
                    "Check schemas, annotations and trust pins below.",
                    t,
                )
                .child(safe_display(review))
                .child(self.b(
                    "Accept exact reviewed changes",
                    Action::Contribution(I::Accept(id)),
                    t,
                    cx,
                )),
            );
        }
        if let Some((_, result)) = &self.contributions.result {
            d = d.child(div().text_color(t.text2).child(safe_display(result)));
        }
        d = d.child(self.b(
            "Reset missing secrets",
            Action::Contribution(I::ResetSecrets),
            t,
            cx,
        ));
        d = d
            .child(self.labeled("instructions", "Custom instructions"))
            .child(self.b(
                "Save instructions",
                Action::Contribution(I::Instructions),
                t,
                cx,
            ));
        if let Some(message) = self.contributions.message {
            d = d.child(
                div()
                    .id(("semantic", line!() as usize))
                    .role(Role::Status)
                    .child(message),
            );
        }
        d
    }
}
impl Render for SettingsContent {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::tokens(w);
        let mut tabs = div()
            .id(("semantic", line!() as usize))
            .role(Role::TabList)
            .flex()
            .justify_between()
            .gap(px(4.));
        for (index, tab) in Tab::ALL.into_iter().enumerate() {
            tabs = tabs.child(self.tab_control(index, tab, t, cx));
        }
        let body=match self.tab {Tab::General=>self.general(t,cx),Tab::Ai=>self.ai(t,cx),Tab::Voice=>self.voice(t,cx),Tab::Knowledge=>self.knowledge_view(t,cx),Tab::Storage=>self.storage_view(t,cx),Tab::Extensions=>self.extensions(t,w,cx),Tab::Permissions=>self.permissions_view(t,cx),Tab::Advanced=>self.row("Safety Dry-Run","Preview safety-aware shell, extension-install and native system actions without executing them.",t).child(self.b(if self.dry_run{"Dry-run is ON"}else{"Turn dry-run ON"},Action::DryRun(!self.dry_run),t,cx)).child(self.b("Quit Tezbar",Action::Quit,t,cx))};
        let mut root = controls::sheet(t)
            .id("native-settings")
            .role(if self.embedded {
                Role::Group
            } else {
                Role::Dialog
            })
            .aria_label(if self.embedded {
                self.tab.title()
            } else {
                "Tezbar Settings"
            })
            .bg(t.settings_panel)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, w, cx| this.key(e, w, cx)))
            .when(!self.embedded, |root| {
                root.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .px(px(20.))
                        .py(px(12.))
                        .border_b_1()
                        .border_color(t.hairline)
                        .child(
                            div()
                                .id(("semantic", line!() as usize))
                                .role(Role::Heading)
                                .text_center()
                                .child("Tezbar Settings"),
                        )
                        .child(tabs),
                )
            })
            .child(
                div()
                    .id("settings-content")
                    .role(Role::TabPanel)
                    .aria_label(self.tab.title())
                    .flex_1()
                    .overflow_y_scroll()
                    .px(px(24.))
                    .child(
                        div()
                            .w_full()
                            .max_w(px(if self.tab == Tab::Ai { 850. } else { 610. }))
                            .mx_auto()
                            .child(body),
                    ),
            );
        if let Some(action) = self.confirmation.as_ref() {
            let (title, description, accept) = self.confirmation_copy(action);
            let weak = cx.entity().downgrade();
            root = root.child(
                div()
                    .absolute()
                    .inset_0()
                    .occlude()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(t.settings_panel)
                    .child(
                        div()
                            .id("settings-confirmation")
                            .role(Role::AlertDialog)
                            .aria_label(title.clone())
                            .w_full()
                            .max_w(px(440.))
                            .p(px(20.))
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .child(div().text_color(t.text1).child(title))
                            .child(div().text_color(t.text2).child(description))
                            .child(
                                controls::action_button(
                                    "confirm-settings-action",
                                    accept,
                                    ButtonTone::Danger,
                                    false,
                                    t,
                                    move |w, cx| {
                                        let _ = weak.update(cx, |v, cx| {
                                            v.action(Action::Confirm(true), w, cx)
                                        });
                                    },
                                )
                                .track_focus(&self.confirm_focus[0]),
                            )
                            .child(
                                self.b("Cancel", Action::Confirm(false), t, cx)
                                    .track_focus(&self.confirm_focus[1]),
                            ),
                    ),
            );
        }
        if let Some(error) = self.durable_error() {
            root = root.child(
                div()
                    .id("settings-save-error")
                    .role(Role::Alert)
                    .text_color(t.danger)
                    .px(px(16.))
                    .child(error.to_owned())
                    .child(self.b("Retry saved changes", Action::Retry, t, cx)),
            );
        }
        if let Some(error) = self
            .error
            .as_ref()
            .filter(|e| e.as_str() != settings::CLOSE_ERROR)
        {
            root = root.child(
                div()
                    .id("settings-action-error")
                    .role(Role::Alert)
                    .text_color(t.danger)
                    .px(px(16.))
                    .child(error.clone()),
            );
        }
        if self.embedded {
            return root
                .child(controls::footer(t).child(self.message.clone().unwrap_or_default()))
                .into_any_element();
        }
        root.child(
            controls::footer(t)
                .child(self.message.clone().unwrap_or_default())
                .child(self.b("Save", Action::Save, t, cx))
                .child(controls::keycap("Enter", t))
                .child(self.b("Close", Action::Close, t, cx))
                .child(controls::keycap("Esc", t)),
        )
        .into_any_element()
    }
}
impl SettingsContent {
    fn key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        if e.is_held {
            return;
        }
        let key = e.keystroke.key.as_str();
        if self.confirmation.is_some() {
            if key == "tab" {
                tezbar_ui::controls::FocusScope::cycle(
                    &self.confirm_focus,
                    e.keystroke.modifiers.shift,
                    w,
                    cx,
                );
                cx.stop_propagation();
            } else if key == "escape" {
                self.action(Action::Confirm(false), w, cx);
                cx.stop_propagation();
            }
            return;
        }

        if self.hotkey_recording {
            cx.stop_propagation();
            if key == "escape" {
                self.hotkey_recording = false;
                return;
            }
            let accelerator = if matches!(key, "backspace" | "delete") {
                Some(
                    if cfg!(windows) {
                        "Control+Space"
                    } else {
                        "Alt+Space"
                    }
                    .to_string(),
                )
            } else {
                accelerator(&e.keystroke)
            };
            if let Some(v) = accelerator {
                self.hotkey_recording = false;
                self.queue(
                    vec![Edit::Global {
                        field: "raymesHotkey".into(),
                        value: json!(v),
                    }],
                    cx,
                )
            }
            return;
        }
        if key == "tab" {
            if e.keystroke.modifiers.shift {
                w.focus_prev(cx)
            } else {
                w.focus_next(cx)
            }
            cx.stop_propagation();
        } else if key == "escape" {
            if !self.embedded {
                self.action(Action::Close, w, cx);
            }
            cx.stop_propagation();
        } else if matches!(key, "left" | "right" | "home" | "end")
            && self.tabs.iter().any(|f| f.is_focused(w))
        {
            let current = Tab::ALL.iter().position(|t| *t == self.tab).unwrap();
            let i = match key {
                "home" => 0,
                "end" => 7,
                "left" => (current + 7) % 8,
                _ => (current + 1) % 8,
            };
            self.select_tab(Tab::ALL[i], w, cx);
            cx.stop_propagation();
        }
    }
}
pub(crate) fn accelerator(k: &Keystroke) -> Option<String> {
    let mut parts = vec![];
    if k.modifiers.platform {
        parts.push("Command".to_string())
    }
    if k.modifiers.control {
        parts.push("Control".into())
    }
    if k.modifiers.alt {
        parts.push("Alt".into())
    }
    if k.modifiers.shift {
        parts.push("Shift".into())
    }
    let function_key = k
        .key
        .strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n) && k.key == format!("f{n}"));
    if parts.is_empty() && !function_key {
        return None;
    }
    if matches!(k.key.as_str(), "shift" | "control" | "alt" | "cmd") {
        return None;
    }
    parts.push(match k.key.as_str() {
        "space" => "Space".into(),
        "enter" => "Return".into(),
        v => v.to_uppercase(),
    });
    Some(parts.join("+"))
}
fn update_label(s: &tezbar_update::AppUpdateStatus) -> String {
    use tezbar_update::AppUpdateStatus::*;
    match s {
        Idle => "Ready to check for updates".into(),
        Checking => "Checking for updates…".into(),
        UpToDate { version } => format!("Up to date · {version}"),
        Available { version, .. } => format!("Update available · {version}"),
        ManualAvailable { version, .. } => format!("Installer available · {version}"),
        Downloading {
            downloaded, total, ..
        } => format!("Downloading update… {downloaded} / {}", total.unwrap_or(0)),
        Ready { .. } => "Restart to update".into(),
        Error { .. } => "Could not check for updates. Retry.".into(),
    }
}
fn format_device(s: &DevicePhase) -> String {
    match s {
        DevicePhase::Waiting { code, .. } => format!("Enter code {code} in your browser"),
        DevicePhase::Saving => "Saving provider before sign-in…".into(),
        DevicePhase::Starting | DevicePhase::Polling => "Waiting for GitHub…".into(),
        DevicePhase::Complete => "Signed in".into(),
        DevicePhase::Failed => "Sign-in failed. Retry.".into(),
        DevicePhase::Cancelled => "Sign-in cancelled".into(),
        DevicePhase::Idle => "Sign in to GitHub Copilot".into(),
    }
}
pub(super) fn safe_display(v: &Value) -> String {
    fn lines(v: &Value, path: &str, literal_keys: bool, out: &mut Vec<String>) {
        match v {
            Value::Object(map) => {
                for (key, value) in map {
                    // Environment names are case-sensitive executable inputs. Keep their
                    // spelling, and HTTP header names, exactly as the review manifest provides.
                    let label = if literal_keys {
                        key.to_owned()
                    } else {
                        display_name(key)
                    };
                    let path = if path.is_empty() {
                        label
                    } else {
                        format!("{path} · {label}")
                    };
                    lines(value, &path, matches!(key.as_str(), "env" | "headers"), out);
                }
            }
            Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    lines(item, &format!("{path} {}", i + 1), literal_keys, out);
                }
            }
            Value::Null => out.push(format!("{path}: Not set")),
            Value::String(value) => out.push(format!("{path}: {value}")),
            Value::Bool(value) => {
                out.push(format!("{path}: {}", if *value { "Yes" } else { "No" }))
            }
            Value::Number(value) => out.push(format!("{path}: {value}")),
        }
    }
    let mut out = Vec::new();
    lines(v, "", false, &mut out);
    out.join("\n")
}

fn display_name(value: &str) -> String {
    let mut result = String::new();
    for (i, c) in value.chars().enumerate() {
        if matches!(c, '_' | '-') {
            result.push(' ');
        } else if i == 0 {
            result.extend(c.to_uppercase());
        } else {
            if c.is_uppercase() && !result.ends_with(' ') {
                result.push(' ');
            }
            result.push(c);
        }
    }
    result
}
pub(super) fn download_progress(done: u64, total: Option<u64>) -> String {
    let done = tezbar_ui_model::utilities::monitor::decimal_bytes(done as f64);
    match total {
        Some(total) if total > 0 => format!(
            "{done} of {}",
            tezbar_ui_model::utilities::monitor::decimal_bytes(total as f64)
        ),
        _ => format!("{done} downloaded"),
    }
}

fn normalize_extension_id(id: &str) -> String {
    id.trim()
        .trim_start_matches('@')
        .strip_prefix("raycast.")
        .unwrap_or(id.trim().trim_start_matches('@'))
        .replace(['\\', '/'], "-")
}
pub(super) fn merge_installed(schema: &Value, installed: &Value) -> Value {
    let mut rows = BTreeMap::new();
    for row in schema.as_array().into_iter().flatten() {
        rows.insert(
            normalize_extension_id(row["extName"].as_str().unwrap_or("")),
            row.clone(),
        );
    }
    for row in installed.as_array().into_iter().flatten() {
        let id = row["slug"].as_str().or(row["id"].as_str()).unwrap_or("");
        let name = normalize_extension_id(id);
        if name.is_empty() || rows.contains_key(&name) {
            continue;
        }
        let commands=row["commands"].as_array().into_iter().flatten().map(|c|json!({"name":c["name"],"title":c["title"],"description":c["description"],"mode":c["mode"],"preferences":[]})).collect::<Vec<_>>();
        rows.insert(name.clone(),json!({"extName":name,"extensionId":row["id"],"title":row["name"].as_str().unwrap_or(&name),"description":row["description"],"preferences":[],"commands":commands}));
    }
    let mut rows = rows.into_values().collect::<Vec<_>>();
    rows.sort_by(|a, b| text(&a["title"]).cmp(&text(&b["title"])));
    json!(rows)
}
