//! M7 composition: one Settings executor, source-scoped effects and the first-show boot gate.
use super::*;
use crate::surfaces::command_bar::navigation::{Navigation, Payload, UpdateInbox};
use crate::surfaces::{
    onboarding,
    settings::{EffectBarrier, SettingsContent, SettingsEffect, SettingsStore},
};
use tezbar_ui_model::{router::Surface, ui_state::Theme};
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Boot {
    Waiting,
    Reading,
    Ready,
    Onboarding,
}
pub(super) struct State {
    pub store: SettingsStore,
    pub barrier: EffectBarrier,
    pub boot: Boot,
    pub decided: bool,
    pub force_onboarding: bool,
    pub gate: Option<Task<()>>,
    pub subscription: Option<Subscription>,
    pub pending_settings: Option<Option<Navigation>>,
    pub pending_project_review: Option<(String, String)>,
    pub after_close: Option<(Surface, Payload)>,
    pub channel: String,
    pub channel_loaded: bool,
    pub layout_pending: bool,
    pub channel_lock: Arc<tokio::sync::Mutex<()>>,
    pub failed_layout: Option<SettingsEffect>,
    pub failed_channel: Option<String>,
}
impl State {
    pub fn new(store: SettingsStore) -> Self {
        Self {
            store,
            barrier: Default::default(),
            boot: Boot::Waiting,
            decided: false,
            force_onboarding: false,
            gate: None,
            subscription: None,
            pending_settings: None,
            pending_project_review: None,
            after_close: None,
            channel: String::new(),
            channel_loaded: false,
            layout_pending: false,
            channel_lock: Arc::default(),
            failed_layout: None,
            failed_channel: None,
        }
    }
}
impl Owner {
    pub(super) fn m7_open_onboarding(&mut self, cx: &mut Context<Self>) {
        if matches!(self.m7.boot, Boot::Waiting | Boot::Reading) {
            self.m7.force_onboarding = true;
            self.show_on_ready = true;
            return;
        }
        self.m7.boot = Boot::Onboarding;
        let _ = self
            .launcher
            .update(cx, |view, w, cx| view.mount_onboarding(w, cx));
        self.show(cx);
    }
    /// The retained source remains mounted; M8 dispatch can call this after its narrow release.
    pub(super) fn settings_project_review(
        &mut self,
        cwd: String,
        nonce: String,
        cx: &mut Context<Self>,
    ) {
        self.m7.pending_project_review = Some((cwd, nonce));
        self.settings_intent(None, cx);
    }
    pub(super) fn m7_deliver_project_review(&mut self, cx: &mut Context<Self>) {
        let Some((cwd, nonce)) = self.m7.pending_project_review.clone() else {
            return;
        };
        let Some(window) = self.settings else {
            return;
        };
        if window
            .update(cx, |shell, w, cx| {
                if let Some(content) = &shell.content {
                    content.update(cx, |view, cx| view.review_project(cwd, nonce, w, cx));
                    true
                } else {
                    false
                }
            })
            .unwrap_or(false)
        {
            self.m7.pending_project_review = None;
        }
    }
    pub(super) fn m7_ready(&mut self, cx: &mut Context<Self>) {
        if self.m7.decided {
            if std::mem::take(&mut self.show_on_ready) {
                self.show(cx)
            }
            return;
        }
        if self.m7.boot == Boot::Reading {
            return;
        }
        if self.lifecycle.visible {
            self.show_on_ready = true;
            self.hide(cx);
        }
        self.m7.boot = Boot::Reading;
        let store = self.m7.store.clone();
        self.m7.gate = Some(cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, onboarding::boot_gate(store)).await;
            let _ = this.update(cx, |owner, cx| {
                if owner.lifecycle.quitting || owner.m7.decided || owner.m7.boot != Boot::Reading {
                    return;
                }
                owner.m7.decided = true;
                let needs = std::mem::take(&mut owner.m7.force_onboarding)
                    || matches!(result, Ok(Ok(true)));
                owner.m7.boot = if needs { Boot::Onboarding } else { Boot::Ready };
                if needs {
                    let result = owner
                        .launcher
                        .update(cx, |view, w, cx| view.mount_onboarding(w, cx));
                    if !matches!(result, Ok(Ok(()))) {
                        owner.m7.boot = Boot::Ready;
                    }
                }
                owner.m7_load_channel(cx);
                if std::mem::take(&mut owner.show_on_ready) {
                    if owner.m7.boot == Boot::Ready {
                        if let Some(intent) = owner.m7.pending_settings.take() {
                            owner.settings_intent(intent, cx)
                        } else {
                            owner.show(cx)
                        }
                    } else {
                        owner.show(cx)
                    }
                }
            });
        }));
    }
    pub(super) fn m7_finished(&mut self, id: EntityId, cx: &mut Context<Self>) {
        if self.m7.boot != Boot::Onboarding {
            return;
        }
        let accepted = self
            .launcher
            .update(cx, |view, w, cx| view.finish_onboarding(id, w, cx))
            .unwrap_or(false);
        if !accepted {
            return;
        }
        self.m7.boot = Boot::Ready;
        if let Some(intent) = self.m7.pending_settings.take() {
            self.settings_intent(intent, cx)
        } else {
            self.show(cx);
        }
    }
    pub(super) fn m7_origin(
        &self,
        id: EntityId,
        cx: &mut Context<Self>,
    ) -> Option<Entity<SettingsContent>> {
        if let Some(window) = self.settings {
            if let Ok(Some(view)) = window.update(cx, |view, _, _| {
                view.content.clone().filter(|view| view.entity_id() == id)
            }) {
                return Some(view);
            }
        }
        self.launcher
            .update(cx, |view, _, cx| view.m7_content(id, cx))
            .ok()
            .flatten()
    }
    fn m7_feedback(&self, id: EntityId, result: Result<(), ()>, cx: &mut Context<Self>) {
        if let Some(view) = self.m7_origin(id, cx) {
            view.update(cx, |view, cx| view.parent_effect_finished(result, cx));
        }
    }
    fn m7_notice(&self, id: EntityId, message: &'static str, error: bool, cx: &mut Context<Self>) {
        if let Some(view) = self.m7_origin(id, cx) {
            view.update(cx, |view, cx| {
                if error {
                    view.error = Some(message.into());
                } else {
                    view.message = Some(message.into());
                }
                cx.notify();
            });
        }
    }
    fn m7_finish_effect(
        &self,
        id: EntityId,
        key: &'static str,
        ticket: u64,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        if self.m7.barrier.finish(key, ticket, result.is_ok()) {
            self.m7_feedback(id, result.map_err(|_| ()), cx);
        }
    }
    pub(super) fn m7_effect(
        &mut self,
        id: EntityId,
        effect: SettingsEffect,
        cx: &mut Context<Self>,
    ) {
        if self.m7_origin(id, cx).is_none() {
            return;
        }
        match effect {
            SettingsEffect::Close => {} // The Settings shell owns this handshake exactly once.
            SettingsEffect::Theme(theme) => {
                let choice = match theme {
                    Theme::White => "white",
                    Theme::Dark => "dark",
                    Theme::System => "system",
                };
                let _ = tezbar_ui::tokens::set(choice);
                cx.set_window_appearance(match theme {
                    Theme::White => Some(WindowAppearance::Light),
                    Theme::Dark => Some(WindowAppearance::Dark),
                    Theme::System => None,
                });
                cx.refresh_windows();
                self.m7_save_ui(id, SettingsEffect::Theme(theme), cx)
            }
            effect @ (SettingsEffect::RememberTab(_)
            | SettingsEffect::Currency(_)
            | SettingsEffect::AutoUpdate(_)) => self.m7_save_ui(id, effect, cx),
            SettingsEffect::RetryPersistence => {
                self.m7_save_ui(id, SettingsEffect::RetryPersistence, cx);
                if let Some(effect) = self.m7.failed_layout.clone() {
                    self.m7_layout(id, effect, cx)
                }
                if let Some(channel) = self.m7.failed_channel.clone() {
                    self.m7_channel(id, channel, cx)
                }
            }
            effect @ SettingsEffect::LauncherLayout { .. } => self.m7_layout(id, effect, cx),
            SettingsEffect::UpdateChannel(channel) => self.m7_channel(id, channel, cx),
            SettingsEffect::Update(action) => {
                if !cx.has_global::<UpdateInbox>() {
                    cx.set_global(UpdateInbox::default());
                }
                match cx.global_mut::<UpdateInbox>().put(action) {
                    Ok(_) => self.m7_notice(id, "Update requested", false, cx),
                    Err(_) => self.m7_notice(id, "Could not request the update. Retry.", true, cx),
                }
            }
            SettingsEffect::LegacyCacheCleanup => self.m7_notice(
                id,
                "Import legacy data before clearing its cache. No files were removed.",
                true,
                cx,
            ),
            SettingsEffect::BrowseStore => {
                self.m7_destination(Surface::Extensions, Payload::None, cx)
            }
            SettingsEffect::ReleaseNotes => {
                self.m7_destination(Surface::ReleaseNotes, Payload::None, cx)
            }
            SettingsEffect::OpenPermissions => {
                self.m7_destination(Surface::Permissions, Payload::None, cx)
            }
            SettingsEffect::Quit => {
                let store = self.m7.store.clone();
                let barrier = self.m7.barrier.clone();
                let core = self.core.clone();
                self.tasks.push(cx.spawn(async move |this, cx| {
                    let result = gpui_tokio::Tokio::spawn(cx, async move {
                        store.drain_with(barrier).await?;
                        core.request(crate::core_client::Request::ConfirmQuit)
                            .await
                            .map(|_| ())
                    })
                    .await;
                    let _ = this.update(cx, |owner, cx| {
                        if !matches!(result, Ok(Ok(()))) {
                            owner.m7_feedback(id, Err(()), cx)
                        }
                    });
                }));
            }
        }
    }
    fn m7_save_ui(&mut self, id: EntityId, effect: SettingsEffect, cx: &mut Context<Self>) {
        let ticket = self.m7.barrier.begin("ui-state");
        let write = self
            .launcher
            .update(cx, |view, _, cx| view.save_settings_ui(&effect, cx));
        self.tasks.push(cx.spawn(async move |this, cx| {
            let result = match write {
                Ok(write) => write.await,
                Err(_) => Err("Preference owner is unavailable".into()),
            };
            let _ = this.update(cx, |owner, cx| {
                owner.m7_finish_effect(id, "ui-state", ticket, result, cx);
                owner.m7_refresh_content(cx);
            });
        }));
    }
    fn m7_destination(&mut self, surface: Surface, payload: Payload, cx: &mut Context<Self>) {
        if self.settings.is_some() {
            self.m7.after_close = Some((surface, payload));
            self.request_settings_close(cx)
        } else {
            let _ = self.launcher.update(cx, |view, _, cx| {
                view.route_from_settings(surface, payload, cx)
            });
            self.show(cx)
        }
    }
    pub(super) fn m7_refresh_content(&mut self, cx: &mut Context<Self>) {
        let Ok((state, status)) = self.launcher.update(cx, |view, _, _| {
            (view.state.clone(), view.update_status.clone())
        }) else {
            return;
        };
        let layout = self.m7_layout_snapshot(cx);
        if let Some(window) = self.settings {
            let channel = self.m7.channel.clone();
            let _ = window.update(cx, |view, w, cx| {
                if let Some(content) = &view.content {
                    content.update(cx, |view, cx| {
                        view.set_ui_state(&state, cx);
                        view.set_update(status, channel, cx);
                        if let Some((name, profile)) = layout {
                            view.set_layout(name, profile, w, cx)
                        }
                    });
                }
            });
        }
    }
    pub(super) fn m7_layout_snapshot(&self, cx: &mut Context<Self>) -> Option<(String, Profile)> {
        self.launcher
            .update(cx, |view, w, cx| {
                let display = w.display(cx)?;
                Some((
                    format!("Display {:?}", display.id()),
                    Profile {
                        width: f64::from(w.bounds().size.width),
                        height: f64::from(w.bounds().size.height),
                        zoom: view.zoom as f64,
                    },
                ))
            })
            .ok()
            .flatten()
    }
    fn m7_load_channel(&mut self, cx: &mut Context<Self>) {
        if self.m7.channel_loaded {
            return;
        }
        self.m7.channel_loaded = true;
        let directory = self.core.0.ctx.paths.data_dir.clone();
        self.tasks.push(cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move {
                tokio::task::spawn_blocking(move || {
                    let version = env!("CARGO_PKG_VERSION")
                        .parse()
                        .map_err(|_| "Invalid app version".to_string())?;
                    tezbar_update::release_channels::read_channel(
                        &directory,
                        tezbar_update::release_channels::NORMAL_APP_ID,
                        &version,
                    )
                    .map(|channel| channel.name().to_string())
                })
                .await
                .map_err(|_| "Could not read update preferences".to_string())
                .and_then(|r| r)
            })
            .await;
            let _ = this.update(cx, |owner, cx| {
                if let Ok(Ok(channel)) = result {
                    owner.m7.channel = channel;
                    owner.m7_refresh_content(cx);
                }
            });
        }));
    }
    fn m7_channel(&mut self, id: EntityId, channel: String, cx: &mut Context<Self>) {
        use tezbar_update::release_channels::ReleaseChannel;
        let choice = match channel.as_str() {
            "stable" => ReleaseChannel::Stable,
            "beta" => ReleaseChannel::Beta,
            "alpha" => ReleaseChannel::Alpha,
            _ => {
                self.m7_notice(id, "Choose a valid release channel.", true, cx);
                return;
            }
        };
        let busy = self
            .launcher
            .update(cx, |view, _, _| {
                matches!(
                    view.update_status,
                    tezbar_update::AppUpdateStatus::Downloading { .. }
                        | tezbar_update::AppUpdateStatus::Ready { .. }
                )
            })
            .unwrap_or(true);
        if busy {
            self.m7_notice(
                id,
                "Finish the current update before changing channels.",
                true,
                cx,
            );
            return;
        }
        self.m7.failed_channel = Some(channel.clone());
        let ticket = self.m7.barrier.begin("update-channel");
        let directory = self.core.0.ctx.paths.data_dir.clone();
        let lock = self.m7.channel_lock.clone();
        let barrier = self.m7.barrier.clone();
        self.tasks.push(cx.spawn(async move |this, cx| {
            let outcome = gpui_tokio::Tokio::spawn(cx, async move {
                let _guard = lock.lock().await;
                if !barrier.current("update-channel", ticket) {
                    return (Ok(()), None);
                }
                tokio::task::spawn_blocking(move || {
                    let result = tezbar_update::release_channels::write_channel(&directory, choice);
                    let actual = env!("CARGO_PKG_VERSION")
                        .parse()
                        .ok()
                        .and_then(|version| {
                            tezbar_update::release_channels::read_channel(
                                &directory,
                                tezbar_update::release_channels::NORMAL_APP_ID,
                                &version,
                            )
                            .ok()
                        })
                        .map(|channel| channel.name().to_string());
                    (result, actual)
                })
                .await
                .unwrap_or_else(|_| (Err("Could not save update preferences".into()), None))
            })
            .await
            .unwrap_or_else(|_| (Err("Could not save update preferences".into()), None));
            let (result, actual) = outcome;
            let current = this
                .update(cx, |owner, _| {
                    owner.m7.barrier.current("update-channel", ticket)
                })
                .unwrap_or(false);
            let ui = if current && result.is_ok() {
                this.update(cx, |owner, cx| {
                    owner.launcher.update(cx, |view, _, cx| {
                        view.update_status = tezbar_update::AppUpdateStatus::Idle;
                        view.reset_update_check(cx)
                    })
                })
                .ok()
                .and_then(Result::ok)
            } else {
                None
            };
            let result = if let Some(ui) = ui { ui.await } else { result };
            let _ = this.update(cx, |owner, cx| {
                if owner.m7.barrier.current("update-channel", ticket) {
                    if let Some(actual) = actual {
                        owner.m7.channel = actual;
                    }
                    if result.is_ok() {
                        owner.m7.failed_channel = None;
                    }
                }
                owner.m7_finish_effect(id, "update-channel", ticket, result, cx);
                owner.m7_refresh_content(cx);
            });
        }));
    }
    fn m7_layout(&mut self, id: EntityId, effect: SettingsEffect, cx: &mut Context<Self>) {
        let ticket = self.m7.barrier.begin("layout");
        self.m7.failed_layout = Some(effect.clone());
        if self.m7.layout_pending {
            self.m7_finish_effect(
                id,
                "layout",
                ticket,
                Err("Layout save is still finishing".into()),
                cx,
            );
            return;
        }
        let SettingsEffect::LauncherLayout {
            width,
            height,
            zoom,
            reset,
        } = effect
        else {
            return;
        };
        let result = self
            .launcher
            .update(cx, |view, w, cx| -> Result<(), String> {
                let display = w.display(cx).ok_or("Launcher display is unavailable")?;
                let work = display.visible_bounds();
                let mut profile = if reset {
                    Profile::default()
                } else {
                    Profile {
                        width: width.unwrap_or(f64::from(w.bounds().size.width)),
                        height: height.unwrap_or(f64::from(w.bounds().size.height)),
                        zoom: zoom.unwrap_or(view.zoom as f64),
                    }
                };
                profile = profile.fit((f64::from(work.size.width), f64::from(work.size.height)));
                let mut bounds = w.bounds();
                bounds.size = size(px(profile.width as f32), px(profile.height as f32));
                view.zoom = profile.zoom as f32;
                native::place_scaled(w, bounds, native::display_scale(display.as_ref()))?;
                cx.notify();
                Ok(())
            })
            .map_err(|_| "Launcher window is unavailable".to_string())
            .and_then(|r| r);
        if result.is_err() {
            self.m7_finish_effect(id, "layout", ticket, result, cx);
            return;
        }
        self.snapshot(cx);
        if let Err(error) = self
            .writer
            .snapshot(self.positions.positions.clone(), self.layouts.clone())
        {
            self.m7_finish_effect(id, "layout", ticket, Err(error), cx);
            return;
        }
        self.m7.layout_pending = true;
        let write = self.writer.drain();
        self.tasks.push(cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move {
                match write {
                    Some(write) => write
                        .await
                        .map_err(|_| "Could not save layout".to_string())
                        .and_then(|r| r),
                    None => Err("Layout writer is unavailable".into()),
                }
            })
            .await
            .unwrap_or_else(|_| Err("Could not save layout".into()));
            let final_write = this
                .update(cx, |owner, _| {
                    owner.m7.layout_pending = false;
                    owner.writer = windows::persistence::Writer::new(
                        owner.core.0.ctx.config.clone(),
                        owner.preview.root.join("config/launcher-layouts-v1.json"),
                    );
                    owner.positions.dirty = true;
                    if owner.lifecycle.quitting {
                        let _ = owner
                            .writer
                            .snapshot(owner.positions.positions.clone(), owner.layouts.clone());
                        owner.writer.drain()
                    } else {
                        owner.flush();
                        None
                    }
                })
                .ok()
                .flatten();
            let result = if let Some(write) = final_write {
                gpui_tokio::Tokio::spawn(cx, async move {
                    write
                        .await
                        .map_err(|_| "Could not save layout".to_string())
                        .and_then(|r| r)
                })
                .await
                .unwrap_or_else(|_| Err("Could not save layout".into()))
            } else {
                result
            };
            let _ = this.update(cx, |owner, cx| {
                if result.is_ok() && owner.m7.barrier.current("layout", ticket) {
                    owner.m7.failed_layout = None;
                }
                owner.m7_finish_effect(id, "layout", ticket, result, cx);
                owner.m7_refresh_content(cx);
            });
        }));
    }
}
