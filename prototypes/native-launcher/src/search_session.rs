//! UI-independent latest-wins ownership. Times are monotonic milliseconds.
#[derive(Debug)]
pub struct SearchSession<R> {
    pub generation: u64,
    pub displayed: Option<u64>,
    pub query: String,
    pub rows: Vec<R>,
    pub selected: usize,
    pub stale_dropped: u64,
    pub activating: bool,
    flight: Option<u64>,
    pending: Option<(u64, String)>,
    deadline: Option<u64>,
    enter: Option<(u64, u64)>,
}
impl<R> Default for SearchSession<R> {
    fn default() -> Self {
        Self {
            generation: 0,
            displayed: None,
            query: String::new(),
            rows: vec![],
            selected: 0,
            stale_dropped: 0,
            activating: false,
            flight: None,
            pending: None,
            deadline: None,
            enter: None,
        }
    }
}
impl<R> SearchSession<R> {
    pub fn commit(&mut self, text: String, now: u64) {
        self.generation += 1;
        self.query = text.clone();
        self.pending = Some((self.generation, text));
        // A fixed coalescing window, never extended by more typing.
        if self.deadline.is_none() {
            self.deadline = Some(now + 8);
        }
        self.enter = None;
    }
    pub fn take_request(&mut self, now: u64) -> Option<(u64, String)> {
        if self.flight.is_some() || now < self.deadline? {
            return None;
        }
        let request = self.pending.take()?;
        self.deadline = None;
        self.flight = Some(request.0);
        Some(request)
    }
    pub fn complete(&mut self, g: u64, rows: Vec<R>, now: u64) -> bool {
        if self.flight == Some(g) {
            self.flight = None;
        }
        if self.pending.is_some() && self.deadline.is_none() {
            self.deadline = Some(now);
        }
        if g != self.generation {
            self.stale_dropped += 1;
            return false;
        }
        self.rows = rows;
        self.displayed = Some(g);
        self.selected = 0;
        true
    }
    pub fn fail(&mut self, g: u64, now: u64) -> bool {
        self.complete(g, vec![], now)
    }
    pub fn pending(&self) -> bool {
        self.displayed != Some(self.generation)
    }
    pub fn move_selection(&mut self, delta: isize) {
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(self.rows.len().saturating_sub(1));
    }
    pub fn enter(&mut self, now: u64, repeat: bool) -> bool {
        if repeat || self.activating {
            return false;
        }
        if self.pending() {
            self.enter = Some((self.generation, now + 1000));
            return false;
        }
        self.begin_activation()
    }
    pub fn deferred_enter(&mut self, now: u64) -> bool {
        let Some((g, expires)) = self.enter else {
            return false;
        };
        if g != self.generation || now > expires {
            self.enter = None;
            return false;
        }
        if self.pending() {
            return false;
        }
        self.enter = None;
        self.begin_activation()
    }
    pub fn expire_enter(&mut self, now: u64) -> bool {
        if self.enter.is_some_and(|(_, expiry)| now >= expiry) {
            self.enter = None;
            true
        } else {
            false
        }
    }
    fn begin_activation(&mut self) -> bool {
        if self.rows.get(self.selected).is_none() {
            false
        } else {
            self.activating = true;
            true
        }
    }
    pub fn activation_finished(&mut self) {
        self.activating = false;
    }
    pub fn hide(&mut self) {
        self.generation += 1;
        self.displayed = None;
        self.rows.clear();
        self.query.clear();
        self.pending = None;
        self.deadline = None;
        self.enter = None;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn latest_pending_coalesces_without_extending_deadline() {
        let mut s = SearchSession::default();
        s.commit("a".into(), 0);
        s.commit("аб".into(), 7);
        assert!(s.take_request(7).is_none());
        assert_eq!(s.take_request(8), Some((2, "аб".into())));
        s.commit("c".into(), 9);
        s.commit("d".into(), 10);
        assert!(s.take_request(90).is_none());
        assert!(!s.complete(2, vec!["old"], 90));
        assert!(s.rows.is_empty());
        assert_eq!(s.take_request(90), Some((4, "d".into())));
        assert!(s.complete(4, vec!["current"], 91));
        assert_eq!(s.rows, vec!["current"]);
        assert!(!s.complete(1, vec!["ancient"], 92));
        assert_eq!(s.rows, vec!["current"]);
    }
    #[test]
    fn enter_waits_for_exact_generation_and_never_replays() {
        let mut s = SearchSession::default();
        s.commit("a".into(), 0);
        s.take_request(8);
        s.complete(1, vec![1], 10);
        s.commit("b".into(), 20);
        assert!(!s.enter(21, false));
        assert!(!s.enter(22, true));
        s.take_request(28);
        s.complete(2, vec![2], 30);
        assert!(s.deferred_enter(30));
        assert!(!s.enter(31, false));
        assert!(!s.deferred_enter(32));
        s.activation_finished();
        s.hide();
        assert!(!s.enter(33, true));
    }
    #[test]
    fn timeout_and_hide_drop_deferred_activation() {
        let mut s = SearchSession::default();
        s.commit("a".into(), 0);
        s.enter(1, false);
        assert!(s.expire_enter(1001));
        s.complete(1, vec![1], 1002);
        assert!(!s.deferred_enter(1002));
        s.commit("b".into(), 1003);
        s.enter(1004, false);
        s.hide();
        assert!(!s.complete(2, vec![2], 1005));
        assert!(!s.deferred_enter(1005));
    }
    #[test]
    fn selection_stops_at_edges_and_resets_for_new_results() {
        let mut s = SearchSession::default();
        s.commit("a".into(), 0);
        s.complete(1, vec![1, 2], 1);
        s.move_selection(-1);
        assert_eq!(s.selected, 0);
        s.move_selection(10);
        assert_eq!(s.selected, 1);
        s.commit("b".into(), 2);
        s.complete(2, vec![3], 3);
        assert_eq!(s.selected, 0);
    }
}
