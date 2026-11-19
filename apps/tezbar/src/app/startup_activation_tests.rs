//! Cold startup and OS-callback composition with disposable paths and a synthetic registrar.
//! No native shortcut manager, tray, clipboard observer or personal profile is opened.
use super::*;
use ::core::prelude::v1::test;
use tezbar_core::host::*;

#[derive(Default)]
struct Registrar {
    inner: TestHost,
    registered: std::sync::Mutex<Vec<Shortcut>>,
    conflict: std::sync::atomic::AtomicBool,
}
impl ShortcutRegistry for Registrar {
    fn available(&self) -> bool {
        true
    }
    fn parse(&self, raw: &str) -> Result<Shortcut, String> {
        // Match the IDs emitted by the actual native library, without OS registration.
        tezbar_host::desktop::parse_shortcut(raw)
    }
    fn register(&self, key: &Shortcut) -> Result<(), String> {
        if self.conflict.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("synthetic OS conflict".into());
        }
        self.registered.lock().unwrap().push(key.clone());
        Ok(())
    }
    fn unregister(&self, key: &Shortcut) -> Result<(), String> {
        self.registered.lock().unwrap().retain(|k| k != key);
        Ok(())
    }
}
impl Host for Registrar {
    fn emit(&self, channel: &str, value: serde_json::Value) {
        self.inner.emit(channel, value)
    }
    fn surface(&self, _: SurfaceOp) -> HostFuture<()> {
        Box::pin(async { Err("fixture forbids desktop effects".into()) })
    }
    fn dialog(&self, _: DialogRequest) -> HostFuture<DialogReply> {
        Box::pin(async { Err("fixture forbids dialogs".into()) })
    }
    fn shortcuts(&self) -> &dyn ShortcutRegistry {
        self
    }
    fn status_items(&self) -> &dyn StatusItems {
        &self.inner
    }
    fn capture_active_display(&self) -> HostFuture<Vec<u8>> {
        Box::pin(async { Err("fixture forbids capture".into()) })
    }
    fn desktop_services(&self) -> bool {
        false
    }
}
fn cold(
    root: &std::path::Path,
    configured: Option<&str>,
    production: bool,
) -> (CoreClient, Arc<Registrar>) {
    let host = Arc::new(Registrar::default());
    let ctx = CoreContext::new(
        tezbar_core::common::CorePaths {
            data_dir: root.join("data"),
            config_dir: root.join("config"),
            resource_dir: root.join("resources"),
            temp_dir: root.join("tmp"),
            home_dir: root.join("home"),
        },
        Some(host.clone()),
    );
    if let Some(key) = configured {
        ctx.config
            .patch(&serde_json::json!({"raymesHotkey":key}))
            .unwrap();
    }
    (
        CoreClient::for_launch(tezbar_core::NativeCore::new(ctx).unwrap(), production),
        host,
    )
}
fn default_key() -> &'static str {
    if cfg!(target_os = "windows") {
        "Control+Space"
    } else {
        "Alt+Space"
    }
}
fn assert_binding(client: &CoreClient, host: &Registrar, expected: &str) -> u32 {
    let key = host.parse(expected).unwrap();
    assert!(
        host.registered.lock().unwrap().contains(&key),
        "cold production startup did not register {expected}"
    );
    assert!(
        client.0.platform.is_launcher_shortcut(key.id()),
        "registrar and core shortcut ownership disagree"
    );
    key.id()
}
fn click(
    id: &str,
    button: tray_icon::MouseButton,
    state: tray_icon::MouseButtonState,
) -> tray_icon::TrayIconEvent {
    tray_icon::TrayIconEvent::Click {
        id: id.into(),
        position: Default::default(),
        rect: Default::default(),
        button,
        button_state: state,
    }
}
fn next(rt: &tokio::runtime::Runtime, rx: &mut events::Receiver) -> Event {
    rt.block_on(async {
        tokio::time::timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("native callback did not deliver activation to Relay")
            .unwrap()
    })
}

#[tokio::test]
async fn startup_activation_cold_default_and_configured_register_without_any_surface() {
    for configured in [None, Some("Control+Shift+K")] {
        let root = tempfile::tempdir().unwrap();
        let (client, host) = cold(root.path(), configured, true);
        client.start().await.unwrap();
        assert_binding(&client, &host, configured.unwrap_or(default_key()));
        assert!(host.inner.calls.lock().unwrap().is_empty());
        client.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn startup_activation_preview_never_registers_production_shortcuts() {
    let root = tempfile::tempdir().unwrap();
    let (client, host) = cold(root.path(), Some("Alt+Space"), false);
    client.start().await.unwrap();
    assert!(host.registered.lock().unwrap().is_empty());
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn startup_activation_failed_optional_core_keeps_launcher_binding() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("data")).unwrap();
    std::fs::write(
        root.path().join("data/search"),
        "synthetic database directory failure",
    )
    .unwrap();
    let (client, host) = cold(root.path(), None, true);
    assert!(client.start().await.is_err());
    assert_binding(&client, &host, default_key());
    let _ = client.shutdown().await;
}

#[gpui::test]
fn startup_activation_primary_callback_reaches_owner_show_without_toggling(
    cx: &mut TestAppContext,
) {
    let (rt, root, backend, _view, cx) =
        crate::surfaces::command_bar::chat_integration::tests::mount(cx);
    let owner = super::m8::tests::owner(cx, &rt, &root, &backend);
    let (relay, mut rx) = Relay::new();
    let callback = events::tray_handler(&relay);
    // The owner is still booting: opening must be remembered, not discarded.
    owner.update(&mut cx.cx, |o, _| {
        o.m7.boot = m7::Boot::Waiting;
        o.lifecycle.visible = false;
    });
    callback(click(
        crate::host_adapter::PRIMARY_TRAY_ID,
        tray_icon::MouseButton::Left,
        tray_icon::MouseButtonState::Up,
    ));
    let event = next(&rt, &mut rx);
    owner.update(&mut cx.cx, |o, cx| o.event(event, cx));
    assert!(owner.read_with(&cx.cx, |o, _| o.show_on_ready));
    // A second click while already visible must focus/show, not hide the launcher.
    owner.update(&mut cx.cx, |o, _| {
        o.m7.boot = m7::Boot::Ready;
        o.lifecycle.visible = true;
        o.show_on_ready = false;
    });
    callback(click(
        crate::host_adapter::PRIMARY_TRAY_ID,
        tray_icon::MouseButton::Left,
        tray_icon::MouseButtonState::Up,
    ));
    let event = next(&rt, &mut rx);
    let generation = owner.read_with(&cx.cx, |o, _| o.visibility_generation);
    owner.update(&mut cx.cx, |o, cx| o.event(event, cx));
    assert!(owner.read_with(&cx.cx, |o, _| o.lifecycle.visible));
    assert!(owner.read_with(&cx.cx, |o, _| o.visibility_generation > generation));
}

#[gpui::test]
fn startup_activation_owner_registers_before_migration_gate(cx: &mut TestAppContext) {
    let (rt, root, backend, view, cx) =
        crate::surfaces::command_bar::chat_integration::tests::mount(cx);
    let owner = super::m8::tests::owner(cx, &rt, &root, &backend);
    let cold_root = tempfile::tempdir().unwrap();
    let (client, host) = cold(cold_root.path(), Some("Control+Shift+K"), true);
    let relay = backend.relay.clone();
    cx.update(|w, cx| {
        view.update(cx, |v, cx| {
            *v = Launcher::new_with_migration(
                client.clone(),
                relay,
                crate::surfaces::settings::SettingsStore::new(Arc::new(client.clone())),
                Some(crate::legacy_state::Source::WebKit(
                    cold_root.path().join("missing-fixture-origin"),
                )),
                w,
                cx,
            );
            assert!(v.migration_pending());
        })
    });
    owner.update(&mut cx.cx, |o, cx| {
        o.core = client.clone();
        o.start_core(cx);
        assert!(
            o.startup.is_none(),
            "migration must still gate optional core services"
        );
        assert!(o.settings.is_none());
    });
    assert_binding(&client, &host, "Control+Shift+K");
    rt.block_on(client.shutdown()).unwrap();
}

#[gpui::test]
fn startup_activation_owner_registers_before_optional_start_future_is_polled(
    cx: &mut TestAppContext,
) {
    let (rt, root, backend, _view, cx) =
        crate::surfaces::command_bar::chat_integration::tests::mount(cx);
    let owner = super::m8::tests::owner(cx, &rt, &root, &backend);
    let cold_root = tempfile::tempdir().unwrap();
    let (client, host) = cold(cold_root.path(), None, true);
    owner.update(&mut cx.cx, |o, cx| {
        o.core = client.clone();
        o.start_core(cx);
        assert!(o.startup.is_some());
        // Deliberately don't pump GPUI's executor: optional startup cannot make progress.
        assert_binding(&client, &host, default_key());
        o.startup = None;
    });
    rt.block_on(client.shutdown()).unwrap();
}

#[tokio::test]
async fn startup_activation_configured_conflict_never_falls_back_or_replaces_working_binding() {
    let root = tempfile::tempdir().unwrap();
    let (client, host) = cold(root.path(), Some("Control+Shift+K"), true);
    client.start().await.unwrap();
    let id = assert_binding(&client, &host, "Control+Shift+K");
    host.conflict
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(
        client
            .0
            .platform
            .register_shortcut(None, "Alt+Space")
            .is_err()
    );
    assert!(client.0.platform.is_launcher_shortcut(id));
    assert_eq!(host.registered.lock().unwrap().len(), 1);
    assert_eq!(
        client.0.ctx.config.read().unwrap()["raymesHotkey"],
        "Control+Shift+K"
    );
    client.shutdown().await.unwrap();

    let root = tempfile::tempdir().unwrap();
    let (client, host) = cold(root.path(), Some("Control+Shift+K"), true);
    host.conflict
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(
        client
            .start()
            .await
            .unwrap_err()
            .contains("synthetic OS conflict")
    );
    assert!(host.registered.lock().unwrap().is_empty());
    assert_eq!(
        client.0.ctx.config.read().unwrap()["raymesHotkey"],
        "Control+Shift+K"
    );
    let _ = client.shutdown().await;
}

#[test]
fn startup_activation_tray_scope_preserves_preview_and_context_menu() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (relay, mut rx) = Relay::new();
    let callback = events::tray_handler(&relay);
    for (id, button, state) in [
        (
            crate::host_adapter::PRIMARY_TRAY_ID,
            tray_icon::MouseButton::Right,
            tray_icon::MouseButtonState::Up,
        ),
        (
            crate::host_adapter::PRIMARY_TRAY_ID,
            tray_icon::MouseButton::Left,
            tray_icon::MouseButtonState::Down,
        ),
        (
            "extension-tray",
            tray_icon::MouseButton::Left,
            tray_icon::MouseButtonState::Up,
        ),
        (
            "tezbar-primary-extension",
            tray_icon::MouseButton::Left,
            tray_icon::MouseButtonState::Up,
        ),
    ] {
        callback(click(id, button, state));
    }
    relay.send(Event::Bounds).unwrap();
    assert!(matches!(next(&rt, &mut rx), Event::Bounds));
    callback(click(
        crate::host_adapter::PREVIEW_TRAY_ID,
        tray_icon::MouseButton::Left,
        tray_icon::MouseButtonState::Up,
    ));
    assert!(matches!(next(&rt, &mut rx), Event::Toggle));
}

#[gpui::test]
fn startup_activation_hotkey_callback_reaches_cold_owner_before_launcher_or_settings(
    cx: &mut TestAppContext,
) {
    let (rt, root, backend, _view, cx) =
        crate::surfaces::command_bar::chat_integration::tests::mount(cx);
    let owner = super::m8::tests::owner(cx, &rt, &root, &backend);
    for configured in [None, Some("Control+Shift+K")] {
        let cold_root = tempfile::tempdir().unwrap();
        let (client, host) = cold(cold_root.path(), configured, true);
        rt.block_on(client.start()).unwrap();
        let id = assert_binding(&client, &host, configured.unwrap_or(default_key()));
        let (relay, mut rx) = Relay::new();
        owner.update(&mut cx.cx, |o, _| {
            o.core = client.clone();
            o.m7.boot = m7::Boot::Waiting;
            o.lifecycle.visible = false;
            o.show_on_ready = false;
            assert!(o.settings.is_none());
        });
        let callback = events::hotkey_handler(&relay);
        callback(global_hotkey::GlobalHotKeyEvent {
            id,
            state: global_hotkey::HotKeyState::Released,
        });
        relay.send(Event::Bounds).unwrap();
        assert!(matches!(next(&rt, &mut rx), Event::Bounds));
        // Unowned native IDs must not activate this core's launcher.
        callback(global_hotkey::GlobalHotKeyEvent {
            id: id ^ 1,
            state: global_hotkey::HotKeyState::Pressed,
        });
        let event = next(&rt, &mut rx);
        owner.update(&mut cx.cx, |o, cx| o.event(event, cx));
        assert!(!owner.read_with(&cx.cx, |o, _| o.show_on_ready));
        callback(global_hotkey::GlobalHotKeyEvent {
            id,
            state: global_hotkey::HotKeyState::Pressed,
        });
        let event = next(&rt, &mut rx);
        owner.update(&mut cx.cx, |o, cx| o.event(event, cx));
        assert!(owner.read_with(&cx.cx, |o, _| o.show_on_ready));
        rt.block_on(client.shutdown()).unwrap();
    }
}
