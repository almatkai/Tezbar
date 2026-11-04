//! Owned process-tree lifetime on Windows. Children are spawned suspended.
use std::{ffi::c_void, os::windows::io::AsRawHandle};
use std::{io, process::Child};
type Handle = *mut c_void;
#[repr(C)]
#[derive(Default)]
struct BasicLimits {
    process_time: i64,
    job_time: i64,
    flags: u32,
    min_working_set: usize,
    max_working_set: usize,
    active_processes: u32,
    affinity: usize,
    priority: u32,
    scheduling: u32,
}
#[repr(C)]
#[derive(Default)]
struct ExtendedLimits {
    basic: BasicLimits,
    io: [u64; 6],
    process_memory: usize,
    job_memory: usize,
    peak_process_memory: usize,
    peak_job_memory: usize,
}
#[link(name = "kernel32")]
extern "system" {
    fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> Handle;
    fn SetInformationJobObject(job: Handle, class: i32, info: *const c_void, size: u32) -> i32;
    fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
}
#[link(name = "ntdll")]
extern "system" {
    fn NtResumeProcess(process: Handle) -> i32;
}
pub struct Job(Handle);
impl Job {
    pub fn new() -> io::Result<Self> {
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Self(handle);
        let mut limits = ExtendedLimits::default();
        limits.basic.flags = 0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        if unsafe {
            SetInformationJobObject(
                handle,
                9,
                &limits as *const _ as _,
                std::mem::size_of_val(&limits) as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }
    pub fn attach(&self, child: &Child) -> io::Result<()> {
        unsafe { self.attach_raw(child.as_raw_handle()) }
    }
    pub unsafe fn attach_raw(&self, process: *mut c_void) -> io::Result<()> {
        if unsafe { AssignProcessToJobObject(self.0, process) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let status = unsafe { NtResumeProcess(process) };
        if status < 0 {
            return Err(io::Error::other(format!(
                "NtResumeProcess failed: {status:#x}"
            )));
        }
        Ok(())
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
// Owned kernel handle; closing is the only cross-thread operation.
unsafe impl Send for Job {}
