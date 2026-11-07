//! Retained extension surfaces on the app's one core, event hub and footer.
use super::*;
use crate::surfaces::{
    extension_runtime::{ExtensionEffectEmission, RuntimeView},
    extensions_store::{
        StoreEffect, StoreEffectEmission, StoreView,
        request::{ExtensionFuture, ExtensionTransport},
    },
};
#[derive(Default)]
pub(super) struct Extensions {
    pub(super) core: Option<CoreClient>,
    pub store: Option<Entity<StoreView>>,
    pub runtime: Option<Entity<RuntimeView>>,
    subscriptions: Vec<Subscription>,
    delivered: Option<Navigation>,
}
struct Transport(Arc<dyn RequestAdapter>, CoreClient);
impl ExtensionTransport for Transport {
    fn send(
        &self,
        r: crate::surfaces::extensions_store::request::ExtensionRequest,
    ) -> ExtensionFuture {
        self.0.send(Request::Extension(r))
    }
    fn drain(&self) -> ExtensionFuture {
        ExtensionTransport::drain(&self.1)
    }
}
impl Launcher {
    pub(super) fn dispose_dropped_extension(&self, reply: &Value, cx: &mut Context<Self>) {
        if reply["mode"] != "view" || reply["root"]["type"] == "MenuBarExtra" {
            return;
        }
        if let Some(id) = reply["sessionId"].as_str() {
            let future = self.adapter.send(Request::Extension(
                crate::surfaces::extensions_store::request::ExtensionRequest::Dispose(id.into()),
            ));
            gpui_tokio::Tokio::spawn(cx, future).detach();
        }
    }
    pub(super) fn configure_extensions(&mut self, core: CoreClient) {
        self.extensions.core = Some(core);
    }
    pub(super) fn extension_mounted(&self) -> bool {
        self.visible
            && !self.onboarding_active
            && self.navigation.as_ref().is_some_and(|n| {
                matches!(n.surface, Surface::Extensions | Surface::ExtensionRuntime)
            })
    }
    pub(crate) fn extension_source(&self, id: EntityId, mount: u64, cx: &App) -> bool {
        self.extension_mounted()
            && self
                .extensions
                .runtime
                .as_ref()
                .is_some_and(|v| v.entity_id() == id && v.read(cx).host_generation() == mount)
    }
    pub(super) fn hide_extensions(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(v) = &self.extensions.store {
            v.update(cx, |v, cx| v.set_visible(false, w, cx));
        }
        if let Some(v) = &self.extensions.runtime {
            v.update(cx, |v, cx| v.set_visible(false, w, cx));
        }
    }
    pub(super) fn show_extensions(&mut self, w: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self
            .navigation
            .as_ref()
            .is_some_and(|n| matches!(n.surface, Surface::Extensions | Surface::ExtensionRuntime))
        {
            return false;
        }
        let intent = self.navigation.clone().unwrap();
        self.deliver_extension(&intent, w, cx).is_ok()
    }
    fn deliver_extension(
        &mut self,
        n: &Navigation,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let core = self
            .extensions
            .core
            .clone()
            .ok_or("Extension core unavailable")?;
        let hub = cx
            .try_global::<CoreEvents>()
            .cloned()
            .ok_or("Extension events unavailable")?;
        if n.surface == Surface::Extensions {
            if self.extensions.store.is_none() {
                let v = cx.new(|cx| {
                    StoreView::new(
                        Arc::new(Transport(self.adapter.clone(), core.clone())),
                        w,
                        cx,
                    )
                });
                v.update(cx, |v, cx| {
                    v.set_assets(Arc::new(core.clone()), cx);
                    v.set_task_indicator(self.task_indicator.clone().into(), cx);
                    v.attach_events(&hub, cx);
                });
                let sub = cx.subscribe_in(
                    &v,
                    w,
                    |this, source, emission: &StoreEffectEmission, w, cx| {
                        let e = &emission.effect;
                        if source.read(cx).host_generation() != emission.generation {
                            return;
                        }
                        if !this.extension_mounted()
                            || this
                                .navigation
                                .as_ref()
                                .is_none_or(|n| n.surface != Surface::Extensions)
                            || this.extensions.store.as_ref() != Some(source)
                        {
                            return;
                        }
                        match e {
                            StoreEffect::Back => this.return_home(w, cx),
                            StoreEffect::Notice(text) => {
                                this.notice = Some(text.clone());
                                cx.notify();
                            }
                            StoreEffect::OpenRuntime(result) => this.navigate(
                                Surface::ExtensionRuntime,
                                Payload::Extension {
                                    result: result.clone(),
                                },
                                cx,
                            ),
                            StoreEffect::Settings => this.navigate(
                                Surface::Settings("extensions".into()),
                                Payload::Settings {
                                    tab: "extensions".into(),
                                },
                                cx,
                            ),
                        }
                    },
                );
                self.extensions.subscriptions.push(sub);
                self.extensions.store = Some(v);
            }
            if let Some(v) = &self.extensions.runtime {
                v.update(cx, |v, cx| v.set_visible(false, w, cx));
            }
            if self.visible {
                self.extensions
                    .store
                    .as_ref()
                    .unwrap()
                    .update(cx, |v, cx| v.activate(w, cx));
            }
        } else {
            if self.extensions.runtime.is_none() {
                let v = cx.new(|cx| {
                    RuntimeView::new(
                        Arc::new(Transport(self.adapter.clone(), core.clone())),
                        Arc::new(core),
                        w,
                        cx,
                    )
                });
                v.update(cx, |v, cx| {
                    v.set_task_indicator(self.task_indicator.clone().into(), cx);
                    v.attach_events(&hub, cx);
                });
                let relay = self.relay.clone();
                let sub = cx.subscribe(&v, move |_, v, e: &ExtensionEffectEmission, _cx| {
                    let _ = relay.send(Event::ExtensionEffect(
                        v.entity_id(),
                        e.generation,
                        e.effect.clone(),
                    ));
                });
                self.extensions.subscriptions.push(sub);
                self.extensions.runtime = Some(v);
            }
            self.extensions
                .runtime
                .as_ref()
                .unwrap()
                .update(cx, |v, cx| {
                    v.attach_events(&hub, cx);
                    v.navigate(n, w, cx)?;
                    v.set_visible(self.visible, w, cx);
                    Ok::<(), String>(())
                })?;
            if let Some(v) = &self.extensions.store {
                v.update(cx, |v, cx| v.set_visible(false, w, cx));
            }
        }
        Ok(())
    }
    pub(super) fn route_extension(&mut self, n: Navigation, cx: &mut Context<Self>) {
        if self.extensions.delivered.as_ref() == Some(&n) {
            return;
        }
        let owner = cx.entity().downgrade();
        let window = self.window;
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, w, cx| {
                let _ = owner.update(cx, |v, cx| {
                    if v.navigation.as_ref() != Some(&n) {
                        if matches!(n.surface, Surface::Extensions | Surface::ExtensionRuntime)
                            && cx.has_global::<navigation::NavigationInbox>()
                        {
                            cx.global_mut::<navigation::NavigationInbox>()
                                .acknowledge(&n);
                        }
                        return;
                    }
                    if matches!(n.surface, Surface::Extensions | Surface::ExtensionRuntime) {
                        match v.deliver_extension(&n, w, cx) {
                            Ok(()) => {}
                            Err(e) => {
                                v.return_home(w, cx);
                                v.error = Some(e);
                            }
                        }
                        if cx.has_global::<navigation::NavigationInbox>() {
                            cx.global_mut::<navigation::NavigationInbox>()
                                .acknowledge(&n);
                        }
                        v.extensions.delivered = Some(n.clone());
                    } else {
                        v.hide_extensions(w, cx);
                    }
                    cx.notify();
                });
            });
        });
    }
    pub(crate) fn drain_extensions(&mut self, _cx: &mut Context<Self>) -> ExtensionFuture {
        // Quiescence is non-destructive: a different surface may still refuse quit.
        self.extensions
            .core
            .as_ref()
            .map(ExtensionTransport::drain)
            .unwrap_or_else(|| Box::pin(async { Ok(json!(true)) }))
    }
    pub(crate) fn dispose_extensions(&mut self, cx: &mut Context<Self>) {
        if let Some(v) = &self.extensions.runtime {
            v.update(cx, |v, cx| v.dispose(cx));
        }
        if let Some(v) = &self.extensions.store {
            v.update(cx, |v, cx| v.dispose(cx));
        }
    }
    pub(crate) fn extension_back(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(v) = &self.extensions.runtime {
            v.update(cx, |v, cx| v.dispose(cx));
        }
        self.return_home(w, cx);
    }
}
impl Launcher {
    pub(crate) fn extension_files(
        &mut self,
        id: EntityId,
        mount: u64,
        ticket: u64,
        result: Result<Vec<String>, String>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.extension_source(id, mount, cx) {
            if let Some(v) = &self.extensions.runtime {
                v.update(cx, |v, cx| v.files_chosen(ticket, result, w, cx));
            }
        }
    }
}
