//! Compile-time opt-in local desktop harness. No listener, command, environment
//! override or sampling runs in ordinary builds. See docs/performance/desktop.py.
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use tauri::{Emitter, Manager};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::oneshot,
};

#[derive(Default)]
struct Replies {
    next: AtomicU64,
    loaded: AtomicBool,
    pending: Mutex<HashMap<u64, oneshot::Sender<Value>>>,
}

pub fn validate_isolation() {
    for key in [
        "TEZBAR_DATA_DIR",
        "TEZBAR_CONFIG_DIR",
        "TEZBAR_BENCH_HOME",
        "TEZBAR_BENCH_SOCKET",
    ] {
        let path =
            PathBuf::from(std::env::var_os(key).expect("desktop-bench requires isolated paths"));
        assert!(
            path.is_absolute() && path.starts_with(std::env::temp_dir()),
            "benchmark paths must be inside the OS temporary directory"
        );
    }
}

pub fn reply(app: &tauri::AppHandle, id: u64, value: Value) {
    let state = app.state::<Arc<Replies>>();
    if let Some(tx) = state.pending.lock().unwrap().remove(&id) {
        let _ = tx.send(value);
    };
}

pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("desktop-bench")
        .on_page_load(|webview, payload| {
            if webview.label() == "main" {
                webview.state::<Arc<Replies>>().loaded.store(
                    payload.event() == tauri::webview::PageLoadEvent::Finished,
                    Ordering::Relaxed,
                );
            }
        })
        .setup(|app, _| {
            let replies = Arc::new(Replies::default());
            app.manage(replies);
            let app = app.clone();
            let socket = PathBuf::from(std::env::var_os("TEZBAR_BENCH_SOCKET").unwrap());
            tauri::async_runtime::spawn(async move {
                use std::os::unix::fs::PermissionsExt;
                let listener = tokio::net::UnixListener::bind(&socket).unwrap();
                std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
                while let Ok((mut stream, _)) = listener.accept().await {
                    let mut line = String::new();
                    let result = async {
                        BufReader::new(&mut stream)
                            .read_line(&mut line)
                            .await
                            .map_err(|e| e.to_string())?;
                        let request: Value =
                            serde_json::from_str(&line).map_err(|e| e.to_string())?;
                        dispatch(&app, request).await
                    }
                    .await;
                    let value = match result {
                        Ok(value) => json!({"ok":true,"value":value}),
                        Err(error) => json!({"ok":false,"error":error}),
                    };
                    let _ = stream.write_all(format!("{value}\n").as_bytes()).await;
                }
            });
            Ok(())
        })
        .build()
}

async fn dispatch(app: &tauri::AppHandle, request: Value) -> Result<Value, String> {
    let window = app
        .get_webview_window("main")
        .ok_or("main window missing")?;
    match request["op"].as_str().unwrap_or("") {
        "status" => {
            let mut status = serde_json::to_value(super::get_backend_status(app.state())).unwrap();
            status["rendererLoaded"] =
                json!(app.state::<Arc<Replies>>().loaded.load(Ordering::Relaxed));
            Ok(status)
        }
        "request" => {
            let core = app.state::<Arc<crate::native_core::NativeCore>>();
            core.ctx
                .request(
                    request["channel"].as_str().ok_or("missing channel")?,
                    request["args"].as_array().cloned().unwrap_or_default(),
                )
                .await
        }
        "show" => {
            super::restore_main_window(app)?;
            Ok(Value::Null)
        }
        "hide" => {
            super::hide_window(window)?;
            Ok(Value::Null)
        }
        "surface" => {
            app.emit("app:open-surface", request["surface"].clone())
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "quit" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                super::quit_app_now(&app);
            });
            Ok(Value::Null)
        }
        "js" => {
            let state = app.state::<Arc<Replies>>();
            let id = state.next.fetch_add(1, Ordering::Relaxed);
            let (tx, rx) = oneshot::channel();
            state.pending.lock().unwrap().insert(id, tx);
            let script = request["script"].as_str().ok_or("missing script")?;
            window.eval(format!("(async()=>{{try{{return {{value:await (async()=>{{{script}}})()}}}}catch(e){{return {{error:String(e)}}}}}})().then(value=>window.__TAURI_INTERNALS__.invoke('call_backend',{{channel:'__desktop_bench_reply',payload:[{id},value]}}))")).map_err(|e| e.to_string())?;
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(request["timeoutSeconds"].as_u64().unwrap_or(20)),
                rx,
            )
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string());
            state.pending.lock().unwrap().remove(&id);
            result
        }
        _ => Err("unknown desktop benchmark operation".into()),
    }
}
