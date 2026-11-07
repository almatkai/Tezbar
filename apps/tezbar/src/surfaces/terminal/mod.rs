//! M10 source lane. Parent explicitly mounts this retained owner; rendering never starts PTYs.
mod ime;
pub mod input;
mod request;
mod state;
#[cfg(test)]
mod tests;
mod view;
use super::command_bar::navigation::Payload;
use crate::events::CoreEvent;
use gpui::{prelude::*, *};
pub use request::{TerminalRequest, TerminalTransport};
pub use state::TerminalEffect;
use state::{Boot, State};
use std::{sync::Arc, time::Duration};
use tezbar_ui::{
    terminal_grid::{CellMetrics, Palette},
    text_input::{Committed, TextInput},
};
use tezbar_ui_model::{terminal::Defaults, ui_state::UiState};

pub struct TerminalSignal;
impl EventEmitter<TerminalSignal> for TerminalView {}
pub struct TerminalView {
    state: State,
    transport: Arc<dyn TerminalTransport>,
    focus: FocusHandle,
    job: Option<Task<()>>,
    blink: Option<Task<()>>,
    cwd_timer: Option<Task<()>>,
    cursor_on: bool,
    focused: bool,
    selection_drag: bool,
    geometry: Option<(Bounds<Pixels>, CellMetrics)>,
    preedit: ime::Preedit,
    name: Entity<TextInput>,
    _subscriptions: Vec<Subscription>,
    wheel: f32,
    scroll_wheel: input::WheelAccumulator,
    motion: input::MotionReporter,
    frame: Option<tezbar_ui::terminal_grid::Frame>,
    task_indicator: Option<AnyView>,
}
impl TerminalView {
    pub fn new(
        transport: Arc<dyn TerminalTransport>,
        ui: &UiState,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let defaults =
            Defaults::from_legacy(&serde_json::to_value(&ui.terminal_defaults).unwrap_or_default());
        let focus = cx.focus_handle();
        let name = cx.new(TextInput::new);
        name.update(cx, |field, _| field.options.name = "Session name".into());
        let name_sub = cx.subscribe(&name, |this, _, event: &Committed, _| {
            if let Some(c) = &mut this.state.configure {
                c.name = event.0.clone();
            }
        });
        let focus_in = cx.on_focus_in(&focus, w, |this, _, cx| {
            this.focused = true;
            this.sync_blink(cx);
        });
        let blur = cx.on_blur(&focus, w, |this, _, cx| {
            this.focused = false;
            this.sync_blink(cx);
        });
        let active = cx.observe_window_activation(w, |this, w, cx| {
            this.focused = this.focus.is_focused(w) && w.is_window_active();
            this.sync_blink(cx);
        });
        let mut state = State::new(defaults, ui.terminal_pinned_sessions.clone());
        state.grid.palette = Palette::new(tezbar_ui::tokens::tokens(w).light);
        Self {
            state,
            transport,
            focus,
            job: None,
            blink: None,
            cwd_timer: None,
            cursor_on: true,
            focused: false,
            selection_drag: false,
            geometry: None,
            preedit: Default::default(),
            name,
            _subscriptions: vec![name_sub, focus_in, blur, active],
            wheel: 0.,
            scroll_wheel: Default::default(),
            motion: Default::default(),
            frame: None,
            task_indicator: None,
        }
    }
    pub fn set_task_indicator(&mut self, view: AnyView, cx: &mut Context<Self>) {
        self.task_indicator = Some(view);
        cx.notify();
    }
    pub fn activate(
        &mut self,
        payload: Payload,
        nonce: u64,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Payload::Terminal {
            command,
            cwd,
            session,
            defaults,
        } = payload
        {
            let defaults =
                Defaults::from_legacy(&serde_json::to_value(defaults).unwrap_or_default());
            let previous = self.state.generation;
            self.state.activate(
                Boot {
                    command,
                    cwd,
                    session,
                    defaults,
                },
                nonce,
            );
            if previous != self.state.generation {
                self.preedit.clear();
                self.cwd_timer = None;
            }
            self.set_visible(true, w, cx);
            self.pump(cx);
        }
    }
    pub fn listeners_ready(&mut self, data: bool, exit: bool, cx: &mut Context<Self>) {
        self.state.listeners_ready(data, exit);
        self.pump(cx);
    }
    pub fn set_visible(&mut self, visible: bool, w: &mut Window, cx: &mut Context<Self>) {
        self.state.set_visible(visible);
        if self.state.visible {
            self.focus.focus(w, cx);
            self.focused = w.is_window_active();
            self.state.refresh();
        } else {
            self.cwd_timer = None;
            self.selection_drag = false;
            self.motion.reset();
            self.scroll_wheel.reset();
        }
        self.pump(cx);
        self.sync_blink(cx);
        self.changed(cx);
    }
    pub fn set_theme(&mut self, light: bool, cx: &mut Context<Self>) {
        self.state.grid.palette = Palette::new(light);
        self.state.dirty = true;
        self.changed(cx);
    }
    /// Called after measured geometry changes. Prepaint schedules this bounded
    /// follow-up; resizing and core requests happen here, outside render/paint.
    pub fn layout_changed(&mut self, cx: &mut Context<Self>) {
        if let Some((bounds, metrics)) = self.geometry {
            if let Some(size) =
                metrics.grid(f32::from(bounds.size.width), f32::from(bounds.size.height))
            {
                self.state.resize(size, metrics);
                self.pump(cx);
                self.changed(cx);
            }
        }
    }
    pub fn core_event(&mut self, event: &CoreEvent, cx: &mut Context<Self>) {
        if let CoreEvent::Other { channel, payload } = event {
            match channel.as_str() {
                "core:terminal:data" => {
                    use base64::{Engine, engine::general_purpose::STANDARD};
                    if let (Some(id), Some(g), Some(start), Some(encoded)) = (
                        payload["sessionId"].as_str(),
                        payload["generation"].as_str(),
                        payload["start"].as_u64(),
                        payload["bytes"].as_str(),
                    ) {
                        if encoded.len() <= 87384 {
                            if let Ok(bytes) = STANDARD.decode(encoded) {
                                if bytes.len() <= 65536
                                    && payload["end"].as_u64() == Some(start + bytes.len() as u64)
                                {
                                    self.state.stream_data(
                                        id,
                                        g,
                                        start,
                                        &bytes,
                                        payload["durable"] == true,
                                    );
                                }
                            }
                        }
                    }
                }
                "core:terminal:exit-event" => {
                    if let (Some(id), Some(g), Some(position), Some(code)) = (
                        payload["sessionId"].as_str(),
                        payload["generation"].as_str(),
                        payload["position"].as_u64(),
                        payload["exitCode"].as_u64(),
                    ) {
                        self.state
                            .stream_exit(id, g, position, code.min(u32::MAX as u64) as u32);
                    }
                }
                "terminal:sessions-changed" => self.state.refresh(),
                _ => return,
            }
            self.pump(cx);
            self.sync_blink(cx);
            self.changed(cx);
        }
    }
    pub fn host_generation(&self) -> u64 {
        self.state.generation
    }
    pub fn paste_result(&mut self, generation: u64, text: &str, cx: &mut Context<Self>) {
        self.state.paste(generation, text);
        self.pump(cx);
        self.changed(cx);
    }
    pub fn drain(&mut self) -> Vec<(u64, TerminalEffect)> {
        self.state.effects.drain(..).collect()
    }
    pub fn dispose(&mut self, cx: &mut Context<Self>) {
        self.blink = None;
        self.cwd_timer = None;
        self.preedit.clear();
        self.state.dispose();
        self.pump(cx);
    }
    pub(crate) fn prepare_quit(&mut self) {
        self.state.quit_preparing = true;
    }
    pub(crate) fn revive_after_quit(&mut self, cx: &mut Context<Self>) {
        self.state.quit_preparing = false;

        self.changed(cx);
    }
    pub fn is_quiescent(&self) -> bool {
        self.job.is_none() && self.state.work.is_empty()
    }
    pub fn retry_cleanup(&mut self, cx: &mut Context<Self>) {
        self.state.retry_cleanup();
        self.pump(cx);
    }
    pub fn quit_error(&self) -> Option<&str> {
        self.state.cleanup_error.as_deref()
    }
    pub fn error(&self) -> Option<&str> {
        self.state.error.as_deref()
    }
    pub fn active_session(&self) -> Option<&str> {
        self.state.active.as_deref()
    }
    pub fn session_rows(&self) -> &[tezbar_ui_model::terminal::Session] {
        &self.state.sessions.rows
    }
    pub fn select_session(&mut self, id: &str, cx: &mut Context<Self>) {
        self.state.select(id);
        self.preedit.clear();
        self.cwd_timer = None;
        self.pump(cx);
        self.changed(cx);
    }
    pub fn new_session(&mut self, cx: &mut Context<Self>) {
        self.state.new_session();
        self.preedit.clear();
        self.pump(cx);
        self.changed(cx);
    }
    pub fn paste(&mut self, generation: u64, text: &str, cx: &mut Context<Self>) {
        self.state.paste(generation, text);
        self.pump(cx);
        self.changed(cx);
    }
    pub fn update_pins(&mut self, pins: Vec<String>) {
        self.state.pins(pins);
    }
    fn changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(TerminalSignal);
        if self.state.visible {
            cx.notify();
        }
    }
    fn pump(&mut self, cx: &mut Context<Self>) {
        if self.job.is_some() {
            return;
        }
        let Some(work) = self.state.next_work() else {
            return;
        };
        let transport = self.transport.clone();
        let request = work.request.clone();
        // Retain this task through hide/selection/dispose. The parent retains our owner
        // until quiescent, so an abandoned create always returns an identity to detach.
        self.job = Some(cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move { transport.send(request).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            cx.background_executor()
                .timer(Duration::from_millis(0))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.job = None;
                this.state.complete(work, result);
                this.pump(cx);
                this.sync_blink(cx);
                this.changed(cx);
            });
        }));
    }
    fn wants_blink(&self) -> bool {
        self.state.visible
            && !self.state.disposed
            && self.state.running
            && self.focused
            && self.preedit.text.is_empty()
            && self.state.configure.is_none()
            && self.state.grid.blink_needed()
    }
    fn sync_blink(&mut self, cx: &mut Context<Self>) {
        if !self.wants_blink() {
            self.blink = None;
            self.cursor_on = true;
            return;
        }
        if self.blink.is_some() {
            return;
        }
        self.blink = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let keep = this
                    .update(cx, |this, cx| {
                        if !this.wants_blink() {
                            this.blink = None;
                            return false;
                        }
                        this.cursor_on = !this.cursor_on;
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        }));
    }
    fn track_cwd(&mut self, cx: &mut Context<Self>) {
        let Some((id, command, revision)) = self.state.cwd_retries.take() else {
            return;
        };
        self.cwd_timer = None;
        if !self.state.visible {
            return;
        }
        self.cwd_timer = Some(cx.spawn(async move |this, cx| {
            for delay in [80, 270, 850] {
                cx.background_executor()
                    .timer(Duration::from_millis(delay))
                    .await;
                if this
                    .update(cx, |this, cx| {
                        this.state
                            .refresh_cwd(id.clone(), command.clone(), revision);
                        this.pump(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| this.cwd_timer = None);
        }));
    }
    fn key(&mut self, event: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        if !self.state.visible || self.state.quit_preparing {
            return;
        }
        if event.prefer_character_input || !self.preedit.text.is_empty() {
            return;
        }
        let k = &event.keystroke;
        let result = input::key(
            &k.key,
            "",
            modifiers(k.modifiers),
            self.state.grid.mode(),
            false,
        );
        if matches!(result, input::KeyResult::PlatformText) {
            return;
        }
        if self.state.configure.is_some()
            && !matches!(result, input::KeyResult::Back | input::KeyResult::Configure)
        {
            return;
        }
        cx.stop_propagation();
        w.prevent_default();
        let configure = matches!(result, input::KeyResult::Configure);
        self.state.handle_key(result);
        if configure {
            if let Some(c) = &self.state.configure {
                self.name.update(cx, |n, cx| n.set_text(&c.name, w, cx));
            }
        }
        self.cursor_on = true;
        self.track_cwd(cx);
        self.pump(cx);
        self.sync_blink(cx);
        self.changed(cx);
    }
    fn mouse_cell(&self, p: Point<Pixels>) -> Option<(usize, usize)> {
        let (b, m) = self.geometry?;
        Some(m.cell(
            f32::from(p.x - b.left()),
            f32::from(p.y - b.top()),
            self.state.grid.size,
        ))
    }
}
fn modifiers(m: gpui::Modifiers) -> input::Modifiers {
    input::Modifiers {
        command: m.platform,
        control: m.control,
        alt: m.alt,
        shift: m.shift,
    }
}
impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
