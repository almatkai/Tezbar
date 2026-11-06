//! Native Settings window and acknowledged durable-close protocol.
use crate::events::{Event, Relay};
use gpui::{prelude::*, *};
pub struct Settings {
    relay: Relay,
    pub content: Option<Entity<crate::surfaces::settings::SettingsContent>>,
    content_subscription: Option<Subscription>,
    pub theme: String,
    pub error: Option<String>,
    _activation: Subscription,
    write_owner:
        Option<std::sync::Arc<dyn Fn() -> tezbar_core::host::HostFuture<()> + Send + Sync>>,
    closing: Option<Task<()>>,
    close_generation: Option<u64>,
}
impl Settings {
    pub fn new(relay: Relay, w: &mut Window, cx: &mut Context<Self>) -> Self {
        let queue = relay.clone();
        let activation = cx.observe_window_activation(w, move |_, w, _| {
            if w.is_window_active() {
                let _ = queue.send(Event::SettingsFocused);
            }
        });
        Self {
            _activation: activation,
            content: None,
            content_subscription: None,
            write_owner: None,
            closing: None,
            close_generation: None,
            relay,
            theme: "system".into(),
            error: None,
        }
    }
}
impl Settings {
    pub fn set_content(
        &mut self,
        content: Entity<crate::surfaces::settings::SettingsContent>,
        cx: &mut Context<Self>,
    ) {
        let store = content.read(cx).store.clone();
        self.set_write_owner(std::sync::Arc::new(move || store.drain()));
        let relay = self.relay.clone();
        self.content_subscription = Some(cx.subscribe(
            &content,
            move |_, _, effect: &crate::surfaces::settings::SettingsEffect, _| {
                if matches!(effect, crate::surfaces::settings::SettingsEffect::Close) {
                    let _ = relay.send(Event::CloseSettings);
                }
            },
        ));
        self.content = Some(content);
        cx.notify();
    }
    /// M7 registers its real WriteQueue executor here. The window shell owns no provider drafts.
    pub fn set_write_owner(
        &mut self,
        owner: std::sync::Arc<dyn Fn() -> tezbar_core::host::HostFuture<()> + Send + Sync>,
    ) {
        self.write_owner = Some(owner);
    }
    pub fn drain_writes(&self) -> tezbar_core::host::HostFuture<()> {
        self.write_owner
            .as_ref()
            .map(|owner| owner())
            .unwrap_or_else(|| Box::pin(async { Ok(()) }))
    }
    pub fn begin_close(&mut self, g: u64, cx: &mut Context<Self>) {
        self.close_generation = Some(g);
        let _ = self.relay.send(Event::SettingsAck(g));
        let drain = self.drain_writes();
        let relay = self.relay.clone();
        self.closing = Some(cx.spawn(async move |_, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, drain)
                .await
                .map_err(|e| e.to_string())
                .and_then(|v| v);
            let _ = relay.send(Event::SettingsFinish(g, result));
        }));
    }
    pub fn close_generation(&self) -> Option<u64> {
        self.close_generation
    }
    pub fn close_failed(&mut self, generation: u64, error: String, cx: &mut Context<Self>) {
        if self.close_generation == Some(generation) {
            self.error = Some(error);
            cx.notify();
        }
    }
}
impl Render for Settings {
    fn render(&mut self, w: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(content) = &self.content {
            let t = tezbar_ui::tokens::tokens(w);
            return div()
                .size_full()
                .flex()
                .flex_col()
                .child(div().flex_1().min_h(px(0.)).child(content.clone()))
                .when_some(self.error.clone(), |d, error| {
                    let content = content.clone();
                    let relay = self.relay.clone();
                    d.child(
                        div()
                            .id("settings-close-error")
                            .role(Role::Alert)
                            .text_color(t.danger)
                            .p(px(12.))
                            .child(error)
                            .child(tezbar_ui::controls::action_button(
                                "retry-close-writes",
                                "Retry saved changes",
                                tezbar_ui::controls::ButtonTone::Quiet,
                                false,
                                t,
                                move |w, cx| {
                                    content.update(cx, |view, cx| {
                                        view.action(crate::surfaces::settings::Action::Retry, w, cx)
                                    });
                                    let _ = relay.send(Event::CloseSettings);
                                },
                            )),
                    )
                })
                .into_any_element();
        }
        let t = tezbar_ui::tokens::tokens(w);
        let mut row = div().flex().gap(px(4.));
        for choice in ["system", "light", "dark"] {
            let relay = self.relay.clone();
            let selected = self.theme == choice;
            row = row.child(
                tezbar_ui::controls::action_button(
                    choice,
                    choice,
                    tezbar_ui::controls::ButtonTone::Ghost,
                    false,
                    t,
                    move |_, cx| {
                        let _ = tezbar_ui::tokens::set(choice);
                        cx.set_window_appearance(match choice {
                            "light" => Some(WindowAppearance::Light),
                            "dark" => Some(WindowAppearance::Dark),
                            _ => None,
                        });
                        let _ = relay.send(Event::ThemeChanged(choice.into()));
                        cx.refresh_windows();
                    },
                )
                .role(Role::Tab)
                .aria_selected(selected)
                .bg(if selected {
                    t.pill_strong
                } else {
                    transparent_black()
                }),
            );
        }
        tezbar_ui::controls::sheet(t)
            .on_key_down({
                let relay = self.relay.clone();
                move |e, w, cx| {
                    if e.keystroke.key == "tab" {
                        if e.keystroke.modifiers.shift {
                            w.focus_prev(cx);
                        } else {
                            w.focus_next(cx);
                        }
                        cx.stop_propagation();
                    } else if e.keystroke.key == "escape" {
                        let _ = relay.send(Event::CloseSettings);
                    }
                }
            })
            .p(px(20.))
            .gap(px(16.))
            .child(
                div()
                    .id("appearance-heading")
                    .role(Role::Heading)
                    .child("Appearance"),
            )
            .child(row)
            .when_some(self.error.clone(), |d, error| {
                d.child(
                    div()
                        .id("save-error")
                        .role(Role::Alert)
                        .text_color(t.danger)
                        .child(error)
                        .child(tezbar_ui::controls::action_button(
                            "retry-saved-settings",
                            "Retry saved changes",
                            tezbar_ui::controls::ButtonTone::Quiet,
                            false,
                            t,
                            {
                                let relay = self.relay.clone();
                                move |_, _| {
                                    let _ = relay.send(Event::CloseSettings);
                                }
                            },
                        )),
                )
            })
            .child(tezbar_ui::controls::action_button(
                "close-settings",
                "Close",
                tezbar_ui::controls::ButtonTone::Quiet,
                false,
                t,
                {
                    let r = self.relay.clone();
                    move |_, _| {
                        let _ = r.send(Event::CloseSettings);
                    }
                },
            ))
            .into_any_element()
    }
}
