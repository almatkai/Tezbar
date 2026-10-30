//! Lightweight rich-format clipboard access; arboard owns text and pixel formats.
#[cfg(target_os = "macos")]
mod platform {
    use std::ffi::{c_char, c_void, CStr, CString};
    type Id = *mut c_void;
    type Sel = *mut c_void;
    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const c_char) -> Id;
        fn sel_registerName(name: *const c_char) -> Sel;
        fn objc_msgSend();
    }
    #[link(name = "AppKit", kind = "framework")]
    extern "C" {}
    unsafe fn class(n: &str) -> Id {
        objc_getClass(CString::new(n).unwrap().as_ptr())
    }
    unsafe fn sel(n: &str) -> Sel {
        sel_registerName(CString::new(n).unwrap().as_ptr())
    }
    unsafe fn s0<T>(o: Id, n: &str) -> T {
        let f: unsafe extern "C" fn(Id, Sel) -> T = std::mem::transmute(objc_msgSend as *const ());
        f(o, sel(n))
    }
    unsafe fn s1<A, T>(o: Id, n: &str, a: A) -> T {
        let f: unsafe extern "C" fn(Id, Sel, A) -> T =
            std::mem::transmute(objc_msgSend as *const ());
        f(o, sel(n), a)
    }
    unsafe fn s2<A, B, T>(o: Id, n: &str, a: A, b: B) -> T {
        let f: unsafe extern "C" fn(Id, Sel, A, B) -> T =
            std::mem::transmute(objc_msgSend as *const ());
        f(o, sel(n), a, b)
    }
    struct Pool(Id);
    impl Pool {
        unsafe fn new() -> Self {
            Self(s0(s0::<Id>(class("NSAutoreleasePool"), "alloc"), "init"))
        }
    }
    impl Drop for Pool {
        fn drop(&mut self) {
            unsafe { s0::<()>(self.0, "drain") }
        }
    }
    unsafe fn board() -> Id {
        s0(class("NSPasteboard"), "generalPasteboard")
    }
    pub fn sequence() -> Option<u64> {
        unsafe {
            let _p = Pool::new();
            Some(s0::<isize>(board(), "changeCount") as u64)
        }
    }
    pub fn files() -> Vec<String> {
        unsafe {
            let _p = Pool::new();
            let classes: Id = s1(class("NSArray"), "arrayWithObject:", class("NSURL"));
            let values: Id = s2(
                board(),
                "readObjectsForClasses:options:",
                classes,
                std::ptr::null_mut::<c_void>(),
            );
            if values.is_null() {
                return vec![];
            }
            let count: usize = s0(values, "count");
            let mut out = vec![];
            for i in 0..count.min(10000) {
                let url: Id = s1(values, "objectAtIndex:", i);
                if !s0::<bool>(url, "isFileURL") {
                    continue;
                }
                let path: Id = s0(url, "path");
                let p: *const c_char = s0(path, "UTF8String");
                if !p.is_null() {
                    out.push(CStr::from_ptr(p).to_string_lossy().into_owned())
                }
            }
            out
        }
    }
    unsafe fn ns_string(value: &str) -> Result<Id, String> {
        let c = CString::new(value).map_err(|e| e.to_string())?;
        Ok(s1(class("NSString"), "stringWithUTF8String:", c.as_ptr()))
    }
    /// Pasteboard type identifiers of the current contents.
    pub fn types() -> Vec<String> {
        unsafe {
            let _p = Pool::new();
            let types: Id = s0(board(), "types");
            if types.is_null() {
                return vec![];
            }
            let count: usize = s0(types, "count");
            let mut out = Vec::with_capacity(count.min(64));
            for i in 0..count.min(64) {
                let value: Id = s1(types, "objectAtIndex:", i);
                let p: *const c_char = s0(value, "UTF8String");
                if !p.is_null() {
                    out.push(CStr::from_ptr(p).to_string_lossy().into_owned())
                }
            }
            out
        }
    }
    /// Writes text marked concealed + transient and returns the new changeCount.
    pub fn write_concealed_text(text: &str) -> Result<u64, String> {
        unsafe {
            let _p = Pool::new();
            let pb = board();
            s0::<isize>(pb, "clearContents");
            let value = ns_string(text)?;
            let empty = ns_string("")?;
            if !s2::<_, _, bool>(pb, "setString:forType:", value, ns_string("public.utf8-plain-text")?) {
                return Err("Could not write to the pasteboard".into());
            }
            for marker in [super::CONCEALED_TYPE, super::TRANSIENT_TYPE] {
                s2::<_, _, bool>(pb, "setString:forType:", empty, ns_string(marker)?);
            }
            Ok(s0::<isize>(pb, "changeCount") as u64)
        }
    }
    /// Clears the pasteboard only if nothing was copied since `sequence`.
    pub fn clear_if_sequence(sequence: u64) {
        unsafe {
            let _p = Pool::new();
            let pb = board();
            if s0::<isize>(pb, "changeCount") as u64 == sequence {
                s0::<isize>(pb, "clearContents");
            }
        }
    }
    pub fn write_files(paths: &[String]) -> Result<(), String> {
        unsafe {
            let _p = Pool::new();
            let urls: Id = s0(class("NSMutableArray"), "array");
            for path in paths {
                let c = CString::new(path.as_str()).map_err(|e| e.to_string())?;
                let s: Id = s1(class("NSString"), "stringWithUTF8String:", c.as_ptr());
                let url: Id = s1(class("NSURL"), "fileURLWithPath:", s);
                s1::<_, ()>(urls, "addObject:", url)
            }
            let pb = board();
            s0::<isize>(pb, "clearContents");
            if s1::<_, bool>(pb, "writeObjects:", urls) {
                Ok(())
            } else {
                Err("Could not write file URLs to pasteboard".into())
            }
        }
    }
}
#[cfg(target_os = "windows")]
mod platform {
    use std::ffi::c_void;
    type Handle = *mut c_void;
    #[link(name = "user32")]
    extern "system" {
        fn GetClipboardSequenceNumber() -> u32;
        fn OpenClipboard(h: Handle) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn GetClipboardData(format: u32) -> Handle;
        fn SetClipboardData(format: u32, h: Handle) -> Handle;
        fn CreateWindowExW(
            ex: u32,
            class: *const u16,
            title: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: Handle,
            menu: Handle,
            instance: Handle,
            param: Handle,
        ) -> Handle;
        fn DestroyWindow(window: Handle) -> i32;
    }
    #[link(name = "shell32")]
    extern "system" {
        fn DragQueryFileW(drop: Handle, i: u32, buffer: *mut u16, size: u32) -> u32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalAlloc(flags: u32, size: usize) -> Handle;
        fn GlobalLock(h: Handle) -> Handle;
        fn GlobalUnlock(h: Handle) -> i32;
        fn GlobalFree(h: Handle) -> Handle;
    }
    struct Board;
    impl Drop for Board {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }
    struct Owner(Handle);
    impl Owner {
        unsafe fn new() -> Result<Self, String> {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let window = CreateWindowExW(
                0,
                class.as_ptr(),
                std::ptr::null(),
                0,
                0,
                0,
                0,
                0,
                -3isize as Handle,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if window.is_null() {
                Err(format!(
                    "Could not create clipboard owner: {}",
                    std::io::Error::last_os_error()
                ))
            } else {
                Ok(Self(window))
            }
        }
    }
    impl Drop for Owner {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }
    pub fn types() -> Vec<String> {
        vec![]
    }
    pub fn write_concealed_text(_: &str) -> Result<u64, String> {
        Err("Concealed clipboard writes are available on macOS for now".into())
    }
    pub fn clear_if_sequence(_: u64) {}
    pub fn sequence() -> Option<u64> {
        Some(unsafe { GetClipboardSequenceNumber() } as u64)
    }
    pub fn files() -> Vec<String> {
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return vec![];
            }
            let _b = Board;
            let h = GetClipboardData(15);
            if h.is_null() {
                return vec![];
            }
            let n = DragQueryFileW(h, u32::MAX, std::ptr::null_mut(), 0).min(10000);
            let mut out = vec![];
            for i in 0..n {
                let len = DragQueryFileW(h, i, std::ptr::null_mut(), 0);
                let mut b = vec![0u16; len as usize + 1];
                DragQueryFileW(h, i, b.as_mut_ptr(), len + 1);
                out.push(String::from_utf16_lossy(&b[..len as usize]));
            }
            out
        }
    }
    pub fn write_files(paths: &[String]) -> Result<(), String> {
        unsafe {
            let mut data: Vec<u16> = vec![];
            for p in paths {
                data.extend(p.encode_utf16());
                data.push(0);
            }
            data.push(0);
            let h = GlobalAlloc(0x42, 20 + data.len() * 2);
            if h.is_null() {
                return Err("Clipboard allocation failed".into());
            }
            let ptr = GlobalLock(h) as *mut u8;
            if ptr.is_null() {
                GlobalFree(h);
                return Err("Clipboard allocation lock failed".into());
            }
            std::ptr::write_bytes(ptr, 0, 20);
            std::ptr::write_unaligned(ptr as *mut u32, 20);
            std::ptr::write_unaligned(ptr.add(16) as *mut u32, 1);
            std::ptr::copy_nonoverlapping(data.as_ptr() as *const u8, ptr.add(20), data.len() * 2);
            GlobalUnlock(h);
            // A NULL owner followed by EmptyClipboard makes SetClipboardData
            // invalid. Use a message-only window owned by this worker thread.
            let owner = match Owner::new() {
                Ok(owner) => owner,
                Err(error) => {
                    GlobalFree(h);
                    return Err(error);
                }
            };
            if OpenClipboard(owner.0) == 0 {
                GlobalFree(h);
                return Err("Clipboard is busy".into());
            }
            let _b = Board;
            if EmptyClipboard() == 0 {
                GlobalFree(h);
                return Err("Could not clear clipboard for file transfer".into());
            }
            if SetClipboardData(15, h).is_null() {
                GlobalFree(h);
                return Err("Could not write file clipboard".into());
            }
            Ok(())
        }
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    pub fn types() -> Vec<String> {
        vec![]
    }
    pub fn write_concealed_text(_: &str) -> Result<u64, String> {
        Err("Concealed clipboard writes are available on macOS for now".into())
    }
    pub fn clear_if_sequence(_: u64) {}
    pub fn sequence() -> Option<u64> {
        None
    }
    pub fn files() -> Vec<String> {
        vec![]
    }
    pub fn write_files(_: &[String]) -> Result<(), String> {
        Err("Native file clipboard is unavailable on this platform".into())
    }
}
pub use platform::*;

/// Markers from nspasteboard.org that password managers set on secrets.
pub const CONCEALED_TYPE: &str = "org.nspasteboard.ConcealedType";
pub const TRANSIENT_TYPE: &str = "org.nspasteboard.TransientType";

/// Clipboard history must not record contents carrying these markers.
pub fn is_private_types<S: AsRef<str>>(types: &[S]) -> bool {
    types
        .iter()
        .any(|t| matches!(t.as_ref(), CONCEALED_TYPE | TRANSIENT_TYPE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concealed_and_transient_contents_are_private() {
        assert!(is_private_types(&["public.utf8-plain-text", CONCEALED_TYPE]));
        assert!(is_private_types(&[TRANSIENT_TYPE]));
        assert!(!is_private_types(&["public.utf8-plain-text", "public.html"]));
        assert!(!is_private_types::<&str>(&[]));
    }
}
