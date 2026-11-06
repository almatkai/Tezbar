//! Raw handle operations missing in GPUI: per-window hide, placement and click-through overlay.
use gpui::{Bounds, Pixels, Window};
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
                    if show { 5 } else { 0 },
                );
            }
            return Ok(());
        }
    }
    #[allow(unreachable_code)]
    Err("Native window visibility unavailable".into())
}
pub fn place(w: &mut Window, b: Bounds<Pixels>) -> Result<(), String> {
    let scale = w.scale_factor() as f64;
    place_scaled(w, b, scale)
}
pub fn place_scaled(w: &mut Window, b: Bounds<Pixels>, _scale: f64) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc2_foundation::{NSPoint, NSRect, NSSize};
        let window = ns_window(w)?;
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
        let _: () = objc2::msg_send![window,setFrame:rect, display:true];
        return Ok(());
    }
    #[cfg(windows)]
    {
        if let RawWindowHandle::Win32(h) = HasWindowHandle::window_handle(w)
            .map_err(|e| e.to_string())?
            .as_raw()
        {
            let dpi = tezbar_host::placement::valid_scale_factor(_scale);
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::SetWindowPos(
                    h.hwnd.get() as _,
                    std::ptr::null_mut(),
                    (f64::from(b.origin.x) * dpi) as i32,
                    (f64::from(b.origin.y) * dpi) as i32,
                    (f64::from(b.size.width) * dpi) as i32,
                    (f64::from(b.size.height) * dpi) as i32,
                    0x0010 | 0x0004,
                );
            }
            return Ok(());
        }
    }
    #[allow(unreachable_code)]
    {
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
