//! Runs the actual Preview host -> CoreClient -> native PTY path in a child with
//! a private HOME and shell, so no user shell startup/profile/credentials are read.
use super::*;
use crate::{events::Relay, host_adapter::GpuiHost, surfaces::terminal::TerminalRequest};
use ::core::prelude::v1::test;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::sync::Mutex;
type Events = Arc<Mutex<Vec<(String, Value)>>>;
use tezbar_core::host::*;
struct RecordingPreviewHost {
    inner: GpuiHost,
    events: Events,
}
impl Host for RecordingPreviewHost {
    fn emit(&self, channel: &str, payload: Value) {
        self.events.lock().unwrap().push((channel.into(), payload));
    }
    fn desktop_services(&self) -> bool {
        self.inner.desktop_services()
    }
    fn native_terminal(&self) -> bool {
        self.inner.native_terminal()
    }
    fn surface(&self, op: SurfaceOp) -> HostFuture<()> {
        self.inner.surface(op)
    }
    fn dialog(&self, req: DialogRequest) -> HostFuture<DialogReply> {
        self.inner.dialog(req)
    }
    fn shortcuts(&self) -> &dyn ShortcutRegistry {
        self.inner.shortcuts()
    }
    fn status_items(&self) -> &dyn StatusItems {
        self.inner.status_items()
    }
    fn capture_active_display(&self) -> HostFuture<Vec<u8>> {
        self.inner.capture_active_display()
    }
}

#[test]
fn real_preview_pty_is_available_without_production_services() {
    if std::env::var_os("TEZBAR_PRIVATE_PTY_CHILD").is_none() {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "core_client::preview_terminal_tests::real_preview_pty_is_available_without_production_services", "--nocapture"])
            .env_clear().env("PATH", "/usr/bin:/bin").env("HOME", root.path()).env("SHELL", "/bin/sh")
            .env("ZDOTDIR", root.path()).env("ENV", "").env("BASH_ENV", "")
            .env("TEZBAR_PRIVATE_PTY_CHILD", root.path()).output().unwrap();
        assert!(
            output.status.success(),
            "Private Preview PTY regression failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let root = std::path::PathBuf::from(std::env::var_os("TEZBAR_PRIVATE_PTY_CHILD").unwrap());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let (relay, _events) = Relay::new();
        let events: Events = Arc::default();
        let host = Arc::new(RecordingPreviewHost {
            inner: GpuiHost::fixture(relay),
            events: events.clone(),
        });
        assert!(
            !host.desktop_services(),
            "Preview must not enable production desktop services"
        );
        assert!(host.native_terminal());
        assert!(
            !TestHost::default().native_terminal(),
            "Generic headless hosts must not gain PTY access"
        );
        let context = tezbar_core::common::CoreContext::new(
            tezbar_core::common::CorePaths {
                data_dir: root.join("data"),
                config_dir: root.join("config"),
                resource_dir: root.join("resources"),
                home_dir: root.clone(),
                temp_dir: root.join("tmp"),
            },
            Some(host),
        );
        assert!(!context.runs_desktop_services());
        let core = tezbar_core::NativeCore::new(context).unwrap();
        struct Cleanup(Arc<tezbar_core::terminal::pty::NativeTerminalState>);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                tezbar_core::terminal::pty::shutdown_all(&self.0);
            }
        }
        let _cleanup = Cleanup(core.ctx.terminal.clone());
        core.start_with_options(tezbar_core::StartupOptions::ISOLATED_PREVIEW)
            .await
            .unwrap();
        let client = CoreClient::new(core);
        let one = create(&client, &root).await;
        let generation_one = attach(&client, &one, 80, 24).await;
        input(
            &client,
            &one,
            &generation_one,
            "printf '%s%s\\n' 'PREVIEW_' 'READY'\n",
        )
        .await;
        wait_output(&events, &one, "PREVIEW_READY").await;
        input(
            &client,
            &one,
            &generation_one,
            "read reply; printf '%s%s\\n' 'INPUT=' \"$reply\"\n",
        )
        .await;
        input(&client, &one, &generation_one, "hello-private\n").await;
        wait_output(&events, &one, "INPUT=hello-private").await;
        request(
            &client,
            TerminalRequest::Resize {
                id: one.clone(),
                cols: 101,
                rows: 33,
            },
        )
        .await;
        input(
            &client,
            &one,
            &generation_one,
            "printf '%s' 'TEZBAR_SIZE='; stty size\n",
        )
        .await;
        wait_output(&events, &one, "TEZBAR_SIZE=33 101").await;
        let pid_one = shell_pid(&client, &events, &one, &generation_one, "PID_ONE=").await;

        let two = create(&client, &root).await;
        let generation_two = attach(&client, &two, 90, 28).await;
        let pid_two = shell_pid(&client, &events, &two, &generation_two, "PID_TWO=").await;
        assert_ne!(one, two);
        let listed = request(&client, TerminalRequest::List).await;
        assert_eq!(
            listed
                .as_array()
                .unwrap()
                .iter()
                .filter(|v| v["status"] == "running")
                .count(),
            2
        );
        request(
            &client,
            TerminalRequest::DetachGeneration {
                id: one.clone(),
                generation: generation_one.clone(),
            },
        )
        .await;
        input(
            &client,
            &two,
            &generation_two,
            "printf '%s%s\\n' 'SWITCH_' 'TWO'\n",
        )
        .await;
        wait_output(&events, &two, "SWITCH_TWO").await;
        let generation_one = attach(&client, &one, 101, 33).await;
        input(
            &client,
            &one,
            &generation_one,
            "printf '%s%s\\n' 'SWITCH_' 'ONE'\n",
        )
        .await;
        wait_output(&events, &one, "SWITCH_ONE").await;
        assert!(!output(&events, &two).contains("SWITCH_ONE"));
        assert_eq!(
            request(
                &client,
                TerminalRequest::Input {
                    id: two.clone(),
                    generation: generation_one.clone(),
                    bytes: b"wrong owner\n".to_vec()
                }
            )
            .await,
            json!(false)
        );

        input(&client, &one, &generation_one, "exit 7\n").await;
        wait_exit(&events, &one).await;
        assert!(!alive(pid_one), "Exited Preview shell must be reaped");
        request(&client, TerminalRequest::Kill(two.clone())).await;
        wait_exit(&events, &two).await;
        assert!(!alive(pid_two), "Stopped Preview shell must be reaped");

        let three = create(&client, &root).await;
        let generation_three = attach(&client, &three, 80, 24).await;
        let pid_three = shell_pid(&client, &events, &three, &generation_three, "PID_THREE=").await;
        input(
            &client,
            &three,
            &generation_three,
            "/bin/sh -c 'sleep 60 & printf \"CHILD_PID=%s\\n\" $!; wait'\n",
        )
        .await;
        let child_pid = wait_pid(&events, &three, "CHILD_PID=").await;
        client.0.shutdown().await;
        assert!(
            !alive(pid_three) && !alive(child_pid),
            "Shutdown/cancel must reap the private shell and its child"
        );
    });
}

async fn request(client: &CoreClient, request: TerminalRequest) -> Value {
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.request(Request::Terminal(request)),
    )
    .await
    .expect("PTY request deadline")
    .expect("Private Preview PTY request must succeed")
}
async fn create(client: &CoreClient, root: &std::path::Path) -> String {
    let mut req = TerminalRequest::Create(Default::default());
    if let TerminalRequest::Create(create) = &mut req {
        create.cwd = Some(root.to_string_lossy().into());
        create.cols = 80;
        create.rows = 24;
    }
    request(client, req).await["sessionId"]
        .as_str()
        .expect("PTY session identity")
        .to_owned()
}
async fn attach(client: &CoreClient, id: &str, cols: u16, rows: u16) -> String {
    let attached = request(
        client,
        TerminalRequest::Attach {
            id: id.into(),
            cols,
            rows,
        },
    )
    .await;
    let generation = attached["generation"].as_str().unwrap().to_owned();
    let replay = attached["replay"].as_str().unwrap().to_owned();
    let snapshot = request(
        client,
        TerminalRequest::Replay {
            token: replay.clone(),
            position: attached["origin"].as_u64().unwrap_or(0),
        },
    )
    .await;
    assert!(
        snapshot["bytes"].is_string(),
        "Native replay capability must remain readable"
    );
    request(client, TerminalRequest::Release(replay)).await;
    generation
}
async fn input(client: &CoreClient, id: &str, generation: &str, text: &str) {
    assert_eq!(
        request(
            client,
            TerminalRequest::Input {
                id: id.into(),
                generation: generation.into(),
                bytes: text.as_bytes().to_vec()
            }
        )
        .await,
        json!(true)
    );
}
fn output(events: &Events, id: &str) -> String {
    let mut bytes = Vec::new();
    for (channel, value) in events.lock().unwrap().iter() {
        if channel == "core:terminal:data" && value["sessionId"] == id {
            bytes.extend(STANDARD.decode(value["bytes"].as_str().unwrap()).unwrap());
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}
async fn wait_output(events: &Events, id: &str, marker: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !output(events, id).contains(marker) {
        assert!(
            std::time::Instant::now() < deadline,
            "Expected private PTY marker {marker}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}
async fn wait_pid(events: &Events, id: &str, marker: &str) -> u32 {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let text = output(events, id);
        for (at, _) in text.match_indices(marker) {
            let digits = text[at + marker.len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>();
            if let Ok(pid) = digits.parse::<u32>() {
                return pid;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Private shell PID deadline for {marker}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}
async fn shell_pid(
    client: &CoreClient,
    events: &Events,
    id: &str,
    generation: &str,
    marker: &str,
) -> u32 {
    input(
        client,
        id,
        generation,
        &format!("printf '{marker}%s\\n' $$\n"),
    )
    .await;
    wait_pid(events, id, marker).await
}
async fn wait_exit(events: &Events, id: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if events
            .lock()
            .unwrap()
            .iter()
            .any(|(c, v)| c == "core:terminal:exit-event" && v["sessionId"] == id)
        {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Private shell exit deadline"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}
fn alive(pid: u32) -> bool {
    std::process::Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}
