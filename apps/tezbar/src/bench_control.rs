//! Authenticated, bounded loopback fixture. Compiled out of ordinary builds.
use crate::events::{Event, Relay};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
pub struct Server {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    address: std::net::SocketAddr,
    endpoint: PathBuf,
}
fn token_equal(a: &str, b: &str) -> bool {
    let mut mismatch = a.len() ^ b.len();
    for (a, b) in a.bytes().zip(b.bytes()) {
        mismatch |= (a ^ b) as usize;
    }
    mismatch == 0
}
impl Server {
    pub fn start(profile: &Path, token: &str, relay: Relay) -> Result<Self, String> {
        if token.len() < 32 || token.len() > 256 {
            return Err("Control token must be 32–256 bytes".into());
        }
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        let endpoint = profile.join("native-control.json");
        std::fs::write(
            &endpoint,
            serde_json::json!({"address":address.to_string(),"pid":std::process::id()}).to_string(),
        )
        .map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let token = token.to_owned();
        let thread = std::thread::spawn(move || {
            for connection in listener.incoming() {
                if stopping.load(Ordering::Acquire) {
                    break;
                }
                let Ok(mut stream) = connection else {
                    break;
                };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                let mut bytes = vec![];
                let mut byte = [0];
                while bytes.len() < 4096 {
                    match stream.read(&mut byte) {
                        Ok(1) if byte[0] != b'\n' => bytes.push(byte[0]),
                        _ => break,
                    }
                }
                let result = (|| {
                    if bytes.len() >= 4096 {
                        return Err("Control request too large".into());
                    }
                    let request: serde_json::Value =
                        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                    if !token_equal(request["token"].as_str().unwrap_or(""), &token) {
                        return Err("Control authentication failed".into());
                    }
                    let (tx, rx) = std::sync::mpsc::sync_channel(1);
                    relay.send(Event::Control(request, tx))?;
                    rx.recv_timeout(Duration::from_secs(3))
                        .map_err(|e| e.to_string())?
                })();
                let value = match result {
                    Ok(value) => serde_json::json!({"ok":true,"result":value}),
                    Err(e) => serde_json::json!({"ok":false,"error":e}),
                };
                let _ = writeln!(stream, "{value}");
            }
        });
        Ok(Self {
            stop,
            thread: Some(thread),
            address,
            endpoint,
        })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = std::fs::remove_file(&self.endpoint);
        let _ = TcpStream::connect_timeout(&self.address, Duration::from_millis(100));
        if let Some(thread) = self.thread.take() {
            // This thread owns no GUI state. A reply timeout bounds its final request.
            std::thread::spawn(move || {
                let _ = thread.join();
            });
        }
    }
}
#[derive(Default)]
pub struct ShutdownGate {
    held: AtomicBool,
    wake: tokio::sync::Notify,
}
impl ShutdownGate {
    pub fn hold(&self) {
        self.held.store(true, Ordering::Release);
    }
    pub fn release(&self) {
        self.held.store(false, Ordering::Release);
        self.wake.notify_one();
    }
    pub async fn wait(&self) {
        while self.held.load(Ordering::Acquire) {
            self.wake.notified().await;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authentication_compares_full_token() {
        assert!(token_equal("abcd", "abcd"));
        assert!(!token_equal("abcd", "abc"));
        assert!(!token_equal("abcd", "abce"));
    }
}
