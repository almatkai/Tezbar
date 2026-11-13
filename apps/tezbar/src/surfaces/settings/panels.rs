//! Composition pinned to the React SettingsView, rather than a native redesign.
use super::*;
use tezbar_ui::{
    controls::{self, ButtonTone},
    tokens::Tokens,
};

pub(super) fn stack() -> Div {
    div().flex().flex_col().gap(px(12.))
}
pub(super) fn line() -> Div {
    div().flex().items_center().flex_wrap().gap(px(8.))
}
pub(super) fn note(copy: impl Into<SharedString>, t: Tokens) -> Div {
    div()
        .text_size(px(11.5))
        .line_height(px(16.))
        .text_color(t.text4)
        .child(copy.into())
}
pub(super) fn card(t: Tokens) -> Div {
    stack()
        .p(px(12.))
        .rounded(px(8.))
        .border_1()
        .border_color(t.hairline)
        .bg(t.panel)
}
pub(super) fn setting(
    label: &str,
    detail: &str,
    content: impl IntoElement,
    t: Tokens,
) -> Stateful<Div> {
    div()
        .id(format!("settings-section-{label}"))
        .flex()
        .items_start()
        .gap(px(16.))
        .py(px(12.))
        .border_b_1()
        .border_color(t.hairline)
        .child(
            div()
                .w(px(150.))
                .flex_shrink_0()
                .pt(px(4.))
                .text_right()
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.text3)
                .child(label.to_owned()),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .child(content)
                .when(!detail.is_empty(), |d| {
                    d.child(note(detail.to_owned(), t).mt(px(4.)))
                }),
        )
}
pub(super) fn progress(id: String, value: f64, t: Tokens) -> Stateful<Div> {
    let value = value.clamp(0., 1.);
    div()
        .id(id)
        .role(Role::ProgressIndicator)
        .aria_label(format!("{:.0}%", value * 100.))
        .h(px(6.))
        .w_full()
        .rounded_full()
        .bg(t.pill_strong)
        .overflow_hidden()
        .child(
            div()
                .h_full()
                .w(relative(value as f32))
                .rounded_full()
                .bg(t.accent_strong),
        )
}
pub(super) fn provider_title(id: &str, config: &Value) -> String {
    config["customProviders"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["id"] == id)
        .and_then(|p| p["name"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            match id {
                "openai" => "OpenAI",
                "anthropic" => "Anthropic",
                "gemini" => "Google Gemini",
                "deepseek" => "DeepSeek",
                "ollama" => "Ollama",
                "copilot" => "GitHub Copilot",
                "opencode" => "OpenCode",
                "antigravity" => "Antigravity",
                "openai-compatible" => "OpenAI Compatible",
                _ => id,
            }
            .into()
        })
}
pub(super) fn provider_subtitle(id: &str) -> &'static str {
    match id {
        "openai" => "GPT models",
        "anthropic" => "Claude models",
        "gemini" => "Gemini models",
        "deepseek" => "DeepSeek models",
        "ollama" => "Local models",
        "copilot" => "GitHub Copilot Chat",
        "opencode" => "OpenCode Zen",
        "antigravity" => "Google account",
        _ => "OpenAI-compatible API",
    }
}

impl SettingsContent {
    pub(super) fn primary(
        &self,
        label: &str,
        action: Action,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.b(label, action, t, cx)
            .bg(t.accent_strong)
            .text_color(rgb(0xffffff))
    }
    pub(super) fn select(
        &self,
        id: &str,
        label: &str,
        options: Vec<(String, Action)>,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        let open = self.menu.as_deref() == Some(id);
        let mut d = stack().relative().gap(px(2.)).child(
            self.button(
                format!("settings-select-{id}"),
                format!("{label}  ⌄"),
                Action::Menu(id.into()),
                t,
                cx,
            )
                .role(Role::ComboBox)
                .aria_label(id.to_owned())
                .aria_expanded(open)
                .debug_selector({
                    let id = id.to_owned();
                    move || format!("settings-select-{id}")
                })
                .min_h(px(32.))
                .w_full()
                .border_1()
                .border_color(t.hairline)
                .bg(t.field),
        );
        if open {
            let mut menu = stack()
                .id(format!("menu-{id}"))
                .role(Role::ListBox)
                .absolute()
                .top_full()
                .left_0()
                .w_full()
                .max_h(px(240.))
                .overflow_y_scroll()
                .occlude()
                .gap(px(2.))
                .p(px(4.))
                .border_1()
                .border_color(t.rim)
                .rounded(px(7.))
                .bg(t.popover_solid);
            for (i, (label, action)) in options.into_iter().enumerate() {
                menu = menu.child(
                    self.button(format!("select-{id}-{i}"), label, action, t, cx)
                        .role(Role::ListBoxOption)
                        .w_full(),
                );
            }
            d = d.child(menu);
        }
        d
    }
    pub(super) fn check(
        &self,
        id: String,
        label: impl Into<SharedString>,
        checked: bool,
        action: Action,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let weak = cx.entity().downgrade();
        let key_view = weak.clone();
        let key_action = action.clone();
        controls::checkbox(id, label, checked, t)
            .on_click(move |_, w, cx| {
                let _ = weak.update(cx, |v, cx| v.action(action.clone(), w, cx));
            })
            .on_key_down(move |event, w, cx| {
                if !event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    let _ = key_view.update(cx, |v, cx| v.action(key_action.clone(), w, cx));
                    cx.stop_propagation();
                }
            })
    }
    pub(super) fn render_general(&self, t: Tokens, cx: &mut Context<Self>) -> Div {
        let zoom = self.layout.as_ref().map(|(_, p)| p.zoom).unwrap_or(1.);
        let appearance=stack().child(note(format!("Launcher display: {}",self.layout.as_ref().map(|(name,_)|name.as_str()).unwrap_or("Loading display…")),t))
            .child(line().child("Interface scale").child(self.select("Interface scale",&format!("{:.0}%",zoom*100.),(0..26).map(|i|{let p=75+i*5;(format!("{p}%"),Action::Zoom(p as f64/100.))}).collect(),t,cx)).child(self.b("100%",Action::Zoom(1.),t,cx)))
            .child(line().child("Width").child(div().w(px(80.)).child(self.field_view("layout.width"))).child("Height").child(div().w(px(80.)).child(self.field_view("layout.height"))).child(self.b("Apply",Action::Layout,t,cx)))
            .child(note("Drag any edge or corner to resize. Size, position and scale are remembered for each display. Use ⌘/Ctrl +/− or 0 to change scale; add Alt in Terminal to keep its text zoom separate.",t))
            .child(line().child(self.b("Reset size & scale for this display",Action::ResetLayout,t,cx)));
        let mut d = div()
            .child(setting(
                "Launcher appearance",
                "Adjust the launcher on its current display, independently of other monitors.",
                appearance,
                t,
            ))
            .child(setting(
                "Startup",
                "Keeps the launcher surface alive between quick open and close cycles.",
                controls::checkbox("startup", "Launch Tezbar at login", true, t)
                    .tab_index(-1)
                    .aria_description("Read only"),
                t,
            ));
        let onboarding = self.check(
            "settings-onboarding-next-launch".into(),
            "Run onboarding on next launch",
            self.config["hasCompletedOnboarding"] == false,
            Action::Toggle(
                "hasCompletedOnboarding",
                self.config["hasCompletedOnboarding"] == false,
            ),
            t,
            cx,
        );
        d=d.child(setting("Onboarding","For testing: show the onboarding tour on the next app launch. Completing or skipping the tour turns this off again.",onboarding,t))
            .child(setting("Tezbar Hotkey",if self.hotkey_recording{"Press a modifier and another key. Escape cancels; Backspace restores the default."}else{"Global shortcut that opens the focused command palette. Click to change it."},self.b(if self.hotkey_recording{"Press shortcut…"}else{self.config["raymesHotkey"].as_str().unwrap_or("Alt+Space")},Action::RecordHotkey,t,cx).w_full().max_w(px(280.)).min_h(px(36.)).text_size(px(13.)).font_weight(FontWeight::SEMIBOLD),t));
        d = d.child(setting(
            "Last Screen",
            "",
            line()
                .child(
                    div()
                        .w(px(80.))
                        .child(self.field_view("uiStateRetentionMs")),
                )
                .child(note("seconds", t))
                .child(self.primary("Save", Action::Save, t, cx)),
            t,
        ));
        for (key, label, detail) in [
            (
                "extensionRuntimeTimeoutMs",
                "Extension Return",
                "Return to the main CommandBar after this much inactivity inside an extension. Choose Never to stay in the extension until you leave it yourself.",
            ),
            (
                "aiModeTimeoutMs",
                "AI Return",
                "Return from AI mode to the main CommandBar after this much inactivity. This timer is independent from Extension and Terminal.",
            ),
            (
                "terminalModeTimeoutMs",
                "Terminal Return",
                "Return from Terminal mode to the main CommandBar after this much inactivity. This timer is independent from Extension and AI.",
            ),
        ] {
            let current = self.value(key);
            let label_value = if current == "0" {
                "Never".into()
            } else {
                format!("After {current} minutes")
            };
            d = d.child(setting(
                label,
                detail,
                line()
                    .child(
                        div().min_w(px(180.)).child(
                            self.select(
                                key,
                                &label_value,
                                ["5", "10", "30", "0"]
                                    .into_iter()
                                    .map(|v| {
                                        (
                                            if v == "0" {
                                                "Never".into()
                                            } else {
                                                format!("After {v} minutes")
                                            },
                                            Action::SetField(key.into(), v.into()),
                                        )
                                    })
                                    .collect(),
                                t,
                                cx,
                            ),
                        ),
                    )
                    .child(self.primary("Save", Action::Save, t, cx)),
                t,
            ));
        }
        let mut themes = line().gap(px(12.));
        for (label, theme, color) in [
            ("Light", Theme::White, rgb(0xffffff)),
            ("Dark", Theme::Dark, rgb(0x020617)),
            ("System", Theme::System, rgb(0x94a3b8)),
        ] {
            themes = themes.child(
                self.choice(label, Action::Theme(theme), t, cx)
                    .text_size(px(0.))
                    .w(px(72.))
                    .h(px(58.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(4.))
                    .border_1()
                    .border_color(if self.state.theme == theme {
                        t.accent_strong
                    } else {
                        t.hairline
                    })
                    .child(
                        div()
                            .size(px(24.))
                            .rounded_full()
                            .bg(color)
                            .border_1()
                            .border_color(t.rim),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(label),
                    )
                    .when(self.state.theme == theme, |d| {
                        d.child(
                            div()
                                .absolute()
                                .right(px(6.))
                                .top(px(6.))
                                .text_size(px(10.))
                                .text_color(t.accent_strong)
                                .child("✓"),
                        )
                    })
                    .relative(),
            );
        }
        d = d
            .child(setting(
                "Appearance",
                "Choose Tezbar's color theme. System follows your device setting.",
                themes,
                t,
            ))
            .child(setting("Updates", "", self.render_updates(t, cx), t));
        let currency = self.state.currency.pinned.as_deref().unwrap_or("");
        let mut currencies = vec![(
            "Auto (most used)".into(),
            Action::SetField("currency".into(), "".into()),
        )];
        let mut codes = self
            .state
            .currency
            .ranked()
            .into_iter()
            .map(|(code, _)| code.to_owned())
            .collect::<Vec<_>>();
        for code in [
            "AED", "AUD", "BGN", "BRL", "CAD", "CHF", "CNY", "CZK", "DKK", "EGP", "EUR", "GBP",
            "HKD", "HUF", "IDR", "ILS", "INR", "JPY", "KRW", "KZT", "MXN", "MYR", "NOK", "NZD",
            "PHP", "PLN", "RON", "RUB", "SAR", "SEK", "SGD", "THB", "TRY", "TWD", "UAH", "USD",
            "VND", "ZAR",
        ] {
            if !codes.iter().any(|c| c == code) {
                codes.push(code.into());
            }
        }
        for code in codes {
            let count = self
                .state
                .currency
                .usage
                .get(&code)
                .map(|u| u.count)
                .unwrap_or(0);
            currencies.push((
                if count > 0 {
                    format!("{code} · {count} uses")
                } else {
                    code.clone()
                },
                Action::SetField("currency".into(), code),
            ));
        }
        d.child(
            stack()
                .py(px(12.))
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Currency conversion"),
                )
                .child(note(
                    "Default target currency when your query omits one (e.g. 1$).",
                    t,
                ))
                .child(
                    line()
                        .child(div().max_w(px(280.)).flex_1().child(self.select(
                            "Default currency",
                            if currency.is_empty() {
                                "Auto (most used)"
                            } else {
                                currency
                            },
                            currencies,
                            t,
                            cx,
                        )))
                        .child(note(
                            format!("Effective target: {}", self.state.currency.preferred()),
                            t,
                        )),
                ),
        )
    }
    fn render_updates(&self, t: Tokens, cx: &mut Context<Self>) -> Div {
        use tezbar_update::AppUpdateStatus::*;
        let mut d = stack()
            .child(
                line().justify_between().child("Update channel").child(
                    self.select(
                        "Update channel",
                        &view::display_name(&self.update_channel),
                        ["stable", "beta", "alpha"]
                            .into_iter()
                            .map(|v| (view::display_name(v), Action::UpdateChannel(v.into())))
                            .collect(),
                        t,
                        cx,
                    ),
                ),
            )
            .child(note(
                match self.update_channel.as_str() {
                    "alpha" => "Alpha — earliest builds, higher risk of bugs.",
                    "beta" => "Beta — preview upcoming features before stable.",
                    _ => "Stable — recommended for most users.",
                },
                t,
            ));
        let mut actions = line()
            .child(self.b(
                "Check for updates",
                Action::Update(UpdateAction::Check),
                t,
                cx,
            ))
            .child(self.b("Release notes", Action::ReleaseNotes, t, cx));
        match &self.update_status {
            Available { .. } => {
                actions = actions.child(self.primary(
                    "Download update",
                    Action::Update(UpdateAction::Download),
                    t,
                    cx,
                ))
            }
            Ready { .. } => {
                actions = actions.child(self.primary(
                    "Restart to update",
                    Action::Update(UpdateAction::Restart),
                    t,
                    cx,
                ))
            }
            ManualAvailable { installer_url, .. } => {
                actions = actions.child(self.b(
                    "Download installer",
                    Action::Update(UpdateAction::Installer(installer_url.clone())),
                    t,
                    cx,
                ))
            }
            _ => {}
        }
        d=d.child(actions).child(self.check("settings-auto-update".into(),"Automatically download updates",self.state.auto_update_enabled,Action::AutoUpdate(!self.state.auto_update_enabled),t,cx))
            .child(note("Signed updates can download automatically in the background. Releases that require manual installation appear in Settings and your search bar with a download link.",t)).child(note("Updates follow your selected release channel.",t)).child(note(view::update_label(&self.update_status),t));
        if let Available { version, notes, .. } | ManualAvailable { version, notes, .. } =
            &self.update_status
        {
            d = d.child(
                card(t)
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("v{version} available")),
                    )
                    .child(note(
                        if notes.is_empty() {
                            "No release notes provided.".into()
                        } else {
                            notes.clone()
                        },
                        t,
                    )),
            );
        }
        if let Downloading {
            downloaded, total, ..
        } = &self.update_status
        {
            d = d
                .child(progress(
                    "app-update-progress".into(),
                    *downloaded as f64 / total.unwrap_or(1).max(1) as f64,
                    t,
                ))
                .child(note(view::download_progress(*downloaded, *total), t));
        }
        d
    }
    pub(super) fn render_ai(&self, t: Tokens, cx: &mut Context<Self>) -> Div {
        let mut sidebar = stack()
            .w(px(220.))
            .flex_shrink_0()
            .pr(px(16.))
            .border_r_1()
            .border_color(t.hairline)
            .child(
                div()
                    .px(px(8.))
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.text3)
                    .child("AI Providers"),
            );
        let mut ids = vec![
            "openai".into(),
            "anthropic".into(),
            "gemini".into(),
            "deepseek".into(),
            "ollama".into(),
            "copilot".into(),
            "opencode".into(),
            "antigravity".into(),
            "openai-compatible".into(),
        ];
        ids.extend(
            self.config["customProviders"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| p["id"].as_str().map(str::to_owned)),
        );
        let mut providers = stack().gap(px(4.));
        for id in ids {
            let selected = self.provider.provider == id;
            let active = self.config["provider"] == id;
            let configured = settings::providers::configured(&self.config, &id);
            let title = provider_title(&id, &self.config);
            let heading = line()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.clone()),
                )
                .when(active, |d| {
                    d.child(
                        div()
                            .text_size(px(8.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.success)
                            .px(px(4.))
                            .py(px(1.))
                            .border_1()
                            .border_color(t.success)
                            .rounded(px(5.))
                            .child("Active"),
                    )
                });
            providers = providers.child(
                self.button(
                    format!("provider-{id}"),
                    title,
                    Action::Provider(id.clone()),
                    t,
                    cx,
                )
                .text_size(px(0.))
                .w_full()
                .px(px(10.))
                .py(px(8.))
                .border_1()
                .border_color(if selected {
                    t.accent_strong
                } else {
                    transparent_black().into()
                })
                .bg(if selected {
                    t.selected
                } else {
                    transparent_black().into()
                })
                .role(Role::Tab)
                .aria_selected(selected)
                .aria_description(if configured {
                    "Configured"
                } else {
                    "Setup required"
                })
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.))
                        .child(
                            stack()
                                .gap(px(2.))
                                .flex_1()
                                .child(heading)
                                .child(note(provider_subtitle(&id), t).text_size(px(10.))),
                        )
                        .child(div().size(px(10.)).rounded_full().bg(if configured {
                            t.success
                        } else {
                            t.warning
                        })),
                ),
            );
        }
        sidebar = sidebar.child(providers).child(self.b(
            "+ Add custom provider",
            Action::Dialog(Some(SettingsDialog::CustomProvider)),
            t,
            cx,
        ));
        let p = self.provider.provider.clone();
        let managed = self.config["customProviders"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|v| v["id"] == p && v.get("_extensionId").is_some());
        let mut header = line().justify_between().child(
            stack()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::BOLD)
                        .child(format!(
                            "{} Configuration",
                            provider_title(&p, &self.config)
                        )),
                )
                .child(note(provider_subtitle(&p), t)),
        );
        header = if self.config["provider"] == p {
            header.child(
                div()
                    .text_size(px(11.))
                    .text_color(t.success)
                    .child("● Active Provider"),
            )
        } else {
            header.child(self.primary("Use this provider", Action::ActivateProvider, t, cx))
        };
        let mut detail = stack()
            .flex_1()
            .min_w(px(0.))
            .pl(px(8.))
            .child(header.pb(px(12.)).border_b_1().border_color(t.hairline));
        if managed {
            detail = detail.child(note(
                "Managed by AI Extension. Edit its provider and credentials in AI Extensions.",
                t,
            ));
        }
        if p == "antigravity" {
            detail = detail.child(self.render_accounts(t, cx));
        }
        detail = detail
            .child(self.toggle(
                "Enable model thinking",
                "thinking",
                self.config["thinking"] == true,
                t,
                cx,
            ))
            .child(self.labeled(
                "headerTimeoutSeconds",
                "Header wait, seconds (blank: 30 remote, 120 local)",
            ));
        if !managed {
            if p.starts_with("custom:") {
                detail = detail
                    .child(self.labeled(&format!("provider.{p}.protocol"), "Protocol"))
                    .child(self.b("Remove provider", Action::RemoveProvider, t, cx));
            }
            if p != "ollama" && p != "antigravity" {
                detail=detail.child(setting(if p=="copilot"{"GitHub Token"}else{"API Key"},if p=="copilot"{"Use a GitHub token or OAuth access token with Copilot Chat access."}else if p.starts_with("custom:")||p=="openai-compatible"{"Optional. Stored in local config. Leave blank for local runners like model-runner or Ollama."}else{"Stored in the local Tezbar config and used by the selected provider."},self.field_view(&format!("provider.{p}.apiKey")),t));
            }
            if p == "copilot" {
                detail = detail.child(
                    card(t)
                        .child("GitHub Device Flow")
                        .child(self.field_view(&format!("provider.{p}.githubOAuthClientId")))
                        .child(
                            line()
                                .child(self.b("Sign in with GitHub", Action::SignIn, t, cx))
                                .child(self.b("Cancel", Action::CancelSignIn, t, cx)),
                        )
                        .child(note(view::format_device(&self.device.phase), t)),
                );
            }
            if !["copilot", "opencode", "antigravity"].contains(&p.as_str()) {
                detail = detail.child(setting(
                    "Base URL",
                    if p == "ollama" {
                        "Point this at your local Ollama server."
                    } else {
                        "Leave the default unless your provider uses a custom endpoint."
                    },
                    self.field_view(&format!("provider.{p}.baseURL")),
                    t,
                ));
            }
        }
        let shown = self
            .provider
            .models
            .iter()
            .filter(|m| m["hiddenFromPicker"] != true)
            .count();
        let mut models = stack().gap(px(8.));
        if !managed {
            models = models
                .child(
                    line()
                        .child(div().flex_1().child(self.field_view("model.new")))
                        .child(self.b("Add", Action::AddModel, t, cx))
                        .child(self.b("Refresh", Action::Discover, t, cx)),
                )
                .child(
                    line()
                        .child(note(
                            format!("{shown} of {} shown in picker", self.provider.models.len()),
                            t,
                        ))
                        .child(self.b("Show all", Action::ModelsVisible(true), t, cx))
                        .child(self.b("Hide all", Action::ModelsVisible(false), t, cx)),
                );
        }
        let mut model_list = stack()
            .gap(px(8.))
            .id("provider-model-list")
            .max_h(px(320.))
            .overflow_y_scroll();
        for model in &self.provider.models {
            let id = text(&model["id"]);
            let selected = self.provider.selected_model == id;
            let mut row = card(t)
                .gap(px(6.))
                .border_color(if selected {
                    t.accent_strong
                } else {
                    t.hairline
                })
                .bg(if selected { t.selected } else { t.panel })
                .child(
                    line()
                        .justify_between()
                        .child(
                            self.button(
                                format!("model-select-{id}"),
                                if selected {
                                    format!("✓ {id}")
                                } else {
                                    id.clone()
                                },
                                Action::SelectModel(id.clone()),
                                t,
                                cx,
                            )
                            .role(Role::RadioButton)
                            .aria_toggled(if selected {
                                Toggled::True
                            } else {
                                Toggled::False
                            }),
                        )
                        .child(note(model["name"].as_str().unwrap_or("").to_owned(), t)),
                );
            let mut badges = line().gap(px(4.));
            for cap in model["capabilities"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                badges = badges.child(
                    div()
                        .text_size(px(11.))
                        .text_color(t.text3)
                        .px(px(6.))
                        .py(px(2.))
                        .rounded(px(5.))
                        .border_1()
                        .border_color(t.hairline)
                        .child(
                            match cap {
                                "vision" => "Vision",
                                "thinking" => "Thinking",
                                "tools" => "Tools",
                                v => v,
                            }
                            .to_owned(),
                        ),
                );
            }
            if let Some(context) = model["contextWindow"].as_u64() {
                badges = badges.child(note(format!("{context} ctx"), t));
            }
            row = row.child(badges);
            if !managed {
                let mut options = vec![(
                    "Automatic".into(),
                    Action::ModelProtocol(id.clone(), String::new()),
                )];
                options.extend(
                    [
                        "openai-chat",
                        "openai-responses",
                        "anthropic",
                        "gemini",
                        "ollama",
                    ]
                    .into_iter()
                    .map(|p| (p.into(), Action::ModelProtocol(id.clone(), p.into()))),
                );
                let mut flags = line()
                    .gap(px(8.))
                    .child(self.check(
                        format!("model-picker-{id}"),
                        "Show in picker",
                        model["hiddenFromPicker"] != true,
                        Action::ModelFlag(
                            id.clone(),
                            "hiddenFromPicker",
                            model["hiddenFromPicker"] != true,
                        ),
                        t,
                        cx,
                    ))
                    .child(div().min_w(px(130.)).child(self.select(
                        &format!("Protocol for {id}"),
                        model["protocol"].as_str().unwrap_or("Automatic"),
                        options,
                        t,
                        cx,
                    )));
                for (cap, label) in [
                    ("vision", "Vision"),
                    ("thinking", "Thinking"),
                    ("tools", "Tools"),
                ] {
                    let checked = model["capabilities"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|c| c == cap));
                    flags = flags.child(
                        self.check(
                            format!("model-{id}-{cap}"),
                            label,
                            checked,
                            Action::ModelCapability(id.clone(), cap.into()),
                            t,
                            cx,
                        )
                        .text_size(px(11.)),
                    );
                }
                row = row.child(flags).child(
                    line()
                        .justify_end()
                        .child(self.button(
                            format!("model-metadata-{id}"),
                            "Edit metadata",
                            Action::Dialog(Some(SettingsDialog::Model(id.clone()))),
                            t,
                            cx,
                        ))
                        .child(self.button(
                            format!("model-remove-{id}"),
                            "Remove",
                            Action::RemoveModel(id),
                            t,
                            cx,
                        )),
                );
            }
            model_list = model_list.child(row);
        }
        detail = detail.child(setting(
            "Models",
            "Add every model you want available for this provider, then choose the selected one.",
            models.child(model_list),
            t,
        ));
        let config = div()
            .flex()
            .gap(px(24.))
            .p(px(16.))
            .min_h(px(500.))
            .rounded(px(8.))
            .border_1()
            .border_color(t.hairline)
            .bg(t.panel)
            .child(sidebar)
            .child(detail);
        let mut d = stack().gap(px(24.)).child(config);
        let mut tasks = stack();
        for (id, label) in [
            ("chat", "Chat"),
            ("search", "Search"),
            ("action", "Action"),
            ("voice", "Voice"),
        ] {
            tasks = tasks.child(
                line()
                    .child(div().w(px(56.)).text_size(px(12.)).child(label))
                    .child(
                        div()
                            .flex_1()
                            .child(self.field_view(&format!("task.{id}.provider"))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .child(self.field_view(&format!("task.{id}.model"))),
                    )
                    .child(self.button(
                        format!("task-save-{id}"),
                        "Save",
                        Action::SaveTask(id.into()),
                        t,
                        cx,
                    )),
            );
        }
        d = d.child(setting(
            "Task models",
            "Leave a provider or model blank to use its default.",
            tasks,
            t,
        ));
        let memory = stack()
            .child(self.toggle(
                "Enable memory retrieval",
                "memoryEnabled",
                self.config["memoryEnabled"].as_bool().unwrap_or(true),
                t,
                cx,
            ))
            .child(self.toggle(
                "Include private memory",
                "memoryIncludePrivate",
                self.config["memoryIncludePrivate"] == true,
                t,
                cx,
            ))
            .child(self.labeled("memoryMaxItems", "Memory items per request"));
        d = d
            .child(setting(
                "AI Memory",
                "Controls retrieval of past notes and conversations during AI responses.",
                memory,
                t,
            ))
            .child(setting(
                "Action Mode",
                "",
                stack()
                    .child(
                        self.toggle(
                            "Require explicit permission",
                            "aiActionRequirePermission",
                            self.config["aiActionRequirePermission"]
                                .as_bool()
                                .unwrap_or(true),
                            t,
                            cx,
                        ),
                    )
                    .child(
                        self.toggle(
                            "Redact sensitive context by default",
                            "aiActionRedactionEnabled",
                            self.config["aiActionRedactionEnabled"]
                                .as_bool()
                                .unwrap_or(true),
                            t,
                            cx,
                        ),
                    ),
                t,
            ));
        d.child(self.render_ai_extensions(t, cx))
    }
    pub(super) fn render_accounts(&self, t: Tokens, cx: &mut Context<Self>) -> Div {
        let mut d = card(t).child(note(
            "This unofficial integration is not endorsed by Google.",
            t,
        ));
        for a in self.accounts.as_array().into_iter().flatten() {
            let id = text(&a["accountId"]);
            d = d.child(
                line()
                    .child(a["email"].as_str().unwrap_or("Google account").to_owned())
                    .child(self.button(
                        format!("account-use-{id}"),
                        "Use",
                        Action::Account(AccountAction::Switch(id.clone())),
                        t,
                        cx,
                    ))
                    .child(self.button(
                        format!("account-remove-{id}"),
                        "Remove",
                        Action::Account(AccountAction::Remove(id)),
                        t,
                        cx,
                    )),
            );
        }
        d.child(
            line()
                .child(self.b(
                    "Sign in with Google",
                    Action::Account(AccountAction::SignIn),
                    t,
                    cx,
                ))
                .child(self.b(
                    "Cancel sign-in",
                    Action::Account(AccountAction::Cancel),
                    t,
                    cx,
                )),
        )
        .child(self.labeled(
            "account.callback",
            "Paste callback URL if the browser did not return to Tezbar",
        ))
        .child(self.b("Use callback URL", Action::AccountCallback, t, cx))
        .child(note(self.account_status.clone(), t))
    }
    pub(super) fn render_voice(&self, t: Tokens, cx: &mut Context<Self>) -> Div {
        let mut d = div()
            .child(setting(
                "Push to Talk",
                "Tap to open Tezbar. Keep the same shortcut held to start dictation.",
                line()
                    .child(controls::keycap(text(&self.config["raymesHotkey"]), t))
                    .child(self.b("Change shortcut", Action::Tab(Tab::General), t, cx)),
                t,
            ))
            .child(setting(
                "Detected Modes",
                "",
                div()
                    .text_size(px(12.5))
                    .child(if self.voice_modes.is_empty() {
                        "none".into()
                    } else {
                        self.voice_modes.join(", ")
                    }),
                t,
            ));
        let models = self.voice.as_array().cloned().unwrap_or_default();
        let current = models
            .iter()
            .find(|m| m["id"] == self.selected_voice)
            .and_then(|m| m["name"].as_str())
            .unwrap_or("Select a model");
        d = d.child(setting(
            "STT Model",
            "",
            self.select(
                "STT Model",
                current,
                models
                    .iter()
                    .map(|m| {
                        (
                            format!(
                                "{}{}",
                                text(&m["name"]),
                                if m["status"] == "downloaded" {
                                    " (ready)"
                                } else {
                                    ""
                                }
                            ),
                            Action::Voice(VoiceAction::Select(text(&m["id"]))),
                        )
                    })
                    .collect(),
                t,
                cx,
            ),
            t,
        ));
        let mut list = stack().gap(px(8.)).py(px(12.));
        for m in models {
            let id = text(&m["id"]);
            let name = text(&m["name"]);
            let ready = m["status"] == "downloaded";
            let downloading = m["status"] == "downloading";
            let mut row = card(t)
                .child(
                    line()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(name.clone()),
                        )
                        .child(note(
                            format!(
                                "{} · {} · {} · {}",
                                text(&m["family"]),
                                text(&m["language"]),
                                text(&m["tier"]),
                                text(&m["sizeLabel"])
                            ),
                            t,
                        )),
                )
                .child(note(text(&m["description"]), t));
            let mut actions = line();
            if downloading {
                actions = actions.child(self.button(
                    format!("voice-cancel-{id}"),
                    "Cancel",
                    Action::Voice(VoiceAction::CancelDownload(id.clone())),
                    t,
                    cx,
                ));
                row = row
                    .child(progress(
                        format!("voice-progress-{id}"),
                        m["progress"].as_f64().unwrap_or(0.),
                        t,
                    ))
                    .child(note(
                        if m["stage"] == "installing-runtime" {
                            format!("Installing {}…", text(&m["runtime"]["label"]))
                        } else {
                            view::download_progress(
                                m["downloadedBytes"].as_u64().unwrap_or(0),
                                m["totalBytes"].as_u64(),
                            )
                        },
                        t,
                    ));
            } else if !ready {
                actions = actions.child(self.button(
                    format!("voice-download-{id}"),
                    if m["runtime"]["ready"] == true {
                        "Download"
                    } else {
                        "Install & download"
                    },
                    Action::Voice(VoiceAction::Download(id.clone())),
                    t,
                    cx,
                ));
            } else {
                actions = actions.child(
                    div()
                        .text_size(px(11.))
                        .text_color(t.success)
                        .child("Ready"),
                );
            }
            if m["downloadedBytes"].as_u64().unwrap_or(0) > 0 && !downloading {
                actions = actions.child(self.button(
                    format!("voice-delete-{id}"),
                    "Delete",
                    Action::Voice(VoiceAction::Delete(id.clone())),
                    t,
                    cx,
                ));
            }
            row = row.child(actions).child(note(
                format!(
                    "{} {}",
                    text(&m["runtime"]["label"]),
                    if m["runtime"]["ready"] == true {
                        "ready"
                    } else {
                        "not installed"
                    }
                ),
                t,
            ));
            if m["status"] == "error" {
                row = row.child(
                    note("Could not download this model. Retry the download.", t)
                        .text_color(t.danger),
                );
            }
            list = list.child(row);
        }
        d.child(list).child(note(
            "Downloading a model also installs its runtime so Hold-to-Speak works locally.",
            t,
        ))
    }
    pub(super) fn render_knowledge(&self, t: Tokens, cx: &mut Context<Self>) -> Div {
        let mut d=div().child(card(t).mb(px(16.)).child(line().justify_between().child(div().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).child("Local knowledge index")).child(div().text_size(px(10.5)).text_color(t.success).child("LOCAL"))).child(note("Tezbar extracts text, recognizes text in images and scanned PDFs, and stores searchable chunks locally. The Command Bar and AI agent share this index.",t)));
        if self.knowledge.loading {
            d = d.child(note(
                if self.knowledge.snapshot.is_some() {
                    "Refreshing Knowledge settings… Showing the last loaded settings."
                } else {
                    "Loading Knowledge settings…"
                },
                t,
            ));
        }
        let mut depths = stack().gap(px(10.));
        for pair in [
            [
                (
                    "off",
                    "Off",
                    "Metadata only",
                    "Filename and path search stays available.",
                ),
                (
                    "basic",
                    "Basic",
                    "Fast keyword search",
                    "Embedded text, first 20 PDF pages, no OCR or vectors.",
                ),
            ],
            [
                (
                    "smart",
                    "Smart",
                    "Recommended",
                    "Full text, selective OCR, and semantic search.",
                ),
                (
                    "deep",
                    "Deep",
                    "Maximum recall",
                    "Full text and embeddings with thorough page OCR.",
                ),
            ],
        ] {
            let mut row = div().flex().gap(px(10.));
            for (id, title, summary, detail) in pair {
                row = row.child(
                    self.choice(
                        title,
                        Action::Knowledge(knowledge::Operation::Depth(id.into())),
                        t,
                        cx,
                    )
                    .flex_1()
                    .min_w(px(0.))
                    .min_h(px(88.))
                    .p(px(12.))
                    .flex()
                    .flex_col()
                    .items_start()
                    .border_1()
                    .border_color(
                        if self
                            .knowledge
                            .snapshot
                            .as_ref()
                            .is_some_and(|s| s.settings.depth == id)
                        {
                            t.accent_strong
                        } else {
                            t.hairline
                        },
                    )
                    .child(note(summary, t))
                    .child(note(detail, t).text_size(px(10.5))),
                );
            }
            depths = depths.child(row);
        }
        d=d.child(setting("Knowledge Depth","The global default for folders set to Use global. Deeper modes trade more background work and storage for better recall.",depths,t))
            .child(setting("Semantic search","Optional on-device search by meaning. Download the model once, then choose it as your search engine.",self.render_semantic(t,cx),t))
            .child(setting("Folders","Only folders added here are content-indexed. Existing filename search remains separate.",self.render_folders(false,t,cx),t))
            .child(setting("Index status","",self.render_index_status(t,cx),t));
        let processing = div()
            .flex()
            .gap(px(10.))
            .child(
                card(t)
                    .flex_1()
                    .child("On this computer")
                    .child(note("Private, offline, and enabled.", t)),
            )
            .child(
                card(t)
                    .flex_1()
                    .child(line().child("Tezbar Cloud").child(note("Coming later", t)))
                    .child(note("Disabled. Files are never uploaded.", t)),
            );
        d.child(setting("Processing","Backend-independent artifacts stay in the local database, even if cloud acceleration is added later.",processing,t))
    }
    pub(super) fn render_semantic(&self, t: Tokens, cx: &mut Context<Self>) -> Div {
        let mut options = vec![(
            "Built-in lightweight search".into(),
            Action::Knowledge(knowledge::Operation::Switch(None)),
        )];
        for file in self.knowledge.files() {
            if file.state == knowledge::FileState::Ready && file.failure.is_none() {
                options.push((
                    file.id.name().into(),
                    Action::Knowledge(knowledge::Operation::Switch(Some(file.id))),
                ));
            }
        }
        let mut d=stack().child(note("Search engine",t)).child(self.select("Search engine",&self.knowledge.selected(),options,t,cx)).child(note("Search by meaning runs on this computer. Keyword search stays available while the model is preparing.",t)).child(div().text_size(px(12.)).text_color(t.text2).child("Model files"));
        for file in self.knowledge.files() {
            let variant = file.id;
            let mut actions = line();
            for b in self.knowledge.buttons(variant) {
                let op = if b.trigger.contains("cancel") {
                    knowledge::Operation::Cancel(variant)
                } else if b.trigger.contains("remove") {
                    knowledge::Operation::Remove(variant)
                } else if b.trigger.contains("download") {
                    knowledge::Operation::Download(variant)
                } else {
                    knowledge::Operation::Switch(Some(variant))
                };
                let weak = cx.entity().downgrade();
                actions = actions.child(
                    controls::action_button(
                        b.trigger.clone(),
                        b.accessible_name,
                        ButtonTone::Quiet,
                        b.inert,
                        t,
                        move |w, cx| {
                            let _ = weak.update(cx, |v, cx| {
                                v.action(
                                    Action::KnowledgeControl(op.clone(), b.trigger.clone()),
                                    w,
                                    cx,
                                )
                            });
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
            d = d.child(
                line()
                    .justify_between()
                    .child(
                        stack()
                            .gap(px(3.))
                            .child(variant.name())
                            .child(note(self.knowledge.file_label(&file), t)),
                    )
                    .child(actions),
            );
        }
        for p in self.knowledge.progress() {
            d = d
                .child(note(
                    format!(
                        "{}: {}",
                        p.variant.name(),
                        p.percent
                            .map(|p| format!("{p}%"))
                            .unwrap_or("Verifying…".into())
                    ),
                    t,
                ))
                .child(progress(
                    format!("semantic-progress-{}", p.variant.id()),
                    p.percent.unwrap_or(0) as f64 / 100.,
                    t,
                ));
        }
        let mut speeds = line();
        for speed in ["slow", "medium", "fast"] {
            speeds = speeds.child(self.choice(
                &view::display_name(speed),
                Action::Knowledge(knowledge::Operation::Speed(speed.into())),
                t,
                cx,
            ));
        }
        d = d
            .child(
                line()
                    .justify_between()
                    .child(
                        stack()
                            .gap(px(3.))
                            .child("Chunking speed")
                            .child(note("Controls background work while preparing search.", t)),
                    )
                    .child(speeds),
            )
            .child(self.b("Refresh", Action::Refresh, t, cx));
        if let Some((summary, counts)) = self.knowledge.semantic_summary() {
            d = d.child(note(summary, t));
            if let Some((done, total)) = counts {
                d = d.child(note(format!("{done} of {total} indexed"), t));
            }
        }
        for alert in self.knowledge.alerts() {
            d = d.child(
                div()
                    .id(format!("knowledge-alert-{alert}"))
                    .role(Role::Alert)
                    .text_size(px(12.))
                    .text_color(t.danger)
                    .child(alert),
            );
        }
        d
    }
    pub(super) fn render_folders(
        &self,
        onboarding: bool,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut d = stack().gap(px(10.));
        let roots = self
            .knowledge
            .snapshot
            .as_ref()
            .map(|s| s.roots.clone())
            .unwrap_or_default();
        if roots.is_empty() {
            d = d.child(
                card(t)
                    .min_h(px(108.))
                    .items_center()
                    .justify_center()
                    .child(note(
                        if onboarding {
                            "No folders yet"
                        } else {
                            "No folders are being content-indexed."
                        },
                        t,
                    ))
                    .child(note(
                        if onboarding {
                            "Add the folders where your documents live."
                        } else {
                            "Nothing leaves this computer."
                        },
                        t,
                    )),
            );
        }
        for root in roots {
            let id = text(&root["id"]);
            let path = text(&root["path"]);
            let title = path
                .rsplit(['/', '\\'])
                .find(|v| !v.is_empty())
                .unwrap_or(&path)
                .to_owned();
            let weak = cx.entity().downgrade();
            let operation = Action::Knowledge(knowledge::Operation::RootEnabled {
                id: id.clone(),
                enabled: root["enabled"] != true,
            });
            let mut row = line()
                .p(px(10.))
                .border_1()
                .border_color(t.hairline)
                .rounded(px(8.));
            if !onboarding {
                row = row.child(
                    controls::switch(
                        format!("root-{id}"),
                        format!("Index {title}"),
                        root["enabled"] == true,
                        t,
                    )
                    .on_click(move |_, w, cx| {
                        let _ = weak.update(cx, |v, cx| v.action(operation.clone(), w, cx));
                    }),
                );
            }
            row = row.child(
                stack()
                    .gap(px(2.))
                    .flex_1()
                    .min_w(px(0.))
                    .child(div().text_size(px(12.5)).child(title.clone()))
                    .child(note(path, t).text_size(px(10.5))),
            );
            if !onboarding {
                let depth = root["depth"].as_str().unwrap_or("inherit");
                row = row
                    .child(
                        div().max_w(px(108.)).child(
                            self.select(
                                &format!("Depth for {title}"),
                                if depth == "inherit" {
                                    "Use global"
                                } else {
                                    depth
                                },
                                ["inherit", "off", "basic", "smart", "deep"]
                                    .into_iter()
                                    .map(|depth| {
                                        (
                                            if depth == "inherit" {
                                                "Use global".into()
                                            } else {
                                                view::display_name(depth)
                                            },
                                            Action::Knowledge(knowledge::Operation::RootDepth {
                                                id: id.clone(),
                                                depth: depth.into(),
                                            }),
                                        )
                                    })
                                    .collect(),
                                t,
                                cx,
                            ),
                        ),
                    )
                    .child(note("Local", t));
            }
            row = row.child(self.button(
                format!("remove-root-{id}"),
                if onboarding { "×" } else { "Remove" },
                Action::Knowledge(knowledge::Operation::RemoveRoot(id)),
                t,
                cx,
            ));
            d = d.child(row);
        }
        d = d.child(
            line()
                .child(self.primary(
                    if onboarding {
                        "Add Desktop, Documents, Downloads & Pictures"
                    } else {
                        "Add Major Folders"
                    },
                    Action::Knowledge(knowledge::Operation::AddMajor),
                    t,
                    cx,
                ))
                .child(self.b(
                    if onboarding {
                        "Add folder…"
                    } else {
                        "Choose Folder…"
                    },
                    Action::ChooseFolder,
                    t,
                    cx,
                )),
        );
        if !onboarding {
            d=d.child(note("Major folders include Desktop, Documents, Downloads, and Pictures. System, cache, dependency, build, and generated files stay excluded.",t));
        }
        d
    }
    pub(super) fn render_index_status(&self, t: Tokens, cx: &mut Context<Self>) -> Div {
        let mut d = stack().gap(px(8.));
        if let Some(s) = &self.knowledge.snapshot {
            let status = &s.status;
            let state = status["state"].as_str().unwrap_or("Ready");
            let p = status["progress"].as_f64().unwrap_or(0.);
            d = d
                .child(
                    line()
                        .justify_between()
                        .child(view::display_name(state))
                        .child(note(
                            format!(
                                "{} files · {} chunks",
                                status["sourceCount"].as_u64().unwrap_or(0),
                                status["chunkCount"].as_u64().unwrap_or(0)
                            ),
                            t,
                        )),
                )
                .child(progress("knowledge-index-progress".into(), p, t))
                .child(note(
                    format!(
                        "{} processed · {} remaining · {:.0}%",
                        status["processedSources"].as_u64().unwrap_or(0),
                        status["queuedSources"].as_u64().unwrap_or(0),
                        p * 100.
                    ),
                    t,
                ))
                .child(note(
                    status["detail"]
                        .as_str()
                        .unwrap_or("Ready to index approved folders.")
                        .to_owned(),
                    t,
                ));
            let (label, op) = match state {
                "indexing" | "scanning" => ("Pause indexing", knowledge::Operation::Pause),
                "paused" => ("Resume indexing", knowledge::Operation::Resume),
                "completed" => ("Reindex now", knowledge::Operation::Start),
                "failed" => ("Retry indexing", knowledge::Operation::Start),
                _ => ("Start indexing", knowledge::Operation::Start),
            };
            d = d.child(
                line()
                    .child(self.b(label, Action::Knowledge(op), t, cx))
                    .child(self.b("Refresh", Action::Refresh, t, cx)),
            );
        } else {
            d = d.child(note("Loading index status…", t));
        }
        d.child(note("Indexing starts automatically after adding a folder. You can close Settings; processing continues in the background. To search, type words from inside a document in the Command Bar; an empty bar only shows launcher recommendations.",t))
    }
    pub(super) fn render_permissions(&self, t: Tokens, cx: &mut Context<Self>) -> Div {
        stack()
            .w_full()
            .min_w(px(0.))
            .child(
                line()
                    .justify_between()
                    .child(
                        note(
                            "System access used by automation, voice, calendar, and screen-aware features.",
                            t,
                        )
                        .flex_1()
                        .min_w(px(0.))
                        .whitespace_normal(),
                    )
                    .child(self.b("Refresh", Action::Refresh, t, cx).flex_shrink_0()),
            )
            .child(self.render_permission_cards(t, cx))
    }
    pub(super) fn render_permission_cards(&self, t: Tokens, cx: &mut Context<Self>) -> Div {
        let mut d = stack().w_full().min_w(px(0.)).gap(px(8.));
        if let Some(snapshot) = &self.permissions.snapshot {
            for status in &snapshot.statuses {
                use tezbar_ui_model::utilities::PermissionState::*;
                let desc = &status.descriptor;
                let granted = matches!(status.state, Granted);
                let supported = !matches!(status.state, Unsupported);
                let state = match status.state {
                    Granted => "Allowed",
                    Denied => "Denied. Turn it on in System Settings.",
                    Unsupported => "Not required on this device",
                    _ => "Not allowed yet",
                };
                let card_selector = format!("permission-card-{}", desc.title);
                let description_selector = format!("permission-description-{}", desc.title);
                let mut row = card(t)
                    .debug_selector(move || card_selector.clone())
                    .w_full()
                    .min_w(px(0.))
                    .flex_row()
                    .items_center()
                    .gap(px(16.))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(if granted { t.success } else { t.text4 })
                            .child(if granted { "✓" } else { "●" }),
                    )
                    .child(
                        stack()
                            .gap(px(3.))
                            .flex_1()
                            .min_w(px(0.))
                            .whitespace_normal()
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(desc.title.clone()),
                            )
                            .child(
                                note(desc.rationale.clone(), t)
                                    .debug_selector(move || description_selector.clone()),
                            )
                            .child(note(state, t).text_color(if granted {
                                t.success
                            } else {
                                t.text4
                            })),
                    );
                if !granted && supported {
                    row = row.child(
                        self.button(
                            format!("permission-{}", desc.title),
                            "Allow",
                            Action::Permission(desc.id.clone()),
                            t,
                            cx,
                        )
                        .flex_shrink_0(),
                    );
                }
                if let Some(url) = &desc.settings_url {
                    row = row.child(
                        self.button(
                            format!("permission-settings-{}", desc.title),
                            "Settings",
                            Action::OpenPermissionSettings(url.clone()),
                            t,
                            cx,
                        )
                        .flex_shrink_0(),
                    );
                }
                d = d.child(row);
            }
        } else {
            d = d.child(note("Loading permissions…", t));
        }
        if let Some((ok, message)) = &self.permissions.banner {
            d = d.child(
                div()
                    .id("permission-feedback")
                    .role(if *ok { Role::Status } else { Role::Alert })
                    .text_size(px(12.))
                    .text_color(if *ok { t.success } else { t.danger })
                    .child(message.clone()),
            );
        }
        d
    }
    pub(super) fn render_storage(&self, t: Tokens, cx: &mut Context<Self>) -> Div {
        let mut clipboard = stack();
        for (key, label) in [
            ("watchEnabled", "Watch clipboard history"),
            ("captureImages", "Capture copied images"),
        ] {
            let checked = self.clipboard[key] == true;
            clipboard = clipboard.child(self.check(
                format!("clipboard-{key}"),
                label,
                checked,
                Action::ClipboardToggle(key, !checked),
                t,
                cx,
            ));
        }
        clipboard = clipboard.child(
            line()
                .child(note("Max image size", t))
                .child(div().w(px(80.)).child(self.field_view("clipboard.size")))
                .child(note("megapixels", t))
                .child(self.primary("Save", Action::ClipboardSize, t, cx)),
        );
        let mut d=div().child(line().justify_end().child(self.b("Refresh usage",Action::Refresh,t,cx))).child(setting("Clipboard","Clipboard history is text-only by default. Image capture is opt-in and size-capped.",clipboard,t));
        if self.storage.is_object() {
            let mut items = stack().gap(px(8.));
            for item in self.storage["items"].as_array().into_iter().flatten() {
                items = items.child(
                    line()
                        .justify_between()
                        .p(px(10.))
                        .border_1()
                        .border_color(t.hairline)
                        .rounded(px(8.))
                        .bg(t.panel)
                        .child(item["label"].as_str().unwrap_or("Storage").to_owned())
                        .child(note(
                            tezbar_ui_model::utilities::monitor::decimal_bytes(
                                item["bytes"].as_f64().unwrap_or(0.),
                            ),
                            t,
                        )),
                );
            }
            d = d.child(setting(
                "Breakdown",
                &format!(
                    "Tracked storage: {}",
                    tezbar_ui_model::utilities::monitor::decimal_bytes(
                        self.storage["totalBytes"].as_f64().unwrap_or(0.)
                    )
                ),
                items,
                t,
            ));
        }
        d.child(setting(
            "Cleanup",
            "",
            line()
                .child(self.b("Clear clipboard images", Action::ClearImages, t, cx))
                .child(self.b("Vacuum search DB", Action::Vacuum, t, cx))
                .child(self.b("Clear Chromium cache", Action::LegacyCleanup, t, cx)),
            t,
        ))
    }
    pub(super) fn render_advanced(&self, t: Tokens, cx: &mut Context<Self>) -> Div {
        div().child(setting("Safety Dry-Run","Preview safety-aware shell, extension-install, and native system actions without executing them.",line().child(self.b(if self.dry_run{"Dry-run is ON"}else{"Turn dry-run ON"},Action::DryRun(!self.dry_run),t,cx)).child(note(if self.dry_run{"Safety-aware actions are previewed and logged."}else{"Safety-aware actions execute after any required confirmation."},t)),t))
            .child(setting("Danger Zone","Quit the application and terminate all background processes.",line().child(self.b("Quit Tezbar",Action::Quit,t,cx).text_color(t.danger)),t))
    }
    pub(super) fn render_extensions(&self, t: Tokens, cx: &mut Context<Self>) -> Div {
        let schema = view::merge_installed(&self.schema, &self.installed);
        let rows = schema.as_array().cloned().unwrap_or_default();
        let filter = self.value("extensions.filter").to_lowercase();
        if self.installed.is_null() && self.schema.is_null() {
            return stack()
                .size_full()
                .items_center()
                .justify_center()
                .child(note("Loading extensions…", t));
        }
        if rows.is_empty() {
            return stack()
                .size_full()
                .items_center()
                .justify_center()
                .child(note("No extensions installed.", t))
                .child(note(
                    "Browse the store to find extensions for your workflow.",
                    t,
                ))
                .child(self.b("Browse Store", Action::BrowseStore, t, cx));
        }
        let selected = self
            .selected_installed
            .clone()
            .or_else(|| rows.first().map(|r| text(&r["extName"])));
        let mut list = stack()
            .gap(px(2.))
            .id("settings-installed-list")
            .flex_1()
            .overflow_y_scroll();
        for e in &rows {
            if !filter.is_empty() && !view::safe_display(e).to_lowercase().contains(&filter) {
                continue;
            }
            let id = text(&e["extName"]);
            let active = selected.as_deref() == Some(&id);
            list = list.child(
                self.button(
                    format!("installed-{id}"),
                    e["title"].as_str().unwrap_or(&id).to_owned(),
                    Action::InstalledCommand(id.clone(), None),
                    t,
                    cx,
                )
                .w_full()
                .text_size(px(12.5))
                .bg(if active && self.selected_command.is_none() {
                    t.selected
                } else {
                    transparent_black().into()
                })
                .child(note(
                    format!(
                        "{} commands",
                        e["commands"].as_array().map(Vec::len).unwrap_or(0)
                    ),
                    t,
                )),
            );
            if active {
                for c in e["commands"].as_array().into_iter().flatten() {
                    let name = text(&c["name"]);
                    list = list.child(
                        self.button(
                            format!("installed-command-{id}-{name}"),
                            c["title"].as_str().unwrap_or(&name).to_owned(),
                            Action::InstalledCommand(id.clone(), Some(name.clone())),
                            t,
                            cx,
                        )
                        .w_full()
                        .pl(px(24.))
                        .bg(
                            if self.selected_command.as_deref() == Some(&name) {
                                t.selected
                            } else {
                                transparent_black().into()
                            },
                        ),
                    );
                }
            }
        }
        let sidebar = stack()
            .gap(px(8.))
            .w(px(320.))
            .min_w(px(280.))
            .flex_shrink_0()
            .border_r_1()
            .border_color(t.hairline)
            .bg(t.panel)
            .child(
                line()
                    .px(px(12.))
                    .py(px(8.))
                    .child(div().flex_1().child(self.field_view("extensions.filter")))
                    .child(self.b("+", Action::BrowseStore, t, cx)),
            )
            .child(list);
        let mut detail = stack()
            .flex_1()
            .min_w(px(0.))
            .p(px(20.))
            .id("extension-settings-detail")
            .overflow_y_scroll();
        if let Some(e) = rows
            .iter()
            .find(|e| selected.as_deref() == e["extName"].as_str())
        {
            let id = text(&e["extName"]);
            let ext = text(&e["extensionId"]);
            let name = e["title"].as_str().unwrap_or(&id);
            let command = self
                .selected_command
                .as_deref()
                .and_then(|name| e["commands"].as_array()?.iter().find(|c| c["name"] == name));
            detail = detail
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(
                            command
                                .and_then(|c| c["title"].as_str())
                                .unwrap_or(name)
                                .to_owned(),
                        ),
                )
                .child(note(
                    command
                        .map(|c| text(&c["description"]))
                        .unwrap_or_else(|| text(&e["description"])),
                    t,
                ));
            if let Some(c) = command {
                let command = format!("extcmd:{id}:{}", text(&c["name"]));
                let enabled = self.extension_settings["disabledCommands"][&command] != true;
                detail = detail
                    .child(
                        card(t).child(
                            line()
                                .justify_between()
                                .child(
                                    stack()
                                        .gap(px(3.))
                                        .child("Enabled")
                                        .child(note("Show this command in search results", t)),
                                )
                                .child(self.button(
                                    format!("command-enable-{command}"),
                                    if enabled { "On" } else { "Off" },
                                    Action::CommandEnabled(command.clone(), !enabled),
                                    t,
                                    cx,
                                )),
                        ),
                    )
                    .child(
                        card(t)
                            .child("Hotkey")
                            .child(note("Global keyboard shortcut for this command", t))
                            .child(self.button(
                                format!("record-command-hotkey-{command}"),
                                if self.command_hotkey_recording.as_deref() == Some(&command) {
                                    "Press shortcut…".into()
                                } else {
                                    let value = self.value(&format!("command.hotkey.{command}"));
                                    if value.is_empty() {
                                        "Record shortcut".into()
                                    } else {
                                        value
                                    }
                                },
                                Action::RecordCommandHotkey(command.clone()),
                                t,
                                cx,
                            ))
                            .child(note("Escape cancels; Backspace removes the shortcut.", t)),
                    )
                    .child(
                        card(t)
                            .child("Alias")
                            .child(note("Type this alias to find the command quickly", t))
                            .child(self.field_view(&format!("command.alias.{command}")))
                            .child(self.b("Save alias", Action::CommandAlias(command), t, cx)),
                    )
                    .child(self.b(
                        "Preferences",
                        Action::Dialog(Some(SettingsDialog::Preferences(
                            ext.clone(),
                            Some(text(&c["name"])),
                        ))),
                        t,
                        cx,
                    ));
            } else {
                let mut commands = stack()
                    .gap(px(0.))
                    .border_1()
                    .border_color(t.hairline)
                    .rounded(px(8.))
                    .child(
                        line()
                            .p(px(8.))
                            .bg(t.pill)
                            .child(div().flex_1().child("Command"))
                            .child(div().w(px(70.)).child("Status")),
                    );
                for c in e["commands"].as_array().into_iter().flatten() {
                    let key = format!("extcmd:{id}:{}", text(&c["name"]));
                    commands = commands.child(
                        line()
                            .p(px(8.))
                            .border_t_1()
                            .border_color(t.hairline)
                            .child(
                                self.button(
                                    format!("command-detail-{key}"),
                                    c["title"].as_str().unwrap_or("Command").to_owned(),
                                    Action::InstalledCommand(id.clone(), Some(text(&c["name"]))),
                                    t,
                                    cx,
                                )
                                .flex_1(),
                            )
                            .child(note(
                                if self.extension_settings["disabledCommands"][&key] == true {
                                    "Disabled"
                                } else {
                                    "Enabled"
                                },
                                t,
                            )),
                    );
                }
                detail = detail
                    .child(commands)
                    .child(self.b(
                        "Preferences",
                        Action::Dialog(Some(SettingsDialog::Preferences(ext, None))),
                        t,
                        cx,
                    ))
                    .child(
                        self.b("Uninstall extension", Action::Uninstall(id), t, cx)
                            .text_color(t.danger),
                    );
            }
        }
        div()
            .flex()
            .h_full()
            .min_h(px(460.))
            .overflow_hidden()
            .rounded(px(12.))
            .child(sidebar)
            .child(detail)
    }
    pub(super) fn render_ai_extensions(&self, t: Tokens, cx: &mut Context<Self>) -> Div {
        use ContributionIntent as I;
        let add = self.select(
            "Add AI extension",
            "Add",
            vec![
                (
                    "Add connection".into(),
                    Action::Dialog(Some(SettingsDialog::Connection)),
                ),
                (
                    "Paste configuration…".into(),
                    Action::Dialog(Some(SettingsDialog::Import)),
                ),
                ("Import file…".into(), Action::ImportFile),
            ],
            t,
            cx,
        );
        let mut d=stack().child(line().justify_between().child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child("AI Extensions")).child(div().min_w(px(170.)).child(add)))
            .child(note("Add tools, prompts and instructions to AI mode. Extensions run on this computer with your permissions. Tezbar asks before using their tools.",t));
        let extensions = self.contributions.display["extensions"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if extensions.is_empty() {
            d = d.child(
                card(t)
                    .child(note("No AI extensions yet.", t))
                    .child(self.b(
                        "Import MCP configuration",
                        Action::Dialog(Some(SettingsDialog::Import)),
                        t,
                        cx,
                    )),
            );
        }
        for e in extensions {
            let id = text(&e["id"]);
            let name = e["name"].as_str().unwrap_or(&id);
            let enabled = e["enabled"] == true;
            let mut actions = line();
            for (section, label) in [
                ("config", "Configuration"),
                ("credentials", "Credentials"),
                ("permissions", "Permissions"),
                ("tools", "Tools"),
                ("oauth", "Sign in"),
            ] {
                actions = actions.child(self.button(
                    format!("ai-extension-{id}-{section}"),
                    label,
                    Action::Dialog(Some(SettingsDialog::Extension {
                        id: id.clone(),
                        section: section.into(),
                    })),
                    t,
                    cx,
                ));
            }
            d = d.child(
                card(t)
                    .child(
                        line()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(name.to_owned()),
                            )
                            .child(
                                self.button(
                                    format!("ai-extension-toggle-{id}"),
                                    if enabled { "Enabled" } else { "Disabled" },
                                    Action::Contribution(I::Enable(id.clone(), !enabled)),
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
                    .child(note(text(&e["description"]), t))
                    .child(actions)
                    .child(
                        line()
                            .child(self.button(
                                format!("ai-extension-test-{id}"),
                                "Test",
                                Action::Contribution(I::Test(id.clone())),
                                t,
                                cx,
                            ))
                            .child(self.button(
                                format!("ai-extension-review-{id}"),
                                "Review changes",
                                Action::Contribution(I::Review(id.clone())),
                                t,
                                cx,
                            ))
                            .child(
                                self.button(
                                    format!("ai-extension-remove-{id}"),
                                    "Remove",
                                    Action::Contribution(I::Remove(id)),
                                    t,
                                    cx,
                                )
                                .text_color(t.danger),
                            ),
                    ),
            );
        }
        if self.contributions.display["project"].is_object() {
            d = d.child(
                card(t)
                    .child("Project AI extensions")
                    .child(note(
                        "Not running. Review the folder configuration before adding it.",
                        t,
                    ))
                    .child(view::safe_display(&self.contributions.display["project"]))
                    .child(
                        line()
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
                    ),
            );
        }
        if let Some(review) = &self.review {
            d = d.child(
                card(t)
                    .child("Review tool changes")
                    .child(view::safe_display(review))
                    .child(self.b(
                        "Accept exact reviewed changes",
                        Action::Contribution(I::Accept(text(&review["id"]))),
                        t,
                        cx,
                    )),
            );
        }
        if let Some((_, result)) = &self.contributions.result {
            d = d.child(note(view::safe_display(result), t));
        }
        d.child(self.b(
            "Reset missing secrets",
            Action::Contribution(I::ResetSecrets),
            t,
            cx,
        ))
        .child(self.labeled("instructions", "Custom instructions"))
        .child(self.check(
            "settings-use-agents-md".into(),
            "Use AGENTS.md instructions from the current folder",
            self.contributions.display["useAgentsMd"] == true,
            Action::Contribution(I::UseAgentsMd(
                self.contributions.display["useAgentsMd"] != true,
            )),
            t,
            cx,
        ))
        .child(self.b(
            "Save instructions",
            Action::Contribution(I::Instructions),
            t,
            cx,
        ))
    }
}
