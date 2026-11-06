pub mod launcher;
pub mod native;
pub mod settings;
pub mod snap_overlay;
pub mod terminal_sessions;
use std::time::{Duration, Instant};
#[derive(Default, Debug)]
pub struct Lifecycle {
    pub visible: bool,
    pub settings: bool,
    pub panel_focused: bool,
    pub suppress: usize,
    pub placed: bool,
    pub quitting: bool,
    generation: u64,
    hidden_at: Option<Instant>,
    hidden_generation: u64,
}
impl Lifecycle {
    pub fn show(&mut self) -> Option<u64> {
        if self.quitting {
            return None;
        }
        self.generation = self.generation.wrapping_add(1);
        self.visible = true;
        self.hidden_at = None;
        Some(self.generation)
    }
    pub fn hide(&mut self, now: Instant) -> Option<u64> {
        if !self.visible {
            return None;
        }
        self.visible = false;
        self.generation = self.generation.wrapping_add(1);
        self.hidden_at = Some(now);
        self.hidden_generation = self.generation;
        Some(self.generation)
    }
    pub fn blur_ticket(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }
    pub fn should_blur_hide(&self, g: u64) -> bool {
        g == self.generation
            && self.visible
            && self.placed
            && self.suppress == 0
            && !self.panel_focused
    }
    pub fn should_trim(&self, g: u64, now: Instant) -> bool {
        g == self.hidden_generation
            && !self.visible
            && self
                .hidden_at
                .is_some_and(|t| now.saturating_duration_since(t) >= Duration::from_secs(5))
    }
    pub fn suppress(&mut self, add: bool) {
        if add {
            self.suppress += 1;
        } else {
            self.suppress = self.suppress.saturating_sub(1);
        }
    }
    pub fn begin_quit(&mut self) -> bool {
        if self.quitting {
            return false;
        }
        self.quitting = true;
        self.generation = self.generation.wrapping_add(1);
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn visibility_timers_are_generation_fenced() {
        let mut l = Lifecycle::default();
        l.placed = true;
        l.show();
        let blur = l.blur_ticket();
        l.show();
        assert!(!l.should_blur_hide(blur));
        let now = Instant::now();
        let idle = l.hide(now).unwrap();
        assert!(!l.should_trim(idle, now));
        assert!(l.should_trim(idle, now + Duration::from_secs(5)));
        l.show();
        assert!(!l.should_trim(idle, now + Duration::from_secs(6)));
    }
    #[::core::prelude::v1::test]
    fn dialog_and_panel_suppress_blur() {
        let mut l = Lifecycle::default();
        l.placed = true;
        l.show();
        let g = l.blur_ticket();
        l.suppress(true);
        l.suppress(true);
        l.suppress(false);
        assert!(!l.should_blur_hide(g));
        l.suppress(false);
        assert!(l.should_blur_hide(g));
        l.panel_focused = true;
        assert!(!l.should_blur_hide(g));
        assert!(l.begin_quit());
        assert!(!l.begin_quit());
        assert!(l.show().is_none());
    }
}

pub mod persistence;
pub mod resize;
pub mod settings_close;
pub mod snap;
pub mod termination;
