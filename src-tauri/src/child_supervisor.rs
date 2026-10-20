//! Private native child lifetime owner. Dependencies are limited to std
//! plus Unix libc. The executable dispatches it before initializing Tauri or app state.
use std::{
    ffi::OsString,
    io::{self, Read},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

pub const MODE: &str = "--tezbar-private-child-supervisor-v1";

/// None means ordinary application startup; malformed helper requests fail closed.
pub fn dispatch() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new(MODE)) {
        return None;
    }
    Some(match supervise(args.collect()) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Native child supervisor: {error}");
            1
        }
    })
}

fn supervise(args: Vec<OsString>) -> io::Result<i32> {
    let executable = args
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing child executable"))?;
    // A dedicated reader observes host death even while the model is starting.
    // The server never inherits this pipe, so it cannot keep its own lease alive.
    let (closed, lease) = mpsc::channel();
    std::thread::spawn(move || {
        let mut block = [0; 256];
        let mut stdin = io::stdin().lock();
        loop {
            match stdin.read(&mut block) {
                Ok(0) => break,
                Ok(_) => continue,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        let _ = closed.send(());
    });
    let mut command = Command::new(executable);
    command.args(&args[1..]).stdin(Stdio::null());
    // stdout/stderr inherit the host's null/drained handles, never its lifetime pipe.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Assign the suspended child to our job before it can spawn descendants.
        command.creation_flags(0x08000000 | 0x00000004);
    }
    #[cfg(windows)]
    let job = windows::Job::new()?;
    let mut child = command.spawn()?;
    #[cfg(unix)]
    let tree = Tree(child.id());
    #[cfg(windows)]
    let tree = match job.attach(&child) {
        Ok(()) => job,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    let result = monitor(&mut child, lease);
    // Cleanup also runs on normal server exit (including any surviving descendants).
    drop(tree);
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn monitor(child: &mut Child, lease: mpsc::Receiver<()>) -> io::Result<i32> {
    loop {
        // Do not reap the Unix group leader yet: retaining its PID prevents reuse
        // before the isolated group is killed. waitid(WNOWAIT) is used below.
        if let Some(code) = exited(child)? {
            return Ok(code);
        }
        match lease.recv_timeout(Duration::from_millis(20)) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(0),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

#[cfg(unix)]
struct Tree(u32);
#[cfg(unix)]
impl Drop for Tree {
    fn drop(&mut self) {
        unsafe {
            kill(-(self.0 as i32), 9);
        }
    }
}
#[cfg(unix)]
extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

#[cfg(unix)]
fn exited(child: &mut Child) -> io::Result<Option<i32>> {
    // `kill(pid, 0)` cannot detect zombies. Use the platform libc waitid layout
    // rather than reaping with try_wait, which permits process-group ID reuse.
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            child.id(),
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result != 0 {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::Interrupted {
            return Ok(None);
        }
        return Err(error);
    }
    if unsafe { info.si_pid() } == 0 {
        return Ok(None);
    }
    let code = if info.si_code == libc::CLD_EXITED {
        unsafe { info.si_status() }
    } else {
        1
    };
    Ok(Some(code))
}
#[cfg(windows)]
fn exited(child: &mut Child) -> io::Result<Option<i32>> {
    Ok(child.try_wait()?.map(|status| status.code().unwrap_or(1)))
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{ffi::c_void, os::windows::io::AsRawHandle};
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
            if unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle()) } == 0 {
                return Err(io::Error::last_os_error());
            }
            let status = unsafe { NtResumeProcess(child.as_raw_handle()) };
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
}

#[cfg(test)]
#[path = "child_supervisor_tests.rs"]
pub(crate) mod tests;
