//! Per-profile exclusive activation endpoint; owner-only local transport, no polling loop.
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Arc};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
pub struct Instance {
    #[cfg(windows)]
    _mutex: MutexLease,
    task: tokio::task::JoinHandle<()>,
    #[cfg(unix)]
    lock: std::fs::File,
    #[cfg(unix)]
    socket: std::path::PathBuf,
}
impl Drop for Instance {
    fn drop(&mut self) {
        self.task.abort();
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(&self.socket);
            unsafe {
                libc::flock(std::os::fd::AsRawFd::as_raw_fd(&self.lock), libc::LOCK_UN);
            }
        }
    }
}
pub enum Claim {
    Owner(Instance),
    Activated,
}
// The ownership guard becomes visible before its activation endpoint is bound.
// Retry only connection establishment; never replay an activation after sending it.
async fn connect_owner<S, F, Fut>(mut connect: F) -> std::io::Result<S>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = std::io::Result<S>>,
{
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match connect().await {
            Ok(stream) => return Ok(stream),
            Err(error)
                if (matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound
                        | std::io::ErrorKind::ConnectionRefused
                        | std::io::ErrorKind::WouldBlock
                ) || cfg!(windows) && error.raw_os_error() == Some(231))
                    && std::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            Err(error) => return Err(error),
        }
    }
}
async fn activate<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    mut stream: S,
) -> Result<(), String> {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        stream.write_all(b"activate\n").await?;
        let mut response = [0; 3];
        stream.read_exact(&mut response).await?;
        if &response != b"ok\n" {
            return Err(std::io::Error::other("Invalid activation reply"));
        }
        Ok::<_, std::io::Error>(())
    })
    .await
    .map_err(|_| "Activation timed out".to_string())?
    .map_err(|e| e.to_string())
}
async fn receive<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    mut stream: S,
    callback: &Arc<dyn Fn() + Send + Sync>,
) {
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let mut request = [0; 9];
        if stream.read_exact(&mut request).await.is_ok() && &request == b"activate\n" {
            callback();
            let _ = stream.write_all(b"ok\n").await;
        }
    })
    .await;
}
impl Instance {
    pub async fn claim(
        directory: &Path,
        identity: &str,
        callback: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Claim, String> {
        let directory = directory.canonicalize().map_err(|e| e.to_string())?;
        let key = format!(
            "{:x}",
            Sha256::digest(format!("{}:{identity}", directory.display()).as_bytes())
        );
        #[cfg(unix)]
        {
            use std::os::{
                fd::AsRawFd,
                unix::fs::{OpenOptionsExt, PermissionsExt},
            };
            let lock = std::fs::OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(directory.join(format!(".instance-{key}.lock")))
                .map_err(|e| e.to_string())?;
            let socket = directory.join(format!(".instance-{}.sock", &key[..16]));
            if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                activate(
                    connect_owner(|| tokio::net::UnixStream::connect(&socket))
                        .await
                        .map_err(|e| e.to_string())?,
                )
                .await?;
                return Ok(Claim::Activated);
            }
            if socket.exists() {
                std::fs::remove_file(&socket).map_err(|e| e.to_string())?;
            }
            let listener = tokio::net::UnixListener::bind(&socket).map_err(|e| e.to_string())?;
            std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
            let task = tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    receive(stream, &callback).await;
                }
            });
            Ok(Claim::Owner(Self { task, lock, socket }))
        }
        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ClientOptions;
            let name = format!(
                r"\\.\pipe\tezbar-{}-{key}",
                process_user_sid().map_err(|e| e.to_string())?
            );
            let mutex = owner_mutex(&format!(
                "Local\\tezbar-{}-{key}",
                process_user_sid().map_err(|e| e.to_string())?
            ))
            .map_err(|e| e.to_string())?;
            if !mutex.owner {
                let stream = connect_owner(|| std::future::ready(ClientOptions::new().open(&name)))
                    .await
                    .map_err(|e| e.to_string())?;
                activate(stream).await?;
                return Ok(Claim::Activated);
            }
            // A dropped owner aborts its async accept task. Its final pipe handle
            // may close on the next executor turn, after the mutex is released.
            // Owning the new mutex means we must wait for that endpoint to drain,
            // never activate an endpoint belonging to the retired owner.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            let server = loop {
                match owner_pipe(&name, true) {
                    Ok(server) => break server,
                    Err(error) if std::time::Instant::now() >= deadline => {
                        return Err(error.to_string())
                    }
                    Err(_) => tokio::time::sleep(std::time::Duration::from_millis(5)).await,
                }
            };
            let task = tokio::spawn(async move {
                let mut server = server;
                loop {
                    if server.connect().await.is_err() {
                        break;
                    }
                    let next = match owner_pipe(&name, false) {
                        Ok(next) => next,
                        Err(_) => break,
                    };
                    receive(server, &callback).await;
                    server = next;
                }
            });
            Ok(Claim::Owner(Self {
                task,
                _mutex: mutex,
            }))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn waits_for_owner_binding_and_does_not_retry_permission_errors() {
        let mut attempts = 0;
        let stream = connect_owner(|| {
            attempts += 1;
            std::future::ready(match attempts {
                1 => Err(std::io::ErrorKind::NotFound.into()),
                2 => Err(std::io::ErrorKind::WouldBlock.into()),
                _ => Ok(42),
            })
        })
        .await
        .unwrap();
        assert_eq!((stream, attempts), (42, 3));
        let mut denied_attempts = 0;
        let denied = connect_owner::<(), _, _>(|| {
            denied_attempts += 1;
            std::future::ready(Err(std::io::ErrorKind::PermissionDenied.into()))
        })
        .await
        .unwrap_err();
        assert_eq!(denied.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(denied_attempts, 1);
    }
    #[tokio::test]
    async fn second_instance_activates_owner_and_drop_releases_profile() {
        let d = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            tx.send(()).unwrap();
        });
        let Claim::Owner(owner) = Instance::claim(d.path(), "fixture", callback.clone())
            .await
            .unwrap()
        else {
            panic!("first must own")
        };
        assert!(matches!(
            Instance::claim(d.path(), "fixture", callback.clone())
                .await
                .unwrap(),
            Claim::Activated
        ));
        rx.recv().await.unwrap();
        drop(owner);
        assert!(matches!(
            Instance::claim(d.path(), "fixture", callback)
                .await
                .unwrap(),
            Claim::Owner(_)
        ));
    }
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
    let sddl: Vec<u16> = format!("D:P(A;;GA;;;{})", process_user_sid()?)
        .encode_utf16()
        .chain(Some(0))
        .collect();
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

#[cfg(windows)]
fn process_user_sid() -> std::io::Result<String> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::ConvertSidToStringSidW, GetTokenInformation, TokenUser, TOKEN_QUERY,
            TOKEN_USER,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    unsafe {
        let mut raw = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let token = OwnedHandle::from_raw_handle(raw);
        let mut length = 0;
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut length,
        );
        if length == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut buffer = vec![
            0usize;
            (length as usize + std::mem::size_of::<usize>() - 1)
                / std::mem::size_of::<usize>()
        ];
        if GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        ) == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut text = std::ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut count = 0;
        while *text.add(count) != 0 {
            count += 1;
        }
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, count));
        LocalFree(text.cast());
        Ok(sid)
    }
}
#[cfg(windows)]
struct MutexLease {
    _handle: std::os::windows::io::OwnedHandle,
    owner: bool,
}
#[cfg(windows)]
fn owner_mutex(name: &str) -> std::io::Result<MutexLease> {
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::{
        Foundation::{GetLastError, LocalFree, ERROR_ALREADY_EXISTS},
        Security::{
            Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SECURITY_ATTRIBUTES,
        },
        System::Threading::CreateMutexW,
    };
    let sddl: Vec<u16> = format!("D:P(A;;GA;;;{})", process_user_sid()?)
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let mut descriptor = std::ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let handle = CreateMutexW(&attributes, 0, name.as_ptr());
        let existed = GetLastError() == ERROR_ALREADY_EXISTS;
        LocalFree(descriptor);
        if handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let owner = !existed;
        Ok(MutexLease {
            _handle: std::os::windows::io::OwnedHandle::from_raw_handle(handle),
            owner,
        })
    }
}
#[cfg(test)]
mod process_tests {
    use super::*;
    #[tokio::test]
    async fn isolated_activation_child() {
        let Some(root) = std::env::var_os("TEZBAR_INSTANCE_FIXTURE") else {
            return;
        };
        assert!(matches!(
            Instance::claim(
                Path::new(&root),
                "fixture-two-process",
                Arc::new(|| panic!("child must not own"))
            )
            .await
            .unwrap(),
            Claim::Activated
        ));
    }
    #[tokio::test]
    async fn actual_second_process_activates_only_its_profile() {
        let d = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = tx.send(());
        });
        let Claim::Owner(_owner) = Instance::claim(d.path(), "fixture-two-process", callback)
            .await
            .unwrap()
        else {
            panic!("owner missing")
        };
        let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "single_instance::process_tests::isolated_activation_child",
            ])
            .env("TEZBAR_INSTANCE_FIXTURE", d.path())
            .kill_on_drop(true);
        let output = tokio::time::timeout(std::time::Duration::from_secs(5), command.output())
            .await
            .unwrap()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        rx.recv().await.unwrap();
        assert!(rx.try_recv().is_err());
    }
}
