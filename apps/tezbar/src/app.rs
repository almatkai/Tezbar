mod m10;
mod m11;
mod m7;
#[cfg(test)]
mod m7_tests;
mod m8;
mod m9;
pub mod production_launch;
mod quit_budget;
pub mod update_integration;
gpui::actions!(
    native_app,
    [RequestQuit, OpenSettings, ZoomIn, ZoomOut, ZoomReset]
);
use crate::{
    cli::Preview,
    core_client::CoreClient,
    events::{self, CoreEvent, Event, Relay},
    host_adapter::{GpuiHost, PrimaryTray},
    windows::{self, launcher::Launcher, native, settings::Settings},
};
use gpui::{prelude::*, *};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tezbar_core::{
    common::CoreContext,
    host::{DialogReply, DialogRequest, ShortcutRegistry, SurfaceOp},
};
use tezbar_host::{
    layout::Profile,
    placement::{self, MonitorGeometry, PersistedWindowPosition},
    single_instance::{Claim, Instance},
};
/// All theme entry points update palette and native window appearance together.
fn apply_theme(theme: tezbar_ui_model::ui_state::Theme, cx: &mut App) {
    use tezbar_ui_model::ui_state::Theme;
    let (tokens, appearance) = match theme {
        Theme::White => ("white", Some(WindowAppearance::Light)),
        Theme::Dark => ("dark", Some(WindowAppearance::Dark)),
        Theme::System => ("system", None),
    };
    let _ = tezbar_ui::tokens::set(tokens);
    cx.set_window_appearance(appearance);
    cx.refresh_windows();
}

struct RuntimeOwner(Option<tokio::runtime::Runtime>);
impl Global for RuntimeOwner {}
impl Drop for RuntimeOwner {
    fn drop(&mut self) {
        if let Some(runtime) = self.0.take() {
            runtime.shutdown_background();
        }
    }
}
struct Owner {
    m7: m7::State,
    updater: Option<update_integration::Client>,
    updater_quiescing: bool,
    updater_paused: bool,
    production_plan: Option<production_launch::LaunchPlan>,
    pending_update: Option<(u64, tezbar_update::native::VerifiedArtifact)>,
    #[cfg(target_os = "macos")]
    staged_update: Option<tezbar_update::native::StagedMac>,
    installed_executable: Option<std::path::PathBuf>,
    layout_path: std::path::PathBuf,
    settings_close: windows::settings_close::CloseHandshake,
    visibility_generation: u64,
    previous_foreground: Option<isize>,
    panel: Option<WindowHandle<windows::terminal_sessions::SessionsPanel>>,
    panel_visible: bool,
    #[cfg(feature = "bench-control")]
    _control: Option<crate::bench_control::Server>,
    #[cfg(feature = "bench-control")]
    shutdown_gate: Arc<crate::bench_control::ShutdownGate>,
    snap: windows::snap::Snap,
    overlay: Option<WindowHandle<windows::snap_overlay::SnapOverlay>>,
    writer: windows::persistence::Writer,
    core: CoreClient,
    host: Arc<GpuiHost>,
    instance: Option<Instance>,
    tray: Option<PrimaryTray>,
    launcher: WindowHandle<Launcher>,
    settings: Option<WindowHandle<Settings>>,
    lifecycle: windows::Lifecycle,
    relay: Relay,
    startup: Option<Task<()>>,
    show_on_ready: bool,
    save_error: Option<String>,
    tasks: Vec<Task<()>>,
    idle: Option<Task<()>>,
    blur: Option<Task<()>>,
    snap_timer: Option<Task<()>>,
    drag_task: Option<Task<()>>,
    settings_close_timer: Option<Task<()>>,
    positions: placement::PendingWindowPositions,
    layouts: BTreeMap<String, Profile>,
    monitor: String,
    config: serde_json::Value,
    preview: Preview,
    log: &'static tezbar_host::logging::FileLog,
    events: events::CoreEvents,
}
impl Owner {
    fn start_core(&mut self, cx: &mut Context<Self>) {
        if self.lifecycle.quitting || self.startup.as_ref().is_some_and(|task| !task.is_ready()) {
            return;
        }
        // The native launcher must remain reachable even while legacy recovery or optional
        // service startup is pending. Run on the GUI thread, before either readiness gate.
        if let Err(error) = self.core.initialize_shortcuts() {
            log::warn!("Native launcher shortcut unavailable: {error}");
        }
        if self
            .launcher
            .update(cx, |view, _, _| view.migration_pending())
            .unwrap_or(true)
        {
            self.m7.fail_open();
            self.show(cx);
            return;
        }
        let core = self.core.clone();
        self.startup = Some(cx.spawn(async move |this, cx| {
            let ready = gpui_tokio::Tokio::spawn(cx, async move { core.start().await }).await;
            let _ = this.update(cx, |owner, cx| {
                let save_error = owner.save_error.clone();
                let shortcut_error = owner.preview.launcher_shortcut.as_ref()
                    .filter(|_| owner.tray.is_none())
                    .map(|key| format!("Could not register {key}. Close the other launcher and restart this preview."));
                let _ = owner.launcher.update(cx, |view, _, cx| {
                    view.ready = matches!(ready, Ok(Ok(())));
                    view.error = match ready {
                        Ok(Err(e)) => Some(e),
                        Err(e) => Some(e.to_string()),
                        _ => save_error.or(shortcut_error),
                    };
                    cx.notify();
                });
                if owner
                    .launcher
                    .update(cx, |view, _, _| view.ready)
                    .unwrap_or(false)
                {
                    owner.m7_ready(cx)
                } else {
                    owner.m7.fail_open();
                    owner.m7_present_ready(cx);
                }
                let _ = owner.launcher.update(cx, |view, _, cx| {
                    view.core_ready(cx);
                });
            });
        }));
    }
    fn restore(&mut self, cx: &mut Context<Self>) {
        cx.set_reduce_motion(native::reduce_motion());
        if self.lifecycle.quitting {
            return;
        }
        if let Some(settings) = self.settings {
            if settings
                .update(cx, |_, w, cx| {
                    let _ = native::visible(w, true);
                    w.activate_window();
                    cx.activate(true);
                })
                .is_ok()
            {
                return;
            }
        }
        self.show(cx);
    }
    fn show(&mut self, cx: &mut Context<Self>) {
        if matches!(self.m7.boot, m7::Boot::Waiting | m7::Boot::Reading) {
            self.show_on_ready = true;
            return;
        }
        self.visibility_generation = self.visibility_generation.wrapping_add(1);
        self.idle = None;
        self.blur = None;
        let was_visible = self.lifecycle.visible;
        if !was_visible {
            self.previous_foreground = native::foreground();
        }
        if self.lifecycle.show().is_none() {
            return;
        }
        if was_visible {
            let _ = self.launcher.update(cx, |_, w, cx| {
                let _ = native::visible(w, true);
                cx.activate(true);
                w.activate_window();
            });
            return;
        }
        let config = self.config.clone();
        let positions = &self.positions.positions;
        let layouts = &self.layouts;
        let mut monitor = String::new();
        let mut placed = false;
        let _ = self.launcher.update(cx, |view, w, cx| {
            let displays = cx.displays();
            let resolve = |x: f64, y: f64| native::display_at(&displays, x, y);
            let cursor = tezbar_host::monitors::cursor_point().and_then(|(x, y)| resolve(x, y));
            let front = tezbar_host::monitors::frontmost(resolve);
            if let Some(display) = placement::select_active_monitor(
                cursor,
                front,
                || w.display(cx),
                || cx.primary_display(),
            ) {
                let b = display.bounds();
                let id = native::monitor_key(display.as_ref());
                let scale = native::display_scale(display.as_ref());
                let work = display.visible_bounds();
                monitor = id.clone();
                let profile = layouts
                    .get(&id)
                    .copied()
                    .unwrap_or_default()
                    .fit((f64::from(work.size.width), f64::from(work.size.height)));
                let saved = positions.get(&id).copied().or_else(|| {
                    placement::monitor_positions_from_config(&config)
                        .get(&id)
                        .copied()
                });
                let geometry = MonitorGeometry {
                    bounds: native::physical_rect(b, scale),
                    work_area: native::physical_rect(work, scale),
                };
                let (width, height) = profile.physical_size(scale);
                let p = placement::plan_window_position(geometry, width, height, saved);
                let bounds = Bounds::new(
                    point(px((p.x / scale) as f32), px((p.y / scale) as f32)),
                    size(px(profile.width as f32), px(profile.height as f32)),
                );
                let _ = native::place_scaled(w, bounds, scale, cx);
                view.zoom = profile.zoom as f32;
                placed = true;
                let _ = native::visible(w, true);
                view.show(w, cx);
                #[cfg(windows)]
                {
                    let _ = native::place_scaled(w, bounds, scale, cx);
                }
            }
        });
        self.monitor = monitor;
        self.lifecycle.placed = placed;
        if !was_visible {
            self.host
                .emit("window-shown", serde_json::json!({"resetUi":false}));
            self.host.emit("window-visibility", serde_json::json!(true));
        }
        if self.panel_visible {
            if let Some(panel) = self.panel {
                let _ = panel.update(cx, |_, w, _| native::visible(w, true));
            }
        }
    }
    fn snapshot(&mut self, cx: &mut Context<Self>) {
        if !self.lifecycle.placed {
            return;
        }
        if let Ok(Some((monitor, p, profile))) = self.launcher.update(cx, |v, w, cx| {
            // A queued move may change monitors/DPI. Preserve the last known-good
            // snapshot until the real OS callback reports the completed geometry.
            if native::placement_pending(w, cx) { return None; }
            let display = w.display(cx)?;
            let monitor = native::monitor_key(display.as_ref());
            let scale = native::display_scale(display.as_ref());
            let b = w.bounds();
            Some(windows::persistence::capture(monitor, scale, b, v.zoom))
        }) {
            self.monitor = monitor;
            self.positions.remember(self.monitor.clone(), p);
            self.layouts.insert(self.monitor.clone(), profile);
        }
    }
    fn flush(&mut self) {
        if self.m7.layout_pending {
            return;
        }
        if self.positions.should_flush(None) {
            match self
                .writer
                .snapshot(self.positions.positions.clone(), self.layouts.clone())
            {
                Ok(()) => self.positions.dirty = false,
                Err(e) => {
                    let _ = self.log.write(e.as_bytes());
                }
            }
        }
    }
    fn hide(&mut self, cx: &mut Context<Self>) {
        self.visibility_generation = self.visibility_generation.wrapping_add(1);
        self.end_snap(cx);
        self.snapshot(cx);
        self.flush();
        if let Some(g) = self.lifecycle.hide(Instant::now()) {
            if let Some(panel) = self.panel {
                let _ = panel.update(cx, |_, w, _| native::visible(w, false));
            }
            let _ = self.launcher.update(cx, |view, w, cx| {
                view.hide(w, cx);
                let _ = native::visible(w, false);
            });
            if !self.lifecycle.settings {
                #[cfg(target_os = "macos")]
                cx.hide();
                #[cfg(windows)]
                native::restore_foreground(self.previous_foreground.take());
            }
            self.host.emit("window-hidden", serde_json::Value::Null);
            self.host
                .emit("window-visibility", serde_json::json!(false));
            let relay = self.relay.clone();
            self.idle = Some(cx.spawn(async move |_, cx| {
                cx.background_executor().timer(Duration::from_secs(5)).await;
                let _ = relay.send(Event::Idle(g));
            }));
        }
    }
    fn settings(&mut self, cx: &mut Context<Self>) {
        self.settings_intent(None, cx)
    }
    fn settings_intent(
        &mut self,
        intent: Option<crate::surfaces::command_bar::navigation::Navigation>,
        cx: &mut Context<Self>,
    ) {
        if self
            .launcher
            .update(cx, |view, _, _| view.migration_pending())
            .unwrap_or(true)
        {
            self.show(cx);
            return;
        }
        if matches!(
            self.m7.boot,
            m7::Boot::Waiting | m7::Boot::Reading | m7::Boot::Onboarding
        ) {
            self.m7.pending_settings = Some(intent);
            self.show_on_ready = true;
            if self.m7.boot == m7::Boot::Onboarding {
                self.show(cx)
            }
            return;
        }

        self.lifecycle.settings = true; // window-only hide, before AppKit/foreground handoff
        self.hide(cx);
        if let Some(settings) = self.settings {
            self.m7_refresh_content(cx);
            let _ = settings.update(cx, |view, w, cx| {
                if let Some(content) = &view.content {
                    content.update(cx, |view, cx| {
                        view.set_visible(true, cx);
                        if let Some(intent) = &intent {
                            view.navigate(intent, w, cx)
                        }
                    });
                }
                let _ = native::visible(w, true);
                w.activate_window();
                cx.activate(true);
            });
            self.m7_deliver_project_review(cx);
            return;
        }
        let relay = self.relay.clone();
        let on_close = relay.clone();
        let (ui_state, update) = self
            .launcher
            .update(cx, |view, _, _| {
                (view.state.clone(), view.update_status.clone())
            })
            .unwrap_or_else(|_| {
                (
                    tezbar_ui_model::ui_state::UiState::default(),
                    tezbar_update::AppUpdateStatus::Idle,
                )
            });
        let layout = self.m7_layout_snapshot(cx);
        let store = self.m7.store.clone();
        let barrier = self.m7.barrier.clone();
        let events = self.events.clone();
        let channel = self.m7.channel.clone();
        let settings_bounds = Bounds::centered(None, size(px(920.), px(680.)), cx);
        match cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(settings_bounds)),
                titlebar: None,
                window_background: if cfg!(target_os = "macos") {
                    WindowBackgroundAppearance::Blurred
                } else {
                    WindowBackgroundAppearance::Transparent
                },
                kind: WindowKind::Normal,
                ..Default::default()
            },
            move |w, cx| {
                w.on_window_should_close(cx, move |_, _| {
                    let _ = on_close.send(Event::CloseSettings);
                    false
                });
                cx.new(|cx| {
                    let mut shell = Settings::new(relay, w, cx);
                    let content = cx.new(|cx| {
                        let mut view = crate::surfaces::settings::SettingsContent::new(
                            store.clone(),
                            &ui_state,
                            w,
                            cx,
                        );
                        view.attach_events(&events, cx);
                        view.set_update(update, channel, cx);
                        if let Some((name, profile)) = layout {
                            view.set_layout(name, profile, w, cx)
                        }
                        view
                    });
                    shell.set_content(content, cx);
                    shell.set_write_owner(Arc::new(move || {
                        let store = store.clone();
                        let barrier = barrier.clone();
                        store.drain_with(barrier)
                    }));
                    shell
                })
            },
        ) {
            Ok(window) => {
                self.settings = Some(window);
                let content = window
                    .update(cx, |view, _, _| view.content.clone())
                    .ok()
                    .flatten();
                if let Some(content) = content {
                    let relay = self.relay.clone();
                    self.m7.subscription = Some(cx.subscribe(
                        &content,
                        move |_, view, e: &crate::surfaces::settings::SettingsEffect, _| {
                            if !matches!(e, crate::surfaces::settings::SettingsEffect::Close) {
                                let _ =
                                    relay.send(Event::SettingsEffect(view.entity_id(), e.clone()));
                            }
                        },
                    ));
                }
                self.lifecycle.settings = true;
                let _ = window.update(cx, |shell, w, cx| {
                    if let Some(content) = &shell.content {
                        content.update(cx, |view, cx| {
                            if let Some(intent) = &intent {
                                view.navigate(intent, w, cx)
                            } else {
                                view.activate(w, cx)
                            }
                        });
                    }
                    let _ = native::visible(w, true);
                    cx.activate(true);
                    w.activate_window();
                });
                self.m7_deliver_project_review(cx);
            }
            Err(e) => {
                self.lifecycle.settings = false;
                self.show(cx);
                let _ = self.log.write(e.to_string().as_bytes());
            }
        }
    }
    fn quit(&mut self, cx: &mut Context<Self>) {
        if self.lifecycle.quitting || self.updater_quiescing {
            return;
        }
        if !self.updater_paused {
            if let Some(client) = self.updater.clone() {
                self.updater_quiescing = true;
                self.tasks.push(cx.spawn(async move |this, cx| {
                    let result = gpui_tokio::Tokio::spawn(cx, async move {
                        client.control(update_integration::Command::Quiesce).await
                    })
                    .await;
                    if !matches!(result, Ok(Ok(()))) {
                        let _ = this.update(cx, |owner, cx| {
                            owner.updater_quiescing = false;
                            let _ = owner.launcher.update(cx, |view, _, cx| {
                                view.error = Some(
                                    "Could not pause updates; quit cancelled. Retry quit.".into(),
                                );
                                cx.notify();
                            });
                        });
                    }
                }));
                return;
            }
        }
        if !self.lifecycle.begin_quit() {
            return;
        }
        self.save_error = None;
        self.snapshot(cx);
        self.flush();
        let chat_drain = self.m8_prepare_quit(cx);
        let settings_drain = self
            .settings
            .map(|window| window.update(cx, |view, _, _| view.drain_writes()));
        let launcher_writes = self
            .launcher
            .update(cx, |view, _, cx| view.drain_state(cx))
            .unwrap_or_default();
        let shared_settings = self.m7.store.clone();
        let m7_barrier = self.m7.barrier.clone();
        let drain = self.writer.drain();
        self.end_snap(cx);
        // Keep dialog, command and Settings fallback owners live if durable saves cancel quit.
        self.relay.enable_core(true);
        self.startup = None;
        self.idle = None;
        self.blur = None;
        self.snap_timer = None;
        #[cfg(feature = "bench-control")]
        let gate = self.shutdown_gate.clone();
        self.tasks.push(cx.spawn(async move |this, cx| {
            #[cfg(feature = "bench-control")]
            gate.wait().await;
            let quit_started = std::time::Instant::now();
            let timer = cx.background_executor().timer(quit_budget::LIMIT);
            let committed = std::cell::Cell::new(false);
            let refusal_source = std::cell::Cell::new(QuitRefusalSource::Durability);
            let prepared = {
            let prepare = async {
            for write in launcher_writes {
                write.await;
            }
            let launcher_error = this
                .update(cx, |owner, cx| {
                    owner
                        .launcher
                        .update(cx, |view, _, _| view.durable_write_error())
                })
                .map_err(|e| e.to_string())
                .and_then(|v| v.map_err(|e| e.to_string()))
                .and_then(|error| error.map_or(Ok(()), Err));
            let chat_result = chat_drain.await;
            let preliminary = gpui_tokio::Tokio::spawn(cx, async move {
                launcher_error?;
                chat_result?;
                shared_settings.drain_with(m7_barrier).await?;
                if let Some(settings_drain) = settings_drain {
                    settings_drain
                        .map_err(|e| format!("Settings window: {e}"))?
                        .await
                        .map_err(|e| format!("Settings: {e}"))?;
                }
                if let Some(drain) = drain {
                    drain
                        .await
                        .map_err(|e| format!("Window layout: {e}"))?
                        .map_err(|e| format!("Window layout: {e}"))
                } else {
                    Ok(())
                }
            })
            .await;
            match preliminary {
                Ok(Ok(())) => {},
                other => return Err(format!("{other:?}")),
            }
            // Quiesce extension work without destroying its view. A busy install
            // must reject quit before chat or terminal runs are cancelled.
            refusal_source.set(QuitRefusalSource::ExtensionWork);
            let extension_drain=this.update(cx,|owner,cx|owner.launcher.update(cx,|view,_,cx|view.drain_extensions(cx)))
                .map_err(|e|e.to_string())?.map_err(|e|e.to_string())?;
            gpui_tokio::Tokio::spawn(cx,extension_drain).await.map_err(|_|"Extension drain interrupted")??;
            refusal_source.set(QuitRefusalSource::Durability);
            // Every unrelated durable barrier succeeded before any Cancel/dispose.
            let final_drains = this.update(cx, |owner, cx| {
                if owner.m7.store.pending() || !owner.m7.barrier.is_clean() || owner.launcher.update(cx,|view,_,_|view.ui_durability_pending()).unwrap_or(true) {
                    return Err("Settings changed while quitting; retry quit.".to_string());
                }
                quit_budget::commit(quit_started.elapsed(), &committed, || {
                    owner.launcher.update(cx, |view, w, cx| {
                        view.dispose_chat(w, cx);
                        (view.drain_chat(w, cx), view.drain_terminal(w, cx))
                    }).map_err(|e| e.to_string())
                })?
            }).map_err(|e| e.to_string())??;
            final_drains.0.await?;
            final_drains.1.await?;
            Ok::<(), String>(())
            };
            quit_preparation(prepare, timer, &committed).await
            };
            let core = this.update(cx, |this, cx| {
                let prepared = if matches!(prepared, QuitPreparation::Saved)
                    && (this.m7.store.pending() || !this.m7.barrier.is_clean() || this.launcher.update(cx,|view,_,_|view.ui_durability_pending()).unwrap_or(true)) {
                    QuitPreparation::Refused("Settings changed while quitting; retry quit.".to_string())
                } else { prepared };
                if let QuitPreparation::Refused(error) = &prepared {
                    let durable_failure = refusal_source.get() == QuitRefusalSource::Durability;
                    this.lifecycle.quitting = false;
                    this.core.resume_extensions();
                    this.defer_update(cx);
                    this.updater_paused=false;
                    if let Some(client)=this.updater.clone() {this.tasks.push(cx.spawn(async move |_,cx|{let _=gpui_tokio::Tokio::spawn(cx,async move {client.control(update_integration::Command::Resume).await}).await;}));}
                    let _ = this.launcher.update(cx, |view, _, cx| view.resume_cancelled_quit(cx));
                    this.save_error = Some(format!(
                        "Quit cancelled: {error}"
                    ));
                    this.writer = windows::persistence::Writer::new(
                        this.core.0.ctx.config.clone(),
                        this.layout_path.clone(),
                    );
                    this.positions.dirty = true;
                    this.flush();
                    let _ = this.log.write(
                        format!(
                            "Quit cancelled: {error}\n"
                        )
                        .as_bytes(),
                    );
                    if durable_failure {
                        this.settings(cx); // Keep durable-save failures visible across launcher queries.
                        if let Some(settings) = this.settings {
                            let _ = settings.update(cx, |view, _, cx| {
                                view.error = Some(format!(
                                    "Couldn't save changes; quit cancelled. Retry: {error}"
                                ));
                                cx.notify();
                            });
                        }
                    }
                    let _ = this.launcher.update(cx, |view, _, cx| {
                        view.error = Some(format!(
                            "Quit cancelled: {error}"
                        ));
                        if view.onboarding_active {if let Some(onboarding)=&view.onboarding {onboarding.update(cx,|view,cx|{view.error=Some(if durable_failure {"Could not save changes; quit cancelled. Retry saved changes.".into()} else {format!("Quit cancelled: {error}")});cx.notify();});}}
                        cx.notify();
                    });
                    windows::termination::complete(false);
                    this.restore(cx);
                    this.start_core(cx); // Reattach the UI waiter after a cancelled quit.
                    return None;
                }
                this.events.dispose();
                if matches!(prepared, QuitPreparation::ForcedAfterCommit) {
                    let _ = this.log.write(b"Quit deadline reached after cancellation began; final writes may be incomplete. Exiting without claiming them saved.\n");
                }
                let _ = this.launcher.update(cx, |view, _, cx| {
                    view.dispose_extensions(cx);
                    view.ready = false;
                    cx.notify();
                });
                if let Some(instance) = &this.instance {
                    instance.begin_shutdown();
                }
                if let Some(tray) = this.tray.take() {
                    if let Some(shortcut) = &tray.shortcut {
                        let _ = this.host.desktop.unregister(shortcut);
                    }
                }
                Some(this.core.clone())
            });
            let Ok(Some(core)) = core else {
                return;
            };
            let result = gpui_tokio::Tokio::spawn(cx, async move { core.shutdown_with_budget(quit_budget::LIMIT.saturating_sub(quit_started.elapsed())).await }).await;
            let _ = this.update(cx, |this, cx| {
                if !matches!(result, Ok(Ok(()))) {
                    let _ = this.log.write(
                        format!("Shutdown bound/failure; proceeding to exit: {result:?}\n")
                            .as_bytes(),
                    );
                }
                // Core has stopped. Release the retiring lease before any update relaunch.
                this.instance.take();
                this.finish_update();
                if let Some(client) = &this.updater { let _ = client.send(update_integration::Command::Shutdown); }
                this.relay.close();
                #[cfg(feature = "bench-control")]
                this._control.take(); // AppKit may exit without dropping GPUI entities.
                this.instance.take();
                if !windows::termination::complete(true) {
                    cx.quit();
                }
            });
        }));
    }
    fn surface(&mut self, op: SurfaceOp, cx: &mut Context<Self>) -> Result<(), String> {
        match op {
            SurfaceOp::ShowLauncher | SurfaceOp::ShowAfterAuth => self.show(cx),
            SurfaceOp::HideLauncher => self.hide(cx),
            SurfaceOp::OpenSettings => self.settings(cx),
            SurfaceOp::CloseCurrent => {
                if self.lifecycle.settings {
                    self.request_settings_close(cx);
                } else {
                    self.hide(cx);
                }
            }
            SurfaceOp::SetAppVisible(visible) => {
                if visible {
                    self.restore(cx)
                } else {
                    self.hide(cx)
                }
            }
            SurfaceOp::SuppressBlurHide(suppress) => self.lifecycle.suppress(suppress),
            SurfaceOp::HideForPaste => {
                self.hide(cx);
                if let Some(settings) = self.settings {
                    let _ = settings.update(cx, |_, w, _| native::visible(w, false));
                }
                cx.hide();
            }
            SurfaceOp::Quit => self.quit(cx),
            SurfaceOp::SnapDragStart => self.begin_snap(None, cx),
            SurfaceOp::SnapDragEnd => self.end_snap(cx),
            SurfaceOp::PressKey { key, modifiers } => {
                // This is the intentional native input effect, not a core request. Return its
                // actual outcome instead of acknowledging an unowned detached task.
                tezbar_host::input::press_key(key, modifiers)?;
            }
            SurfaceOp::ClearLegacyWebviewCache => {
                return Err(
                    "Import legacy data before clearing its cache. No files were removed.".into(),
                );
            }
        }
        Ok(())
    }
    #[cfg(feature = "bench-control")]
    fn control(
        &mut self,
        request: serde_json::Value,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        match request["op"].as_str().unwrap_or("") {
            "status" => Ok(
                serde_json::json!({"visible":self.lifecycle.visible,"settings":self.lifecycle.settings,"quitting":self.lifecycle.quitting,"pid":std::process::id(),"suppression":self.lifecycle.suppress,"tray":self.tray.is_some(),"error":self.launcher.update(cx,|view,_,_| view.error.clone()).ok().flatten()}),
            ),
            "show" => {
                self.show(cx);
                Ok(serde_json::Value::Null)
            }
            "hide" => {
                self.hide(cx);
                Ok(serde_json::Value::Null)
            }
            "settings" => {
                self.settings(cx);
                Ok(serde_json::Value::Null)
            }
            "theme" => {
                tezbar_ui::tokens::set(request["value"].as_str().unwrap_or("system"))?;
                cx.refresh_windows();
                Ok(serde_json::Value::Null)
            }
            "panel" => {
                let rows = windows::terminal_sessions::fixture_rows(request["sessions"].clone())?;
                self.panel(rows, cx);
                Ok(serde_json::Value::Null)
            }
            "native-quit" => {
                windows::termination::request_native()?;
                Ok(serde_json::Value::Null)
            }
            "quit" => {
                self.quit(cx);
                Ok(serde_json::Value::Null)
            }
            "hold-core-stop" => {
                self.core.hold_shutdown_for_fixture();
                Ok(serde_json::Value::Null)
            }
            "hold-shutdown" => {
                self.shutdown_gate.hold();
                Ok(serde_json::Value::Null)
            }
            "release-shutdown" => {
                self.shutdown_gate.release();
                Ok(serde_json::Value::Null)
            }
            _ => Err("Unknown control operation".into()),
        }
    }
    fn request_settings_close(&mut self, cx: &mut Context<Self>) {
        if self.settings_close.is_pending() {
            return;
        }
        if let Some(settings) = self.settings {
            let g = self.settings_close.request(Instant::now());
            self.host.emit(
                "settings-close-requested",
                serde_json::json!({"generation":g}),
            );
            let _ = settings.update(cx, |view, _, cx| view.begin_close(g, cx));
            let relay = self.relay.clone();
            self.settings_close_timer = Some(cx.spawn(async move |_, cx| {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                let _ = relay.send(Event::SettingsClose(g));
            }));
        }
    }
    fn finish_settings_close(&mut self, cx: &mut Context<Self>) {
        self.settings_close_timer = None;
        let settings = self.settings.take();
        self.lifecycle.settings = false;
        self.m7.subscription = None;
        self.show(cx);
        if let Some(settings) = settings {
            let _ = settings.update(cx, |_, w, _| w.remove_window());
        }
        if let Some((surface, payload)) = self.m7.after_close.take() {
            let _ = self.launcher.update(cx, |view, _, cx| {
                view.route_from_settings(surface, payload, cx)
            });
        }
    }
    fn panel(&mut self, rows: Vec<windows::terminal_sessions::SessionRow>, cx: &mut Context<Self>) {
        if !self.lifecycle.visible {
            return;
        }
        self.panel_visible = true;
        let Ok((bounds, parent)) = self.launcher.update(cx, |_, w, _| {
            (
                windows::terminal_sessions::bounds(w.bounds()),
                native::token(w),
            )
        }) else {
            return;
        };
        if let Some(panel) = self.panel {
            let updated = panel.update(cx, |view, w, cx| {
                view.sessions = rows.clone();
                let _ = native::place(w, bounds, cx);
                let _ = native::visible(w, true);
                cx.notify();
            });
            if updated.is_ok() {
                return;
            }
            self.panel = None;
        }
        let relay = self.relay.clone();
        self.panel = cx
            .open_window(
                WindowOptions {
                    window_background: WindowBackgroundAppearance::Transparent,
                    kind: WindowKind::Floating,
                    titlebar: None,
                    focus: false,
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                move |w, cx| {
                    w.set_window_title("Terminal Sessions");
                    let _ = native::no_shadow(w);
                    let _ = native::frameless(w);
                    let close = relay.clone();
                    w.on_window_should_close(cx, move |_, _| {
                        let _ = close.send(Event::PanelHide);
                        false
                    });
                    if let Ok(parent) = parent {
                        let _ = native::attach(w, parent);
                    }
                    cx.new(|cx| windows::terminal_sessions::SessionsPanel::new(rows, relay, w, cx))
                },
            )
            .ok();
    }
    fn viewport_event(&self, cx: &mut Context<Self>) {
        if let Ok(viewport)=self.launcher.update(cx,|view,w,_|{let size=w.bounds().size;serde_json::json!({"width":f32::from(size.width),"height":f32::from(size.height),"scaleFactor":w.scale_factor(),"zoom":view.zoom})}){self.host.emit("launcher-viewport-changed",viewport);}
    }
    fn follow_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(panel) = self.panel {
            if let Ok(bounds) = self
                .launcher
                .update(cx, |_, w, _| windows::terminal_sessions::bounds(w.bounds()))
            {
                let _ = panel.update(cx, |_, w, cx| native::place(w, bounds, cx));
            }
        }
    }
    fn snap_update(&mut self, timer: Option<u64>, cx: &mut Context<Self>) {
        if !self.snap.active {
            return;
        }
        let Ok((bounds, monitor)) = self
            .launcher
            .update(cx, |_, w, cx| (w.bounds(), w.display(cx)))
        else {
            return;
        };
        let Some(display) = monitor else {
            return;
        };
        let monitor = display.bounds();
        let scale = native::display_scale(display.as_ref());
        let geometry = MonitorGeometry {
            bounds: native::physical_rect(monitor, scale),
            work_area: native::physical_rect(display.visible_bounds(), scale),
        };
        let position = PersistedWindowPosition {
            x: f64::from(bounds.left()) * scale,
            y: f64::from(bounds.top()) * scale,
        };
        if let Some((position, locked, schedule)) = self.snap.update(
            position,
            f64::from(bounds.size.width) * scale,
            f64::from(bounds.size.height) * scale,
            geometry,
            timer,
            Instant::now(),
        ) {
            if (locked.x || locked.y) && (cfg!(windows) || !native::left_button_down()) {
                let _ = self.launcher.update(cx, |_, w, cx| {
                    native::place(
                        w,
                        Bounds::new(
                            point(
                                px((position.x / scale) as f32),
                                px((position.y / scale) as f32),
                            ),
                            bounds.size,
                        ),
                        cx,
                    )
                });
            }
            if self.overlay.is_none() {
                self.overlay = cx
                    .open_window(
                        WindowOptions {
                            kind: WindowKind::PopUp,
                            titlebar: None,
                            focus: false,
                            show: false,
                            is_resizable: false,
                            window_background: WindowBackgroundAppearance::Transparent,
                            window_bounds: Some(WindowBounds::Windowed(display.visible_bounds())),
                            ..Default::default()
                        },
                        |w, cx| {
                            let _ = native::passthrough(w);
                            cx.new(|_| windows::snap_overlay::SnapOverlay::default())
                        },
                    )
                    .ok();
            }
            if let Some(overlay) = self.overlay {
                let _ = overlay.update(cx, |view, w, cx| {
                    view.x = locked.x;
                    view.y = locked.y;
                    let _ = native::place_scaled(w, display.visible_bounds(), scale, cx);
                    let _ = native::visible(w, locked.x || locked.y);
                    cx.notify();
                });
            }
            if schedule {
                let g = self.snap.generation;
                let relay = self.relay.clone();
                self.snap_timer = Some(cx.spawn(async move |_, cx| {
                    cx.background_executor()
                        .timer(placement::SNAP_DWELL_DURATION)
                        .await;
                    let _ = relay.send(Event::SnapDwell(g));
                }));
            }
        }
    }
    fn begin_snap(&mut self, grab: Option<(f32, f32)>, cx: &mut Context<Self>) {
        self.end_snap(cx);
        if self.lifecycle.quitting {
            return;
        }
        let generation = self.snap.begin();
        self.lifecycle.suppress(true);
        let initial = self
            .launcher
            .update(cx, |_, w, _| (w.bounds(), w.scale_factor() as f64))
            .ok();
        // Anchor to the press point (screen pixels) rather than wherever the cursor
        // has travelled by the time this event is handled.
        let origin = match (grab, initial) {
            (Some((gx, gy)), Some((bounds, scale))) => Some((
                (f64::from(bounds.left()) + f64::from(gx)) * scale,
                (f64::from(bounds.top()) + f64::from(gy)) * scale,
            )),
            _ => tezbar_host::monitors::cursor_point(),
        };
        self.drag_task = Some(cx.spawn(async move |owner, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let active = owner
                    .update(cx, |owner, cx| {
                        if !owner.snap.active || owner.snap.generation != generation {
                            return false;
                        }
                        if !native::left_button_down() {
                            owner.end_snap(cx);
                            return false;
                        }
                        #[cfg(windows)]
                        if let (Some((initial, initial_scale)), Some((ox, oy)), Some((x, y))) =
                            (initial, origin, tezbar_host::monitors::cursor_point())
                        {
                            let _ =
                                owner.launcher.update(cx, |_, w, cx| {
                                    let scale = w.scale_factor() as f64;
                                    native::place(
                                        w,
                                        Bounds::new(
                                            point(
                                                px(((f64::from(initial.left()) * initial_scale + x
                                                    - ox)
                                                    / scale)
                                                    as f32),
                                                px(((f64::from(initial.top()) * initial_scale + y
                                                    - oy)
                                                    / scale)
                                                    as f32),
                                            ),
                                            initial.size,
                                        ),
                                        cx,
                                    )
                                });
                        }
                        #[cfg(not(windows))]
                        let _ = (initial, origin);
                        owner.snap_update(None, cx);
                        true
                    })
                    .unwrap_or(false);
                if !active {
                    break;
                }
            }
        }));
        #[cfg(target_os = "macos")]
        let _ = self.launcher.update(cx, |_, w, _| w.start_window_move());
    }
    fn end_snap(&mut self, cx: &mut Context<Self>) {
        if self.snap.active {
            if !native::left_button_down() {
                self.snap_update(None, cx);
            }
            self.lifecycle.suppress(false);
        }
        self.drag_task = None;
        self.snap_timer = None;
        self.snap.end();
        if let Some(overlay) = self.overlay.take() {
            let _ = overlay.update(cx, |_, w, _| w.remove_window());
        }
        self.snapshot(cx);
    }
    fn event(&mut self, event: Event, cx: &mut Context<Self>) {
        self.tasks.retain(|task| !task.is_ready());
        match event {
            Event::ExtensionEffect(id, mount, e) => self.m9_effect(id, mount, e, cx),
            Event::Navigate(intent) => {
                use crate::surfaces::command_bar::navigation::NavigationInbox;
                if !cx.has_global::<NavigationInbox>() {
                    cx.set_global(NavigationInbox::default());
                }
                if let Err(error) = cx.global_mut::<NavigationInbox>().put(intent.clone()) {
                    let _ = self.launcher.update(cx, |view, _, cx| {
                        view.error = Some(error);
                        cx.notify();
                    });
                    return;
                }
                if matches!(
                    intent.surface,
                    tezbar_ui_model::router::Surface::Settings(_)
                ) {
                    cx.global_mut::<NavigationInbox>().acknowledge(&intent);
                    self.settings_intent(Some(intent), cx);
                } else if intent.surface == tezbar_ui_model::router::Surface::Onboarding {
                    cx.global_mut::<NavigationInbox>().acknowledge(&intent);
                    self.m7_open_onboarding(cx);
                } else {
                    let _ = self
                        .launcher
                        .update(cx, |view, _, cx| view.receive_navigation(intent, cx));
                    self.show(cx);
                }
            }
            Event::SettingsEffect(id, effect) => self.m7_effect(id, effect, cx),
            Event::OnboardingFinished(id) => self.m7_finished(id, cx),
            Event::Reveal(path) => {
                cx.reveal_path(std::path::Path::new(&path));
            }
            Event::Update(action) => {
                use crate::surfaces::command_bar::navigation::UpdateInbox;
                if !cx.has_global::<UpdateInbox>() {
                    cx.set_global(UpdateInbox::default());
                }
                if let Err(error) = cx.global_mut::<UpdateInbox>().put(action) {
                    let _ = self.launcher.update(cx, |view, _, cx| {
                        view.error = Some(error);
                        cx.notify();
                    });
                }
                self.drain_updates(cx);
            }
            #[cfg(feature = "bench-control")]
            Event::Control(request, reply) => {
                let result = self.control(request, cx);
                let _ = reply.send(result);
            }
            Event::Zoom(delta) => {
                let _ = self.launcher.update(cx, |view, _, cx| {
                    view.zoom = if delta == 0. {
                        1.
                    } else {
                        (view.zoom + delta).clamp(0.75, 2.)
                    };
                    cx.notify();
                });
                self.snapshot(cx);
                self.flush();
                self.viewport_event(cx);
                self.host
                    .emit("launcher-layout-changed", serde_json::json!(self.layouts));
            }
            Event::PanelShow(rows) => self.panel(rows, cx),
            Event::PanelHide => {
                self.panel_visible = false;
                if let Some(panel) = self.panel {
                    let _ = panel.update(cx, |_, w, _| native::visible(w, false));
                }
            }
            Event::PanelFocused(focused) => {
                self.lifecycle.panel_focused = focused;
                let g = self.lifecycle.blur_ticket();
                if !focused {
                    let relay = self.relay.clone();
                    self.tasks.push(cx.spawn(async move |_, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(80))
                            .await;
                        let _ = relay.send(Event::Blur(g));
                    }));
                }
            }
            Event::Main(op) => op(),
            Event::Activate => self.restore(cx),
            Event::ShowLauncher => self.show(cx),
            Event::Hide => self.hide(cx),
            Event::Toggle => {
                if self.lifecycle.visible {
                    self.hide(cx)
                } else {
                    self.restore(cx)
                }
            }
            Event::Settings => self.settings(cx),
            Event::SettingsFocused => self.hide(cx),
            Event::ThemeChanged(theme) => {
                let _ = self.launcher.update(cx, |v, w, cx| {
                    v.terminal_theme(tezbar_ui::tokens::tokens(w).light, cx)
                });
                if let Some(settings) = self.settings {
                    let _ = settings.update(cx, |view, _, cx| {
                        view.theme = theme;
                        cx.notify();
                    });
                }
            }
            Event::Quit => self.quit(cx),
            Event::TerminalEffect(source, effect) => self.m10_effect(source, effect, cx),
            Event::TerminalRows(rows) => {
                if self.panel_visible {
                    self.panel(rows, cx);
                }
            }
            Event::TerminalPanelAction(id) => {
                let _ = self
                    .launcher
                    .update(cx, |v, w, cx| v.terminal_panel_action(id, w, cx));
            }
            Event::ChatEffect(source, effect) => self.m8_effect(source, effect, cx),
            Event::RetryCore => self.start_core(cx),
            Event::ConfirmFreshState => {
                if self
                    .launcher
                    .update(cx, |view, _, cx| view.confirm_fresh_state(cx))
                    .unwrap_or(false)
                {
                    if let Ok(theme) = self
                        .launcher
                        .update(cx, |view, _, _| view.state.theme.clone())
                    {
                        apply_theme(theme, cx);
                    }
                    self.m7 = m7::State::new(self.m7.store.clone());
                    self.start_core(cx);
                    self.mount_updater_after_recovery(cx);
                }
            }
            Event::RetryLegacy => {
                if self
                    .launcher
                    .update(cx, |view, _, cx| view.retry_migration(cx))
                    .unwrap_or(false)
                {
                    if let Ok(theme) = self
                        .launcher
                        .update(cx, |view, _, _| view.state.theme.clone())
                    {
                        apply_theme(theme, cx);
                    }
                    self.m7 = m7::State::new(self.m7.store.clone());
                    self.start_core(cx);
                    self.mount_updater_after_recovery(cx);
                }
            }
            Event::CloseSettings => {
                let _ = self.surface(SurfaceOp::CloseCurrent, cx);
            }
            Event::Bounds => {
                let _ = self.launcher.update(cx, |v, _, cx| v.terminal_layout(cx));
                self.snapshot(cx);
                self.viewport_event(cx);
                self.follow_panel(cx);
                self.snap_update(None, cx);
            }
            Event::SnapBegin(grab) => self.begin_snap(grab, cx),
            Event::SnapEnd => self.end_snap(cx),
            Event::SnapDwell(g) => self.snap_update(Some(g), cx),
            Event::Surface(op, reply) => {
                if !reply.is_closed() {
                    let result = self.surface(op, cx);
                    let _ = reply.send(result);
                }
            }
            Event::Menu(id) => match id.as_str() {
                "preview-show" => self.restore(cx),
                "preview-settings" => self.settings(cx),
                "preview-quit" => self.quit(cx),
                _ => {
                    tezbar_host::desktop::NativeDesktop::dispatch_menu_event(&id);
                }
            },
            Event::Hotkey(id) => {
                if self.tray.as_ref().is_some_and(|t| {
                    t.shortcut
                        .as_ref()
                        .is_some_and(|shortcut| shortcut.id() == id)
                }) {
                    if self.lifecycle.visible {
                        self.hide(cx)
                    } else {
                        self.restore(cx)
                    }
                } else if self.core.0.platform.is_launcher_shortcut(id) {
                    if self.lifecycle.visible {
                        self.hide(cx)
                    } else {
                        self.restore(cx)
                    }
                } else if let Some(command) = self.core.0.platform.command_for_shortcut(id) {
                    let core = self.core.clone();
                    self.tasks.push(cx.spawn(async move |_, cx| {
                        let _ = gpui_tokio::Tokio::spawn(cx, async move {
                            tokio::spawn(
                                async move { core.0.execute_command_shortcut(&command).await },
                            )
                            .await
                        })
                        .await;
                    }));
                }
            }
            Event::Core(CoreEvent::OpenSurface(payload)) => {
                self.events
                    .publish(CoreEvent::OpenSurface(payload.clone()), cx);
            }
            Event::Core(event) => {
                let update =
                    matches!(&event,CoreEvent::Other{channel,..}if channel=="update-status");
                self.events.publish(event, cx);
                if update {
                    self.m7_refresh_content(cx);
                }
            }
            Event::Blur(0) => {
                let g = self.lifecycle.blur_ticket();
                let relay = self.relay.clone();
                self.blur = Some(cx.spawn(async move |_, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(120))
                        .await;
                    let _ = relay.send(Event::Blur(g));
                }));
            }
            Event::Blur(g) => {
                if self.lifecycle.should_blur_hide(g)
                    && self
                        .launcher
                        .update(cx, |_, w, _| !w.is_window_active())
                        .unwrap_or(false)
                {
                    self.hide(cx);
                }
            }
            Event::Idle(g) => {
                if self.lifecycle.should_trim(g, Instant::now()) {
                    let core = self.core.clone();
                    self.tasks.push(cx.spawn(async move |_, cx| {
                        let _ =
                            gpui_tokio::Tokio::spawn(
                                cx,
                                async move { core.0.trim_idle_memory().await },
                            )
                            .await;
                    }));
                }
            }
            Event::Capture(reply) => {
                let mut visible = vec![];
                if self.lifecycle.visible {
                    let _ = self.launcher.update(cx, |_, w, _| {
                        visible.push(w.window_handle().window_id());
                        let _ = native::visible(w, false);
                    });
                }
                if let Some(settings) = self.settings {
                    let _ = settings.update(cx, |_, w, _| {
                        visible.push(w.window_handle().window_id());
                        let _ = native::visible(w, false);
                    });
                }
                if let Some(panel) = self.panel {
                    if self.lifecycle.visible && self.panel_visible {
                        let _ = panel.update(cx, |_, w, _| {
                            visible.push(w.window_handle().window_id());
                            let _ = native::visible(w, false);
                        });
                    }
                }
                if let Some(overlay) = self.overlay {
                    let _ = overlay.update(cx, |_, w, _| {
                        visible.push(w.window_handle().window_id());
                        let _ = native::visible(w, false);
                    });
                }
                self.lifecycle.suppress(true);
                let _ = reply.send(events::CaptureLease::new(
                    self.relay.clone(),
                    events::CaptureSnapshot {
                        windows: visible,
                        generation: self.visibility_generation,
                    },
                ));
            }
            Event::RestoreCapture(snapshot) => {
                self.lifecycle.suppress(false);
                if snapshot.generation != self.visibility_generation || self.lifecycle.quitting {
                    return;
                }
                let windows = snapshot.windows;
                if self.lifecycle.visible {
                    let _ = self.launcher.update(cx, |_, w, _| {
                        if windows.contains(&w.window_handle().window_id()) {
                            let _ = native::visible(w, true);
                        }
                    });
                }
                if let Some(settings) = self.settings {
                    let _ = settings.update(cx, |_, w, _| {
                        if windows.contains(&w.window_handle().window_id()) {
                            let _ = native::visible(w, true);
                        }
                    });
                }
                if self.lifecycle.visible && self.panel_visible {
                    if let Some(panel) = self.panel {
                        let _ = panel.update(cx, |_, w, _| {
                            if windows.contains(&w.window_handle().window_id()) {
                                let _ = native::visible(w, true);
                            }
                        });
                    }
                }
                if self.snap.active {
                    if let Some(overlay) = self.overlay {
                        let _ = overlay.update(cx, |_, w, _| {
                            if windows.contains(&w.window_handle().window_id()) {
                                let _ = native::visible(w, true);
                            }
                        });
                    }
                }
            }
            Event::SavePng {
                directory,
                name,
                png,
                reply,
            } => {
                if reply.is_closed() {
                    self.lifecycle.suppress(true);
                    let _ = self.relay.send(Event::DialogClosed);
                    return;
                }
                self.lifecycle.suppress(true);
                let lease = events::DialogLease(self.relay.clone());
                let prompt = cx.prompt_for_new_path(&directory, Some(&name));
                self.tasks.push(cx.spawn(async move |_, cx| {
                    let _lease = lease;
                    let selected = prompt
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|v| v.map_err(|e| e.to_string()));
                    if reply.is_closed() {
                        return;
                    }
                    let result = match selected {
                        Ok(Some(path)) => {
                            let destination = path.clone();
                            gpui_tokio::Tokio::spawn(cx, async move {
                                tokio::task::spawn_blocking(move || {
                                    std::fs::write(&destination, png)
                                        .map(|_| Some(destination))
                                        .map_err(|e| e.to_string())
                                })
                                .await
                                .map_err(|e| e.to_string())
                                .and_then(|v| v)
                            })
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|v| v)
                        }
                        Ok(None) => Ok(None),
                        Err(e) => Err(e),
                    };
                    let _ = reply.send(result);
                }));
            }
            Event::Dialog(req, reply) => {
                if reply.is_closed() {
                    self.lifecycle.suppress(true);
                    let _ = self.relay.send(Event::DialogClosed);
                    return;
                }
                self.lifecycle.suppress(true);
                let lease = events::DialogLease(self.relay.clone());
                match req {
                    DialogRequest::Confirm(value) => {
                        let title = value["title"].as_str().unwrap_or("Confirm");
                        let message = value["message"].as_str().unwrap_or(title);
                        let body = value["detail"].as_str().unwrap_or("");
                        let detail = if message != title {
                            Some(format!(
                                "{message}{}",
                                if body.is_empty() {
                                    String::new()
                                } else {
                                    format!("\n\n{body}")
                                }
                            ))
                        } else {
                            (!body.is_empty()).then(|| body.to_owned())
                        };
                        let message = title.to_owned();
                        let level = match value["type"].as_str() {
                            Some("warning") => PromptLevel::Warning,
                            Some("error") => PromptLevel::Critical,
                            _ => PromptLevel::Info,
                        };
                        if self.settings.is_some() {
                            self.restore(cx);
                        } else {
                            self.show(cx);
                        }
                        let buttons = value["buttons"]
                            .as_array()
                            .map(|v| {
                                v.iter()
                                    .filter_map(|v| v.as_str().map(str::to_owned))
                                    .collect::<Vec<_>>()
                            })
                            .filter(|v| !v.is_empty())
                            .unwrap_or_else(|| vec!["Cancel".into(), "Confirm".into()]);
                        let affirmative = buttons.len() - 1;
                        let answers = buttons.iter().map(String::as_str).collect::<Vec<_>>();
                        let prompt = if let Some(settings) = self.settings {
                            settings.update(cx, |_, w, cx| {
                                w.prompt(level, &message, detail.as_deref(), &answers, cx)
                            })
                        } else {
                            self.launcher.update(cx, |_, w, cx| {
                                w.prompt(level, &message, detail.as_deref(), &answers, cx)
                            })
                        };
                        self.tasks.push(cx.spawn(async move |_, _| {
                            let _lease = lease;
                            let result = match prompt {
                                Ok(prompt) => prompt
                                    .await
                                    .map(|i| DialogReply::Confirm(i == affirmative))
                                    .map_err(|e| e.to_string()),
                                Err(e) => Err(e.to_string()),
                            };
                            let _ = reply.send(result);
                        }));
                    }
                    DialogRequest::ChooseFolder => {
                        let prompt = cx.prompt_for_paths(PathPromptOptions {
                            files: false,
                            directories: true,
                            multiple: false,
                            prompt: None,
                        });
                        self.tasks.push(cx.spawn(async move |_, _| {
                            let _lease = lease;
                            let result = prompt.await.map_err(|e| e.to_string()).and_then(|r| {
                                r.map(|p| DialogReply::Path(p.and_then(|mut p| p.pop())))
                                    .map_err(|e| e.to_string())
                            });
                            let _ = reply.send(result);
                        }));
                    }
                }
            }
            Event::DialogClosed => {
                self.lifecycle.suppress(false);
                if !self.lifecycle.quitting {
                    self.restore(cx);
                }
            }
            Event::SettingsAck(g) => {
                self.settings_close.acknowledge(g);
            }
            Event::SettingsFinish(g, result) => {
                // Check at the actual destruction boundary, after any input queued during drain.
                if result.is_ok()
                    && self.settings_close.is_pending()
                    && (self.m7.store.pending()
                        || self.m7.store.failures() > 0
                        || !self.m7.barrier.is_clean())
                {
                    if let Some(settings) = self.settings {
                        let _ = settings.update(cx, |view, _, cx| {
                            if view.close_generation() == Some(g) {
                                view.begin_close(g, cx);
                            }
                        });
                    }
                    return;
                }
                let success = result.is_ok();
                if self.settings_close.finish(g, success) {
                    self.finish_settings_close(cx);
                } else if let Err(error) = result {
                    if let Some(settings) = self.settings {
                        let _ = settings.update(cx, |view, _, cx| {
                            view.close_failed(g, error, cx);
                        });
                    }
                }
            }
            Event::SettingsClose(g) => {
                if self.settings_close.timeout(g, Instant::now()) {
                    self.finish_settings_close(cx);
                }
            }
        }
    }
}
use tezbar_core::host::Host;
/// GUI builds have no console, and a panic inside a platform callback (window
/// procedure, AppKit, accessibility) aborts the process. Record every panic in the
/// app log first so the crash leaves its message, location and backtrace behind.
fn install_panic_log() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        log::error!(
            "panic on thread '{}': {info}\n{}",
            thread.name().unwrap_or("unnamed"),
            std::backtrace::Backtrace::force_capture()
        );
        log::logger().flush();
        previous(info);
    }));
}
pub fn run(preview: Preview) -> anyhow::Result<()> {
    run_app(preview, None)
}
/// Admission is opaque and can only be obtained by verifying the installed binary.
pub fn run_production(admission: production_launch::Admission) -> anyhow::Result<()> {
    let plan = admission.plan()?;
    let preview = Preview {
        root: plan.core.config_dir.clone(),
        resources: plan.core.resource_dir.clone(),
        show: false,
        allow_desktop_integration: true,
        allow_desktop_services: true,
        launcher_shortcut: None,
        theme: "system".into(),
        home: Some(plan.core.home_dir.clone()),
        #[cfg(feature = "bench-control")]
        control_token: None,
    };
    run_app(preview, Some(plan))
}
fn run_app(
    preview: Preview,
    production: Option<production_launch::LaunchPlan>,
) -> anyhow::Result<()> {
    let identity = production
        .as_ref()
        .map_or("com.tezbar.preview.native", |p| p.instance_identity);
    let core_paths = production
        .as_ref()
        .map_or_else(|| preview.core_paths(), |p| p.core.clone());
    let layouts_path = production.as_ref().map_or_else(
        || preview.root.join("config/launcher-layouts-v1.json"),
        |p| p.layouts.clone(),
    );
    let log_dir = production
        .as_ref()
        .map_or_else(|| preview.root.join("logs"), |p| p.profile.logs.clone());
    let instance_dir = production
        .as_ref()
        .map_or_else(|| preview.root.clone(), |p| p.profile.data.clone());
    let migration = production.as_ref().and_then(|p| {
        p.legacy_webkit_root
            .clone()
            .map(crate::legacy_state::Source::WebKit)
            .or_else(|| {
                p.legacy_webview2_root
                    .clone()
                    .map(crate::legacy_state::Source::WebView2)
            })
            .map(|source| source.for_install(&p.profile.config, &p.profile.data))
    });
    let installed_executable = production
        .as_ref()
        .map(|_| std::env::current_exe())
        .transpose()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(16)
        .enable_all()
        .build()?;
    let handle = runtime.handle().clone();
    let _entered = handle.enter();
    let (relay, mut rx) = Relay::new();
    let activate = relay.clone();
    // Snapshot first-install evidence above before creating our own data folder.
    if production.is_some() {
        std::fs::create_dir_all(&instance_dir)?;
    }
    let instance = match runtime
        .block_on(Instance::claim(
            &instance_dir,
            identity,
            Arc::new(move || {
                let _ = activate.send(Event::Activate);
            }),
        ))
        .map_err(anyhow::Error::msg)?
    {
        Claim::Activated => return Ok(()),
        Claim::Owner(instance) => instance,
    };
    let log: &'static tezbar_host::logging::FileLog = Box::leak(Box::new(
        tezbar_host::logging::FileLog::open(&log_dir, "tezbar").map_err(anyhow::Error::msg)?,
    ));
    log::set_logger(log).map_err(|e| anyhow::anyhow!("Native logger: {e}"))?;
    log::set_max_level(log::LevelFilter::Info);
    install_panic_log();
    let application = gpui_platform::application();
    let reopen = relay.clone();
    application.on_reopen(move |_| {
        let _ = reopen.send(Event::Activate);
    });
    application.run(move |cx| {
        gpui_tokio::init_from_handle(cx, handle);
        cx.set_reduce_motion(native::reduce_motion());
        cx.set_global(RuntimeOwner(Some(runtime)));
        apply_theme(match preview.theme.as_str() {
            "light" | "white" => tezbar_ui_model::ui_state::Theme::White,
            "dark" => tezbar_ui_model::ui_state::Theme::Dark,
            _ => tezbar_ui_model::ui_state::Theme::System,
        }, cx);
        cx.set_activation_policy(ActivationPolicy::Regular);
        cx.set_quit_mode(QuitMode::Explicit);
        tezbar_ui::text_input::bind_keys(cx);
        tezbar_ui::markdown::bind_keys(cx);
        events::install_native_handlers(&relay);
        windows::termination::install(&relay).expect("native termination adapter");
        #[cfg(target_os = "macos")]
        cx.bind_keys([KeyBinding::new("cmd-q", RequestQuit, None)]);
        cx.bind_keys([KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-,"
            } else {
                "ctrl-,"
            },
            OpenSettings,
            None,
        )]);
        let quit_relay = relay.clone();
        cx.on_action(move |_: &RequestQuit, _| {
            let _ = quit_relay.send(Event::Quit);
        });
        let settings_relay = relay.clone();
        cx.on_action(move |_: &OpenSettings, _| {
            let _ = settings_relay.send(Event::Settings);
        });
        let zoom = relay.clone();
        cx.on_action(move |_: &ZoomIn, _| {
            let _ = zoom.send(Event::Zoom(0.1));
        });
        let zoom = relay.clone();
        cx.on_action(move |_: &ZoomOut, _| {
            let _ = zoom.send(Event::Zoom(-0.1));
        });
        let zoom = relay.clone();
        cx.on_action(move |_: &ZoomReset, _| {
            let _ = zoom.send(Event::Zoom(0.));
        });
        cx.bind_keys([
            KeyBinding::new(
                if cfg!(target_os = "macos") {
                    "cmd-="
                } else {
                    "ctrl-="
                },
                ZoomIn,
                None,
            ),
            KeyBinding::new(
                if cfg!(target_os = "macos") {
                    "cmd--"
                } else {
                    "ctrl--"
                },
                ZoomOut,
                None,
            ),
            KeyBinding::new(
                if cfg!(target_os = "macos") {
                    "cmd-0"
                } else {
                    "ctrl-0"
                },
                ZoomReset,
                None,
            ),
        ]);
        cx.set_menus(vec![Menu {
            name: "Tezbar".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Settings", OpenSettings),
                MenuItem::separator(),
                MenuItem::action("Quit Tezbar", RequestQuit),
            ],
        }]);
        let events = events::CoreEvents::new(relay.clone());
        cx.set_global(events.clone());
        let host = Arc::new(if production.is_some() {
            GpuiHost::production(relay.clone())
        } else {
            GpuiHost::with_desktop_integration(relay.clone(), preview.allow_desktop_integration)
                .with_desktop_services(preview.allow_desktop_services)
        });
        let context = CoreContext::new(core_paths, Some(host.clone()))
            .with_dev_mode(production.is_none())
            .with_legacy_data_dir(None);
        let core = CoreClient::for_launch(
            tezbar_core::NativeCore::new(context).expect("native core"),
            production.is_some(),
        );
        host.bind_status_assets(&core.0);
        let settings_store = crate::surfaces::settings::SettingsStore::new(Arc::new(core.clone()));
        let launcher_settings_store = settings_store.clone();
        let view_core = core.clone();
        let view_relay = relay.clone();
        let launcher = cx
            .open_window(
                WindowOptions {
                    kind: WindowKind::PopUp,
                    show: false,
                    focus: false,
                    titlebar: None,
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(760.), px(640.)),
                        cx,
                    ))),
                    window_min_size: Some(size(px(480.), px(360.))),
                    window_background: if cfg!(target_os = "macos") {
                        WindowBackgroundAppearance::Blurred
                    } else {
                        WindowBackgroundAppearance::Transparent
                    },
                    app_id: Some(identity.into()),
                    ..Default::default()
                },
                move |w, cx| {
                    let _ = native::frameless(w);
                    let close = view_relay.clone();
                    w.on_window_should_close(cx, move |_, _| {
                        let _ = close.send(Event::Hide);
                        false
                    });
                    cx.new(|cx| {
                        Launcher::new_with_migration(
                            view_core,
                            view_relay,
                            launcher_settings_store,
                            migration,
                            w,
                            cx,
                        )
                    })
                },
            )
            .expect("launcher window");
        if preview.theme == "system" {
            if let Ok(theme) = launcher.update(cx, |view, _, _| view.state.theme) {
                apply_theme(theme, cx);
            }
        }
        let tray = (preview.allow_desktop_integration || preview.launcher_shortcut.is_some())
            .then(|| {
                if production.is_some() {
                    PrimaryTray::production(&host.desktop, &preview.resources)
                } else if let Some(shortcut) = &preview.launcher_shortcut {
                    PrimaryTray::with_shortcut(&host.desktop, &preview.resources, shortcut)
                } else {
                    PrimaryTray::new(&host.desktop, &preview.resources)
                }
            })
            .transpose()
            .map_err(|e| {
                let _ = log.write(e.as_bytes());
            })
            .ok()
            .flatten();
        let config = core.0.ctx.config.read().unwrap_or_default();
        let layouts = std::fs::read_to_string(&layouts_path)
            .ok()
            .map(|s| tezbar_host::layout::profiles_from_json(&s))
            .unwrap_or_default();
        let writer =
            windows::persistence::Writer::new(core.0.ctx.config.clone(), layouts_path.clone());
        #[cfg(feature = "bench-control")]
        let control = preview.control_token.as_deref().map(|token| {
            crate::bench_control::Server::start(&preview.root, token, relay.clone())
                .expect("isolated control")
        });
        let show = preview.show;
        let owner = cx.new(|_| Owner {
            m7: m7::State::new(settings_store),
            updater: None,
            updater_quiescing: false,
            updater_paused: false,
            production_plan: production.clone(),
            pending_update: None,
            #[cfg(target_os = "macos")]
            staged_update: None,
            installed_executable,
            layout_path: layouts_path,
            settings_close: Default::default(),
            visibility_generation: 0,
            previous_foreground: None,
            panel: None,
            panel_visible: false,
            #[cfg(feature = "bench-control")]
            _control: control,
            #[cfg(feature = "bench-control")]
            shutdown_gate: Arc::default(),
            snap: Default::default(),
            overlay: None,
            writer,
            core: core.clone(),
            host,
            instance: Some(instance),
            tray,
            launcher,
            settings: None,
            lifecycle: windows::Lifecycle::default(),
            relay: relay.clone(),
            tasks: vec![],
            startup: None,
            show_on_ready: show,
            save_error: None,
            idle: None,
            blur: None,
            snap_timer: None,
            drag_task: None,
            settings_close_timer: None,
            positions: Default::default(),
            layouts,
            monitor: String::new(),
            config,
            preview,
            log,
            events,
        });
        let owned = owner.clone();
        cx.spawn(async move |cx| {
            while let Some(event) = rx.recv().await {
                owned.update(cx, |owner, cx| owner.event(event, cx));
            }
        })
        .detach();
        owner.update(cx, |owner, cx| {
            owner.mount_updater(production.as_ref(), cx);
            owner.start_core(cx);
        });
    });
    Ok(())
}

#[cfg(test)]
mod m9_review_tests;
#[cfg(test)]
mod startup_activation_tests;

#[derive(Debug)]
enum QuitPreparation {
    Saved,
    Refused(String),
    ForcedAfterCommit,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum QuitRefusalSource {
    Durability,
    ExtensionWork,
}
async fn quit_preparation<P, T>(
    prepare: P,
    timer: T,
    committed: &std::cell::Cell<bool>,
) -> QuitPreparation
where
    P: std::future::Future<Output = Result<(), String>>,
    T: std::future::Future,
{
    use std::task::Poll;
    let mut prepare = std::pin::pin!(prepare);
    let mut timer = std::pin::pin!(timer);
    std::future::poll_fn(|cx| {
        if let Poll::Ready(result) = prepare.as_mut().poll(cx) {
            return Poll::Ready(match result {
                Ok(()) => QuitPreparation::Saved,
                Err(e) => QuitPreparation::Refused(e),
            });
        }
        if timer.as_mut().poll(cx).is_ready() {
            return Poll::Ready(if committed.get() {
                QuitPreparation::ForcedAfterCommit
            } else {
                QuitPreparation::Refused("Quit save/drain exceeded eight seconds".into())
            });
        }
        Poll::Pending
    })
    .await
}
#[cfg(test)]
mod m11_deadline_tests {
    use super::*;
    #[tokio::test]
    async fn slow_preflight_returns_refused_without_destructive_callback() {
        let committed = std::cell::Cell::new(false);
        let destroyed = std::cell::Cell::new(false);
        let prepare = async {
            quit_budget::commit(Duration::from_millis(7900), &committed, || destroyed.set(true))
        };
        assert!(matches!(quit_preparation(prepare, std::future::pending::<()>(), &committed).await, QuitPreparation::Refused(_)));
        assert!(!committed.get());
        assert!(!destroyed.get());
    }
    #[tokio::test]
    async fn review_m11_timeout_after_commit_cannot_resume_cancelled_runs() {
        let committed = std::cell::Cell::new(false);
        let prepare = async {
            committed.set(true);
            std::future::pending::<Result<(), String>>().await
        };
        assert!(matches!(
            quit_preparation(prepare, std::future::ready(()), &committed).await,
            QuitPreparation::ForcedAfterCommit
        ));
    }
    #[tokio::test]
    async fn review_m11_save_timeout_before_commit_still_refuses_quit() {
        assert!(matches!(
            quit_preparation(
                std::future::pending(),
                std::future::ready(()),
                &std::cell::Cell::new(false)
            )
            .await,
            QuitPreparation::Refused(_)
        ));
    }
}
