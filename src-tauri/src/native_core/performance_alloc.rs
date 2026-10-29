//! Thread-local allocation-volume probe. Test executables only; other threads
//! and ordinary builds do not contribute. Counts requested bytes, not live RAM.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! { static VOLUME: Cell<Option<(u64,u64)>> = const { Cell::new(None) }; }
struct Counting;
fn record(bytes: usize) {
    let _ = VOLUME.try_with(|v| {
        if let Some((n, b)) = v.get() {
            v.set(Some((n + 1, b + bytes as u64)));
        }
    });
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;
pub fn volume<T>(run: impl FnOnce() -> T) -> (T, u64, u64) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            VOLUME.with(|v| v.set(None));
        }
    }
    VOLUME.with(|v| {
        assert!(v.get().is_none());
        v.set(Some((0, 0)));
    });
    let reset = Reset;
    let value = run();
    let (n, b) = VOLUME.with(|v| v.get().unwrap());
    drop(reset);
    (value, n, b)
}
