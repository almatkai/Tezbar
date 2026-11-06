//! Snap acquisition uses the host's exact dwell/speed rules; timers carry an interaction generation.
use std::time::Instant;
use tezbar_host::placement::{self, *};
#[derive(Default)]
pub struct Snap {
    motion: SnapMotionState,
    locked: SnapLockState,
    pub active: bool,
    pub generation: u64,
}
impl Snap {
    pub fn begin(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.active = true;
        self.motion = Default::default();
        self.locked = Default::default();
        self.generation
    }
    pub fn end(&mut self) {
        self.active = false;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn update(
        &mut self,
        position: PersistedWindowPosition,
        width: f64,
        height: f64,
        monitor: MonitorGeometry,
        timer: Option<u64>,
        now: Instant,
    ) -> Option<(PersistedWindowPosition, SnapLockState, bool)> {
        if !self.active || timer.is_some_and(|g| g != self.generation) {
            return None;
        }
        let (acquire, _, schedule) = placement::snap_acquire_state(
            &mut self.motion,
            SnapAcquireContext {
                position,
                window_width: width,
                window_height: height,
                monitor,
                locked: self.locked,
                now,
                from_timer: timer.is_some(),
            },
        );
        let (p, locked, _) =
            placement::snap_window_position(position, width, height, monitor, self.locked, acquire);
        self.locked = locked;
        Some((p, locked, schedule))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ended_drag_discards_dwell() {
        let mut snap = Snap::default();
        let g = snap.begin();
        snap.end();
        assert!(
            snap.update(
                PersistedWindowPosition { x: 0., y: 0. },
                760.,
                640.,
                MonitorGeometry {
                    bounds: (0., 0., 1000., 1000.),
                    work_area: (0., 0., 1000., 1000.)
                },
                Some(g),
                Instant::now()
            )
            .is_none()
        );
    }
    #[test]
    fn stationary_center_acquires_after_host_dwell() {
        let mut snap = Snap::default();
        let g = snap.begin();
        let now = Instant::now();
        let m = MonitorGeometry {
            bounds: (0., 0., 1000., 1000.),
            work_area: (0., 0., 1000., 1000.),
        };
        let p = PersistedWindowPosition { x: 120., y: 180. };
        assert!(snap.update(p, 760., 640., m, None, now).unwrap().2);
        let (_, lock, _) = snap
            .update(
                p,
                760.,
                640.,
                m,
                Some(g),
                now + std::time::Duration::from_millis(130),
            )
            .unwrap();
        assert!(lock.x && lock.y);
    }
}
