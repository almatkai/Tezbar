//! Private local, bounded, token-authenticated JSON-lines control. No JS/raw commands.
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    sync::{mpsc, oneshot},
};
pub struct Command {
    pub request: Value,
    pub reply: oneshot::Sender<Result<Value, String>>,
}
pub struct Server {
    task: tokio::task::JoinHandle<()>,
    socket: Option<PathBuf>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
        if let Some(p) = &self.socket {
            let _ = std::fs::remove_file(p);
        }
    }
}
impl Server {
    pub fn start(root: PathBuf, token: String, tx: mpsc::Sender<Command>) -> Result<Self, String> {
        if token.len() < 32 || !token.is_ascii() {
            return Err("control token must contain at least 32 ASCII characters".into());
        }
        let token = Arc::new(token);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let socket = root.join("control.sock");
            let listener = tokio::net::UnixListener::bind(&socket).map_err(|e| e.to_string())?;
            std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
            let task = tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    handle(stream, &token, &tx).await;
                }
            });
            Ok(Self {
                task,
                socket: Some(socket),
            })
        }
        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ServerOptions;
            let pipe = format!(r"\\.\pipe\tezbar-native-{}", std::process::id());
            let first = owner_pipe(&pipe, true).map_err(|e| e.to_string())?;
            std::fs::write(root.join("control-pipe.txt"), &pipe).map_err(|e| e.to_string())?;
            let task = tokio::spawn(async move {
                let mut server = first;
                loop {
                    if server.connect().await.is_err() {
                        break;
                    }
                    let next = match owner_pipe(&pipe, false) {
                        Ok(v) => v,
                        Err(_) => break,
                    };
                    handle(server, &token, &tx).await;
                    server = next;
                }
            });
            Ok(Self {
                task,
                socket: Some(root.join("control-pipe.txt")),
            })
        }
    }
}
async fn handle<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    token: &str,
    tx: &mpsc::Sender<Command>,
) {
    let mut stream = BufReader::new(stream);
    let mut bytes = vec![];
    use tokio::io::AsyncReadExt;
    let read = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        (&mut stream).take(65537).read_until(b'\n', &mut bytes),
    )
    .await;
    let result = async {
        read.map_err(|_| "control read timeout")?
            .map_err(|_| "control read failed")?;
        if bytes.len() > 65536 {
            return Err("control request too large".into());
        }
        let mut request: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid JSON")?;
        // Constant-work comparison across the configured length; do not log tokens.
        let supplied = request["token"].as_str().unwrap_or("");
        let mut difference = supplied.len() ^ token.len();
        for (a, b) in supplied.bytes().zip(token.bytes()) {
            difference |= (a ^ b) as usize;
        }
        if difference != 0 {
            return Err("unauthorized".into());
        }
        request
            .as_object_mut()
            .ok_or("expected object")?
            .remove("token");
        let (reply, rx) = oneshot::channel();
        tx.send(Command { request, reply })
            .await
            .map_err(|_| "launcher closed")?;
        tokio::time::timeout(std::time::Duration::from_secs(35), rx)
            .await
            .map_err(|_| "operation timeout")?
            .map_err(|_| "launcher closed")?
    }
    .await;
    let response = match result {
        Ok(v) => json!({"ok":true,"value":v}),
        Err(e) => json!({"ok":false,"error":e}),
    };
    let _ = stream.write_all(format!("{response}\n").as_bytes()).await;
}

#[cfg(windows)]
fn owner_pipe(
    name: &str,
    first: bool,
) -> std::io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
    use tokio::net::windows::named_pipe::ServerOptions;
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SECURITY_ATTRIBUTES,
        },
    };
    let sddl: Vec<u16> = "D:P(A;;GA;;;OW)".encode_utf16().chain(Some(0)).collect();
    let mut descriptor = std::ptr::null_mut();
    unsafe {
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let result = ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(name, &mut attributes as *mut _ as *mut _);
        LocalFree(descriptor);
        result
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[tokio::test]
    async fn rejects_unauthenticated_requests_before_dispatch() {
        let root = tempfile::tempdir().unwrap();
        let token = "a".repeat(64);
        let (tx, mut rx) = mpsc::channel(1);
        let server = Server::start(root.path().into(), token.clone(), tx).unwrap();
        let mut bad = tokio::net::UnixStream::connect(root.path().join("control.sock"))
            .await
            .unwrap();
        bad.write_all(b"{\"op\":\"quit\",\"token\":\"wrong\"}\n")
            .await
            .unwrap();
        let mut line = String::new();
        BufReader::new(bad).read_line(&mut line).await.unwrap();
        assert!(line.contains("unauthorized"));
        assert!(rx.try_recv().is_err());
        let mut good = tokio::net::UnixStream::connect(root.path().join("control.sock"))
            .await
            .unwrap();
        good.write_all(format!("{}\n", json!({"op":"status","token":token})).as_bytes())
            .await
            .unwrap();
        let command = rx.recv().await.unwrap();
        assert_eq!(command.request, json!({"op":"status"}));
        command.reply.send(Ok(json!({"ready":true}))).unwrap();
        let mut line = String::new();
        BufReader::new(good).read_line(&mut line).await.unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&line).unwrap()["value"]["ready"],
            true
        );
        drop(server);
        assert!(!root.path().join("control.sock").exists());
    }
}
