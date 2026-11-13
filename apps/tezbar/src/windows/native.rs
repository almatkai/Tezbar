//! Raw handle operations missing in GPUI: per-window hide, placement and click-through overlay.
use gpui::{App, Bounds, Pixels, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
#[cfg(target_os = "macos")]
unsafe fn ns_window(w: &Window) -> Result<*mut objc2_app_kit::NSWindow, String> {
    let h = HasWindowHandle::window_handle(w).map_err(|e| e.to_string())?;
    if let RawWindowHandle::AppKit(h) = h.as_raw() {
        let view = h.ns_view.as_ptr() as *mut objc2_app_kit::NSView;
        let window: *mut objc2_app_kit::NSWindow = unsafe { objc2::msg_send![view, window] };
        if !window.is_null() {
            return Ok(window);
        }
    }
    Err("NSWindow handle unavailable".into())
}
pub fn visible(w: &Window, show: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    unsafe {
        let window = ns_window(w)?;
        if show {
            let _: () = objc2::msg_send![window, orderFrontRegardless];
        } else {
            let _: () =
                objc2::msg_send![window,orderOut:std::ptr::null::<objc2::runtime::AnyObject>()];
        }
        return Ok(());
    }
    #[cfg(windows)]
    {
        if let RawWindowHandle::Win32(h) = HasWindowHandle::window_handle(w)
            .map_err(|e| e.to_string())?
            .as_raw()
        {
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
                    h.hwnd.get() as _,
                    if show { 8 } else { 0 },
                );
            }
            return Ok(());
        }
    }
    #[allow(unreachable_code)]
    Err("Native window visibility unavailable".into())
}
pub fn place(w: &mut Window, b: Bounds<Pixels>, cx: &mut App) -> Result<(), String> {
    let scale = w.scale_factor() as f64;
    place_scaled(w, b, scale, cx)
}
pub fn place_scaled(
    w: &mut Window,
    b: Bounds<Pixels>,
    _scale: f64,
    cx: &mut App,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc2_foundation::{NSPoint, NSRect, NSSize};
        let window = objc2::rc::Retained::retain(ns_window(w)?).ok_or("NSWindow was released")?;
        let mtm = objc2::MainThreadMarker::new().ok_or("placement requires GUI thread")?;
        let screens = objc2_app_kit::NSScreen::screens(mtm);
        let primary = screens.firstObject().ok_or("No screen")?;
        let height = primary.frame().size.height;
        let rect = NSRect::new(
            NSPoint::new(
                f64::from(b.origin.x),
                height - f64::from(b.origin.y) - f64::from(b.size.height),
            ),
            NSSize::new(f64::from(b.size.width), f64::from(b.size.height)),
        );
        dispatch_placement(w, b, cx, move || {
            let current: NSRect = objc2::msg_send![&*window, frame];
            apply_changed_geometry(
                [
                    current.origin.x,
                    current.origin.y,
                    current.size.width,
                    current.size.height,
                ],
                [
                    rect.origin.x,
                    rect.origin.y,
                    rect.size.width,
                    rect.size.height,
                ],
                || {
                    let _: () = objc2::msg_send![&*window,setFrame:rect, display:true];
                },
            );
        });
        return Ok(());
    }
    #[cfg(windows)]
    {
        if let RawWindowHandle::Win32(h) = HasWindowHandle::window_handle(w)
            .map_err(|e| e.to_string())?
            .as_raw()
        {
            let dpi = tezbar_host::placement::valid_scale_factor(_scale);
            let hwnd = h.hwnd.get();
            dispatch_placement(w, b, cx, move || unsafe {
                let target = [
                    (f64::from(b.origin.x) * dpi) as i32,
                    (f64::from(b.origin.y) * dpi) as i32,
                    (f64::from(b.size.width) * dpi) as i32,
                    (f64::from(b.size.height) * dpi) as i32,
                ];
                let mut current: windows_sys::Win32::Foundation::RECT = std::mem::zeroed();
                if windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect(
                    hwnd as _,
                    &mut current,
                ) != 0
                    && same_geometry(
                        [
                            current.left as f64,
                            current.top as f64,
                            (current.right - current.left) as f64,
                            (current.bottom - current.top) as f64,
                        ],
                        target.map(f64::from),
                    )
                {
                    return;
                }
                let result = windows_sys::Win32::UI::WindowsAndMessaging::SetWindowPos(
                    hwnd as _,
                    std::ptr::null_mut(),
                    target[0],
                    target[1],
                    target[2],
                    target[3],
                    0x0010 | 0x0004,
                );
                if result == 0 {
                    log::error!(
                        "Native window placement failed: {}",
                        std::io::Error::last_os_error()
                    );
                }
            });
            return Ok(());
        }
    }
    #[allow(unreachable_code)]
    {
        let _ = cx;
        w.resize(b.size);
        Err("Native placement unavailable".into())
    }
}
pub fn passthrough(w: &Window) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    unsafe {
        let window = ns_window(w)?;
        let _: () = objc2::msg_send![window,setIgnoresMouseEvents:true];
        return Ok(());
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        if let RawWindowHandle::Win32(h) = HasWindowHandle::window_handle(w)
            .map_err(|e| e.to_string())?
            .as_raw()
        {
            unsafe {
                let hwnd = h.hwnd.get() as _;
                let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                SetWindowLongPtrW(
                    hwnd,
                    GWL_EXSTYLE,
                    style
                        | WS_EX_TRANSPARENT as isize
                        | WS_EX_LAYERED as isize
                        | WS_EX_NOACTIVATE as isize,
                );
                SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
            }
            return Ok(());
        }
    }
    #[allow(unreachable_code)]
    Err("Overlay pass-through unavailable".into())
}
/// GPUI display bounds include menu bars/docks; restore and clamp against the OS work area.
pub fn work_area(bounds: Bounds<Pixels>) -> (f64, f64, f64, f64) {
    #[cfg(target_os = "macos")]
    if let Some(mtm) = objc2::MainThreadMarker::new() {
        let screens = objc2_app_kit::NSScreen::screens(mtm);
        if let Some(primary) = screens.firstObject() {
            let primary_height = primary.frame().size.height;
            for screen in screens.iter() {
                let frame = screen.frame();
                let y = primary_height - frame.origin.y - frame.size.height;
                if (frame.origin.x - f64::from(bounds.left())).abs() < 2.
                    && (y - f64::from(bounds.top())).abs() < 2.
                {
                    let work = screen.visibleFrame();
                    return (
                        work.origin.x,
                        primary_height - work.origin.y - work.size.height,
                        work.size.width,
                        work.size.height,
                    );
                }
            }
        }
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Graphics::Gdi::*;
        let center = windows_sys::Win32::Foundation::POINT {
            x: f64::from(bounds.center().x) as i32,
            y: f64::from(bounds.center().y) as i32,
        };
        let monitor = MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) != 0 {
            let r = info.rcWork;
            return (
                r.left as f64,
                r.top as f64,
                (r.right - r.left) as f64,
                (r.bottom - r.top) as f64,
            );
        }
    }
    (
        f64::from(bounds.left()),
        f64::from(bounds.top()),
        f64::from(bounds.size.width),
        f64::from(bounds.size.height),
    )
}
/// Tokens never leave the GUI thread and are valid only while the owning GPUI window is retained.
pub fn token(w: &Window) -> Result<isize, String> {
    #[cfg(target_os = "macos")]
    unsafe {
        return Ok(ns_window(w)? as isize);
    }
    #[cfg(windows)]
    {
        if let RawWindowHandle::Win32(h) = HasWindowHandle::window_handle(w)
            .map_err(|e| e.to_string())?
            .as_raw()
        {
            return Ok(h.hwnd.get());
        }
    }
    #[allow(unreachable_code)]
    Err("Native parent handle unavailable".into())
}
pub fn attach(child: &Window, parent: isize) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    unsafe {
        let child = ns_window(child)?;
        let parent = parent as *mut objc2_app_kit::NSWindow;
        let _: () = objc2::msg_send![parent,addChildWindow:child,ordered:1isize];
        return Ok(());
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let child = token(child)?;
        unsafe {
            SetWindowLongPtrW(child as _, GWLP_HWNDPARENT, parent);
        }
        return Ok(());
    }
    #[allow(unreachable_code)]
    Err("Native parenting unavailable".into())
}

/// Windows DisplayId at this pin is the HMONITOR; cursor/frontmost points are physical desktop
/// coordinates, unlike GPUI display bounds. Resolve by that identity instead of comparing units.
pub fn display_at(
    displays: &[std::rc::Rc<dyn gpui::PlatformDisplay>],
    x: f64,
    y: f64,
) -> Option<std::rc::Rc<dyn gpui::PlatformDisplay>> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Graphics::Gdi::*;
        let point = windows_sys::Win32::Foundation::POINT {
            x: x as i32,
            y: y as i32,
        };
        let handle = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        return displays
            .iter()
            .find(|d| u64::from(d.id()) as usize == handle as usize)
            .cloned();
    }
    #[cfg(not(windows))]
    displays
        .iter()
        .find(|d| {
            let b = d.bounds();
            x >= f64::from(b.left())
                && x < f64::from(b.right())
                && y >= f64::from(b.top())
                && y < f64::from(b.bottom())
        })
        .cloned()
}
pub fn display_scale(display: &dyn gpui::PlatformDisplay) -> f64 {
    let bounds = display.bounds();
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Graphics::Gdi::*;
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(u64::from(display.id()) as usize as _, &mut info) != 0 {
            return tezbar_host::placement::valid_scale_factor(
                (info.rcMonitor.right - info.rcMonitor.left) as f64 / f64::from(bounds.size.width),
            );
        }
    }
    #[cfg(target_os = "macos")]
    if let Some(mtm) = objc2::MainThreadMarker::new() {
        let screens = objc2_app_kit::NSScreen::screens(mtm);
        if let Some(primary) = screens.firstObject() {
            let height = primary.frame().size.height;
            for screen in screens.iter() {
                let frame = screen.frame();
                if (frame.origin.x - f64::from(bounds.left())).abs() < 2.
                    && (height - frame.origin.y - frame.size.height - f64::from(bounds.top())).abs()
                        < 2.
                {
                    return screen.backingScaleFactor();
                }
            }
        }
    }
    1.
}
pub fn monitor_key(display: &dyn gpui::PlatformDisplay) -> String {
    #[cfg(target_os = "macos")]
    if let Some(key) = tezbar_host::monitors::display_uuid(u64::from(display.id()) as u32) {
        return key;
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Graphics::Gdi::*;
        let mut info: MONITORINFOEXW = std::mem::zeroed();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(u64::from(display.id()) as usize as _, &mut info.monitorInfo) != 0 {
            let len = info
                .szDevice
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(info.szDevice.len());
            return String::from_utf16_lossy(&info.szDevice[..len]);
        }
    }
    display
        .uuid()
        .map(|u| u.to_string())
        .unwrap_or_else(|_| format!("display-{:?}", display.id()))
}
pub fn physical_rect(bounds: Bounds<Pixels>, scale: f64) -> (f64, f64, f64, f64) {
    let scale = tezbar_host::placement::valid_scale_factor(scale);
    (
        f64::from(bounds.left()) * scale,
        f64::from(bounds.top()) * scale,
        f64::from(bounds.size.width) * scale,
        f64::from(bounds.size.height) * scale,
    )
}
#[cfg(test)]
mod coordinate_tests {
    use super::*;
    #[test]
    fn physical_snapshots_keep_negative_monitor_origins_and_dpi() {
        let bounds = gpui::Bounds::new(
            gpui::point(gpui::px(-1280.), gpui::px(40.)),
            gpui::size(gpui::px(760.), gpui::px(640.)),
        );
        assert_eq!(physical_rect(bounds, 1.5), (-1920., 60., 1140., 960.));
    }
}
/// macOS restores the previous app through App::hide; Windows retains the prior foreground HWND.
pub fn foreground() -> Option<isize> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        return (pid != std::process::id()).then_some(hwnd as isize);
    }
    #[cfg(not(windows))]
    None
}
pub fn restore_foreground(_previous: Option<isize>) {
    #[cfg(windows)]
    if let Some(hwnd) = _previous {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd as _);
        }
    }
}

pub fn no_shadow(w: &Window) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    unsafe {
        let window = ns_window(w)?;
        let _: () = objc2::msg_send![window,setHasShadow:false];
    }
    #[cfg(windows)]
    unsafe {
        #[link(name = "dwmapi")]
        unsafe extern "system" {
            fn DwmSetWindowAttribute(
                hwnd: *mut std::ffi::c_void,
                attribute: u32,
                value: *const std::ffi::c_void,
                size: u32,
            ) -> i32;
        }
        let policy: u32 = 1; // DWMNCRP_DISABLED: no native frame/shadow on this child only.
        let result = DwmSetWindowAttribute(token(w)? as _, 2, &policy as *const _ as _, 4);
        if result < 0 {
            return Err(format!("DWM no-shadow failed: {result}"));
        }
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    let _ = w;
    Ok(())
}
/// Windows 11 draws its own 1px square border around popup windows. Tezbar draws one
/// rounded GPUI frame inside a transparent window, so the system border and corner
/// treatment are removed to leave a single frame (no-op before Windows 11 / elsewhere).
pub fn frameless(w: &Window) -> Result<(), String> {
    #[cfg(windows)]
    unsafe {
        #[link(name = "dwmapi")]
        unsafe extern "system" {
            fn DwmSetWindowAttribute(
                hwnd: *mut std::ffi::c_void,
                attribute: u32,
                value: *const std::ffi::c_void,
                size: u32,
            ) -> i32;
        }
        const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
        const DWMWA_BORDER_COLOR: u32 = 34;
        const DWMWCP_DONOTROUND: u32 = 1;
        const DWMWA_COLOR_NONE: u32 = 0xFFFF_FFFE;
        let hwnd = token(w)? as *mut std::ffi::c_void;
        let corner = DWMWCP_DONOTROUND;
        let border = DWMWA_COLOR_NONE;
        // Older builds reject these attributes; they also have no border to remove.
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const _ as _, 4);
        let result = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &border as *const _ as _, 4);
        if result < 0 {
            return Err(format!("DWM border removal failed: {result}"));
        }
    }
    #[cfg(not(windows))]
    let _ = w;
    Ok(())
}
/// Physical button state ends native move loops even when AppKit consumes mouse-up.
pub fn left_button_down() -> bool {
    #[cfg(target_os = "macos")]
    unsafe {
        let cls = objc2::runtime::AnyClass::get(c"NSEvent").unwrap();
        let buttons: usize = objc2::msg_send![cls, pressedMouseButtons];
        return buttons & 1 != 0;
    }
    #[cfg(windows)]
    unsafe {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetAsyncKeyState(key: i32) -> i16;
        }
        return GetAsyncKeyState(1) < 0;
    }
    #[allow(unreachable_code)]
    false
}

pub fn reduce_motion() -> bool {
    #[cfg(target_os = "macos")]
    unsafe {
        let cls = objc2::runtime::AnyClass::get(c"NSWorkspace").unwrap();
        let workspace: *mut objc2::runtime::AnyObject = objc2::msg_send![cls, sharedWorkspace];
        return objc2::msg_send![workspace, accessibilityDisplayShouldReduceMotion];
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::SystemParametersInfoW;
        let mut enabled: i32 = 1;
        if SystemParametersInfoW(0x1042, 0, &mut enabled as *mut _ as _, 0) != 0 {
            return enabled == 0;
        }
    }
    #[allow(unreachable_code)]
    false
}

// AppKit can round frames to device pixels. Ignore sub-hundredth-point noise,
// but compare at execution time so a newest no-op request cancels stale queued work.
fn same_geometry(current: [f64; 4], target: [f64; 4]) -> bool {
    current
        .into_iter()
        .zip(target)
        .all(|(current, target)| (current - target).abs() < 0.01)
}
fn apply_changed_geometry(current: [f64; 4], target: [f64; 4], apply: impl FnOnce()) {
    if !same_geometry(current, target) {
        apply();
    }
}

/// One foreground tick applies the latest requested placement per live GPUI window.
/// Native move/resize callbacks synchronously update App, so applying inside any
/// App/window update loses those callbacks to the App RefCell borrow guard.
struct PendingPlacement {
    bounds: Bounds<Pixels>,
    apply: Box<dyn FnOnce()>,
}
#[derive(Default)]
struct PendingPlacements(std::collections::HashMap<gpui::WindowId, PendingPlacement>);
impl gpui::Global for PendingPlacements {}
pub fn requested_bounds(w: &Window, cx: &App) -> Bounds<Pixels> {
    cx.try_global::<PendingPlacements>()
        .and_then(|pending| pending.0.get(&gpui::Window::window_handle(w).window_id()))
        .map_or_else(|| w.bounds(), |pending| pending.bounds)
}
/// Persist OS-reported monitor identity only after pending placement has landed.
pub fn placement_pending(w: &Window, cx: &App) -> bool {
    cx.try_global::<PendingPlacements>().is_some_and(|pending| {
        pending
            .0
            .contains_key(&gpui::Window::window_handle(w).window_id())
    })
}
fn dispatch_placement(
    w: &Window,
    bounds: Bounds<Pixels>,
    cx: &mut App,
    apply: impl FnOnce() + 'static,
) {
    let handle = gpui::Window::window_handle(w);
    let id = handle.window_id();
    if !cx.has_global::<PendingPlacements>() {
        cx.set_global(PendingPlacements::default());
    }
    let already_scheduled = cx
        .global_mut::<PendingPlacements>()
        .0
        .insert(
            id,
            PendingPlacement {
                bounds,
                apply: Box::new(apply),
            },
        )
        .is_some();
    if already_scheduled {
        return;
    }
    cx.spawn(async move |cx| {
        // Remove before checking liveness or applying. A callback which schedules
        // another placement gets a fresh task; closed windows leave no pending entry.
        let apply = cx.update(|cx| cx.global_mut::<PendingPlacements>().0.remove(&id));
        let Some(apply) = apply else {
            return;
        };
        if let Err(error) = handle.update(cx, |_, _, _| ()) {
            log::debug!("Skipped placement for closed window {id:?}: {error}");
            return;
        }
        // Both GPUI updates above have returned. Do not wrap this in cx.update.
        (apply.apply)();
    })
    .detach();
}

#[cfg(test)]
mod placement_tests {
    use super::*;
    use ::core::prelude::v1::test;
    use gpui::{prelude::*, *};
    use std::{cell::Cell, rc::Rc};
    struct Root;
    impl Render for Root {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full()
        }
    }
    #[gpui::test]
    fn native_resize_callback_runs_after_app_update_and_updates_viewport(cx: &mut TestAppContext) {
        let simulator = cx.clone();
        let (_, cx) = cx.add_window_view(|_, _| Root);
        let handle = cx.update(|w, _| gpui::Window::window_handle(w));
        let ran = Rc::new(Cell::new(false));
        let performed = ran.clone();
        let target = size(px(900.), px(800.));
        cx.update(|w, app| {
            dispatch_placement(w, w.bounds(), app, move || {
                performed.set(true);
                simulator.simulate_window_resize(handle, target);
            });
            assert!(
                !ran.get(),
                "Native resize must not reenter GPUI while App is borrowed"
            );
        });
        cx.run_until_parked();
        assert!(ran.get());
        assert_eq!(cx.update(|w, _| w.viewport_size()), target);
    }
    #[gpui::test]
    fn pending_placement_coalesces_and_closed_window_drops_retained_work(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| Root);
        let applied = Rc::new(Cell::new(0));
        cx.update(|w, app| {
            let first = applied.clone();
            dispatch_placement(w, w.bounds(), app, move || first.set(1));
            let latest = applied.clone();
            dispatch_placement(w, w.bounds(), app, move || latest.set(2));
            assert_eq!(app.global::<PendingPlacements>().0.len(), 1);
        });
        cx.run_until_parked();
        assert_eq!(
            applied.get(),
            2,
            "Only the newest placement should be applied"
        );
        cx.update(|w, app| {
            let closed = applied.clone();
            dispatch_placement(w, w.bounds(), app, move || closed.set(3));
            w.remove_window();
        });
        cx.run_until_parked();
        assert_eq!(
            applied.get(),
            2,
            "Closed windows must not receive native mutations"
        );
        cx.read(|app| assert!(app.global::<PendingPlacements>().0.is_empty()));
    }
    #[gpui::test]
    fn placement_coalescing_is_per_window_and_callback_can_queue_next_tick(
        cx: &mut TestAppContext,
    ) {
        let first = cx.add_window(|_, _| Root);
        let second = cx.add_window(|_, _| Root);
        let mut scheduler = cx.clone();
        let order = Rc::new(std::cell::RefCell::new(Vec::new()));
        first
            .update(cx, |_, w, app| {
                let stale = order.clone();
                dispatch_placement(w, w.bounds(), app, move || stale.borrow_mut().push("stale"));
                let applied = order.clone();
                dispatch_placement(w, w.bounds(), app, move || {
                    applied.borrow_mut().push("first");
                    let next = applied.clone();
                    first
                        .update(&mut scheduler, |_, w, app| {
                            dispatch_placement(w, w.bounds(), app, move || {
                                next.borrow_mut().push("next")
                            })
                        })
                        .unwrap();
                });
            })
            .unwrap();
        second
            .update(cx, |_, w, app| {
                let applied = order.clone();
                dispatch_placement(w, w.bounds(), app, move || {
                    applied.borrow_mut().push("second")
                });
                assert_eq!(app.global::<PendingPlacements>().0.len(), 2);
            })
            .unwrap();
        cx.run_until_parked();
        let values = order.borrow();
        assert_eq!(values.len(), 3);
        assert!(!values.contains(&"stale"));
        assert!(values.contains(&"second"));
        assert!(
            values.iter().position(|v| *v == "first") < values.iter().position(|v| *v == "next")
        );
        cx.read(|app| assert!(app.global::<PendingPlacements>().0.is_empty()));
    }
    #[gpui::test]
    fn newest_noop_cancels_older_resize_and_equal_native_frame_cannot_feedback(
        cx: &mut TestAppContext,
    ) {
        let (_, cx) = cx.add_window_view(|_, _| Root);
        let native_frame = Rc::new(Cell::new([-1280., 40., 1097., 647.]));
        let changes = Rc::new(Cell::new(0));
        let original = native_frame.get();
        cx.update(|w, app| {
            let frame = native_frame.clone();
            let count = changes.clone();
            dispatch_placement(w, w.bounds(), app, move || {
                apply_changed_geometry(frame.get(), [-1280., 40., 1097., 430.], || {
                    count.set(count.get() + 1)
                })
            });
            let frame = native_frame.clone();
            let count = changes.clone();
            dispatch_placement(w, w.bounds(), app, move || {
                apply_changed_geometry(frame.get(), original, || count.set(count.get() + 1))
            });
        });
        cx.run_until_parked();
        assert_eq!(
            changes.get(),
            0,
            "Newest same-frame target must cancel a stale queued resize"
        );
        apply_changed_geometry(original, [-1280., 40., 1097., 430.], || {
            changes.set(changes.get() + 1)
        });
        assert_eq!(changes.get(), 1, "A real height change must still resize");
        apply_changed_geometry(original, [-1280.001, 40., 1097., 647.], || {
            changes.set(changes.get() + 1)
        });
        assert_eq!(
            changes.get(),
            1,
            "Subpixel same-frame notifications must not feed back into native resize"
        );
    }
    #[gpui::test]
    fn pending_restore_and_partial_layouts_use_target_bounds_before_native_callback(
        cx: &mut TestAppContext,
    ) {
        let simulator = cx.clone();
        let (_, cx) = cx.add_window_view(|_, _| Root);
        let handle = cx.update(|w, _| gpui::Window::window_handle(w));
        cx.update(|w, app| {
            let initial = w.bounds();
            let restore = Bounds::new(point(px(-1280.), px(40.)), size(px(1097.), px(647.)));
            dispatch_placement(w, restore, app, || {});
            assert_eq!(
                requested_bounds(w, app),
                restore,
                "Restore target is available before native resize"
            );
            assert!(
                placement_pending(w, app),
                "Monitor/position snapshot must wait for actual crossscreen placement"
            );
            let mut height_only = requested_bounds(w, app);
            height_only.size.height = px(430.);
            dispatch_placement(w, height_only, app, || {});
            let mut width_only = requested_bounds(w, app);
            width_only.size.width = px(1000.);
            let final_target = width_only;
            dispatch_placement(w, final_target, app, move || {
                simulator.simulate_window_resize(handle, final_target.size)
            });
            assert_eq!(
                requested_bounds(w, app).size,
                size(px(1000.), px(430.)),
                "Partial updates preserve the latest pending other dimension"
            );
            assert_eq!(
                w.bounds(),
                initial,
                "The OS has not applied any queued placement during App update"
            );
            let (_, position, profile) = crate::windows::persistence::capture(
                "target-monitor".into(),
                2.,
                requested_bounds(w, app),
                0.85,
            );
            assert_eq!(
                (profile.width, profile.height, profile.zoom),
                (1000., 430., f64::from(0.85_f32))
            );
            assert_eq!((position.x, position.y), (-2560., 80.));
        });
        cx.run_until_parked();
        cx.update(|w, app| {
            assert!(!placement_pending(w, app));
            assert_eq!(w.viewport_size(), size(px(1000.), px(430.)));
            assert_eq!(requested_bounds(w, app), w.bounds());
        });
    }
}
