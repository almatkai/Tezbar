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

fn scripted() -> (tempfile::TempDir, CoreContext, Arc<crate::host::TestHost>) {
    use super::super::common::CorePaths;
    let d = tempfile::tempdir().unwrap();
    let h = Arc::new(crate::host::TestHost {
        allow_interaction: true,
        desktop: true,
        ..Default::default()
    });
    let ctx = CoreContext::new(
        CorePaths {
            data_dir: d.path().into(),
            config_dir: d.path().join("config"),
            resource_dir: d.path().into(),
            temp_dir: d.path().into(),
            home_dir: d.path().into(),
        },
        Some(h.clone()),
    );
    (d, ctx, h)
}
#[tokio::test]
async fn host_surface_routes_keep_event_payload_and_order() {
    let (_d, ctx, host) = scripted();
    let platform = PlatformCore::new(ctx);
    for channel in [
        "window:show",
        "window:hide",
        "window:close-current",
        "window:snap-drag-start",
        "window:snap-drag-end",
    ] {
        assert_eq!(
            platform.handle(channel, &[]).await.unwrap().unwrap(),
            Value::Null
        );
    }
    assert_eq!(
        *host.calls.lock().unwrap(),
        [
            "surface:ShowLauncher",
            "surface:HideLauncher",
            "surface:CloseCurrent",
            "surface:SnapDragStart",
            "surface:SnapDragEnd"
        ]
    );
    platform
        .handle("core:platform:open-surface", &[json!("vault")])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        *host.events.lock().unwrap(),
        vec![("app:open-surface".into(), json!("vault"))]
    );
    let before = host.calls.lock().unwrap().len();
    assert!(platform
        .handle("core:platform:open-surface", &[json!("invalid")])
        .await
        .unwrap()
        .is_err());
    assert_eq!(host.calls.lock().unwrap().len(), before);
}
#[tokio::test]
async fn host_paste_hides_before_key_and_does_not_write_existing_clipboard() {
    let (_d, ctx, host) = scripted();
    let platform = PlatformCore::new(ctx);
    assert_eq!(
        platform
            .handle("core:platform:paste", &[])
            .await
            .unwrap()
            .unwrap(),
        json!({"ok":true,"message":"Pasted text"})
    );
    let calls = host.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0], "surface:HideForPaste");
    assert!(calls[1].starts_with("surface:PressKey"));
}
#[tokio::test]
async fn host_confirm_maps_both_replies_and_propagates_closed_dialog() {
    use crate::host::DialogReply;
    let (_d, ctx, host) = scripted();
    let platform = PlatformCore::new(ctx);
    for (accepted, response) in [(false, 0), (true, 1)] {
        host.dialogs
            .lock()
            .unwrap()
            .push_back(Ok(DialogReply::Confirm(accepted)));
        assert_eq!(
            platform
                .handle(
                    "core:platform:confirm",
                    &[json!({"title":"Fixture","buttons":["No","Yes"]})]
                )
                .await
                .unwrap()
                .unwrap(),
            json!({"response":response})
        );
    }
    host.dialogs.lock().unwrap().push_back(Err("closed".into()));
    assert_eq!(
        platform
            .handle("core:platform:confirm", &[])
            .await
            .unwrap()
            .unwrap_err(),
        "closed"
    );
    host.dialogs
        .lock()
        .unwrap()
        .push_back(Ok(DialogReply::Path(None)));
    assert!(platform
        .handle("core:platform:confirm", &[])
        .await
        .unwrap()
        .is_err());
}
#[tokio::test]
async fn host_quit_occurs_only_after_accepted_confirmation() {
    use crate::host::DialogReply;
    let (_d, ctx, host) = scripted();
    let platform = PlatformCore::new(ctx);
    host.dialogs
        .lock()
        .unwrap()
        .push_back(Ok(DialogReply::Confirm(false)));
    platform
        .handle("app:confirm-quit", &[])
        .await
        .unwrap()
        .unwrap();
    assert!(!host
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|c| c == "surface:Quit"));
    host.dialogs
        .lock()
        .unwrap()
        .push_back(Ok(DialogReply::Confirm(true)));
    platform
        .handle("app:confirm-quit", &[])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(host.calls.lock().unwrap().last().unwrap(), "surface:Quit");
}
#[tokio::test]
async fn host_settings_op_precedes_validated_tab_event() {
    let (_d, ctx, host) = scripted();
    let platform = PlatformCore::new(ctx);
    platform
        .handle("core:platform:open-settings", &[json!("ai")])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(*host.calls.lock().unwrap(), ["surface:OpenSettings"]);
    assert_eq!(
        *host.events.lock().unwrap(),
        vec![("app:open-surface".into(), json!("settings-ai"))]
    );
    platform
        .handle("core:platform:open-settings", &[json!("invalid")])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(host.events.lock().unwrap().len(), 1);
}
#[test]
fn host_shortcut_replacement_registers_before_releasing_and_keeps_old_on_conflict() {
    let (_d, ctx, host) = scripted();
    let platform = PlatformCore::new(ctx);
    platform.register_shortcut(None, "Alt+A").unwrap();
    host.shortcut_results
        .lock()
        .unwrap()
        .push_back(Err("conflict".into()));
    assert!(platform.register_shortcut(None, "Alt+B").is_err());
    assert_eq!(
        platform
            .shortcuts
            .lock()
            .unwrap()
            .launcher
            .as_ref()
            .unwrap()
            .accelerator,
        "Alt+A"
    );
    platform.register_shortcut(None, "Alt+B").unwrap();
    let calls = host.calls.lock().unwrap();
    assert_eq!(
        &calls[calls.len() - 2..],
        ["register:Alt+B", "unregister:Alt+A"]
    );
}
#[test]
fn host_shortcut_unregister_failure_removes_new_binding_without_changing_owner() {
    let (_d, ctx, host) = scripted();
    let platform = PlatformCore::new(ctx);
    platform.register_shortcut(None, "Alt+A").unwrap();
    host.shortcut_results.lock().unwrap().extend([
        Ok(()),
        Err("old release failed".into()),
        Ok(()),
    ]);
    assert!(platform
        .register_shortcut(None, "Alt+B")
        .unwrap_err()
        .contains("old release failed"));
    assert_eq!(
        platform
            .shortcuts
            .lock()
            .unwrap()
            .launcher
            .as_ref()
            .unwrap()
            .accelerator,
        "Alt+A"
    );
    assert_eq!(
        host.calls.lock().unwrap().last().unwrap(),
        "unregister:Alt+B"
    );
}
#[tokio::test]
async fn host_shortcut_persistence_failure_restores_previous_binding() {
    let (_d, ctx, host) = scripted();
    let platform = PlatformCore::new(ctx.clone());
    platform.register_shortcut(None, "Alt+A").unwrap();
    std::fs::create_dir_all(ctx.config.path()).unwrap();
    let error = platform
        .handle("core:platform:update-launcher-hotkey", &[json!("Alt+B")])
        .await
        .unwrap()
        .unwrap_err();
    assert!(error.contains("previous shortcut restored"));
    assert_eq!(
        platform
            .shortcuts
            .lock()
            .unwrap()
            .launcher
            .as_ref()
            .unwrap()
            .accelerator,
        "Alt+A"
    );
    assert_eq!(
        host.calls.lock().unwrap().last().unwrap(),
        "unregister:Alt+B"
    );
}
#[tokio::test]
async fn host_capture_keeps_png_reply_contract_and_resizes() {
    use std::io::Cursor;
    let (_d, ctx, host) = scripted();
    let platform = PlatformCore::new(ctx);
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(640, 480)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    host.captures
        .lock()
        .unwrap()
        .push_back(Ok(bytes.into_inner()));
    let result = platform
        .handle("core:platform:screenshot", &[json!({"maxWidth":320})])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result["mimeType"], "image/png");
    assert_eq!(result["width"], 320);
    assert_eq!(result["height"], 240);
    assert!(base64::engine::general_purpose::STANDARD
        .decode(result["data"].as_str().unwrap())
        .is_ok());
}

#[test]
fn host_shortcut_duplicate_assignment_and_empty_launcher_do_not_release_owner() {
    let (_d, ctx, host) = scripted();
    let p = PlatformCore::new(ctx);
    p.register_shortcut(None, "Alt+A").unwrap();
    assert!(p
        .register_shortcut(Some("command:a"), "Alt+A")
        .unwrap_err()
        .contains("already assigned"));
    assert!(p
        .register_shortcut(None, "")
        .unwrap_err()
        .contains("cannot be empty"));
    assert_eq!(*host.calls.lock().unwrap(), ["register:Alt+A"]);
    p.register_shortcut(None, "Alt+A").unwrap();
    assert_eq!(host.calls.lock().unwrap().len(), 1);
    p.register_shortcut(Some("command:a"), "Alt+B").unwrap();
    p.register_shortcut(Some("command:a"), "").unwrap();
    assert!(p.shortcuts.lock().unwrap().commands.is_empty());
    assert_eq!(
        host.calls.lock().unwrap().last().unwrap(),
        "unregister:Alt+B"
    );
}
#[tokio::test]
async fn host_headless_shortcuts_and_capture_never_touch_the_system() {
    let (_d, ctx, _) = scripted();
    let h = Arc::new(crate::host::TestHost::default());
    let ctx = CoreContext::new(ctx.paths, Some(h.clone()));
    let p = PlatformCore::new(ctx);
    p.initialize_shortcuts().unwrap();
    assert!(h.calls.lock().unwrap().is_empty());
    assert!(p
        .handle("core:platform:screenshot", &[])
        .await
        .unwrap()
        .is_err());
}
#[tokio::test]
async fn host_visibility_suppression_and_cache_remain_distinct_operations() {
    let (_d, ctx, h) = scripted();
    let p = PlatformCore::new(ctx);
    for (c, v) in [
        ("core:platform:visibility", json!(false)),
        ("window:suppress-blur-hide", json!(true)),
        ("core:platform:clear-webview-cache", Value::Null),
    ] {
        p.handle(c, &[v]).await.unwrap().unwrap();
    }
    assert_eq!(
        *h.calls.lock().unwrap(),
        [
            "surface:SetAppVisible(false)",
            "surface:SuppressBlurHide(true)",
            "surface:ClearLegacyWebviewCache"
        ]
    );
}
