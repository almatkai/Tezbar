//! OS coordinate lookup; frameworks supply only the typed point-to-monitor mapping.
#[cfg(target_os = "macos")]
use core_foundation::base::{kCFAllocatorDefault, CFType, TCFType};
#[cfg(target_os = "macos")]
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
#[cfg(target_os = "macos")]
use core_foundation::number::CFNumber;
#[cfg(target_os = "macos")]
use core_foundation::string::{CFString, CFStringRef};
#[cfg(target_os = "macos")]
use core_foundation::uuid::{CFUUIDCreateString, CFUUIDRef, CFUUID};
#[cfg(target_os = "macos")]
use core_graphics::display::{CGDirectDisplayID, CGDisplay};
#[cfg(target_os = "macos")]
use core_graphics::window::{
    copy_window_info, kCGWindowAlpha, kCGWindowBounds, kCGWindowLayer,
    kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly, kCGWindowOwnerName,
};

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGDisplayCreateUUIDFromDisplayID(display: CGDirectDisplayID) -> CFUUIDRef;
}

#[cfg(target_os = "macos")]
fn cf_type_for_static_key(key: CFStringRef) -> CFType {
    unsafe { CFString::wrap_under_get_rule(key).as_CFType() }
}

#[cfg(target_os = "macos")]
fn cf_type_for_string_key(key: &str) -> CFType {
    CFString::new(key).as_CFType()
}

#[cfg(target_os = "macos")]
fn dictionary_value_for_key(
    dictionary: &CFDictionary<CFType, CFType>,
    key: &CFType,
) -> Option<CFType> {
    dictionary.find(key).map(|value| (*value).clone())
}

#[cfg(target_os = "macos")]
fn dictionary_number_for_key(
    dictionary: &CFDictionary<CFType, CFType>,
    key: &CFType,
) -> Option<f64> {
    dictionary_value_for_key(dictionary, key)?
        .downcast::<CFNumber>()?
        .to_f64()
}

#[cfg(target_os = "macos")]
fn dictionary_string_for_key(
    dictionary: &CFDictionary<CFType, CFType>,
    key: &CFType,
) -> Option<String> {
    dictionary_value_for_key(dictionary, key)?
        .downcast::<CFString>()
        .map(|value| value.to_string())
}

#[cfg(target_os = "macos")]
fn dictionary_for_key(
    dictionary: &CFDictionary<CFType, CFType>,
    key: &CFType,
) -> Option<CFDictionary<CFType, CFType>> {
    let value = dictionary_value_for_key(dictionary, key)?;
    Some(unsafe {
        CFDictionary::<CFType, CFType>::wrap_under_get_rule(value.as_CFTypeRef() as CFDictionaryRef)
    })
}

#[cfg(target_os = "macos")]
pub fn display_uuid(display_id: CGDirectDisplayID) -> Option<String> {
    let uuid_ref = unsafe { CGDisplayCreateUUIDFromDisplayID(display_id) };
    if uuid_ref.is_null() {
        return None;
    }
    let uuid = unsafe { CFUUID::wrap_under_create_rule(uuid_ref) };
    let string_ref = unsafe { CFUUIDCreateString(kCFAllocatorDefault, uuid.as_concrete_TypeRef()) };
    if string_ref.is_null() {
        return None;
    }
    let uuid_string = unsafe { CFString::wrap_under_create_rule(string_ref) };
    Some(format!("macos:{}", uuid_string))
}

#[cfg(target_os = "macos")]
pub fn frontmost<T>(mut resolve: impl FnMut(f64, f64) -> Option<T>) -> Option<T> {
    let window_infos = copy_window_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        0,
    )?;
    let layer_key = cf_type_for_static_key(unsafe { kCGWindowLayer });
    let alpha_key = cf_type_for_static_key(unsafe { kCGWindowAlpha });
    let owner_key = cf_type_for_static_key(unsafe { kCGWindowOwnerName });
    let bounds_key = cf_type_for_static_key(unsafe { kCGWindowBounds });
    let x_key = cf_type_for_string_key("X");
    let y_key = cf_type_for_string_key("Y");
    let width_key = cf_type_for_string_key("Width");
    let height_key = cf_type_for_string_key("Height");

    for item in window_infos.iter() {
        let dictionary = unsafe {
            CFDictionary::<CFType, CFType>::wrap_under_get_rule(*item as CFDictionaryRef)
        };
        let Some(layer) = dictionary_number_for_key(&dictionary, &layer_key) else {
            continue;
        };
        if layer.round() as i32 != 0 {
            continue;
        }
        if dictionary_number_for_key(&dictionary, &alpha_key).unwrap_or(0.0) <= 0.0 {
            continue;
        }
        let owner = dictionary_string_for_key(&dictionary, &owner_key).unwrap_or_default();
        if owner.is_empty()
            || owner == "Tezbar"
            || owner == "Raymes"
            || owner == "Dock"
            || owner == "Window Server"
        {
            continue;
        }
        let Some(bounds) = dictionary_for_key(&dictionary, &bounds_key) else {
            continue;
        };
        let Some(x) = dictionary_number_for_key(&bounds, &x_key) else {
            continue;
        };
        let Some(y) = dictionary_number_for_key(&bounds, &y_key) else {
            continue;
        };
        let Some(width) = dictionary_number_for_key(&bounds, &width_key) else {
            continue;
        };
        let Some(height) = dictionary_number_for_key(&bounds, &height_key) else {
            continue;
        };
        if width < 40.0 || height < 40.0 {
            continue;
        }
        if let Some(monitor) = resolve(x + width / 2.0, y + height / 2.0) {
            return Some(monitor);
        }
    }
    None
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn frontmost<T>(_resolve: impl FnMut(f64, f64) -> Option<T>) -> Option<T> {
    None
}

#[cfg(target_os = "windows")]
pub fn frontmost<T>(mut resolve: impl FnMut(f64, f64) -> Option<T>) -> Option<T> {
    use std::mem::size_of;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let foreground_window = unsafe { GetForegroundWindow() };
    if foreground_window.is_null() {
        return None;
    }

    let monitor_handle = unsafe { MonitorFromWindow(foreground_window, MONITOR_DEFAULTTONEAREST) };
    if monitor_handle.is_null() {
        return None;
    }

    let mut monitor_info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        rcMonitor: unsafe { std::mem::zeroed() },
        rcWork: unsafe { std::mem::zeroed() },
        dwFlags: 0,
    };
    if unsafe { GetMonitorInfoW(monitor_handle, &mut monitor_info) } == 0 {
        return None;
    }

    // Tauri's monitor_from_point uses the same physical desktop coordinate
    // space as the Win32 monitor rectangles. Use the foreground monitor's
    // center to map the native handle back to Tauri's Monitor value.
    let center_x = (i64::from(monitor_info.rcMonitor.left)
        + i64::from(monitor_info.rcMonitor.right)) as f64
        / 2.0;
    let center_y = (i64::from(monitor_info.rcMonitor.top)
        + i64::from(monitor_info.rcMonitor.bottom)) as f64
        / 2.0;
    resolve(center_x, center_y)
}
#[cfg(target_os = "macos")]
pub fn cursor_point() -> Option<(f64, f64)> {
    use core_graphics::{
        event::CGEvent,
        event_source::{CGEventSource, CGEventSourceStateID},
    };
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).ok()?;
    let point = CGEvent::new(source).ok()?.location();
    Some((point.x, point.y))
}
#[cfg(windows)]
pub fn cursor_point() -> Option<(f64, f64)> {
    let mut point = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
    if unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut point) } == 0 {
        None
    } else {
        Some((point.x as f64, point.y as f64))
    }
}
