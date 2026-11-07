//! Explicit session/order owner. Parsing is exclusively alacritty_terminal in terminal_grid.
use super::{
    input,
    request::{Attached, Create, Reply, TerminalRequest, Update},
};
use serde_json::{Value, json};
use std::collections::{BTreeSet, VecDeque};
use tezbar_ui::terminal_grid::{CellMetrics, GridSize, TerminalGrid};
use tezbar_ui_model::terminal::{self as model, Defaults, Session};

const WRITE_BUDGET: usize = 1024 * 1024;
const WORK_LIMIT: usize = 64;
#[derive(Clone, Debug, PartialEq)]
pub enum TerminalEffect {
    Back { sentinel: &'static str },
    OpenSettings,
    SessionsToggle,
    OpenUrl(String),
    Copy(String),
    ReadPaste { generation: u64 },
    Zoom(input::Zoom),
    Pins(Vec<String>),
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
        self.work.retain(Work::cleanup);
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
            } else {
                WORK_LIMIT - 4
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
        while let Some(w) = self.work.pop_front() {
            if w.cleanup() || w.generation == self.generation {
                return Some(w);
            }
        }
        None
    }
    fn detach_request(&mut self, id: String, generation: Option<String>) {
        if let Some(generation) = generation {
            self.enqueue(
                TerminalRequest::DetachGeneration { id, generation },
                Purpose::Cleanup,
            );
        }
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
                self.enqueue(
                    TerminalRequest::Attach {
                        id: id.clone(),
                        cols: self.grid.size.cols,
                        rows: self.grid.size.rows,
                    },
                    Purpose::Resume(id),
                );
            }
        }
    }
    fn detach_hidden(&mut self) {
        if !self.visible && !self.visibility_detached {
            if let Some(id) = self.active.clone() {
                self.visibility_detached = true;
                self.detach_request(id, self.stream_generation.clone());
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
            self.replaying = true;
            self.enqueue(
                TerminalRequest::Create(Create {
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
                }),
                Purpose::Create,
            );
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
        self.replaying = true;
        self.enqueue(TerminalRequest::Create(r), Purpose::Create);
    }
    fn attach(&mut self, id: String) {
        self.active = Some(id.clone());
        self.replaying = true;
        self.enqueue(
            TerminalRequest::Attach {
                id: id.clone(),
                cols: self.grid.size.cols,
                rows: self.grid.size.rows,
            },
            Purpose::Attach(id),
        );
    }
    pub fn refresh(&mut self) {
        if !self.disposed && !self.work.iter().any(|w| matches!(w.purpose, Purpose::List)) {
            self.enqueue(TerminalRequest::List, Purpose::List);
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
                if matches!(w.purpose, Purpose::Replay) {
                    self.release_replay();
                    if e.contains("expired or released") {
                        if let Some(id) = self.active.clone() {
                            self.enqueue(
                                TerminalRequest::Attach {
                                    id: id.clone(),
                                    cols: self.grid.size.cols,
                                    rows: self.grid.size.rows,
                                },
                                Purpose::Resume(id),
                            );
                        }
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
                    self.replaying = false;
                }
            },
            Purpose::Attach(id) => {
                if value.is_null() {
                    self.active = None;
                    self.replaying = false;
                    self.refresh();
                    return;
                }
                match serde_json::from_value::<Attached>(value) {
                    Ok(r) if r.session_id == id && self.active.as_deref() == Some(&id) => {
                        self.install_stream(r, false);
                    }
                    _ => {
                        self.error = Some("Terminal attach returned a different session.".into());
                        self.replaying = false;
                    }
                }
            }
            Purpose::Resume(id) => {
                if value.is_null() {
                    self.running = false;
                    self.active = None;
                    self.refresh();
                } else if value["sessionId"].as_str() == Some(&id)
                    && self.active.as_deref() == Some(&id)
                {
                    match serde_json::from_value::<Attached>(value) {
                        Ok(r) => self.install_stream(r, true),
                        Err(e) => self.error = Some(e.to_string()),
                    }
                } else {
                    self.error = Some("Terminal resume returned a different session.".into());
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
            Purpose::Write if value == Value::Bool(false) => {
                self.error = Some(
                    "Terminal input was not accepted; the remaining input was cancelled.".into(),
                );
                self.work.retain(|w| !matches!(w.purpose, Purpose::Write));
            }
            Purpose::Mutation => self.refresh(),
            _ => {}
        }
    }
    fn replace_rows(&mut self, v: Value) -> bool {
        match serde_json::from_value::<Vec<Session>>(v) {
            Ok(rows) => {
                self.sessions.replace(rows);
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
            self.replaying = false;
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
        self.stream_generation = Some(r.generation);
        self.replay_token = Some(r.replay);
        self.watermark = r.watermark;
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
            self.enqueue(
                TerminalRequest::Replay {
                    token,
                    position: self.position,
                },
                Purpose::Replay,
            );
        }
    }
    fn replay_chunk(&mut self, v: Value) {
        use base64::{Engine, engine::general_purpose::STANDARD};
        if v["generation"].as_str() != self.stream_generation.as_deref()
            || v["start"].as_u64() != Some(self.position)
        {
            self.error = Some("Terminal replay identity/position mismatch".into());
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
        if let Ok(replies) = live_replies {
            for reply in replies {
                if let (Some(id), Some(generation)) =
                    (self.active.clone(), self.stream_generation.clone())
                {
                    self.enqueue(
                        TerminalRequest::Input {
                            id,
                            generation,
                            bytes: reply.into_bytes(),
                        },
                        Purpose::Write,
                    );
                }
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
    pub fn write_bytes(&mut self, bytes: Vec<u8>) {
        if self.quit_preparing {
            return;
        }
        if self.disposed || self.replaying || !self.running || bytes.is_empty() {
            return;
        }
        let (Some(id), Some(generation)) = (self.active.clone(), self.stream_generation.clone())
        else {
            return;
        };
        // One request is one atomic input admission, including both paste delimiters.
        self.enqueue(
            TerminalRequest::Input {
                id,
                generation,
                bytes,
            },
            Purpose::Write,
        );
    }
    pub fn write(&mut self, data: String, track: bool) {
        if self.quit_preparing {
            return;
        }
        if self.disposed || self.replaying || !self.running {
            return;
        }
        let Some(id) = self.active.clone() else {
            return;
        };
        if data.len() > WRITE_BUDGET {
            self.error = Some("Terminal input exceeds 1 MiB.".into());
            return;
        }
        if data.is_empty() {
            return;
        }
        self.write_bytes(data.as_bytes().to_vec());
        if !track {
            return;
        }
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
            self.sessions.pins.remove(&id);
            self.emit(TerminalEffect::Pins(
                self.sessions.pins.iter().cloned().collect(),
            ));
            self.enqueue(TerminalRequest::Delete(id), Purpose::Mutation);
        }
    }
    pub fn pin(&mut self) {
        if let Some(id) = self.active.clone() {
            let model::Effect::PersistPins(pins) = self.sessions.toggle_pin(id) else {
                unreachable!()
            };
            self.emit(TerminalEffect::Pins(pins));
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
