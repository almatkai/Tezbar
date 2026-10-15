//! Launcher dimensions and UI zoom are logical values, independent of display DPI.
//! Kept outside config.json: backend preference writes must not race native resize writes.
use super::{monitor_id, openray_config_path, physical_monitor_work_area, valid_scale_factor};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, sync::Mutex, time::Duration};
use tauri::{Emitter, LogicalSize, Manager, Monitor, PhysicalSize, WebviewWindow};

pub const MIN_WIDTH: f64 = 480.0;
pub const MIN_HEIGHT: f64 = 360.0;
const MIN_ZOOM: f64 = 0.75;
const MAX_ZOOM: f64 = 2.0;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub width: f64,
    pub height: f64,
    pub zoom: f64,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            width: super::WINDOW_WIDTH,
            height: super::WINDOW_HEIGHT,
            zoom: 1.0,
        }
    }
}

fn finite_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

impl Profile {
    pub fn physical_size(self, dpi: f64) -> (f64, f64) {
        let dpi = valid_scale_factor(dpi);
        (self.width * dpi, self.height * dpi)
    }

    fn fit(self, available: (f64, f64)) -> Self {
        let zoom = finite_or(self.zoom, 1.0).clamp(MIN_ZOOM, MAX_ZOOM);
        let max_width = available.0.max(1.0);
        let max_height = available.1.max(1.0);
        Self {
            width: finite_or(self.width, super::WINDOW_WIDTH)
                .clamp((MIN_WIDTH * zoom).min(max_width), max_width),
            height: finite_or(self.height, super::WINDOW_HEIGHT)
                .clamp((MIN_HEIGHT * zoom).min(max_height), max_height),
            zoom,
        }
    }
}

#[derive(Default)]
struct Layouts {
    loaded: bool,
    profiles: BTreeMap<String, Profile>,
    active: Option<String>,
    current: Profile,
    generation: u64,
    resizing: bool,
}

#[derive(Default)]
pub struct WindowLayoutState(Mutex<Layouts>);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutInfo {
    display_id: String,
    display_name: String,
    #[serde(flatten)]
    profile: Profile,
}

fn storage_path() -> Option<std::path::PathBuf> {
    openray_config_path().map(|path| path.with_file_name("launcher-layouts-v1.json"))
}

fn load(layouts: &mut Layouts) {
    if layouts.loaded {
        return;
    }
    layouts.loaded = true;
    layouts.profiles = storage_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|raw| profiles_from_json(&raw))
        .unwrap_or_default();
}

fn profiles_from_json(raw: &str) -> BTreeMap<String, Profile> {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .map(|profiles| {
            profiles
                .into_iter()
                .filter_map(|(id, value)| {
                    serde_json::from_value(value)
                        .ok()
                        .map(|profile| (id, profile))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn save(layouts: &Layouts) -> Result<(), String> {
    let path = storage_path().ok_or("Launcher settings directory unavailable")?;
    fs::create_dir_all(path.parent().ok_or("Missing settings directory")?)
        .map_err(|e| e.to_string())?;
    let raw = serde_json::to_vec_pretty(&layouts.profiles).map_err(|e| e.to_string())?;
    // Serialize under the native state lock and replace atomically, so a crash
    // during a resize cannot truncate the last complete set of display profiles.
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, raw).map_err(|e| e.to_string())?;
    fs::rename(temporary, path).map_err(|e| e.to_string())
}

fn available(monitor: &Monitor) -> (f64, f64) {
    let (_, _, width, height) = physical_monitor_work_area(monitor);
    let dpi = valid_scale_factor(monitor.scale_factor());
    (width / dpi, height / dpi)
}

fn info(window: &WebviewWindow, monitor: &Monitor, profile: Profile) -> LayoutInfo {
    LayoutInfo {
        display_id: monitor_id(window, monitor).unwrap_or_else(|| "primary".into()),
        display_name: monitor
            .name()
            .cloned()
            .unwrap_or_else(|| "Current display".into()),
        profile,
    }
}

fn emit(window: &WebviewWindow, monitor: &Monitor, profile: Profile) {
    let _ = window
        .app_handle()
        .emit("launcher-layout-changed", info(window, monitor, profile));
}

fn emit_viewport(window: &WebviewWindow, width: f64, height: f64, dpi: f64, zoom: f64) {
    let dpi = valid_scale_factor(dpi);
    let _ = window.emit(
        "launcher-viewport-changed",
        Profile {
            width: width / dpi,
            height: height / dpi,
            zoom,
        },
    );
}

/// Native events cover normal resizes, DPI changes, and restoring saved layouts.
/// During custom dragging the resize loop emits the latest requested frame itself,
/// so a delayed platform Resized event cannot roll the renderer back one frame.
pub fn sync_viewport(window: &WebviewWindow, size: PhysicalSize<u32>) {
    let dpi = window.scale_factor().unwrap_or(1.0);
    let state = window.state::<WindowLayoutState>();
    let zoom = {
        let layouts = state.0.lock().unwrap();
        if layouts.resizing {
            return;
        }
        layouts.current.zoom
    };
    emit_viewport(window, size.width as f64, size.height as f64, dpi, zoom);
}

fn apply_controls(
    window: &WebviewWindow,
    monitor: &Monitor,
    profile: Profile,
) -> Result<(), String> {
    let space = available(monitor);
    window
        .set_min_size(Some(LogicalSize::new(
            (MIN_WIDTH * profile.zoom).min(space.0),
            (MIN_HEIGHT * profile.zoom).min(space.1),
        )))
        .map_err(|e| e.to_string())?;
    window
        .set_max_size(Some(LogicalSize::new(space.0, space.1)))
        .map_err(|e| e.to_string())?;
    window.set_zoom(profile.zoom).map_err(|e| e.to_string())
}

/// Called before show, using the same target monitor as position restoration.
pub fn prepare(window: &WebviewWindow, monitor: &Monitor) -> Result<Profile, String> {
    let id = monitor_id(window, monitor).unwrap_or_else(|| "primary".into());
    let state = window.state::<WindowLayoutState>();
    let mut layouts = state.0.lock().unwrap();
    load(&mut layouts);
    let profile = layouts
        .profiles
        .get(&id)
        .copied()
        .unwrap_or_default()
        .fit(available(monitor));
    layouts.active = Some(id);
    layouts.current = profile;
    layouts.generation = layouts.generation.wrapping_add(1);
    drop(layouts);
    apply_controls(window, monitor, profile)?;
    emit(window, monitor, profile);
    Ok(profile)
}

fn main_window(app: &tauri::AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("main")
        .ok_or_else(|| "Launcher window unavailable".into())
}

#[tauri::command]
pub fn get_launcher_layout(app: tauri::AppHandle) -> Result<LayoutInfo, String> {
    let window = main_window(&app)?;
    let monitor = window
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("No display")?;
    let id = monitor_id(&window, &monitor).unwrap_or_else(|| "primary".into());
    let size = window.inner_size().ok();
    let dpi = valid_scale_factor(window.scale_factor().unwrap_or(monitor.scale_factor()));
    let state = window.state::<WindowLayoutState>();
    let mut layouts = state.0.lock().unwrap();
    load(&mut layouts);
    let mut profile = layouts.profiles.get(&id).copied().unwrap_or_default();
    if layouts.active.as_ref() == Some(&id) {
        profile = layouts.current;
        if let Some(size) = size {
            profile.width = size.width as f64 / dpi;
            profile.height = size.height as f64 / dpi;
        }
    }
    drop(layouts);
    Ok(info(&window, &monitor, profile.fit(available(&monitor))))
}

#[tauri::command]
pub fn set_launcher_layout(
    app: tauri::AppHandle,
    width: Option<f64>,
    height: Option<f64>,
    zoom: Option<f64>,
    reset: Option<bool>,
) -> Result<LayoutInfo, String> {
    if [width, height, zoom]
        .iter()
        .flatten()
        .any(|v| !v.is_finite() || *v <= 0.0)
    {
        return Err("Dimensions and scale must be positive finite numbers".into());
    }
    let window = main_window(&app)?;
    let monitor = window
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("No display")?;
    let current = get_launcher_layout(app.clone())?.profile;
    let profile = if reset.unwrap_or(false) {
        Profile::default()
    } else {
        Profile {
            width: width.unwrap_or(current.width),
            height: height.unwrap_or(current.height),
            zoom: zoom.unwrap_or(current.zoom),
        }
    }
    .fit(available(&monitor));
    let id = monitor_id(&window, &monitor).unwrap_or_else(|| "primary".into());
    let state = window.state::<WindowLayoutState>();
    {
        let mut layouts = state.0.lock().unwrap();
        layouts.active = Some(id.clone());
        layouts.current = profile;
        layouts.profiles.insert(id, profile);
        layouts.generation = layouts.generation.wrapping_add(1);
        save(&layouts)?;
    }
    apply_controls(&window, &monitor, profile)?;
    window
        .set_size(LogicalSize::new(profile.width, profile.height))
        .map_err(|e| e.to_string())?;
    clamp_window(&window, &monitor, profile)?;
    emit(&window, &monitor, profile);
    schedule_capture(window.clone());
    Ok(info(&window, &monitor, profile))
}

fn clamp_window(window: &WebviewWindow, monitor: &Monitor, profile: Profile) -> Result<(), String> {
    if let Ok(position) = window.outer_position() {
        let dpi = valid_scale_factor(monitor.scale_factor());
        let next = super::clamp_position_to_work_area(
            super::PersistedWindowPosition {
                x: position.x as f64,
                y: position.y as f64,
            },
            physical_monitor_work_area(monitor),
            profile.width * dpi,
            profile.height * dpi,
        );
        super::set_window_position_on_monitor(window, next, monitor)?;
    }
    Ok(())
}

/// Debounce native moves/resizes. Programmatic placement and active drags are not user resizes.
pub fn schedule_capture(window: WebviewWindow) {
    let generation = {
        let state = window.state::<WindowLayoutState>();
        let mut layouts = state.0.lock().unwrap();
        layouts.generation = layouts.generation.wrapping_add(1);
        layouts.generation
    };
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let behavior = window.state::<super::WindowBehaviorState>();
        if !*behavior.main_window_placed.lock().unwrap()
            || *behavior.snap_drag_active.lock().unwrap()
            || !window.is_visible().unwrap_or(false)
        {
            return;
        }
        // Native getters dispatch to the UI thread. Never hold the layout lock
        // while calling them: a Moved/Resized handler also needs that lock.
        let Some(monitor) = window.current_monitor().ok().flatten() else {
            return;
        };
        let id = monitor_id(&window, &monitor).unwrap_or_else(|| "primary".into());
        let Ok(size) = window.inner_size() else {
            return;
        };
        let dpi = valid_scale_factor(window.scale_factor().unwrap_or(monitor.scale_factor()));
        let state = window.state::<WindowLayoutState>();
        let mut layouts = state.0.lock().unwrap();
        if layouts.generation != generation || layouts.resizing {
            return;
        }
        if layouts.active.as_ref() != Some(&id) {
            // A drag crossed displays: restore the destination's scale/size,
            // but keep the user's drop position rather than teleporting them.
            drop(layouts);
            if let Ok(profile) = prepare(&window, &monitor) {
                let _ = window.set_size(LogicalSize::new(profile.width, profile.height));
                let _ = clamp_window(&window, &monitor, profile);
                super::persist_current_window_position(&window);
            }
            return;
        }
        let profile = Profile {
            width: size.width as f64 / dpi,
            height: size.height as f64 / dpi,
            zoom: layouts.current.zoom,
        }
        .fit(available(&monitor));
        let changed = layouts.profiles.get(&id) != Some(&profile);
        if changed {
            layouts.current = profile;
            layouts.profiles.insert(id, profile);
            if let Err(error) = save(&layouts) {
                log::warn!("Failed to save launcher layout: {error}");
            }
        }
        drop(layouts);
        if changed {
            emit(&window, &monitor, profile);
        }
        super::persist_current_window_position(&window);
        let _ = super::sync_terminal_sessions_window(window.app_handle());
    });
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub enum ResizeEdge {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
}

/// Pure geometry; opposite edges remain anchored when a minimum/maximum is reached.
fn resized_rect(
    rect: (f64, f64, f64, f64),
    delta: (f64, f64),
    edge: ResizeEdge,
    minimum: (f64, f64),
    maximum: (f64, f64),
) -> (f64, f64, f64, f64) {
    let (mut x, mut y, width, height) = rect;
    let west = matches!(
        edge,
        ResizeEdge::West | ResizeEdge::NorthWest | ResizeEdge::SouthWest
    );
    let east = matches!(
        edge,
        ResizeEdge::East | ResizeEdge::NorthEast | ResizeEdge::SouthEast
    );
    let north = matches!(
        edge,
        ResizeEdge::North | ResizeEdge::NorthEast | ResizeEdge::NorthWest
    );
    let south = matches!(
        edge,
        ResizeEdge::South | ResizeEdge::SouthEast | ResizeEdge::SouthWest
    );
    let next_width = (width
        + if west {
            -delta.0
        } else if east {
            delta.0
        } else {
            0.0
        })
    .clamp(minimum.0.min(maximum.0), maximum.0);
    let next_height = (height
        + if north {
            -delta.1
        } else if south {
            delta.1
        } else {
            0.0
        })
    .clamp(minimum.1.min(maximum.1), maximum.1);
    if west {
        x += width - next_width;
    }
    if north {
        y += height - next_height;
    }
    (x, y, next_width, next_height)
}

#[cfg(target_os = "macos")]
fn resize_cursor() -> Option<(f64, f64, bool)> {
    use core_graphics::{
        event::CGEvent,
        event_source::{CGEventSource, CGEventSourceStateID},
    };
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
    }
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).ok()?;
    let position = CGEvent::new(source).ok()?.location();
    Some((position.x, position.y, unsafe {
        CGEventSourceButtonState(1, 0)
    }))
}

#[cfg(target_os = "windows")]
fn resize_cursor() -> Option<(f64, f64, bool)> {
    let position = super::windows_cursor_position().ok()?;
    Some((
        position.x,
        position.y,
        super::windows_left_mouse_button_is_down(),
    ))
}

#[tauri::command]
pub fn start_launcher_resize(window: WebviewWindow, edge: ResizeEdge) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Only the launcher can be resized here".into());
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let direction = match edge {
            ResizeEdge::North => tauri::ResizeDirection::North,
            ResizeEdge::NorthEast => tauri::ResizeDirection::NorthEast,
            ResizeEdge::East => tauri::ResizeDirection::East,
            ResizeEdge::SouthEast => tauri::ResizeDirection::SouthEast,
            ResizeEdge::South => tauri::ResizeDirection::South,
            ResizeEdge::SouthWest => tauri::ResizeDirection::SouthWest,
            ResizeEdge::West => tauri::ResizeDirection::West,
            ResizeEdge::NorthWest => tauri::ResizeDirection::NorthWest,
        };
        return window
            .start_resize_dragging(direction)
            .map_err(|e| e.to_string());
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let (cursor_x, cursor_y, down) = resize_cursor().ok_or("Cursor unavailable")?;
        if !down {
            return Ok(());
        }
        let monitor = window
            .current_monitor()
            .map_err(|e| e.to_string())?
            .ok_or("No display")?;
        let dpi = valid_scale_factor(window.scale_factor().unwrap_or(monitor.scale_factor()));
        let position = window.outer_position().map_err(|e| e.to_string())?;
        let size = window.inner_size().map_err(|e| e.to_string())?;
        let rect = (
            position.x as f64,
            position.y as f64,
            size.width as f64,
            size.height as f64,
        );
        let state = window.state::<WindowLayoutState>();
        let zoom = {
            let mut layouts = state.0.lock().unwrap();
            if layouts.resizing {
                return Ok(());
            }
            layouts.resizing = true;
            layouts.current.zoom
        };
        let behavior = window.state::<super::WindowBehaviorState>();
        #[cfg(target_os = "windows")]
        {
            let mut generation = behavior.window_placement_generation.lock().unwrap();
            *generation = generation.wrapping_add(1);
            *behavior.main_window_placed.lock().unwrap() = true;
        }
        let previous_suppression = *behavior.suppress_blur_hide.lock().unwrap();
        *behavior.suppress_blur_hide.lock().unwrap() = true;
        let (_, _, max_width, max_height) = physical_monitor_work_area(&monitor);
        tauri::async_runtime::spawn(async move {
            let mut previous = rect;
            // Track the physical mouse outside the WebView too. macOS has no
            // Tao resize-drag support; JS pointer deltas fail after leaving it.
            for _ in 0..3600 {
                let Some((x, y, down)) = resize_cursor() else {
                    break;
                };
                if !down || !window.is_visible().unwrap_or(false) {
                    break;
                }
                #[cfg(target_os = "macos")]
                let delta = ((x - cursor_x) * dpi, (y - cursor_y) * dpi);
                #[cfg(target_os = "windows")]
                let delta = (x - cursor_x, y - cursor_y);
                let next = resized_rect(
                    rect,
                    delta,
                    edge,
                    (MIN_WIDTH * zoom * dpi, MIN_HEIGHT * zoom * dpi),
                    (max_width, max_height),
                );
                if next != previous {
                    let _ = window.set_size(PhysicalSize::new(
                        next.2.round() as u32,
                        next.3.round() as u32,
                    ));
                    if next.0 != previous.0 || next.1 != previous.1 {
                        let _ = super::set_window_position_on_monitor(
                            &window,
                            super::PersistedWindowPosition {
                                x: next.0,
                                y: next.1,
                            },
                            &monitor,
                        );
                    }
                    // Do not wait for WKWebView's viewport or the debounced
                    // profile save. The footer must follow this frame now.
                    emit_viewport(&window, next.2, next.3, dpi, zoom);
                    previous = next;
                }
                tokio::time::sleep(Duration::from_millis(16)).await;
            }
            if let Ok(size) = window.inner_size() {
                emit_viewport(&window, size.width as f64, size.height as f64, dpi, zoom);
                let _ = clamp_window(
                    &window,
                    &monitor,
                    Profile {
                        width: size.width as f64 / dpi,
                        height: size.height as f64 / dpi,
                        zoom,
                    },
                );
            }
            window
                .state::<WindowLayoutState>()
                .0
                .lock()
                .unwrap()
                .resizing = false;
            *window
                .state::<super::WindowBehaviorState>()
                .suppress_blur_hide
                .lock()
                .unwrap() = previous_suppression;
            schedule_capture(window);
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fits_small_displays_and_invalid_values() {
        assert_eq!(
            Profile {
                width: f64::NAN,
                height: 5000.0,
                zoom: f64::INFINITY
            }
            .fit((600.0, 400.0)),
            Profile {
                width: 600.0,
                height: 400.0,
                zoom: 1.0
            }
        );
        assert_eq!(
            Profile {
                width: 1.0,
                height: 1.0,
                zoom: 2.0
            }
            .fit((800.0, 600.0)),
            Profile {
                width: 800.0,
                height: 600.0,
                zoom: 2.0
            }
        );
    }
    #[test]
    fn saved_logical_size_survives_mixed_dpi_transitions() {
        let profile = Profile {
            width: 610.0,
            height: 470.0,
            zoom: 1.25,
        };
        assert_eq!(profile.physical_size(1.0), (610.0, 470.0));
        assert_eq!(profile.physical_size(2.0), (1220.0, 940.0));
        assert_eq!(profile.physical_size(1.5), (915.0, 705.0));
        assert_eq!(profile.physical_size(f64::NAN), (610.0, 470.0));
    }

    #[test]
    fn a_corrupt_display_profile_does_not_destroy_other_displays() {
        let profiles = profiles_from_json(
            r#"{"bad":{"width":"wrong"},"good":{"width":600,"height":480,"zoom":1.5}}"#,
        );
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles["good"].width, 600.0);
        assert!(profiles_from_json("broken json").is_empty());
    }

    #[test]
    fn minimum_keeps_opposite_corner_anchored() {
        assert_eq!(
            resized_rect(
                (100.0, 200.0, 760.0, 640.0),
                (1000.0, 1000.0),
                ResizeEdge::NorthWest,
                (480.0, 360.0),
                (2000.0, 2000.0)
            ),
            (380.0, 480.0, 480.0, 360.0)
        );
    }
    #[test]
    fn all_eight_handles_resize_the_requested_edges() {
        let cases = [
            (ResizeEdge::North, (100.0, 280.0, 760.0, 560.0)),
            (ResizeEdge::NorthEast, (100.0, 280.0, 860.0, 560.0)),
            (ResizeEdge::East, (100.0, 200.0, 860.0, 640.0)),
            (ResizeEdge::SouthEast, (100.0, 200.0, 860.0, 720.0)),
            (ResizeEdge::South, (100.0, 200.0, 760.0, 720.0)),
            (ResizeEdge::SouthWest, (200.0, 200.0, 660.0, 720.0)),
            (ResizeEdge::West, (200.0, 200.0, 660.0, 640.0)),
            (ResizeEdge::NorthWest, (200.0, 280.0, 660.0, 560.0)),
        ];
        for (edge, expected) in cases {
            assert_eq!(
                resized_rect(
                    (100.0, 200.0, 760.0, 640.0),
                    (100.0, 80.0),
                    edge,
                    (480.0, 360.0),
                    (1920.0, 1080.0)
                ),
                expected,
                "{edge:?}"
            );
        }
    }

    #[test]
    fn edge_resizes_only_one_axis_and_maximum_is_enforced() {
        assert_eq!(
            resized_rect(
                (100.0, 200.0, 760.0, 640.0),
                (2000.0, 300.0),
                ResizeEdge::East,
                (480.0, 360.0),
                (1920.0, 1080.0)
            ),
            (100.0, 200.0, 1920.0, 640.0)
        );
    }
    #[test]
    fn profiles_round_trip_independently_of_dpi_and_other_displays() {
        let mut profiles = BTreeMap::new();
        profiles.insert(
            "retina".to_string(),
            Profile {
                width: 600.0,
                height: 480.0,
                zoom: 0.9,
            },
        );
        profiles.insert(
            "4k".to_string(),
            Profile {
                width: 1200.0,
                height: 900.0,
                zoom: 1.5,
            },
        );
        let restored: BTreeMap<String, Profile> =
            serde_json::from_str(&serde_json::to_string(&profiles).unwrap()).unwrap();
        assert_eq!(restored, profiles);
        assert_eq!(restored["4k"].fit((3840.0, 2160.0)).zoom, 1.5);
        assert_eq!(restored["retina"].fit((1512.0, 982.0)).width, 600.0);
    }
}
