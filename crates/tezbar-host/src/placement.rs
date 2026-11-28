use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
pub const WINDOW_WIDTH: f64 = 760.0;
pub const WINDOW_HEIGHT: f64 = 640.0;
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct PersistedWindowPosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct MonitorGeometry {
    pub bounds: (f64, f64, f64, f64),
    pub work_area: (f64, f64, f64, f64),
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct SnapLockState {
    pub x: bool,
    pub y: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapGuidesState {
    pub visible: bool,
    pub snap_x: bool,
    pub snap_y: bool,
    pub centered: bool,
    pub target_rect: Option<SnapTargetRect>,
}

#[derive(Debug)]
pub struct SnapMotionState {
    pub generation: u64,
    pub last_raw_position: Option<PersistedWindowPosition>,
    pub last_raw_at: Option<Instant>,
    pub slow_x_since: Option<Instant>,
    pub slow_y_since: Option<Instant>,
    pub programmatic_position: Option<(PersistedWindowPosition, Instant)>,
    pub settle_timer_active: bool,
    pub snap_anchor: PersistedWindowPosition,
    pub snap_escape: PersistedWindowPosition,
}

impl Default for SnapMotionState {
    fn default() -> Self {
        Self {
            generation: 0,
            last_raw_position: None,
            last_raw_at: None,
            slow_x_since: None,
            slow_y_since: None,
            programmatic_position: None,
            settle_timer_active: false,
            snap_anchor: PersistedWindowPosition { x: 0.0, y: 0.0 },
            snap_escape: PersistedWindowPosition { x: 0.0, y: 0.0 },
        }
    }
}

pub struct SnapAcquireContext {
    pub position: PersistedWindowPosition,
    pub window_width: f64,
    pub window_height: f64,
    pub monitor: MonitorGeometry,
    pub locked: SnapLockState,
    pub now: Instant,
    pub from_timer: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapTargetRect {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

pub const SNAP_THRESHOLD: f64 = 32.0;
pub const SNAP_RELEASE_THRESHOLD: f64 = 32.0;
pub const SNAP_SLOW_SPEED_THRESHOLD: f64 = 420.0;
pub const SNAP_DWELL_DURATION: Duration = Duration::from_millis(130);
#[cfg(target_os = "macos")]
pub const WINDOW_POSITIONS_BY_MONITOR_KEY: &str = "windowPositionsByMonitorV3";
#[cfg(not(target_os = "macos"))]
pub const WINDOW_POSITIONS_BY_MONITOR_KEY: &str = "windowPositionsByMonitorV2";
pub const LEGACY_WINDOW_POSITION_KEYS: &[&str] = &[
    "tauriWindowPosition",
    "tauriWindowPositionsByDisplay",
    "windowPositionsByMonitor",
    "windowPosition",
    "windowPositionsByDisplay",
    "tezbarWindowPlacementInitialized",
];

pub fn position_from_config_value(
    value: Option<&serde_json::Value>,
) -> Option<PersistedWindowPosition> {
    serde_json::from_value(value?.clone()).ok()
}

pub fn monitor_positions_from_config(
    config: &serde_json::Value,
) -> BTreeMap<String, PersistedWindowPosition> {
    config
        .get(WINDOW_POSITIONS_BY_MONITOR_KEY)
        .and_then(serde_json::Value::as_object)
        .map(|positions| {
            positions
                .iter()
                .filter_map(|(monitor_id, value)| {
                    position_from_config_value(Some(value))
                        .map(|position| (monitor_id.clone(), position))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn window_position_config_needs_reset(config: &serde_json::Value) -> bool {
    let has_legacy_position = LEGACY_WINDOW_POSITION_KEYS
        .iter()
        .any(|key| config.get(key).is_some());

    #[cfg(target_os = "macos")]
    return has_legacy_position || config.get("windowPositionsByMonitorV2").is_some();

    #[cfg(not(target_os = "macos"))]
    has_legacy_position
}

pub fn update_window_position_config(
    config: &mut serde_json::Value,
    monitor_id: &str,
    position: PersistedWindowPosition,
) {
    let mut positions = monitor_positions_from_config(config);
    positions.insert(monitor_id.to_string(), position);

    let Some(config_object) = config.as_object_mut() else {
        return;
    };
    for &obsolete_key in LEGACY_WINDOW_POSITION_KEYS {
        config_object.remove(obsolete_key);
    }
    #[cfg(target_os = "macos")]
    config_object.remove("windowPositionsByMonitorV2");
    config_object.insert(
        WINDOW_POSITIONS_BY_MONITOR_KEY.to_string(),
        json!(positions),
    );
}

pub fn monitor_id_from_name(name: Option<&str>) -> Option<String> {
    name.map(str::to_string)
}

pub fn position_is_in_bounds(
    position: PersistedWindowPosition,
    bounds: (f64, f64, f64, f64),
) -> bool {
    let (x, y, width, height) = bounds;
    position.x >= x && position.x < x + width && position.y >= y && position.y < y + height
}

pub fn clamp_position_to_work_area(
    position: PersistedWindowPosition,
    work_area: (f64, f64, f64, f64),
    window_width: f64,
    window_height: f64,
) -> PersistedWindowPosition {
    let (work_x, work_y, work_width, work_height) = work_area;
    let max_x = work_x + (work_width - window_width).max(0.0);
    let max_y = work_y + (work_height - window_height).max(0.0);
    PersistedWindowPosition {
        x: position.x.round().clamp(work_x, max_x),
        y: position.y.round().clamp(work_y, max_y),
    }
}

pub fn plan_window_position(
    active_monitor: MonitorGeometry,
    window_width: f64,
    window_height: f64,
    saved_for_active_monitor: Option<PersistedWindowPosition>,
) -> PersistedWindowPosition {
    if let Some(position) = saved_for_active_monitor {
        return clamp_position_to_work_area(
            position,
            active_monitor.work_area,
            window_width,
            window_height,
        );
    }

    let (work_x, work_y, work_width, work_height) = active_monitor.work_area;
    let position = PersistedWindowPosition {
        x: work_x + ((work_width - window_width) / 2.0).max(0.0),
        y: work_y + ((work_height - window_height) / 2.0).max(0.0),
    };
    debug_assert!(position_is_in_bounds(position, active_monitor.bounds));
    position
}

pub fn snap_axis(
    window_start: f64,
    window_extent: f64,
    target: f64,
    distance: f64,
    locked: bool,
    acquire: bool,
) -> (f64, bool, bool) {
    let still_locked = locked && distance <= SNAP_RELEASE_THRESHOLD;
    let should_lock = !locked && acquire && distance <= SNAP_THRESHOLD;
    if !still_locked && !should_lock {
        return (window_start, false, false);
    }
    (target - window_extent / 2.0, true, true)
}

pub fn window_position_for_cursor_drag(
    initial_window_position: PersistedWindowPosition,
    initial_cursor_position: PersistedWindowPosition,
    cursor_position: PersistedWindowPosition,
) -> PersistedWindowPosition {
    PersistedWindowPosition {
        x: initial_window_position.x + cursor_position.x - initial_cursor_position.x,
        y: initial_window_position.y + cursor_position.y - initial_cursor_position.y,
    }
}

pub fn snap_window_position(
    position: PersistedWindowPosition,
    window_width: f64,
    window_height: f64,
    monitor: MonitorGeometry,
    locked: SnapLockState,
    acquire: SnapLockState,
) -> (PersistedWindowPosition, SnapLockState, bool) {
    let (work_x, work_y, work_width, work_height) = monitor.work_area;
    let window_center_x = position.x + window_width / 2.0;
    let window_center_y = position.y + window_height / 2.0;
    let monitor_center_x = work_x + work_width / 2.0;
    let monitor_center_y = work_y + work_height / 2.0;
    let dx = (window_center_x - monitor_center_x).abs();
    let dy = (window_center_y - monitor_center_y).abs();
    let (x, x_locked, x_active) = snap_axis(
        position.x,
        window_width,
        monitor_center_x,
        dx,
        locked.x,
        acquire.x,
    );
    let (y, y_locked, y_active) = snap_axis(
        position.y,
        window_height,
        monitor_center_y,
        dy,
        locked.y,
        acquire.y,
    );

    (
        PersistedWindowPosition { x, y },
        SnapLockState {
            x: x_locked,
            y: y_locked,
        },
        x_active || y_active,
    )
}

pub fn snap_target_rect(
    window_width: f64,
    window_height: f64,
    monitor: MonitorGeometry,
) -> SnapTargetRect {
    let (work_x, work_y, work_width, work_height) = monitor.work_area;
    let target_x = work_x + (work_width - window_width) / 2.0;
    let target_y = work_y + (work_height - window_height) / 2.0;
    SnapTargetRect {
        left: ((target_x - work_x) / work_width * 100.0).clamp(0.0, 100.0),
        top: ((target_y - work_y) / work_height * 100.0).clamp(0.0, 100.0),
        right: ((target_x + window_width - work_x) / work_width * 100.0).clamp(0.0, 100.0),
        bottom: ((target_y + window_height - work_y) / work_height * 100.0).clamp(0.0, 100.0),
    }
}

pub fn update_snap_candidate(
    since: &mut Option<Instant>,
    eligible: bool,
    now: Instant,
) -> (bool, bool) {
    if !eligible {
        *since = None;
        return (false, false);
    }
    let started = since.is_none();
    let started_at = *since.get_or_insert(now);
    (
        now.duration_since(started_at) >= SNAP_DWELL_DURATION,
        started,
    )
}

pub fn snap_acquire_state(
    motion: &mut SnapMotionState,
    context: SnapAcquireContext,
) -> (SnapLockState, bool, bool) {
    let SnapAcquireContext {
        position,
        window_width,
        window_height,
        monitor,
        locked,
        now,
        from_timer,
    } = context;
    let speed = if from_timer {
        0.0
    } else {
        match (motion.last_raw_position, motion.last_raw_at) {
            (Some(previous), Some(previous_at)) => {
                let elapsed = now.duration_since(previous_at).as_secs_f64();
                if elapsed > 0.0 {
                    ((position.x - previous.x).hypot(position.y - previous.y)) / elapsed
                } else {
                    f64::INFINITY
                }
            }
            _ => f64::INFINITY,
        }
    };
    if !from_timer {
        motion.last_raw_position = Some(position);
        motion.last_raw_at = Some(now);
    }

    let (work_x, work_y, work_width, work_height) = monitor.work_area;
    let dx = (position.x + window_width / 2.0 - (work_x + work_width / 2.0)).abs();
    let dy = (position.y + window_height / 2.0 - (work_y + work_height / 2.0)).abs();
    let stationary = from_timer
        && motion
            .last_raw_at
            .map(|last| now.duration_since(last) >= SNAP_DWELL_DURATION)
            .unwrap_or(false);
    let moving_slowly = speed <= SNAP_SLOW_SPEED_THRESHOLD || stationary;
    let near_x = !locked.x && dx <= SNAP_THRESHOLD;
    let near_y = !locked.y && dy <= SNAP_THRESHOLD;
    let should_schedule_timer = if from_timer {
        motion.settle_timer_active = false;
        false
    } else if (near_x || near_y) && !motion.settle_timer_active {
        motion.settle_timer_active = true;
        true
    } else {
        false
    };
    if stationary {
        if near_x && motion.slow_x_since.is_none() {
            motion.slow_x_since = Some(now - SNAP_DWELL_DURATION);
        }
        if near_y && motion.slow_y_since.is_none() {
            motion.slow_y_since = Some(now - SNAP_DWELL_DURATION);
        }
    }
    let x_eligible = !locked.x
        && near_x
        && moving_slowly
        && (!from_timer || motion.slow_x_since.is_some() || stationary);
    let y_eligible = !locked.y
        && near_y
        && moving_slowly
        && (!from_timer || motion.slow_y_since.is_some() || stationary);
    let (acquire_x, started_x) = update_snap_candidate(&mut motion.slow_x_since, x_eligible, now);
    let (acquire_y, started_y) = update_snap_candidate(&mut motion.slow_y_since, y_eligible, now);

    (
        SnapLockState {
            x: acquire_x,
            y: acquire_y,
        },
        started_x || started_y,
        should_schedule_timer,
    )
}

pub fn select_active_monitor<T>(
    cursor: Option<T>,
    frontmost: Option<T>,
    current: impl FnOnce() -> Option<T>,
    primary: impl FnOnce() -> Option<T>,
) -> Option<T> {
    cursor.or(frontmost).or_else(current).or_else(primary)
}

pub fn window_size_for_target_monitor(
    _current_size: (f64, f64),
    _current_scale_factor: f64,
    target_scale_factor: f64,
) -> (f64, f64) {
    let target_scale_factor = valid_scale_factor(target_scale_factor);
    (
        WINDOW_WIDTH * target_scale_factor,
        WINDOW_HEIGHT * target_scale_factor,
    )
}

pub fn valid_scale_factor(scale_factor: f64) -> f64 {
    if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    }
}

#[cfg(any(target_os = "macos", test))]
pub fn logical_position_for_monitor(
    position: PersistedWindowPosition,
    target_scale_factor: f64,
) -> PersistedWindowPosition {
    let target_scale_factor = valid_scale_factor(target_scale_factor);
    PersistedWindowPosition {
        x: position.x / target_scale_factor,
        y: position.y / target_scale_factor,
    }
}

#[cfg(any(target_os = "macos", test))]
pub fn logical_window_center(
    position: PersistedWindowPosition,
    size: (f64, f64),
    scale_factor: f64,
) -> PersistedWindowPosition {
    let scale_factor = valid_scale_factor(scale_factor);
    PersistedWindowPosition {
        x: (position.x + size.0 / 2.0) / scale_factor,
        y: (position.y + size.1 / 2.0) / scale_factor,
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StoredWindowPosition {
    pub position: Option<PersistedWindowPosition>,
    pub needs_reset: bool,
}

#[derive(Default)]
pub struct PendingWindowPositions {
    pub positions: BTreeMap<String, PersistedWindowPosition>,
    pub generation: u64,
    pub dirty: bool,
}

impl PendingWindowPositions {
    pub fn should_flush(&self, generation: Option<u64>) -> bool {
        self.dirty && generation.is_none_or(|generation| generation == self.generation)
    }

    pub fn remember(&mut self, monitor_id: String, position: PersistedWindowPosition) -> u64 {
        self.positions.insert(monitor_id, position);
        self.generation = self.generation.wrapping_add(1);
        self.dirty = true;
        self.generation
    }
}

#[cfg(test)]
mod window_placement_tests {
    use super::*;

    #[test]
    fn reopening_a_collapsed_launcher_restores_standard_height() {
        for scale in [1.0, 1.5, 2.0] {
            assert_eq!(
                window_size_for_target_monitor((760.0, 120.0), 1.0, scale),
                (760.0 * scale, 640.0 * scale),
            );
        }
    }

    #[test]
    fn monitor_storage_uses_only_the_monitor_id() {
        assert_eq!(
            monitor_id_from_name(Some(r"\\.\DISPLAY2")).as_deref(),
            Some(r"\\.\DISPLAY2")
        );
    }

    #[test]
    fn cursor_monitor_is_the_active_monitor() {
        let selected = select_active_monitor(
            Some("cursor"),
            Some("frontmost"),
            || panic!("current monitor lookup must be short-circuited"),
            || panic!("primary monitor lookup must be short-circuited"),
        );

        assert_eq!(selected, Some("cursor"));
    }

    #[test]
    fn frontmost_monitor_is_used_when_cursor_detection_is_unavailable() {
        let selected = select_active_monitor(
            None,
            Some("frontmost"),
            || panic!("current monitor lookup must be short-circuited"),
            || panic!("primary monitor lookup must be short-circuited"),
        );

        assert_eq!(selected, Some("frontmost"));
    }

    #[test]
    fn persistence_keeps_one_coordinate_pair_per_monitor_id() {
        let mut config = json!({
            "unrelatedSetting": true,
            (WINDOW_POSITIONS_BY_MONITOR_KEY): {
                "Monitor B": { "x": 2100.0, "y": 150.0 }
            }
        });

        update_window_position_config(
            &mut config,
            "Monitor A",
            PersistedWindowPosition { x: 30.0, y: 40.0 },
        );

        assert_eq!(
            config,
            json!({
                "unrelatedSetting": true,
                (WINDOW_POSITIONS_BY_MONITOR_KEY): {
                    "Monitor A": { "x": 30.0, "y": 40.0 },
                    "Monitor B": { "x": 2100.0, "y": 150.0 }
                }
            })
        );
    }

    #[test]
    fn broken_legacy_coordinates_are_reset_instead_of_restored() {
        let mut config = json!({
            "tauriWindowPosition": { "x": 1.0, "y": 2.0 },
            "windowPositionsByMonitor": {
                "macos:broken": { "x": 52.0, "y": 568.0 }
            },
            "windowPosition": { "x": 0.5, "y": 1.0 },
            "windowPositionsByDisplay": {
                "Monitor A:1920:1080:2": { "x": 15.0, "y": 25.0 }
            },
            "tauriWindowPositionsByDisplay": {
                "Monitor A:0:0:1920:1080:2": { "x": 30.0, "y": 50.0 }
            }
        });

        assert!(window_position_config_needs_reset(&config));
        update_window_position_config(
            &mut config,
            "Monitor B",
            PersistedWindowPosition { x: 200.0, y: 300.0 },
        );

        assert_eq!(
            config,
            json!({
                (WINDOW_POSITIONS_BY_MONITOR_KEY): {
                    "Monitor B": { "x": 200.0, "y": 300.0 }
                }
            })
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_startup_overwritten_v2_position_is_reset() {
        let mut config = json!({
            "windowPositionsByMonitorV2": {
                "macos:startup-overwritten": { "x": 130.0, "y": 130.0 }
            }
        });

        assert!(window_position_config_needs_reset(&config));
        update_window_position_config(
            &mut config,
            "macos:current",
            PersistedWindowPosition { x: 520.0, y: 264.0 },
        );

        assert_eq!(
            config,
            json!({
                "windowPositionsByMonitorV3": {
                    "macos:current": { "x": 520.0, "y": 264.0 }
                }
            })
        );
    }

    #[test]
    fn missing_position_centers_on_the_active_monitor() {
        let external = MonitorGeometry {
            bounds: (1920.0, -180.0, 2560.0, 1440.0),
            work_area: (1920.0, -156.0, 2560.0, 1416.0),
        };

        let position = plan_window_position(external, 760.0, 640.0, None);

        assert!(
            position_is_in_bounds(position, external.bounds),
            "launcher must be placed on the active external monitor, got {position:?}"
        );
        assert_eq!(
            position,
            PersistedWindowPosition {
                x: 2820.0,
                y: 232.0
            }
        );
    }

    #[test]
    fn mixed_dpi_monitor_uses_its_own_scale_and_center() {
        let external = MonitorGeometry {
            bounds: (1800.0, 0.0, 1920.0, 1080.0),
            work_area: (1800.0, 0.0, 1920.0, 1080.0),
        };
        let (window_width, window_height) =
            window_size_for_target_monitor((1520.0, 1280.0), 2.0, 1.0);

        assert_eq!((window_width, window_height), (760.0, 640.0));
        let physical_position = plan_window_position(external, window_width, window_height, None);
        assert_eq!(
            logical_position_for_monitor(physical_position, 1.0),
            PersistedWindowPosition {
                x: 2380.0,
                y: 220.0
            }
        );
        assert!(position_is_in_bounds(physical_position, external.bounds));

        // This is the old Tauri PhysicalPosition behavior: Tao divides by the
        // hidden window's current scale (2), putting x=1190 back on the Retina
        // monitor instead of the active external monitor.
        let old_effective_position = PersistedWindowPosition {
            x: physical_position.x / 2.0,
            y: physical_position.y / 2.0,
        };
        assert!(!position_is_in_bounds(
            old_effective_position,
            external.bounds
        ));
    }

    #[test]
    fn windows_mixed_dpi_centers_from_200_percent_to_100_percent() {
        let target = MonitorGeometry {
            bounds: (0.0, 0.0, 1920.0, 1080.0),
            work_area: (0.0, 0.0, 1920.0, 1040.0),
        };
        let (window_width, window_height) =
            window_size_for_target_monitor((1520.0, 1280.0), 2.0, 1.0);

        assert_eq!((window_width, window_height), (760.0, 640.0));
        assert_eq!(
            plan_window_position(target, window_width, window_height, None),
            PersistedWindowPosition { x: 580.0, y: 200.0 }
        );
    }

    #[test]
    fn windows_mixed_dpi_centers_on_negative_200_percent_monitor() {
        let target = MonitorGeometry {
            bounds: (-3840.0, 0.0, 3840.0, 2160.0),
            work_area: (-3840.0, 0.0, 3840.0, 2080.0),
        };
        let (window_width, window_height) =
            window_size_for_target_monitor((760.0, 640.0), 1.0, 2.0);

        assert_eq!((window_width, window_height), (1520.0, 1280.0));
        assert_eq!(
            plan_window_position(target, window_width, window_height, None),
            PersistedWindowPosition {
                x: -2680.0,
                y: 400.0
            }
        );
    }

    #[test]
    fn mixed_dpi_monitor_scales_up_from_external_to_retina() {
        let retina = MonitorGeometry {
            bounds: (0.0, 0.0, 3600.0, 2338.0),
            work_area: (0.0, 0.0, 3600.0, 2338.0),
        };
        let (window_width, window_height) =
            window_size_for_target_monitor((760.0, 640.0), 1.0, 2.0);

        assert_eq!((window_width, window_height), (1520.0, 1280.0));
        let physical_position = plan_window_position(retina, window_width, window_height, None);
        assert_eq!(
            physical_position,
            PersistedWindowPosition {
                x: 1040.0,
                y: 529.0
            }
        );
        assert_eq!(
            logical_position_for_monitor(physical_position, 2.0),
            PersistedWindowPosition { x: 520.0, y: 264.5 }
        );
    }

    #[test]
    fn active_monitor_restores_and_clamps_its_own_saved_position() {
        let external = MonitorGeometry {
            bounds: (-2560.0, -180.0, 2560.0, 1440.0),
            work_area: (-2560.0, -156.0, 2560.0, 1416.0),
        };

        let position = plan_window_position(
            external,
            760.0,
            640.0,
            Some(PersistedWindowPosition {
                x: -3000.0,
                y: -500.0,
            }),
        );

        assert_eq!(
            position,
            PersistedWindowPosition {
                x: -2560.0,
                y: -156.0,
            }
        );
    }

    #[test]
    fn cursor_drag_preserves_the_initial_grab_offset() {
        let position = window_position_for_cursor_drag(
            PersistedWindowPosition { x: 180.0, y: 120.0 },
            PersistedWindowPosition { x: 184.0, y: 124.0 },
            PersistedWindowPosition { x: 598.0, y: 214.0 },
        );

        assert_eq!(position, PersistedWindowPosition { x: 594.0, y: 210.0 });
    }

    #[test]
    fn window_snaps_to_monitor_center_when_both_axes_are_near() {
        let monitor = MonitorGeometry {
            bounds: (0.0, 0.0, 1920.0, 1080.0),
            work_area: (0.0, 24.0, 1920.0, 1056.0),
        };
        let (position, locked, active) = snap_window_position(
            PersistedWindowPosition { x: 572.0, y: 208.0 },
            760.0,
            640.0,
            monitor,
            SnapLockState::default(),
            SnapLockState { x: true, y: true },
        );

        assert_eq!(position, PersistedWindowPosition { x: 580.0, y: 232.0 });
        assert!(locked.x);
        assert!(locked.y);
        assert!(active);
    }

    #[test]
    fn window_snaps_each_axis_independently() {
        let monitor = MonitorGeometry {
            bounds: (0.0, 0.0, 1920.0, 1080.0),
            work_area: (0.0, 24.0, 1920.0, 1056.0),
        };
        let (position, locked, active) = snap_window_position(
            PersistedWindowPosition { x: 572.0, y: 320.0 },
            760.0,
            640.0,
            monitor,
            SnapLockState::default(),
            SnapLockState { x: true, y: true },
        );

        assert_eq!(position.x, 580.0);
        assert_eq!(position.y, 320.0);
        assert!(locked.x);
        assert!(!locked.y);
        assert!(active);
    }

    #[test]
    fn window_snap_waits_for_a_slow_dwell_before_acquiring() {
        let monitor = MonitorGeometry {
            bounds: (0.0, 0.0, 1920.0, 1080.0),
            work_area: (0.0, 24.0, 1920.0, 1056.0),
        };
        let position = PersistedWindowPosition { x: 580.0, y: 320.0 };
        let now = Instant::now();
        let mut motion = SnapMotionState {
            last_raw_position: Some(position),
            last_raw_at: Some(now),
            ..SnapMotionState::default()
        };

        let (acquire, started, _) = snap_acquire_state(
            &mut motion,
            SnapAcquireContext {
                position,
                window_width: 760.0,
                window_height: 640.0,
                monitor,
                locked: SnapLockState::default(),
                now: now + Duration::from_millis(1),
                from_timer: false,
            },
        );
        assert!(started);
        assert!(!acquire.x);

        let (acquire, _, _) = snap_acquire_state(
            &mut motion,
            SnapAcquireContext {
                position,
                window_width: 760.0,
                window_height: 640.0,
                monitor,
                locked: SnapLockState::default(),
                now: now + SNAP_DWELL_DURATION + Duration::from_millis(1),
                from_timer: true,
            },
        );
        assert!(acquire.x);
    }

    #[test]
    fn window_does_not_snap_just_outside_the_soft_capture_radius() {
        let monitor = MonitorGeometry {
            bounds: (0.0, 0.0, 1920.0, 1080.0),
            work_area: (0.0, 24.0, 1920.0, 1056.0),
        };
        let (position, locked, active) = snap_window_position(
            PersistedWindowPosition { x: 616.0, y: 268.0 },
            760.0,
            640.0,
            monitor,
            SnapLockState::default(),
            SnapLockState { x: true, y: true },
        );

        assert_eq!(position, PersistedWindowPosition { x: 616.0, y: 268.0 });
        assert!(!locked.x);
        assert!(!locked.y);
        assert!(!active);
    }

    #[test]
    fn snap_guides_describe_a_fixed_centered_target_frame() {
        let monitor = MonitorGeometry {
            bounds: (-1920.0, 0.0, 1920.0, 1080.0),
            work_area: (-1920.0, 24.0, 1920.0, 1056.0),
        };
        let target = snap_target_rect(760.0, 640.0, monitor);

        assert!((target.left - 30.208_333).abs() < 0.000_01);
        assert!((target.right - 69.791_667).abs() < 0.000_01);
        assert!((target.top - 19.696_970).abs() < 0.000_01);
        assert!((target.bottom - 80.303_030).abs() < 0.000_01);
    }

    #[test]
    fn window_snap_uses_hysteresis_before_releasing() {
        let monitor = MonitorGeometry {
            bounds: (0.0, 0.0, 1920.0, 1080.0),
            work_area: (0.0, 24.0, 1920.0, 1056.0),
        };

        let (_, locked, active) = snap_window_position(
            PersistedWindowPosition { x: 600.0, y: 250.0 },
            760.0,
            640.0,
            monitor,
            SnapLockState { x: true, y: true },
            SnapLockState::default(),
        );
        assert!(locked.x);
        assert!(locked.y);
        assert!(active);

        let (_, locked, active) = snap_window_position(
            PersistedWindowPosition { x: 700.0, y: 340.0 },
            760.0,
            640.0,
            monitor,
            SnapLockState { x: true, y: true },
            SnapLockState::default(),
        );
        assert!(!locked.x);
        assert!(!locked.y);
        assert!(!active);
    }
}
