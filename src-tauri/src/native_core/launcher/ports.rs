use super::providers::output;
use crate::native_core::common::CoreResult;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};

/// A safety-only snapshot. Never serve this from the keystroke/search cache: the
/// process birth time is required to distinguish a reused PID during confirmation.
pub async fn list_fresh() -> CoreResult {
    let mut snapshot = list().await?;
    snapshot
        .as_array_mut()
        .ok_or("Invalid open port list")?
        .retain_mut(|row| {
            let Some(pid) = row["pid"].as_str().and_then(|v| v.parse::<u32>().ok()) else {
                return false;
            };
            match identity(pid) {
                Ok(birth) => {
                    row["identity"] = json!(birth);
                    true
                }
                // A listener may disappear during enumeration, or belong to a
                // protected process. Never offer it as a verified kill target.
                Err(_) => false,
            }
        });
    Ok(snapshot)
}

#[cfg(target_os = "macos")]
pub(super) fn identity(pid: u32) -> Result<String, String> {
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::uninit();
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    // SAFETY: proc_pidinfo writes at most the provided size into this aligned buffer.
    let read = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if read != size {
        return Err(format!("Cannot verify process identity for PID {pid}"));
    }
    // SAFETY: the call reported a complete proc_bsdinfo structure.
    let info = unsafe { info.assume_init() };
    if info.pbi_start_tvsec == 0 {
        return Err("Missing process creation time".into());
    }
    Ok(format!(
        "{}:{}",
        info.pbi_start_tvsec, info.pbi_start_tvusec
    ))
}

#[cfg(windows)]
mod windows_process {
    use std::ffi::c_void;
    type Handle = *mut c_void;
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn GetProcessTimes(
            handle: Handle,
            created: *mut FileTime,
            exited: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn TerminateProcess(handle: Handle, exit_code: u32) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }
    const QUERY: u32 = 0x1000; // PROCESS_QUERY_LIMITED_INFORMATION
    const TERMINATE: u32 = 0x0001;
    struct Process(Handle);
    impl Drop for Process {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    fn open(pid: u32, access: u32) -> Result<Process, String> {
        let handle = unsafe { OpenProcess(access, 0, pid) };
        if handle.is_null() {
            Err(format!(
                "Cannot open PID {pid}: {}",
                std::io::Error::last_os_error()
            ))
        } else {
            Ok(Process(handle))
        }
    }
    fn birth(process: &Process) -> Result<String, String> {
        let (mut created, mut exited, mut kernel, mut user) = (
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
        );
        let ok = unsafe {
            GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user)
        };
        if ok == 0 {
            return Err(format!(
                "Cannot read process creation time: {}",
                std::io::Error::last_os_error()
            ));
        }
        if created.high == 0 && created.low == 0 {
            return Err("Missing process creation time".into());
        }
        Ok(format!("{}:{}", created.high, created.low))
    }
    pub(super) fn identity(pid: u32) -> Result<String, String> {
        birth(&open(pid, QUERY)?)
    }
    pub(super) fn terminate_verified(pid: u32, expected: &str) -> Result<(), String> {
        // Keep this handle alive through termination: Windows cannot recycle the
        // process object/PID while this handle refers to the original object.
        let process = open(pid, QUERY | TERMINATE)?;
        if birth(&process)? != expected {
            return Err("Process identity changed; no process was terminated".into());
        }
        if unsafe { TerminateProcess(process.0, 1) } == 0 {
            return Err(format!(
                "Could not terminate PID {pid}: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }
}
#[cfg(windows)]
fn identity(pid: u32) -> Result<String, String> {
    windows_process::identity(pid)
}

#[cfg(target_os = "macos")]
pub async fn terminate_verified(pid: u32, signal: &str, expected: &str) -> Result<(), String> {
    // Verify again immediately before the signal; macOS has no equivalent of
    // Windows' terminating-by-process-handle. A very narrow reuse race remains.
    if identity(pid)? != expected {
        return Err("Process identity changed; no process was terminated".into());
    }
    let sig = match signal {
        "15" => libc::SIGTERM,
        "9" => libc::SIGKILL,
        _ => return Err("Invalid signal".into()),
    };
    if unsafe { libc::kill(pid as i32, sig) } != 0 {
        return Err(format!(
            "Could not terminate PID {pid}: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}
#[cfg(windows)]
pub async fn terminate_verified(pid: u32, signal: &str, expected: &str) -> Result<(), String> {
    if signal != "15" {
        return Err("Invalid Windows termination signal".into());
    }
    windows_process::terminate_verified(pid, expected)
}
#[cfg(not(any(windows, target_os = "macos")))]
pub(super) fn identity(_pid: u32) -> Result<String, String> {
    Err("Process identity is unavailable".into())
}
#[cfg(not(any(windows, target_os = "macos")))]
pub async fn terminate_verified(_pid: u32, _signal: &str, _expected: &str) -> Result<(), String> {
    Err("Port termination is not available on this platform".into())
}

pub async fn list() -> CoreResult {
    #[cfg(windows)]
    {
        let raw = output("netstat.exe", &["-ano", "-p", "tcp"], 8).await?;
        let processes = output("tasklist.exe", &["/fo", "csv", "/nh"], 8)
            .await
            .unwrap_or_default();
        let mut names = HashMap::new();
        for line in processes.lines() {
            let parts: Vec<_> = line.trim_matches('"').split("\",\"").collect();
            if parts.len() >= 2 {
                names.insert(parts[1].to_owned(), parts[0].to_owned());
            }
        }
        return Ok(json!(parse_windows(&raw, &names)));
    }
    #[cfg(not(windows))]
    {
        let raw = match output("/usr/sbin/lsof", &["-nP", "-iTCP", "-sTCP:LISTEN"], 8).await {
            Ok(s) => s,
            Err(e) if e.is_empty() => String::new(),
            Err(e) => return Err(e),
        };
        let ps = output("/bin/ps", &["-axo", "pid=,comm="], 8)
            .await
            .unwrap_or_default();
        let mut names = HashMap::new();
        for line in ps.lines() {
            let line = line.trim();
            if let Some((pid, cmd)) = line.split_once(char::is_whitespace) {
                names.insert(
                    pid.to_owned(),
                    std::path::Path::new(cmd.trim())
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
        Ok(json!(parse_lsof(&raw, &names)))
    }
}
fn grouped(entries: Vec<(String, String, String, u16)>) -> Vec<Value> {
    let mut groups: HashMap<(String, String, String), BTreeSet<u16>> = HashMap::new();
    for (name, pid, user, port) in entries {
        groups.entry((name, pid, user)).or_default().insert(port);
    }
    let mut out:Vec<_>=groups.into_iter().map(|((name,pid,user),ports)|json!({"process":name,"pid":pid,"user":user,"ports":ports})).collect();
    out.sort_by(|a, b| {
        a["process"]
            .as_str()
            .cmp(&b["process"].as_str())
            .then_with(|| a["pid"].as_str().cmp(&b["pid"].as_str()))
    });
    out
}
fn decode(s: &str) -> String {
    regex::Regex::new(r"\\x([0-9a-fA-F]{2})")
        .unwrap()
        .replace_all(s, |c: &regex::Captures| {
            u8::from_str_radix(&c[1], 16)
                .map(|n| (n as char).to_string())
                .unwrap_or_default()
        })
        .into_owned()
}
pub fn parse_lsof(raw: &str, names: &HashMap<String, String>) -> Vec<Value> {
    let re = regex::Regex::new(r":(\d+)\s+\(LISTEN\)$").unwrap();
    grouped(
        raw.lines()
            .filter_map(|line| {
                let cols: Vec<_> = line.split_whitespace().collect();
                let port = re.captures(line)?.get(1)?.as_str().parse().ok()?;
                if cols.len() < 3 {
                    return None;
                }
                Some((
                    names
                        .get(cols[1])
                        .cloned()
                        .unwrap_or_else(|| decode(cols[0])),
                    cols[1].into(),
                    cols[2].into(),
                    port,
                ))
            })
            .collect(),
    )
}
#[cfg(any(windows, test))]
pub fn parse_windows(raw: &str, names: &HashMap<String, String>) -> Vec<Value> {
    grouped(
        raw.lines()
            .filter_map(|line| {
                let c: Vec<_> = line.split_whitespace().collect();
                if c.len() < 5 || c[0] != "TCP" || c[3] != "LISTENING" {
                    return None;
                }
                let port = c[1].rsplit(':').next()?.parse().ok()?;
                Some((
                    names.get(c[4]).cloned().unwrap_or("unknown".into()),
                    c[4].into(),
                    "current user".into(),
                    port,
                ))
            })
            .collect(),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ipv4_ipv6_dedup() {
        let raw="COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME\nnode 123 dev 4u IPv4 a 0t0 TCP *:3000 (LISTEN)\nnode 123 dev 5u IPv6 b 0t0 TCP [::1]:3000 (LISTEN)\nnode 123 dev 6u IPv4 c 0t0 TCP *:8080 (LISTEN)";
        let r = parse_lsof(raw, &HashMap::new());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0]["ports"], json!([3000, 8080]));
        let r = parse_windows(" TCP [::]:3000 [::]:0 LISTENING 123", &HashMap::new());
        assert_eq!(r[0]["pid"], "123");
    }
}
