//! Retained native renderer. RuntimeSession/FormState own worker transitions.
//! Construction and Render never perform requests or schedule timers.
pub mod assets;
mod color_wheel;
mod images;
mod preferences;
#[cfg(test)]
mod real_sdk;
#[cfg(test)]
mod tests;
mod view;
use super::command_bar::navigation::{Navigation, Payload};
use super::extensions_store::request::{
    ExtensionFuture, ExtensionRequest, ExtensionTransport, success,
};
use crate::events::{CoreEvent, CoreEvents, EventSubscription};
use assets::{AssetScope, ExtensionAssets};
use gpui::{prelude::*, *};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};
use tezbar_ui::{
    markdown::{DocumentEffect, MarkdownView},
    text_input::{Committed, FieldOptions, TextInput},
};
use tezbar_ui_model::extensions::runtime::{
    Effect, Node, NodeKind, RequestToken, RuntimeSession, TreeReply, UiEvent,
};

/// Requests for parent-owned host facilities. No direct OS APIs in this surface.
#[derive(Clone)]
pub enum ExtensionEffect {
    Preferences,
    Back,
    Close,
    PickFiles { ticket: u64, multiple: bool },
    Notice(String),
}
impl std::fmt::Debug for ExtensionEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExtensionEffect(<redacted>)")
    }
}
pub struct ExtensionEffectEmission {
    pub generation: u64,
    pub effect: ExtensionEffect,
}
impl EventEmitter<ExtensionEffectEmission> for RuntimeView {}
struct DocumentEntry {
    view: Entity<MarkdownView>,
    _subscription: Subscription,
}
struct FieldEntry {
    input: Entity<TextInput>,
    _subscription: Subscription,
}
pub struct RuntimeView {
    host_generation: u64,
    wheel_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    wheel_drag: bool,
    transport: Arc<dyn ExtensionTransport>,
    assets: Arc<dyn ExtensionAssets>,
    pub(super) session: Option<RuntimeSession>,
    window: AnyWindowHandle,
    focus: FocusHandle,
    footer: Option<AnyView>,
    search: Entity<TextInput>,
    action_search: Entity<TextInput>,
    subscriptions: Vec<Subscription>,
    events: Option<EventSubscription>,
    documents: BTreeMap<String, DocumentEntry>,
    fields: BTreeMap<String, FieldEntry>,
    pub(super) rows: Vec<view::Row>,
    pub(super) list_scroll: UniformListScrollHandle,
    viewport: std::ops::Range<usize>,
    viewport_pending: bool,
    action_index: usize,
    opened: Option<u64>,
    visible: bool,
    disposed: bool,
    start: Instant,
    timer: Option<Task<()>>,
    job: Option<Task<()>>,
    queue: VecDeque<(RequestToken, ExtensionRequest)>,
    polling: bool,
    image_light: bool,
    host_tickets: BTreeMap<u64, (RequestToken, String)>,
    next_ticket: u64,
    color_ticket: Option<(
        u64,
        tezbar_ui_model::extensions::runtime::SessionKey,
        Option<String>,
    )>,
    scope: Option<AssetScope>,
    image_state: images::Images,
    auxiliary: BTreeMap<u64, Task<()>>,
    error: Option<&'static str>,
    preference: preferences::Setup,
}
impl RuntimeView {
    fn emit_effect(&self, effect: ExtensionEffect, cx: &mut Context<Self>) {
        cx.emit(ExtensionEffectEmission {
            generation: self.host_generation,
            effect,
        });
    }

    pub(super) fn focus_content(&self, w: &mut Window, cx: &mut Context<Self>) {
        if self
            .session
            .as_ref()
            .is_some_and(|s| view::shows_search(&s.root))
        {
            self.search.focus_handle(cx).focus(w, cx);
        } else {
            self.focus.focus(w, cx);
        }
    }
    pub fn host_generation(&self) -> u64 {
        self.host_generation
    }
    pub fn new(
        transport: Arc<dyn ExtensionTransport>,
        assets: Arc<dyn ExtensionAssets>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(TextInput::new);
        search.update(cx, |f, _| {
            f.configure(FieldOptions {
                name: "Search extension".into(),
                placeholder: "Search".into(),
                ..Default::default()
            })
        });
        let action_search = cx.new(TextInput::new);
        action_search.update(cx, |f, _| {
            f.configure(FieldOptions {
                name: "Search actions".into(),
                placeholder: "Search actions".into(),
                ..Default::default()
            })
        });
        let search_sub = cx.subscribe_in(&search, w, |v, _, text: &Committed, w, cx| {
            v.search_text(text.0.clone(), w, cx)
        });
        let action_sub = cx.subscribe(&action_search, |v, _, _: &Committed, cx| {
            v.action_index = 0;
            cx.notify();
        });
        let list_scroll = UniformListScrollHandle::new();

        cx.on_release(|v, cx| {
            v.dispose(cx);
            gpui_tokio::Tokio::spawn(cx, v.transport.drain()).detach();
        })
        .detach();
        Self {
            host_generation: 0,
            wheel_bounds: color_wheel::new_bounds(),
            wheel_drag: false,
            transport,
            assets,
            session: None,
            window: w.window_handle(),
            focus: cx.focus_handle(),
            footer: None,
            search,
            action_search,
            subscriptions: vec![search_sub, action_sub],
            events: None,
            documents: BTreeMap::new(),
            fields: BTreeMap::new(),
            rows: vec![],
            list_scroll,
            viewport: 0..48,
            viewport_pending: false,
            action_index: 0,
            opened: None,
            visible: false,
            disposed: false,
            start: Instant::now(),
            timer: None,
            job: None,
            queue: VecDeque::new(),
            polling: false,
            image_light: false,
            host_tickets: BTreeMap::new(),
            next_ticket: 0,
            color_ticket: None,
            scope: None,
            image_state: Default::default(),
            auxiliary: BTreeMap::new(),
            error: None,
            preference: Default::default(),
        }
    }
    pub fn set_task_indicator(&mut self, footer: AnyView, cx: &mut Context<Self>) {
        self.footer = Some(footer);
        cx.notify();
    }
    fn now(&self) -> u64 {
        self.start.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
    pub fn attach_events(&mut self, hub: &CoreEvents, cx: &mut Context<Self>) {
        let window = self.window;
        self.events = Some(
            hub.subscribe(
                &cx.entity(),
                [
                    "extension:session-invalidated",
                    "extension-worker-status",
                    "extension:ui",
                    "extension:session-disposed",
                ]
                .map(str::to_owned)
                .to_vec(),
                move |owner, e, cx| {
                    let _ = window.update(cx, |_, w, cx| {
                        owner.update(cx, |v, cx| v.core_event(e, w, cx))
                    });
                },
                cx,
            ),
        );
    }
    /// Parent acknowledges the Navigation only after this returns Ok. A failed/no-view
    /// delivery leaves the previous live session usable and never creates a new screen.
    pub fn navigate(
        &mut self,
        n: &Navigation,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if n.surface != tezbar_ui_model::router::Surface::ExtensionRuntime {
            return Err("Wrong extension destination".into());
        }
        let Payload::Extension { result } = &n.payload else {
            return Err("Missing extension view".into());
        };
        self.activate(result.clone(), n.nonce, w, cx)
    }
    pub fn activate(
        &mut self,
        result: Value,
        nonce: u64,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.opened == Some(nonce) {
            return Ok(());
        }
        let reply = TreeReply::from_wire(result).map_err(str::to_owned)?;
        let polling = !matches!(
            reply.refresh,
            tezbar_ui_model::extensions::runtime::PollPolicy::EventDriven
        );
        let mut next = RuntimeSession::new(reply, self.now()).map_err(str::to_owned)?;
        if let Some(text) = next.root.string("searchText").map(str::to_owned) {
            let _ = next.search(text);
        }
        self.retire(cx);
        self.host_generation += 1;
        self.disposed = false;
        self.visible = true;
        self.opened = Some(nonce);
        self.session = Some(next);
        self.polling = polling;
        self.error = None;
        let search_text = self
            .session
            .as_ref()
            .map(|s| s.search_text.clone())
            .unwrap_or_default();
        self.search.update(cx, |f, cx| {
            f.replace_silently(
                &search_text,
                tezbar_ui::text_input::ByteSelection {
                    range: search_text.len()..search_text.len(),
                    reversed: false,
                },
                w,
                cx,
            )
        });
        self.prepare(w, cx);
        self.drain_model_effects(w, cx);
        self.resolve_scope(cx);
        if self
            .session
            .as_ref()
            .is_some_and(|s| view::shows_search(&s.root))
        {
            self.search.focus_handle(cx).focus(w, cx);
        } else {
            self.focus.focus(w, cx);
        }
        self.arm(cx);
        cx.notify();
        Ok(())
    }
    pub fn set_visible(&mut self, visible: bool, w: &mut Window, cx: &mut Context<Self>) {
        if self.visible == visible || self.disposed {
            return;
        }
        self.host_generation += 1;
        self.wheel_drag = false;
        self.visible = visible;
        self.timer = None;
        if !visible {
            self.host_tickets.clear();
            self.color_ticket = None;
            self.search.update(cx, |f, cx| f.cancel_composition(w, cx));
            self.release_images(cx);
            self.scope = None;
            for f in self.fields.values() {
                f.input.update(cx, |f, cx| f.cancel_composition(w, cx));
            }
        }
        let now = self.now();
        let e = self
            .session
            .as_mut()
            .and_then(|s| s.visibility(visible, now));
        if let Some(e) = e {
            self.execute(e, w, cx);
        }
        if visible {
            self.prepare(w, cx);
            self.resolve_scope(cx);
            self.arm(cx);
        }
        cx.notify();
    }
    pub fn core_event(&mut self, e: &CoreEvent, w: &mut Window, cx: &mut Context<Self>) {
        let CoreEvent::Other { channel, payload } = e else {
            return;
        };
        if channel == "extension:session-disposed"
            && self
                .session
                .as_ref()
                .is_some_and(|s| payload["sessionId"] == s.key.session_id)
        {
            self.retire(cx);
            self.error = Some("Extension session ended. Run the command again.");
            cx.notify();
            return;
        }
        if channel == "extension:ui" {
            if !self.visible
                || payload["source"].as_u64() != Some(self.host_generation)
                || !self
                    .session
                    .as_ref()
                    .is_some_and(|s| payload["sessionId"] == s.key.session_id)
            {
                return;
            }
            match payload["kind"].as_str() {
                Some("close") => self.emit_effect(ExtensionEffect::Close, cx),
                Some("preferences") => self.emit_effect(ExtensionEffect::Preferences, cx),
                Some("clear-search") => {
                    self.search.update(cx, |f, cx| f.clear_silently(w, cx));
                    self.search_text(String::new(), w, cx);
                }
                Some("hud") => {
                    if let Some(s) = self.session.as_mut() {
                        s.notice_message = payload["message"].as_str().map(str::to_owned);
                    }
                }
                _ => {}
            }
        }
        if channel == "extension:session-invalidated" {
            let e = self.session.as_mut().and_then(|s| {
                s.invalidate(
                    payload["sessionId"].as_str().unwrap_or(""),
                    payload["generation"].as_str(),
                    payload["revision"].as_u64().unwrap_or(0),
                )
            });
            if let Some(e) = e {
                self.execute(e, w, cx);
            }
        } else if channel == "extension-worker-status"
            && matches!(
                payload["phase"].as_str(),
                Some("failed" | "stopped" | "error")
            )
        {
            self.error = Some("Extension worker stopped. Run the command again.");
            self.timer = None;
            // EOF invalidates callbacks before any late response can alter this owner.
            self.retire(cx);
        }
        cx.notify();
    }
    pub fn drain(&self) -> ExtensionFuture {
        self.transport.drain()
    }
    pub fn dispose(&mut self, cx: &mut App) {
        if self.disposed {
            return;
        }
        self.visible = false;
        self.retire(cx);
        self.disposed = true;
        self.events = None;
    }
    fn retire(&mut self, cx: &mut App) {
        self.timer = None;
        self.job = None;
        self.queue.clear();

        self.host_tickets.clear();
        self.color_ticket = None;
        self.auxiliary.clear();
        self.release_images(cx);
        self.scope = None;
        self.fields.clear();
        self.documents.clear();
        self.rows.clear();
        self.preference = Default::default();
        if let Some(mut s) = self.session.take() {
            if let Some(Effect::Dispose(key)) = s.dispose() {
                let future = self
                    .transport
                    .send(ExtensionRequest::Dispose(key.session_id));
                // Retained independently: view release/hide must not cancel an admitted Dispose.
                gpui_tokio::Tokio::spawn(cx, future).detach();
            }
        }
    }
    fn arm(&mut self, cx: &mut Context<Self>) {
        self.timer = None;
        if !self.visible || self.disposed || self.session.is_none() || !self.polling {
            return;
        }
        // One bounded wake owner. Model honors event-driven null refreshMs and intervals.
        self.timer = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let keep = this
                    .update(cx, |v, cx| {
                        if !v.visible || v.disposed || !v.polling {
                            return false;
                        }
                        let now = v.now();
                        let e = v.session.as_mut().and_then(|s| s.tick(now));
                        if let Some(e) = e {
                            let window = v.window;
                            let owner = cx.entity().downgrade();
                            cx.defer(move |cx| {
                                let _ = window.update(cx, |_, w, cx| {
                                    let _ = owner.update(cx, |v, cx| v.execute(e, w, cx));
                                });
                            });
                        }
                        true
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        }));
    }
    pub fn event(&mut self, event: UiEvent, w: &mut Window, cx: &mut Context<Self>) {
        if !self.visible || self.disposed {
            return;
        }
        // Actions never overlap. A search burst is coalesced separately below.
        if self.job.is_some()
            && matches!(
                event,
                UiEvent::Primary | UiEvent::Secondary | UiEvent::Action(_) | UiEvent::Menu(_)
            )
        {
            return;
        }
        self.error = None;
        let open_actions = matches!(event, UiEvent::OpenActions);
        let close_actions = matches!(event, UiEvent::CloseActions);
        let result = self.session.as_mut().map(|s| {
            if let UiEvent::Action(id) = &event {
                if s.actions
                    .iter()
                    .any(|a| a.id == *id && a.kind.as_deref() == Some("submit-form"))
                {
                    s.form.validate()?;
                    return s.invoke(id, Some(s.form.values().clone())).map(Some);
                }
            }
            s.ui_event(event)
        });
        match result {
            Some(Ok(Some(e))) => self.execute(e, w, cx),
            Some(Err(_)) => self.error = Some("Check extension fields and try again."),
            _ => {}
        }
        if open_actions {
            self.action_search.focus_handle(cx).focus(w, cx);
        } else if close_actions {
            self.focus_content(w, cx);
        }
        self.prepare(w, cx);
        self.schedule_viewport(w, cx);
        cx.notify();
    }
    fn search_text(&mut self, text: String, w: &mut Window, cx: &mut Context<Self>) {
        if !self.visible {
            return;
        }
        self.queue
            .retain(|(_, r)| !matches!(r, ExtensionRequest::SearchText { .. }));
        if let Some(e) = self.session.as_mut().and_then(|s| s.search(text)) {
            self.execute(e, w, cx);
        }
        self.prepare(w, cx);
        cx.notify();
    }
    fn execute(&mut self, e: Effect, w: &mut Window, cx: &mut Context<Self>) {
        let (t, r) = match e {
            Effect::Refresh(t) => (
                t.clone(),
                ExtensionRequest::RefreshOwned {
                    session: t.key.session_id.clone(),
                    view_revision: self.session.as_ref().map_or(0, |s| s.view_revision),
                },
            ),
            Effect::Search { token: t, text } => (
                t.clone(),
                ExtensionRequest::SearchText {
                    session: t.key.session_id.clone(),
                    text,
                },
            ),
            Effect::LoadMore(t) => (
                t.clone(),
                ExtensionRequest::LoadMore(t.key.session_id.clone()),
            ),
            Effect::Invoke {
                token: t,
                action_id,
                form_values,
            } => (
                t.clone(),
                ExtensionRequest::InvokeOwned {
                    source: self.host_generation,
                    session: t.key.session_id.clone(),
                    action: action_id,
                    values: form_values,
                },
            ),
            Effect::Leave => {
                self.emit_effect(ExtensionEffect::Back, cx);
                return;
            }
            Effect::Open(url) => {
                self.quiet(ExtensionRequest::Open(url), cx);
                return;
            }
            Effect::Toast { title, message } => {
                self.emit_effect(
                    ExtensionEffect::Notice(if message.is_empty() { title } else { message }),
                    cx,
                );
                return;
            }
            Effect::Hud(message) => {
                let _ = message; // Displayed by the retained footer; diagnostics never replay host effects.
                return;
            }
            Effect::PickFiles {
                token,
                field_id,
                multiple,
            } => {
                self.next_ticket += 1;
                self.host_tickets
                    .insert(self.next_ticket, (token, field_id));
                self.emit_effect(
                    ExtensionEffect::PickFiles {
                        ticket: self.next_ticket,
                        multiple,
                    },
                    cx,
                );
                return;
            }
            Effect::NativeColorWheel { token, coordinates } => {
                if self.session.as_ref().is_some_and(|s| s.accepts(&token)) {
                    let values = ["hue", "saturation", "value", "alpha"].map(|key| {
                        coordinates[key]
                            .as_f64()
                            .unwrap_or(if key == "alpha" { 1. } else { 100. })
                    });
                    self.wheel_update(values, w, cx);
                }
                return;
            }
            Effect::Dispose(key) => {
                self.quiet(ExtensionRequest::Dispose(key.session_id), cx);
                return;
            }
        };
        if self.queue.len() >= 16 {
            self.error = Some("Extension is busy. Try again.");
            return;
        }
        self.queue.push_back((t, r));
        self.pump(w, cx);
    }
    fn pump(&mut self, _w: &mut Window, cx: &mut Context<Self>) {
        if self.job.is_some() {
            return;
        }
        while let Some((token, request)) = self.queue.pop_front() {
            if !self.session.as_ref().is_some_and(|s| s.accepts(&token)) {
                let now = self.now();
                let next = self
                    .session
                    .as_mut()
                    .and_then(|s| s.refresh_finished(&token, None, now).ok().flatten());
                if let Some(next) = next {
                    self.execute(next, _w, cx);
                }
                continue;
            }
            let future = self.transport.send(request);
            let window = self.window;
            self.job = Some(cx.spawn(async move |this, cx| {
                let result = gpui_tokio::Tokio::spawn(cx, async move { future.await })
                    .await
                    .unwrap_or_else(|_| Err("Extension interrupted".into()));
                let _ = cx.update_window(window, |_, w, cx| {
                    let _ = this.update(cx, |v, cx| {
                        v.job = None;
                        v.complete(token, result, w, cx);
                    });
                });
            }));
            break;
        }
    }
    fn complete(
        &mut self,
        token: RequestToken,
        result: Result<Value, String>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = self.now();
        let Some(s) = self.session.as_mut() else {
            return;
        };
        // Stale successes AND failures release model refresh ownership without
        // parsing or presenting their payload in the latest screen.
        if !s.accepts(&token) {
            let next = s.refresh_finished(&token, None, now).ok().flatten();
            if let Some(next) = next {
                self.execute(next, w, cx);
            }
            self.pump(w, cx);
            return;
        }
        self.error = None;
        let next = match result {
            Ok(v) if v["ok"] == true && v["mode"] == "view" => match TreeReply::from_wire(v) {
                Ok(r) => {
                    self.polling = !matches!(
                        r.refresh,
                        tezbar_ui_model::extensions::runtime::PollPolicy::EventDriven
                    );
                    match s.refresh_finished(&token, Some(r), now) {
                        Ok(e) => e,
                        Err(_) => {
                            self.error = Some("Invalid extension response. Retry.");
                            s.failed(&token, now)
                        }
                    }
                }
                Err(_) => {
                    self.error = Some("Unsupported extension response. Retry.");
                    s.failed(&token, now)
                }
            },
            Ok(v) if v["ok"] == true && v["mode"] == "unchanged" => {
                s.refresh_finished(&token, None, now).ok().flatten()
            }
            Ok(v) if v["ok"] == true && v["mode"] == "no-view" => {
                let effects = serde_json::from_value(v["effects"].clone()).unwrap_or_default();
                s.no_view(
                    &token,
                    v["message"].as_str().unwrap_or("").to_owned(),
                    effects,
                    v["effectSequence"].as_u64(),
                    now,
                );
                None
            }
            _ => {
                self.error = Some("Extension request failed. Try again.");
                s.failed(&token, now)
            }
        };
        if let Some(e) = next {
            self.execute(e, w, cx);
        }
        if self.polling && self.timer.is_none() {
            self.arm(cx);
        } else if !self.polling {
            self.timer = None;
        }
        self.drain_model_effects(w, cx);
        self.prepare(w, cx);
        self.pump(w, cx);
        cx.notify();
    }
    fn drain_model_effects(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let effects = self
            .session
            .as_mut()
            .map(|s| s.drain_effects().collect::<Vec<_>>())
            .unwrap_or_default();
        for effect in effects {
            self.execute(effect, w, cx);
        }
        // Production WorkerRecords produced this fresh notice. Never derive OS
        // commands or repeat HUDs from the cumulative wire diagnostics ourselves.
        if let Some(message) = self.session.as_ref().and_then(|s| s.notice_message.clone()) {
            let _ = message; // Displayed by the retained footer; diagnostics never replay host effects.
        }
    }
    pub fn color_chosen(
        &mut self,
        ticket: u64,
        result: Result<[f64; 4], String>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((expected, key, generation)) = self.color_ticket.take() else {
            return;
        };
        if expected != ticket {
            self.color_ticket = Some((expected, key, generation));
            return;
        }
        if !self.visible
            || !self
                .session
                .as_ref()
                .is_some_and(|s| s.key == key && s.generation == generation)
        {
            return;
        }
        if let Ok(values) = result {
            if values.iter().all(|v| v.is_finite()) {
                let Some(s) = self.session.as_mut() else {
                    return;
                };
                s.form.wheel = Some(values);
                for (id, value) in ["hue", "saturation", "value", "alpha"]
                    .into_iter()
                    .zip(values)
                {
                    let _ = s.form.change(id, json!(value.to_string()));
                }
                let callback = s
                    .root
                    .props
                    .get("nativeColorWheel")
                    .and_then(|v| v["changeActionId"].as_str())
                    .map(str::to_owned);
                if let Some(callback) = callback {
                    let form_values = ["hue", "saturation", "value", "alpha"]
                        .into_iter()
                        .zip(values)
                        .map(|(id, value)| (id.into(), json!(value)))
                        .collect();
                    match s.invoke(&callback, Some(form_values)) {
                        Ok(effect) => self.execute(effect, w, cx),
                        Err(_) => {
                            self.error =
                                Some("Colour action is unavailable. Run the command again.")
                        }
                    }
                }
                self.prepare(w, cx);
                cx.notify();
            }
        }
    }
    fn quiet(&mut self, request: ExtensionRequest, cx: &mut Context<Self>) {
        if self.auxiliary.len() >= 16 {
            self.error = Some("Extension is busy. Try again.");
            return;
        }
        self.next_ticket += 1;
        let ticket = self.next_ticket;
        let future = self.transport.send(request);
        let task = cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move {
                let v = future.await?;
                success(&v)
            })
            .await;
            let _ = this.update(cx, |v, cx| {
                v.auxiliary.remove(&ticket);
                if !matches!(result, Ok(Ok(()))) {
                    v.error = Some("Extension operation failed. Retry.");
                }
                cx.notify();
            });
        });
        self.auxiliary.insert(ticket, task);
    }
    pub fn files_chosen(
        &mut self,
        ticket: u64,
        result: Result<Vec<String>, String>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.visible || self.disposed {
            return;
        }
        let Some((token, id)) = self.host_tickets.remove(&ticket) else {
            return;
        };
        if !self.session.as_ref().is_some_and(|s| s.accepts(&token)) {
            return;
        }
        if let Ok(paths) = result {
            self.event(
                UiEvent::ChangeField {
                    id,
                    value: json!(paths),
                },
                w,
                cx,
            );
        }
    }
    fn prepare(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.image_light = tezbar_ui::tokens::tokens(w).light;
        let Some(s) = self.session.as_ref() else {
            return;
        };
        self.rows = view::rows(s);
        let root = s.root.clone();
        self.search.update(cx, |f, _| {
            f.configure(FieldOptions {
                name: "Search extension".into(),
                placeholder: root
                    .string("searchBarPlaceholder")
                    .unwrap_or("Search")
                    .to_owned()
                    .into(),
                ..Default::default()
            })
        });
        let mut nodes = vec![];
        root.visit(&mut nodes);
        let node_by_field = nodes
            .iter()
            .filter(|n| n.kind.field())
            .enumerate()
            .map(|(i, n)| (view::field_id(n, i), *n))
            .collect::<BTreeMap<_, _>>();
        let definitions = s
            .form
            .fields()
            .map(|f| (f.id.clone(), f.kind, f.error.clone()))
            .collect::<Vec<_>>();
        let values = s.form.values().clone();
        let mut keep = std::collections::BTreeSet::new();
        for (id, kind, error) in definitions {
            keep.insert(id.clone());
            if matches!(
                kind,
                NodeKind::Checkbox
                    | NodeKind::Dropdown
                    | NodeKind::TagPicker
                    | NodeKind::FilePicker
            ) {
                continue;
            }
            if !self.fields.contains_key(&id) {
                let input = cx.new(TextInput::new);
                let field = id.clone();
                let subscription = cx.subscribe_in(&input, w, move |v, _, e: &Committed, w, cx| {
                    v.event(
                        UiEvent::ChangeField {
                            id: field.clone(),
                            value: json!(e.0),
                        },
                        w,
                        cx,
                    )
                });
                self.fields.insert(
                    id.clone(),
                    FieldEntry {
                        input,
                        _subscription: subscription,
                    },
                );
            }
            let n = node_by_field.get(&id).copied();
            let entry = &self.fields[&id];
            entry.input.update(cx, |f, cx| {
                f.configure(FieldOptions {
                    name: n
                        .and_then(|n| n.string("title"))
                        .unwrap_or(&id)
                        .to_owned()
                        .into(),
                    placeholder: n
                        .and_then(|n| n.string("placeholder"))
                        .unwrap_or("")
                        .to_owned()
                        .into(),
                    multiline: kind == NodeKind::TextArea,
                    password: kind == NodeKind::PasswordField,
                    error: error.map(Into::into),
                    ..Default::default()
                });
                let text = values.get(&id).and_then(Value::as_str).unwrap_or("");
                if f.content.as_ref() != text && f.marked_range.is_none() {
                    f.replace_silently(
                        text,
                        tezbar_ui::text_input::ByteSelection {
                            range: text.len()..text.len(),
                            reversed: false,
                        },
                        w,
                        cx,
                    );
                }
            });
        }
        self.fields.retain(|id, _| keep.contains(id));
        self.prepare_documents(&root, w, cx);
        if root.kind == NodeKind::PreferenceSetup {
            self.prepare_preferences(&root, w, cx);
        }
        if self.visible {
            self.prepare_images(cx);
        }
    }
    fn prepare_documents(&mut self, root: &Node, _w: &mut Window, cx: &mut Context<Self>) {
        let docs = if matches!(root.kind, NodeKind::List | NodeKind::Grid) {
            self.session
                .as_ref()
                .and_then(|s| s.selection.as_ref())
                .and_then(|selected| self.rows.iter().find(|r| &r.id == selected))
                .and_then(|r| view::item_detail(&r.node))
                .map(|n| view::documents(&n))
                .unwrap_or_default()
        } else {
            view::documents(root)
        };
        self.documents.retain(|id, _| docs.contains_key(id));
        for (id, text) in docs {
            if !self.documents.contains_key(&id) {
                let view = cx.new(|_| MarkdownView::default());
                let doc_id = id.clone();
                let sub = cx.subscribe(
                    &view,
                    move |v, _, effect: &DocumentEffect, cx| match effect {
                        DocumentEffect::OpenLink(url) => {
                            v.quiet(ExtensionRequest::Open(url.clone()), cx)
                        }
                        DocumentEffect::Copy(text) => {
                            v.quiet(ExtensionRequest::Copy(text.clone()), cx)
                        }
                        DocumentEffect::Image(ticket) => {
                            v.document_image(doc_id.clone(), ticket.clone(), cx)
                        }
                    },
                );
                self.documents.insert(
                    id.clone(),
                    DocumentEntry {
                        view,
                        _subscription: sub,
                    },
                );
            }
            let revision = text
                .bytes()
                .fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64));
            self.documents[&id]
                .view
                .update(cx, |v, cx| v.set_content(revision, &text, true, cx));
        }
    }
}
impl Focusable for RuntimeView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
