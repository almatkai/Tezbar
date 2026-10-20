//! Opt-in tests of the real Rust supervisor and the built Bun compatibility worker.
use super::*;
use crate::native_core::common::CorePaths;
use std::sync::atomic::AtomicUsize;

const WAIT: Duration = Duration::from_secs(10);

async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(WAIT, future)
        .await
        .expect("real worker integration test timed out")
}

async fn until(mut condition: impl FnMut() -> bool) {
    bounded(async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
}

struct ActiveHost(Arc<AtomicUsize>);
impl Drop for ActiveHost {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

// Explicit invocation required. Missing prerequisites FAIL rather than silently skip.
// TEZBAR_TEST_BUN may select an already-installed runtime; this test never downloads one.
#[tokio::test]
#[ignore = "requires built dist-backend/extension-worker.js and installed Bun; see TEZBAR_TEST_BUN"]
async fn real_rust_host_bun_roundtrip_and_termination() {
    let runtime = std::env::var_os("TEZBAR_TEST_BUN")
        .map(PathBuf::from)
        .or_else(|| {
            let name = if cfg!(windows) { "bun.exe" } else { "bun" };
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .map(|path| path.join(name))
                .find(|path| path.is_file())
        })
        .expect(
            "Install Bun or set TEZBAR_TEST_BUN to an existing executable (no download attempted)",
        );
    assert!(runtime.is_file(), "TEZBAR_TEST_BUN must exist");
    let runtime = std::fs::canonicalize(runtime).expect("installed runtime must resolve");
    let version = std::process::Command::new(&runtime)
        .arg("--version")
        .output()
        .expect("installed Bun must execute");
    assert!(version.status.success(), "Bun --version failed");
    eprintln!(
        "Rust host integration runtime: {} ({})",
        runtime.display(),
        String::from_utf8_lossy(&version.stdout).trim()
    );
    let resources = std::env::var_os("TEZBAR_TEST_RESOURCES")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."));
    assert!(
        resources.join("dist-backend/extension-worker.js").is_file(),
        "Build the extension worker before explicitly running this test"
    );

    let root = tempfile::tempdir().unwrap();
    let paths = CorePaths {
        data_dir: root.path().join("data"),
        config_dir: root.path().join("config"),
        resource_dir: resources,
        temp_dir: root.path().join("temp"),
        home_dir: root.path().to_owned(),
    };
    let cached = paths
        .data_dir
        .join("bun")
        .join(if cfg!(windows) { "bun.exe" } else { "bun" });
    std::fs::create_dir_all(cached.parent().unwrap()).unwrap();
    // Cache lookup precedes profile/PATH discovery, so production launch cannot download.
    if std::fs::hard_link(&runtime, &cached).is_err() {
        std::fs::copy(&runtime, &cached).unwrap();
    }
    assert!(
        cached.is_file(),
        "staged runtime must exist before production discovery"
    );
    assert_eq!(crate::locate_bun(&paths.data_dir).unwrap(), cached);
    let extension = paths.data_dir.join("extensions/protocol-fixture");
    std::fs::create_dir_all(extension.join(".sc-build")).unwrap();
    std::fs::write(extension.join("package.json"), json!({
        "name":"protocol-fixture", "title":"Rust supervisor fixture",
        "platforms":["macOS","Windows","Linux"],
        "commands":(["run","failure","hang","crash"].map(|name| json!({"name":name,"title":name,"mode":"no-view"})))
    }).to_string()).unwrap();
    for (name, body) in [
        ("run", "const text = await Clipboard.readText(); const answer = await AI.ask(text); await Clipboard.copy(text + answer); await Clipboard.copy('pid:' + process.pid);"),
        ("failure", "await Clipboard.copy('reject this write');"),
        ("hang", "await Clipboard.readText();"),
        ("crash", "process.exit(23);"),
    ] {
        std::fs::write(extension.join(".sc-build").join(format!("{name}.js")), format!(
            "const {{ Clipboard, AI }} = require('@raycast/api'); exports.default = async () => {{ {body} }};"
        )).unwrap();
    }
    let events = Arc::new(Mutex::new(Vec::new()));
    let event_log = events.clone();
    let ctx = CoreContext::headless(paths).with_event_sink(Arc::new(move |channel, value| {
        eprintln!("worker event {channel}: {value}");
        event_log.lock().unwrap().push((channel.to_owned(), value));
    }));
    let original = Arc::new(Mutex::new(String::new()));
    let writes = Arc::new(Mutex::new(Vec::<String>::new()));
    let block_read = Arc::new(AtomicBool::new(false));
    let active = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(Mutex::new(Vec::new()));
    ctx.set_router(Arc::new({
        let original = original.clone();
        let writes = writes.clone();
        let block_read = block_read.clone();
        let active = active.clone();
        let calls = calls.clone();
        move |channel, args| {
            let original = original.clone();
            let writes = writes.clone();
            let block_read = block_read.clone();
            let active = active.clone();
            eprintln!("host request: {channel}");
            calls.lock().unwrap().push(channel.clone());
            Box::pin(async move {
                active.fetch_add(1, Ordering::SeqCst);
                let _active = ActiveHost(active);
                match channel.as_str() {
                    "core:apps:list" => Ok(json!([])),
                    "clipboard:read" => {
                        if block_read.load(Ordering::SeqCst) {
                            std::future::pending::<()>().await;
                        }
                        Ok(json!(original.lock().unwrap().clone()))
                    }
                    "core:ai:extension-ask" => {
                        assert_eq!(args[0], json!(original.lock().unwrap().clone()));
                        Ok(json!(" ✓🐎\u{2028}\u{2029}"))
                    }
                    "clipboard:write" => {
                        let text = args[0].as_str().expect("fixture clipboard text");
                        if text == "reject this write" {
                            return Err("fixture write rejected".into());
                        }
                        writes.lock().unwrap().push(text.to_owned());
                        Ok(json!({"ok":true}))
                    }
                    _ => Err(format!("Unexpected real host request: {channel}")),
                }
            })
        }
    }))
    .unwrap();
    let worker = ExtensionWorker::new(ctx);
    let invoke = |command: &str| {
        vec![json!({"extensionId":"raycast.protocol-fixture", "commandName":command})]
    };
    assert!(!worker.is_running().await);
    let mut pids = Vec::new();
    for text in [
        "ASCII clipboard",
        "Қазақша\u{2028}日本語🐎\u{2029}",
        "escaped\r\n\0",
    ] {
        *original.lock().unwrap() = text.to_owned();
        writes.lock().unwrap().clear();
        calls.lock().unwrap().clear();
        let result = bounded(worker.request("extension:run-command", invoke("run")))
            .await
            .unwrap();
        assert_eq!(result["ok"], true, "{result}");
        let captured = writes.lock().unwrap().clone();
        assert_eq!(captured[0], format!("{text} ✓🐎\u{2028}\u{2029}"));
        pids.push(
            captured[1]
                .strip_prefix("pid:")
                .unwrap()
                .parse::<u32>()
                .unwrap(),
        );
        assert_eq!(
            *calls.lock().unwrap(),
            vec![
                "core:apps:list",
                "clipboard:read",
                "core:ai:extension-ask",
                "clipboard:write",
                "clipboard:write"
            ]
        );
    }
    assert!(
        pids.iter().all(|pid| *pid == pids[0]),
        "worker should be reused"
    );
    assert!(events
        .lock()
        .unwrap()
        .iter()
        .any(|(c, v)| c == "extension-worker-status" && v["phase"] == "ready"));
    let result = bounded(worker.request("extension:run-command", invoke("failure")))
        .await
        .unwrap();
    assert_eq!(result["ok"], false);
    assert!(result["message"]
        .as_str()
        .unwrap()
        .contains("fixture write rejected"));
    assert!(worker
        .request("search:all", vec![])
        .await
        .unwrap_err()
        .contains("not an extension operation"));
    assert!(bounded(worker.request("extension:run-command", vec![]))
        .await
        .unwrap_err()
        .contains("extensionId"));

    // Cancelling a Rust caller must immediately remove its reply sender.
    // The independent host task stays owned by the process until it is terminated.
    block_read.store(true, Ordering::SeqCst);
    let caller = tokio::spawn({
        let worker = worker.clone();
        let args = invoke("hang");
        async move { worker.request("extension:run-command", args).await }
    });
    until(|| active.load(Ordering::SeqCst) == 1).await;
    let pending = worker
        .process
        .lock()
        .await
        .as_ref()
        .unwrap()
        .pending
        .clone();
    assert_eq!(pending.lock().unwrap().len(), 1);
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    assert!(pending.lock().unwrap().is_empty());

    // A second outstanding request must be rejected by a real process crash.
    let caller = tokio::spawn({
        let worker = worker.clone();
        let args = invoke("hang");
        async move { worker.request("extension:run-command", args).await }
    });
    until(|| active.load(Ordering::SeqCst) == 2).await;
    assert!(
        bounded(worker.request("extension:run-command", invoke("crash")))
            .await
            .is_err()
    );
    assert!(bounded(caller)
        .await
        .unwrap()
        .unwrap_err()
        .contains("stopped"));
    until(|| active.load(Ordering::SeqCst) == 0).await;
    assert!(pending.lock().unwrap().is_empty());
    #[cfg(unix)]
    assert!(!process_exists(pids[0]), "crashed worker must be reaped");

    block_read.store(false, Ordering::SeqCst);
    writes.lock().unwrap().clear();
    assert_eq!(
        bounded(worker.request("extension:run-command", invoke("run")))
            .await
            .unwrap()["ok"],
        true
    );
    let restarted_pid: u32 = writes.lock().unwrap()[1]
        .strip_prefix("pid:")
        .unwrap()
        .parse()
        .unwrap();
    assert_ne!(restarted_pid, pids[0]);
    block_read.store(true, Ordering::SeqCst);
    let caller = tokio::spawn({
        let worker = worker.clone();
        let args = invoke("hang");
        async move { worker.request("extension:run-command", args).await }
    });
    until(|| active.load(Ordering::SeqCst) == 1).await;
    let pending = worker
        .process
        .lock()
        .await
        .as_ref()
        .unwrap()
        .pending
        .clone();
    bounded(worker.shutdown()).await;
    assert_eq!(
        active.load(Ordering::SeqCst),
        0,
        "shutdown must await host cancellation"
    );
    assert!(bounded(caller)
        .await
        .unwrap()
        .unwrap_err()
        .contains("stopped"));
    assert!(pending.lock().unwrap().is_empty());
    assert!(!worker.is_running().await);
    #[cfg(unix)]
    assert!(!process_exists(restarted_pid), "shutdown must reap worker");
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, v)| c == "extension-worker-status" && v["phase"] == "stopped")
            .count(),
        2
    );
    // Explicit shutdown remains restartable and idempotent.
    block_read.store(false, Ordering::SeqCst);
    assert_eq!(
        bounded(worker.request("extension:run-command", invoke("run")))
            .await
            .unwrap()["ok"],
        true
    );
    let last_pid: u32 = writes
        .lock()
        .unwrap()
        .last()
        .unwrap()
        .strip_prefix("pid:")
        .unwrap()
        .parse()
        .unwrap();
    // An oversized outgoing frame makes the real writer finish. The supervisor
    // must kill/reap Bun and reject callers instead of leaving them to time out.
    let pending = worker
        .process
        .lock()
        .await
        .as_ref()
        .unwrap()
        .pending
        .clone();
    assert!(
        bounded(worker.request("extension:run-command", vec![json!("x".repeat(MAX_FRAME))]))
            .await
            .is_err()
    );
    bounded(worker.shutdown()).await;
    assert!(pending.lock().unwrap().is_empty());
    #[cfg(unix)]
    assert!(!process_exists(last_pid), "writer failure must reap worker");
    bounded(worker.shutdown()).await;
    assert_eq!(active.load(Ordering::SeqCst), 0);
}
