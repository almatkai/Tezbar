//! Private native child lifetime owner. Dependencies are limited to std
//! plus Unix libc. The executable dispatches it before initializing Tauri or app state.
#[cfg(any(windows, test))]
use std::process::Child;
use std::{
    ffi::OsString,
    io::{self, Read, Write},
    process::{Command, Stdio},
    sync::mpsc,
    time::Duration,
};

pub const EXTENSION_MODE: &str = "--tezbar-private-extension-supervisor-v1";

pub const MODE: &str = "--tezbar-private-child-supervisor-v1";

enum MonitorEvent {
    Closed,
    #[cfg(unix)]
    Exited(io::Result<i32>),
}

/// None means ordinary application startup; malformed helper requests fail closed.
pub fn dispatch() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    let mode = args.next()?;
    let relay = mode == std::ffi::OsStr::new(EXTENSION_MODE);
    if !relay && mode != std::ffi::OsStr::new(MODE) {
        return None;
    }
    Some(match supervise(args.collect(), relay) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Native child supervisor: {error}");
            1
        }
    })
}

fn supervise(args: Vec<OsString>, relay: bool) -> io::Result<i32> {
    let executable = args
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing child executable"))?;
    // A dedicated reader observes host death even while the model is starting.
    // The server never inherits this pipe, so it cannot keep its own lease alive.
    let (closed, lease) = mpsc::channel();
    let relay_closed = closed.clone();
    if !relay {
        let closed = closed.clone();
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
            let _ = closed.send(MonitorEvent::Closed);
        });
    }
    let mut command = Command::new(executable);
    command
        .args(&args[1..])
        .stdin(if relay { Stdio::piped() } else { Stdio::null() });
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
    let tree = Tree {
        group: child.id(),
        graceful: relay,
    };
    #[cfg(windows)]
    let tree = match job.attach(&child) {
        Ok(()) => job,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    if relay {
        // Protocol relay and lifetime detection are independent: a child blocked
        // reading stdin cannot prevent the host-death EOF from being observed.
        let mut input = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("Missing extension stdin"))?;
        let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(64);
        std::thread::spawn(move || {
            let mut stdin = io::stdin().lock();
            let mut block = [0u8; 16384];
            loop {
                match stdin.read(&mut block) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.try_send(block[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            let _ = relay_closed.send(MonitorEvent::Closed);
        });
        std::thread::spawn(move || {
            for bytes in rx {
                if input.write_all(&bytes).and_then(|_| input.flush()).is_err() {
                    break;
                }
            }
        });
    }
    // waitid(WNOWAIT) blocks without reaping the Unix group leader. Its PID
    // therefore remains reserved until tree cleanup, including a lease-close
    // racing normal child exit. Joining after kill prevents a detached waiter.
    #[cfg(unix)]
    let waiter = {
        let pid = child.id();
        std::thread::Builder::new()
            .name("child-exit".into())
            .spawn(move || {
                let _ = closed.send(MonitorEvent::Exited(wait_for_exit(pid)));
            })
    };
    #[cfg(unix)]
    let result = match &waiter {
        Ok(_) => monitor(lease),
        Err(error) => Err(io::Error::new(error.kind(), error.to_string())),
    };
    #[cfg(windows)]
    let result = monitor(&mut child, lease);
    // Cleanup also runs on normal server exit (including any surviving descendants).
    drop(tree);
    let _ = child.kill();
    #[cfg(unix)]
    if let Ok(waiter) = waiter {
        let _ = waiter.join();
    }
    let _ = child.wait();
    result
}

#[cfg(unix)]
fn monitor(lease: mpsc::Receiver<MonitorEvent>) -> io::Result<i32> {
    match lease.recv() {
        Ok(MonitorEvent::Exited(result)) => result,
        Ok(MonitorEvent::Closed) | Err(_) => Ok(0),
    }
}
#[cfg(windows)]
fn monitor(child: &mut Child, lease: mpsc::Receiver<MonitorEvent>) -> io::Result<i32> {
    loop {
        if let Some(code) = exited(child)? {
            return Ok(code);
        }
        match lease.recv_timeout(Duration::from_millis(20)) {
            Ok(MonitorEvent::Closed) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(0),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

#[cfg(unix)]
struct Tree {
    group: u32,
    graceful: bool,
}
#[cfg(unix)]
impl Drop for Tree {
    fn drop(&mut self) {
        unsafe {
            if self.graceful {
                kill(-(self.group as i32), 15);
                // SDK protocol cleanup already had its grace; this small final
                // TERM window lets native helpers release OS-owned resources.
                std::thread::sleep(Duration::from_millis(200));
            }
            kill(-(self.group as i32), 9);
        }
    }
}
#[cfg(unix)]
extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

#[cfg(unix)]
fn wait_for_exit(pid: u32) -> io::Result<i32> {
    // `kill(pid, 0)` cannot detect zombies. Use the platform libc waitid layout
    // rather than reaping with try_wait, which permits process-group ID reuse.
    loop {
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let result =
            unsafe { libc::waitid(libc::P_PID, pid, &mut info, libc::WEXITED | libc::WNOWAIT) };
        if result != 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        return Ok(if info.si_code == libc::CLD_EXITED {
            unsafe { info.si_status() }
        } else {
            1
        });
    }
}
#[cfg(windows)]
fn exited(child: &mut Child) -> io::Result<Option<i32>> {
    Ok(child.try_wait()?.map(|status| status.code().unwrap_or(1)))
}

#[cfg(windows)]
#[path = "windows_job.rs"]
mod windows;

#[cfg(test)]
#[path = "child_supervisor_tests.rs"]
pub(crate) mod tests;
