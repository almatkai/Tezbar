//! Event owner used by the GPUI surface. Optimistic rows are never activation authority.
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
    Move(isize),
    ActivationComplete {
        ticket: u64,
        result: Result<(), String>,
    },
    Hide,
    Show,
    Invalidate,
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
    Hide,
    RecordQuery(String),
}
#[derive(Debug, Default)]
pub struct Launcher {
    pub session: SearchSession<Arc<Row>>,
    pub cache: ResultCache,
    pub error: Option<String>,
    pub visible: bool,
    pub composing: bool,
    pub input: String,
    pub terminal_mode: bool,
    composition_draft: Option<String>,
    ticket: u64,
    activation: Option<(u64, u64)>,
}
impl Launcher {
    pub fn display_rows(&self) -> Vec<Arc<Row>> {
        if self.session.pending() {
            self.cache.preview(&request_input(&self.session.query))
        } else {
            self.session.rows.clone()
        }
    }
    fn activation_effects(&mut self) -> Vec<Effect> {
        let Some(row) = self.session.rows.get(self.session.selected) else {
            return vec![];
        };
        self.ticket += 1;
        self.activation = Some((self.ticket, self.session.generation));
        let mut out = vec![Effect::Execute {
            ticket: self.ticket,
            generation: self.session.generation,
            row_id: row.id.clone(),
            action: row.action.clone(),
            query: self.session.query.clone(),
            rank: self.session.selected,
        }];
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
                if self.composing {
                    self.composition_draft = Some(s);
                } else {
                    let q = request_input(&s);
                    let changed_mode = input_mode(&s, self.terminal_mode).mode
                        != input_mode(&self.input, self.terminal_mode).mode;
                    self.input = s;
                    if q != self.session.query || changed_mode {
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
                            self.cache.accept(&self.session.query, rows.clone());
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
                if accepted && success && self.session.deferred_enter(now) {
                    out.extend(self.activation_effects());
                }
                self.drain(now, &mut out);
            }
            Input::Enter { repeat } => {
                if !self.composing && self.session.enter(now, repeat) {
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
                            out.extend(self.activation_effects());
                        }
                    }
                }
            }
            Input::Move(delta) => self.session.move_selection(delta),
            Input::ActivationComplete { ticket, result } => {
                if let Some((owned, generation)) = self.activation {
                    if owned == ticket {
                        self.activation = None;
                        let current = self.session.activation_finished(generation);
                        if current {
                            match result {
                                Ok(()) => {
                                    self.session.hide();
                                    self.visible = false;
                                    out.push(Effect::Hide);
                                }
                                Err(e) => self.error = Some(e),
                            }
                        }
                    }
                }
            }
            Input::Hide => {
                self.visible = false;
                self.composing = false;
                self.composition_draft = None;
                self.input.clear();
                self.session.hide();
            }
            Input::Show => {
                self.visible = true;
                if self.session.query.is_empty() {
                    self.session.commit(String::new(), now);
                    self.drain(now, &mut out);
                }
            }
            Input::Invalidate => {
                self.cache.invalidate();
                self.session.commit(request_input(&self.input), now);
                self.drain(now, &mut out);
            }
            Input::BackendChanged => {
                self.session.reset_transport();
                self.cache.invalidate();
                self.session.commit(request_input(&self.input), now);
                self.drain(now, &mut out);
            }
            Input::TerminalMode(active) => {
                self.terminal_mode = active;
                self.session.hide();
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
    fn ime_search_errors_and_activation_owner() {
        let mut l = Launcher::default();
        l.update(Input::Composition(true), 0);
        assert!(l.update(Input::Edit("雪".into()), 1).is_empty());
        assert!(l.update(Input::Tick, 100).is_empty());
        l.update(Input::Composition(false), 101);
        assert_eq!(
            l.update(Input::Tick, 109),
            vec![Effect::Search {
                generation: 1,
                query: "雪".into()
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
        l.update(Input::BackendChanged, 12);
        assert!(
            matches!(l.update(Input::Tick,20).as_slice(),[Effect::Search{query,..}]if query.is_empty())
        );
    }
}
