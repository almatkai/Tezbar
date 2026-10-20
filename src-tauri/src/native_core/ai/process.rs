use super::*;
use std::process::Stdio;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
};
#[derive(Clone)]
pub struct Cancel(tokio::sync::watch::Sender<bool>);
impl Cancel {
    pub fn new() -> Self {
        Self(tokio::sync::watch::channel(false).0)
    }
    pub fn cancel(&self) {
        self.0.send_replace(true);
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
    c.creation_flags(0x08000200);
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
}
impl GroupChild {
    pub fn spawn(cmd: &mut Command) -> Result<Self, String> {
        let child = cmd.spawn().map_err(err)?;
        let pid = child.id();
        Ok(Self { child, pid })
    }
    pub async fn terminate(&mut self) {
        #[cfg(unix)]
        if let Some(pid) = self.pid {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGTERM);
            }
        }
        #[cfg(windows)]
        if let Some(pid) = self.pid.take() {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .output()
                .await;
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
        if let Some(pid) = self.pid.take() {
            // Drop can run when an outer timeout cancels the whole request future.
            // Launch tree termination immediately, then reap asynchronously when a runtime exists.
            let mut command = Command::new("taskkill");
            command
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(0x08000000);
            if let Ok(mut killer) = command.spawn() {
                if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                    runtime.spawn(async move {
                        let _ = killer.wait().await;
                    });
                }
            }
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
    let mut group = GroupChild::spawn(cmd)?;
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
/// Bytes remain bytes until LF arrives: split UTF-8 and U+2028/U+2029 are safe.
#[derive(Default)]
pub struct Lines {
    buffer: Vec<u8>,
}
impl Lines {
    pub fn feed(&mut self, bytes: &[u8], eof: bool) -> Result<Vec<String>, String> {
        self.buffer.extend_from_slice(bytes);
        if self.buffer.len() > 16 * 1024 * 1024 {
            return Err("Protocol line exceeds 16 MiB".into());
        }
        let mut lines = Vec::new();
        while let Some(i) = self.buffer.iter().position(|b| *b == b'\n') {
            let mut line: Vec<u8> = self.buffer.drain(..=i).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            lines.push(String::from_utf8(line).map_err(err)?);
        }
        if eof && !self.buffer.is_empty() {
            lines.push(String::from_utf8(std::mem::take(&mut self.buffer)).map_err(err)?);
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
