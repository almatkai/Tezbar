//! Native Windows shell, registry, power, speech and UI Automation calls.
//!
//! These replace `powershell.exe` children: every function here runs in-process and is
//! blocking, so call it from `spawn_blocking` (or a dedicated thread for long work).
use std::path::{Path, PathBuf};
use windows::core::{Interface, BSTR, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, RPC_E_CHANGED_MODE, SIZE};
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::Media::Speech::{ISpVoice, SpVoice, SPF_ASYNC, SPF_IS_NOT_XML, SPF_PURGEBEFORESPEAK};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IServiceProvider,
    CLSCTX_ALL, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationTextPattern, UIA_TextPatternId,
};
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOLDERID_AppsFolder, IEnumShellItems, IFolderView, IFolderView2,
    IPersistFolder2, IShellBrowser, IShellItem, IShellItemImageFactory, IShellWindows,
    SHCreateItemFromParsingName, SHGetKnownFolderItem, SHGetPathFromIDListW, ShellExecuteW,
    ShellWindows, KF_FLAG_DEFAULT, SIGDN, SIGDN_FILESYSPATH, SIGDN_NORMALDISPLAY,
    SIGDN_PARENTRELATIVEPARSING, SID_STopLevelBrowser, SIIGBF, SIIGBF_BIGGERSIZEOK,
    SIIGBF_ICONONLY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetTopWindow, GetWindow, GW_HWNDNEXT, SW_SHOWNORMAL};

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// COM for the current thread (single-threaded apartment, which shell objects prefer).
pub(crate) struct Apartment {
    owned: bool,
}
impl Apartment {
    pub(crate) fn enter() -> Result<Self, String> {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if hr == RPC_E_CHANGED_MODE {
            // Already initialised as MTA by someone else; shell objects marshal fine there.
            return Ok(Self { owned: false });
        }
        hr.ok().map_err(|e| format!("COM initialisation failed: {e}"))?;
        Ok(Self { owned: true })
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        if self.owned {
            unsafe { CoUninitialize() };
        }
    }
}

unsafe fn take_string(text: windows::core::PWSTR) -> String {
    unsafe {
        let value = text.to_string().unwrap_or_default();
        CoTaskMemFree(Some(text.0 as _));
        value
    }
}
fn display_name(item: &IShellItem, kind: SIGDN) -> Option<String> {
    unsafe { item.GetDisplayName(kind).ok().map(|text| take_string(text)) }
}

// ---------------------------------------------------------------- registry

const HKCU: windows_sys::Win32::System::Registry::HKEY =
    windows_sys::Win32::System::Registry::HKEY_CURRENT_USER;

pub(crate) fn read_user_dword(path: &str, name: &str) -> Option<u32> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_DWORD};
    let (path, name) = (wide(path), wide(name));
    let mut value = 0u32;
    let mut size = 4u32;
    let status = unsafe {
        RegGetValueW(
            HKCU,
            path.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut value as *mut u32).cast(),
            &mut size,
        )
    };
    (status == 0).then_some(value)
}
pub(crate) fn write_user_dword(path: &str, name: &str, value: u32) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{RegSetKeyValueW, REG_DWORD};
    let (path, name) = (wide(path), wide(name));
    let status = unsafe {
        RegSetKeyValueW(
            HKCU,
            path.as_ptr(),
            name.as_ptr(),
            REG_DWORD,
            (&value as *const u32).cast(),
            4,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(format!("Could not write the {name:?} setting (error {status})", name = String::from_utf16_lossy(&name[..name.len() - 1])))
    }
}
/// Read a value under HKLM or HKCU (`root` = "HKLM" | "HKCU") as text, like `reg query`.
pub(crate) fn read_registry_text(root: &str, path: &str, name: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY_LOCAL_MACHINE, REG_DWORD, REG_EXPAND_SZ, REG_SZ, RRF_RT_ANY,
    };
    let hive = if root.eq_ignore_ascii_case("HKLM") { HKEY_LOCAL_MACHINE } else { HKCU };
    let (path, name) = (wide(path), wide(name));
    let mut kind = 0u32;
    let mut size = 0u32;
    unsafe {
        if RegGetValueW(hive, path.as_ptr(), name.as_ptr(), RRF_RT_ANY, &mut kind, std::ptr::null_mut(), &mut size) != 0 {
            return None;
        }
        let mut buffer = vec![0u8; size as usize + 2];
        if RegGetValueW(hive, path.as_ptr(), name.as_ptr(), RRF_RT_ANY, &mut kind, buffer.as_mut_ptr().cast(), &mut size) != 0 {
            return None;
        }
        buffer.truncate(size as usize);
        match kind {
            REG_DWORD if buffer.len() >= 4 => {
                Some(u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]).to_string())
            }
            REG_SZ | REG_EXPAND_SZ => {
                let units: Vec<u16> = buffer.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                Some(String::from_utf16_lossy(&units).trim_end_matches('\0').to_owned())
            }
            _ => None,
        }
    }
}

const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

/// Whether apps currently use the dark theme.
pub(crate) fn dark_mode() -> bool {
    read_user_dword(PERSONALIZE, "AppsUseLightTheme") == Some(0)
}
/// Switch apps and the shell between dark and light, then tell running windows.
pub(crate) fn set_dark_mode(dark: bool) -> Result<(), String> {
    let light = u32::from(!dark);
    write_user_dword(PERSONALIZE, "AppsUseLightTheme", light)?;
    write_user_dword(PERSONALIZE, "SystemUsesLightTheme", light)?;
    broadcast_setting_change("ImmersiveColorSet");
    Ok(())
}
fn broadcast_setting_change(area: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };
    let area = wide(area);
    let mut result = 0usize;
    unsafe {
        SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, area.as_ptr() as isize, SMTO_ABORTIFHUNG, 1000, &mut result);
    }
}

// ---------------------------------------------------------------- power, input, recycle bin

pub(crate) fn lock_workstation() -> Result<(), String> {
    if unsafe { windows_sys::Win32::System::Shutdown::LockWorkStation() } != 0 {
        Ok(())
    } else {
        Err(format!("Windows refused to lock: {}", std::io::Error::last_os_error()))
    }
}
/// Press and release a virtual key (volume/mute media keys).
pub(crate) fn tap_key(vk: u8) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP};
    unsafe {
        keybd_event(vk, 0, 0, 0);
        keybd_event(vk, 0, KEYEVENTF_KEYUP, 0);
    }
}
pub(crate) const VK_VOLUME_MUTE: u8 = 0xAD;
pub(crate) const VK_VOLUME_DOWN: u8 = 0xAE;
pub(crate) const VK_VOLUME_UP: u8 = 0xAF;

pub(crate) fn sleep_display() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, HWND_BROADCAST, SC_MONITORPOWER, WM_SYSCOMMAND};
    unsafe {
        PostMessageW(HWND_BROADCAST, WM_SYSCOMMAND, SC_MONITORPOWER as usize, 2);
    }
}
pub(crate) fn suspend() -> Result<(), String> {
    if unsafe { windows_sys::Win32::System::Power::SetSuspendState(0, 0, 0) } != 0 {
        Ok(())
    } else {
        Err("Windows refused suspend request".into())
    }
}
/// Empty every Recycle Bin. `Ok(false)` when it was already empty.
pub(crate) fn empty_recycle_bin() -> Result<bool, String> {
    use windows_sys::Win32::UI::Shell::{SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND};
    let hr = unsafe {
        SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND)
    };
    match hr {
        0 => Ok(true),
        // E_UNEXPECTED: nothing to empty.
        h if h as u32 == 0x8000_FFFF => Ok(false),
        h => Err(format!("Could not empty the Recycle Bin (0x{:08X})", h as u32)),
    }
}
/// `ipconfig /flushdns` without the child: dnsapi's resolver-cache flush.
pub(crate) fn flush_dns() -> Result<(), String> {
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    unsafe {
        let library = LoadLibraryW(wide("dnsapi.dll").as_ptr());
        if library.is_null() {
            return Err("DNS client library unavailable".into());
        }
        let Some(flush) = GetProcAddress(library, c"DnsFlushResolverCache".as_ptr().cast()) else {
            return Err("DNS cache flush is unavailable on this Windows".into());
        };
        let flush: unsafe extern "system" fn() -> i32 = std::mem::transmute(flush);
        if flush() != 0 {
            Ok(())
        } else {
            Err("Windows refused to flush the DNS cache".into())
        }
    }
}

/// Keeps the system and display awake until dropped (the request belongs to a parked thread).
pub(crate) struct KeepAwake {
    stop: Option<std::sync::mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl KeepAwake {
    pub(crate) fn start() -> Result<Self, String> {
        use windows_sys::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED};
        let (stop, wait) = std::sync::mpsc::channel::<()>();
        let (ready, started) = std::sync::mpsc::channel::<bool>();
        let thread = std::thread::Builder::new()
            .name("tezbar-keep-awake".into())
            .spawn(move || unsafe {
                let ok = SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED) != 0;
                let _ = ready.send(ok);
                let _ = wait.recv();
                SetThreadExecutionState(ES_CONTINUOUS);
            })
            .map_err(|e| e.to_string())?;
        if started.recv().unwrap_or(false) {
            Ok(Self { stop: Some(stop), thread: Some(thread) })
        } else {
            drop(stop);
            let _ = thread.join();
            Err("Windows refused the keep-awake request".into())
        }
    }
    pub(crate) fn is_active(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }
}
impl Drop for KeepAwake {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

// ---------------------------------------------------------------- Wi-Fi radio (WinRT, no elevation)

/// Toggle the first Wi-Fi radio. Returns (was_on, now_on).
pub(crate) fn toggle_wifi() -> Result<(bool, bool), String> {
    use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};
    let _apartment = Apartment::enter()?;
    let access = Radio::RequestAccessAsync().and_then(|op| op.get()).map_err(|e| e.to_string())?;
    if access != RadioAccessStatus::Allowed {
        return Err("Windows does not allow Tezbar to control radios (check Settings › Privacy › Radios).".into());
    }
    let radios = Radio::GetRadiosAsync().and_then(|op| op.get()).map_err(|e| e.to_string())?;
    let radio = (0..radios.Size().map_err(|e| e.to_string())?)
        .filter_map(|i| radios.GetAt(i).ok())
        .find(|r| r.Kind().ok() == Some(RadioKind::WiFi))
        .ok_or("No Wi-Fi adapter found.")?;
    let was = radio.State().map_err(|e| e.to_string())? == RadioState::On;
    let status = radio
        .SetStateAsync(if was { RadioState::Off } else { RadioState::On })
        .and_then(|op| op.get())
        .map_err(|e| e.to_string())?;
    if status != RadioAccessStatus::Allowed {
        return Err("Windows refused to change the Wi-Fi radio.".into());
    }
    let now = radio.State().map_err(|e| e.to_string())? == RadioState::On;
    Ok((was, now))
}

// ---------------------------------------------------------------- speech (SAPI)

/// Speak `text` with the default voice until done or `cancelled()` returns true.
pub(crate) fn speak(text: &str, cancelled: impl Fn() -> bool) -> Result<(), String> {
    let _apartment = Apartment::enter()?;
    unsafe {
        let voice: ISpVoice = CoCreateInstance(&SpVoice, None, CLSCTX_ALL).map_err(|e| format!("Text-to-speech runtime is unavailable: {e}"))?;
        let text = HSTRING::from(text);
        voice
            .Speak(PCWSTR(text.as_ptr()), (SPF_ASYNC.0 | SPF_IS_NOT_XML.0) as u32, None)
            .map_err(|e| e.to_string())?;
        loop {
            if cancelled() {
                let _ = voice.Speak(PCWSTR::null(), SPF_PURGEBEFORESPEAK.0 as u32, None);
                return Ok(());
            }
            // S_OK when finished, S_FALSE (also Ok) on timeout: distinguish with the HRESULT.
            let hr = (Interface::vtable(&voice).WaitUntilDone)(Interface::as_raw(&voice), 100);
            if hr.0 == 0 {
                return Ok(());
            }
            if hr.is_err() {
                return Err(hr.message());
            }
        }
    }
}

// ---------------------------------------------------------------- Start menu apps and icons

/// Every entry of the shell Apps folder as `[{"Name":..,"AppID":..}]` (Get-StartApps' shape).
pub(crate) fn start_apps_json() -> Result<String, String> {
    let _apartment = Apartment::enter()?;
    let mut apps = Vec::new();
    unsafe {
        let folder: IShellItem = SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None).map_err(|e| e.to_string())?;
        let items: IEnumShellItems = folder.BindToHandler(None, &BHID_EnumItems).map_err(|e| e.to_string())?;
        loop {
            let mut batch: [Option<IShellItem>; 16] = Default::default();
            let mut fetched = 0u32;
            if items.Next(&mut batch, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            for item in batch.iter().take(fetched as usize).flatten() {
                if let (Some(name), Some(id)) = (display_name(item, SIGDN_NORMALDISPLAY), display_name(item, SIGDN_PARENTRELATIVEPARSING)) {
                    apps.push(serde_json::json!({"Name": name, "AppID": id}));
                }
            }
        }
    }
    serde_json::to_string(&apps).map_err(|e| e.to_string())
}

/// Render the shell's icon (or thumbnail) of a file, shortcut or `shell:AppsFolder\…` item to PNG.
pub(crate) fn shell_icon_png(source: &str, dest: &Path, size: i32, icon_only: bool) -> Result<(), String> {
    let _apartment = Apartment::enter()?;
    let source = HSTRING::from(source);
    unsafe {
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&source, None).map_err(|e| e.to_string())?;
        let flags = if icon_only { SIIGBF(SIIGBF_BIGGERSIZEOK.0 | SIIGBF_ICONONLY.0) } else { SIIGBF_BIGGERSIZEOK };
        let bitmap = factory.GetImage(SIZE { cx: size, cy: size }, flags).map_err(|e| e.to_string())?;
        let pixels = bitmap_rgba(bitmap);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let (width, height, rgba) = pixels?;
        image::RgbaImage::from_raw(width, height, rgba)
            .ok_or("Icon bitmap has an unexpected size")?
            .save_with_format(dest, image::ImageFormat::Png)
            .map_err(|e| e.to_string())
    }
}
unsafe fn bitmap_rgba(bitmap: windows::Win32::Graphics::Gdi::HBITMAP) -> Result<(u32, u32, Vec<u8>), String> {
    unsafe {
        let mut info = BITMAP::default();
        if GetObjectW(HGDIOBJ(bitmap.0), std::mem::size_of::<BITMAP>() as i32, Some((&mut info as *mut BITMAP).cast())) == 0 {
            return Err("Unreadable icon bitmap".into());
        }
        let (width, height) = (info.bmWidth.max(0) as u32, info.bmHeight.unsigned_abs());
        let mut header = BITMAPINFO::default();
        header.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let dc = GetDC(None);
        let rows = GetDIBits(dc, bitmap, 0, height, Some(pixels.as_mut_ptr().cast()), &mut header, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        if rows == 0 {
            return Err("Unreadable icon pixels".into());
        }
        let has_alpha = pixels.chunks_exact(4).any(|p| p[3] != 0);
        for p in pixels.chunks_exact_mut(4) {
            p.swap(0, 2); // BGRA -> RGBA
            if !has_alpha {
                p[3] = 255;
            } else if p[3] != 0 && p[3] != 255 {
                // Shell images are premultiplied.
                let a = u32::from(p[3]);
                for c in &mut p[..3] {
                    *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        Ok((width, height, pixels))
    }
}

/// `Start-Process app "target"`: open `target` with `app` (an .exe, .lnk or registered app).
pub(crate) fn open_with(app: &str, target: &str) -> Result<(), String> {
    let _apartment = Apartment::enter()?;
    let parameters = HSTRING::from(format!("\"{}\"", target.replace('"', "")));
    let result = unsafe {
        ShellExecuteW(None, &HSTRING::from("open"), &HSTRING::from(app), &parameters, PCWSTR::null(), SW_SHOWNORMAL)
    };
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err(format!("Windows could not open {target} with {app} (code {})", result.0 as isize))
    }
}

// ---------------------------------------------------------------- File Explorer windows

/// Explorer browsers in front-to-back window order.
fn explorer_browsers() -> Result<Vec<IShellBrowser>, String> {
    unsafe {
        let windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL).map_err(|e| e.to_string())?;
        let mut browsers = Vec::new();
        for index in 0..windows.Count().map_err(|e| e.to_string())? {
            let Ok(dispatch) = windows.Item(&VARIANT::from(index)) else { continue };
            let Ok(provider) = dispatch.cast::<IServiceProvider>() else { continue };
            let Ok(browser) = provider.QueryService::<IShellBrowser>(&SID_STopLevelBrowser) else { continue };
            let Ok(hwnd) = browser.GetWindow() else { continue };
            browsers.push((z_order(hwnd), browser));
        }
        browsers.sort_by_key(|(z, _)| *z);
        Ok(browsers.into_iter().map(|(_, b)| b).collect())
    }
}
fn z_order(target: HWND) -> usize {
    unsafe {
        let mut current = GetTopWindow(None).ok();
        let mut index = 0;
        while let Some(hwnd) = current {
            if hwnd == target {
                return index;
            }
            index += 1;
            current = GetWindow(hwnd, GW_HWNDNEXT).ok();
        }
        usize::MAX
    }
}
fn folder_view(browser: &IShellBrowser) -> Option<IFolderView> {
    unsafe { browser.QueryActiveShellView().ok()?.cast::<IFolderView>().ok() }
}

/// Folder shown by the front-most File Explorer window.
pub(crate) fn explorer_folder() -> Result<PathBuf, String> {
    let _apartment = Apartment::enter()?;
    for browser in explorer_browsers()? {
        let Some(view) = folder_view(&browser) else { continue };
        unsafe {
            let Ok(folder) = view.GetFolder::<IPersistFolder2>() else { continue };
            let Ok(pidl) = folder.GetCurFolder() else { continue };
            let mut buffer = [0u16; 260];
            let ok = SHGetPathFromIDListW(pidl, &mut buffer).as_bool();
            CoTaskMemFree(Some(pidl as _));
            if ok {
                let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
                return Ok(PathBuf::from(String::from_utf16_lossy(&buffer[..end])));
            }
        }
    }
    Err("No Explorer folder window is open.".into())
}

/// Files selected in the front-most File Explorer window.
pub(crate) fn explorer_selection(limit: usize) -> Result<Vec<PathBuf>, String> {
    let _apartment = Apartment::enter()?;
    for browser in explorer_browsers()? {
        let Some(view) = folder_view(&browser).and_then(|v| v.cast::<IFolderView2>().ok()) else { continue };
        let Ok(items) = (unsafe { view.GetSelection(false) }) else { continue };
        let count = unsafe { items.GetCount() }.unwrap_or(0) as usize;
        let paths = (0..count.min(limit))
            .filter_map(|i| unsafe { items.GetItemAt(i as u32) }.ok())
            .filter_map(|item| display_name(&item, SIGDN_FILESYSPATH))
            .map(PathBuf::from)
            .collect();
        return Ok(paths);
    }
    Ok(Vec::new())
}

// ---------------------------------------------------------------- selected text (UI Automation)

/// Text selected in the focused control of the foreground application.
pub(crate) fn selected_text() -> Result<String, String> {
    let _apartment = Apartment::enter()?;
    unsafe {
        let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
        let focused = automation.GetFocusedElement().map_err(|_| "No focused text element")?;
        let pattern: IUIAutomationTextPattern = focused
            .GetCurrentPatternAs(UIA_TextPatternId)
            .map_err(|_| "The focused application does not expose text selection")?;
        let ranges = pattern.GetSelection().map_err(|e| e.to_string())?;
        let mut text = String::new();
        for index in 0..ranges.Length().unwrap_or(0) {
            if let Ok(range) = ranges.GetElement(index) {
                text.push_str(&range.GetText(-1).unwrap_or_else(|_| BSTR::new()).to_string());
            }
        }
        Ok(text)
    }
}

// ---------------------------------------------------------------- clipboard file drop list

/// Put `paths` on the clipboard as a file drop list (CF_HDROP), like Explorer's Copy.
pub(crate) fn set_clipboard_files(paths: &[PathBuf]) -> Result<(), String> {
    use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT};
    use windows_sys::Win32::UI::Shell::DROPFILES;
    const CF_HDROP: u32 = 15;
    let mut list: Vec<u16> = Vec::new();
    for path in paths {
        list.extend(path.as_os_str().to_string_lossy().encode_utf16());
        list.push(0);
    }
    list.push(0);
    let header = std::mem::size_of::<DROPFILES>();
    let bytes = header + list.len() * 2;
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes);
        if memory.is_null() {
            return Err("Out of memory for the clipboard".into());
        }
        let data = GlobalLock(memory) as *mut u8;
        let drop = data as *mut DROPFILES;
        (*drop).pFiles = header as u32;
        (*drop).fWide = 1;
        std::ptr::copy_nonoverlapping(list.as_ptr() as *const u8, data.add(header), list.len() * 2);
        GlobalUnlock(memory);
        let mut opened = false;
        for _ in 0..10 {
            if OpenClipboard(std::ptr::null_mut()) != 0 {
                opened = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        if !opened {
            GlobalFree(memory);
            return Err("The clipboard is busy".into());
        }
        EmptyClipboard();
        let placed = !SetClipboardData(CF_HDROP, memory).is_null();
        CloseClipboard();
        if placed {
            Ok(())
        } else {
            GlobalFree(memory);
            Err("Windows refused the clipboard file list".into())
        }
    }
}

// ---------------------------------------------------------------- process trees

/// Terminate `pid` and every descendant (what `taskkill /T /F` did).
pub(crate) fn kill_tree(pid: u32) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    let mut parents: Vec<(u32, u32)> = Vec::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot as isize != -1 {
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            if Process32FirstW(snapshot, &mut entry) != 0 {
                loop {
                    parents.push((entry.th32ProcessID, entry.th32ParentProcessID));
                    if Process32NextW(snapshot, &mut entry) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snapshot);
        }
        let mut doomed = vec![pid];
        let mut index = 0;
        while index < doomed.len() {
            let parent = doomed[index];
            for (child, of) in &parents {
                if *of == parent && *child != 0 && !doomed.contains(child) {
                    doomed.push(*child);
                }
            }
            index += 1;
        }
        // Children first, so nothing is re-parented mid-way.
        for target in doomed.into_iter().rev() {
            let handle = OpenProcess(PROCESS_TERMINATE, 0, target);
            if !handle.is_null() {
                TerminateProcess(handle, 1);
                CloseHandle(handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_reads_the_current_theme() {
        // Either value is valid; the read itself must work on every Windows 10/11 desktop.
        assert!(read_user_dword(PERSONALIZE, "AppsUseLightTheme").is_some());
        assert!(read_registry_text("HKLM", r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "CurrentBuild").is_some());
    }
    #[test]
    fn start_apps_lists_installed_applications() {
        let apps: Vec<serde_json::Value> = serde_json::from_str(&start_apps_json().unwrap()).unwrap();
        assert!(apps.len() > 5, "{apps:?}");
        assert!(apps.iter().all(|a| a["Name"].is_string() && a["AppID"].is_string()));
    }
    #[test]
    fn shell_icons_render_to_png() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("icon.png");
        let notepad = std::env::var("SystemRoot").unwrap_or("C:\\Windows".into()) + "\\System32\\notepad.exe";
        shell_icon_png(&notepad, &dest, 64, true).unwrap();
        let png = image::open(&dest).unwrap();
        assert!(png.width() >= 32 && png.height() >= 32);
        assert!(png.to_rgba8().pixels().any(|p| p[3] > 0));
    }
    #[test]
    fn kill_tree_ends_a_child_and_its_descendants() {
        let mut parent = std::process::Command::new("cmd.exe")
            .args(["/c", "ping -n 30 127.0.0.1 >NUL"])
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        kill_tree(parent.id());
        let status = parent.wait().unwrap();
        assert!(!status.success());
    }
}
