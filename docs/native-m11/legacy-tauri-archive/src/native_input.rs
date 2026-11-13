use tauri::WebviewWindow;
use tezbar_host::input;
#[tauri::command]
pub fn move_mouse(x: f64, y: f64) -> Result<(), String> {
    input::move_mouse(x, y)
}
#[tauri::command]
pub fn click(x: f64, y: f64, button: String) -> Result<(), String> {
    input::click(x, y, button)
}
#[tauri::command]
pub fn double_click(x: f64, y: f64) -> Result<(), String> {
    input::double_click(x, y)
}
#[tauri::command]
pub fn press_key(key: String, mods: Vec<String>) -> Result<(), String> {
    input::press_key(key, mods)
}
#[tauri::command]
pub fn type_text(text: String) -> Result<(), String> {
    input::type_text(text)
}
#[tauri::command]
pub fn scroll(x: f64, y: f64, dx: i32, dy: i32) -> Result<(), String> {
    input::scroll(x, y, dx, dy)
}
#[tauri::command]
pub fn is_physical_key_down(key: String) -> bool {
    input::is_physical_key_down(key)
}
#[tauri::command]
pub async fn screenshot(window: WebviewWindow) -> Result<Vec<u8>, String> {
    capture(window, false).await
}
pub async fn active_capture(window: WebviewWindow) -> Result<Vec<u8>, String> {
    capture(window, true).await
}
async fn capture(window: WebviewWindow, active: bool) -> Result<Vec<u8>, String> {
    use tauri::Manager;
    input::capture_protected(active, move || {
        let mut guards = Vec::new();
        for (_, window) in window.app_handle().webview_windows() {
            window
                .set_content_protected(true)
                .map_err(|e| format!("failed to protect Tezbar window: {e}"))?;
            guards.push(input::RestoreOnDrop::new(Box::new(move || {
                window
                    .set_content_protected(false)
                    .map_err(|e| e.to_string())
            })
                as Box<dyn FnMut() -> Result<(), String> + Send>));
        }
        Ok(guards)
    })
    .await
}
