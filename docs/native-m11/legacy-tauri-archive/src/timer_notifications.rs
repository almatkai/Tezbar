pub use tezbar_host::timer_notifications::deliver;
pub fn setup(app: &tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    let app = app.clone();
    tezbar_host::timer_notifications::setup(std::sync::Arc::new(move || {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
            let _ = window.emit("window-shown", serde_json::json!({"resetUi":false}));
        }
    }));
}
