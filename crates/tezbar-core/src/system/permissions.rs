use super::*;
fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(windows) {
        "win32"
    } else {
        "linux"
    }
}
fn catalog() -> Value {
    serde_json::from_str(if cfg!(windows) {
        include_str!("permissions-windows.json")
    } else {
        include_str!("permissions-mac.json")
    })
    .unwrap()
}
impl SystemCore {
    pub(super) async fn permissions(&self, request: Option<&str>) -> CoreResult {
        let descriptors = catalog();
        if let Some(id) = request {
            if !descriptors
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d["id"] == id)
            {
                return Err(format!("Unknown permission: {id}"));
            }
        }
        let wanted = request.map(str::to_owned);
        if let Some(id) = request {
            #[cfg(target_os = "macos")]
            if id == "accessibility" {
                blocking(|| {
                    native::prompt_accessibility();
                    Ok(Value::Null)
                })
                .await?;
            }
            #[cfg(target_os = "macos")]
            if id == "microphone" {
                native::request_microphone().await?;
            }
            let d = descriptors
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["id"] == id)
                .unwrap();
            if cfg!(target_os = "macos") || (cfg!(windows) && id == "microphone") {
                if let Some(url) = d["settingsUrl"].as_str() {
                    let _ = self
                        .ctx
                        .request("core:platform:open-url", vec![json!(url)])
                        .await;
                }
            }
        }
        blocking(move || {
            let statuses: Vec<Value> = descriptors
                .as_array()
                .unwrap()
                .iter()
                .filter(|d| wanted.as_ref().is_none_or(|id| d["id"] == *id))
                .map(|d| {
                    let id = d["id"].as_str().unwrap();
                    json!({"descriptor":d,"state":probe(id),"checkedAt":now_ms()})
                })
                .collect();
            if wanted.is_some() {
                Ok(statuses[0].clone())
            } else {
                Ok(json!({"platform":platform(),"statuses":statuses}))
            }
        })
        .await
    }
}
fn probe(id: &str) -> &'static str {
    #[cfg(target_os = "macos")]
    {
        return native::probe(id);
    }
    #[cfg(windows)]
    {
        return match id {
            "accessibility" | "input-monitoring" | "screen-recording" => windows_desktop(id),
            "microphone" => windows_microphone(),
            _ => "unsupported",
        };
    }
    #[allow(unreachable_code)]
    {
        let _ = id;
        "unsupported"
    }
}
#[cfg(windows)]
fn windows_desktop(id: &str) -> &'static str {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn OpenInputDesktop(flags: u32, inherit: i32, access: u32) -> *mut std::ffi::c_void;
        fn CloseDesktop(desktop: *mut std::ffi::c_void) -> i32;
        fn GetSystemMetrics(index: i32) -> i32;
    }
    // Windows has no per-application TCC grant for these capabilities. Probe the
    // current input desktop rather than reporting success from a runtime shim.
    unsafe {
        let desktop = OpenInputDesktop(0, 0, 1);
        if desktop.is_null() {
            return "denied";
        }
        CloseDesktop(desktop);
        if id == "screen-recording" && (GetSystemMetrics(0) <= 0 || GetSystemMetrics(1) <= 0) {
            return "not-determined";
        }
    }
    "granted"
}
#[cfg(windows)]
fn windows_microphone() -> &'static str {
    fn registry(path: &str, key: &str) -> Option<String> {
        use std::os::windows::process::CommandExt;
        let out = std::process::Command::new("reg.exe")
            .args(["query", path, "/v", key])
            .creation_flags(0x08000000)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    }
    if registry(
        r"HKLM\Software\Policies\Microsoft\Windows\AppPrivacy",
        "LetAppsAccessMicrophone",
    )
    .is_some_and(|s| s.split_whitespace().last() == Some("0x2"))
    {
        return "restricted";
    }
    for path in [
        r"HKLM\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone\NonPackaged",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone",
    ] {
        if let Some(value) = registry(path, "Value") {
            if value.split_whitespace().last() == Some("Deny") {
                return "denied";
            }
        }
    }
    match registry(
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone",
        "Value",
    ) {
        Some(value) if value.split_whitespace().last() == Some("Allow") => "granted",
        _ => "not-determined",
    }
}
#[cfg(target_os = "macos")]
mod native {
    use std::ffi::{c_char, c_void, CStr};
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
        static kAXTrustedCheckOptionPrompt: *const c_void;
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGPreflightListenEventAccess() -> bool;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFBooleanTrue: *const c_void;
        fn CFDictionaryCreate(
            a: *const c_void,
            k: *const *const c_void,
            v: *const *const c_void,
            n: isize,
            kc: *const c_void,
            vc: *const c_void,
        ) -> *const c_void;
        fn CFRelease(v: *const c_void);
    }
    #[link(name = "AVFoundation", kind = "framework")]
    unsafe extern "C" {}
    #[link(name = "EventKit", kind = "framework")]
    unsafe extern "C" {}
    #[repr(C)]
    struct AEDesc {
        descriptor_type: u32,
        data_handle: *mut c_void,
    }
    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn AECreateDesc(kind: u32, data: *const c_void, size: isize, desc: *mut AEDesc) -> i32;
        fn AEDisposeDesc(desc: *mut AEDesc) -> i32;
        fn AEDeterminePermissionToAutomateTarget(
            target: *const AEDesc,
            event_class: u32,
            event_id: u32,
            ask: bool,
        ) -> i32;
    }
    fn automation() -> &'static str {
        unsafe {
            let target = b"com.apple.systemevents";
            let mut desc = AEDesc {
                descriptor_type: 0,
                data_handle: std::ptr::null_mut(),
            };
            if AECreateDesc(
                u32::from_be_bytes(*b"bund"),
                target.as_ptr() as *const c_void,
                target.len() as isize,
                &mut desc,
            ) != 0
            {
                return "not-determined";
            }
            let status = AEDeterminePermissionToAutomateTarget(
                &desc,
                u32::from_be_bytes(*b"core"),
                u32::from_be_bytes(*b"getd"),
                false,
            );
            AEDisposeDesc(&mut desc);
            match status {
                0 => "granted",
                -1743 => "denied",
                _ => "not-determined",
            }
        }
    }
    #[link(name = "objc")]
    unsafe extern "C" {
        fn objc_getClass(n: *const c_char) -> *mut c_void;
        fn sel_registerName(n: *const c_char) -> *mut c_void;
        fn objc_msgSend();
    }
    unsafe fn class(n: &CStr) -> *mut c_void {
        unsafe { objc_getClass(n.as_ptr()) }
    }
    unsafe fn status(class_name: &CStr, selector: &CStr, arg: usize) -> isize {
        unsafe {
            let class = class(class_name);
            if class.is_null() {
                return -1;
            }
            let f: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> isize =
                std::mem::transmute(objc_msgSend as *const ());
            f(class, sel_registerName(selector.as_ptr()), arg)
        }
    }
    fn map(n: isize) -> &'static str {
        match n {
            0 => "not-determined",
            1 => "restricted",
            2 => "denied",
            3 | 4 => "granted",
            _ => "not-determined",
        }
    }
    pub async fn request_microphone() -> Result<(), String> {
        // macOS terminates the process if the bundled usage string is absent.
        let usage_present = objc2::rc::autoreleasepool(|_| unsafe {
            let noarg: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let string: unsafe extern "C" fn(
                *mut c_void,
                *mut c_void,
                *const c_char,
            ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
            let witharg: unsafe extern "C" fn(
                *mut c_void,
                *mut c_void,
                *mut c_void,
            ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
            let length: unsafe extern "C" fn(*mut c_void, *mut c_void) -> usize =
                std::mem::transmute(objc_msgSend as *const ());
            let bundle = noarg(class(c"NSBundle"), sel_registerName(c"mainBundle".as_ptr()));
            let key = string(
                class(c"NSString"),
                sel_registerName(c"stringWithUTF8String:".as_ptr()),
                c"NSMicrophoneUsageDescription".as_ptr(),
            );
            let value = witharg(
                bundle,
                sel_registerName(c"objectForInfoDictionaryKey:".as_ptr()),
                key,
            );
            !value.is_null() && length(value, sel_registerName(c"length".as_ptr())) > 0
        });
        if !usage_present {
            return Err("Microphone permission requires NSMicrophoneUsageDescription in the application bundle Info.plist".into());
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        let sender = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
        {
            let block = block2::RcBlock::new(move |_granted: objc2::runtime::Bool| {
                if let Some(tx) = sender.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    let _ = tx.send(());
                }
            });
            // AVFoundation copies the completion block until the native permission prompt resolves.
            unsafe {
                let string: unsafe extern "C" fn(
                    *mut c_void,
                    *mut c_void,
                    *const c_char,
                ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
                let media = string(
                    class(c"NSString"),
                    sel_registerName(c"stringWithUTF8String:".as_ptr()),
                    c"soun".as_ptr(),
                );
                let send: unsafe extern "C" fn(
                    *mut c_void,
                    *mut c_void,
                    *mut c_void,
                    *const c_void,
                ) = std::mem::transmute(objc_msgSend as *const ());
                send(
                    class(c"AVCaptureDevice"),
                    sel_registerName(c"requestAccessForMediaType:completionHandler:".as_ptr()),
                    media,
                    (&*block) as *const _ as *const c_void,
                );
            }
        }
        tokio::time::timeout(std::time::Duration::from_secs(300), rx)
            .await
            .map_err(|_| "Microphone permission request timed out".to_string())?
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn prompt_accessibility() {
        unsafe {
            let key = kAXTrustedCheckOptionPrompt;
            let value = kCFBooleanTrue;
            let options = CFDictionaryCreate(
                std::ptr::null(),
                &key,
                &value,
                1,
                std::ptr::null(),
                std::ptr::null(),
            );
            if !options.is_null() {
                AXIsProcessTrustedWithOptions(options);
                CFRelease(options);
            }
        }
    }
    pub fn probe(id: &str) -> &'static str {
        objc2::rc::autoreleasepool(|_| unsafe {
            match id {
                "accessibility" => {
                    if AXIsProcessTrusted() {
                        "granted"
                    } else {
                        "denied"
                    }
                }
                "input-monitoring" => {
                    if CGPreflightListenEventAccess() {
                        "granted"
                    } else {
                        "denied"
                    }
                }
                "screen-recording" => {
                    if CGPreflightScreenCaptureAccess() {
                        "granted"
                    } else {
                        "denied"
                    }
                }
                "microphone" => {
                    let f: unsafe extern "C" fn(
                        *mut c_void,
                        *mut c_void,
                        *const c_char,
                    ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
                    let media = f(
                        class(c"NSString"),
                        sel_registerName(c"stringWithUTF8String:".as_ptr()),
                        c"soun".as_ptr(),
                    );
                    map(status(
                        c"AVCaptureDevice",
                        c"authorizationStatusForMediaType:",
                        media as usize,
                    ))
                }
                "calendar" => {
                    let value = status(c"EKEventStore", c"authorizationStatusForEntityType:", 0);
                    if value == 4 {
                        "restricted"
                    } else {
                        map(value)
                    }
                }
                "automation" => automation(),
                _ => "unsupported",
            }
        })
    }
}
