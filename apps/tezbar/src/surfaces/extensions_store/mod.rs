//! Retained Store. Catalog owns paging/install transitions; core owns acquisition/safety.
mod artwork;
pub mod request;
#[cfg(test)]
mod tests;
mod view;
use crate::events::{CoreEvent, CoreEvents, EventSubscription};
use gpui::{prelude::*, *};
use request::{ExtensionFuture, ExtensionRequest, ExtensionTransport, success};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{Duration, Instant},
};
use tezbar_ui::{
    markdown::{DocumentEffect, MarkdownView},
    text_input::{Committed, FieldOptions, TextInput},
};
use tezbar_ui_model::extensions::catalog::{Catalog, Effect, Entry, VisibleRefresh, merge_catalog};
#[derive(Clone)]
pub enum StoreEffect {
    Settings,
    Back,
    Notice(String),
    OpenRuntime(Value),
    Preferences(String),
}
impl std::fmt::Debug for StoreEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StoreEffect(<redacted>)")
    }
}
pub struct StoreEffectEmission {
    pub generation: u64,
    pub effect: StoreEffect,
}
impl EventEmitter<StoreEffectEmission> for StoreView {}
pub struct StoreView {
    transport: Arc<dyn ExtensionTransport>,
    catalog: Catalog,
    search: Entity<TextInput>,
    focus: FocusHandle,
    footer: Option<AnyView>,
    window: AnyWindowHandle,
    subscriptions: Vec<Subscription>,
    events: Option<EventSubscription>,
    rows: Vec<Entry>,
    scroll: UniformListScrollHandle,
    visible: bool,
    disposed: bool,
    refresh: VisibleRefresh,
    start: Instant,
    epoch: u64,
    query_revision: u64,
    pagination_scheduled: bool,
    search_job: Option<Task<()>>,
    debounce: Option<Task<()>>,
    timer: Option<Task<()>>,
    jobs: BTreeSet<u64>,
    next: u64,
    busy: BTreeSet<String>,
    remove: Option<String>,
    cancel_focus: FocusHandle,
    confirm_focus: FocusHandle,
    detail: Entity<MarkdownView>,
    error: Option<String>,
    artwork: artwork::Artwork,
}
impl StoreView {
    fn emit_effect(&self, effect: StoreEffect, cx: &mut Context<Self>) {
        cx.emit(StoreEffectEmission {
            generation: self.epoch,
            effect,
        });
    }

    pub fn host_generation(&self) -> u64 {
        self.epoch
    }
    pub fn new(
        transport: Arc<dyn ExtensionTransport>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(TextInput::new_native);
        search.update(cx, |v, _| {
            v.configure(FieldOptions {
                name: "Search extensions".into(),
                border: false,
                placeholder: "Search extensions or paste a GitHub URL".into(),
                ..Default::default()
            })
        });
        let sub = cx.subscribe_in(&search, w, |v, _, e: &Committed, _, cx| {
            v.query(e.0.clone(), cx)
        });
        let detail = cx.new(|_| MarkdownView::default());
        let doc = cx.subscribe(&detail, |v, _, e: &DocumentEffect, cx| match e {
            DocumentEffect::OpenLink(url) => v.quiet(ExtensionRequest::Open(url.clone()), cx),
            DocumentEffect::Copy(text) => v.quiet(ExtensionRequest::Copy(text.clone()), cx),
            DocumentEffect::Image(_) => {}
        });
        cx.on_release(|v, cx| {
            v.dispose(cx);
        })
        .detach();
        Self {
            transport,
            catalog: Default::default(),
            search,
            focus: cx.focus_handle(),
            footer: None,
            window: w.window_handle(),
            subscriptions: vec![sub, doc],
            events: None,
            rows: vec![],
            scroll: UniformListScrollHandle::new(),
            visible: false,
            disposed: false,
            refresh: Default::default(),
            start: Instant::now(),
            epoch: 0,
            query_revision: 0,
            pagination_scheduled: false,
            search_job: None,
            debounce: None,
            timer: None,
            jobs: BTreeSet::new(),
            next: 0,
            busy: BTreeSet::new(),
            remove: None,
            cancel_focus: cx.focus_handle(),
            confirm_focus: cx.focus_handle(),
            detail,
            error: None,
            artwork: Default::default(),
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
        self.events = Some(
            hub.subscribe(
                &cx.entity(),
                [
                    "extensions-changed",
                    "extension:install-progress",
                    "extension-worker-status",
                ]
                .map(str::to_owned)
                .to_vec(),
                |owner, e, cx| {
                    let _ = owner.update(cx, |v, cx| v.core_event(e, cx));
                },
                cx,
            ),
        );
    }
    pub fn activate(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.disposed = false;
        self.set_visible(true, w, cx);
        self.search.focus_handle(cx).focus(w, cx);
    }
    pub fn set_visible(&mut self, visible: bool, w: &mut Window, cx: &mut Context<Self>) {
        if visible == self.visible || self.disposed {
            return;
        }
        self.visible = visible;
        self.timer = None;
        if !visible {
            self.debounce = None;
            self.search_job = None;
            self.search.update(cx, |v, cx| v.cancel_composition(w, cx));
            self.release_artwork(cx);
            self.release_row_icons(cx);
            self.epoch += 1;
            self.remove = None;
        }
        if self.refresh.visibility(visible, self.now()) {
            self.reload(cx);
        }
        if visible {
            self.timer = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(250))
                        .await;
                    let keep = this
                        .update(cx, |v, cx| {
                            if !v.visible || v.disposed {
                                return false;
                            }
                            if v.refresh.tick(v.now()) {
                                v.reload(cx);
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
        cx.notify();
    }
    pub fn core_event(&mut self, e: &CoreEvent, cx: &mut Context<Self>) {
        if let CoreEvent::Other { channel, payload } = e {
            match channel.as_str() {
                "extensions-changed" => self.refresh.changed(self.now()),
                "extension:install-progress" => {
                    if let Some(id) = payload["extensionId"]
                        .as_str()
                        .or_else(|| payload["id"].as_str())
                    {
                        let progress =
                            payload["progress"].as_f64().unwrap_or(0.).clamp(0., 100.) as u8;
                        self.catalog.progress(id, progress);
                    }
                }
                "extension-worker-status" => {
                    if matches!(payload["phase"].as_str(), Some("failed" | "error")) {
                        self.error = Some("Extension worker stopped. Retry the operation.".into());
                    }
                }
                _ => {}
            }
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
        self.epoch += 1;
        self.disposed = true;
        self.visible = false;
        self.release_artwork(cx);
        self.release_row_icons(cx);
        self.refresh.stop();
        self.timer = None;
        self.debounce = None;
        self.search_job = None;
        self.events = None; /* admitted mutations retain independent task ownership */
    }
    fn query(&mut self, text: String, cx: &mut Context<Self>) {
        if self.remove.is_some() {
            return;
        }
        self.query_revision += 1;
        self.debounce = None;
        self.search_job = None;
        // Invalidate the production token immediately; delayed old search cannot win.
        let effect = self.catalog.search(text, false);
        self.rebuild(cx);
        let epoch = self.epoch;
        let revision = self.query_revision;
        if let Some(effect) = effect {
            self.debounce = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(180))
                    .await;
                let _ = this.update(cx, |v, cx| {
                    if v.epoch == epoch && v.query_revision == revision && v.visible {
                        v.search_effect(effect, cx);
                    }
                });
            }));
        }
        cx.notify();
    }
    fn reload(&mut self, cx: &mut Context<Self>) {
        if !self.visible || self.disposed {
            return;
        }
        if self.search_job.is_some() {
            return;
        }
        if let Some(e) = self.catalog.search(self.catalog.query.clone(), false) {
            self.search_effect(e, cx);
        }
    }
    fn more(&mut self, cx: &mut Context<Self>) {
        if !self.visible || self.disposed || self.remove.is_some() {
            return;
        }
        if let Some(e) = self.catalog.search(self.catalog.query.clone(), true) {
            self.search_effect(e, cx);
            cx.notify();
        }
    }
    fn schedule_visible_range(
        &mut self,
        range: std::ops::Range<usize>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pagination_scheduled
            || !self.visible
            || self.disposed
            || self.remove.is_some()
            || !self.catalog.has_more
            || self.catalog.loading
            || self.catalog.loading_more
            || self.catalog.pagination_failed
            || range.is_empty()
            || range.end.saturating_add(3) < self.rows.len()
        {
            return;
        }
        self.pagination_scheduled = true;
        let epoch = self.epoch;
        let revision = self.query_revision;
        let owner = cx.entity().downgrade();
        w.on_next_frame(move |_, cx| {
            let _ = owner.update(cx, |v, cx| {
                v.pagination_scheduled = false;
                if v.epoch == epoch && v.query_revision == revision {
                    v.visible_range(range, cx);
                }
            });
        });
    }
    fn visible_range(&mut self, range: std::ops::Range<usize>, cx: &mut Context<Self>) {
        if !range.is_empty()
            && range.end.saturating_add(3) >= self.rows.len()
            && !self.catalog.pagination_failed
        {
            self.more(cx);
        }
    }
    fn search_effect(&mut self, e: Effect, cx: &mut Context<Self>) {
        let Effect::Search {
            token,
            query,
            offset,
            limit,
        } = e
        else {
            return;
        };
        let t = self.transport.clone();
        let epoch = self.epoch;
        let revision = self.query_revision;
        self.search_job = Some(cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move {
                let page = t
                    .send(ExtensionRequest::Search {
                        query,
                        offset,
                        limit,
                    })
                    .await?;
                success(&page)?;
                let installed = t.send(ExtensionRequest::List).await?;
                success(&installed)?;
                let items = serde_json::from_value(page["items"].clone())
                    .map_err(|_| "Invalid catalog".to_string())?;
                let locals = decode_installed(installed)?;
                Ok::<_, String>((
                    items,
                    locals,
                    page["total"].as_u64().unwrap_or(0) as usize,
                    page["hasMore"] == true,
                ))
            })
            .await
            .unwrap_or_else(|_| Err("Interrupted".into()));
            let _ = this.update(cx, |v, cx| {
                if v.epoch != epoch || v.query_revision != revision || !v.visible {
                    return;
                }
                v.search_job = None;
                let result = result.map_err(|_| "Catalog could not be loaded. Retry.".into());
                v.catalog.loaded(token, result, false);
                v.rebuild(cx);
                cx.notify();
            });
        }));
    }
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.rows = merge_catalog(
            &self.catalog.store,
            &self.catalog.installed,
            &self.catalog.query,
        );
        if !self
            .catalog
            .selected
            .as_ref()
            .is_some_and(|id| self.rows.iter().any(|r| &r.id == id))
        {
            self.catalog.selected = self.rows.first().map(|r| r.id.clone());
        }
        self.prepare_detail(cx);
    }
    fn select(&mut self, id: String, cx: &mut Context<Self>) {
        if self.rows.iter().any(|r| r.id == id) {
            self.catalog.selected = Some(id);
            self.prepare_detail(cx);
        }
        cx.notify();
    }
    fn prepare_detail(&mut self, cx: &mut Context<Self>) {
        let text = self
            .selected()
            .map(|e| e.description.clone())
            .unwrap_or_default();
        let revision = text
            .bytes()
            .fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64));
        self.detail
            .update(cx, |v, cx| v.set_content(revision, &text, true, cx));
        self.prepare_artwork(cx);
    }
    fn selected(&self) -> Option<&Entry> {
        self.rows
            .iter()
            .find(|e| Some(&e.id) == self.catalog.selected.as_ref())
    }
    fn install(&mut self, id: String, cx: &mut Context<Self>) {
        if self.busy.contains(&id) {
            return;
        }
        // GitHub URLs follow the existing core parser and safety authorization.
        if id == self.catalog.query && github_url(&id) {
            self.catalog.installing.insert(id.clone(), 5);
            self.mutate(id, false, cx);
            return;
        }
        match self.catalog.install(&id, platform()) {
            Ok(Effect::Install(id)) => self.mutate(id, false, cx),
            _ => {
                self.error = Some("This extension cannot be installed on this platform.".into());
                cx.notify();
            }
        }
    }
    fn mutate(&mut self, id: String, remove: bool, cx: &mut Context<Self>) {
        if self.jobs.len() >= 16 || !self.busy.insert(id.clone()) {
            return;
        }
        self.next += 1;
        let ticket = self.next;
        let t = self.transport.clone();
        let epoch = self.epoch;
        let future = t.send(if remove {
            ExtensionRequest::Uninstall(id.clone())
        } else {
            ExtensionRequest::Install(id.clone())
        });
        // Mutation completion remains owned when hidden; transport.drain is the parent barrier.
        let job = cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move {
                let v = future.await?;
                success(&v)
            })
            .await;
            let _ = this.update(cx, |v, cx| {
                v.jobs.remove(&ticket);
                v.busy.remove(&id);
                v.catalog.installed(
                    &id,
                    if matches!(result, Ok(Ok(()))) {
                        if remove {
                            "Extension removed.".into()
                        } else {
                            "Extension installed.".into()
                        }
                    } else {
                        match &result {
                            Ok(Err(e)) => format!(
                                "Could not {} extension: {}",
                                if remove { "remove" } else { "install" },
                                e.chars().take(300).collect::<String>()
                            ),
                            _ => "Extension operation interrupted. Try again.".into(),
                        }
                    },
                );
                if v.epoch == epoch {
                    v.remove = None;
                }
                v.reload(cx);
                cx.notify();
            });
        });
        job.detach();
        self.jobs.insert(ticket);
        cx.notify();
    }
    fn run(&mut self, extension: String, command: String, cx: &mut Context<Self>) {
        if self.jobs.len() >= 16 {
            return;
        }
        let t = self.transport.clone();
        let preference_extension = extension.clone();
        let cleanup = t.clone();
        let epoch = self.epoch;
        self.next += 1;
        let ticket = self.next;
        let job = cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move {
                t.send(ExtensionRequest::Run {
                    extension,
                    command,
                    arguments: Default::default(),
                })
                .await
            })
            .await;
            let _ = this.update(cx, |v, cx| {
                v.jobs.remove(&ticket);
                if v.epoch != epoch || !v.visible {
                    if let Ok(Ok(reply)) = &result {
                        if reply["mode"] == "view" && reply["root"]["type"] != "MenuBarExtra" {
                            if let Some(id) = reply["sessionId"].as_str() {
                                gpui_tokio::Tokio::spawn(
                                    cx,
                                    cleanup.send(ExtensionRequest::Dispose(id.into())),
                                )
                                .detach();
                            }
                        }
                    }
                    return;
                }
                match result {
                    Ok(Ok(result))
                        if result["ok"] == true && result["preferencesRequested"] == true =>
                    {
                        if result["mode"] == "view" && result["root"]["type"] != "MenuBarExtra" {
                            if let Some(id) = result["sessionId"].as_str() {
                                gpui_tokio::Tokio::spawn(
                                    cx,
                                    cleanup.send(ExtensionRequest::Dispose(id.into())),
                                )
                                .detach();
                            }
                        }
                        v.emit_effect(StoreEffect::Preferences(preference_extension), cx);
                    }
                    Ok(Ok(result)) if result["ok"] == true && result["mode"] == "view" => {
                        if tezbar_ui_model::extensions::runtime::TreeReply::from_wire(
                            result.clone(),
                        )
                        .is_ok()
                        {
                            v.emit_effect(StoreEffect::OpenRuntime(result), cx);
                        } else {
                            if result["root"]["type"] != "MenuBarExtra" {
                                if let Some(id) = result["sessionId"].as_str() {
                                    gpui_tokio::Tokio::spawn(
                                        cx,
                                        cleanup.send(ExtensionRequest::Dispose(id.into())),
                                    )
                                    .detach();
                                }
                            }
                            v.error = Some("Extension returned an invalid view.".into());
                        }
                    }
                    Ok(Ok(result)) if result["ok"] == true => v.emit_effect(
                        StoreEffect::Notice(
                            super::command_bar::presentation::extension_notice(&result)
                                .unwrap_or_else(|| "Command completed.".into()),
                        ),
                        cx,
                    ),
                    Ok(Ok(result)) => {
                        v.error = Some(
                            super::command_bar::presentation::extension_notice(&result)
                                .unwrap_or_else(|| "Command failed. Try again.".into())
                                .chars()
                                .take(2048)
                                .collect(),
                        )
                    }
                    Ok(Err(error)) => v.error = Some(error.chars().take(2048).collect()),
                    Err(_) => v.error = Some("Command interrupted. Try again.".into()),
                };
                cx.notify();
            });
        });
        job.detach();
        self.jobs.insert(ticket);
    }
    fn quiet(&mut self, r: ExtensionRequest, cx: &mut Context<Self>) {
        if self.jobs.len() >= 16 {
            return;
        }
        let t = self.transport.clone();
        self.next += 1;
        let ticket = self.next;
        let job = cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move {
                let v = t.send(r).await?;
                success(&v)
            })
            .await;
            let _ = this.update(cx, |v, cx| {
                v.jobs.remove(&ticket);
                if !matches!(result, Ok(Ok(()))) {
                    v.error = Some("Operation failed. Retry.".into());
                }
                cx.notify();
            });
        });
        job.detach();
        self.jobs.insert(ticket);
    }
}
fn decode_installed(v: Value) -> Result<Vec<Entry>, String> {
    serde_json::from_value::<Vec<Entry>>(if v.is_array() {
        v
    } else {
        v["extensions"].clone()
    })
    .map(|v| v.into_iter().map(Entry::normalize_installed).collect())
    .map_err(|_| "Invalid installed catalog".into())
}
fn platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "Linux"
    }
}
fn github_url(value: &str) -> bool {
    value.starts_with("https://github.com/") && !value.contains('@') && value.trim() == value
}
impl Focusable for StoreView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

#[cfg(test)]
mod followup_tests;
