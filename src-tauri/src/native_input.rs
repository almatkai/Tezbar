// src-tauri/src/native_input.rs
#[cfg(target_os = "macos")]
mod platform {
    use std::collections::HashSet;
    use std::io::Cursor;

    use core_graphics::display::CGDisplay;
    use core_graphics::event::{
        CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGKeyCode, CGMouseButton,
        EventField, ScrollEventUnit,
    };
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use core_graphics::geometry::CGPoint;
    use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
    use tauri::WebviewWindow;

    const KCG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE: u32 = 1;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceKeyState(state_id: u32, key: CGKeyCode) -> bool;
    }

    fn physical_key_is_down(code: CGKeyCode) -> bool {
        unsafe { CGEventSourceKeyState(KCG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE, code) }
    }

    fn event_source() -> Result<CGEventSource, String> {
        CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| "failed to create CGEventSource".to_string())
    }

    fn post_mouse_event(
        event_type: CGEventType,
        x: f64,
        y: f64,
        button: CGMouseButton,
    ) -> Result<(), String> {
        let source = event_source()?;
        let event = CGEvent::new_mouse_event(source, event_type, CGPoint::new(x, y), button)
            .map_err(|_| "failed to create mouse event".to_string())?;
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    #[tauri::command]
    pub fn move_mouse(x: f64, y: f64) -> Result<(), String> {
        post_mouse_event(CGEventType::MouseMoved, x, y, CGMouseButton::Left)
    }

    fn parse_button(button: &str) -> Result<CGMouseButton, String> {
        match button {
            "left" => Ok(CGMouseButton::Left),
            "right" => Ok(CGMouseButton::Right),
            _ => Err("button must be 'left' or 'right'".to_string()),
        }
    }

    #[tauri::command]
    pub fn click(x: f64, y: f64, button: String) -> Result<(), String> {
        let btn = parse_button(&button)?;
        let (down, up) = match btn {
            CGMouseButton::Left => (CGEventType::LeftMouseDown, CGEventType::LeftMouseUp),
            CGMouseButton::Right => (CGEventType::RightMouseDown, CGEventType::RightMouseUp),
            _ => (CGEventType::LeftMouseDown, CGEventType::LeftMouseUp),
        };

        post_mouse_event(down, x, y, btn)?;
        post_mouse_event(up, x, y, btn)
    }

    #[tauri::command]
    pub fn double_click(x: f64, y: f64) -> Result<(), String> {
        let source = event_source()?;

        let down_1 = CGEvent::new_mouse_event(
            source.clone(),
            CGEventType::LeftMouseDown,
            CGPoint::new(x, y),
            CGMouseButton::Left,
        )
        .map_err(|_| "failed to create first down event".to_string())?;
        down_1.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, 2);
        down_1.post(CGEventTapLocation::HID);

        let up_1 = CGEvent::new_mouse_event(
            source.clone(),
            CGEventType::LeftMouseUp,
            CGPoint::new(x, y),
            CGMouseButton::Left,
        )
        .map_err(|_| "failed to create first up event".to_string())?;
        up_1.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, 2);
        up_1.post(CGEventTapLocation::HID);

        let down_2 = CGEvent::new_mouse_event(
            source.clone(),
            CGEventType::LeftMouseDown,
            CGPoint::new(x, y),
            CGMouseButton::Left,
        )
        .map_err(|_| "failed to create second down event".to_string())?;
        down_2.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, 2);
        down_2.post(CGEventTapLocation::HID);

        let up_2 = CGEvent::new_mouse_event(
            source,
            CGEventType::LeftMouseUp,
            CGPoint::new(x, y),
            CGMouseButton::Left,
        )
        .map_err(|_| "failed to create second up event".to_string())?;
        up_2.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, 2);
        up_2.post(CGEventTapLocation::HID);

        Ok(())
    }

    fn keycode_for(key: &str) -> Option<CGKeyCode> {
        let key = key.to_lowercase();
        let code = match key.as_str() {
            "a" => 0,
            "s" => 1,
            "d" => 2,
            "f" => 3,
            "h" => 4,
            "g" => 5,
            "z" => 6,
            "x" => 7,
            "c" => 8,
            "v" => 9,
            "b" => 11,
            "q" => 12,
            "w" => 13,
            "e" => 14,
            "r" => 15,
            "y" => 16,
            "t" => 17,
            "1" => 18,
            "2" => 19,
            "3" => 20,
            "4" => 21,
            "6" => 22,
            "5" => 23,
            "=" => 24,
            "9" => 25,
            "7" => 26,
            "-" => 27,
            "8" => 28,
            "0" => 29,
            "]" => 30,
            "o" => 31,
            "u" => 32,
            "[" => 33,
            "i" => 34,
            "p" => 35,
            "l" => 37,
            "j" => 38,
            "'" => 39,
            "k" => 40,
            ";" => 41,
            "\\" => 42,
            "," => 43,
            "/" => 44,
            "n" => 45,
            "m" => 46,
            "." => 47,
            "tab" => 48,
            "space" => 49,
            "return" | "enter" => 36,
            "backspace" => 51,
            "escape" | "esc" => 53,
            "left" => 123,
            "right" => 124,
            "down" => 125,
            "up" => 126,
            _ => return None,
        };
        Some(code)
    }

    #[tauri::command]
    pub fn is_physical_key_down(key: String) -> bool {
        let k = key.trim().to_lowercase();
        match k.as_str() {
            "space" => physical_key_is_down(49),
            "option" | "alt" => physical_key_is_down(58) || physical_key_is_down(61),
            "shift" => physical_key_is_down(56) || physical_key_is_down(60),
            "control" | "ctrl" => physical_key_is_down(59) || physical_key_is_down(62),
            "command" | "cmd" => physical_key_is_down(55) || physical_key_is_down(54),
            other => keycode_for(other)
                .map(physical_key_is_down)
                .unwrap_or(false),
        }
    }

    fn flags_for_mods(mods: &[String]) -> CGEventFlags {
        let set: HashSet<String> = mods.iter().map(|m| m.to_lowercase()).collect();
        let mut flags = CGEventFlags::CGEventFlagNull;
        if set.contains("shift") {
            flags |= CGEventFlags::CGEventFlagShift;
        }
        if set.contains("cmd") || set.contains("command") {
            flags |= CGEventFlags::CGEventFlagCommand;
        }
        if set.contains("ctrl") || set.contains("control") {
            flags |= CGEventFlags::CGEventFlagControl;
        }
        if set.contains("opt") || set.contains("option") || set.contains("alt") {
            flags |= CGEventFlags::CGEventFlagAlternate;
        }
        flags
    }

    #[tauri::command]
    pub fn press_key(key: String, mods: Vec<String>) -> Result<(), String> {
        let keycode = keycode_for(&key).ok_or_else(|| format!("unknown key: {key}"))?;
        let source = event_source()?;
        let flags = flags_for_mods(&mods);

        let key_down = CGEvent::new_keyboard_event(source.clone(), keycode, true)
            .map_err(|_| "failed to create key down event".to_string())?;
        key_down.set_flags(flags);
        key_down.post(CGEventTapLocation::HID);

        let key_up = CGEvent::new_keyboard_event(source, keycode, false)
            .map_err(|_| "failed to create key up event".to_string())?;
        key_up.set_flags(flags);
        key_up.post(CGEventTapLocation::HID);
        Ok(())
    }

    #[tauri::command]
    pub fn type_text(text: String) -> Result<(), String> {
        let source = event_source()?;
        for unit in text.encode_utf16() {
            let key_down = CGEvent::new_keyboard_event(source.clone(), 0, true)
                .map_err(|_| "failed to create text key down event".to_string())?;
            key_down.set_string_from_utf16_unchecked(&[unit]);
            key_down.post(CGEventTapLocation::HID);

            let key_up = CGEvent::new_keyboard_event(source.clone(), 0, false)
                .map_err(|_| "failed to create text key up event".to_string())?;
            key_up.set_string_from_utf16_unchecked(&[unit]);
            key_up.post(CGEventTapLocation::HID);
        }
        Ok(())
    }

    #[tauri::command]
    pub fn scroll(x: f64, y: f64, dx: i32, dy: i32) -> Result<(), String> {
        move_mouse(x, y)?;
        let source = event_source()?;
        let event = CGEvent::new_scroll_event(source, ScrollEventUnit::PIXEL, 2, dy, dx, 0)
            .map_err(|_| "failed to create scroll event".to_string())?;
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    #[tauri::command]
    pub async fn screenshot(window: WebviewWindow) -> Result<Vec<u8>, String> {
        window
            .set_content_protected(true)
            .map_err(|e| format!("failed to protect Raymes window: {e}"))?;
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;

        let result = capture_display();
        let restore_result = window
            .set_content_protected(false)
            .map_err(|e| format!("failed to restore Raymes window capture setting: {e}"));

        match (result, restore_result) {
            (Ok(bytes), Ok(())) => Ok(bytes),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    }

    fn capture_display() -> Result<Vec<u8>, String> {
        let image = CGDisplay::main()
            .image()
            .ok_or_else(|| "failed to capture display image".to_string())?;

        let width = image.width() as u32;
        let height = image.height() as u32;
        let bytes_per_row = image.bytes_per_row();

        let data = image.data();
        let bytes = data.bytes();

        let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);
        for y in 0..height as usize {
            let row = &bytes[(y * bytes_per_row)..((y * bytes_per_row) + (width as usize * 4))];
            for px in row.chunks_exact(4) {
                rgba.push(px[2]);
                rgba.push(px[1]);
                rgba.push(px[0]);
                rgba.push(px[3]);
            }
        }

        let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_raw(width, height, rgba)
            .ok_or_else(|| "failed to build image buffer".to_string())?;

        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(img)
            .write_to(&mut out, ImageFormat::Png)
            .map_err(|e| format!("failed to encode png: {e}"))?;

        Ok(out.into_inner())
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use std::io::Cursor;
    use std::mem::{size_of, zeroed};
    use std::ptr::null_mut;

    use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
    use tauri::WebviewWindow;
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT,
        DIB_RGB_COLORS, SRCCOPY,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        mouse_event, GetAsyncKeyState, SendInput, VkKeyScanW, INPUT, INPUT_0, INPUT_KEYBOARD,
        KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN,
        MOUSEEVENTF_LEFTUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, VK_BACK,
        VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN,
        VK_MENU, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SetCursorPos, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    fn set_cursor(x: f64, y: f64) -> Result<(), String> {
        let ok = unsafe { SetCursorPos(x.round() as i32, y.round() as i32) };
        if ok == 0 {
            Err("failed to move the Windows cursor".to_string())
        } else {
            Ok(())
        }
    }

    #[tauri::command]
    pub fn move_mouse(x: f64, y: f64) -> Result<(), String> {
        set_cursor(x, y)
    }

    #[tauri::command]
    pub fn click(x: f64, y: f64, button: String) -> Result<(), String> {
        set_cursor(x, y)?;
        let (down, up) = match button.to_ascii_lowercase().as_str() {
            "left" => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
            "right" => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
            _ => return Err("button must be 'left' or 'right'".to_string()),
        };
        unsafe {
            mouse_event(down, 0, 0, 0, 0);
            mouse_event(up, 0, 0, 0, 0);
        }
        Ok(())
    }

    #[tauri::command]
    pub fn double_click(x: f64, y: f64) -> Result<(), String> {
        click(x, y, "left".to_string())?;
        click(x, y, "left".to_string())
    }

    fn virtual_key_for(key: &str) -> Option<u16> {
        let normalized = key.trim().to_ascii_lowercase();
        let named = match normalized.as_str() {
            "tab" => Some(VK_TAB),
            "space" => Some(VK_SPACE),
            "return" | "enter" => Some(VK_RETURN),
            "backspace" => Some(VK_BACK),
            "escape" | "esc" => Some(VK_ESCAPE),
            "left" => Some(VK_LEFT),
            "right" => Some(VK_RIGHT),
            "down" => Some(VK_DOWN),
            "up" => Some(VK_UP),
            _ => None,
        };
        if named.is_some() {
            return named;
        }
        let mut units = key.encode_utf16();
        let unit = units.next()?;
        if units.next().is_some() {
            return None;
        }
        let mapped = unsafe { VkKeyScanW(unit) };
        (mapped != -1).then_some((mapped as u16) & 0xff)
    }

    fn modifier_key(modifier: &str) -> Option<u16> {
        match modifier.trim().to_ascii_lowercase().as_str() {
            "shift" => Some(VK_LSHIFT),
            "ctrl" | "control" => Some(VK_LCONTROL),
            "opt" | "option" | "alt" => Some(VK_LMENU),
            "cmd" | "command" | "super" | "win" | "windows" => Some(VK_LWIN),
            _ => None,
        }
    }

    fn send_key(vk: u16, key_up: bool) -> Result<(), String> {
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if key_up { KEYEVENTF_KEYUP } else { 0 },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let sent = unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };
        if sent == 1 {
            Ok(())
        } else {
            Err("Windows rejected synthetic keyboard input".to_string())
        }
    }

    #[tauri::command]
    pub fn press_key(key: String, mods: Vec<String>) -> Result<(), String> {
        let key = virtual_key_for(&key).ok_or_else(|| format!("unknown key: {key}"))?;
        let modifiers: Vec<u16> = mods
            .iter()
            .filter_map(|value| modifier_key(value))
            .collect();
        for modifier in &modifiers {
            send_key(*modifier, false)?;
        }
        send_key(key, false)?;
        send_key(key, true)?;
        for modifier in modifiers.iter().rev() {
            send_key(*modifier, true)?;
        }
        Ok(())
    }

    fn send_unicode(unit: u16, key_up: bool) -> Result<(), String> {
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE | if key_up { KEYEVENTF_KEYUP } else { 0 },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let sent = unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };
        if sent == 1 {
            Ok(())
        } else {
            Err("Windows rejected synthetic text input".to_string())
        }
    }

    #[tauri::command]
    pub fn type_text(text: String) -> Result<(), String> {
        for unit in text.encode_utf16() {
            send_unicode(unit, false)?;
            send_unicode(unit, true)?;
        }
        Ok(())
    }

    #[tauri::command]
    pub fn scroll(x: f64, y: f64, dx: i32, dy: i32) -> Result<(), String> {
        set_cursor(x, y)?;
        unsafe {
            if dy != 0 {
                mouse_event(MOUSEEVENTF_WHEEL, 0, 0, dy, 0);
            }
            if dx != 0 {
                mouse_event(MOUSEEVENTF_HWHEEL, 0, 0, dx, 0);
            }
        }
        Ok(())
    }

    #[tauri::command]
    pub fn is_physical_key_down(key: String) -> bool {
        let normalized = key.trim().to_ascii_lowercase();
        let vk = match normalized.as_str() {
            "shift" => Some(VK_SHIFT),
            "ctrl" | "control" => Some(VK_CONTROL),
            "opt" | "option" | "alt" => Some(VK_MENU),
            "cmd" | "command" | "super" | "win" | "windows" => Some(VK_LWIN),
            other => virtual_key_for(other),
        };
        vk.is_some_and(|code| unsafe { GetAsyncKeyState(code as i32) } < 0)
    }

    #[tauri::command]
    pub async fn screenshot(window: WebviewWindow) -> Result<Vec<u8>, String> {
        window
            .set_content_protected(true)
            .map_err(|error| format!("failed to protect Tezbar window: {error}"))?;
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        let result = capture_virtual_desktop();
        let restore = window
            .set_content_protected(false)
            .map_err(|error| format!("failed to restore Tezbar capture setting: {error}"));
        match (result, restore) {
            (Ok(bytes), Ok(())) => Ok(bytes),
            (Err(error), _) | (Ok(_), Err(error)) => Err(error),
        }
    }

    fn capture_virtual_desktop() -> Result<Vec<u8>, String> {
        let x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
        let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
        if width <= 0 || height <= 0 {
            return Err("Windows reported an invalid virtual desktop size".to_string());
        }

        let screen_dc = unsafe { GetDC(null_mut()) };
        if screen_dc.is_null() {
            return Err("failed to access the Windows desktop".to_string());
        }
        let memory_dc = unsafe { CreateCompatibleDC(screen_dc) };
        let bitmap = unsafe { CreateCompatibleBitmap(screen_dc, width, height) };
        if memory_dc.is_null() || bitmap.is_null() {
            unsafe {
                if !memory_dc.is_null() {
                    DeleteDC(memory_dc);
                }
                if !bitmap.is_null() {
                    DeleteObject(bitmap);
                }
                ReleaseDC(null_mut(), screen_dc);
            }
            return Err("failed to allocate a Windows screenshot surface".to_string());
        }

        let previous = unsafe { SelectObject(memory_dc, bitmap) };
        let copied = unsafe {
            BitBlt(
                memory_dc,
                0,
                0,
                width,
                height,
                screen_dc,
                x,
                y,
                SRCCOPY | CAPTUREBLT,
            )
        };
        let mut info: BITMAPINFO = unsafe { zeroed() };
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        };
        let mut bgra = vec![0_u8; width as usize * height as usize * 4];
        let rows = if copied != 0 {
            unsafe {
                GetDIBits(
                    memory_dc,
                    bitmap,
                    0,
                    height as u32,
                    bgra.as_mut_ptr().cast(),
                    &mut info,
                    DIB_RGB_COLORS,
                )
            }
        } else {
            0
        };

        unsafe {
            SelectObject(memory_dc, previous);
            DeleteObject(bitmap);
            DeleteDC(memory_dc);
            ReleaseDC(null_mut(), screen_dc);
        }
        if rows != height {
            return Err("failed to copy pixels from the Windows desktop".to_string());
        }

        for pixel in bgra.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        let image: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_raw(width as u32, height as u32, bgra)
                .ok_or_else(|| "failed to build the Windows screenshot image".to_string())?;
        let mut output = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut output, ImageFormat::Png)
            .map_err(|error| format!("failed to encode Windows screenshot: {error}"))?;
        Ok(output.into_inner())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use tauri::WebviewWindow;

    fn unsupported() -> Result<(), String> {
        Err("native input automation is not available on this platform".to_string())
    }

    #[tauri::command]
    pub fn move_mouse(_x: f64, _y: f64) -> Result<(), String> {
        unsupported()
    }
    #[tauri::command]
    pub fn click(_x: f64, _y: f64, _button: String) -> Result<(), String> {
        unsupported()
    }
    #[tauri::command]
    pub fn double_click(_x: f64, _y: f64) -> Result<(), String> {
        unsupported()
    }
    #[tauri::command]
    pub fn press_key(_key: String, _mods: Vec<String>) -> Result<(), String> {
        unsupported()
    }
    #[tauri::command]
    pub fn type_text(_text: String) -> Result<(), String> {
        unsupported()
    }
    #[tauri::command]
    pub fn scroll(_x: f64, _y: f64, _dx: i32, _dy: i32) -> Result<(), String> {
        unsupported()
    }
    #[tauri::command]
    pub fn is_physical_key_down(_key: String) -> bool {
        false
    }
    #[tauri::command]
    pub async fn screenshot(_window: WebviewWindow) -> Result<Vec<u8>, String> {
        Err("screenshots are not available on this platform".to_string())
    }
}

pub use platform::*;
