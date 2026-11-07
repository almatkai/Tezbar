//! Real routed handlers validate before the finite TestHost execution boundary.
use crate::{
    common::{CoreContext, CorePaths},
    host::{DesktopEffect, TestHost},
    NativeCore, StartupOptions,
};
use serde_json::{json, Value};
use std::sync::Arc;
#[tokio::test]
async fn desktop_boundary_rejects_invalid_inputs_and_preserves_full_payload_without_os() {
    let root = tempfile::tempdir().unwrap();
    let host = Arc::new(TestHost {
        allow_interaction: true,
        ..Default::default()
    });
    let ctx = CoreContext::new(
        CorePaths {
            data_dir: root.path().join("data"),
            config_dir: root.path().join("config"),
            temp_dir: root.path().join("temp"),
            resource_dir: root.path().join("resources"),
            home_dir: root.path().join("home"),
        },
        Some(host.clone()),
    );
    let core = NativeCore::new(ctx).unwrap();
    core.start_with_options(StartupOptions::ISOLATED_PREVIEW)
        .await
        .unwrap();
    for (channel, args) in [
        ("clipboard:write", vec![json!(42)]),
        ("shell:open", vec![json!("")]),
        ("open-external-url", vec![json!("http://example.invalid")]),
    ] {
        assert!(core.request(channel, &args).await.is_err());
    }
    for (channel, args) in [
        ("core:platform:clipboard-write", vec![json!({"html":42})]),
        ("core:platform:reveal-file", vec![json!("\0")]),
    ] {
        assert!(core.ctx.request(channel, args).await.is_err());
    }
    assert!(host.desktop_effects.lock().unwrap().is_empty());
    let text = format!("long copy:{}", "x".repeat(10000));
    core.request("clipboard:write", &[json!(text)])
        .await
        .unwrap();
    let copied = core.request("clipboard:read", &[]).await.unwrap();
    assert!(copied.as_str().is_some_and(|s| s == text));
    core.ctx
        .request(
            "core:platform:clipboard-write",
            vec![json!({"html":"<b>synthetic</b>","text":"synthetic"})],
        )
        .await
        .unwrap();
    core.request("shell:open", &[json!("https://example.invalid")])
        .await
        .unwrap();
    core.ctx
        .request("core:platform:reveal-file", vec![json!("/fixture/reveal")])
        .await
        .unwrap();
    core.ctx
        .request("core:platform:selected-text", vec![])
        .await
        .unwrap();
    let effects = host.desktop_effects.lock().unwrap();
    assert_eq!(effects.len(), 6);
    assert!(effects
        .iter()
        .any(|e| matches!(e,DesktopEffect::WriteText(s) if s.len()==10010)));
    assert!(effects
        .iter()
        .all(|e| !format!("{e:?}").contains("synthetic")));
    drop(effects);
    assert!(core
        .request("core:platform:selected-text", &[])
        .await
        .is_err());
    core.shutdown().await;
}
#[test]
fn headless_host_never_falls_through_even_when_interaction_disabled() {
    use crate::host::Host;
    let host = TestHost::default();
    for e in [
        DesktopEffect::ReadClipboard,
        DesktopEffect::WriteText("synthetic".into()),
        DesktopEffect::SelectedText,
        DesktopEffect::SelectedFiles,
        DesktopEffect::ReadImage,
        DesktopEffect::Open("https://example.invalid".into()),
        DesktopEffect::Reveal("/fixture".into()),
        DesktopEffect::WriteFiles(vec![]),
        DesktopEffect::WriteHtml {
            html: String::new(),
            text: None,
        },
        DesktopEffect::WriteImage {
            bytes: vec![],
            width: 0,
            height: 0,
        },
        DesktopEffect::OpenApp(String::new()),
        DesktopEffect::OpenWith {
            path: String::new(),
            app: String::new(),
        },
    ] {
        assert!(host.desktop_effect(e).unwrap().is_err());
    }
    let _ = Value::Null;
}
