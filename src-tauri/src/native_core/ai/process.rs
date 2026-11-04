#[cfg(windows)]
#[path = "../../windows_job.rs"]
mod windows_job;
use super::*;
use std::process::Stdio;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
};
#[derive(Clone)]
pub struct Cancel(tokio::sync::watch::Sender<bool>, Arc<Mutex<()>>);
impl Cancel {
    pub fn new() -> Self {
        Self(
            tokio::sync::watch::channel(false).0,
            Arc::new(Mutex::new(())),
        )
    }
    pub fn cancel(&self) {
        let _effect = self.1.lock().unwrap_or_else(|e| e.into_inner());
        self.0.send_replace(true);
    }
    pub fn effect<T>(&self, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let _guard = self
            .1
            .lock()
            .map_err(|_| "Cancellation checkpoint failed")?;
        if self.is_cancelled() {
            return Err("Operation cancelled".into());
        }
        f()
    }
    pub fn is_cancelled(&self) -> bool {
        *self.0.borrow()
    }
    pub async fn cancelled(&self) {
        let mut rx = self.0.subscribe();
        if *rx.borrow() {
            return;
        }
        while rx.changed().await.is_ok() {
            if *rx.borrow() {
                return;
            }
        }
    }
}
pub fn command(binary: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut c = Command::new(binary);
    c.kill_on_drop(true);
    c.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    c.process_group(0);
    #[cfg(windows)]
    c.creation_flags(0x08000204);
    c
}
/// Packaged GUI applications do not inherit the user's interactive PATH.
/// Resolve it lazily in Rust and preserve known package-manager locations even
/// when a login shell fails or omits them. Never mutate the process environment.
pub async fn login_path(home: &std::path::Path, cancel: &Cancel) -> String {
    let inherited = std::env::var("PATH").unwrap_or_default();
    if cfg!(windows) {
        return inherited;
    }
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|p| std::path::Path::new(p).is_absolute())
        .unwrap_or_else(|| "/bin/bash".into());
    let path = output(
        command(shell).args(["-l", "-c", "printf %s \"$PATH\""]),
        Duration::from_secs(5),
        cancel,
    )
    .await
    .unwrap_or(inherited);
    augment_path(&path, home)
}
fn augment_path(path: &str, home: &std::path::Path) -> String {
    let mut paths: Vec<std::path::PathBuf> = std::env::split_paths(path).collect();
    let mut extras = vec![];
    if let Some(p) = std::env::var_os("PNPM_HOME") {
        extras.push(p.into());
    }
    extras.extend(
        [
            "Library/pnpm",
            "Library/pnpm/bin",
            ".local/bin",
            ".local/share/pnpm",
            ".npm-global/bin",
            ".cargo/bin",
            ".bun/bin",
        ]
        .iter()
        .map(|p| home.join(p)),
    );
    extras.extend(
        ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"]
            .iter()
            .map(std::path::PathBuf::from),
    );
    for p in extras {
        if !paths.contains(&p) {
            paths.push(p);
        }
    }
    std::env::join_paths(paths)
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.into())
}

pub struct GroupChild {
    pub child: Child,
    pid: Option<u32>,
    #[cfg(windows)]
    job: Option<windows_job::Job>,
}
impl GroupChild {
    pub fn spawn(cmd: &mut Command) -> Result<Self, String> {
        #[cfg(windows)]
        let job = windows_job::Job::new().map_err(err)?;
        let mut child = cmd.spawn().map_err(err)?;
        #[cfg(windows)]
        if let Err(e) =
            unsafe { job.attach_raw(child.raw_handle().ok_or("Missing Windows process handle")?) }
        {
            let _ = child.start_kill();
            return Err(format!("Native process ownership: {e}"));
        }
        let pid = child.id();
        Ok(Self {
            child,
            pid,
            #[cfg(windows)]
            job: Some(job),
        })
    }
    pub async fn terminate(&mut self) {
        #[cfg(unix)]
        if let Some(pid) = self.pid {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGTERM);
            }
        }
        #[cfg(windows)]
        {
            self.job.take();
        }
        if tokio::time::timeout(Duration::from_millis(1500), self.child.wait())
            .await
            .is_err()
        {
            self.kill();
            let _ = self.child.wait().await;
        }
        self.kill(); // also reap descendants that ignored TERM after the parent exited
    }
    fn kill(&mut self) {
        #[cfg(unix)]
        if let Some(pid) = self.pid.take() {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
        #[cfg(windows)]
        {
            self.job.take();
            self.pid.take();
        }
        let _ = self.child.start_kill();
    }
}
impl Drop for GroupChild {
    fn drop(&mut self) {
        self.kill();
    }
}
async fn bounded_read(mut reader: impl AsyncRead + Unpin, limit: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf).await.map_err(err)?;
        if n == 0 {
            break;
        }
        if out.len() + n > limit {
            return Err("Subprocess output exceeded limit".into());
        }
        out.extend_from_slice(&buf[..n]);
    }
    Ok(out)
}
pub async fn output(
    cmd: &mut Command,
    timeout: Duration,
    cancel: &Cancel,
) -> Result<String, String> {
    if cancel.is_cancelled() {
        return Err("Operation cancelled".into());
    }
    let mut group = cancel.effect(|| GroupChild::spawn(cmd))?;
    let stdout = group
        .child
        .stdout
        .take()
        .ok_or("Missing subprocess stdout")?;
    let stderr = group
        .child
        .stderr
        .take()
        .ok_or("Missing subprocess stderr")?;
    let work = async {
        let (status, out, error) = tokio::try_join!(
            async { group.child.wait().await.map_err(err) },
            bounded_read(stdout, 16 * 1024 * 1024),
            bounded_read(stderr, 4 * 1024 * 1024)
        )?;
        if !status.success() {
            return Err(format!(
                "Process exited {status}: {}",
                String::from_utf8_lossy(&error)
            ));
        }
        Ok(String::from_utf8_lossy(&out).to_string())
    };
    let result = tokio::select! {r=tokio::time::timeout(timeout,work)=>r.map_err(|_|"Subprocess timed out".to_string()).and_then(|v|v),_=cancel.cancelled()=>Err("Operation cancelled".into())};
    group.terminate().await;
    result
}
/// Shell outcomes retain both streams even on failure or timeout. Success-only
/// callers deliberately keep using `output` above.
#[derive(Debug)]
pub struct ShellCapture {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub truncated: bool,
}
impl ShellCapture {
    pub fn result(self) -> Result<String, String> {
        let text = format!(
            "Exit status: {}{}\nstdout:\n{}\nstderr:\n{}{}",
            self.exit_code
                .map(|n| n.to_string())
                .unwrap_or_else(|| "unavailable".into()),
            if self.timed_out { " (timed out)" } else { "" },
            self.stdout,
            self.stderr,
            if self.truncated {
                "\n[Output truncated]"
            } else {
                ""
            }
        );
        if self.timed_out || self.exit_code != Some(0) {
            Err(text)
        } else {
            Ok(text)
        }
    }
}
async fn capture_read(
    reader: &mut (impl AsyncRead + Unpin),
    out: &mut Vec<u8>,
    truncated: &mut bool,
    limit: usize,
) -> Result<(), String> {
    let mut buf = [0; 8192];
    loop {
        let n = reader.read(&mut buf).await.map_err(err)?;
        if n == 0 {
            return Ok(());
        }
        let keep = n.min(limit.saturating_sub(out.len()));
        out.extend_from_slice(&buf[..keep]);
        *truncated |= keep < n;
    }
}
pub async fn shell_capture(
    cmd: &mut Command,
    timeout: Duration,
    cancel: &Cancel,
) -> Result<ShellCapture, String> {
    let mut group = cancel.effect(|| GroupChild::spawn(cmd))?;
    let mut stdout = group
        .child
        .stdout
        .take()
        .ok_or("Missing subprocess stdout")?;
    let mut stderr = group
        .child
        .stderr
        .take()
        .ok_or("Missing subprocess stderr")?;
    let (mut out, mut error) = (Vec::new(), Vec::new());
    let (mut out_cut, mut err_cut) = (false, false);
    let work = async {
        tokio::try_join!(
            async { group.child.wait().await.map_err(err) },
            capture_read(&mut stdout, &mut out, &mut out_cut, 48 * 1024),
            capture_read(&mut stderr, &mut error, &mut err_cut, 12 * 1024)
        )
        .map(|r| r.0)
    };
    let status = tokio::select! { biased; _=cancel.cancelled()=>Err("Operation cancelled".to_string()), r=tokio::time::timeout(timeout,work)=>match r { Ok(r)=>r.map(Some), Err(_)=>Ok(None) } };
    group.terminate().await;
    let status = status?;
    // Killing the tree closes pipes; collect bytes already queued before timeout.
    let _ = tokio::time::timeout(Duration::from_millis(500), async {
        tokio::join!(
            capture_read(&mut stdout, &mut out, &mut out_cut, 48 * 1024),
            capture_read(&mut stderr, &mut error, &mut err_cut, 12 * 1024)
        )
    })
    .await;
    Ok(ShellCapture {
        stdout: String::from_utf8_lossy(&out).into(),
        stderr: String::from_utf8_lossy(&error).into(),
        exit_code: status.and_then(|s| s.code()),
        timed_out: status.is_none(),
        truncated: out_cut || err_cut,
    })
}
/// Bytes remain bytes until LF arrives: split UTF-8 and U+2028/U+2029 are safe.
#[derive(Default)]
pub struct Lines {
    buffer: Vec<u8>,
    scanned: usize,
}
impl Lines {
    pub fn feed(&mut self, bytes: &[u8], eof: bool) -> Result<Vec<String>, String> {
        self.buffer.extend_from_slice(bytes);
        if self.buffer.len() > 16 * 1024 * 1024 {
            return Err("Protocol line exceeds 16 MiB".into());
        }
        let mut lines = Vec::new();
        while let Some(i) = self.buffer[self.scanned..]
            .iter()
            .position(|b| *b == b'\n')
            .map(|i| i + self.scanned)
        {
            let mut line: Vec<u8> = self.buffer.drain(..=i).collect();
            self.scanned = 0;
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            lines.push(String::from_utf8(line).map_err(err)?);
        }
        self.scanned = self.buffer.len();
        if eof && !self.buffer.is_empty() {
            lines.push(String::from_utf8(std::mem::take(&mut self.buffer)).map_err(err)?);
            self.scanned = 0;
        }
        Ok(lines)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lf_only_split_utf8_and_partial_tail() {
        let mut lines = Lines::default();
        let input = "{\"text\":\"🦀\u{2028}\u{2029}\"}\r\n{\"last\":true}";
        let mut got = Vec::new();
        for byte in input.as_bytes() {
            got.extend(lines.feed(&[*byte], false).unwrap());
        }
        got.extend(lines.feed(&[], true).unwrap());
        assert_eq!(got.len(), 2);
        assert_eq!(
            serde_json::from_str::<Value>(&got[0]).unwrap()["text"],
            "🦀\u{2028}\u{2029}"
        );
    }
    #[tokio::test]
    #[cfg(unix)]
    async fn packaged_gui_path_finds_user_cli_and_reaches_child_tools() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join(".local/bin");
        std::fs::create_dir_all(&bin).unwrap();
        let script = bin.join("tezbar-cli-fixture");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf 'resolved:'\ncommand -v tezbar-cli-fixture\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = augment_path("/usr/bin:/bin", home.path());
        assert_eq!(path.split(':').filter(|v| *v == "/usr/bin").count(), 1);
        let out = output(
            command("tezbar-cli-fixture").env("PATH", path),
            Duration::from_secs(2),
            &Cancel::new(),
        )
        .await
        .unwrap();
        assert_eq!(out.trim(), format!("resolved:{}", script.display()));
        let marker = home.path().join("must-not-run");
        let cancel = Cancel::new();
        cancel.cancel();
        assert!(output(
            command("/bin/sh")
                .arg("-c")
                .arg("touch \"$1\"")
                .arg("fixture")
                .arg(&marker),
            Duration::from_secs(2),
            &cancel
        )
        .await
        .is_err());
        assert!(!marker.exists());
    }
    #[tokio::test]
    #[cfg(unix)]
    async fn cancelled_process_is_reaped() {
        let cancel = Cancel::new();
        let c = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            c.cancel();
        });
        let result = output(
            command("sh").args(["-c", "sleep 30"]),
            Duration::from_secs(60),
            &cancel,
        )
        .await;
        assert!(result.unwrap_err().contains("cancelled"));
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    #[tokio::test]
    async fn powershell_unicode_and_job_owned_tree() {
        let output=output(command("powershell.exe").args(["-NoLogo","-NoProfile","-NonInteractive","-Command","[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false); Write-Output 'Привет 🦀'" ]),Duration::from_secs(10),&Cancel::new()).await.unwrap();
        assert!(output.contains("Привет 🦀"));
        let temp = tempfile::tempdir().unwrap();
        let pids = temp.path().join("pids.txt");
        let child_script=format!("Start-Process powershell.exe -ArgumentList '-NoProfile','-Command','Start-Sleep 120' -PassThru | Select-Object -ExpandProperty Id | Set-Content -LiteralPath '{}'; Start-Sleep 120",pids.to_string_lossy().replace('\'',"''"));
        let mut parent = command("powershell.exe");
        parent.args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &child_script,
        ]);
        let mut group = GroupChild::spawn(&mut parent).unwrap();
        let root = group.child.id().unwrap();
        let grandchild = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Ok(s) = tokio::fs::read_to_string(&pids).await {
                    if let Ok(id) = s.trim().parse::<u32>() {
                        break id;
                    }
                }
                tokio::time::sleep(Duration::from_millis(20)).await
            }
        })
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(2), group.terminate())
            .await
            .unwrap();
        for pid in [root, grandchild] {
            let status = std::process::Command::new("tasklist.exe")
                .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
                .output()
                .unwrap();
            assert!(!String::from_utf8_lossy(&status.stdout).contains(&format!("\"{pid}\"")));
        }
    }
}

#[cfg(windows)]
pub fn windows_mcp_command(
    binary: &str,
    path: &std::ffi::OsStr,
    pathext: &str,
    args: &[String],
) -> Result<Command, String> {
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    let input = Path::new(binary);
    let dirs: Vec<PathBuf> = if input.components().count() > 1 {
        vec![PathBuf::new()]
    } else {
        std::env::split_paths(path).collect()
    };
    let extensions: Vec<String> = if input.extension().is_some() {
        vec![String::new()]
    } else {
        pathext
            .split(';')
            .filter(|e| !e.is_empty())
            .map(str::to_string)
            .collect()
    };
    let program = dirs
        .iter()
        .flat_map(|d| {
            extensions
                .iter()
                .map(move |ext| d.join(format!("{binary}{ext}")))
        })
        .find(|p| p.is_file())
        .ok_or("Program not found on PATH. Install the program, then Test again.")?;
    let batch = program
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));
    if !batch {
        let mut cmd = command(program);
        cmd.args(args);
        return Ok(cmd);
    }
    let program = program
        .canonicalize()
        .map_err(err)?
        .to_string_lossy()
        .trim_start_matches("\\\\?\\")
        .to_string();
    // Batch files run through cmd; unsupported expansion/metacharacters fail
    // closed rather than letting an imported argument become another command.
    if std::iter::once(program.as_str())
        .chain(args.iter().map(String::as_str))
        .any(|a| a.contains(['%', '!', '^', '&', '|', '<', '>', '"', '\r', '\n', '\0']))
    {
        return Err("Batch-program arguments contain unsupported shell syntax. Use an executable wrapper with literal arguments.".into());
    }
    let mut cmd = command(std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into()));
    let invocation = format!(
        "\"\"{}\"{}\"",
        program,
        args.iter().map(|a| format!(" \"{a}\"")).collect::<String>()
    );
    cmd.args(["/d", "/s", "/c"]);
    cmd.as_std_mut().raw_arg(invocation);
    Ok(cmd)
}

#[cfg(all(test, windows))]
mod windows_repair_tests {
    use super::*;
    #[tokio::test]
    async fn pathext_cmd_programs_accept_literal_arguments_and_refuse_injection() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("npx"), "#!/bin/sh\necho wrong shim").unwrap();
        let file = dir.path().join("npx.cmd");
        std::fs::write(&file, "@echo off\r\necho %~1\r\n").unwrap();
        let mut cmd = windows_mcp_command(
            "npx",
            dir.path().as_os_str(),
            ".EXE;.CMD",
            &["literal argument".into()],
        )
        .unwrap();
        assert!(output(&mut cmd, Duration::from_secs(5), &Cancel::new())
            .await
            .unwrap()
            .contains("literal argument"));
        for argument in [
            "x & echo injected",
            "%PATH%",
            "x\" & echo injected",
            "!evil!",
        ] {
            assert!(
                windows_mcp_command("npx", dir.path().as_os_str(), ".CMD", &[argument.into()])
                    .is_err()
            );
        }
    }
}
