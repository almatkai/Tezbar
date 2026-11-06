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
    tasks: Vec<Task<()>>,
    idle: Option<Task<()>>,
    blur: Option<Task<()>>,
    snap_timer: Option<Task<()>>,
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
        if self.lifecycle.quitting {
            return;
        }
        let core = self.core.clone();
        self.startup = Some(cx.spawn(async move |this, cx| {
            let ready = gpui_tokio::Tokio::spawn(cx, async move { core.start().await }).await;
            let _ = this.update(cx, |owner, cx| {
                let _ = owner.launcher.update(cx, |view, _, cx| {
                    view.ready = matches!(ready, Ok(Ok(())));
                    view.error = match ready {
                        Ok(Err(e)) => Some(e),
                        Err(e) => Some(e.to_string()),
                        _ => None,
                    };
                    cx.notify();
                });
                if std::mem::take(&mut owner.show_on_ready) {
                    owner.show(cx);
                }
                let _ = owner.launcher.update(cx, |view, _, cx| {
                    let query = view.input.read(cx).content.to_string();
                    view.query(query, cx);
                });
            });
        }));
    }
    fn restore(&mut self, cx: &mut Context<Self>) {
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
                let _ = native::place_scaled(w, bounds, scale);
                view.zoom = profile.zoom as f32;
                placed = true;
                let _ = native::visible(w, true);
                view.show(w, cx);
                #[cfg(windows)]
                {
                    let _ = native::place_scaled(w, bounds, scale);
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
        if !self.lifecycle.placed || self.monitor.is_empty() {
            return;
        }
        if let Ok((p, profile)) = self.launcher.update(cx, |v, w, _| {
            let b = w.bounds();
            (
                PersistedWindowPosition {
                    x: f64::from(b.left()) * f64::from(w.scale_factor()),
                    y: f64::from(b.top()) * f64::from(w.scale_factor()),
                },
                Profile {
                    width: f64::from(b.size.width),
                    height: f64::from(b.size.height),
                    zoom: v.zoom as f64,
                },
            )
        }) {
            self.positions.remember(self.monitor.clone(), p);
            self.layouts.insert(self.monitor.clone(), profile);
        }
    }
    fn flush(&mut self) {
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
        self.hide(cx);
        if let Some(settings) = self.settings {
            let _ = settings.update(cx, |_, w, cx| {
                let _ = native::visible(w, true);
                w.activate_window();
                cx.activate(true);
            });
            return;
        }
        let relay = self.relay.clone();
        let on_close = relay.clone();
        let settings_bounds = Bounds::centered(None, size(px(920.), px(680.)), cx);
        match cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(settings_bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Tezbar Settings".into()),
                    ..Default::default()
                }),
                kind: WindowKind::Normal,
                ..Default::default()
            },
            move |w, cx| {
                w.on_window_should_close(cx, move |_, _| {
                    let _ = on_close.send(Event::CloseSettings);
                    false
                });
                cx.new(|cx| Settings::new(relay, w, cx))
            },
        ) {
            Ok(window) => {
                self.settings = Some(window);
                self.lifecycle.settings = true;
                let _ = window.update(cx, |_, w, cx| {
                    let _ = native::visible(w, true);
                    cx.activate(true);
                    w.activate_window();
                });
            }
            Err(e) => {
                let _ = self.log.write(e.to_string().as_bytes());
            }
        }
    }
    fn quit(&mut self, cx: &mut Context<Self>) {
        if !self.lifecycle.begin_quit() {
            return;
        }
        self.snapshot(cx);
        self.flush();
        let settings_drain = self
            .settings
            .map(|window| window.update(cx, |view, _, _| view.drain_writes()));
        let drain = self.writer.drain();
        self.tasks.clear();
        self.startup = None;
        self.idle = None;
        self.blur = None;
        self.snap_timer = None;
        #[cfg(feature = "bench-control")]
        let gate = self.shutdown_gate.clone();
        self.tasks.push(cx.spawn(async move |this, cx| {
            #[cfg(feature = "bench-control")]
            gate.wait().await;
            let prepared = gpui_tokio::Tokio::spawn(cx, async move {
                if let Some(settings_drain) = settings_drain {
                    settings_drain.map_err(|e| e.to_string())?.await?;
                }
                if let Some(drain) = drain {
                    drain.await.map_err(|e| e.to_string())?
                } else {
                    Ok(())
                }
            })
            .await;
            let core = this.update(cx, |this, cx| {
                if !matches!(prepared, Ok(Ok(()))) {
                    this.lifecycle.quitting = false;
                    this.writer = windows::persistence::Writer::new(
                        this.core.0.ctx.config.clone(),
                        this.preview.root.join("config/launcher-layouts-v1.json"),
                    );
                    let _=this.log.write(format!("Could not drain Settings/window layout; quit cancelled: {prepared:?}\n").as_bytes());
                    if let Some(settings)=this.settings{let _=settings.update(cx,|view,_,cx|{view.error=Some("Couldn't save Settings. Retry the edit, or retry saved changes from a previous tab.".into());cx.notify();});}
                    windows::termination::complete(false);
                    this.restore(cx);
                    return None;
                }
                this.events.dispose();
                let _ = this.launcher.update(cx, |view, _, cx| {
                    view.ready = false;
                    cx.notify();
                });
                if let Some(instance) = &this.instance {
                    instance.begin_shutdown();
                }
                if let Some(tray) = this.tray.take() {
                    let _ = this.host.desktop.unregister(&tray.shortcut);
                }
                Some(this.core.clone())
            });
            let Ok(Some(core)) = core else {
                return;
            };
            let result = gpui_tokio::Tokio::spawn(cx, async move { core.shutdown().await }).await;
            let _ = this.update(cx, |this, cx| {
                if !matches!(result, Ok(Ok(()))) {
                    let error = format!("Quit deferred: {result:?}");
                    let _ = this.log.write(error.as_bytes());
                    this.lifecycle.quitting = false;
                    let _ = this.launcher.update(cx, |view, _, cx| {
                        view.error = Some(error);
                        cx.notify();
                    });
                    windows::termination::complete(false);
                    return;
                }
                this.relay.close();
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
            SurfaceOp::SnapDragStart => {
                self.snap.begin();
                let _ = self.launcher.update(cx, |_, w, _| w.start_window_move());
            }
            SurfaceOp::SnapDragEnd => self.end_snap(cx),
            SurfaceOp::PressKey { key, modifiers } => {
                // This is the intentional native input effect, not a core request. Return its
                // actual outcome instead of acknowledging an unowned detached task.
                tezbar_host::input::press_key(key, modifiers)?;
            }
            SurfaceOp::ClearLegacyWebviewCache => { /* Isolated preview has no legacy profile and never touches production web data. */
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
                serde_json::json!({"visible":self.lifecycle.visible,"settings":self.lifecycle.settings,"quitting":self.lifecycle.quitting,"pid":std::process::id()}),
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
                let rows = serde_json::from_value(request["sessions"].clone())
                    .map_err(|e| e.to_string())?;
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
            self.tasks.push(cx.spawn(async move |_, cx| {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                let _ = relay.send(Event::SettingsClose(g));
            }));
        }
    }
    fn finish_settings_close(&mut self, cx: &mut Context<Self>) {
        if let Some(settings) = self.settings.take() {
            let _ = settings.update(cx, |_, w, _| w.remove_window());
        }
        self.lifecycle.settings = false;
    }
    fn panel(&mut self, rows: Vec<(String, String)>, cx: &mut Context<Self>) {
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
            let _ = panel.update(cx, |view, w, cx| {
                view.sessions = rows;
                let _ = native::place(w, bounds);
                let _ = native::visible(w, true);
                cx.notify();
            });
            return;
        }
        let relay = self.relay.clone();
        self.panel = cx
            .open_window(
                WindowOptions {
                    kind: WindowKind::Floating,
                    titlebar: None,
                    focus: false,
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                move |w, cx| {
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
                let _ = panel.update(cx, |_, w, _| native::place(w, bounds));
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
            if locked.x || locked.y {
                let _ = self.launcher.update(cx, |_, w, _| {
                    native::place(
                        w,
                        Bounds::new(
                            point(
                                px((position.x / scale) as f32),
                                px((position.y / scale) as f32),
                            ),
                            bounds.size,
                        ),
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
                            window_bounds: Some(WindowBounds::Windowed(monitor)),
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
                    let _ = native::place_scaled(w, monitor, scale);
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
    fn end_snap(&mut self, cx: &mut Context<Self>) {
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
                if let Some(settings) = self.settings {
                    let _ = settings.update(cx, |view, _, cx| {
                        view.theme = theme;
                        cx.notify();
                    });
                }
            }
            Event::Quit => self.quit(cx),
            Event::RetryCore => self.start_core(cx),
            Event::CloseSettings => {
                let _ = self.surface(SurfaceOp::CloseCurrent, cx);
            }
            Event::Bounds => {
                self.snapshot(cx);
                self.viewport_event(cx);
                self.follow_panel(cx);
                self.snap_update(None, cx);
            }
            Event::SnapBegin => {
                self.snap.begin();
                let _ = self.launcher.update(cx, |_, w, _| w.start_window_move());
            }
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
                if self.tray.as_ref().is_some_and(|t| t.shortcut.id() == id) {
                    if self.lifecycle.visible {
                        self.hide(cx)
                    } else {
                        self.restore(cx)
                    }
                } else if let Some(command) = self.core.0.platform.command_for_shortcut(id) {
                    let core = self.core.clone();
                    self.tasks.push(cx.spawn(async move |_, cx| {
                        let _ = gpui_tokio::Tokio::spawn(cx, async move {
                            core.0.execute_command_shortcut(&command).await
                        })
                        .await;
                    }));
                }
            }
            Event::Core(CoreEvent::OpenSurface(payload)) => {
                self.events
                    .publish(CoreEvent::OpenSurface(payload.clone()), cx);
                if payload.as_str().is_some_and(|s| s.starts_with("settings")) {
                    self.settings(cx);
                }
            }
            Event::Core(event) => {
                self.events.publish(event, cx);
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
                    return;
                }
                self.lifecycle.suppress(true);
                let lease = events::DialogLease(self.relay.clone());
                match req {
                    DialogRequest::Confirm(value) => {
                        let message = value["title"].as_str().unwrap_or("Confirm").to_string();
                        let detail = value["detail"]
                            .as_str()
                            .or_else(|| value["message"].as_str())
                            .map(str::to_owned);
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
                        let prompt = self.launcher.update(cx, |_, w, cx| {
                            w.prompt(
                                PromptLevel::Warning,
                                &message,
                                detail.as_deref(),
                                &answers,
                                cx,
                            )
                        });
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
            Event::DialogClosed => self.lifecycle.suppress(false),
            Event::SettingsAck(g) => {
                self.settings_close.acknowledge(g);
            }
            Event::SettingsFinish(g, result) => {
                let success = result.is_ok();
                if self.settings_close.finish(g, success) {
                    self.finish_settings_close(cx);
                } else if let Err(error) = result {
                    if let Some(settings) = self.settings {
                        let _ = settings.update(cx, |view, _, cx| {
                            view.error = Some(error);
                            cx.notify();
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
pub fn run(preview: Preview) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let handle = runtime.handle().clone();
    let _entered = handle.enter();
    let (relay, mut rx) = Relay::new();
    let activate = relay.clone();
    let instance = match runtime
        .block_on(Instance::claim(
            &preview.root,
            "com.tezbar.preview.native",
            Arc::new(move || {
                let _ = activate.send(Event::Activate);
            }),
        ))
        .map_err(anyhow::Error::msg)?
    {
        Claim::Activated => return Ok(()),
        Claim::Owner(instance) => instance,
    };
    tezbar_ui::tokens::set(&preview.theme).map_err(anyhow::Error::msg)?;
    let log: &'static tezbar_host::logging::FileLog = Box::leak(Box::new(
        tezbar_host::logging::FileLog::open(&preview.root.join("logs"), "tezbar")
            .map_err(anyhow::Error::msg)?,
    ));
    log::set_logger(log).map_err(|e| anyhow::anyhow!("Native logger: {e}"))?;
    log::set_max_level(log::LevelFilter::Info);
    let application = gpui_platform::application();
    let reopen = relay.clone();
    application.on_reopen(move |_| {
        let _ = reopen.send(Event::Activate);
    });
    application.run(move |cx| {
        gpui_tokio::init_from_handle(cx, handle);
        cx.set_global(RuntimeOwner(Some(runtime)));
        cx.set_window_appearance(match preview.theme.as_str() {
            "light" | "white" => Some(WindowAppearance::Light),
            "dark" => Some(WindowAppearance::Dark),
            _ => None,
        });
        cx.set_activation_policy(ActivationPolicy::Regular);
        cx.set_quit_mode(QuitMode::Explicit);
        tezbar_ui::text_input::bind_keys(cx);
        tezbar_ui::markdown::bind_keys(cx);
        events::install_native_handlers(&relay);
        windows::termination::install(&relay).expect("native termination adapter");
        cx.bind_keys([
            KeyBinding::new(
                if cfg!(target_os = "macos") {
                    "cmd-q"
                } else {
                    "alt-f4"
                },
                RequestQuit,
                None,
            ),
            KeyBinding::new(
                if cfg!(target_os = "macos") {
                    "cmd-,"
                } else {
                    "ctrl-,"
                },
                OpenSettings,
                None,
            ),
        ]);
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
        let host = Arc::new(GpuiHost::new(relay.clone()));
        let context = CoreContext::new(preview.core_paths(), Some(host.clone()))
            .with_dev_mode(true)
            .with_legacy_data_dir(None);
        let core = CoreClient::new(tezbar_core::NativeCore::new(context).expect("isolated core"));
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
                    window_background: WindowBackgroundAppearance::Transparent,
                    app_id: Some("com.tezbar.preview.native".into()),
                    ..Default::default()
                },
                move |w, cx| {
                    let close = view_relay.clone();
                    w.on_window_should_close(cx, move |_, _| {
                        let _ = close.send(Event::Hide);
                        false
                    });
                    cx.new(|cx| Launcher::new(view_core, view_relay, w, cx))
                },
            )
            .expect("launcher window");
        let tray = PrimaryTray::new(&host.desktop, &preview.resources)
            .map_err(|e| {
                let _ = log.write(e.as_bytes());
            })
            .ok();
        let config = core.0.ctx.config.read().unwrap_or_default();
        let layouts = std::fs::read_to_string(preview.root.join("config/launcher-layouts-v1.json"))
            .ok()
            .map(|s| tezbar_host::layout::profiles_from_json(&s))
            .unwrap_or_default();
        let writer = windows::persistence::Writer::new(
            core.0.ctx.config.clone(),
            preview.root.join("config/launcher-layouts-v1.json"),
        );
        #[cfg(feature = "bench-control")]
        let control = preview.control_token.as_deref().map(|token| {
            crate::bench_control::Server::start(&preview.root, token, relay.clone())
                .expect("isolated control")
        });
        let show = preview.show;
        let owner = cx.new(|_| Owner {
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
            idle: None,
            blur: None,
            snap_timer: None,
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
        owner.update(cx, |owner, cx| owner.start_core(cx));
    });
    Ok(())
}
