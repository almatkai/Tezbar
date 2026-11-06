//! Retained M8 composition. No new controller, config executor, runtime or footer poller.
//! Shares the existing App window, SettingsStore and persistent CoreEvents owner.
use super::*;
use crate::surfaces::ai_chat::{ChatEffect, ChatFuture, ChatRequest, ChatTransport, ChatView};
use tezbar_ui_model::chat::{BackgroundStatus, Boot};

/// Source and mount travel with each queued effect, including across native dialogs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ChatSource {
    pub entity: EntityId,
    pub mount: u64,
}

#[derive(Default)]
pub(super) struct ChatIntegration {
    pub view: Option<Entity<ChatView>>,
    transport: Option<Arc<dyn ChatTransport>>,
    assets: Option<CoreClient>,
    events: Option<EventSubscription>,
    observation: Option<Subscription>,
    image_jobs: HashMap<(bool, u64), Task<()>>,
    pub background: Vec<BackgroundStatus>,
    disposed: bool,
    admitted: Option<Navigation>,
}
/// Uses the existing finite CoreClient/fixture adapter; it does not execute a second model.
struct Transport(Arc<dyn RequestAdapter>);
impl ChatTransport for Transport {
    fn send(&self, request: ChatRequest) -> ChatFuture {
        self.0.send(Request::Chat(request))
    }
}
impl Launcher {
    #[cfg(test)]
    pub(crate) fn chat_entity(&self) -> Option<Entity<ChatView>> {
        self.chat.view.clone()
    }
    pub(crate) fn configure_chat(&mut self, core: CoreClient) {
        self.chat.transport = Some(Arc::new(core.clone()));
        self.chat.assets = Some(core);
    }
    pub(super) fn chat_mounted(&self) -> bool {
        self.visible
            && !self.onboarding_active
            && !self.chat.disposed
            && self.chat.view.is_some()
            && self
                .navigation
                .as_ref()
                .is_some_and(|n| n.surface == Surface::AiChat)
    }
    pub(crate) fn owns_chat(&self, id: EntityId) -> bool {
        self.chat.view.as_ref().is_some_and(|v| v.entity_id() == id)
    }
    pub(crate) fn accepts_chat_source(&self, source: ChatSource, cx: &App) -> bool {
        self.chat_mounted()
            && self.chat.view.as_ref().is_some_and(|v| {
                v.entity_id() == source.entity && v.read(cx).host_generation() == source.mount
            })
    }
    pub(crate) fn chat_back(&mut self, source: ChatSource, w: &mut Window, cx: &mut Context<Self>) {
        if self.accepts_chat_source(source, cx) {
            self.return_home(w, cx);
        }
    }
    pub(super) fn chat_row_status(&self, row: &Row) -> Option<&'static str> {
        let session = row.id.strip_prefix("chat:")?;
        self.chat
            .background
            .iter()
            .find(|status| status.session_id == session && status.busy)
            .map(|status| {
                if status.waiting_approval {
                    "Waiting for approval…"
                } else {
                    "Generating…"
                }
            })
    }
    pub(crate) fn chat_clipboard_job(
        &mut self,
        source: ChatSource,
        ticket: u64,
        future: crate::asset_transport::AssetFuture<Vec<Value>>,
        cx: &mut Context<Self>,
    ) {
        self.chat_image_job(source, ticket, future, cx);
    }
    /// Delivery clones the admitted intent once. A later navigation invalidates deferred mounting.
    pub(super) fn route_chat_navigation(&mut self, intent: Navigation, cx: &mut Context<Self>) {
        if self.chat.admitted.as_ref() == Some(&intent) {
            return;
        }
        self.chat.admitted = Some(intent.clone());
        // Invalidate A synchronously, before deferred B activation or mailbox effects.
        self.hide_chat(cx);
        if intent.surface != Surface::AiChat {
            return;
        }
        let entity = cx.entity().downgrade();
        let window = self.window;
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, w, cx| {
                let _ = entity.update(cx, |this, cx| {
                    if this.navigation.as_ref() != Some(&intent) || this.chat.disposed {
                        return;
                    }
                    if let Err(error) = this.activate_chat(&intent, w, cx) {
                        this.error = Some(error);
                        cx.notify();
                    }
                });
            });
        });
    }
    fn ensure_chat(
        &mut self,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Entity<ChatView>, String> {
        if let Some(view) = &self.chat.view {
            return Ok(view.clone());
        }
        let store = self
            .settings_store
            .clone()
            .ok_or("Settings owner is unavailable")?;
        let transport = self
            .chat
            .transport
            .clone()
            .unwrap_or_else(|| Arc::new(Transport(self.adapter.clone())));
        let view = cx.new(|cx| ChatView::new(transport, store, self.state.clone(), w, cx));
        view.update(cx, |view, cx| {
            view.set_task_indicator(self.task_indicator.clone().into(), cx)
        });
        // Install persistent payload subscriptions BEFORE activate can request a run.
        // CoreEvents holds weak entity ownership. WindowHandle plus ChatView run/source
        // generations fence late deliveries; hidden owners continue receiving streams.
        let hub = cx
            .try_global::<CoreEvents>()
            .cloned()
            .ok_or("Core event owner is unavailable")?;
        let window = self.window;
        self.chat.events = Some(
            hub.subscribe(
                &view,
                ["agent:event", "llm-config-changed", "ai-extensions-changed"]
                    .map(str::to_owned)
                    .into(),
                move |view, event, cx| {
                    if let CoreEvent::Other { channel, payload } = event {
                        let _ = window.update(cx, |_, w, cx| {
                            view.update(cx, |view, cx| {
                                view.core_event(channel, payload.clone(), w, cx)
                            });
                        });
                    }
                },
                cx,
            ),
        );
        // Exactly one consumption path: drain the view's effect queue on notification.
        // EventEmitter is intentionally not subscribed by the parent.
        self.chat.observation = Some(cx.observe(&view, |this, view, cx| {
            let effects = view.update(cx, |v, _| v.take_host_effects());
            for (mount, effect) in effects {
                match effect {
                    ChatEffect::Background(statuses) => {
                        this.task_indicator.update(cx, |footer, cx| footer.chat_status(&statuses, cx));
                        this.chat.background = statuses;
                        cx.notify();
                    }
                    effect => {
                        let refused = effect.clone();
                        let sent = this.relay.send(Event::ChatEffect(
                            ChatSource {
                                entity: view.entity_id(),
                                mount,
                            },
                            effect,
                        ));
                        if sent.is_err() {
                            let view = view.downgrade();
                            let window = this.window;
                            cx.defer(move |cx| {
                                let _ = window.update(cx, |_, w, cx| {
                                    let _ = view.update(cx, |view, cx| match refused {
                                        ChatEffect::ChooseImages { ticket } | ChatEffect::ClipboardImage { ticket } => view.attachments_result(ticket, Err("Native action queue is busy. Retry.".into()), w, cx),
                                        ChatEffect::DocumentImage { ticket, .. } => view.document_image_result(ticket, Err("Image request queue is busy. Retry image previews.".into()), w, cx),
                                        _ => {},
                                    });
                                });
                            });
                        }
                    }
                }
            }
        }));
        self.chat.view = Some(view.clone());
        Ok(view)
    }
    pub(crate) fn activate_chat(
        &mut self,
        intent: &Navigation,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if intent.surface != Surface::AiChat {
            return Err("Invalid chat navigation".into());
        }
        let view = self.ensure_chat(w, cx)?;
        let boot = boot(intent, self.view_owner)?;
        self.chat.image_jobs.clear();
        let visible = self.visible && !self.onboarding_active;
        view.update(cx, |v, cx| {
            v.set_ui_state(&self.state, cx);
            v.set_visible(visible, cx);
            v.activate(
                boot,
                format!("launcher:{}:{}", self.view_owner, intent.nonce),
                w,
                cx,
            )?;
            if !visible {
                v.set_visible(false, cx);
            }
            Ok::<_, String>(())
        })?;
        self.sync_task_indicator(cx);
        cx.notify();
        Ok(())
    }
    pub(super) fn hide_chat(&mut self, cx: &mut Context<Self>) {
        // Dropping GPUI tasks aborts gpui_tokio waiters and their read/fetch futures.
        self.chat.image_jobs.clear();
        if let Some(view) = &self.chat.view {
            view.update(cx, |view, cx| view.set_visible(false, cx));
        }
    }
    pub(super) fn show_chat(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.chat_mounted() {
            return false;
        }
        if let Some(view) = &self.chat.view {
            view.update(cx, |view, cx| view.set_visible(true, cx));
        }
        true
    }
    pub(crate) fn chat_image_job(
        &mut self,
        source: ChatSource,
        ticket: u64,
        future: crate::asset_transport::AssetFuture<Vec<Value>>,
        cx: &mut Context<Self>,
    ) {
        if !self.accepts_chat_source(source, cx) {
            return;
        }
        let window = self.window;
        let task = cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, future)
                .await
                .unwrap_or_else(|_| Err("Image preparation failed".into()));
            let _ = window.update(cx, |_, w, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.chat.image_jobs.remove(&(false, ticket));
                    if !this.accepts_chat_source(source, cx) {
                        return;
                    }
                    if let Some(view) = &this.chat.view {
                        view.update(cx, |v, cx| v.attachments_result(ticket, result, w, cx));
                    }
                });
            });
        });
        self.chat.image_jobs.insert((false, ticket), task);
    }
    pub(crate) fn chat_document_image(
        &mut self,
        source: ChatSource,
        ticket: u64,
        raw: String,
        cx: &mut Context<Self>,
    ) {
        if !self.accepts_chat_source(source, cx) {
            return;
        }
        let Some(client) = self.chat.assets.clone() else {
            return;
        };
        let window = self.window;
        let task = cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, client.document_image(raw))
                .await
                .unwrap_or_else(|_| Err("Document image preparation failed".into()));
            let _ = window.update(cx, |_, w, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.chat.image_jobs.remove(&(true, ticket));
                    if !this.accepts_chat_source(source, cx) {
                        return;
                    }
                    if let Some(view) = &this.chat.view {
                        view.update(cx, |v, cx| v.document_image_result(ticket, result, w, cx));
                    }
                });
            });
        });
        self.chat.image_jobs.insert((true, ticket), task);
    }
    /// Real quit only. Hide/back/settings never call this. Keep the Entity through drain.
    pub(crate) fn dispose_chat(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.chat.image_jobs.clear();
        if self.chat.disposed {
            return;
        }
        self.chat.disposed = true;
        self.sync_task_indicator(cx);
        if let Some(view) = &self.chat.view {
            view.update(cx, |v, cx| v.dispose(w, cx));
        }
    }
    pub(crate) fn drain_chat(
        &mut self,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        self.dispose_chat(w, cx);
        let Some(view) = self.chat.view.clone() else {
            return Task::ready(Ok(()));
        };
        // A failed quit retains this exact entity. Retry only its failed persistence;
        // disposal and Cancel effects are never replayed.
        view.update(cx, |v, cx| v.retry_quit_persistence(w, cx));
        let notify = Arc::new(tokio::sync::Notify::new());
        let signal = notify.clone();
        let subscription = cx.observe(&view, move |_, _, _| signal.notify_one());
        cx.spawn(async move |_, cx| {
            let _subscription = subscription;
            let timer = cx.background_executor().timer(Duration::from_secs(8));
            let wait = async {
                loop {
                    let wake = notify.notified();
                    tokio::pin!(wake);
                    wake.as_mut().enable();
                    let state = view.read_with(cx, |v, _| v.quit_settlement());
                    match state {
                        Ok(true) => return Ok(()),
                        Err(error) => return Err(error),
                        Ok(false) => wake.await,
                    }
                }
            };
            // The App's existing quit deadline also bounds the composite barrier.
            // This bound ensures an individual chat waiter cannot outlive that budget.
            use std::{future::Future, task::Poll};
            let mut wait = std::pin::pin!(wait);
            let mut timer = std::pin::pin!(timer);
            std::future::poll_fn(|cx| {
                if let Poll::Ready(result) = wait.as_mut().poll(cx) {
                    return Poll::Ready(result);
                }
                if timer.as_mut().poll(cx).is_ready() {
                    return Poll::Ready(Err(
                        "Chat changes did not finish saving. Retry quit.".into()
                    ));
                }
                Poll::Pending
            })
            .await
        })
    }
}
fn boot(intent: &Navigation, owner: u64) -> Result<Boot, String> {
    let session = || format!("native-chat-{owner}-{}", intent.nonce);
    match &intent.payload {
        Payload::None => Ok(Boot::Panel),
        Payload::Ai {
            prompt,
            cwd,
            resume,
            capture,
        } => {
            if let Some(session_id) = resume {
                return Ok(Boot::Resume {
                    session_id: session_id.clone(),
                });
            }
            if *capture {
                return Ok(Boot::Screen {
                    session_id: session(),
                });
            }
            if let Some(prompt) = prompt {
                return Ok(Boot::Submit {
                    session_id: session(),
                    prompt: prompt.clone(),
                    cwd: cwd.clone(),
                });
            }
            Ok(Boot::NewChat {
                session_id: session(),
                cwd: cwd.clone(),
            })
        }
        _ => Err("Invalid AI navigation payload".into()),
    }
}

#[cfg(test)]
#[path = "chat_integration_tests.rs"]
pub(crate) mod tests;
