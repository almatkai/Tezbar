//! Event owner used by GPUI. Pending previews never authorize activation; one-unit Basic local results do.
use super::*;
#[derive(Clone, Debug)]
pub enum Input {
    Edit(String),
    Composition(bool),
    Tick,
    SearchComplete {
        generation: u64,
        result: Result<Vec<Row>, String>,
    },
    Enter {
        repeat: bool,
    },
    Click {
        generation: u64,
        id: String,
    },
    /// Explicit user gesture over a canonical saved pin, independent of search previews.
    ActivatePin {
        row: Row,
        rank: usize,
    },
    Move(isize),
    ActivationComplete {
        ticket: u64,
        result: Result<(), String>,
    },
    /// A local route completed while the current launcher session remains open.
    ActivationHandled {
        ticket: u64,
    },
    Hide,
    Show,
    Invalidate {
        text: Option<String>,
    },
    ArgumentValue(Option<String>),
    DirectoryResolved {
        ticket: u64,
        result: Result<String, String>,
    },
    SelectTerminalSession(Option<String>),
    BackendChanged,
    TerminalMode(bool),
}
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Search {
        generation: u64,
        query: String,
    },
    CompletePath {
        generation: u64,
        query: String,
    },
    CompleteApplications {
        generation: u64,
        query: String,
    },
    SearchChats {
        generation: u64,
        query: String,
    },
    Schedule {
        at_ms: u64,
    },
    Execute {
        ticket: u64,
        generation: u64,
        row_id: String,
        action: Action,
        query: String,
        rank: usize,
    },
    ResolveDirectory {
        ticket: u64,
        path: String,
    },
    OpenAiChat {
        ticket: u64,
        boot: Option<AiBoot>,
    },
    OpenTerminal {
        ticket: u64,
        command: Option<String>,
        session_id: Option<String>,
    },
    OpenNote {
        ticket: u64,
        note_id: u64,
    },
    Hide,
    RecordQuery(String),
}
#[derive(Debug, Default)]
pub struct Launcher {
    pub session: SearchSession<Arc<Row>>,
    pub cache: ResultCache,
    // AI history and ordinary home can both request "". They must never share
    // speculative rows or candidate lists; only current session rows authorize clicks.
    ai_cache: ResultCache,
    pub error: Option<String>,
    pub visible: bool,
    pub composing: bool,
    pub input: String,
    pub terminal_mode: bool,
    pub list_selection_active: bool,
    pub selected_terminal_session: Option<String>,
    argument_value: Option<String>,
    resolving: Option<(u64, InputMode)>,
    dirty: bool,
    composition_draft: Option<String>,
    ticket: u64,
    activation: Option<(u64, u64)>,
}
impl Launcher {
    pub fn display_rows(&self) -> Vec<Arc<Row>> {
        if self.session.pending() {
            let cache = if input_mode(&self.input, self.terminal_mode).mode == Mode::Ai {
                &self.ai_cache
            } else {
                &self.cache
            };
            cache.preview(&request_input(&self.session.query))
        } else {
            self.session.rows.clone()
        }
    }
    fn retire_activation(&mut self) {
        self.activation = None;
        self.resolving = None;
        self.session.activating = false;
    }
    fn reset_hidden(&mut self) {
        self.visible = false;
        self.composing = false;
        self.composition_draft = None;
        self.input.clear();
        self.terminal_mode = false;
        self.list_selection_active = false;
        self.selected_terminal_session = None;
        self.argument_value = None;
        self.retire_activation();
        self.session.reset_transport();
        self.session.hide();
        self.dirty = true;
    }
    fn begin_submit(&mut self) -> u64 {
        self.ticket += 1;
        self.session.activating = true;
        self.activation = Some((self.ticket, self.session.generation));
        self.ticket
    }
    fn submit(&mut self, mode: InputMode) -> Vec<Effect> {
        let ticket = self.begin_submit();
        if mode.mode == Mode::Terminal {
            return vec![Effect::OpenTerminal {
                ticket,
                command: if self.selected_terminal_session.is_some() {
                    None
                } else {
                    let command = self.input.trim().trim_start_matches('>').trim();
                    (!command.is_empty()).then(|| command.to_owned())
                },
                session_id: self.selected_terminal_session.clone(),
            }];
        }
        if let Some(path) = mode.ai_working_directory.clone() {
            self.resolving = Some((ticket, mode));
            vec![Effect::ResolveDirectory { ticket, path }]
        } else {
            let mut out = vec![Effect::OpenAiChat {
                ticket,
                boot: ai_boot(&mode, None),
            }];
            if let Some(q) = history_entry(&self.input, false) {
                out.push(Effect::RecordQuery(q));
            }
            out
        }
    }
    fn activation_effects(&mut self) -> Vec<Effect> {
        let Some(row) = self.session.rows.get(self.session.selected).cloned() else {
            return vec![];
        };
        self.activation_for_row(row, self.session.selected)
    }
    fn activation_for_row(&mut self, row: Arc<Row>, rank: usize) -> Vec<Effect> {
        self.ticket += 1;
        self.activation = Some((self.ticket, self.session.generation));
        let mut out = if row.category == "quick-notes" {
            if let Some(id) = row.id.strip_prefix("note:").and_then(|id| id.parse().ok()) {
                vec![Effect::OpenNote {
                    ticket: self.ticket,
                    note_id: id,
                }]
            } else {
                self.retire_activation();
                self.error = Some("Invalid note identity".into());
                return vec![];
            }
        } else {
            vec![Effect::Execute {
                ticket: self.ticket,
                generation: self.session.generation,
                row_id: row.id.clone(),
                action: row.action.clone(),
                query: self.session.query.clone(),
                rank,
            }]
        };
        if let Some(q) = history_entry(&self.input, self.terminal_mode) {
            out.push(Effect::RecordQuery(q));
        }
        out
    }
    fn drain(&mut self, now: u64, out: &mut Vec<Effect>) {
        if let Some((generation, query)) = self.session.take_request(now) {
            let mode = input_mode(&self.input, self.terminal_mode);
            match mode.mode {
                Mode::Ai => out.push(Effect::SearchChats {
                    generation,
                    query: mode.ai_task,
                }),
                Mode::Path => out.push(Effect::CompletePath { generation, query }),
                Mode::Applications => out.push(Effect::CompleteApplications { generation, query }),
                Mode::Terminal => {
                    self.session.fail(generation, now);
                }
                Mode::Basic if query.encode_utf16().count() == 1 => {
                    // This synchronous, current-generation local result is activation authority.
                    let rows = self.cache.preview(&query);
                    self.session.complete(generation, rows, now);
                }
                _ => out.push(Effect::Search { generation, query }),
            }
        } else if let Some(delay) = self.session.request_delay_ms(now) {
            out.push(Effect::Schedule {
                at_ms: now.saturating_add(delay),
            });
        }
    }
    pub fn update(&mut self, input: Input, now: u64) -> Vec<Effect> {
        let mut out = vec![];
        match input {
            Input::Composition(active) => {
                self.composing = active;
                if active {
                    self.retire_activation();
                    self.session.cancel_enter();
                }
                if !active {
                    if let Some(s) = self.composition_draft.take() {
                        self.input = s.clone();
                        self.session.commit(request_input(&s), now);
                        self.drain(now, &mut out);
                    }
                }
            }
            Input::Edit(s) => {
                self.error = None;
                self.list_selection_active = false;
                self.selected_terminal_session = None;
                if self.composing {
                    self.composition_draft = Some(s);
                } else {
                    let q = request_input(&s);
                    let changed_mode = input_mode(&s, self.terminal_mode).mode
                        != input_mode(&self.input, self.terminal_mode).mode;
                    self.input = s;
                    if q != self.session.query || changed_mode {
                        self.retire_activation();
                        self.session.commit(q, now);
                        self.drain(now, &mut out);
                    }
                }
            }
            Input::Tick => {
                self.session.expire_enter(now);
                self.drain(now, &mut out);
                if self.session.deferred_enter(now) {
                    out.extend(self.activation_effects());
                }
            }
            Input::SearchComplete { generation, result } => {
                let success = result.is_ok();
                let accepted = match result {
                    Ok(rows) => {
                        let rows: Vec<_> = rows
                            .into_iter()
                            .filter(|r| r.score.is_finite())
                            .take(1000)
                            .map(Arc::new)
                            .collect();
                        if generation == self.session.generation {
                            let cache =
                                if input_mode(&self.input, self.terminal_mode).mode == Mode::Ai {
                                    &mut self.ai_cache
                                } else {
                                    &mut self.cache
                                };
                            cache.accept(&self.session.query, rows.clone());
                        }
                        self.session.complete(generation, rows, now)
                    }
                    Err(e) => {
                        if generation == self.session.generation {
                            self.error = Some(e);
                        }
                        self.session.fail(generation, now)
                    }
                };
                if accepted {
                    self.list_selection_active = false;
                }
                if accepted && success && self.session.deferred_enter(now) {
                    out.extend(self.activation_effects());
                }
                self.drain(now, &mut out);
            }
            Input::Enter { repeat } => {
                let mode = input_mode(&self.input, self.terminal_mode);
                if !repeat
                    && !self.composing
                    && !self.session.activating
                    && (mode.mode == Mode::Terminal
                        || (mode.mode == Mode::Ai && !self.list_selection_active))
                {
                    out.extend(self.submit(mode));
                } else if !self.composing && self.session.enter(now, repeat) {
                    out.extend(self.activation_effects());
                } else if self.session.pending() && !repeat && !self.composing {
                    out.push(Effect::Schedule {
                        at_ms: now.saturating_add(1000),
                    });
                }
            }
            Input::Click { generation, id } => {
                if generation == self.session.generation && !self.composing {
                    if let Some(index) = self.session.rows.iter().position(|r| r.id == id) {
                        if self.session.click(index) {
                            self.list_selection_active = true;
                            out.extend(self.activation_effects());
                        }
                    }
                }
            }
            Input::ActivatePin { row, rank } => {
                if self.visible
                    && !self.composing
                    && !self.session.activating
                    && !row.ephemeral
                    && row.pinnable()
                {
                    self.session.cancel_enter();
                    self.session.activating = true;
                    out.extend(self.activation_for_row(Arc::new(row), rank));
                }
            }
            Input::Move(delta) => {
                if !self.composing && !self.session.pending() && !self.session.rows.is_empty() {
                    self.list_selection_active = true;
                    self.session.move_selection(delta);
                }
            }
            Input::ArgumentValue(value) => self.argument_value = value,
            Input::SelectTerminalSession(id) => {
                self.selected_terminal_session = id;
                self.list_selection_active = self.selected_terminal_session.is_some();
            }
            Input::DirectoryResolved { ticket, result } => {
                if self
                    .resolving
                    .as_ref()
                    .is_some_and(|(owner, _)| *owner == ticket)
                    && self.activation == Some((ticket, self.session.generation))
                {
                    let (_, mode) = self.resolving.take().unwrap();
                    match result {
                        Ok(path) if !path.is_empty() => {
                            out.push(Effect::OpenAiChat {
                                ticket,
                                boot: ai_boot(&mode, Some(path)),
                            });
                            if let Some(q) = history_entry(&self.input, false) {
                                out.push(Effect::RecordQuery(q));
                            }
                        }
                        result => {
                            self.retire_activation();
                            self.error = Some(result.err().unwrap_or_else(|| {
                                "Directory resolution returned an empty path".into()
                            }));
                        }
                    }
                }
            }
            Input::ActivationHandled { ticket } => {
                if let Some((owned, generation)) = self.activation {
                    if owned == ticket {
                        self.activation = None;
                        self.session.activation_finished(generation);
                    }
                }
            }
            Input::ActivationComplete { ticket, result } => {
                if let Some((owned, generation)) = self.activation {
                    if owned == ticket {
                        self.activation = None;
                        let current = self.session.activation_finished(generation);
                        if current {
                            match result {
                                Ok(()) => {
                                    self.reset_hidden();
                                    out.push(Effect::Hide);
                                }
                                Err(e) => self.error = Some(e),
                            }
                        }
                    }
                }
            }
            Input::Hide => self.reset_hidden(),
            Input::Show => {
                if !self.visible || self.dirty {
                    self.visible = true;
                    self.dirty = false;
                    self.session.commit(request_input(&self.input), now);
                    self.drain(now, &mut out);
                }
            }
            Input::Invalidate { text } => {
                if input_mode(&self.input, self.terminal_mode).mode != Mode::Deep
                    && text.as_ref().is_none_or(|t| {
                        *t == request_input(&self.input) || self.argument_value.as_ref() == Some(t)
                    })
                {
                    self.list_selection_active = false;
                    self.dirty = true;
                    if self.visible {
                        self.dirty = false;
                        self.session.commit(request_input(&self.input), now);
                        self.drain(now, &mut out);
                    }
                }
            }
            Input::BackendChanged => {
                self.list_selection_active = false;
                self.retire_activation();
                self.session.reset_transport();
                self.dirty = true;
                if self.visible {
                    self.dirty = false;
                    self.session.commit(request_input(&self.input), now);
                    self.drain(now, &mut out);
                }
            }
            Input::TerminalMode(active) => {
                self.reset_hidden();
                self.visible = true;
                self.terminal_mode = active;
                self.input = if active { ">".into() } else { String::new() };
            }
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_pin_has_its_own_activation_authority_and_uses_normal_tickets() {
        let mut launcher = Launcher::default();
        launcher.update(Input::Show, 0);
        let row = Row::command("saved", "Saved command");
        assert!(launcher
            .update(
                Input::Click {
                    generation: launcher.session.generation,
                    id: row.id.clone()
                },
                1
            )
            .is_empty());
        let first = launcher.update(
            Input::ActivatePin {
                row: row.clone(),
                rank: 3,
            },
            2,
        );
        let Effect::Execute {
            ticket,
            row_id,
            rank,
            ..
        } = &first[0]
        else {
            panic!("pin must use normal activation")
        };
        let first_ticket = *ticket;
        assert_eq!(row_id, &row.id);
        assert_eq!(*rank, 3);
        assert!(launcher.session.rows.is_empty());
        assert!(launcher
            .update(
                Input::ActivatePin {
                    row: row.clone(),
                    rank: 3
                },
                3
            )
            .is_empty());
        launcher.update(
            Input::ActivationComplete {
                ticket: first_ticket,
                result: Err("fixture failure".into()),
            },
            4,
        );
        let second = launcher.update(
            Input::ActivatePin {
                row: row.clone(),
                rank: 3,
            },
            5,
        );
        let Effect::Execute {
            ticket: second_ticket,
            ..
        } = second[0]
        else {
            panic!()
        };
        launcher.update(
            Input::ActivationComplete {
                ticket: first_ticket,
                result: Ok(()),
            },
            6,
        );
        assert!(launcher.session.activating);
        assert!(launcher.visible);
        launcher.update(
            Input::ActivationComplete {
                ticket: second_ticket,
                result: Ok(()),
            },
            7,
        );
        assert!(!launcher.visible);
        assert!(launcher
            .update(Input::ActivatePin { row, rank: 3 }, 8)
            .is_empty());
    }
    #[test]
    fn saved_pin_respects_composition_and_keeps_typed_note_identity() {
        let mut launcher = Launcher::default();
        launcher.update(Input::Show, 0);
        let mut note = Row::command("open-notes", "Saved note");
        note.category = "quick-notes".into();
        note.id = "note:42".into();
        launcher.update(Input::Composition(true), 1);
        assert!(launcher
            .update(
                Input::ActivatePin {
                    row: note.clone(),
                    rank: 0
                },
                2
            )
            .is_empty());
        launcher.update(Input::Composition(false), 3);
        assert!(matches!(
            launcher
                .update(Input::ActivatePin { row: note, rank: 0 }, 4)
                .first(),
            Some(Effect::OpenNote { note_id: 42, .. })
        ));
    }
    #[test]
    fn ime_search_errors_and_activation_owner() {
        let mut l = Launcher::default();
        l.update(Input::Composition(true), 0);
        assert!(l.update(Input::Edit("雪山".into()), 1).is_empty());
        assert!(l.update(Input::Tick, 100).is_empty());
        l.update(Input::Composition(false), 101);
        assert_eq!(
            l.update(Input::Tick, 109),
            vec![Effect::Search {
                generation: 1,
                query: "雪山".into()
            }]
        );
        l.update(Input::Enter { repeat: false }, 110);
        let e = l.update(
            Input::SearchComplete {
                generation: 1,
                result: Ok(vec![Row::command("x", "X")]),
            },
            111,
        );
        let Effect::Execute { ticket, .. } = e[0] else {
            panic!()
        };
        assert!(l.update(Input::Enter { repeat: true }, 112).is_empty());
        l.update(Input::Hide, 113);
        l.update(Input::Edit("new".into()), 114);
        l.update(
            Input::ActivationComplete {
                ticket,
                result: Ok(()),
            },
            115,
        );
        assert_eq!(l.session.query, "new");
        assert!(!l.session.activating);
    }
    #[test]
    fn stale_generation_and_click_never_execute_preview() {
        let mut l = Launcher::default();
        l.update(Input::Edit("a".into()), 0);
        l.update(Input::Tick, 8);
        l.update(Input::Edit("b".into()), 9);
        l.update(
            Input::SearchComplete {
                generation: 1,
                result: Ok(vec![Row::command("old", "old")]),
            },
            10,
        );
        assert!(l
            .update(
                Input::Click {
                    generation: 1,
                    id: "command:old".into()
                },
                11
            )
            .is_empty());
        assert!(l.session.rows.is_empty());
    }
}
#[cfg(test)]
mod mode_tests {
    use super::*;
    #[test]
    fn handled_local_activation_preserves_live_query_and_rejects_stale_ticket() {
        let mut launcher = Launcher::default();
        launcher.update(Input::Show, 0);
        launcher.update(Input::Edit("updates".into()), 1);
        launcher.update(Input::Tick, 9);
        launcher.update(
            Input::SearchComplete {
                generation: launcher.session.generation,
                result: Ok(vec![Row::command("check-for-updates", "Updates")]),
            },
            10,
        );
        let first = launcher.update(Input::Enter { repeat: false }, 11);
        let ticket = first
            .iter()
            .find_map(|effect| match effect {
                Effect::Execute { ticket, .. } => Some(*ticket),
                _ => None,
            })
            .unwrap();
        launcher.update(Input::ActivationHandled { ticket: ticket + 1 }, 12);
        assert!(launcher.session.activating);
        assert!(launcher
            .update(Input::ActivationHandled { ticket }, 13)
            .is_empty());
        assert!(launcher.visible);
        assert_eq!(launcher.input, "updates");
        assert!(!launcher.session.pending());
        assert!(!launcher.session.activating);
        assert_eq!(launcher.session.rows.len(), 1);
        assert!(launcher
            .update(Input::Enter { repeat: false }, 14)
            .iter()
            .any(|effect| matches!(effect,Effect::Execute{ticket:next,..} if *next>ticket)));
    }
    #[test]
    fn ai_history_cache_is_separate_from_home_and_pending_previews_have_no_click_authority() {
        let mut launcher = Launcher::default();
        launcher.update(Input::Show, 0);
        launcher.update(Input::Tick, 8);
        launcher.update(
            Input::SearchComplete {
                generation: launcher.session.generation,
                result: Ok(vec![Row::command("home", "Home")]),
            },
            9,
        );
        launcher.update(Input::Edit(" ".into()), 10);
        launcher.update(Input::Tick, 18);
        launcher.update(
            Input::SearchComplete {
                generation: launcher.session.generation,
                result: Ok(vec![Row::command("history", "History")]),
            },
            19,
        );
        assert_eq!(launcher.cache.preview("")[0].id, "command:home");
        assert!(launcher
            .cache
            .preview("h")
            .iter()
            .all(|row| row.id != "command:history"));
        launcher.update(Input::Hide, 20);
        launcher.update(Input::Show, 21);
        launcher.update(Input::Tick, 29);
        assert!(launcher.session.pending());
        assert_eq!(launcher.display_rows()[0].id, "command:home");
        launcher.update(Input::Edit(" ".into()), 30);
        launcher.update(Input::Tick, 38);
        assert_eq!(launcher.display_rows()[0].id, "command:history");
        assert!(launcher
            .update(
                Input::Click {
                    generation: launcher.session.generation,
                    id: "command:history".into()
                },
                39
            )
            .is_empty());
    }
    #[test]
    fn explicit_saved_pin_retires_deferred_enter_before_search_reply() {
        let mut launcher = Launcher::default();
        launcher.update(Input::Show, 0);
        launcher.update(Input::Edit("slow".into()), 1);
        launcher.update(Input::Tick, 9);
        launcher.update(Input::Enter { repeat: false }, 10);
        let effects = launcher.update(
            Input::ActivatePin {
                row: Row::command("pin", "Pin"),
                rank: 0,
            },
            11,
        );
        let pin_ticket = effects
            .iter()
            .find_map(|effect| match effect {
                Effect::Execute { ticket, .. } => Some(*ticket),
                _ => None,
            })
            .unwrap();
        let effects = launcher.update(
            Input::SearchComplete {
                generation: launcher.session.generation,
                result: Ok(vec![Row::command("late", "Late")]),
            },
            12,
        );
        assert!(!effects
            .iter()
            .any(|effect| matches!(effect, Effect::Execute { .. })));
        assert_eq!(
            launcher.activation.map(|(ticket, _)| ticket),
            Some(pin_ticket)
        );
        assert_eq!(launcher.input, "slow");
    }
    #[test]
    fn rejected_saved_pin_preserves_deferred_enter() {
        let mut launcher = Launcher::default();
        launcher.update(Input::Show, 0);
        launcher.update(Input::Edit("slow".into()), 1);
        launcher.update(Input::Tick, 9);
        launcher.update(Input::Enter { repeat: false }, 10);
        let mut pin = Row::command("invalid", "Invalid");
        pin.ephemeral = true;
        assert!(launcher
            .update(Input::ActivatePin { row: pin, rank: 0 }, 11)
            .is_empty());
        let effects = launcher.update(
            Input::SearchComplete {
                generation: launcher.session.generation,
                result: Ok(vec![Row::command("late", "Late")]),
            },
            12,
        );
        assert!(effects.iter().any(
            |effect| matches!(effect, Effect::Execute {row_id,..} if row_id == "command:late")
        ));
    }
    #[test]
    fn mode_switch_cancels_normal_ownership_and_respects_ai_sentinel() {
        let mut l = Launcher::default();
        l.update(Input::Edit("pending".into()), 0);
        l.update(Input::Edit(" explain".into()), 1);
        assert_eq!(
            l.update(Input::Tick, 8),
            vec![Effect::SearchChats {
                generation: 2,
                query: "explain".into()
            }]
        );
        assert_eq!(history_entry(&l.input, false), Some(" explain".into()));
    }
    #[test]
    fn hide_interrupts_ime_and_backend_restart_does_not_block() {
        let mut l = Launcher::default();
        l.update(Input::Edit("old".into()), 0);
        l.update(Input::Tick, 8);
        l.update(Input::Composition(true), 9);
        l.update(Input::Edit("draft".into()), 10);
        l.update(Input::Hide, 11);
        assert!(!l.composing);
        assert!(l.update(Input::BackendChanged, 12).is_empty());
        l.update(Input::Show, 12);
        assert!(
            matches!(l.update(Input::Tick,20).as_slice(),[Effect::Search{query,..}]if query.is_empty())
        );
    }
}

#[cfg(test)]
mod opus_repairs {
    use super::*;
    fn result(l: &mut Launcher, rows: Vec<Row>, now: u64) {
        let generation = l.session.generation;
        l.update(
            Input::SearchComplete {
                generation,
                result: Ok(rows),
            },
            now,
        );
    }
    fn ticket(e: &[Effect]) -> u64 {
        match e[0] {
            Effect::Execute { ticket, .. }
            | Effect::OpenAiChat { ticket, .. }
            | Effect::OpenTerminal { ticket, .. }
            | Effect::ResolveDirectory { ticket, .. } => ticket,
            _ => panic!("{e:?}"),
        }
    }
    #[test]
    fn opus_h1_ai_enter_submits_until_history_deliberately_selected() {
        let mut l = Launcher::default();
        l.update(Input::Edit(" summarize my day".into()), 0);
        l.update(Input::Tick, 8);
        result(&mut l, vec![Row::command("resume", "Old chat")], 9);
        let e = l.update(Input::Enter { repeat: false }, 10);
        assert!(
            matches!(&e[0],Effect::OpenAiChat{boot:Some(AiBoot::Submit{prompt,..}),..}if prompt=="summarize my day")
        );
        assert!(l.update(Input::Enter { repeat: true }, 11).is_empty());
        assert!(l.update(Input::Enter { repeat: false }, 11).is_empty());
        l.update(
            Input::ActivationComplete {
                ticket: ticket(&e),
                result: Err("failed".into()),
            },
            12,
        );
        l.update(Input::Move(1), 13);
        assert!(l.list_selection_active);
        assert!(
            matches!(&l.update(Input::Enter{repeat:false},14)[0],Effect::Execute{row_id,..}if row_id=="command:resume")
        );
    }
    #[test]
    fn opus_h1_directory_resolution_is_once_owned_and_error_preserves_draft() {
        let mut l = Launcher::default();
        l.update(Input::Edit("/work  task".into()), 0);
        let e = l.update(Input::Enter { repeat: false }, 1);
        let old = ticket(&e);
        l.update(Input::Edit("/else  ".into()), 2);
        assert!(l
            .update(
                Input::DirectoryResolved {
                    ticket: old,
                    result: Ok("/work".into())
                },
                3
            )
            .is_empty());
        let e = l.update(Input::Enter { repeat: false }, 4);
        let t = ticket(&e);
        l.update(
            Input::DirectoryResolved {
                ticket: t,
                result: Err("Not a directory".into()),
            },
            5,
        );
        assert_eq!(l.error.as_deref(), Some("Not a directory"));
        assert_eq!(l.input, "/else  ");
        let e = l.update(Input::Enter { repeat: false }, 6);
        let t = ticket(&e);
        let e = l.update(
            Input::DirectoryResolved {
                ticket: t,
                result: Ok("/resolved".into()),
            },
            7,
        );
        assert!(
            matches!(&e[0],Effect::OpenAiChat{boot:Some(AiBoot::NewChat{working_directory}),..}if working_directory=="/resolved")
        );
        assert!(l
            .update(
                Input::DirectoryResolved {
                    ticket: t,
                    result: Ok("/again".into())
                },
                8
            )
            .is_empty());
        l.update(Input::Hide, 9);
        assert!(l
            .update(
                Input::ActivationComplete {
                    ticket: t,
                    result: Ok(())
                },
                10
            )
            .is_empty());
    }
    #[test]
    fn opus_h1_empty_chat_terminal_command_session_and_ime() {
        let mut l = Launcher::default();
        l.update(Input::Edit(" ".into()), 0);
        assert!(matches!(
            l.update(Input::Enter { repeat: false }, 1)[0],
            Effect::OpenAiChat { boot: None, .. }
        ));
        l.update(Input::TerminalMode(true), 2);
        l.update(Input::Edit(">echo hi".into()), 3);
        l.update(Input::Composition(true), 4);
        assert!(l.update(Input::Enter { repeat: false }, 5).is_empty());
        l.update(Input::Composition(false), 6);
        let e = l.update(Input::Enter { repeat: false }, 7);
        assert!(
            matches!(&e[0],Effect::OpenTerminal{command:Some(c),session_id:None,..}if c=="echo hi")
        );
        l.update(
            Input::ActivationComplete {
                ticket: ticket(&e),
                result: Err("failed".into()),
            },
            8,
        );
        l.update(Input::SelectTerminalSession(Some("session:1".into())), 9);
        assert!(
            matches!(&l.update(Input::Enter{repeat:false},10)[0],Effect::OpenTerminal{command:None,session_id:Some(id),..}if id=="session:1")
        );
    }
    #[test]
    fn opus_m1_invalidation_payload_deep_hidden_and_cache() {
        let mut l = Launcher::default();
        l.update(Input::Show, 0);
        l.update(Input::Edit("alpha".into()), 0);
        l.update(Input::Tick, 8);
        result(&mut l, vec![Row::command("alpha", "Alpha")], 9);
        assert!(l
            .update(
                Input::Invalidate {
                    text: Some("other".into())
                },
                10
            )
            .is_empty());
        assert!(!l.session.pending());
        l.update(Input::ArgumentValue(Some("arg".into())), 10);
        l.update(
            Input::Invalidate {
                text: Some("arg".into()),
            },
            11,
        );
        assert_eq!(l.display_rows().len(), 1);
        assert!(matches!(&l.update(Input::Tick,19)[0],Effect::Search{query,..}if query=="alpha"));
        l.update(Input::Hide, 20);
        assert!(l.update(Input::Invalidate { text: None }, 21).is_empty());
        assert!(l.update(Input::BackendChanged, 22).is_empty());
        assert!(l.update(Input::Tick, 30).is_empty());
        l.update(Input::Show, 31);
        assert!(matches!(&l.update(Input::Tick,39)[0],Effect::Search{query,..}if query.is_empty()));
        l.update(Input::Edit("!deep".into()), 40);
        let g = l.session.generation;
        assert!(l.update(Input::Invalidate { text: None }, 41).is_empty());
        assert_eq!(l.session.generation, g);
    }
    #[test]
    fn opus_m2_backend_retires_activation_and_stale_completion_cannot_unlock_new() {
        let mut l = Launcher::default();
        l.update(Input::Show, 0);
        l.update(Input::Edit("query".into()), 0);
        l.update(Input::Tick, 8);
        result(&mut l, vec![Row::command("x", "X")], 9);
        let old = ticket(&l.update(Input::Enter { repeat: false }, 10));
        l.update(Input::BackendChanged, 11);
        assert!(!l.session.activating);
        l.update(Input::Tick, 19);
        result(&mut l, vec![Row::command("new", "New")], 20);
        let new = ticket(&l.update(Input::Enter { repeat: false }, 21));
        assert_ne!(new, old);
        assert!(l
            .update(
                Input::ActivationComplete {
                    ticket: old,
                    result: Ok(())
                },
                22
            )
            .is_empty());
        assert!(l.session.activating);
        assert_eq!(l.input, "query");
        l.update(
            Input::ActivationComplete {
                ticket: new,
                result: Ok(()),
            },
            23,
        );
        assert!(!l.visible);
    }
    #[test]
    fn opus_m3_all_hides_reset_input_composition_and_mode() {
        let mut l = Launcher::default();
        l.update(Input::TerminalMode(true), 0);
        l.update(Input::Edit(">pwd".into()), 1);
        let t = ticket(&l.update(Input::Enter { repeat: false }, 2));
        l.update(
            Input::ActivationComplete {
                ticket: t,
                result: Ok(()),
            },
            3,
        );
        assert!(l.input.is_empty());
        assert!(!l.terminal_mode);
        assert!(!l.composing);
        l.update(Input::Show, 4);
        assert!(matches!(&l.update(Input::Tick,12)[0],Effect::Search{query,..}if query.is_empty()));
    }
    #[test]
    fn opus_m4_one_utf16_unit_has_local_authority_and_stale_click_fails() {
        let mut l = Launcher::default();
        l.cache
            .accept("", vec![Arc::new(Row::command("alpha", "Alpha"))]);
        l.update(Input::Edit("a".into()), 0);
        assert!(l.update(Input::Tick, 8).is_empty());
        assert!(!l.session.pending());
        assert_eq!(l.session.rows.len(), 1);
        assert!(matches!(
            l.update(
                Input::Click {
                    generation: l.session.generation,
                    id: "command:alpha".into()
                },
                9
            )[0],
            Effect::Execute { .. }
        ));
        l.update(Input::Hide, 10);
        l.update(Input::Edit("😀".into()), 11);
        assert!(matches!(
            l.update(Input::Tick, 19)[0],
            Effect::Search { .. }
        ));
    }
    #[test]
    fn opus_note_rows_emit_typed_navigation_identity() {
        let mut l = Launcher::default();
        l.update(Input::Edit("note".into()), 0);
        l.update(Input::Tick, 8);
        let mut r = Row::command("x", "Note");
        r.id = "note:123".into();
        r.category = "quick-notes".into();
        result(&mut l, vec![r], 9);
        assert!(matches!(
            l.update(Input::Enter { repeat: false }, 10)[0],
            Effect::OpenNote { note_id: 123, .. }
        ));
    }
}

#[cfg(test)]
mod composition_ownership_repair {
    use super::*;
    #[test]
    fn opus_ime_cancels_deferred_enter_before_async_result() {
        let mut l = Launcher::default();
        l.update(Input::Edit("query".into()), 0);
        l.update(Input::Tick, 8);
        l.update(Input::Enter { repeat: false }, 9);
        l.update(Input::Composition(true), 10);
        assert!(l
            .update(
                Input::SearchComplete {
                    generation: l.session.generation,
                    result: Ok(vec![Row::command("x", "X")])
                },
                11
            )
            .is_empty());
        l.update(Input::Composition(false), 12);
        assert!(l.update(Input::Tick, 13).is_empty());
        assert!(!l.session.activating);
    }
}

#[cfg(test)]
mod refreshed_selection_repair {
    use super::*;
    #[test]
    fn opus_ai_history_selection_cannot_transfer_to_refreshed_first_row() {
        for backend_change in [false, true] {
            let mut l = Launcher::default();
            l.update(Input::Show, 0);
            l.update(Input::Edit(" task".into()), 0);
            l.update(Input::Tick, 8);
            l.update(
                Input::SearchComplete {
                    generation: l.session.generation,
                    result: Ok(vec![Row::command("old", "Old")]),
                },
                9,
            );
            l.update(Input::Move(1), 10);
            assert!(l.list_selection_active);
            l.update(
                if backend_change {
                    Input::BackendChanged
                } else {
                    Input::Invalidate { text: None }
                },
                11,
            );
            assert!(!l.list_selection_active);
            l.update(Input::Tick, 19);
            l.update(
                Input::SearchComplete {
                    generation: l.session.generation,
                    result: Ok(vec![Row::command("new", "New")]),
                },
                20,
            );
            assert!(
                matches!(&l.update(Input::Enter{repeat:false},21)[0],Effect::OpenAiChat{boot:Some(AiBoot::Submit{prompt,..}),..}if prompt=="task")
            );
        }
    }
}
