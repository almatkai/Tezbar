use super::*;
use std::cell::Cell;

#[test]
fn windows_shortcut_launch_keeps_untrusted_paths_out_of_script() {
    let app = r"C:\Start Menu\O'Brien $(bad); app.lnk";
    let path = r"C:\Users\A B\$test & 'report'.txt";
    let command = windows_open_with_command(app, path);
    let command = command.as_std();
    let args: Vec<_> = command.get_args().map(|s| s.to_str().unwrap()).collect();
    assert_eq!(command.get_program(), "powershell.exe");
    assert_eq!(
        args[0..4],
        ["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"]
    );
    assert!(args[4].contains("Start-Process -FilePath $env:TEZBAR_APP_PATH"));
    assert!(!args[4].contains(app));
    assert!(!args[4].contains(path));
    let env: std::collections::HashMap<_, _> = command.get_envs().collect();
    assert_eq!(env[std::ffi::OsStr::new("TEZBAR_APP_PATH")].unwrap(), app);
    assert_eq!(
        env[std::ffi::OsStr::new("TEZBAR_TARGET_PATH")].unwrap(),
        path
    );
}

#[test]
fn application_name_fallback_and_settings_routes() {
    assert_eq!(
        application_target(&json!({"appName":"TextEdit"})).unwrap(),
        "TextEdit"
    );
    assert_eq!(
        application_target(&json!({"appPath":"", "appName":"TextEdit"})).unwrap(),
        "TextEdit"
    );
    assert_eq!(
        application_target(&json!({"appPath":"/Applications/Test.app", "appName":"Test"})).unwrap(),
        "/Applications/Test.app"
    );
    assert!(application_target(&json!({"appName":"\u{0}"})).is_err());
    assert_eq!(settings_surface("extensions"), Some("settings-extensions"));
    assert_eq!(settings_surface("general"), Some("settings"));
    assert_eq!(settings_surface("ai"), Some("settings-ai"));
    assert_eq!(settings_surface("knowledge"), Some("settings-knowledge"));
    assert_eq!(settings_surface("invalid"), None);
}

#[test]
fn persistence_failure_rolls_back_and_reports_rollback_failure() {
    let rolled_back = Cell::new(false);
    assert!(persist_or_rollback(|| Ok(()), || { panic!("unexpected rollback") }).is_ok());
    let error = persist_or_rollback(
        || Err("disk full".into()),
        || {
            rolled_back.set(true);
            Ok(())
        },
    )
    .unwrap_err();
    assert!(rolled_back.get());
    assert!(error.contains("previous shortcut restored"));
    let error = persist_or_rollback(|| Err("disk full".into()), || Err("binding taken".into()))
        .unwrap_err();
    assert!(error.contains("disk full") && error.contains("rollback failed: binding taken"));
}

#[tokio::test]
async fn paste_existing_clipboard_never_requests_a_clipboard_write() {
    use super::super::common::CorePaths;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().to_owned();
    let ctx = CoreContext::headless(CorePaths {
        data_dir: path.clone(),
        config_dir: path.clone(),
        resource_dir: path.clone(),
        temp_dir: path.clone(),
        home_dir: path,
    });
    let writes = Arc::new(AtomicUsize::new(0));
    let seen = writes.clone();
    ctx.set_router(Arc::new(move |channel, args| {
        assert_eq!(channel, "clipboard:write");
        assert_eq!(args, vec![json!("new text")]);
        seen.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(Value::Null) })
    }))
    .unwrap();
    let platform = PlatformCore::new(ctx);
    // A headless context fails at desktop activation, before any OS mutation.
    assert!(platform
        .handle("core:platform:paste", &[])
        .await
        .unwrap()
        .is_err());
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    assert!(platform
        .handle("core:platform:paste-text", &[json!("new text")])
        .await
        .unwrap()
        .is_err());
    assert_eq!(writes.load(Ordering::SeqCst), 1);
}
