// Tezbar is a GUI application. This must also apply to debug builds: Windows
// otherwise attaches every `tauri dev` launch to the user's default terminal,
// which looks like a terminal window that reopens whenever the dev watcher
// restarts the app.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    app_lib::run();
}
