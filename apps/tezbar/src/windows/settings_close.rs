//! Acknowledged Settings owners retain unlimited drain time. Only failed bootstrap uses a deadline.
use std::time::{Duration, Instant};
#[derive(Default)]
pub struct CloseHandshake {
    generation: u64,
    pending: Option<(u64, Instant, bool)>,
}
impl CloseHandshake {
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn request(&mut self, now: Instant) -> u64 {
        if let Some((g, _, _)) = self.pending {
            return g;
        }
        self.generation = self.generation.wrapping_add(1);
        self.pending = Some((self.generation, now, false));
        self.generation
    }
    pub fn acknowledge(&mut self, g: u64) -> bool {
        if let Some((current, _, acked)) = &mut self.pending {
            if *current == g {
                *acked = true;
                return true;
            }
        }
        false
    }
    pub fn finish(&mut self, g: u64, success: bool) -> bool {
        if self
            .pending
            .is_some_and(|(current, _, acked)| current == g && acked)
        {
            self.pending = None;
            return success;
        }
        false
    }
    pub fn timeout(&mut self, g: u64, now: Instant) -> bool {
        if self.pending.is_some_and(|(current, since, acked)| {
            current == g && !acked && now.saturating_duration_since(since) >= Duration::from_secs(3)
        }) {
            self.pending = None;
            return true;
        }
        false
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn acknowledged_drain_does_not_expire() {
        let mut h = CloseHandshake::default();
        let now = Instant::now();
        let g = h.request(now);
        h.acknowledge(g);
        assert!(!h.timeout(g, now + Duration::from_secs(20)));
        assert!(h.finish(g, true));
    }
    #[test]
    fn unacknowledged_bootstrap_falls_back_after_three_seconds() {
        let mut h = CloseHandshake::default();
        let now = Instant::now();
        let g = h.request(now);
        assert!(!h.timeout(g, now + Duration::from_secs(2)));
        assert!(h.timeout(g, now + Duration::from_secs(3)));
    }
    #[test]
    fn failures_retain_window_and_stale_completions_cannot_close_retry() {
        let mut h = CloseHandshake::default();
        let now = Instant::now();
        let old = h.request(now);
        h.acknowledge(old);
        assert!(!h.finish(old, false));
        let new = h.request(now);
        assert!(!h.finish(old, true));
        h.acknowledge(new);
        assert!(h.finish(new, true));
    }
}
