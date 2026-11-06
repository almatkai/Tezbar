//! Lazy composition of native utility views with the app's existing transport and lifecycle.
use super::*;
use crate::surfaces::utilities::{Utilities, UtilityEffect};

fn handles(surface: &Surface) -> bool {
    matches!(
        surface,
        Surface::Clipboard
            | Surface::Snippets
            | Surface::Notes
            | Surface::Vault
            | Surface::Emoji
            | Surface::SystemMonitor
            | Surface::Indexing
            | Surface::Permissions
            | Surface::ReleaseNotes
    )
}
impl Launcher {
    pub(super) fn route_utility_navigation(&mut self, intent: Navigation, cx: &mut Context<Self>) {
        let entity = cx.entity().downgrade();
        let window = self.window;
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                let _ = entity.update(cx, |this, cx| {
                    if handles(&intent.surface) {
                        this.mount_utility(intent, window, cx);
                    } else {
                        this.hide_utilities(window, cx);
                    }
                });
            });
        });
    }
    fn mount_utility(&mut self, intent: Navigation, window: &mut Window, cx: &mut Context<Self>) {
        if self.utilities.is_none() {
            let Some(transport) = self.utility_transport.clone() else {
                return;
            };
            let state = &self.state;
            let utility = cx.new(|cx| Utilities::new(transport, state, window, cx));
            if let Some(events) = cx.try_global::<CoreEvents>().cloned() {
                utility.update(cx, |view, cx| view.attach_events(&events, cx));
            }
            utility.update(cx, |view, cx| {
                view.set_presentation(Arc::new(locale::NativeLocale));
                view.set_update_status(self.update_status.clone(), cx);
                view.configure_release(
                    env!("CARGO_PKG_VERSION").into(),
                    option_env!("TEZBAR_APP_RELEASE_NOTES").map(str::to_owned),
                );
            });
            self.utility_subscription = Some(cx.subscribe(
                &utility,
                |this, _, effect: &UtilityEffect, cx| {
                    this.utility_effect(effect, cx);
                },
            ));
            self.utilities = Some(utility);
        }
        let utility = self.utilities.as_ref().unwrap().clone();
        utility.update(cx, |view, cx| {
            view.navigate(intent.clone(), window, cx);
            let active = self.visible
                && self
                    .navigation
                    .as_ref()
                    .is_some_and(|current| current == &intent);
            if !active {
                view.set_visible(false, window, cx);
            }
        });
        if cx.has_global::<navigation::NavigationInbox>() {
            cx.global_mut::<navigation::NavigationInbox>()
                .acknowledge(&intent);
        }
        self.drag = None;
        self.suppress_pin_click = false;
        self.model_menu = false;
        self.menu = None;
        self.pin_picker = None;
        cx.notify();
    }
    pub(super) fn hide_utilities(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(utility) = &self.utilities {
            utility.update(cx, |view, cx| view.set_visible(false, window, cx));
        }
    }
    fn utility_effect(&mut self, effect: &UtilityEffect, cx: &mut Context<Self>) {
        match effect {
            UtilityEffect::Navigate(surface, payload) => {
                self.navigate(surface.clone(), payload.clone(), cx)
            }
            UtilityEffect::Back => self.receive_navigation(
                Navigation {
                    nonce: self.nonce,
                    surface: Surface::Command,
                    payload: Payload::None,
                },
                cx,
            ),
            UtilityEffect::Hide => {
                let _ = self.relay.send(Event::Hide);
            }
            UtilityEffect::HideThenPaste(text) => self.request_notice(
                Request::Execute {
                    action: json!(Action::CopyAndPasteText { text: text.clone() }),
                    query: String::new(),
                    rank: 1,
                },
                cx,
            ),
            UtilityEffect::RecentEmoji(recent) => {
                self.state.recent_emojis = recent.iter().take(24).cloned().collect();
                self.persist(cx);
            }
            UtilityEffect::ReleaseRead(version) => {
                self.state.release_notes_version = Some(version.clone());
                self.persist(cx);
            }
            UtilityEffect::SaveQr(png) => self.save_utility_qr(png.clone(), cx),
            UtilityEffect::OpenSettings(tab) => self.navigate(
                Surface::Settings(tab.clone()),
                Payload::Settings { tab: tab.clone() },
                cx,
            ),
            UtilityEffect::Reveal(path) => {
                let _ = self
                    .relay
                    .send(Event::Reveal(path.to_string_lossy().into_owned()));
            }
            UtilityEffect::UpdateAction(action) => {
                let _ = self.relay.send(Event::Update(action.clone()));
            }
            UtilityEffect::OpenExtension {
                extension_id,
                command_name,
            } => {
                let adapter = self.adapter.clone();
                let source = self.navigation.clone();
                let request = Request::ExtensionRun(
                    json!({"extensionId":extension_id,"commandName":command_name}),
                );
                self.tasks.push(cx.spawn(async move |this, cx| {
                    let result =
                        gpui_tokio::Tokio::spawn(cx, async move { adapter.send(request).await })
                            .await
                            .unwrap_or_else(|error| Err(error.to_string()));
                    let _ = this.update(cx, |this, cx| {
                        if this.navigation != source {
                            return;
                        }
                        match result {
                            Ok(result) if presentation::extension_has_view(&result) => this
                                .navigate(
                                    Surface::ExtensionRuntime,
                                    Payload::Extension { result },
                                    cx,
                                ),
                            Ok(result) if result["ok"] == false => this.utility_error(
                                result["message"]
                                    .as_str()
                                    .unwrap_or("Extension command failed")
                                    .into(),
                                cx,
                            ),
                            Ok(result) => {
                                if let Some(message) = presentation::extension_notice(&result) {
                                    this.message(message, 4000, cx);
                                }
                            }
                            Err(error) => this.utility_error(error, cx),
                        }
                    });
                }));
            }
        }
    }
    fn utility_error(&mut self, error: String, cx: &mut Context<Self>) {
        if let Some(utility) = &self.utilities {
            utility.update(cx, |view, cx| {
                view.error = Some(error);
                cx.notify();
            });
        } else {
            self.error = Some(error);
            cx.notify();
        }
    }
    fn save_utility_qr(&mut self, png: Vec<u8>, cx: &mut Context<Self>) {
        if png.len() > 16 * 1024 * 1024 || !png.starts_with(b"\x89PNG\r\n\x1a\n") {
            self.utility_error("A PNG of at most 16 MiB is required".into(), cx);
            return;
        }
        let (reply, receiver) = tokio::sync::oneshot::channel();
        if let Err(error) = self.relay.send(Event::SavePng {
            directory: self.utility_save_directory.clone(),
            name: "Tezbar QR.png".into(),
            png,
            reply,
        }) {
            self.utility_error(error, cx);
            return;
        }
        let source = self.navigation.clone();
        self.tasks.push(cx.spawn(async move |this, cx| {
            let result = receiver
                .await
                .unwrap_or_else(|_| Err("Save dialog closed".into()));
            let _ = this.update(cx, |this, cx| {
                if this.navigation != source {
                    return;
                }
                match result {
                    Err(error) => this.utility_error(error, cx),
                    Ok(Some(_)) => {
                        if let Some(utility) = &this.utilities {
                            utility.update(cx, |view, cx| {
                                view.message = Some("QR code saved.".into());
                                cx.notify();
                            });
                        }
                    }
                    Ok(None) => {}
                }
            });
        }));
    }
    pub(super) fn drain_utilities(&mut self, cx: &mut Context<Self>) {
        let Some(utility) = self.utilities.clone() else {
            return;
        };
        self.utility_drain_serial += 1;
        let serial = self.utility_drain_serial;
        let drain = utility.update(cx, |view, cx| view.drain(cx));
        self.writes.push(cx.spawn(async move |this, cx| {
            let result = drain.await;
            let _ = this.update(cx, |this, _| {
                if serial == this.utility_drain_serial {
                    this.utility_save_error = result.err();
                }
            });
        }));
    }
    pub fn durable_write_error(&self) -> Option<String> {
        self.state_write_error()
            .map(|error| format!("Launcher preferences: {error}"))
            .or_else(|| {
                self.utility_save_error
                    .as_ref()
                    .map(|error| format!("Notes: {error}"))
            })
    }
}
