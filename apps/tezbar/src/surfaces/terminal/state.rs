//! Explicit session/order owner. Parsing is exclusively alacritty_terminal in terminal_grid.
use super::{
    input,
    request::{Attached, Create, Reply, TerminalRequest, Update},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use tezbar_ui::terminal_grid::{CellMetrics, GridSize, TerminalGrid};
use tezbar_ui_model::terminal::{self as model, Defaults, Session};

const WRITE_BUDGET: usize = 1024 * 1024;
const WORK_LIMIT: usize = 64;
const STARTUP_LIMIT: usize = 64;
struct PendingStartup {
    generation: String,
    through: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub enum TerminalEffect {
    Back { sentinel: &'static str },
    OpenSettings,
    SessionsToggle,
    OpenUrl(String),
    Copy(String),
    ReadPaste { generation: u64 },
    Zoom(input::Zoom),
    TogglePin(String),
    Unpin(String),
}
#[derive(Clone, Debug)]
pub struct Boot {
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub session: Option<String>,
    pub defaults: Defaults,
}
#[derive(Clone, Debug)]
pub enum Purpose {
    Boot(Boot),
    List,
    Create,
    Attach(String),
    Resume(String),
    Replay,
    Write,
    ProtocolReply,
    Resize,
    Cwd {
        id: String,
        revision: u64,
        command: String,
    },
    Update,
    Cleanup,
    Mutation,
}
#[derive(Clone, Debug)]
pub struct Work {
    pub generation: u64,
    pub request: TerminalRequest,
    pub purpose: Purpose,
}
impl Work {
    pub fn cleanup(&self) -> bool {
        matches!(self.purpose, Purpose::Cleanup)
    }
}
#[derive(Clone)]
pub struct Configure {
    pub id: String,
    pub generation: u64,
    pub name: String,
    pub defaults: Defaults,
}

pub struct State {
    pub grid: TerminalGrid,
    pub sessions: model::Sessions,
    pub active: Option<String>,
    pub visible: bool,
    pub disposed: bool,
    pub quit_preparing: bool,
    pub font_size: u8,
    pub configure: Option<Configure>,
    pub error: Option<String>,
    pub cleanup_error: Option<String>,
    failed_cleanup: Vec<Work>,
    pub generation: u64,
    pub metrics: CellMetrics,
    pub effects: VecDeque<(u64, TerminalEffect)>,
    pub work: VecDeque<Work>,
    pub dirty: bool,
    pub running: bool,
    visibility_detached: bool,
    listeners: bool,
    data_ready: bool,
    exit_ready: bool,
    boot: Option<Boot>,
    nonce: Option<u64>,
    replaying: bool,
    stream_generation: Option<String>,
    replay_token: Option<String>,
    position: u64,
    watermark: u64,
    pending_startup: BTreeMap<String, PendingStartup>,
    pending_input: VecDeque<(Vec<u8>, bool)>,
    pending_input_bytes: usize,
    target: u64,
    final_exit: Option<(String, u64, u32)>,
    input: String,
    cwd_revision: u64,
    pub cwd_retries: Option<(String, String, u64)>,
}
impl State {
    pub fn new(defaults: Defaults, pins: Vec<String>) -> Self {
        let mut sessions = model::Sessions::default();
        sessions.defaults = defaults;
        sessions.pins = pins.into_iter().collect();
        Self {
            grid: TerminalGrid::new(GridSize::default(), false),
            sessions,
            active: None,
            visible: false,
            disposed: false,
            quit_preparing: false,
            font_size: 12,
            configure: None,
            error: None,
            cleanup_error: None,
            failed_cleanup: Vec::new(),
            generation: 0,
            metrics: CellMetrics {
                width: 7.,
                height: 14.64,
            },
            effects: VecDeque::new(),
            work: VecDeque::new(),
            dirty: true,
            running: false,
            visibility_detached: false,
            listeners: false,
            data_ready: false,
            exit_ready: false,
            boot: None,
            nonce: None,
            replaying: false,
            stream_generation: None,
            replay_token: None,
            position: 0,
            watermark: 0,
            pending_startup: BTreeMap::new(),
            pending_input: VecDeque::new(),
            pending_input_bytes: 0,
            target: 0,
            final_exit: None,
            input: String::new(),
            cwd_revision: 0,
            cwd_retries: None,
        }
    }
    pub fn activate(&mut self, boot: Boot, nonce: u64) {
        if self.disposed || self.nonce.is_some_and(|n| nonce <= n) {
            return;
        }
        self.nonce = Some(nonce);
        self.generation += 1;
        self.cancel_queued();
        self.detach_active();
        self.configure = None;
        self.error = None;
        self.sessions.defaults = boot.defaults.clone();
        self.boot = Some(boot);
        self.start_boot();
    }
    pub fn listeners_ready(&mut self, data: bool, exit: bool) {
        self.data_ready |= data;
        self.exit_ready |= exit;
        self.listeners = self.data_ready && self.exit_ready;
        self.start_boot();
    }
    fn start_boot(&mut self) {
        if self.listeners {
            if let Some(b) = self.boot.take() {
                self.enqueue(TerminalRequest::List, Purpose::Boot(b));
            }
        }
    }
    fn cancel_queued(&mut self) {
        self.work
            .retain(|w| w.cleanup() || matches!(w.purpose, Purpose::ProtocolReply));
        self.cwd_retries = None;
    }
    pub fn enqueue(&mut self, r: TerminalRequest, p: Purpose) -> bool {
        if self.disposed && !matches!(p, Purpose::Cleanup) {
            return false;
        }
        if r.retained_bytes() > WRITE_BUDGET {
            self.error = Some("Terminal request exceeds 1 MiB.".into());
            return false;
        }
        let queued_bytes = self
            .work
            .iter()
            .map(|w| w.request.retained_bytes())
            .sum::<usize>();
        let size = r.retained_bytes();
        if self.work.len()
            >= if matches!(p, Purpose::Cleanup) {
                WORK_LIMIT
            } else if matches!(p, Purpose::ProtocolReply) {
                WORK_LIMIT - 4
            } else {
                WORK_LIMIT - 5
            }
            || (queued_bytes + size > WRITE_BUDGET && !matches!(p, Purpose::Cleanup))
        {
            self.error = Some("Terminal input is still draining. Try again shortly.".into());
            return false;
        }
        self.work.push_back(Work {
            generation: self.generation,
            request: r,
            purpose: p,
        });
        true
    }
    pub fn next_work(&mut self) -> Option<Work> {
        self.flush_pending_input();
        while let Some(w) = self.work.pop_front() {
            if w.generation == self.generation
                && matches!(w.purpose, Purpose::Create)
                && self.pending_startup.len() >= STARTUP_LIMIT
            {
                self.error=Some("Terminal starts are waiting for attachment. Open or stop a pending session before creating another.".into());
                self.running = false;
                self.replaying = false;
                self.discard_pending_input();
                continue;
            }
            if w.cleanup()
                || matches!(w.purpose, Purpose::ProtocolReply)
                || w.generation == self.generation
            {
                return Some(w);
            }
        }
        None
    }
    fn detach_request(&mut self, id: String, generation: Option<String>) -> bool {
        generation.is_some_and(|generation| {
            self.enqueue(
                TerminalRequest::DetachGeneration { id, generation },
                Purpose::Cleanup,
            )
        })
    }
    fn detach_active(&mut self) {
        if let Some(id) = self.active.take() {
            if !self.visibility_detached {
                self.detach_request(id, self.stream_generation.clone());
            }
        }
        self.release_replay();
        self.stream_generation = None;
        self.position = 0;
        self.target = 0;
        self.final_exit = None;
        self.running = false;
        self.replaying = false;
        self.visibility_detached = false;
        self.input.clear();
        self.pending_input.clear();
        self.pending_input_bytes = 0;
    }
    /// Visibility changes core attachment policy, while retaining every local grid cell.
    /// Events remain owned, so a successful resume never replays/resets retained state.
    pub fn set_visible(&mut self, visible: bool) {
        let visible = visible && !self.disposed;
        if visible == self.visible {
            return;
        }
        self.visible = visible;
        if !visible {
            self.detach_hidden();
        } else if self.visibility_detached {
            self.visibility_detached = false;
            if let Some(id) = self.active.clone() {
                self.replaying = self.enqueue(
                    TerminalRequest::Attach {
                        id: id.clone(),
                        cols: self.grid.size.cols,
                        rows: self.grid.size.rows,
                    },
                    Purpose::Resume(id),
                );
                if !self.replaying {
                    self.running = false;
                    self.discard_pending_input();
                }
            }
        }
    }
    fn detach_hidden(&mut self) {
        if !self.visible && !self.visibility_detached {
            if let Some(id) = self.active.clone() {
                self.visibility_detached = self.detach_request(id, self.stream_generation.clone());
            }
        }
    }
    pub fn retry_cleanup(&mut self) {
        self.cleanup_error = None;
        for work in std::mem::take(&mut self.failed_cleanup) {
            self.work.push_back(work);
        }
    }
    pub fn dispose(&mut self) {
        if self.disposed {
            return;
        }
        self.generation += 1;
        self.cancel_queued();
        self.detach_active();
        self.disposed = true;
        self.visible = false;
        self.effects.clear();
        self.configure = None;
    }
    pub fn select(&mut self, id: &str) {
        if self.quit_preparing {
            return;
        }
        if self.disposed || !self.listeners {
            return;
        }
        let Some(s) = self
            .sessions
            .rows
            .iter()
            .find(|s| s.session_id == id)
            .cloned()
        else {
            return;
        };
        self.generation += 1;
        self.cancel_queued();
        self.detach_active();
        self.configure = None;
        self.sessions.selected = self.sessions.rows.iter().position(|s| s.session_id == id);
        if s.running {
            self.attach(id.to_owned());
        } else {
            self.create(Create {
                cwd: Some(s.cwd),
                restore_session_id: Some(s.session_id),
                restore_command: s.last_command.or(s.initial_command),
                name: Some(s.name),
                defaults: Defaults {
                    save_for: s.save_for,
                    keep_alive_for: s.keep_alive_for,
                },
                cols: self.grid.size.cols,
                rows: self.grid.size.rows,
                ..Default::default()
            });
        }
    }
    pub fn new_session(&mut self) {
        if self.quit_preparing {
            return;
        }
        if self.disposed || !self.listeners {
            return;
        }
        self.generation += 1;
        self.cancel_queued();
        self.detach_active();
        self.create(Create {
            cols: self.grid.size.cols,
            rows: self.grid.size.rows,
            defaults: self.sessions.defaults.clone(),
            ..Default::default()
        });
    }
    fn create(&mut self, r: Create) {
        self.replaying = self.enqueue(TerminalRequest::Create(r), Purpose::Create);
        if !self.replaying {
            self.running = false;
            self.discard_pending_input();
        }
    }
    fn attach(&mut self, id: String) {
        self.active = Some(id.clone());
        self.replaying = self.enqueue(
            TerminalRequest::Attach {
                id: id.clone(),
                cols: self.grid.size.cols,
                rows: self.grid.size.rows,
            },
            Purpose::Attach(id),
        );
        if !self.replaying {
            self.running = false;
            self.discard_pending_input();
        }
    }
    pub fn refresh(&mut self) {
        if !self.disposed && !self.work.iter().any(|w| matches!(w.purpose, Purpose::List)) {
            self.enqueue(TerminalRequest::List, Purpose::List);
        }
    }
    fn remember_startup(&mut self, value: &Value) {
        let Some((id, generation)) = value["sessionId"]
            .as_str()
            .zip(value["generation"].as_str())
            .filter(|(id, g)| !id.is_empty() && !g.is_empty() && id.len() <= 200 && g.len() <= 200)
        else {
            return;
        };
        if self.pending_startup.len() < STARTUP_LIMIT || self.pending_startup.contains_key(id) {
            self.pending_startup.insert(
                id.into(),
                PendingStartup {
                    generation: generation.into(),
                    through: 0,
                },
            );
        } else {
            self.error =
                Some("Terminal startup tracking is full. Open a pending session first.".into());
        }
    }
    pub fn complete(&mut self, w: Work, result: Reply) {
        if w.cleanup() {
            if let Err(error) = result {
                self.cleanup_error = Some(error.chars().take(2048).collect());
                self.error = self.cleanup_error.clone();
                self.failed_cleanup.push(w);
            }
            return;
        }
        // A retained Create job can finish after navigation. Its process identity
        // remains ours until its first startup replay is actually consumed.
        if matches!(w.purpose, Purpose::Create) {
            if let Ok(value) = &result {
                self.remember_startup(value);
            }
        }
        if w.generation != self.generation || self.disposed {
            if matches!(
                w.purpose,
                Purpose::Create | Purpose::Attach(_) | Purpose::Resume(_)
            ) {
                if let Ok(value) = result {
                    if let Some(token) = value["replay"].as_str() {
                        self.enqueue(TerminalRequest::Release(token.into()), Purpose::Cleanup);
                    }
                    if let Some(id) = value["sessionId"].as_str() {
                        self.detach_request(
                            id.into(),
                            value["generation"].as_str().map(str::to_owned),
                        );
                    }
                }
            }
            return;
        }
        let value = match result {
            Ok(v) => v,
            Err(e) => {
                self.error = Some(e.chars().take(2048).collect());
                self.replaying = false;
                if matches!(
                    w.purpose,
                    Purpose::Create | Purpose::Attach(_) | Purpose::Resume(_)
                ) {
                    self.running = false;
                    self.discard_pending_input();
                }
                if matches!(w.purpose, Purpose::Write | Purpose::ProtocolReply) {
                    self.work
                        .retain(|w| !matches!(w.purpose, Purpose::Write | Purpose::ProtocolReply));
                    self.discard_pending_input();
                }
                if matches!(w.purpose, Purpose::Replay) {
                    self.release_replay();
                    if e.contains("expired or released") {
                        if let Some(id) = self.active.clone() {
                            self.replaying = self.enqueue(
                                TerminalRequest::Attach {
                                    id: id.clone(),
                                    cols: self.grid.size.cols,
                                    rows: self.grid.size.rows,
                                },
                                Purpose::Resume(id),
                            );
                        }
                    }
                    if !self.replaying {
                        self.discard_pending_input();
                    }
                }
                return;
            }
        };
        match w.purpose {
            Purpose::Boot(b) => {
                if !self.replace_rows(value) {
                    return;
                }
                if let Some(id) = b
                    .session
                    .filter(|id| self.sessions.rows.iter().any(|s| &s.session_id == id))
                {
                    self.select(&id);
                } else if b.command.is_some()
                    || b.cwd.is_some()
                    || !self.sessions.rows.iter().any(|s| s.running)
                {
                    self.create(Create {
                        cwd: b.cwd,
                        initial_command: b.command,
                        defaults: b.defaults,
                        cols: self.grid.size.cols,
                        rows: self.grid.size.rows,
                        ..Default::default()
                    });
                } else if let Some(id) = self
                    .sessions
                    .rows
                    .iter()
                    .find(|s| s.running)
                    .map(|s| s.session_id.clone())
                {
                    self.attach(id);
                }
            }
            Purpose::List => {
                self.replace_rows(value);
            }
            Purpose::Create => match serde_json::from_value::<Attached>(value.clone()) {
                Ok(r) => {
                    self.upsert(r.summary);
                    self.attach(r.session_id);
                }
                Err(e) => {
                    if let Some(id) = value["sessionId"].as_str() {
                        self.detach_request(
                            id.into(),
                            value["generation"].as_str().map(str::to_owned),
                        );
                    }
                    self.error = Some(e.to_string());
                    self.running = false;
                    self.replaying = false;
                    self.discard_pending_input();
                }
            },
            Purpose::Attach(id) => {
                if value.is_null() {
                    self.active = None;
                    self.running = false;
                    self.replaying = false;
                    self.discard_pending_input();
                    self.refresh();
                    return;
                }
                match serde_json::from_value::<Attached>(value) {
                    Ok(r) if r.session_id == id && self.active.as_deref() == Some(&id) => {
                        self.install_stream(r, false);
                    }
                    _ => {
                        self.error = Some("Terminal attach returned a different session.".into());
                        self.running = false;
                        self.replaying = false;
                        self.discard_pending_input();
                    }
                }
            }
            Purpose::Resume(id) => {
                if value.is_null() {
                    self.running = false;
                    self.replaying = false;
                    self.active = None;
                    self.discard_pending_input();
                    self.refresh();
                } else if value["sessionId"].as_str() == Some(&id)
                    && self.active.as_deref() == Some(&id)
                {
                    match serde_json::from_value::<Attached>(value) {
                        Ok(r) => self.install_stream(r, true),
                        Err(e) => {
                            self.error = Some(e.to_string());
                            self.running = false;
                            self.replaying = false;
                            self.discard_pending_input();
                        }
                    }
                } else {
                    self.error = Some("Terminal resume returned a different session.".into());
                    self.running = false;
                    self.replaying = false;
                    self.discard_pending_input();
                }
            }
            Purpose::Replay => self.replay_chunk(value),
            Purpose::Cwd {
                id,
                revision,
                command,
            } if revision == self.cwd_revision && self.active.as_deref() == Some(&id) => {
                if let Some(cwd) = value.as_str().filter(|s| !s.is_empty()) {
                    let name = model::session_name(cwd, &command);
                    let unchanged = self.sessions.rows.iter().any(|s| {
                        s.session_id == id
                            && s.name == name
                            && s.cwd == cwd
                            && s.last_command.as_deref() == Some(&command)
                    });
                    if !unchanged {
                        self.enqueue(
                            TerminalRequest::Update(Update {
                                session_id: id,
                                name: Some(name),
                                cwd: Some(cwd.into()),
                                last_command: Some(command),
                                save_for: None,
                                keep_alive_for: None,
                            }),
                            Purpose::Update,
                        );
                    }
                }
            }
            Purpose::Update => {
                if let Ok(s) = serde_json::from_value(value) {
                    self.upsert(s);
                    self.configure = None;
                }
            }
            Purpose::Write | Purpose::ProtocolReply if value == Value::Bool(false) => {
                self.error = Some(
                    "Terminal input was not accepted; the remaining input was cancelled.".into(),
                );
                self.work
                    .retain(|w| !matches!(w.purpose, Purpose::Write | Purpose::ProtocolReply));
                self.pending_input.clear();
                self.pending_input_bytes = 0;
            }
            Purpose::Mutation => self.refresh(),
            _ => {}
        }
        if !self.replaying && !self.running {
            self.discard_pending_input();
        }
    }
    fn replace_rows(&mut self, v: Value) -> bool {
        match serde_json::from_value::<Vec<Session>>(v) {
            Ok(rows) => {
                self.sessions.replace(rows);
                self.pending_startup.retain(|id, _| {
                    self.sessions
                        .rows
                        .iter()
                        .any(|s| s.session_id == *id && s.running)
                });
                true
            }
            Err(e) => {
                self.error = Some(e.to_string());
                false
            }
        }
    }
    fn upsert(&mut self, s: Session) {
        if let Some(row) = self
            .sessions
            .rows
            .iter_mut()
            .find(|r| r.session_id == s.session_id)
        {
            *row = s;
        } else {
            self.sessions.rows.push(s);
        }
    }
    fn release_replay(&mut self) {
        if let Some(token) = self.replay_token.take() {
            self.enqueue(TerminalRequest::Release(token), Purpose::Cleanup);
        }
    }
    fn install_stream(&mut self, r: Attached, resume: bool) {
        self.upsert(r.summary);
        if r.replay.is_empty() || r.generation.is_empty() {
            self.error = Some("Terminal stream identity is missing".into());
            self.running = false;
            self.replaying = false;
            self.discard_pending_input();
            return;
        }
        let same = resume && self.stream_generation.as_deref() == Some(&r.generation);
        if !same {
            self.grid = TerminalGrid::new(self.grid.size, self.grid.palette.light);
            self.position = 0;
            self.target = 0;
        }
        self.release_replay();
        self.error = None;
        if self
            .stream_generation
            .as_ref()
            .is_some_and(|old| old != &r.generation)
        {
            self.error = Some("Terminal process changed.".into());
            self.discard_pending_input();
        }
        self.stream_generation = Some(r.generation);
        self.replay_token = Some(r.replay);
        // Reattach may follow an abandoned Create/Attach. Retain ownership
        // through partial replay, suppressing bytes already consumed. Protocol
        // replies are retained across navigation independently of user input.
        self.watermark = if let Some(start) = self
            .pending_startup
            .get(&r.session_id)
            .filter(|s| Some(&s.generation) == self.stream_generation.as_ref())
        {
            r.origin
                .filter(|o| *o <= r.watermark)
                .map(|o| o.max(start.through).min(r.watermark))
                .unwrap_or(r.watermark)
        } else {
            self.pending_startup.remove(&r.session_id);
            r.watermark
        };
        self.target = self.target.max(r.watermark);
        self.replaying = true;
        self.running = true;
        self.request_replay();
        self.detach_hidden();
    }
    fn request_replay(&mut self) {
        if self
            .work
            .iter()
            .any(|w| matches!(w.purpose, Purpose::Replay))
        {
            return;
        }
        if let Some(token) = self.replay_token.clone() {
            if !self.enqueue(
                TerminalRequest::Replay {
                    token,
                    position: self.position,
                },
                Purpose::Replay,
            ) {
                self.replaying = false;
                self.running = false;
                self.discard_pending_input();
                self.release_replay();
            }
        }
    }
    fn replay_chunk(&mut self, v: Value) {
        use base64::{Engine, engine::general_purpose::STANDARD};
        if v["generation"].as_str() != self.stream_generation.as_deref()
            || v["start"].as_u64() != Some(self.position)
        {
            self.error = Some("Terminal replay identity/position mismatch".into());
            self.replaying = false;
            self.discard_pending_input();
            self.release_replay();
            return;
        }
        let parsed = v["bytes"]
            .as_str()
            .filter(|s| s.len() <= 87384)
            .and_then(|s| STANDARD.decode(s).ok());
        let Some(bytes) = parsed.filter(|b| {
            b.len() <= 65536 && v["end"].as_u64() == Some(self.position + b.len() as u64)
        }) else {
            self.error = Some("Invalid terminal replay chunk".into());
            self.replaying = false;
            self.discard_pending_input();
            self.release_replay();
            return;
        };
        let historical = self
            .watermark
            .saturating_sub(self.position)
            .min(bytes.len() as u64) as usize;
        self.grid.feed(&bytes[..historical]);
        self.grid.discard_protocol_replies();
        self.grid.feed(&bytes[historical..]);
        // Data produced after the attach watermark is live, even when recovered from disk.
        let live_replies = self.grid.protocol_replies(self.metrics);

        self.position += bytes.len() as u64;
        self.target = self
            .target
            .max(v["watermark"].as_u64().unwrap_or(self.position));
        self.dirty = true;
        // One bounded reply batch has its own reserved admission slot. Keep it
        // across navigation: these bytes answer the exact process whose output
        // we just parsed, rather than the next view's user-input owner.
        let admitted = match live_replies {
            Ok(replies) => {
                let total = replies.iter().map(String::len).sum::<usize>();
                if total > 16384 {
                    self.error = Some("Terminal protocol reply buffer overflow".into());
                    false
                } else if total == 0 {
                    true
                } else if let (Some(id), Some(generation)) =
                    (self.active.clone(), self.stream_generation.clone())
                {
                    self.enqueue(
                        TerminalRequest::Input {
                            id,
                            generation,
                            bytes: replies.concat().into_bytes(),
                        },
                        Purpose::ProtocolReply,
                    )
                } else {
                    false
                }
            }
            Err(e) => {
                self.error = Some(e.into());
                false
            }
        };
        if !admitted {
            self.replaying = false;
            self.running = false;
            self.discard_pending_input();
            self.release_replay();
            return;
        }
        if let Some(id) = &self.active {
            if let Some(start) = self
                .pending_startup
                .get_mut(id)
                .filter(|s| Some(&s.generation) == self.stream_generation.as_ref())
            {
                start.through = self.position;
            }
            if self.position >= self.target {
                self.pending_startup.remove(id);
            }
        }
        if self.position < self.target {
            self.request_replay();
        } else {
            self.replaying = false;
            self.finish_exit();
        }
    }
    pub fn stream_data(
        &mut self,
        id: &str,
        generation: &str,
        start: u64,
        bytes: &[u8],
        durable: bool,
    ) {
        if self.disposed
            || !self.running
            || self.active.as_deref() != Some(id)
            || self.stream_generation.as_deref() != Some(generation)
        {
            return;
        }
        let end = start + bytes.len() as u64;
        self.target = self.target.max(end);
        if end <= self.position {
            return;
        }
        if self.replaying {
            return;
        } // durable cursor recovers without retaining live backlog
        if start > self.position {
            if durable {
                self.replaying = true;
                self.request_replay();
            } else {
                self.error = Some(
                    "Terminal output gap cannot be recovered: history persistence failed".into(),
                );
            }
            return;
        }
        self.grid.feed(&bytes[(self.position - start) as usize..]);
        self.position = end;
        self.dirty = true;
        self.protocol_replies();
        self.finish_exit();
    }
    pub fn stream_exit(&mut self, id: &str, generation: &str, position: u64, code: u32) {
        if self
            .pending_startup
            .get(id)
            .is_some_and(|s| s.generation == generation)
        {
            self.pending_startup.remove(id);
        }
        if self.disposed || self.active.as_deref() != Some(id) {
            return;
        }
        if self
            .stream_generation
            .as_deref()
            .is_some_and(|g| g != generation)
        {
            return;
        }
        self.final_exit = Some((generation.into(), position, code));
        self.target = self.target.max(position);
        self.finish_exit();
        self.refresh();
    }
    fn finish_exit(&mut self) {
        if let Some((generation, position, code)) = self.final_exit.clone() {
            if self.stream_generation.as_deref() == Some(&generation)
                && self.position >= position
                && !self.replaying
            {
                self.final_exit = None;
                self.show_exit(code);
                self.release_replay();
            }
        }
    }
    fn show_exit(&mut self, code: u32) {
        self.running = false;
        self.grid
            .feed(format!("\r\n\x1b[90m[process exited with code {code}]\x1b[0m\r\n").as_bytes());
        self.dirty = true;
    }
    fn protocol_replies(&mut self) {
        match self.grid.protocol_replies(self.metrics) {
            Ok(replies) => {
                for r in replies {
                    self.write(r, false);
                }
            }
            Err(e) => self.error = Some(e.into()),
        }
    }
    /// Positional reports are meaningful only for the current, fully replayed
    /// screen. They never consume the keyboard/IME FIFO's admission budget.
    pub fn write_pointer_bytes(&mut self, bytes: Vec<u8>) -> bool {
        if self.quit_preparing
            || self.disposed
            || !self.running
            || self.replaying
            || !self.pending_input.is_empty()
            || bytes.is_empty()
        {
            return false;
        }
        self.enqueue_input(bytes, false)
    }
    pub fn write_bytes(&mut self, bytes: Vec<u8>) {
        self.admit_input(bytes, false);
    }
    fn admit_input(&mut self, bytes: Vec<u8>, track: bool) {
        if self.quit_preparing || self.disposed || bytes.is_empty() {
            return;
        }
        if !self.replaying && !self.running {
            self.discard_pending_input();
            return;
        }
        if self.replaying || !self.pending_input.is_empty() {
            if self.pending_input.len() >= 60
                || self.pending_input_bytes.saturating_add(bytes.len()) > WRITE_BUDGET
            {
                self.error = Some(
                    "Terminal history is loading; input buffer is full. This input was not sent."
                        .into(),
                );
            } else {
                self.pending_input_bytes += bytes.len();
                self.pending_input.push_back((bytes, track));
            }
            return;
        }
        if !self.running {
            return;
        }
        self.enqueue_input(bytes, track);
    }
    fn enqueue_input(&mut self, bytes: Vec<u8>, track: bool) -> bool {
        let (Some(id), Some(generation)) = (self.active.clone(), self.stream_generation.clone())
        else {
            return false;
        };
        let tracked = track.then(|| String::from_utf8_lossy(&bytes).into_owned());
        if self.enqueue(
            TerminalRequest::Input {
                id,
                generation,
                bytes,
            },
            Purpose::Write,
        ) {
            if let Some(text) = tracked {
                self.track_input(&text);
            }
            return true;
        }
        false
    }
    fn discard_pending_input(&mut self) {
        if !self.pending_input.is_empty() {
            self.pending_input.clear();
            self.pending_input_bytes = 0;
            self.error = Some(format!(
                "{} Pending input was not sent.",
                self.error.as_deref().unwrap_or("Terminal unavailable.")
            ));
        }
    }
    fn flush_pending_input(&mut self) {
        if !self.replaying && !self.running {
            self.discard_pending_input();
        }
        if self.replaying || self.disposed || !self.running {
            return;
        }
        while let Some((bytes, _)) = self.pending_input.front() {
            let retained = self
                .work
                .iter()
                .map(|w| w.request.retained_bytes())
                .sum::<usize>();
            if self.work.len() >= WORK_LIMIT - 5 || retained + bytes.len() > WRITE_BUDGET {
                break;
            }
            let (bytes, track) = self.pending_input.pop_front().unwrap();
            self.pending_input_bytes -= bytes.len();
            self.enqueue_input(bytes, track);
        }
    }
    pub fn write(&mut self, data: String, track: bool) {
        if self.quit_preparing {
            return;
        }
        if data.len() > WRITE_BUDGET {
            self.error = Some("Terminal input exceeds 1 MiB.".into());
            return;
        }
        self.admit_input(data.into_bytes(), track);
    }
    fn track_input(&mut self, data: &str) {
        let Some(id) = self.active.clone() else {
            return;
        };
        self.grid.bottom();
        self.grid.clear_selection();
        self.dirty = true;
        if data == "\x03" {
            self.input.clear();
            return;
        }
        if data.starts_with('\x1b') {
            return;
        }
        for c in data.chars() {
            match c {
                '\r' | '\n' => {
                    let command = model::command_from_line(&self.grid.frame().cursor_line)
                        .unwrap_or_else(|| model::normalize_command(&self.input));
                    self.input.clear();
                    if !command.is_empty() {
                        self.cwd_revision += 1;
                        self.cwd_retries = Some((id.clone(), command, self.cwd_revision));
                    }
                }
                '\x7f' => {
                    self.input.pop();
                }
                c if c >= ' ' && self.input.len() < 4096 => self.input.push(c),
                _ => {}
            }
        }
    }
    pub fn refresh_cwd(&mut self, id: String, command: String, revision: u64) {
        if self.visible && self.active.as_deref() == Some(&id) && revision == self.cwd_revision {
            self.enqueue(
                TerminalRequest::Cwd(id.clone()),
                Purpose::Cwd {
                    id,
                    command,
                    revision,
                },
            );
        }
    }
    pub fn resize(&mut self, size: GridSize, metrics: CellMetrics) {
        self.metrics = metrics;
        if self.grid.size == size {
            return;
        }
        self.grid.resize(size);
        self.dirty = true;
        self.work.retain(|w| !matches!(w.purpose, Purpose::Resize));
        if let Some(id) = self.active.clone() {
            self.enqueue(
                TerminalRequest::Resize {
                    id,
                    cols: size.cols,
                    rows: size.rows,
                },
                Purpose::Resize,
            );
        }
    }
    pub fn configure(&mut self) {
        if let Some(s) = self
            .active
            .as_ref()
            .and_then(|id| self.sessions.rows.iter().find(|s| &s.session_id == id))
        {
            self.configure = Some(Configure {
                id: s.session_id.clone(),
                generation: self.generation,
                name: s.name.clone(),
                defaults: Defaults {
                    save_for: s.save_for.clone(),
                    keep_alive_for: s.keep_alive_for.clone(),
                },
            });
        }
    }
    pub fn save_configure(&mut self) {
        if let Some(c) = self
            .configure
            .clone()
            .filter(|c| c.generation == self.generation && self.active.as_ref() == Some(&c.id))
        {
            self.enqueue(
                TerminalRequest::Update(Update {
                    session_id: c.id,
                    name: Some(c.name),
                    cwd: None,
                    last_command: None,
                    save_for: Some(c.defaults.save_for),
                    keep_alive_for: Some(c.defaults.keep_alive_for),
                }),
                Purpose::Update,
            );
        }
    }
    pub fn stop(&mut self) {
        if let Some(id) = self.active.clone() {
            self.enqueue(TerminalRequest::Kill(id), Purpose::Mutation);
        }
    }
    pub fn delete(&mut self) {
        if let Some(id) = self.active.clone() {
            self.generation += 1;
            self.cancel_queued();
            self.detach_active();
            self.emit(TerminalEffect::Unpin(id.clone()));
            self.enqueue(TerminalRequest::Delete(id), Purpose::Mutation);
        }
    }
    pub fn pin(&mut self) {
        if let Some(id) = self.active.clone() {
            self.emit(TerminalEffect::TogglePin(id));
        }
    }
    pub fn emit(&mut self, e: TerminalEffect) {
        if self.effects.len() < 64 {
            self.effects.push_back((self.generation, e));
        } else {
            self.error = Some("Terminal actions are still draining.".into());
        }
    }
    pub fn handle_key(&mut self, k: input::KeyResult) {
        use input::KeyResult::*;
        match k {
            Bytes(b) => self.write(b, true),
            Back => {
                if self.configure.take().is_none() {
                    self.emit(TerminalEffect::Back { sentinel: ">" });
                }
            }
            Configure => self.configure(),
            Stop => self.stop(),
            Pin => self.pin(),
            Delete => self.delete(),
            Copy => {
                if let Some(s) = self.grid.selected_text() {
                    self.emit(TerminalEffect::Copy(s));
                }
            }
            Paste => self.emit(TerminalEffect::ReadPaste {
                generation: self.generation,
            }),
            TextZoom(z) => {
                self.font_size = input::font_size(self.font_size, z);
                self.dirty = true;
            }
            UiZoom(z) => self.emit(TerminalEffect::Zoom(z)),
            Session(i) => {
                if let Some(id) = self.sessions.rows.get(i).map(|s| s.session_id.clone()) {
                    self.select(&id);
                }
            }
            _ => {}
        }
    }
    pub fn paste(&mut self, generation: u64, text: &str) {
        if self.visible && generation == self.generation {
            self.write(input::paste(text, self.grid.mode()), true);
        }
    }
    pub fn pins(&mut self, pins: Vec<String>) {
        self.sessions.pins = pins.into_iter().collect::<BTreeSet<_>>();
    }
    pub fn snapshot_summary(&self) -> Value {
        json!({"sessionId":self.active,"running":self.running,"rows":self.sessions.rows.len()})
    }
}
