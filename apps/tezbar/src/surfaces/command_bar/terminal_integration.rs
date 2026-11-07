//! Retained terminal composition using the one CoreEvents, CoreClient and UI-state writer.
use super::*;
use crate::surfaces::terminal::{TerminalRequest, TerminalSignal, TerminalTransport, TerminalView};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalSource {
    pub entity: EntityId,
    pub mount: u64,
}
#[derive(Default)]
pub(super) struct TerminalIntegration {
    pub view: Option<Entity<TerminalView>>,
    events: Option<EventSubscription>,
    observation: Option<Subscription>,
    admitted: Option<Navigation>,
    disposed: bool,
}
struct Transport(Arc<dyn RequestAdapter>);
impl TerminalTransport for Transport {
    fn send(
        &self,
        r: TerminalRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>> {
        self.0.send(Request::Terminal(r))
    }
}
impl Launcher {
    pub(super) fn defer_hide_terminal(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.terminal.view.clone() else {
            return;
        };
        let generation = view.read(cx).host_generation();
        let window = self.window;
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, w, cx| {
                view.update(cx, |v, cx| {
                    if v.host_generation() == generation {
                        v.set_visible(false, w, cx);
                    }
                });
            });
        });
    }
    pub(crate) fn terminal_mounted(&self) -> bool {
        self.visible
            && !self.onboarding_active
            && !self.terminal.disposed
            && self.terminal.view.is_some()
            && self
                .navigation
                .as_ref()
                .is_some_and(|n| n.surface == Surface::Terminal)
    }
    pub(crate) fn accepts_terminal_source(&self, s: TerminalSource, cx: &App) -> bool {
        self.terminal_mounted()
            && self.terminal.view.as_ref().is_some_and(|v| {
                v.entity_id() == s.entity && v.read(cx).host_generation() == s.mount
            })
    }
    pub(super) fn route_terminal_navigation(&mut self, intent: Navigation, cx: &mut Context<Self>) {
        if self.terminal.admitted.as_ref() == Some(&intent) {
            return;
        }
        self.terminal.admitted = Some(intent.clone());
        let entity = cx.entity().downgrade();
        let window = self.window;
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, w, cx| {
                let _ = entity.update(cx, |this, cx| {
                    this.hide_terminal(w, cx);
                    if this.navigation.as_ref() != Some(&intent)
                        || intent.surface != Surface::Terminal
                        || this.terminal.disposed
                    {
                        return;
                    }
                    if let Err(e) = this.activate_terminal(intent, w, cx) {
                        this.error = Some(e);
                        cx.notify();
                    }
                });
            });
        });
    }
    fn activate_terminal(
        &mut self,
        intent: Navigation,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let view = if let Some(view) = &self.terminal.view {
            view.clone()
        } else {
            let hub = cx
                .try_global::<CoreEvents>()
                .cloned()
                .ok_or("Core event owner is unavailable")?;
            let transport = Arc::new(Transport(self.adapter.clone()));
            let view = cx.new(|cx| TerminalView::new(transport, &self.state, w, cx));
            view.update(cx, |v, cx| {
                v.set_task_indicator(self.task_indicator.clone().into(), cx)
            });
            self.terminal.events = Some(hub.subscribe(
                &view,
                vec![
                    "core:terminal:data".into(),
                    "core:terminal:exit-event".into(),
                    "terminal:sessions-changed".into(),
                ],
                |view, event, cx| view.update(cx, |v, cx| v.core_event(event, cx)),
                cx,
            ));
            self.terminal.observation =
                Some(cx.subscribe(&view, |this, view, _: &TerminalSignal, cx| {
                    let effects = view.update(cx, |v, _| v.drain());
                    for (mount, effect) in effects {
                        let _ = this.relay.send(Event::TerminalEffect(
                            TerminalSource {
                                entity: view.entity_id(),
                                mount,
                            },
                            effect,
                        ));
                    }
                    if this.terminal_mounted() {
                        let rows = this.terminal_panel_rows(cx);
                        let _ = this.relay.send(Event::TerminalRows(rows));
                    }
                }));
            view.update(cx, |v, cx| v.listeners_ready(true, true, cx));
            self.terminal.view = Some(view.clone());
            view
        };
        view.update(cx, |v, cx| {
            v.activate(intent.payload, intent.nonce, w, cx);
            if !self.visible {
                v.set_visible(false, w, cx);
            }
        });
        Ok(())
    }
    pub(super) fn hide_terminal(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(view) = &self.terminal.view {
            view.update(cx, |v, cx| v.set_visible(false, w, cx));
        }
    }
    pub(super) fn show_terminal(&mut self, w: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.terminal_mounted() {
            return false;
        }
        if let Some(view) = &self.terminal.view {
            view.update(cx, |v, cx| v.set_visible(true, w, cx));
        }
        true
    }
    pub(crate) fn terminal_theme(&mut self, light: bool, cx: &mut Context<Self>) {
        if let Some(v) = &self.terminal.view {
            v.update(cx, |v, cx| v.set_theme(light, cx));
        }
    }
    pub(crate) fn terminal_layout(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = &self.terminal.view {
            view.update(cx, |v, cx| v.layout_changed(cx));
        }
    }
    pub(crate) fn terminal_panel_rows(&self, cx: &App) -> Vec<(String, String)> {
        self.terminal
            .view
            .as_ref()
            .map(|v| {
                v.read(cx)
                    .session_rows()
                    .iter()
                    .map(|s| {
                        (
                            s.session_id.clone(),
                            format!(
                                "{} · {}",
                                s.name,
                                if s.running { "running" } else { "saved" }
                            ),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub(crate) fn terminal_panel_action(
        &mut self,
        id: Option<String>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.terminal_mounted() {
            return;
        }
        if let Some(view) = &self.terminal.view {
            view.update(cx, |v, cx| {
                if let Some(id) = id {
                    v.select_session(&id, cx);
                } else {
                    v.new_session(cx);
                }
                v.set_visible(true, w, cx);
            });
        }
    }
    pub(crate) fn terminal_back(
        &mut self,
        s: TerminalSource,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.accepts_terminal_source(s, cx) {
            self.return_home(w, cx);
            self.edit(">", w, cx);
        }
    }
    pub(crate) fn terminal_pins(&mut self, pins: Vec<String>, cx: &mut Context<Self>) {
        self.state.terminal_pinned_sessions = pins;
        self.persist(cx);
    }
    pub(crate) fn terminal_paste(
        &mut self,
        s: TerminalSource,
        generation: u64,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        if self.accepts_terminal_source(s, cx) {
            if let Some(view) = &self.terminal.view {
                view.update(cx, |v, cx| v.paste_result(generation, text, cx));
            }
        }
    }
    pub(crate) fn drain_terminal(
        &mut self,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        let Some(view) = self.terminal.view.clone() else {
            return Task::ready(Ok(()));
        };
        view.update(cx, |v, cx| {
            v.set_visible(false, w, cx);
            v.retry_cleanup(cx);
        });
        let notify = Arc::new(tokio::sync::Notify::new());
        let wake = notify.clone();
        let observation = cx.subscribe(&view, move |_, _, _: &TerminalSignal, _| wake.notify_one());
        cx.spawn(async move |_, cx| {
            let _observation = observation;
            let timer = cx.background_executor().timer(Duration::from_secs(8));
            let wait = async {
                loop {
                    let wake = notify.notified();
                    tokio::pin!(wake);
                    wake.as_mut().enable();
                    let result = view.read_with(cx, |v, _| {
                        v.is_quiescent()
                            .then(|| v.quit_error().map(str::to_owned).map_or(Ok(()), Err))
                    });
                    if let Some(result) = result {
                        return result;
                    }
                    wake.await;
                }
            };
            use std::{future::Future, task::Poll};
            let mut wait = std::pin::pin!(wait);
            let mut timer = std::pin::pin!(timer);
            std::future::poll_fn(|cx| {
                if let Poll::Ready(r) = wait.as_mut().poll(cx) {
                    return Poll::Ready(r);
                }
                if timer.as_mut().poll(cx).is_ready() {
                    return Poll::Ready(Err(
                        "Terminal cleanup exceeded eight seconds; owner retained for retry".into(),
                    ));
                }
                Poll::Pending
            })
            .await
        })
    }
}

#[cfg(test)]
#[path = "terminal_integration_tests.rs"]
mod tests;
