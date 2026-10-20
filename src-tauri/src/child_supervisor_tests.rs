use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Instant,
};

/// Compile the actual supervisor into a tiny, isolated executable. No Tauri,
/// models, profile reads, shell profiles, or mutation of the test process env.
pub(crate) fn fixture_executable() -> PathBuf {
    static FIXTURE: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("src")).unwrap();
        fs::write(directory.path().join("Cargo.toml"), "[package]\nname='tezbar-supervisor-fixture'\nversion='0.0.0'\nedition='2021'\n[workspace]\n[dependencies]\nlibc='0.2'\n").unwrap();
        let module = directory.path().join("src/child_supervisor.rs");
        fs::write(&module, include_str!("child_supervisor.rs")).unwrap();
        let source = format!("#[path = {module:?}] mod child_supervisor;\n{}", r#"
use std::{process::{Command, Stdio}, time::Duration, path::Path};
fn main() {
 if let Some(code) = child_supervisor::dispatch() { std::process::exit(code); }
 let args: Vec<_> = std::env::args_os().collect();
 let mode = args[1].to_str().unwrap();
 let dir = Path::new(&args[2]);
 if mode == "parent" {
  let mut child = Command::new(std::env::current_exe().unwrap())
   .arg(child_supervisor::MODE).arg(std::env::current_exe().unwrap())
   .arg("tree").arg(dir).stdin(Stdio::piped()).spawn().unwrap();
  std::fs::write(dir.join("supervisor"), child.id().to_string()).unwrap();
  loop { std::thread::sleep(Duration::from_secs(1)); let _ = &mut child; }
 }
 if mode == "tree" || mode == "fail" {
  let _leaf = Command::new(std::env::current_exe().unwrap()).arg("leaf").arg(dir).spawn().unwrap();
  std::fs::write(dir.join("root"), std::process::id().to_string()).unwrap();
  while !dir.join("leaf").exists() { std::thread::sleep(Duration::from_millis(10)); }
  if mode == "fail" { eprintln!("fixture child failure"); std::process::exit(23); }
 } else {
  std::fs::write(dir.join("leaf"), std::process::id().to_string()).unwrap();
 }
 loop { std::thread::sleep(Duration::from_secs(1)); }
}
"#);
        fs::write(directory.path().join("src/main.rs"), source).unwrap();
        let output = Command::new("cargo").args(["build", "--offline", "--quiet", "--manifest-path"])
            .arg(directory.path().join("Cargo.toml")).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let binary = directory.path().join("target/debug").join(format!("tezbar-supervisor-fixture{}", std::env::consts::EXE_SUFFIX));
        (directory, binary)
    }).1.clone()
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "fixture condition timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn pid(dir: &Path, name: &str) -> u32 {
    let path = dir.join(name);
    wait_until(|| {
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .is_some()
    });
    fs::read_to_string(path).unwrap().parse().unwrap()
}
#[cfg(unix)]
pub(crate) fn alive(pid: u32) -> bool {
    let output = Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    let state = String::from_utf8_lossy(&output.stdout);
    !state.trim().is_empty() && !state.trim().starts_with('Z')
}
#[cfg(windows)]
pub(crate) fn alive(pid: u32) -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
        fn GetExitCodeProcess(process: *mut std::ffi::c_void, code: *mut u32) -> i32;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    }
    unsafe {
        let handle = OpenProcess(0x1000, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut code = 0;
        let ok = GetExitCodeProcess(handle, &mut code);
        CloseHandle(handle);
        ok != 0 && code == 259
    }
}
fn supervised(mode: &str, dir: &Path) -> Child {
    let binary = fixture_executable();
    Command::new(&binary)
        .arg(MODE)
        .arg(&binary)
        .arg(mode)
        .arg(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}
fn wait_exit(child: &mut Child) -> std::process::ExitStatus {
    wait_until(|| child.try_wait().unwrap().is_some());
    child.wait().unwrap()
}

#[test]
fn pipe_disconnect_kills_owned_tree_and_preserves_unrelated_process() {
    let directory = tempfile::tempdir().unwrap();
    let unrelated_dir = tempfile::tempdir().unwrap();
    let mut unrelated = Command::new(fixture_executable())
        .arg("leaf")
        .arg(unrelated_dir.path())
        .spawn()
        .unwrap();
    let mut child = supervised("tree", directory.path());
    let root = pid(directory.path(), "root");
    let leaf = pid(directory.path(), "leaf");
    assert!(alive(root) && alive(leaf));
    drop(child.stdin.take());
    assert!(wait_exit(&mut child).success());
    wait_until(|| !alive(root) && !alive(leaf));
    assert!(unrelated.try_wait().unwrap().is_none());
    unrelated.kill().unwrap();
    unrelated.wait().unwrap();
}

#[test]
fn abrupt_host_death_closes_lease_and_kills_tree() {
    let directory = tempfile::tempdir().unwrap();
    let mut parent = Command::new(fixture_executable())
        .arg("parent")
        .arg(directory.path())
        .spawn()
        .unwrap();
    let supervisor = pid(directory.path(), "supervisor");
    let root = pid(directory.path(), "root");
    let leaf = pid(directory.path(), "leaf");
    parent.kill().unwrap();
    parent.wait().unwrap();
    wait_until(|| !alive(root) && !alive(leaf) && !alive(supervisor));
}

#[test]
fn server_failure_preserves_status_and_cleans_descendants() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = supervised("fail", directory.path());
    let root = pid(directory.path(), "root");
    let leaf = pid(directory.path(), "leaf");
    assert_eq!(wait_exit(&mut child).code(), Some(23));
    wait_until(|| !alive(root) && !alive(leaf));
}

#[test]
fn missing_child_fails_without_waiting_for_open_lease() {
    let mut child = Command::new(fixture_executable())
        .arg(MODE)
        .arg("/nonexistent/tezbar-fixture")
        .stdin(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(!wait_exit(&mut child).success());
}
